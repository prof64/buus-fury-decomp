//! Menu screens vs the emulator, tick by tick and frame by frame (headless, muted; skipped without the ROM).
//!
//! Traces: `<repo>/data/menu/<scenario>.json` (`re/buu/menu/capture.py`: keys, the mode loop's frame counter,
//! sound calls and the sha1 of the emulator's screen per video frame, the GameData block decoded at frame 0 and how
//! the screen opens). The remake's story session gets that state, then both run the same keys. Compared per video
//! frame: the sounds and, while the remake shows a menu screen, the sha1 of the 240x160 picture (wf-core menu view
//! -> wf-render), one remake step per game logic iteration (`Options::no_lag`: the app recreates no lag), compared
//! at each iteration's last video frame (see `run`). Pictures for diffs: `work/buu/menu/<scenario>/`
//! (git-ignored, ROM-derived; `WF_MENU_DIR` overrides): `MENU_PARITY_OUT=<dir>` writes emulator | ours | diff
//! strips of differing frames; `MENU_PARITY_LOG=1` prints every frame; `MENU_PARITY_ONLY=<substring>` filters.
//! Scenarios whose names start with a `STRICT` prefix must be exact (sounds and every compared frame).

use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_core::field::FieldConfig;
use wf_core::session::Session;
use wf_core::story::{CharState, Story};
use wf_render::Renderer;
use wf_view::{AudioEvent, RenderView, Sim, SimEvent};

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| v.as_u64().map(|u| u as i64).unwrap_or(0))
}

fn load_png(p: &Path) -> Option<Vec<u32>> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).ok()?));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let ch = info.color_type.samples();
    Some(buf.chunks(ch).take(240 * 160).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect())
}

