//! The whole real run on the app from power-on, no RAM import anywhere (headless, muted, no window / audio device):
//! the launcher's title session fed the emulator's whole-run recording (`tools/wholerun/record.py`: the soft reset,
//! then every key log of the real run), compared row by row.
//!
//!   cargo run --release -p wf-app --example wholerun -- MASTER.jsonl [--lag rec|sim] [--frames N] [--show A..B]
//!       [--rng A..B] [--report OUT.json]
//!
//! `--lag rec` (default): the recording's lag injected - a row is a logic step when its logic counter moved (or a menu
//! shows), else the sound hardware's frame only (as the app soak and the segment harness). `--lag sim`: the sim makes
//! its own load lag (`Options::sim_lag`, `Session::lag_step`), as `SEGMENT_SIMLAG=1`; a lag frame the sim does not model
//! is still taken from the recording (counted). Exact = every compared field equal on the row: area (outside menus),
//! the leader's position and facing (when both show one), the RNG (in the field, outside menus), the scene depth.
//! Prints the first exact divergence (row, stretch, field) and each field's first lasting divergence (150 rows).

use std::io::{BufRead, BufReader};
use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::Sim;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let master = args.first().expect("MASTER.jsonl").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let limit: usize = opt("--frames").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let sim_lag = opt("--lag").as_deref() == Some("sim");
    // --lag none: no lag frames at all - one app step per game logic iteration (the keys the game sampled at its
    // start, the row where the counter moved), compared at the iteration's end; nothing for the lag rows
    // (recorded lag too, as the segment harness now: the sim runs without lag (menus step on the recorded logic rows
    // only), the recording's lag rows - load iterations, menu redraws and closes - replayed as plain lag frames)
    let no_lag = !sim_lag && std::env::var("WR_MENULAG").is_err();
    let rec_lag = opt("--lag").as_deref() != Some("none") && !sim_lag;
    let mode_name = if sim_lag { "sim" } else if rec_lag { "recorded" } else { "none" };
    let range = |name: &str| opt(name).and_then(|v| v.split_once("..").and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))));
    let show = range("--show");
    let rng_rows = range("--rng");
    let index: serde_json::Value = std::fs::read_to_string(format!("{master}.index.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let stretches: Vec<(String, usize)> = index["stretches"].as_array().map(|a| a.iter().map(|s| (s[0].as_str().unwrap_or("").to_string(), s[1].as_u64().unwrap_or(0) as usize)).collect()).unwrap_or_default();
    let stretch_of = |k: usize| stretches.iter().rev().find(|s| s.1 <= k).map(|s| s.0.clone()).unwrap_or_default();

    // power-on: the launcher's title app (`launcher::title_app`), no slot, no import, no sync
    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    // (the RNG words at power-on: cleared RAM, [0, 0] - the game draws nothing until the prologue)
    let cfg = wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::State(0, 0), ..Default::default() };
    let mut session = wf_core::session::Session::new_field(data, cfg);
    // (the placeholder field under the title is ours only: its build's draws are not the game's - as `title_world`)
    session.world.rng = wf_core::rng::Rng::new(0, 0);
    // (--no-boot: the title at once, as the launcher opens it today)
    if args.iter().any(|a| a == "--no-boot") {
        wf_core::menu::title::open_title(&mut session.world);
    } else {
        session.world.opts.no_lag = no_lag;
        wf_core::menu::title::open_title_boot(&mut session.world);
    }
    // --power-on FILE (tools/wholerun/power_on.py): the sound hardware at the reset - the driver's RAM as the boot code
    // sets it (the same after every reset) and the reset moment's play position and timer phase. Nothing else.
    if let Some(p) = opt("--power-on") {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).expect("power-on")).expect("json");
        let hex = v["driver"].as_str().unwrap_or("");
        let dram: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap_or(0)).collect();
        let n = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
        if session.world.data.sound.is_some() {
            wf_core::sndsim::import(&mut session.world, &dram, n("pos"), n("last"), Some(n("phase")));
            println!("power-on sound: pos {} last {} phase {}", n("pos"), n("last"), n("phase"));
        } else {
            println!("power-on sound: no sound data loaded - ignored");
        }
    }
    // --import SEG.json --from-row K (the resync mode, tools/wholerun/board.py --resync): the game's state cut at row K
    // of the chain (its RAM before row K) instead of the power-on; the rows before K are skipped. --stop-first: end at
    // the first exact divergence with a `STOP {json}` line (an import the importer refuses: `STOP` with "import")
    let from_row: usize = opt("--from-row").and_then(|v| v.parse().ok()).unwrap_or(0);
    let stop_first = args.iter().any(|a| a == "--stop-first");
    let mut app = match opt("--import") {
        Some(seg) => {
            let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&seg).expect("read")).expect("json");
            match StorySim::from_segment(&packs, &doc) {
                Ok(a) => a,
                Err(e) => {
                    println!("STOP {}", serde_json::json!({"row": from_row, "field": "import", "what": e}));
                    return;
                }
            }
        }
        None => StorySim::with_session(&packs, session).expect("story sim"),
    };
    if opt("--import").is_none() {
        app.skip_area_banner();
    }
    app.session.world.opts.sim_lag = sim_lag;
    app.session.world.opts.no_lag = no_lag;
    println!("sound model: driver {} calibrated {} phase {}", app.world().sound.drv.is_some(), !app.world().sound.uncalibrated, app.world().sound.phase);

    // every row's logic counter and camera up front (the menu close's recorded length and camera step look ahead)
    let (lfs, cams): (Vec<i64>, Vec<serde_json::Value>) = BufReader::new(std::fs::File::open(&master).expect("master"))
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(&l.expect("line")).expect("row");
            (v["lf"].as_i64().unwrap_or(0), v["cam"].clone())
        })
        .unzip();
    let mut was_leaving = false;
    let rd = BufReader::new(std::fs::File::open(&master).expect("master"));
    let mut raw = rd.lines();
    // (no lag: row 0 is not an iteration - the counter is 0 there)
    let mut prev_lf = if no_lag { 0 } else { -1i64 };
    // (skip to the import's row without parsing; the row before it gives the logic counter)
    for j in 0..from_row {
        let l = raw.next().expect("rows").expect("line");
        if j + 1 == from_row {
            prev_lf = serde_json::from_str::<serde_json::Value>(&l).expect("row")["lf"].as_i64().unwrap_or(-1);
        }
    }
    let mut lines = raw.map(|l| serde_json::from_str::<serde_json::Value>(&l.expect("line")).expect("row")).peekable();
    let mut first_exact: Option<(usize, String)> = None;
    // lasting divergences per field: (count of consecutive bad rows, first row of the run, reported)
    let mut game_flags: Vec<u8> = Vec::new();
    let mut lasting: Vec<(&str, usize, usize, Option<(usize, String)>)> = ["area", "pos", "rng", "depth", "flags"].iter().map(|n| (*n, 0, 0, None)).collect();
    let (mut stepped, mut own_lag, mut replayed_lag, mut lag_bad) = (0usize, 0usize, 0usize, 0usize);
    let mut errors = 0usize;
    let (mut load_watch, mut load_bad): (Option<(usize, usize)>, usize) = (None, 0);
    let mut clock_reported = false;
    let mut k = from_row;
    let mut lfd_last: Option<i64> = None;
    let mut hp_bad = false;
    // --sounds FILE: the game's sound requests per row (tools/wholerun/sounds.py; .json or .json.gz) - compared per
    // logic iteration in no-lag mode; SND lines for the first `--sounds-show N` (default 40) differing iterations
    let game_snd: Option<std::collections::HashMap<usize, Vec<String>>> = opt("--sounds").map(|p| {
        let txt = if p.ends_with(".gz") {
            String::from_utf8(std::process::Command::new("gzip").args(["-dc", &p]).output().expect("gzip").stdout).expect("utf8")
        } else {
            std::fs::read_to_string(&p).expect("sounds")
        };
        let v: serde_json::Value = serde_json::from_str(&txt).expect("sounds json");
        v["ev"]
            .as_array()
            .expect("ev")
            .iter()
            .map(|r| {
                let evs = r[1]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| {
                        let a = s.as_array()?;
                        let n = |k: usize| a.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                        Some(match a[0].as_str()? {
                            "play" => format!("play {} {}", n(1), n(2)),
                            "stop" | "stopmatch" => format!("stop {}", n(1)),
                            "stopall" => "stopall".into(),
                            "song" => format!("song {}", n(1)),
                            o => o.to_string(),
                        })
                    })
                    .collect();
                (r[0].as_u64().unwrap_or(0) as usize, evs)
            })
            .collect()
    });
    let snd_show: usize = opt("--sounds-show").and_then(|v| v.parse().ok()).unwrap_or(40);
    let (mut snd_bad, mut snd_iters) = (0usize, 0usize);
    let mut sup_last: Option<Vec<(usize, u8)>> = None;
    while let Some(r) = lines.next() {
        if k >= limit {
            break;
        }
        let keys = r["keys"].as_u64().unwrap_or(0) as u16;
        let lf = r["lf"].as_i64().unwrap_or(0);
        let logic = lf != prev_lf;
        prev_lf = lf;
        let menu = wf_core::menu::active(app.world());
        let next_lf = lines.peek().map(|n| n["lf"].as_i64().unwrap_or(0));
        let trace = rng_rows.is_some_and(|(a, b)| a <= k && k <= b);
        if trace {
            eprintln!("  rng calls of row {k} (app rng before {:08x?}, game after {})", app.world().rng.state(), r["rng"]);
        }
        wf_core::rng::TRACE.store(trace, std::sync::atomic::Ordering::Relaxed);
        // (the option is the app's: a world the title builds (New Game) starts without it)
        app.session.world.opts.sim_lag = sim_lag;
        app.session.world.opts.no_lag = no_lag;
        // (sim lag: each load's lag run against the recording's lag run from the same row)
        if sim_lag {
            if logic {
                if let Some((at, sim)) = load_watch.take() {
                    let game = k - at;
                    if game != sim {
                        load_bad += 1;
                        if load_bad <= 12 {
                            println!("sim-lag load at row {at} ({}): sim {sim} lag frames, game {game}", stretch_of(at));
                        }
                    }
                }
            }
            if app.world().g.load_pending && !menu {
                load_watch = Some((k, 0));
            }
        }
        if sim_lag && !menu && app.session.lag_step() {
            if let Some(lw) = load_watch.as_mut() {
                lw.1 += 1;
            }
            own_lag += 1;
            if logic {
                lag_bad += 1;
                if lag_bad <= 5 || (lag_bad <= 40 && std::env::var("WR_LAG").is_ok()) {
                    println!("sim lag frame on a game logic row {k} ({}): app area {:?} lag left {}", stretch_of(k), app.world().field.as_ref().map(|f| (f.area.world, f.area.area)), app.world().g.lag);
                }
            }
        } else if logic
            || (menu
                && !no_lag
                // (as the segment harness: the pause menu's opening fade-out is still the field's loop - a lag row
                // there is a frame without an update; a script-opened menu fades inside its own loop)
                && !(matches!(app.world().menu.phase, wf_core::menu::Phase::FadeOut { .. }) && !app.world().menu.scripted)
                // (as the segment harness: a power gauge that ended inside the last world frame - the lag after it is
                // the script's, the screen's last step is the next world frame's; the ending's Spirit Bomb, whole run
                // row 1,638,553)
                && !(wf_core::menu::ended_in_world(app.world()) && wf_core::menu::minigame::world_runs(app.world())))
        {
            let mut ev = vec![];
            app.step(&[keys], &mut ev);
            stepped += 1;
            // --sounds FILE (tools/wholerun/sounds.py): this iteration's sound requests against the game's - the
            // game's of every row with this logic counter (a request in a long iteration lands on one of its rows)
            if let Some(gs) = game_snd.as_ref().filter(|_| logic && no_lag) {
                let mut game: Vec<String> = vec![];
                let mut j = k;
                while j < lfs.len() && lfs[j] == lf {
                    if let Some(v) = gs.get(&j) {
                        game.extend(v.iter().cloned());
                    }
                    j += 1;
                }
                game.sort();
                let mut ours: Vec<String> = app
                    .world()
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        wf_core::world::Event::Sfx { id, vol } => Some(format!("play {id} {vol}")),
                        wf_core::world::Event::SfxStop { id } | wf_core::world::Event::SfxStopMatch { id } => Some(format!("stop {id}")),
                        wf_core::world::Event::SfxStopAll => Some("stopall".into()),
                        wf_core::world::Event::Song { id } => Some(format!("song {id}")),
                        _ => None,
                    })
                    .collect();
                ours.sort();
                if ours != game {
                    snd_bad += 1;
                    if snd_bad <= snd_show {
                        println!("SND row {k} ({}) lf {lf} menu {menu}: app {ours:?} game {game:?}", stretch_of(k));
                    }
                }
                snd_iters += 1;
            }
        } else {
            // the game's lag frame (recorded): the sound hardware's frame only
            if sim_lag {
                replayed_lag += 1;
            }
            // (no lag: nothing happens in the app for a lag row - not even the sound hardware's frame)
            if rec_lag || sim_lag {
                wf_core::sndsim::lag_frame_ex(&mut app.session.world, next_lf.is_some_and(|n| n != lf));
            }
        }
        wf_core::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
        // a menu close's black frames (the world restore's long iteration) are alignment, not logic - as the segment
        // harness (`segment.rs` replay): the close takes the recording's length, the video frames up to the next logic
        // frame (`Menu.tr.leave`), and the camera's restore step in the frame the recorded camera first moves
        // (`Menu.leave_cam`). Recorded-lag mode only (no lag: the close is logic-frame compared)
        {
            let w = &mut app.session.world;
            let leaving = matches!(w.menu.phase, wf_core::menu::Phase::Leave { .. }) && w.menu.tr.fade_in != 0;
            if !no_lag && !sim_lag && leaving && !was_leaving && w.menu.reload_close && w.menu.phase == (wf_core::menu::Phase::Leave { t: 0 }) && k + 1 < lfs.len() {
                let lf1 = lfs[k + 1];
                let run = lfs[k + 1..].iter().take_while(|&&n| n == lf1).count();
                if k + 1 + run < lfs.len() {
                    let leave = run as u16 + 1;
                    let cam0 = &cams[k];
                    let cam = cams[k + 1..k + 1 + run].iter().position(|c| c != cam0).map(|j| (leave as i64 - 2 - j as i64).clamp(0, 3) as u16).unwrap_or(w.menu.leave_cam);
                    w.menu.tr.leave = leave;
                    w.menu.leave_cam = cam;
                }
            }
            was_leaving = leaving;
        }
        if app.errors.len() > errors {
            for e in &app.errors[errors..] {
                println!("row {k}: script error: {e}");
            }
            errors = app.errors.len();
        }

        // the comparison
        let w = app.world();
        let menu = wf_core::menu::active(w);
        let mut bad: Vec<(&str, String)> = vec![];
        let ga = (r["area"][0].as_i64().unwrap_or(-1), r["area"][1].as_i64().unwrap_or(-1));
        let aa = w.field.as_ref().map(|f| (f.area.world as i64, f.area.area as i64)).unwrap_or((-1, -1));
        // (the game's RAM is a finished main-loop iteration only on the row before the next one starts - the next
        // row moves the logic counter; a long iteration's rows show it half done: an area load's half-built player,
        // a draw made in its lag frames. Those rows are compared, the others not)
        let complete = next_lf.map_or(true, |n| n != lf);
        if std::env::var("WR_CLOCK").is_ok() {
            println!("clock row {k} app {} game {lf} menu {menu} field {} fc0 {} frame {} phase {:?} shown {:?}", w.g.clock(w.frame), w.field.is_some(), w.g.fc0, w.frame, w.menu.phase, w.menu.shown.as_ref().map(|s| format!("{s:?}").chars().take(40).collect::<String>()));
        }
        // the frame counter 0x03000F94 (`Globals::clock`; the punch marker's blink, enemy state stamps): reported once
        if complete && !clock_reported && !menu && w.field.is_some() && w.g.clock(w.frame) != lf {
            clock_reported = true;
            println!("clock differs from row {k} ({}): app {} game {lf}", stretch_of(k), w.g.clock(w.frame));
        }
        let in_field = ga != (0, 0) && !r["p"].is_null() && complete;
        if !menu && in_field && aa != ga {
            bad.push(("area", format!("app {aa:?} game {ga:?}")));
        }
        let ap = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.x as i64, w.pl_ref(pi).a.y as i64, w.pl_ref(pi).a.facing as i64));
        if let (Some(p), Some(g)) = (ap, r["p"].as_array()) {
            let g = (g[0].as_i64().unwrap_or(0), g[1].as_i64().unwrap_or(0), g[2].as_i64().unwrap_or(0));
            if !menu && complete && p != g {
                bad.push(("pos", format!("app {p:?} game {g:?}")));
            }
        } else if !menu && in_field && ap.is_none() != r["p"].is_null() {
            bad.push(("pos", format!("app leader {:?} game {}", ap, r["p"])));
        }
        if let Some(gr) = r["rng"].as_array() {
            let g = [gr[0].as_u64().unwrap_or(0) as u32, gr[1].as_u64().unwrap_or(0) as u32];
            if w.field.is_some() && !menu && in_field && w.rng.state() != g {
                bad.push(("rng", format!("app {:08x?} game {g:08x?}", w.rng.state())));
            }
        }
        let gd = r["depth"].as_i64().unwrap_or(0);
        if !menu && in_field && (w.scene.depth as i64 > 0) != (gd > 0) {
            bad.push(("depth", format!("app {} game {gd}", w.scene.depth)));
        }
        if let (Some(fl), true) = (std::env::var("WR_FLAG").ok().and_then(|v| v.parse::<usize>().ok()), show.is_some_and(|(a, b)| a <= k && k <= b)) {
            println!("    flag {fl}: app {}", (w.story.flags.get(fl >> 3).copied().unwrap_or(0) >> (fl & 7)) & 1);
        }
        // the story flags (GameData +0x55; a row carries them when they changed): every flag, both ways
        if let Some(h) = r.get("flags").and_then(|v| v.as_str()) {
            game_flags = (0..h.len() / 2).filter_map(|j| u8::from_str_radix(&h[2 * j..2 * j + 2], 16).ok()).collect();
        }
        if complete && !menu && w.field.is_some() && !game_flags.is_empty() {
            let n = game_flags.len().min(w.story.flags.len());
            if let Some(f) = (0..n * 8).find(|&f| (game_flags[f >> 3] >> (f & 7)) & 1 != (w.story.flags[f >> 3] >> (f & 7)) & 1) {
                let game_on = (game_flags[f >> 3] >> (f & 7)) & 1 != 0;
                bad.push(("flags", format!("flag {f} {}", if game_on { "game only" } else { "app only" })));
            }
        }
        if first_exact.is_none() {
            if let Some((f, d)) = bad.first() {
                first_exact = Some((k, format!("{f}: {d}")));
                println!("first exact divergence: row {k} ({}): {f}: {d}", stretch_of(k));
                if stop_first {
                    let w = app.world();
                    let menu_open = wf_core::menu::active(w);
                    let leader = w.player_idx(0).map(|pi| (w.pl_ref(pi).gs.name().to_string(), w.pl_ref(pi).form));
                    println!("STOP {}", serde_json::json!({
                        "row": k, "stretch": stretch_of(k), "field": f, "what": d,
                        "all": bad.iter().map(|b| format!("{}: {}", b.0, b.1)).collect::<Vec<_>>(),
                        "app_depth": w.scene.depth, "game_depth": r["depth"], "menu": menu_open,
                        "game_p": r["p"], "app_leader": leader, "logic": logic,
                        "started": app.host().started.iter().rev().take(2).map(|s| s.1.clone()).collect::<Vec<_>>(),
                    }));
                    return;
                }
            }
        }
        for l in lasting.iter_mut() {
            match bad.iter().find(|b| b.0 == l.0) {
                Some((_, d)) => {
                    if l.1 == 0 {
                        l.2 = k;
                    }
                    l.1 += 1;
                    if l.1 == 150 && l.3.is_none() {
                        l.3 = Some((l.2, d.clone()));
                        println!("lasting {} divergence from row {} ({}): {d}", l.0, l.2, stretch_of(l.2));
                    }
                }
                None => l.1 = 0,
            }
        }
        // WR_STORY=ROW: our story flags (hex, GameData +0x55 layout), the script words and the chapter-ish state at ROW
        if std::env::var("WR_STORY").ok().and_then(|v| v.parse::<usize>().ok()) == Some(k) {
            let st = &w.story;
            println!("STORY row {k} flags {} l {:?} g {:?} current {} zenie {}", st.flags.iter().map(|b| format!("{b:02x}")).collect::<String>(), st.l, st.g, st.current, st.zenie);
            let mut c = w.clone();
            wf_core::story::sync_from_players(&mut c);
            println!("STORY row {k} chars {:?}", c.story.chars.iter().enumerate().map(|(i, ch)| (i, ch.rec.hp, ch.rec.max_hp, ch.rec.ep, ch.rec.max_ep, ch.rec.xp, ch.rec.level)).collect::<Vec<_>>());
        }
        // WR_SUP=1: our supplies whenever they change (the game's rows carry none: compare with a cut's RAM)
        if std::env::var_os("WR_SUP").is_some() {
            let s: Vec<(usize, u8)> = w.story.supplies.iter().enumerate().filter(|(_, &n)| n != 0).map(|(i, &n)| (i, n)).collect();
            if Some(&s) != sup_last.as_ref() {
                println!("SUP row {k} ({}): {s:?}", stretch_of(k));
                sup_last = Some(s);
            }
        }
        // WR_HP=1: the rows where the leader's HP (the current record's, the row's `hp`) starts / stops differing - on
        // a logic row at the iteration's end (the next row with another counter is the next iteration)
        if std::env::var_os("WR_HP").is_some() && logic && next_lf.is_some_and(|n| n != lf) && !menu {
            let mut c = w.clone();
            wf_core::story::sync_from_players(&mut c);
            let ours = c.story.chars.get(c.story.levelup_char.unwrap_or(c.story.current).max(0) as usize).map(|ch| ch.rec.hp as i64);
            let game = r["hp"].as_i64();
            let bad = ours.is_some() && game.is_some() && ours != game;
            if bad != hp_bad {
                println!("HP {} at row {k} ({}): app {ours:?} game {game:?}", if bad { "differs" } else { "equal again" }, stretch_of(k));
                hp_bad = bad;
            }
        }
        // WR_LFD=1: the rows where the game's logic counter (0x03000F94) and ours (play time + bias) change offset
        if std::env::var_os("WR_LFD").is_some() {
            let d = lf - (w.story.play_frames as i64 + w.story.lf_bias);
            if Some(d) != lfd_last {
                println!("LFD row {k} ({}) lf {lf} offset {d} (was {:?}) menu {menu} phase {:?} area {aa:?}", stretch_of(k), lfd_last, w.menu.phase);
                lfd_last = Some(d);
            }
        }
        if show.is_some_and(|(a, b)| a <= k && k <= b) {
            println!("row {k} keys {keys:x} logic {logic} menu {menu} app area {aa:?} p {ap:?} rng {:08x?} depth {} | game area {ga:?} p {} rng {} depth {gd} lf {lf} ourlf {} clock {}", w.rng.state(), w.scene.depth, r["p"], r["rng"], w.story.play_frames as i64 + w.story.lf_bias, w.g.clock(w.frame));
            if std::env::var("WR_HOST").is_ok() {
                println!("    host started {:?} pending {:?} fade {:?}", app.host().started.iter().rev().take(2).collect::<Vec<_>>(), app.host().hs.pending, w.scene.screen.fade);
            }
            if menu || std::env::var("WR_MENU").is_ok() {
                let page = match w.menu.stack.last() {
                    Some(wf_core::menu::Screen::Title(t)) => format!("title {:?} leaving {:?}", t.page, t.leaving),
                    Some(wf_core::menu::Screen::Status(s)) => format!("status alloc {} vals {:?} orig {:?} blink {} drawn {}", s.alloc, s.vals, s.orig, s.blink, s.c.drawn),
                    Some(wf_core::menu::Screen::Minigame(m)) => format!("minigame counter {} wait {} done {} peak {} pos {} | play_frames+bias {} clock {} ticks {} pf {}", m.counter, m.wait, m.g.done, m.g.peak, m.g.pos, w.story.play_frames as i64 + w.story.lf_bias, w.g.clock(w.frame), w.menu.ticks, w.story.play_frames),
                    Some(s) => format!("other {:?} busy {:?} scripted {}", std::mem::discriminant(s), w.field.as_ref().map(|f| f.busy), w.menu.scripted),
                    None => format!("- busy {:?} raw {:?}", w.field.as_ref().map(|f| f.busy), w.field.as_ref().map(|f| f.raw_input.is_some())),
                };
                println!("    menu phase {:?} lag {} boot {} page {page} pending {} shown {}", w.menu.phase, w.menu.lag, w.menu.boot, w.menu.pending.is_some(), w.menu.shown.is_some());
            }
        }
        k += 1;
    }
    let exact_to = first_exact.as_ref().map(|f| f.0).unwrap_or(k);
    println!("rows {k} stepped {stepped} | sim lag {own_lag} (on game logic rows {lag_bad}), recorded lag replayed {replayed_lag} | script errors {errors}");
    if game_snd.is_some() {
        println!("sounds: {snd_bad} of {snd_iters} logic iterations differ");
    }
    println!("whole run, no imports ({} lag): exact to row {exact_to} of {k} ({})", mode_name, stretch_of(exact_to));
    for l in &lasting {
        println!("  first lasting {}: {:?}", l.0, l.3.as_ref().map(|(r, d)| (r, stretch_of(*r), d)));
    }
    if let Some(out) = opt("--report") {
        let rep = serde_json::json!({
            "lag": mode_name,
            "rows": k,
            "exact_to": exact_to,
            "exact_stretch": stretch_of(exact_to),
            "first": first_exact.as_ref().map(|f| f.1.clone()),
            "lasting": lasting.iter().map(|l| (l.0.to_string(), l.3.as_ref().map(|(r, d)| serde_json::json!({"row": r, "stretch": stretch_of(*r), "what": d})))).collect::<std::collections::BTreeMap<_, _>>(),
            "stepped": stepped, "sim_lag": own_lag, "sim_lag_on_logic": lag_bad, "recorded_lag_replayed": replayed_lag, "script_errors": errors,
        });
        std::fs::write(out, serde_json::to_string_pretty(&rep).unwrap()).expect("report");
    }
}
