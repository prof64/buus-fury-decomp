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

/// A quick battle is set up as the story would have the fighter (user report: SS3 Goku vs Kid Buu at level 60 -
/// every skill greyed out, one hit from Kid Buu a KO): the enemies' level (Kid Buu's level byte 150), the level's
/// stat points spent, the story's skill level and a full SS3 gauge - every skill of the form usable at the start.
#[test]
fn quick_battle_defaults_follow_the_enemy() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let p = packs(&c.files);
    let mut s = Settings::default();
    s.battle.fighter = "goku".into();
    s.battle.form = Some(5);
    s.battle.enemy = "t60".into();
    let l = Launcher::new(&p, s, true);
    let cfg = l.battle_config();
    let f = &cfg.players[0];
    assert_eq!((f.form, f.level, f.skill_level, f.full_gauge, f.auto_spend.as_str()), (5, 150, 2, true, "even"));
    let mut ses = wf_core::session::Session::new(wf_app::battle::load_data(&p).unwrap(), cfg);
    ses.step(&[0]);
    let w = &ses.world;
    let pi = w.player_idx(0).unwrap();
    assert!(w.pl_ref(pi).ss3_gauge >> 16 > 1000, "SS3 gauge {}", w.pl_ref(pi).ss3_gauge >> 16);
    // (Super Saiyan itself is not usable in an SS3 form - the game's check)
    for &sk in w.pl_ref(pi).skills.iter().filter(|&&s| s != 20) {
        assert!(wf_core::player::can_use_skill(w, pi, sk), "skill {sk} greyed out");
    }
    // settings saved before the automatic level existed get it
    assert!(Settings::from_json(r#"{"battle":{"level":60}}"#).unwrap().battle.level_auto);
}
