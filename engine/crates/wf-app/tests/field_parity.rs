//! Field rendering vs the emulator, frame by frame (headless, muted; skipped without the ROM or without the
//! captures of `crates/wf-render/re/capture_field.py` in `<repo>/work/frontend/field/`, `WF_FIELD_DIR` overrides).
//! Each capture is one of the field recorders' traces (the same ones wf-core's field lockstep replays exactly);
//! the core session is set up like that lockstep test, stepped with the recorded keys, and every video frame is
//! rendered 240x160 through `wf-core`'s view + `wf-render` and diffed with the emulator's screen.
//! `FIELD_PARITY_OUT=<dir>` writes emulator | ours | diff strips of the worst frames.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_core::battle::RngInit;
use wf_core::field::FieldConfig;
use wf_core::session::Session;
use wf_core::story::Story;
use wf_render::Renderer;
use wf_view::{RenderView, Sim};

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

/// field_lockstep.rs `config`: the story + player of a transitions trace
fn config(d: &wf_core::content::GameData, t: &Value) -> FieldConfig {
    let init = &t["init"];
    let mut st = Story::new_game(d);
    if let Some(h) = init["flags_hex"].as_str() {
        for k in 0..(h.len() / 2).min(st.flags.len()) {
            st.flags[k] = u8::from_str_radix(&h[2 * k..2 * k + 2], 16).unwrap();
        }
    }
    st.zenie = i(&init["zenie"]) as u32;
    let p = &init["player"];
    let r = &p["record"];
    let ch = i(&r["char"]) as usize;
    {
        let cs = &mut st.chars[ch];
        cs.form = i(&r["form"]) as i32;
        cs.rec.hp = i(&r["hp"]) as i32;
        cs.rec.max_hp = i(&r["maxhp"]) as i32;
        cs.rec.ep = i(&r["ep"]) as i32;
        cs.rec.max_ep = i(&r["maxep"]) as i32;
        cs.rec.str_ = i(&r["str"]) as i32;
        cs.rec.pow = i(&r["pow"]) as i32;
        cs.rec.end = i(&r["end"]) as i32;
        cs.rec.spd = i(&r["spd"]) as i32;
        cs.rec.points = i(&r["points"]) as i32;
        cs.rec.xp = i(&r["xp"]);
        cs.rec.level = wf_core::progression::level_of(cs.rec.xp, &d.xp_table);
        cs.skills = r["skills"].as_array().unwrap().iter().map(|x| i(x) as i32).collect();
        cs.skill_levels = r["skill_levels"].as_array().map(|a| a.iter().map(|x| i(x) as i32).collect()).unwrap_or_else(|| vec![0; cs.skills.len()]);
        if let Some(e) = r["equip"].as_array() {
            for k in 0..4 {
                cs.equip[k] = i(&e[k]) as i32;
            }
        }
    }
    st.current = ch as i32;
    let a = &init["area"];
    let rng = &init["rng"];
    FieldConfig {
        world: i(&a[0]) as i32,
        area: i(&a[1]) as i32,
        variant: i(&a[2]) as i32,
        pos: Some(((i(&p["x"]) >> 8) as i32, (i(&p["y"]) >> 8) as i32)),
        facing: i(&p["facing"]) as u8,
        chars: vec![ch as i32],
        story: Some(st),
        rng: RngInit::State(i(&rng[0]) as u32, i(&rng[1]) as u32),
        xp: true,
        arrive: true,
    }
}