fn save(p: &Path, imgs: &[&[u32]]) {
    let (w, h) = (240 * imgs.len(), 160);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for im in imgs {
            for x in 0..240 {
                let c = im[y * 240 + x];
                rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    let f = std::fs::File::create(p).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

/// the GameData block (capture `story.json`) as the remake's story
pub fn story_from(d: &wf_core::content::GameData, s: &Value) -> Story {
    let mut st = Story::new_game(d);
    st.current = i(&s["current"]) as i32;
    st.game_bits = i(&s["game_bits"]) as u32;
    if let Some(a) = s["supplies"].as_array() {
        for (k, v) in a.iter().enumerate() {
            if k < st.supplies.len() {
                st.supplies[k] = i(v) as u8;
            }
        }
    }
    if let Some(h) = s["flags"].as_str() {
        for k in 0..(h.len() / 2).min(st.flags.len()) {
            st.flags[k] = u8::from_str_radix(&h[2 * k..2 * k + 2], 16).unwrap();
        }
    }
    // the remake's owned list holds unequipped copies (equipped ones are the characters' slots): marked entries out
    st.equipment = s["equipment"].as_array().map(|a| a.iter().map(|v| (i(v) & 0x0FFF_FFFF) as i32).collect()).unwrap_or_default();
    st.zenie = i(&s["zenie"]) as u32;
    st.play_frames = (i(&s["counter"]) + i(&s["play_base"])).max(0) as u64;
    if let Some(cs) = s["chars"].as_array() {
        for (k, c) in cs.iter().enumerate().take(st.chars.len()) {
            let skills: Vec<i32> = c["skills"].as_array().map(|a| a.iter().map(|v| i(v) as i32).collect()).unwrap_or_default();
            let levels: Vec<i32> = c.get("skill_max").unwrap_or(&c["skill_levels"]).as_array().map(|a| a.iter().map(|v| i(v) as i32).collect()).unwrap_or_default();
            let eq: Vec<i32> = c["equip"].as_array().map(|a| a.iter().map(|v| i(v) as i32).collect()).unwrap_or_default();
            let mut rec = st.chars[k].rec.clone();
            rec.hp = i(&c["hp"]) as i32;
            rec.max_hp = i(&c["max_hp"]) as i32;
            rec.ep = i(&c["ep"]) as i32;
            rec.max_ep = i(&c["max_ep"]) as i32;
            rec.xp = i(&c["xp"]);
            rec.level = wf_core::progression::level_of(rec.xp, &d.xp_table);
            rec.str_ = i(&c["str"]) as i32;
            rec.pow = i(&c["pow"]) as i32;
            rec.end = i(&c["end"]) as i32;
            rec.spd = i(&c["spd"]) as i32;
            rec.points = i(&c["points"]) as i32;
            st.chars[k] = CharState {
                rec,
                form: i(&c["form"]) as i32,
                skills,
                skill_levels: levels,
                sel: i(&c["sel"]) as i32,
                equip: [eq[0], eq[1], eq[2], eq[3]],
                ss_cooldown: i(&c["ss_cooldown"]) as i32,
                flags: i(&c["flags"]) as u8,
                skill_cur: c["skill_cur"].as_array().map(|a| a.iter().map(|v| i(v) as i32).collect()).unwrap_or_default(),
                fusion: 0,
                fusion_time: 0,
                ss_gauge: 0,
                ss3_gauge: 0,
            };
        }
    }
    st
}

/// a picture differing from both neighbours: the VRAM copy raced the scanout (sub-frame CPU timing)
fn tear(hs: &[String], k: usize) -> bool {
    k > 0 && k + 1 < hs.len() && hs[k] != hs[k - 1] && hs[k] != hs[k + 1] && hs[k - 1] != hs[k + 1]
}

/// a scanout tear in a one-frame iteration: the capture's picture `k` is the next picture except one band of at
/// most a tile row (8 lines) still holding older content (a VRAM write that finished during the scanout, e.g. a
/// nested mode's box drawn from a press handler: title_dojo frame 171, rows 64..67 of its top edge); needs the
/// work/ PNGs
fn split_tear(d: &Path, k: usize) -> bool {
    let png = |j: usize| load_png(&d.join(format!("f{j:04}.png")));
    let (Some(b), Some(c)) = (png(k), png(k + 1)) else {
        return false;
    };
    let bad: Vec<usize> = (0..160).filter(|&y| b[y * 240..y * 240 + 240] != c[y * 240..y * 240 + 240]).collect();
    !bad.is_empty() && bad.len() <= 8 && bad[bad.len() - 1] - bad[0] + 1 == bad.len()
}

fn dir() -> PathBuf {
    std::env::var("WF_MENU_DIR").map(PathBuf::from).unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/buu/menu"))
}

fn traces() -> PathBuf {
    wf_rom::testutil::repo_root().join("data/menu")
}

/// scenarios that must be exact (prefixes)
const STRICT: &[&str] = &["shop_", "computer_", "save_", "title_", "charsel_", "status_", "inventory_", "equip_", "cr_", "mg_", "options_", "journal_", "skills_", "cr_", "map_snake", "map_forest", "map_left", "map_scouter"];

/// the capture's picture hash: sha1 of the RGB bytes, first 16 hex digits
fn hash(fb: &[u32]) -> String {
    let mut h = Sha1::new();
    let mut rgb = Vec::with_capacity(240 * 160 * 3);
    for &c in fb.iter().take(240 * 160) {
        rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
    }
    h.update(&rgb);
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// result of one scenario: (frames compared, frames exact, sound mismatches)
fn run(packs: &Packs, data: &Arc<wf_core::content::GameData>, name: &str) -> Option<(usize, usize, Vec<String>, Vec<usize>)> {
    let d = dir().join(name);
    let tr: Value = serde_json::from_slice(&std::fs::read(traces().join(format!("{name}.json"))).ok()?).ok()?;
    let frames = &tr;
    let sj = &tr["story"];
    let story = story_from(data, sj);
    let mut cfg = FieldConfig { world: 1, area: 1, story: Some(story), rng: wf_core::battle::RngInit::Seed(1), ..Default::default() };
    // the capture's field place (newer captures): area and the player's position (footprint top-left px)
    if let Some(a) = sj["area"].as_array().filter(|a| a.len() == 3) {
        cfg.world = i(&a[0]) as i32;
        cfg.area = i(&a[1]) as i32;
        cfg.variant = i(&a[2]) as i32;
    }
    if let Some(p) = sj["player"].as_array().filter(|a| a.len() >= 2) {
        cfg.pos = Some((i(&p[0]) as i32, i(&p[1]) as i32));
    }
    let meta = &tr["meta"];
    // screens showing the live field (the map screen's Scouter; `meta.live_field`): the player's direction, the
    // other actors' positions / directions, the camera and the RNG as the emulator had them
    let live = meta["live_field"].as_bool().unwrap_or(false);
    if live {
        if let Some(f) = sj["facing"].as_i64() {
            cfg.facing = f as u8;
        }
    }
    let session = Session::new_field(Arc::clone(data), cfg);
    let mut sim = StorySim::with_session(packs, session).expect("story sim");
    sim.skip_area_banner();
    if live {
        let w = &mut sim.session.world;
        let mut objs: Vec<Option<&Value>> = sj["objs"].as_array().map(|a| a.iter().map(Some).collect()).unwrap_or_default();
        for e in w.ents.iter_mut().rev() {
            let a = match &mut e.body {
                wf_core::world::Body::Npc(n) => &mut n.a,
                wf_core::world::Body::Enemy(n) => &mut n.a,
                _ => continue,
            };
            let Some(def) = a.sprite.map(|s| s + 9) else { continue };
            if let Some(o) = objs.iter_mut().rev().find(|o| o.is_some_and(|o| i(&o[0]) as i32 == def)).and_then(|o| o.take()) {
                a.x = (i(&o[1]) as i32) << 8;
                a.y = (i(&o[2]) as i32) << 8;
                a.facing = i(&o[3]) as u8;
            }
        }
        if let Some(c) = sj["cam"].as_array().filter(|c| c.len() == 2) {
            let (x, y) = (i(&c[0]) as i32, i(&c[1]) as i32);
            let cam = &mut w.cameras[0];
            cam.x = x << 10;
            cam.y = y << 10;
            cam.rect = wf_core::geom::Rect::new(x, y, x + 240, y + 160);
            cam.shown = cam.rect;
        }
    }
    let rows = frames["frames"].as_array()?;
    // script-opened screens: the remake opens the screen the frame the emulator's script did
    // the save slots (the game's EEPROM cache, title scenarios): `meta.saves[k]` = a story dict + location
    if let Some(sv) = meta["saves"].as_array() {
        for (k, s) in sv.iter().enumerate() {
            let d = (!s.is_null()).then(|| wf_core::menu::save::SaveData {
                story: story_from(data, s),
                world: i(&s["world"]) as i32,
                area: i(&s["area"]) as i32,
                variant: i(&s["variant"]) as i32,
                x: i(&s["x"]) as i32,
                y: i(&s["y"]) as i32,
                flag: s["flag"].as_bool().unwrap_or(false),
            });
            sim.session.world.saves.set(k, d);
        }
        sim.session.world.saves.generation = 0;
    }
    let mut r = Renderer::new();
    let log = std::env::var("MENU_PARITY_LOG").is_ok();
    let out = std::env::var("MENU_PARITY_OUT").ok().map(PathBuf::from);
    let (mut compared, mut exact) = (0, 0);
    let mut snd_bad = Vec::new();
    let mut bad_frames = Vec::new();
    let mut ev = Vec::new();
    let mut menu_seen = false;
    // the app's pacing (`Options::no_lag`): one step per game logic iteration - the remake steps on the rows where
    // the emulator's mode-loop counter moved and is compared at each iteration's last video frame (the picture the
    // long iteration ends with); the rows in between are the hardware's lag frames (a redraw's / a close's CPU time)
    sim.session.world.opts.no_lag = true;
    let lf: Vec<i64> = rows.iter().map(|r| i(&r["lf"])).collect();
    let hs: Vec<String> = rows.iter().map(|r| r["h"].as_str().unwrap_or("").to_string()).collect();
    let mut tears = 0;
    let mut iters = 0usize;
    let mut emu_snd: Vec<i64> = Vec::new();
    // the previous compared row and our picture's hash there
    let mut prev_cmp: Option<(usize, String)> = None;
    for (k, row) in rows.iter().enumerate() {
        let keys = i(&row["keys"]) as u16;
        let logic = k == 0 || lf[k] != lf[k - 1];
        let complete = k + 1 == lf.len() || lf[k + 1] != lf[k];
        if logic {
            emu_snd.clear();
        }
        emu_snd.extend(row["snd"].as_array().map(|a| a.iter().filter(|s| s[0] == "play").map(|s| i(&s[1])).collect::<Vec<_>>()).unwrap_or_default());
        if !logic && !complete {
            continue;
        }
        if logic {
            iters += 1;
            ev.clear();
        }
        if logic && k == 0 && meta["open"].is_object() {
            if !wf_core::menu::open_from_json(&mut sim.session.world, &meta["open"]) {
                // the screen is not implemented (yet): nothing to compare (a STRICT scenario then fails: 0 frames)
                println!("menu {name}: no screen opens {}; skipped", meta["open"]);
                return Some((0, 0, vec!["not openable".into()], Vec::new()));
            }
        }
        // (a script-opened screen: the opening is the script's iteration, row 0 - its fade starts with the next one,
        // as START's open in the field)
        let opened_fade = k == 0 && meta["open"].is_object() && matches!(sim.world().menu.phase, wf_core::menu::Phase::FadeOut { .. });
        if logic && !opened_fade {
            sim.step(&[keys], &mut ev);
            // a screen whose fade-out runs the world (the computer's): the field around it is only approximately
            // imported (NPC timers), so the RNG follows the game's while it fades (captures with per-row `rng`)
            let w = &mut sim.session.world;
            if live && w.menu.live_fade && matches!(w.menu.phase, wf_core::menu::Phase::FadeOut { .. }) {
                if let Some(r) = row["rng"].as_array().filter(|r| r.len() == 2) {
                    w.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
                }
            }
        }
        // the RNG once the screen is open (the field frame before may draw differently)
        if live && k == 0 {
            if let Some(r) = sj["rng"].as_array().filter(|r| r.len() == 2) {
                sim.session.world.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
            }
        }
        if !complete {
            continue;
        }
        if std::env::var("MENU_PARITY_STORY").is_ok_and(|v| v.split(',').any(|x| x.parse::<usize>().ok() == Some(k))) {
            let st = &sim.world().story;
            eprintln!("{name} {k}: equipment {:?} equip {:?} supplies {:?}", st.equipment, st.chars.iter().map(|c| c.equip).collect::<Vec<_>>(), st.supplies.iter().enumerate().filter(|(_, n)| **n > 0).collect::<Vec<_>>());
        }
        // sounds of menu frames (and the START frame): the emulator's play calls vs ours
        let mut v = RenderView::default();
        sim.view(&mut v);
        let in_menu = wf_core::menu::active(sim.world()) || k == 0;
        if in_menu {
            menu_seen = true;
            // (the iteration's sounds: its lag frames' play calls belong to it)
            let mut emu = emu_snd.clone();
            emu.sort();
            let mut ours: Vec<i64> = ev.iter().filter_map(|e| if let SimEvent::Audio(AudioEvent::PlaySfx { id, .. }) = e { Some(*id as i64) } else { None }).collect();
            ours.sort();
            if emu != ours {
                snd_bad.push(format!("{name} frame {k}: emulator {emu:?} ours {ours:?}"));
            }
        }
        if v.menu.is_none() {
            if log {
                eprintln!("{name} {k:4} keys {keys:03x} field ({:?})", sim.world().menu.phase);
            }
            continue;
        }
        // (overlay screens animate every frame: no tear rule for them)
        // a torn picture needs a VRAM copy still running at the scanout: only around a long iteration (frames
        // without a mode-loop tick); screens redrawn every frame (overlays, credits) never tear
        let long_iter = lf[k] == lf[k - 1.min(k)] || (k + 1 < lf.len() && lf[k + 1] == lf[k]);
        if k > 0 && tear(&hs, k) && (long_iter || split_tear(&d, k)) && !v.menu.as_ref().is_some_and(|m| m.overlay) {
            tears += 1;
            if log {
                eprintln!("{name} {k:4} keys {keys:03x} tear frame (VRAM copy during the scanout), not compared");
            }
            continue;
        }
        if let Some(mv) = v.menu.as_ref().filter(|m| m.overlay) {
            // a screen over the frozen field (mini-games): only its own pixels are compared with the capture's
            // picture (the field under it is the field renderer's, tests/field_parity.rs); needs work/ PNGs
            let mut mr = wf_render::menu::MenuRenderer::load(packs);
            let pic = mr.render(packs, mv, None, 0);
            let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) else { continue };
            compared += 1;
            let diff = pic.iter().zip(&emu).filter(|(o, e)| **o >> 24 != 0 && (**o & 0xFF_FFFF) != (**e & 0xFF_FFFF)).count();
            if diff == 0 {
                exact += 1;
            } else {
                bad_frames.push(k);
                if let Some(o) = &out {
                    std::fs::create_dir_all(o).ok();
                    let ours = r.render(packs, &v, 0, 240, 160).to_vec();
                    let dm: Vec<u32> = pic.iter().zip(&emu).map(|(a, b)| if *a >> 24 == 0 || (*a & 0xFF_FFFF) == (*b & 0xFF_FFFF) { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                    save(&o.join(format!("{name}_{k:04}.png")), &[&emu, &ours, &dm]);
                }
            }
            if log {
                eprintln!("{name} {k:4} keys {keys:03x} {:?} overlay diff {diff}", sim.world().menu.phase);
            }
            continue;
        }
        let ours = r.render(packs, &v, 0, 240, 160).to_vec();
        compared += 1;
        let oh = hash(&ours);
        // a long iteration (more than one video frame) whose picture the emulator has not presented by its last
        // frame - unchanged since the previous iteration's end - while the remake (no lag) shows its draw at once:
        // the previous picture is the game's here (the status pages' R: the tab highlight is drawn but never shown)
        let long = k > 0 && lf[k] == lf[k - 1];
        let stale = long && prev_cmp.as_ref().is_some_and(|(pk, ph): &(usize, String)| hs[*pk] == hs[k] && *ph == hs[k]);
        // and the other side of the same thing: a step whose redraw is the next iteration's update, a long one - the
        // game presents this step's picture at that iteration's end (title_delete lf 975 / 976: the confirm's redraw)
        let next_end = {
            let mut e = k + 1;
            while e + 1 < lf.len() && lf[e + 1] == lf[k + 1] {
                e += 1;
            }
            e
        };
        let deferred = next_end < lf.len() && next_end > k + 1 && hs[next_end] == oh;
        let same = oh == hs[k] || stale || deferred;
        prev_cmp = Some((k, oh));
        let mut diff = 0;
        if same {
            exact += 1;
        } else {
            bad_frames.push(k);
            if let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) {
                diff = emu.iter().zip(&ours).filter(|(a, b)| a != b).count();
                if let Some(o) = &out {
                    std::fs::create_dir_all(o).ok();
                    let dm: Vec<u32> = emu.iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                    save(&o.join(format!("{name}_{k:04}.png")), &[&emu, &ours, &dm]);
                }
            }
        }
        if log {
            let m = &sim.world().menu;
            eprintln!("{name} {k:4} keys {keys:03x} {:?} diff {diff} lag {} top-focus {:?} shown-focus {:?} pending {} fade {:?}", m.phase, m.lag, m.stack.last().map(|s| s.container_ref().focus), m.shown.as_ref().map(|s| s.container_ref().focus), m.pending.is_some(), v.menu.as_ref().and_then(|x| x.fade).map(|f| f.level));
        }
    }
    let _ = menu_seen;
    // natives that push the screen's result (mini-games: code - 1): the capture's push
    if let Some(want) = meta.get("native_push").and_then(|x| x.as_i64()) {
        let got = sim.world().menu.result.as_ref().map(|r| r.code as i64 - 1);
        if got != Some(want) {
            snd_bad.push(format!("{name}: native push {want}, ours {got:?}"));
        }
    }
    println!("menu {name}: {iters} logic iterations; {tears} tear frames skipped");
    Some((compared, exact, snd_bad, bad_frames))
}

#[test]
fn menus_match_emulator() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    if !traces().exists() {
        return;
    }
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data: Arc<wf_core::content::GameData> = wf_app::battle::load_data(&packs).expect("data");
    let mut names: Vec<String> = std::fs::read_dir(traces())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(String::from))
        .collect();
    names.sort();
    if let Ok(only) = std::env::var("MENU_PARITY_ONLY") {
        names.retain(|n| n.contains(&only));
    }
    let mut failed = Vec::new();
    let strict_all = std::env::var("MENU_PARITY_STRICT").is_ok();
    for n in &names {
        let Some((cmp, ex, snd, bad)) = run(&packs, &data, n) else { continue };
        println!("menu {n}: {ex} / {cmp} frames exact; sound mismatches {}; differing frames {:?}", snd.len(), &bad[..bad.len().min(40)]);
        for s in snd.iter().take(10) {
            println!("  {s}");
        }
        let strict = strict_all || STRICT.iter().any(|p| n.starts_with(p));
        if strict && (ex != cmp || !snd.is_empty()) {
            failed.push(n.clone());
        }
    }
    assert!(failed.is_empty(), "menu scenarios not exact: {failed:?}");
}
