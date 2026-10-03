//! Replays, rewind, save states, take-over and tool overlays on the real battle (wf-core), headless and muted.
//! Content: the user's ROM (roms/buu.gba) extracted once into the cargo target dir. Prints size / perf numbers
//! (`cargo test -p wf-app --release --test replay -- --nocapture`).

use std::path::PathBuf;
use std::time::Instant;

use wf_app::control::{Control, Request};
use wf_app::hotkeys::{Action, Hotkeys};
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_app::wf_replay::{lz, Replay, SaveState};
use wf_app::App;
use wf_view::{buttons as b, RenderView, SpriteEffect};

fn content_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("WF_CONTENT") {
        return Some(PathBuf::from(d));
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-replay-content");
    let only = ["tables", "sprites", "effects", "items", "maps", "ui"];
    // the marker records the extractor's source hash: an extractor change re-extracts (wf_extract::cache)
    if wf_extract::cache::fresh(&dir, &only) {
        return Some(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let rom = wf_rom::testutil::rom()?;
    let opts = wf_extract::Options { only: ["tables", "sprites", "effects", "items", "maps", "ui"].map(String::from).to_vec() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    for (p, d) in &c.files {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, d).unwrap();
    }
    std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&only)).unwrap();
    Some(dir)
}

fn packs() -> Option<Packs> {
    let d = content_dir()?;
    Some(Packs::new(vec![Box::new(DirSource(d)) as Box<dyn FileSource>]).unwrap())
}

fn app(arena: &str, seed: u32) -> Option<App> {
    let cfg = wf_app::battle::config(arena, "t68", 2, "goku", None, None, 1, seed);
    Some(App::battle(packs()?, cfg).expect("battle"))
}

/// Walk to the nearest enemy and punch, from the view only (like the battle test's autopilot).
fn autopilot(v: &RenderView, f: u64) -> u16 {
    let Some(cam) = v.cameras.first() else { return 0 };
    let focus = (cam.x + 120, cam.y + 84);
    let bodies: Vec<(u32, i32, i32)> = v.sprites.iter().filter(|s| s.sheet.starts_with("sprites/")).map(|s| (s.entity, s.x, s.y)).collect();
    let d = |p: (i32, i32), q: (i32, i32)| (p.0 - q.0).pow(2) + (p.1 - q.1).pow(2);
    let Some(pl) = bodies.iter().min_by_key(|s| d((s.1, s.2), focus)).copied() else { return 0 };
    let Some(en) = bodies.iter().filter(|s| s.0 != pl.0).min_by_key(|s| d((s.1, s.2), (pl.1, pl.2))) else { return 0 };
    let (dx, dy) = (en.1 - pl.1, en.2 - pl.2);
    if (f / 150) % 4 == 3 {
        return [b::UP, b::LEFT, b::DOWN, b::RIGHT][((f / 37) % 4) as usize]; // wander to vary the fight
    }
    if dx.abs() > 20 {
        if dx > 0 { b::RIGHT } else { b::LEFT }
    } else if dy.abs() > 6 {
        if dy > 0 { b::DOWN } else { b::UP }
    } else if f % 12 < 2 {
        b::A
    } else if f % 90 < 3 {
        b::B
    } else if dx > 0 {
        b::RIGHT
    } else {
        b::LEFT
    }
}

/// Uninterrupted reference run: inputs and state hash per frame.
fn reference(frames: u64) -> Option<(Vec<u16>, Vec<u64>)> {
    let mut a = app("forest", 7)?;
    let mut inputs = Vec::new();
    let mut hashes = vec![a.sim.state_hash()];
    let mut v = RenderView::default();
    let mut ev = Vec::new();
    for f in 0..frames {
        a.sim.view(&mut v);
        let i = autopilot(&v, f);
        inputs.push(i);
        a.sim.step(&[i], &mut ev);
        hashes.push(a.sim.state_hash());
    }
    Some((inputs, hashes))
}

fn replays(reqs: Vec<Request>) -> Vec<(String, Vec<u8>)> {
    reqs.into_iter().filter_map(|r| if let Request::SaveReplay { id, bytes, .. } = r { Some((id, bytes)) } else { None }).collect()
}