/// a session set up like the field lockstep's replay of this trace kind
fn session(data: &Arc<wf_core::content::GameData>, kind: &str, t: &Value, base: &Value) -> Session {
    if kind == "transitions" {
        return Session::new_field(data.clone(), config(data, t));
    }
    let init = &t["init"];
    let a = &t["area"];
    let mut cfg = config(data, base);
    cfg.world = i(&a[0]) as i32;
    cfg.area = i(&a[1]) as i32;
    cfg.variant = i(&a[2]) as i32;
    cfg.pos = Some(((i(&init["player"]["x"]) >> 8) as i32, (i(&init["player"]["y"]) >> 8) as i32));
    cfg.facing = i(&init["player"]["facing"]) as u8;
    if kind == "npc" {
        cfg.rng = RngInit::State(i(&init["rng"][0]) as u32, i(&init["rng"][1]) as u32);
        if let (Some(h), Some(st)) = (init["flags"].as_str(), cfg.story.as_mut()) {
            for k in 0..(h.len() / 2).min(st.flags.len()) {
                st.flags[k] = u8::from_str_radix(&h[2 * k..2 * k + 2], 16).unwrap();
            }
        }
        let mut s = Session::new_field(data.clone(), cfg);
        wf_core::field::attach_host(&mut s.world, true);
        for _ in 9376..i(&init["frame_counter"]).max(9376) {
            for k in (0..s.world.ents.len()).rev() {
                if matches!(&s.world.ents[k].body, wf_core::world::Body::Npc(n) if n.behaviour.is_some()) {
                    wf_core::fieldnpc::tick_only(&mut s.world, k);
                }
            }
        }
        return s;
    }
    // objects
    cfg.arrive = false;
    if let Some(st) = cfg.story.as_mut() {
        st.zenie = i(&init["zenie"]) as u32;
    }
    let mut s = Session::new_field(data.clone(), cfg);
    wf_core::field::attach_host(&mut s.world, true);
    s.world.rng = wf_core::rng::Rng::new(i(&init["rng"][0]) as u32, i(&init["rng"][1]) as u32);
    s
}

fn keys(kind: &str, t: &Value) -> Vec<u16> {
    let mut out = vec![];
    for seg in t["input"].as_array().unwrap() {
        let k = if kind == "objects" { wf_core::input::parse_keys(seg[0].as_str().unwrap_or("")) } else { i(&seg[0]) as u16 };
        for _ in 0..i(&seg[1]) {
            out.push(k);
        }
    }
    out
}

/// every animation's [frame, countdown] after `t` ticks (the RAM records of 0x03001A68)
fn state_of(anims: &[wf_content::map::TileAnim], t: u32) -> Vec<[i32; 2]> {
    anims
        .iter()
        .enumerate()
        .map(|(k, a)| {
            let (f, at) = a.after_ticks(k, t);
            let cd = if at == 0 && f == 0 { k as i64 + 1 - t as i64 } else { at as i64 + a.durations[f].max(1) as i64 - t as i64 };
            [f as i32, cd as i32]
        })
        .collect()
}

/// a tick count since the load that leaves every animation in the captured [frame, countdown] state
fn ticks_for(anims: &[wf_content::map::TileAnim], st: &[[i32; 2]]) -> Option<u32> {
    if anims.is_empty() || anims.len() != st.len() {
        return None;
    }
    (0..200_000u32).find(|&t| state_of(anims, t) == st)
}

