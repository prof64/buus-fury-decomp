//! Replays, rewind and save states in story mode (`control.rs` with a `StorySim` started on the title), headless and
//! muted: the title -> New Game -> the prologue with A taps, then rewind (seek) and resume, a save state loaded back
//! (also into an app running a battle), the recording verified and played on a fresh app - every hash equal to the
//! uninterrupted run's. Content: the user's ROM (roms/buu.gba), fully extracted once into the cargo target dir.

use std::path::PathBuf;

use wf_app::control::Control;
use wf_app::hotkeys::Hotkeys;
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_app::App;

fn content_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("WF_CONTENT") {
        return Some(PathBuf::from(d));
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-story-control-content");
    if wf_extract::cache::fresh(&dir, &[]) {
        return Some(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let rom = wf_rom::testutil::rom()?;
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    for (p, d) in &c.files {
        let rel = p.strip_prefix("base/").unwrap_or(p);
        let f = dir.join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, d).unwrap();
    }
    std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&[])).unwrap();
    Some(dir)
}

fn packs() -> Option<Packs> {
    let d = content_dir()?;
    Some(Packs::new(vec![Box::new(DirSource(d)) as Box<dyn FileSource>]).unwrap())
}

/// START taps through the title, then A every 6th step (New Game, the prologue's boxes)
fn key(k: usize) -> u16 {
    if k < 60 {
        if k % 2 == 0 { 8 } else { 0 }
    } else if k % 6 == 0 {
        1
    } else {
        0
    }
}

#[test]
fn story_rewind_save_states_and_replays() {
    let Some(p) = packs() else {
        eprintln!("skipped: no ROM");
        return;
    };
    let n = 2400usize;
    let mut app = wf_app::launcher::title_app(p, &[vec![], vec![], vec![]], 1).expect("title");
    let mut ctl = Control::new(&mut app, Hotkeys::default(), 0);
    assert!(ctl.enabled, "story mode records");
    let mut hashes = vec![app.sim.state_hash()];
    for k in 0..n {
        assert!(ctl.step(&mut app, &[key(k)]));
        hashes.push(app.sim.state_hash());
    }
    assert_eq!(app.sim.frame(), n as u64);

    // rewind (seek back through the timeline) and resume with the same inputs: the same states
    for target in [n - 30, 1500, 700, 61, 0] {
        let f = ctl.seek(&mut app, target as u64);
        assert_eq!(f, target as u64);
        assert_eq!(app.sim.state_hash(), hashes[target], "seek to {target}");
    }
    for k in 0..n {
        ctl.step(&mut app, &[key(k)]);
        assert_eq!(app.sim.state_hash(), hashes[k + 1], "resumed step {}", k + 1);
    }

    // a save state: taken, the run goes on differently, loaded back
    let st = ctl.make_state(&app, 1).expect("story save state").to_bytes();
    for _ in 0..300 {
        ctl.step(&mut app, &[16]);
    }
    assert_ne!(app.sim.state_hash(), hashes[n]);
    ctl.load_state(&mut app, &st).expect("load");
    assert_eq!((app.sim.frame(), app.sim.state_hash()), (n as u64, hashes[n]));

    // the recording, verified and played on a fresh app that runs a battle
    let (_, rep) = ctl.current_replay(&app).expect("recording");
    let bytes = rep.to_bytes();
    let cfg = wf_app::battle::config("forest", "t68", 1, "goku", None, None, 1, 1);
    let mut other = App::battle(packs().unwrap(), cfg).expect("battle");
    let mut c2 = Control::new(&mut other, Hotkeys::default(), 0);
    let v = c2.verify(&other, &bytes).expect("verify");
    assert!(v.ok, "{:?}", v.divergence);
    assert_eq!(v.final_hash, hashes[n]);
    c2.play_replay(&mut other, "story", &bytes).expect("play");
    while c2.step(&mut other, &[0]) {}
    assert_eq!((other.sim.frame(), other.sim.state_hash()), (n as u64, hashes[n]));
    // ... and the save state into that app too
    let mut third = App::battle(packs().unwrap(), wf_app::battle::config("forest", "t68", 1, "goku", None, None, 1, 1)).unwrap();
    let mut c3 = Control::new(&mut third, Hotkeys::default(), 0);
    c3.load_state(&mut third, &st).expect("load into a battle app");
    assert_eq!(third.sim.state_hash(), hashes[n]);
}
