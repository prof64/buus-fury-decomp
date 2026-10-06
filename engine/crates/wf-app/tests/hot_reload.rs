//! Dev mode's hot reload (`wf_app::dev`, `buus-fury --dev`), headless and muted: a temp copy of an example mod is
//! loaded, a running scene steps, a mod file is edited, the reload hook runs between frames - the new value is live
//! and the scene restarted from scratch; a broken edit keeps the old content and reports the checker's message.
//! Skipped without `roms/buu.gba`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wf_app::dev::{restart, Dev, Reload};
use wf_app::App;

fn base_dir() -> Option<PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if let Ok(d) = std::env::var("WF_FULL_CONTENT") {
            return Some(PathBuf::from(d));
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-hotreload-content");
        if wf_extract::cache::fresh(&dir, &[]) {
            return Some(dir);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let rom = wf_rom::testutil::rom()?;
        let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
        for (p, b) in &c.files {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b).unwrap();
        }
        std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&[])).unwrap();
        Some(dir)
    })
    .clone()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t);
        } else {
            std::fs::copy(&p, &t).unwrap();
        }
    }
}

/// a fresh temp copy of `engine/mods-examples/<name>`
fn temp_mod(name: &str, tag: &str) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods-examples").join(name);
    let dst = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("hotreload-{tag}")).join(name);
    let _ = std::fs::remove_dir_all(&dst);
    copy_dir(&src, &dst);
    dst
}

/// an edit with a changed modification time even on coarse file systems (the watcher compares size and mtime)
fn write_later(p: &Path, text: &str) {
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(p, text).unwrap();
}

fn enemy_hp(app: &App, ty: i32) -> i64 {
    wf_app::battle::game_data(&app.packs).unwrap().enemy(ty).unwrap().hp as i64
}

#[test]
fn battle_hot_reload_restarts_with_the_new_numbers() {
    let Some(base) = base_dir() else { return };
    let m = temp_mod("balance-tweak", "battle");
    let mods = vec![m.clone()];
    let mut dev = Dev::new(&base, &mods);
    let cfg = wf_app::battle::config("forest", "t68", 1, "goku", None, None, 1, 7);
    let build = |p| App::battle(p, cfg.clone());
    let mut app = build(wf_app::mods::load(&base, &mods).unwrap()).unwrap();
    assert_eq!(enemy_hp(&app, 68), 1500);
    for _ in 0..90 {
        app.step();
        assert!(matches!(dev.poll(), Reload::Unchanged), "nothing edited yet");
    }
    let start_hash = build(wf_app::mods::load(&base, &mods).unwrap()).unwrap().sim.state_hash();
    assert_ne!(app.sim.state_hash(), start_hash, "the battle ran on");

    // a broken edit: the old content stays, the checker's message names the file
    let f = m.join("enemies/majin_fighter.patch.json");
    write_later(&f, "{ \"hp\": 2500, ");
    match dev.check_now() {
        Reload::Rejected(msg) => assert!(msg.contains("majin_fighter.patch.json"), "{msg}"),
        _ => panic!("a broken patch must be rejected"),
    }
    // the running scene is untouched: it steps on from where it was (its game data was built at its start)
    let (f0, h0) = (app.sim.frame(), app.sim.state_hash());
    app.step();
    assert_eq!(app.sim.frame(), f0 + 1, "old scene kept running");
    assert_ne!(app.sim.state_hash(), h0);
    // (polled again with no further change: nothing new)
    assert!(matches!(dev.check_now(), Reload::Unchanged));

    // the fixed edit: reloaded, the scene restarted from its start with the new number
    write_later(&f, "{\n  \"hp\": 2500,\n  \"STR\": 30,\n  \"exp\": 2000\n}\n");
    let packs = match dev.check_now() {
        Reload::Reloaded(p) => p,
        Reload::Rejected(m) => panic!("rejected: {m}"),
        Reload::Unchanged => panic!("the edit was not seen"),
    };
    app = restart(&app, packs, build).unwrap();
    assert_eq!(enemy_hp(&app, 68), 2500, "new value live");
    let fresh = build(wf_app::mods::load(&base, &mods).unwrap()).unwrap();
    assert_eq!(app.sim.frame(), 0, "the scene restarted");
    assert_eq!(app.sim.state_hash(), fresh.sim.state_hash(), "a restart is a clean start (deterministic)");
    // the restarted battle's enemy has the new HP
    let s = wf_app::battle::session(&app.packs, cfg.clone()).unwrap();
    let (_, en) = s.world.enemy_by_id(s.world.enemies[0]).unwrap();
    assert_eq!(en.max_hp1024 >> 10, 2500);
}

#[test]
fn story_hot_reload_picks_up_a_skill_patch_and_dialogue() {
    let Some(base) = base_dir() else { return };
    let m = temp_mod("story-tweaks", "story");
    let mods = vec![m.clone()];
    let mut dev = Dev::new(&base, &mods);
    let build = |p| wf_app::story::app(p, (3, 1), vec![], 1);
    let mut app = build(wf_app::mods::load(&base, &mods).unwrap()).unwrap();
    for _ in 0..60 {
        app.step();
    }
    assert!(app.sim.frame() > 0);
    // the Kamehameha's damage and the clerk's goodbye edited together
    write_later(&m.join("skills/15_kamehameha.patch.json"), "{\"engine\": {\"check\": [10], \"damage\": [2000]}}\n");
    let t = m.join("scripts/events/s028.text.json");
    let txt = std::fs::read_to_string(&t).unwrap().replace("Say hi to Korin for me!", "Come back soon, hero!");
    write_later(&t, &txt);
    let packs = match dev.check_now() {
        Reload::Reloaded(p) => p,
        Reload::Rejected(m) => panic!("rejected: {m}"),
        Reload::Unchanged => panic!("the edits were not seen"),
    };
    app = restart(&app, packs, build).unwrap();
    assert_eq!(app.sim.frame(), 0, "story mode restarted at its start area");
    let d = wf_app::battle::game_data(&app.packs).unwrap();
    assert_eq!(d.skills.get(&15).unwrap().engine.clone().unwrap().damage, vec![2000]);
    assert!(app.packs.read_str("scripts/events/s028.text.json").unwrap().contains("Come back soon, hero!"));
    // an edit to a script that does not compile: rejected, with the file in the message
    write_later(&t, "{\"nodes\": {\"9999\": \"no such node\"}}\n");
    match dev.check_now() {
        Reload::Rejected(msg) => assert!(msg.contains("s028"), "{msg}"),
        _ => panic!("a bad dialogue override must be rejected"),
    }
}