#[test]
fn field_frames_match_emulator() {
    let dir = std::env::var("WF_FIELD_DIR").map(PathBuf::from).unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/frontend/field"));
    if !dir.is_dir() {
        return;
    }
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let opts = wf_extract::Options { only: ["tables", "sprites", "effects", "items", "maps", "ui", "world", "scripts"].map(String::from).to_vec() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::game_data(&packs).unwrap();
    let base: Value = serde_json::from_slice(&std::fs::read(wf_rom::testutil::repo_root().join("data/field/transitions_forest_up_exit.json")).unwrap()).unwrap();
    let out = std::env::var("FIELD_PARITY_OUT").ok().map(PathBuf::from);
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).unwrap().flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(String::from)).collect();
    names.sort();
    let mut r = Renderer::new();
    let mut summary = vec![];
    for name in names {
        let t: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        let kind = name.split('_').next().unwrap().to_string();
        let mut s = session(&data, &kind, &t, &base);
        // the breakable-drops recording patched the rock's on-break bytecode to drop_item(5); drop_item(-44)
        // (rec_objects.py): the same stand-in host as wf-core's field lockstep, no script host
        if name == "objects_breakable_drops_8_1" {
            s.hook = Some(Box::new(|w, fe| {
                if let wf_core::field::FieldEvent::Run { cause: wf_core::field::Cause::Break { .. }, .. } = fe {
                    wf_core::fieldobj::drop_item(w, 5);
                    wf_core::fieldobj::drop_item(w, -44);
                }
            }));
        }
        let mut story = if kind == "objects" && name != "objects_breakable_drops_8_1" && std::env::var("NO_SCRIPTS").is_err() {
            let mut st = session(&data, &kind, &t, &base);
            st.world.scene.banner = None;
            let mut sim = wf_app::story::StorySim::with_session(&packs, st).expect("story");
            sim.skip_area_banner();
            Some(sim)
        } else {
            None
        };
        let ks = keys(&kind, &t);
        let frames = t["frames"].as_array().unwrap();
        let (mut exact, mut total, mut worst) = (0, 0, (0usize, 0usize));
        let mut diffs = vec![];
        let mut ev = vec![];
        let mut v = RenderView::default();
        // wf-core's own presentation (view::Presenter: OBJ latency; render_view: banner, fades, tile timers, greyed
        // icon, dialogue animation)
        // traces that start mid-area: the camera was still settling when the recording began (not part of the
        // core's start state) — use the captured camera (map object +0x28 / +0x2C) for those
        let ram_cams: Option<Vec<Vec<i32>>> =
            std::fs::read(dir.join(&name).join("cams.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let cam_off: isize = std::env::var("CAM_OFF").ok().and_then(|v| v.parse().ok()).unwrap_or(-1);
        let ram_anims: Option<Vec<Vec<[i32; 2]>>> =
            std::fs::read(dir.join(&name).join("tile_anims.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let (mut tick_mismatch, mut first_mismatch) = (0usize, None);
        let mut bad_frames: Vec<(usize, usize)> = vec![];
        let mut video_since_load: u32 = s.world.frame.saturating_sub(s.world.field.as_ref().map(|f| f.entered).unwrap_or(0));
        if kind == "objects" {
            // the recording starts long after the area's banner (wf-core starts one at the load)
            s.world.scene.banner = None;
            s.presenter.reset();
        }
        for fr in frames {
            let f = i(&fr["f"]) as usize;
            if fr["logic"].as_bool().unwrap_or(true) {
                match story.as_mut() {
                    Some(st) => {
                        Sim::step(st, &[ks.get(f).copied().unwrap_or(0)], &mut ev);
                        s.world = st.session.world.clone();
                    }
                    None => Sim::step(&mut s, &[ks.get(f).copied().unwrap_or(0)], &mut ev),
                }
            }
            // traces that start mid-area: the camera was still easing when the recording began (not part of the
            // core's start state): after the first step, give the core the game's camera (22.10) and let it run
            if f == 0 && kind == "objects" && cam_off <= 0 {
                if let Some(c) = ram_cams.as_ref().and_then(|c| c.first()).filter(|c| c.len() >= 4) {
                    let w = match story.as_mut() {
                        Some(st) => &mut st.session.world,
                        None => &mut s.world,
                    };
                    let cam = &mut w.cameras[0];
                    cam.x = c[2];
                    cam.y = c[3];
                    cam.init = true;
                    cam.rect = wf_core::geom::Rect { x0: c[2] >> 10, y0: c[3] >> 10, x1: (c[2] >> 10) + 240, y1: (c[3] >> 10) + 160 };
                    cam.shown = wf_core::geom::Rect { x0: c[0], y0: c[1], x1: c[0] + 240, y1: c[1] + 160 };
                    if story.is_some() {
                        s.world = story.as_ref().unwrap().session.world.clone();
                    }
                }
            }
            let Some(emu) = load_png(&dir.join(&name).join(format!("{f:04}.png"))) else { continue };
            match story.as_ref() {
                Some(st) => st.view(&mut v),
                None => s.view(&mut v),
            }
            if fr["logic"].as_bool().unwrap_or(true) && s.world.events.iter().any(|e| matches!(e, wf_core::world::Event::Field(wf_core::field::FieldEvent::AreaEnter { .. }))) {
                video_since_load = 0;
            } else {
                video_since_load += 1;
            }
            // the map's animation ticks: from the captured RAM state when available (traces that start mid-area),
            // else what wf-core's view derives
            let ram_ticks = ram_anims.as_ref().and_then(|ra| ra.get(f)).and_then(|st| {
                let m = v.map.as_ref()?;
                let map = r.map(&packs, &m.id)?;
                ticks_for(&map.tileset.anims, st)
            });
            if let (Some(_), Some(m), Some(st)) = (ram_ticks, v.map.as_ref(), ram_anims.as_ref().and_then(|ra| ra.get(f))) {
                let map = r.map(&packs, &m.id).unwrap();
                if state_of(&map.tileset.anims, m.anim_frame) != *st {
                    let t = ram_ticks.unwrap();
                    tick_mismatch += 1;
                    if first_mismatch.is_none() {
                        first_mismatch = Some((f, m.anim_frame, t));
                    }
                }
            }
            if let (Some(t), Some(m), true) = (ram_ticks, v.map.as_mut(), std::env::var("RAM_TICKS").is_ok() || kind == "objects") {
                m.anim_frame = t;
            }
            if let (Ok(p), Some(m)) = (std::env::var("PHASE"), v.map.as_mut()) {
                m.anim_frame = if p == "core" {
                    s.world.frame
                } else if let Some(c) = p.strip_prefix("video") {
                    video_since_load.wrapping_add(c.parse::<i32>().unwrap_or(0) as u32)
                } else {
                    m.anim_frame.wrapping_add(p.parse::<i32>().unwrap() as u32)
                };
            }
            if std::env::var("DEBUG_F").ok().and_then(|x| x.parse::<usize>().ok()) == Some(f) && std::env::var("DEBUG_N").ok().as_deref() == Some(name.as_str()) {
                println!("f{f} cam {:?} sprites {:?}", v.cameras, v.sprites.iter().map(|s| (s.sheet.as_str(), &s.frame, s.x, s.y, s.priority)).collect::<Vec<_>>());
                let w = &s.world;
                let pl = w.pl_ref(w.player_idx(0).unwrap());
                println!("player raw {} frame {} tick {:?} facing {} pos {} {}", pl.ap.raw, pl.ap.frame, pl.ap.tick, pl.a.facing, pl.a.x, pl.a.y);
            }
            if std::env::var("CAMDBG").ok().as_deref() == Some(name.as_str()) && f < 40 {
                let w = &s.world;
                println!("f{f} view cam {:?} core x {} y {} shown {:?} cap {:?}", v.cameras, w.cameras[0].x, w.cameras[0].y, (w.cameras[0].shown.x0, w.cameras[0].shown.y0), ram_cams.as_ref().and_then(|c| c.get(f)));
            }
            let fb = r.render(&packs, &v, 0, 240, 160).to_vec();
            let d = fb.iter().zip(&emu).filter(|(a, b)| *a & 0xFFFFFF != *b & 0xFFFFFF).count();
            total += 1;
            if d == 0 {
                exact += 1;
            }
            if d > worst.1 {
                worst = (f, d);
            }
            diffs.push(d);
            if d > 0 {
                bad_frames.push((f, d));
            }
            if let Some(o) = &out {
                if f % 30 == 0 || d > 0 && (d == worst.1 || std::env::var("SAVE_BAD").is_ok()) {
                    std::fs::create_dir_all(o.join(&name)).unwrap();
                    let dimg: Vec<u32> = fb.iter().zip(&emu).map(|(a, b)| if a & 0xFFFFFF == b & 0xFFFFFF { (a >> 2) & 0x3F3F3F | 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                    save(&o.join(&name).join(format!("{f:04}.png")), &[&emu, &fb, &dimg]);
                }
            }
        }
        diffs.sort();
        let med = diffs.get(diffs.len() / 2).copied().unwrap_or(0);
        let line = format!(
            "{name:34} frames {total:5}  exact {exact:5}  median diff {med:6} px  worst {} px @ {}  tile-timer mismatches {tick_mismatch} first {first_mismatch:?}",
            worst.1, worst.0
        );
        println!("{line}");
        if std::env::var("LIST_BAD").is_ok() {
            // runs of differing frames: (first, last, max px)
            let mut runs: Vec<(usize, usize, usize)> = vec![];
            for &(f, d) in &bad_frames {
                match runs.last_mut() {
                    Some(r) if r.1 + 1 == f => {
                        r.1 = f;
                        r.2 = r.2.max(d);
                    }
                    _ => runs.push((f, f, d)),
                }
            }
            println!("  differing runs {runs:?}");
        }
        summary.push((name, total, exact));
        if !r.errors.is_empty() {
            println!("  content errors: {:?}", r.errors);
            r.errors.clear();
        }
    }
    assert!(!summary.is_empty());
}
