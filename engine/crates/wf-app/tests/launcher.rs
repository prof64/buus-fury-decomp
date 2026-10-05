//! The launcher (headless): its menus produce the games the shells start, and a story save slot written by a
//! session continues where it was saved.

use wf_app::launcher::{self, Choice, Launcher, Settings};
use wf_app::wf_content::{FileSource, MemSource, Packs};

fn packs(files: &std::collections::BTreeMap<String, Vec<u8>>) -> Packs {
    Packs::new(vec![Box::new(MemSource(files.clone())) as Box<dyn FileSource>]).unwrap()
}

#[test]
fn launcher_menus_and_save_slots() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let p = packs(&c.files);
    let mut l = Launcher::new(&p, Settings::default(), true);
    // Battle page: START
    assert_eq!(l.key("ArrowDown"), Choice::None);
    assert_eq!(l.key("Enter"), Choice::None);
    let Choice::Battle(cfg) = l.key("Enter") else { panic!("battle") };
    assert_eq!(cfg.players[0].fighter, "goku");
    assert!(launcher::Launcher::new(&p, Settings::default(), true).battle_config().phases[0].enemy.starts_with('t'));
    let mut fb = vec![0u32; 240 * 160];
    l.draw(&p, &mut fb, 240, 160);
    assert!(fb.iter().any(|&px| px != 0xFF00_0000));
    // Story page: the title screen hand-over
    l.key("Escape");
    l.key("ArrowUp");
    assert_eq!(l.key("Enter"), Choice::None);
    assert_eq!(l.key("Enter"), Choice::Title);
    assert!(launcher::title_app(packs(&c.files), &[], 1).is_ok());

    // a slot saved by a session continues there
    let app = launcher::story_app(packs(&c.files), &[], None, 1).expect("new game");
    let (gen, files) = launcher::saves_of(&app).expect("story saves");
    assert_eq!(gen, 0);
    // (the slot files are empty; the block after them is the options, `saves_of`)
    assert!(files[..launcher::SLOTS].iter().all(|f| f.is_empty()));
    let s = app.sim.as_any().unwrap().downcast_ref::<wf_app::story::StorySim>().unwrap();
    let mut saves = s.session.world.saves.clone();
    saves.set(1, Some(wf_core_save(s)));
    let slots: Vec<Vec<u8>> = (0..launcher::SLOTS).map(|i| saves.slot_bytes(i)).collect();
    assert!(!slots[1].is_empty());
    let d = wf_app::battle::load_data(&p).unwrap();
    assert!(launcher::slot_summary(&slots[1], &d).is_some_and(|t| t.contains("1-1")), "{:?}", launcher::slot_summary(&slots[1], &d));
    let app = launcher::story_app(packs(&c.files), &slots, Some(1), 1).expect("continue");
    let s2 = app.sim.as_any().unwrap().downcast_ref::<wf_app::story::StorySim>().unwrap();
    assert_eq!(s2.session.world.saves.current, 1);
    assert!(launcher::story_app(packs(&c.files), &slots, Some(2), 1).is_err(), "empty slot");
}

fn wf_core_save(s: &wf_app::story::StorySim) -> wf_core_saves::SaveData {
    wf_core_saves::save_data(&s.session.world, false)
}

use wf_core::menu::save as wf_core_saves;