#[test]
fn replay_round_trip_and_playback() {
    let Some((inputs, hashes)) = reference(3600) else { return };
    let mut a = app("forest", 7).unwrap();
    let mut c = Control::new(&mut a, Hotkeys::default(), 1_790_000_000);
    assert!(c.enabled);
    for &i in &inputs {
        assert!(c.step(&mut a, &[i]));
    }
    assert_eq!(a.sim.state_hash(), hashes[3600]);
    let (id, r) = c.current_replay(&a).unwrap();
    let bytes = r.to_bytes();
    println!("replay {id}: 3600 frames (1 min) -> {} bytes ({} checkpoints, {} input bytes)", bytes.len(), r.checkpoints.len(), r.inputs.to_bytes().len());
    assert!(bytes.len() < 4096, "{} bytes", bytes.len());

    // headless verification on a fresh sim
    let t = Instant::now();
    let rep = c.verify(&a, &bytes).unwrap();
    println!("verify: {:?} in {:.1} ms", rep, t.elapsed().as_secs_f64() * 1e3);
    assert!(rep.ok && rep.frames == 3600 && rep.final_hash == hashes[3600]);

    // play it back through the app in another session (different arena first: the replay's config wins)
    let mut b2 = app("crater", 3).unwrap();
    let mut c2 = Control::new(&mut b2, Hotkeys::default(), 1_790_000_100);
    c2.play_replay(&mut b2, &id, &bytes).unwrap();
    assert!(c2.is_playback());
    let mut f = 0;
    while c2.step(&mut b2, &[0]) {
        f += 1;
        assert_eq!(b2.sim.state_hash(), hashes[f], "frame {f}");
    }
    assert_eq!(f, 3600);
    assert!(c2.divergence().is_none());

    // seek around (snapshots + re-simulation), then branch
    for target in [100u64, 3500, 0, 1800, 2999, 61] {
        let t = Instant::now();
        assert_eq!(c2.seek(&mut b2, target), target);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert_eq!(b2.sim.state_hash(), hashes[target as usize], "seek {target}");
        println!("seek to {target}: {ms:.3} ms");
    }
    println!("timeline: {} snapshots, {} KB", c2.timeline.len(), c2.timeline.bytes() / 1024);
    c2.seek(&mut b2, 1000);
    c2.act(&mut b2, Action::TakeOver);
    assert!(!c2.is_playback());
    for _ in 0..300 {
        c2.step(&mut b2, &[b::LEFT | b::UP]);
    }
    let (bid, br) = c2.current_replay(&b2).unwrap();
    assert_ne!(bid, id);
    assert_eq!(br.header.branch.as_ref().map(|x| (x.parent.as_str(), x.frame)), Some((id.as_str(), 1000)));
    assert_eq!(br.inputs.len(), 1300);
    assert!(c2.verify(&b2, &br.to_bytes()).unwrap().ok);

    // a tampered replay reports where it diverges
    let mut bad = Replay::from_bytes(&bytes).unwrap();
    let mut l = wf_app::wf_replay::InputLog::new(1, 0);
    for (k, x) in bad.inputs.frames().enumerate() {
        l.push(&[if (1500..1530).contains(&k) { b::B } else { x[0] }]);
    }
    bad.inputs = l;
    let rep = c.verify(&a, &bad.to_bytes()).unwrap();
    println!("tampered: {}", rep.divergence.unwrap());
    assert!(!rep.ok && rep.divergence.unwrap().frame > 1500 && rep.divergence.unwrap().last_good <= 1500);
}

#[test]
fn rewind_then_resume_equals_uninterrupted() {
    let Some((inputs, hashes)) = reference(3000) else { return };
    let mut a = app("forest", 7).unwrap();
    let mut c = Control::new(&mut a, Hotkeys::default(), 1_790_000_000);
    for &i in &inputs[..2400] {
        c.step(&mut a, &[i]);
    }
    // hold rewind for 10 s of host frames at 60 Hz
    c.hold(&mut a, Action::Rewind, true);
    let t = Instant::now();
    let mut n = 0;
    while a.sim.frame() > 1800 {
        c.advance(&mut a, 1.0 / 60.0, &[0]);
        n += 1;
    }
    let per = t.elapsed().as_secs_f64() * 1e3 / n as f64;
    c.hold(&mut a, Action::Rewind, false);
    let f = a.sim.frame() as usize;
    println!("rewound 2400 -> {f} in {n} host frames ({per:.3} ms per host frame)");
    assert_eq!(a.sim.state_hash(), hashes[f]);
    // resume with the same inputs
    for &i in &inputs[f..3000] {
        c.step(&mut a, &[i]);
    }
    assert_eq!(a.sim.state_hash(), hashes[3000]);
    let (_, r) = c.current_replay(&a).unwrap();
    assert_eq!(r.inputs.len(), 3000);
    assert!(c.verify(&a, &r.to_bytes()).unwrap().ok);

    // rewind and play differently: the recording is truncated there and continues (a branch)
    c.seek(&mut a, 1000);
    for _ in 0..200 {
        c.step(&mut a, &[b::DOWN]);
    }
    let (_, r) = c.current_replay(&a).unwrap();
    assert_eq!(r.inputs.len(), 1200);
    assert_eq!(r.inputs.get(1100), Some(&[b::DOWN][..]));
    assert!(c.verify(&a, &r.to_bytes()).unwrap().ok);
}

