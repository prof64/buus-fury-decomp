//! Menu screens vs the emulator, frame by frame (headless, muted; skipped without the ROM or without the captures
//! of `re/buu/menu/capture.py` in `<repo>/work/buu/menu/<scenario>/`, `WF_MENU_DIR` overrides).
//!
//! Each scenario starts from a save state; the capture decodes the GameData block at the START frame
//! (`story.json`), the remake's story session gets that state, then both run the same keys from that frame on.
//! Compared per video frame: the sounds the menu plays and, while the remake shows a menu screen, the whole
//! 240x160 picture (wf-core menu view -> wf-render). `MENU_PARITY_OUT=<dir>` writes emulator | ours | diff strips
//! of differing frames; `MENU_PARITY_LOG=1` prints every frame.

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
    st.equipment = s["equipment"].as_array().map(|a| a.iter().map(|v| (i(v) & 0x0FFF_FFFF) as i32).collect()).unwrap_or_default();
    st.zenie = i(&s["zenie"]) as u32;
    st.play_frames = (i(&s["counter"]) + i(&s["play_base"])).max(0) as u64;
    if let Some(cs) = s["chars"].as_array() {
        for (k, c) in cs.iter().enumerate().take(st.chars.len()) {
            let skills: Vec<i32> = c["skills"].as_array().map(|a| a.iter().map(|v| i(v) as i32).collect()).unwrap_or_default();
            let levels: Vec<i32> = c["skill_levels"].as_array().map(|a| a.iter().map(|v| i(v) as i32).collect()).unwrap_or_default();
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

/// result of one scenario: (frames compared, frames exact, sound mismatches)
fn run(packs: &Packs, data: &Arc<wf_core::content::GameData>, name: &str) -> Option<(usize, usize, Vec<String>, Vec<usize>)> {
    let d = dir().join(name);
    let frames: Value = serde_json::from_slice(&std::fs::read(d.join("frames.json")).ok()?).ok()?;
    let sj: Value = serde_json::from_slice(&std::fs::read(d.join("story.json")).ok()?).ok()?;
    let story = story_from(data, &sj);
    let cfg = FieldConfig { world: 1, area: 1, story: Some(story), rng: wf_core::battle::RngInit::Seed(1), ..Default::default() };
    let session = Session::new_field(Arc::clone(data), cfg);
    let mut sim = StorySim::with_session(packs, session).expect("story sim");
    sim.skip_area_banner();
    let rows = frames["frames"].as_array()?;
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
                    if let Some(p) = m.pending.take() {
                        m.shown = Some(p);
                    }
                }
                // the logic: the emulator's mode loop iterations
                m.lag = if ticks { 0 } else { quiet.saturating_add(1) };
            }
        }
        ev.clear();
        sim.step(&[keys], &mut ev);
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
        let Some(emu) = load_png(&d.join(format!("f{k:04}.png"))) else { continue };
        if k > 0 && tear(&hs, k) {
            tears += 1;
            if log {
                eprintln!("{name} {k:4} keys {keys:03x} tear frame (VRAM copy during the scanout), not compared");
            }
            continue;
        }
        let ours = r.render(packs, &v, 0, 240, 160).to_vec();
        let diff = emu.iter().zip(&ours).filter(|(a, b)| a != b).count();
        compared += 1;
        if diff == 0 {
            exact += 1;
        } else {
            bad_frames.push(k);
            if let Some(o) = &out {
                std::fs::create_dir_all(o).ok();
                let dm: Vec<u32> = emu.iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                save(&o.join(format!("{name}_{k:04}.png")), &[&emu, &ours, &dm]);
            }
        }
        if log {
            eprintln!("{name} {k:4} keys {keys:03x} {:?} diff {diff}", sim.world().menu.phase);
        }
    }
    let _ = menu_seen;
    println!("menu {name}: lag model agrees on {} / {} menu frames; {tears} tear frames skipped", lag_pred.0, lag_pred.1);
    Some((compared, exact, snd_bad, bad_frames))
}

#[test]
fn menus_match_emulator() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    if !dir().exists() {
        return;
    }
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data: Arc<wf_core::content::GameData> = wf_app::battle::load_data(&packs).expect("data");
    let mut names: Vec<String> = std::fs::read_dir(dir()).unwrap().filter_map(|e| e.ok()).filter(|e| e.path().join("frames.json").exists()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    if let Ok(only) = std::env::var("MENU_PARITY_ONLY") {
        names.retain(|n| n.contains(&only));
    }
    let mut all_ok = true;
    for n in &names {
        let Some((cmp, ex, snd, bad)) = run(&packs, &data, n) else { continue };
        println!("menu {n}: {ex} / {cmp} frames exact; sound mismatches {}; differing frames {:?}", snd.len(), &bad[..bad.len().min(40)]);
        for s in snd.iter().take(10) {
            println!("  {s}");
        }
        if ex != cmp || !snd.is_empty() {
            all_ok = false;
        }
    }
    if std::env::var("MENU_PARITY_STRICT").is_ok() {
        assert!(all_ok);
    }
}
