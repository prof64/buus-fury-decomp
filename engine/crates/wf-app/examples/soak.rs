//! Story-mode soak of the real app path (headless, muted, no window, no audio device): the launcher's title app
//! (`launcher::title_app`: title -> Single Player -> New Game -> the prologue -> the field) or a continued slot
//! (`launcher::story_app`), fed a real-game recording's keys, compared row by row with that recording.
//!
//!   cargo run --release -p wf-app --example soak -- RECORDING.json [--frames N] [--slot FILE] [--save-out FILE]
//!
//! RECORDING = a playthrough / segment file (rows with `keys`, `lf`, `area`, `p`, `rng`; data/story_playthrough/
//! opening.json starts at the soft reset). A row is stepped when its logic counter advanced or while the app shows a
//! menu / the title (the app has no lag-frame model: the recording's `lf` says which video frames were logic
//! frames). Prints the first area / position / RNG divergence, script errors, songs, and the end state.

use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::Sim;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rec = args.first().expect("recording");
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let limit: usize = opt("--frames").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let slots: Vec<Vec<u8>> = opt("--slot").map(|p| vec![std::fs::read(p).expect("slot file")]).unwrap_or_default();
    // the launcher's builders (`launcher::title_app` / `story_app`), kept as a StorySim so the soak can read (and, for
    // `--sync`, seed) the world
    let data = wf_app::battle::load_data(&packs).expect("data");
    let mut saves = wf_core::menu::save::Saves::default();
    for (i, b) in slots.iter().enumerate() {
        saves.set_slot_bytes(i, b);
    }
    let mut cfg = wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::Seed(1), ..Default::default() };
    let cont = !slots.is_empty();
    if cont {
        let sd = saves.slot(0).expect("slot 1 unreadable").clone();
        cfg = wf_core::field::FieldConfig { world: sd.world, area: sd.area, variant: sd.variant, pos: Some((sd.x, sd.y)), story: Some(sd.story), ..cfg };
    }
    let mut session = wf_core::session::Session::new_field(data, cfg);
    session.world.saves = saves;
    if !cont {
        wf_core::menu::title::open_title(&mut session.world);
    }
    // --import SEG.json: start from that cut's RAM (one import), the recordings' rows follow the cut
    let continue_on_save = args.iter().any(|a| a == "--continue-on-save");
    let dump_at: Option<(usize, usize)> = opt("--dump-at").and_then(|v| {
        let (a, b) = v.split_once("..").unwrap_or((v.as_str(), v.as_str()));
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    let save_at: Option<usize> = opt("--save-at").and_then(|v| v.parse().ok());
    let mut continued = false;
    let mut continue_row: Option<usize> = None;
    let mut app = match opt("--import") {
        Some(seg) => {
            let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&seg).expect("read")).expect("json");
            println!("imported {seg} (area {}, snd_phase {})", doc["init"]["area"], doc["init"]["snd_phase"]);
            StorySim::from_segment(&packs, &doc).expect("import")
        }
        None => StorySim::with_session(&packs, session).expect("story sim"),
    };
    if !cont && opt("--import").is_none() {
        app.skip_area_banner();
    }
    // --sync W-A: the recording's keys are open loop, and the app's title / prologue run a few text-box phases off
    // the recording; at the first row the recording presses a key in area W-A, the soak checks the leader stands
    // where the game's does and takes the recording's RNG (the one thing only a harness sets up)
    let sync: Option<(i64, i64)> = opt("--sync").and_then(|v| wf_app::story::parse_area(&v)).map(|(w, a)| (w as i64, a as i64));
    let mut synced = sync.is_none();
    // --show A..B: print the app's leader beside the game's for those rows
    let show: Option<(usize, usize)> = opt("--show").and_then(|v| v.split_once("..").and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))));
    // --rng A..B: print every generator call of those rows (with its source location)
    let rng_rows: Option<(usize, usize)> = opt("--rng").and_then(|v| v.split_once("..").and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))));
    // several recordings chain: the app keeps running, the next recording's rows follow (a recording that starts
    // where the previous one ended: opening.json, then the Earth master work/earth/rec/earth_all.json ...)
    let recs: Vec<&String> = args.iter().take_while(|a| !a.starts_with("--")).collect();
    let mut all: Vec<serde_json::Value> = vec![];
    for r in &recs {
        let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(r).expect("read")).expect("json");
        println!("recording {r}: {} rows from row {}", doc["frames"].as_array().map(|f| f.len()).unwrap_or(0), all.len());
        all.extend(doc["frames"].as_array().expect("frames").iter().cloned());
    }
    let rows = &all;
    let _ = rec;
    let mut prev_lf = -1i64;
    let mut rng_bad = 0usize;
    let mut flag_check: Option<(usize, String)> = None;
    let mut flag_reports = 0usize;
    let (mut hp_bad, mut hp_reports) = (0usize, 0usize);
    let mut pos_bad = 0usize;
    let (mut stepped, mut first_area, mut first_pos, mut first_rng) = (0usize, None, None, None);
    let mut songs: Vec<(usize, i32)> = vec![];
    let mut areas: Vec<(usize, (i32, i32))> = vec![];
    let mut game_areas: Vec<(usize, (i64, i64))> = vec![];
    let mut errors = 0usize;
    for (k, r) in rows.iter().enumerate().take(limit) {
        let keys = r["keys"].as_u64().unwrap_or(0) as u16;
        let lf = r["lf"].as_i64().unwrap_or(0);
        let ga0 = (r["area"][0].as_i64().unwrap_or(-1), r["area"][1].as_i64().unwrap_or(-1));
        if !synced && Some(ga0) == sync && keys & 0xF0 != 0 && r["depth"].as_i64() == Some(0) && !r["p"].is_null() {
            synced = true;
            // the app is still in the prologue's last event (it entered the title a few press cycles later): let
            // it finish first
            let mut extra = 0;
            while extra < 3000 && (app.world().field.as_ref().is_some_and(|f| f.busy) || app.world().scene.depth > 0 || wf_core::menu::active(app.world())) {
                let mut ev = vec![];
                app.step(&[if extra % 30 < 4 { 1 } else { 0 }], &mut ev);
                extra += 1;
            }
            println!("sync: {extra} extra app frames (A mash) until the field is free");
            let w = &mut app.session.world;
            let pos = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y));
            println!("sync at row {k}: app area {:?} leader {:?} busy {:?} | game p {}", w.field.as_ref().map(|f| (f.area.world, f.area.area)), pos, w.field.as_ref().map(|f| f.busy), rows[k - 1]["p"]);
            if let Some(gr) = rows[k - 1]["rng"].as_array() {
                w.rng.set_state([gr[0].as_u64().unwrap_or(0) as u32, gr[1].as_u64().unwrap_or(0) as u32]);
            }
            first_area = None;
            first_pos = None;
            first_rng = None;
            rng_bad = 0;
            pos_bad = 0;
        }
        // --save-at ROW: the player stands in a save point's rect there (the route used it for Switch Character):
        // the app saves through the real screens (A, "-Save", the slot, confirm) on extra frames - no rows consumed
        if save_at == Some(k) {
            let gen0 = app.world().saves.generation;
            for (key, n) in [(0x40u16, 4), (0, 6), (1, 3), (0, 60), (1, 3), (0, 90), (1, 3), (0, 60), (1, 3), (0, 60), (1, 3), (0, 120)] {
                for _ in 0..n {
                    let mut ev = vec![];
                    app.step(&[key], &mut ev);
                }
                if app.world().saves.generation != gen0 && !wf_core::menu::active(app.world()) && app.world().scene.depth == 0 {
                    break;
                }
            }
            println!("save program at row {k}: generation {} -> {}", gen0, app.world().saves.generation);
        }
        // --continue-on-save: the app's own Save screen wrote a slot (the route saves at a save point) and the field
        // is free again: a fresh session on the title with that slot file, Continue through the title's menus, the
        // continued story compared with the saved one; then the key log goes on in the continued session
        if continue_on_save && !continued && app.world().saves.generation > 0 && !wf_core::menu::active(app.world())
            && app.world().scene.depth == 0 && app.world().field.as_ref().is_some_and(|f| !f.busy)
        {
            continued = true;
            let w = app.world();
            let slot = w.saves.current.max(0) as usize;
            let slots: Vec<Vec<u8>> = (0..3).map(|i| w.saves.slot_bytes(i)).collect();
            let saved = w.story.clone();
            let pos = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py()));
            println!("save written at row {k} (slot {}, {} bytes): area {:?} leader {:?}", slot + 1, slots[slot].len(), w.field.as_ref().map(|f| (f.area.world, f.area.area)), pos);
            let data = wf_app::battle::load_data(&packs).expect("data");
            let mut sv = wf_core::menu::save::Saves::default();
            for (i, b) in slots.iter().enumerate() {
                sv.set_slot_bytes(i, b);
            }
            let mut sess = wf_core::session::Session::new_field(data, wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::Seed(7), ..Default::default() });
            sess.world.saves = sv;
            wf_core::menu::title::open_title(&mut sess.world);
            let mut fresh = StorySim::with_session(&packs, sess).expect("title");
            fresh.skip_area_banner();
            // START, A (Single Player), DOWN to the saved slot, A (Continue)
            let mut ok = false;
            for i in 0..2000u32 {
                let phase = i / 40;
                let key = if i % 40 < 4 { match phase { 0 => 8u16, 1 => 1, p if p >= 2 && p < 2 + slot as u32 => 0x80, _ => 1 } } else { 0 };
                let mut ev = vec![];
                fresh.step(&[key], &mut ev);
                let fw = fresh.world();
                if i > 120 && !wf_core::menu::active(fw) && fw.field.is_some() && fw.player_idx(0).is_some() && fw.story.flags == saved.flags {
                    ok = true;
                    println!("continued after {i} title frames");
                    break;
                }
            }
            let fw = fresh.world();
            let cpos = fw.player_idx(0).map(|pi| (fw.pl_ref(pi).a.px(), fw.pl_ref(pi).a.py()));
            let mut diffs = vec![];
            if fw.story.flags != saved.flags { diffs.push("flags".to_string()); }
            if fw.story.supplies != saved.supplies { diffs.push("supplies".into()); }
            if fw.story.equipment != saved.equipment { diffs.push("equipment".into()); }
            if fw.story.zenie != saved.zenie { diffs.push("zenie".into()); }
            if fw.story.current != saved.current { diffs.push("current".into()); }
            if fw.story.g != saved.g || fw.story.l != saved.l { diffs.push("script words".into()); }
            for (c, (a, b)) in fw.story.chars.iter().zip(saved.chars.iter()).enumerate() {
                if serde_json::to_string(a).ok() != serde_json::to_string(b).ok() { diffs.push(format!("char {c}")); }
            }
            println!("Continue: ok {ok}, area {:?} leader {cpos:?}, story differences {diffs:?}", fw.field.as_ref().map(|f| (f.area.world, f.area.area)));
            app = fresh;
            first_area = None;
            first_pos = None;
            first_rng = None;
            rng_bad = 0;
            pos_bad = 0;
            continue_row = Some(k);
        }
        let menu = wf_core::menu::active(app.world());
        let logic = lf != prev_lf;
        prev_lf = lf;
        if logic || menu {
            let mut ev = vec![];
            let trace = rng_rows.is_some_and(|(a, b)| a <= k && k <= b);
            if trace {
                eprintln!("  rng calls of row {k} (app rng before {:08x?}, game after {})", app.world().rng.state(), r["rng"]);
            }
            wf_core::rng::TRACE.store(trace, std::sync::atomic::Ordering::Relaxed);
            app.step(&[keys], &mut ev);
            wf_core::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
            stepped += 1;
        }
        let s = &app;
        if s.errors.len() > errors {
            for e in &s.errors[errors..] {
                println!("f{k}: script error: {e}");
            }
            errors = s.errors.len();
        }
        let w = s.world();
        if songs.last().map(|x| x.1) != Some(w.scene.song) {
            songs.push((k, w.scene.song));
        }
        let a = w.field.as_ref().map(|f| (f.area.world, f.area.area)).unwrap_or((-1, -1));
        if areas.last().map(|x| x.1) != Some(a) {
            areas.push((k, a));
        }
        let ga = (r["area"][0].as_i64().unwrap_or(-1), r["area"][1].as_i64().unwrap_or(-1));
        if game_areas.last().map(|x| x.1) != Some(ga) {
            game_areas.push((k, ga));
        }
        if first_area.is_none() && (a.0 as i64, a.1 as i64) != ga && !wf_core::menu::active(w) {
            first_area = Some((k, a, ga));
        }
        if let (Some(pi), Some(gp)) = (w.player_idx(0), r["p"].as_array()) {
            let p = (w.pl_ref(pi).a.x as i64, w.pl_ref(pi).a.y as i64);
            let g = (gp[0].as_i64().unwrap_or(0), gp[1].as_i64().unwrap_or(0));
            // (scene rows where the game's "player" is another object pass; a divergence counts after 150 rows)
            if p != g {
                pos_bad += 1;
                if first_pos.is_none() && pos_bad >= 150 {
                    first_pos = Some((k + 1 - pos_bad, p, g));
                    let pl = w.pl_ref(pi);
                    println!("position divergence since row {}: app leader hp {}/{} ep {}/{} level {} form {} state {} | game hp {} p {}", k + 1 - pos_bad, pl.rec.hp, pl.rec.max_hp, pl.rec.ep, pl.rec.max_ep, pl.rec.level, pl.form, pl.gs.name(), rows[k + 1 - pos_bad]["hp"], rows[k + 1 - pos_bad]["p"]);
                }
            } else {
                pos_bad = 0;
            }
        }
        // the leader's HP (rows carry the current character's record HP)
        if let (Some(pi), Some(gh)) = (w.player_idx(0), r["hp"].as_i64()) {
            let pl = w.pl_ref(pi);
            // (only while the game shows a player: its row HP is the current record's, which a scene can switch)
            if !r["p"].is_null() && pl.rec.hp as i64 != gh && w.story.current as i64 == r["goal"].as_i64().map(|_| w.story.current as i64).unwrap_or(w.story.current as i64) {
                hp_bad += 1;
                if hp_bad == 30 && hp_reports < 6 {
                    hp_reports += 1;
                    println!("HP divergence since row {}: app {}/{} (char {} level {} form {}) vs game {} in {:?}", k + 1 - hp_bad, pl.rec.hp, pl.rec.max_hp, w.story.current, pl.rec.level, pl.form, gh, r["area"]);
                }
            } else {
                hp_bad = 0;
            }
        }
        // story flags: a row carries `flags` (hex of GD+0x55 ..) when they changed; compare 20 rows later (the app
        // may set a flag a few rows off inside the same event)
        if let Some(h) = r.get("flags").and_then(|v| v.as_str()) {
            flag_check = Some((k + 20, h.to_string()));
        }
        if flag_check.as_ref().is_some_and(|(at, _)| *at == k) && flag_reports < 8 {
            let (_, h) = flag_check.take().unwrap();
            let game: Vec<u8> = (0..h.len() / 2).filter_map(|j| u8::from_str_radix(&h[2 * j..2 * j + 2], 16).ok()).collect();
            let diff: Vec<String> = (0..game.len().min(w.story.flags.len()) * 8)
                .filter(|&f| (game[f >> 3] >> (f & 7)) & 1 != (w.story.flags[f >> 3] >> (f & 7)) & 1)
                .map(|f| format!("{f}:{}", if (game[f >> 3] >> (f & 7)) & 1 != 0 { "game only" } else { "app only" }))
                .collect();
            if !diff.is_empty() {
                flag_reports += 1;
                println!("flags differ at row {k}: {diff:?}");
            }
        }
        if dump_at.is_some_and(|(a, b)| a <= k && k <= b) {
            if let Some(pi) = w.player_idx(0) {
                let pl = w.pl_ref(pi);
                for (ei, e) in w.ents.iter().enumerate() { if let wf_core::world::Body::Enemy(en) = &e.body { if !e.removed { println!("  row {k} enemy #{} ty {} pos ({}, {}) hp {} marked {:?} det {} anim {:?} ai {:?}", ei, en.ty, en.a.x, en.a.y, en.hp1024 >> 10, en.marked, en.detached, en.ea.pos, en.ai.stack.iter().map(|s| s.state).collect::<Vec<_>>()); if std::env::var("SOAK_AI").is_ok() { if let Some(st) = en.ai.stack.last() { for (ni, (n, r)) in st.tree.nodes.iter().zip(st.rt.iter()).enumerate() { if true { println!("    node {ni} {} {:?} f {} i {} on {} d {:?}", n.name, n.p, r.f, r.i, r.on, r.d); } } } } } } }
                // --diff-seg FILE, or ROW=FILE,ROW=FILE.. (each at its row)
                let seg_at = opt("--diff-seg").map(|v| v.strip_prefix('@').map(|f| std::fs::read_to_string(f).expect("list").trim().to_string()).unwrap_or(v)).and_then(|v| if v.contains('=') { v.split(',').find_map(|kv| kv.split_once('=').filter(|(r, _)| r.parse::<usize>().ok() == Some(k)).map(|(_, f)| f.to_string())) } else { Some(v) });
                if let Some(seg) = seg_at {
                    println!("  diff vs {seg} at row {k}");
                    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&seg).expect("read")).expect("json");
                    let fresh = StorySim::from_segment(&packs, &doc).expect("import");
                    let v = fresh.world();
                    let (a, b) = (w.col.as_ref(), v.col.as_ref());
                    if let (Some(a), Some(b)) = (a, b) {
                        let g = wf_core::content::GRID;
                        let de: Vec<(usize, usize, u8, u8)> = a.enemy.iter().zip(b.enemy.iter()).enumerate().filter(|(_, (x, y))| x != y).map(|(j, (x, y))| (j % g, j / g, *x, *y)).take(40).collect();
                        println!("  enemy grid diffs (x, y, app, import): {} {:?}", a.enemy.iter().zip(b.enemy.iter()).filter(|(x, y)| x != y).count(), de);
                        let wa = a.walk.clone().unwrap_or_else(|| a.map.walk.clone());
                        let wb = b.walk.clone().unwrap_or_else(|| b.map.walk.clone());
                        let dw: Vec<(usize, usize, u8, u8)> = wa.iter().zip(wb.iter()).enumerate().filter(|(_, (x, y))| x != y).map(|(j, (x, y))| (j % g, j / g, *x, *y)).take(40).collect();
                        println!("  walk grid diffs: {} {:?}", wa.iter().zip(wb.iter()).filter(|(x, y)| x != y).count(), dw);
                    }
                    for (ei, e) in v.ents.iter().enumerate() { if let wf_core::world::Body::Enemy(en) = &e.body { if !e.removed { println!("  import enemy #{} ty {} pos ({}, {}) hp {} marked {:?} anim {:?} ai {:?}", ei, en.ty, en.a.x, en.a.y, en.hp1024 >> 10, en.marked, en.ea.pos, en.ai.stack.iter().map(|s| (s.state, s.rt.iter().map(|r| r.path).collect::<Vec<_>>())).collect::<Vec<_>>()); } } }
                    for (ei, e) in w.ents.iter().enumerate() { if let wf_core::world::Body::Enemy(en) = &e.body { if !e.removed { println!("  app    enemy #{} ty {} pos ({}, {}) hp {} marked {:?} anim {:?} ai {:?}", ei, en.ty, en.a.x, en.a.y, en.hp1024 >> 10, en.marked, en.ea.pos, en.ai.stack.iter().map(|s| (s.state, s.rt.iter().map(|r| r.path).collect::<Vec<_>>())).collect::<Vec<_>>()); } } }
                    for (nm, ww) in [("app", w), ("imp", v)] {
                        let o: Vec<String> = ww.ents.iter().enumerate().filter(|(_, e)| !e.removed).map(|(k, e)| format!("{k}:{}", match &e.body {
                            wf_core::world::Body::Player(_) => "P".to_string(),
                            wf_core::world::Body::Enemy(en) => format!("E{:x}", en.ty),
                            wf_core::world::Body::Npc(n) => format!("N{}", n.sprite_id),
                            b => format!("o[{}]", format!("{b:?}").chars().take(60).collect::<String>()),
                        })).collect();
                        println!("  order {nm} {}", o.join(" "));
                    }
                    if let (Some(a), Some(b)) = (w.field.as_ref().and_then(|f| f.vram.as_ref()), v.field.as_ref().and_then(|f| f.vram.as_ref())) {
                        let mut oa: Vec<String> = a.owners.iter().map(|o| format!("{}:{:?}", o.0, o.1)).collect();
                        let mut ob: Vec<String> = b.owners.iter().map(|o| format!("{}:{:?}", o.0, o.1)).collect();
                        oa.sort(); ob.sort();
                        println!("  vram app owners {:?} rc {:?} susp {}\n  vram imp owners {:?} rc {:?} susp {}", oa, a.shared_rc, a.suspended.len(), ob, b.shared_rc, b.suspended.len());
                    }
                    if std::env::var("SOAK_G").is_ok() { println!("  g app {:?}\n  g imp {:?}", w.g, v.g); }
                }
                println!("leader at row {k}: rec {:?} equip {:?} skills {:?} form {} eff {:?} story equipment {:?}", pl.rec, pl.equip, pl.skills, pl.form, pl.eff, w.story.equipment);
            }
        }
        if show.is_some_and(|(a, b)| a <= k && k <= b) {
            let pl = w.player_idx(0).map(|pi| w.pl_ref(pi)).map(|p| (p.a.x, p.a.y, p.a.facing, p.gs.name(), p.rec.hp, p.rec.ep, (p.form, p.ss_gauge, p.ss_acc, p.ss3_gauge, p.ss_cooldown, p.regen_timer, p.story_cnt)));
            println!("row {k} loads {:?} props {:?} started {:?} keys {keys:x} app {pl:?} pending {:?} tracks {:?} busy {:?} depth {} | game p {} hp {} depth {}", w.field.as_ref().map(|f| f.loads), w.scene.props.iter().map(|p| (p.id, p.x >> 8, p.y >> 8, p.load)).collect::<Vec<_>>(), app.host().started.iter().rev().take(3).collect::<Vec<_>>(), app.host().hs.pending, w.scene.tracks.iter().filter(|t| !t.queue.is_empty()).map(|t| (t.owner, format!("{:?}", t.queue), t.started, t.live)).collect::<Vec<_>>(), w.field.as_ref().map(|f| (f.busy, f.nested, f.held, f.held_event)), w.scene.depth, r["p"], r["hp"], r["depth"]);
        }
        if let Some(gr) = r["rng"].as_array() {
            let g = [gr[0].as_u64().unwrap_or(0) as u32, gr[1].as_u64().unwrap_or(0) as u32];
            // (a load spans several video rows in the game and one step here: a divergence counts once it has
            // lasted 40 rows)
            if w.field.is_some() && !wf_core::menu::active(w) && w.rng.state() != g {
                rng_bad += 1;
                if first_rng.is_none() && rng_bad >= 150 {
                    first_rng = Some((k + 1 - rng_bad, w.rng.state(), g));
                }
            } else {
                rng_bad = 0;
            }
        }
    }
    let s = &app;
    let w = s.world();
    println!("rows {} stepped {} | script errors {}", rows.len().min(limit), stepped, s.errors.len());
    println!("continued at row {continue_row:?}");
    println!("first area divergence: {first_area:?}");
    println!("first position divergence: {first_pos:?}");
    println!("first RNG divergence: {first_rng:?}");
    println!("app areas:  {:?}", &areas[areas.len().saturating_sub(40)..]);
    println!("game areas: {:?}", &game_areas[game_areas.len().saturating_sub(40)..]);
    println!("songs: {:?}", &songs[songs.len().saturating_sub(30)..]);
    let flags: Vec<usize> = (0..448).filter(|&f| w.story.flags.get(f >> 3).is_some_and(|b| b >> (f & 7) & 1 != 0)).collect();
    println!("end: frame {} current char {} zenie {} flags {:?}", w.frame, w.story.current, w.story.zenie, flags);
    println!("saves: generation {} slots {:?}", w.saves.generation, (0..3).map(|i| w.saves.slot_bytes(i).len()).collect::<Vec<_>>());
    if let Some(out) = opt("--save-out") {
        std::fs::write(out, w.saves.slot_bytes(0)).expect("write slot");
    }
}