#[test]
fn save_load_across_restarts() {
    let Some((inputs, hashes)) = reference(1800) else { return };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-states");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("slot1.wfs");
    {
        let mut a = app("forest", 7).unwrap();
        let mut c = Control::new(&mut a, Hotkeys::default(), 1_790_000_000);
        for &i in &inputs[..1200] {
            c.step(&mut a, &[i]);
        }
        let t = Instant::now();
        c.act(&mut a, Action::SaveState(1));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let reqs = c.take_requests();
        let Some(Request::SaveState { slot: 1, bytes }) = reqs.into_iter().find(|r| matches!(r, Request::SaveState { .. })) else { panic!("no save request") };
        let raw = a.sim.snapshot().unwrap().len();
        println!("save state: {} bytes on disk (snapshot {raw} raw), {ms:.2} ms", bytes.len());
        std::fs::write(&path, bytes).unwrap();
    }
    // "restart": new packs, a different battle running, load from disk
    let mut a = app("crater", 99).unwrap();
    let mut c = Control::new(&mut a, Hotkeys::default(), 1_790_000_500);
    c.act(&mut a, Action::LoadState(1));
    assert!(c.take_requests().iter().any(|r| matches!(r, Request::LoadState { slot: 1 })));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(SaveState::peek_header(&bytes).unwrap().frame, 1200);
    let t = Instant::now();
    c.load_state(&mut a, &bytes).unwrap();
    println!("load state: {:.2} ms", t.elapsed().as_secs_f64() * 1e3);
    assert_eq!(a.sim.frame(), 1200);
    assert_eq!(a.sim.state_hash(), hashes[1200]);
    for &i in &inputs[1200..] {
        c.step(&mut a, &[i]);
    }
    assert_eq!(a.sim.state_hash(), hashes[1800]);
    // the recording carries on from the state's history: verifiable from frame 0, rewindable before the load
    let (_, r) = c.current_replay(&a).unwrap();
    assert_eq!(r.inputs.len(), 1800);
    assert!(c.verify(&a, &r.to_bytes()).unwrap().ok);
    c.seek(&mut a, 600);
    assert_eq!(a.sim.state_hash(), hashes[600]);
    // finalize hands the recording to the host
    c.finalize(&a);
    assert_eq!(replays(c.take_requests()).len(), 1);
}

#[test]
fn ghost_and_overlays_never_touch_the_sim() {
    let Some((inputs, _)) = reference(900) else { return };
    let mut a = app("forest", 7).unwrap();
    let mut c = Control::new(&mut a, Hotkeys::default(), 1_790_000_000);
    for &i in &inputs {
        c.step(&mut a, &[i]);
    }
    let (gid, g) = c.current_replay(&a).unwrap();
    // new live fight with the first one as a ghost
    let mut a = app("forest", 7).unwrap();
    let mut c = Control::new(&mut a, Hotkeys::default(), 1_790_000_100);
    c.set_ghost(&a, &gid, &g.to_bytes()).unwrap();
    for f in 0..300 {
        c.step(&mut a, &[if f % 50 < 25 { b::UP } else { b::LEFT }]);
    }
    let h = a.sim.state_hash();
    let plain = {
        let f = c.render(&mut a, 0, 960, 640);
        assert!(a.view.sprites.iter().any(|s| matches!(s.effect, SpriteEffect::Blend { eva: 7, evb: 9 })), "no ghost sprite");
        (a.renderer.fb.clone(), f)
    };
    c.act(&mut a, Action::Training);
    c.act(&mut a, Action::InputDisplay);
    let t = Instant::now();
    c.render(&mut a, 0, 960, 640);
    println!("render with ghost + training + input overlays: {:.2} ms", t.elapsed().as_secs_f64() * 1e3);
    assert_ne!(plain.0, a.renderer.fb, "overlays drew nothing");
    assert_eq!(a.sim.state_hash(), h, "rendering changed the sim");
    if let Ok(d) = std::env::var("WF_SHOT_DIR") {
        let (w, hh) = (plain.1.view_w as usize, plain.1.view_h as usize);
        let mut out = format!("P6\n{w} {hh}\n255\n").into_bytes();
        for &p in &a.renderer.fb[..w * hh] {
            out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
        std::fs::write(format!("{d}/training.ppm"), out).unwrap();
    }
    let st = c.status_json(&a);
    assert!(st.contains("\"ghost\":\"") && st.contains("\"training\":true"), "{st}");
}

#[test]
fn snapshot_costs() {
    let Some(mut a) = app("forest", 7) else { return };
    let mut ev = Vec::new();
    for f in 0..600 {
        a.sim.step(&[if f % 40 < 20 { b::RIGHT } else { b::A }], &mut ev);
    }
    let n = 200;
    let t = Instant::now();
    let mut s = Vec::new();
    for _ in 0..n {
        s = a.sim.snapshot().unwrap();
    }
    let snap = t.elapsed().as_secs_f64() * 1e6 / n as f64;
    let t = Instant::now();
    let mut z = Vec::new();
    for _ in 0..n {
        z = lz::compress(&s);
    }
    let comp = t.elapsed().as_secs_f64() * 1e6 / n as f64;
    let t = Instant::now();
    for _ in 0..n {
        assert_eq!(lz::decompress(&z).unwrap().len(), s.len());
    }
    let dec = t.elapsed().as_secs_f64() * 1e6 / n as f64;
    let t = Instant::now();
    for _ in 0..n {
        assert!(a.sim.restore(&s));
    }
    let rest = t.elapsed().as_secs_f64() * 1e6 / n as f64;
    println!("snapshot {} B -> lz {} B; snapshot {snap:.1} us, compress {comp:.1} us, decompress {dec:.1} us, restore {rest:.1} us", s.len(), z.len());
    assert!(z.len() * 4 < s.len());
}
