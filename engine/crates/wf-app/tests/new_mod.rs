//! `buus-fury --new-mod NAME` (wf_app::newmod): the scaffold is valid and playable out of the box - modcheck OK,
//! loads, the patched value reads back, the dialogue override shows, the sample NPC's Rhai event runs and sets its
//! flag. Headless and muted. Skipped without `roms/buu.gba`.

use std::path::PathBuf;
use std::sync::OnceLock;

use wf_app::story::StorySim;
use wf_core::input as btn;
use wf_core::world::{Body, World};
use wf_view::{RenderView, Sim};

/// the base pack, shared with tests/mods.rs (the same cargo tmp dir; `WF_FULL_CONTENT` uses an existing one)
fn base_dir() -> Option<PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if let Ok(d) = std::env::var("WF_FULL_CONTENT") {
            return Some(PathBuf::from(d));
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-mods-content");
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

fn w(s: &StorySim) -> &World {
    s.world()
}

fn place(s: &mut StorySim, x: i32, y: i32, facing: u8) {
    let i = s.world().player_idx(0).unwrap();
    let a = &mut s.session.world.pl(i).a;
    a.x = x << 8;
    a.y = y << 8;
    a.facing = facing;
}

fn dialog_text(s: &StorySim) -> Option<String> {
    let mut v = RenderView::default();
    s.view(&mut v);
    v.dialog.map(|d| d.text)
}

/// steps holding `k` (A alternates so text boxes advance); every frame's dialog text is passed to `seen`
fn run(s: &mut StorySim, frames: u32, k: u16, seen: &mut Vec<String>, mut until: impl FnMut(&StorySim) -> bool) -> bool {
    let mut ev = vec![];
    for f in 0..frames {
        let a = if k & btn::A != 0 && f % 6 >= 3 { k & !btn::A } else { k };
        s.step(&[a], &mut ev);
        assert!(s.errors.is_empty(), "script errors: {:?}", s.errors);
        if let Some(t) = dialog_text(s) {
            if seen.last() != Some(&t) {
                seen.push(t);
            }
        }
        if until(s) {
            return true;
        }
    }
    false
}

#[test]
fn names_are_checked_and_nothing_is_overwritten() {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("new-mod-names");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    for bad in ["", "My Mod", "../x", "-x", "a/b", "UPPER"] {
        assert!(wf_app::newmod::scaffold(&tmp, bad).is_err(), "'{bad}' accepted");
    }
    let m = wf_app::newmod::scaffold(&tmp, "my-first-mod").expect("scaffold");
    assert!(m.dir.join("pack.json").exists());
    let e = wf_app::newmod::scaffold(&tmp, "my-first-mod").expect_err("a second scaffold over it");
    assert!(e.contains("already exists"), "{e}");
}

#[test]
fn a_new_mod_checks_ok_loads_and_plays() {
    let Some(base) = base_dir() else { return };
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("new-mod-play");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let m = wf_app::newmod::scaffold(&tmp, "hello-mod").expect("scaffold");
    // modcheck: OK as it is
    let r = wf_app::modcheck::check(&base, &[m.dir.clone()]);
    assert!(r.ok(), "modcheck:\n{}", r.text());
    // loads with no warnings; the patch reads back
    let p = wf_app::mods::load(&base, &[m.dir.clone()]).expect("packs");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let d = wf_app::battle::game_data(&p).expect("game data");
    let goku = d.chars.iter().find(|c| c.index == 0).unwrap();
    assert_eq!((goku.end, goku.str_), (wf_app::newmod::SAMPLE_END, 27), "goku.patch.json: END changed, STR kept");
    // the sample NPC in Snakeway 1-1: talk from below -> its text, the flag
    let mut s = StorySim::new(&p, wf_app::newmod::SAMPLE_AREA, vec![], 5).expect("story");
    let mut seen = vec![];
    run(&mut s, 60, 0, &mut seen, |_| false);
    let npc = w(&s)
        .ents
        .iter()
        .filter(|e| !e.removed)
        .find_map(|e| match &e.body {
            Body::Npc(n) if n.talk == Some(m.event as i32) => Some((n.a.px(), n.a.py())),
            _ => None,
        })
        .expect("the sample NPC stands in 1-1");
    assert_eq!(npc, wf_app::newmod::SAMPLE_NPC_POS);
    place(&mut s, npc.0, npc.1 + 18, 1);
    let flag = m.flag as i32;
    assert!(
        run(&mut s, 1500, btn::A, &mut seen, |s| s.world().story.flag(flag) && !s.world().field.as_ref().unwrap().busy),
        "the NPC's event ran and set flag {flag}; texts {seen:?}"
    );
    assert!(seen.iter().any(|t| t.contains("from the hello-mod mod")), "its text box: {seen:?}");
    // talking again: the other line
    seen.clear();
    place(&mut s, npc.0, npc.1 + 18, 1);
    assert!(run(&mut s, 600, btn::A, &mut seen, |s| dialog_text(s).is_some_and(|t| t.contains("Hello again"))), "{seen:?}");
}

#[test]
fn a_new_mods_dialogue_override_shows() {
    let Some(base) = base_dir() else { return };
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("new-mod-text");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let m = wf_app::newmod::scaffold(&tmp, "text-mod").expect("scaffold");
    let p = wf_app::mods::load(&base, &[m.dir.clone()]).expect("packs");
    // King Yemma's counter (Snakeway 1-3, the opening's route: A facing up at (232, 362)) runs s048
    let mut s = StorySim::new(&p, (1, 3), vec![], 5).expect("story");
    let mut seen = vec![];
    run(&mut s, 60, 0, &mut seen, |_| false);
    place(&mut s, 232, 362, 1);
    assert!(
        run(&mut s, 900, btn::A, &mut seen, |s| dialog_text(s).is_some_and(|t| t.contains("from the text-mod mod"))),
        "s048.text.json's line: {seen:?}"
    );
}
