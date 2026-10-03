//! Menu screens vs the emulator, tick by tick and frame by frame (headless, muted; skipped without the ROM).
//!
//! Traces: `<repo>/data/menu/<scenario>.json` (`re/buu/menu/capture.py`: keys, the mode loop's frame counter,
//! sound calls and the sha1 of the emulator's screen per video frame, the GameData block decoded at frame 0 and how
//! the screen opens). The remake's story session gets that state, then both run the same keys. Compared per video
//! frame: the sounds and, while the remake shows a menu screen, the sha1 of the 240x160 picture (wf-core menu view
//! -> wf-render). CPU time is an oracle from the trace (see `run`). Pictures for diffs: `work/buu/menu/<scenario>/`
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
            };
        }
    }
    st
}

/// a picture differing from both neighbours: the VRAM copy raced the scanout (sub-frame CPU timing)
fn tear(hs: &[String], k: usize) -> bool {
    k > 0 && k + 1 < hs.len() && hs[k] != hs[k - 1] && hs[k] != hs[k + 1] && hs[k - 1] != hs[k + 1]
}

fn dir() -> PathBuf {
    std::env::var("WF_MENU_DIR").map(PathBuf::from).unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/buu/menu"))
}

fn traces() -> PathBuf {
    wf_rom::testutil::repo_root().join("data/menu")
}

/// scenarios that must be exact (prefixes)
const STRICT: &[&str] = &["shop_", "save_", "title_", "charsel_", "status_", "inventory_", "equip_", "cr_", "mg_", "options_", "journal_", "skills_", "cr_", "map_snake", "map_forest", "map_left", "map_scouter"];

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
    let oracle = std::env::var("MENU_PARITY_FREE").is_err();
    let lf: Vec<i64> = rows.iter().map(|r| i(&r["lf"])).collect();
    let hs: Vec<String> = rows.iter().map(|r| r["h"].as_str().unwrap_or("").to_string()).collect();
    let mut tears = 0;
    let mut lag_pred = (0usize, 0usize);
    for (k, row) in rows.iter().enumerate() {
        let keys = i(&row["keys"]) as u16;
        // CPU-time oracle: the emulator's mode loop ran a logic iteration in frame k iff its frame counter moved;
        // the remake's lag (frames a long redraw takes) is forced to the emulator's (see docs: the sound driver's
        // IRQ load makes it unpredictable to the frame); MENU_PARITY_FREE=1 runs the remake's own model
        if k > 0 && sim.world().menu.phase == wf_core::menu::Phase::Run {
            let ticks = lf[k] > lf[k - 1];
            let mut quiet = 0u8;
            while k + (quiet as usize) < lf.len() && k + (quiet as usize) > 0 && lf[k + quiet as usize] == lf[k + quiet as usize - 1] {
                quiet += 1;
            }
            let model = sim.world().menu.lag;
            if ticks == (model == 0) && (ticks || model == quiet) {
                lag_pred.0 += 1;
            }
            lag_pred.1 += 1;
            if oracle {
                let m = &mut sim.session.world.menu;
                // the display: a finished redraw shows from the frame the emulator's picture changes (a frame whose
                // picture differs from both neighbours is a VRAM copy racing the scanout: shown from the next one)
                let changed = hs[k] != hs[k - 1] && !tear(&hs, k);
                if changed {
                    // an intermediate picture of the same iteration first (`Menu.pre`)
                    if let Some(p) = m.pre.take() {
                        m.shown = Some(p);
                    } else if let Some(p) = m.pending.take() {
                        m.shown = Some(p);
                    }
                }
                // the logic: the emulator's mode loop iterations
                m.lag = if ticks { 0 } else { quiet.saturating_add(1) };
            }
        }
        // the world restore's length is CPU time too: the fade-in starts the frame the emulator's counter moves again
        if oracle && k > 0 {
            if let wf_core::menu::Phase::Leave { t } = sim.world().menu.phase {
                let leave = sim.world().menu.tr.leave;
                if t >= 1 && lf[k] > lf[k - 1] && t + 1 < leave {
                    sim.session.world.menu.phase = wf_core::menu::Phase::Leave { t: leave - 1 };
                }
            }
        }
        ev.clear();
        if k == 0 && meta["open"].is_object() {
            if !wf_core::menu::open_from_json(&mut sim.session.world, &meta["open"]) {
                // the screen is not implemented (yet): nothing to compare (a STRICT scenario then fails: 0 frames)
                println!("menu {name}: no screen opens {}; skipped", meta["open"]);
                return Some((0, 0, vec!["not openable".into()], Vec::new()));
            }
        }
        sim.step(&[keys], &mut ev);
        // the RNG once the screen is open (the field frame before may draw differently)
        if live && k == 0 {
            if let Some(r) = sj["rng"].as_array().filter(|r| r.len() == 2) {
                sim.session.world.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
            }
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
            let emu: Vec<i64> = row["snd"].as_array().map(|a| a.iter().filter(|s| s[0] == "play").map(|s| i(&s[1])).collect()).unwrap_or_default();
            let ours: Vec<i64> = ev.iter().filter_map(|e| if let SimEvent::Audio(AudioEvent::PlaySfx { id, .. }) = e { Some(*id as i64) } else { None }).collect();
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
        if k > 0 && tear(&hs, k) && long_iter && !v.menu.as_ref().is_some_and(|m| m.overlay) {
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
        let same = hash(&ours) == hs[k];
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
    println!("menu {name}: lag model agrees on {} / {} menu frames; {tears} tear frames skipped", lag_pred.0, lag_pred.1);
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
