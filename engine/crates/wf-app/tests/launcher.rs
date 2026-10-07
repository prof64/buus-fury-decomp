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

/// The Battle page's character picker (Character / Form / Outfit): no two rows with the same labels, every row starts
/// a battle in exactly its form record, that form's sprite has the fighting animations and the fighter lands hits on
/// Majin Fighters; a form's transformations stay on the picker (a halo stays a halo); settings naming a record the
/// picker no longer lists move to its replacement.
#[test]
fn battle_picker_rows_are_distinct_and_playable() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let p = packs(&c.files);
    let data = wf_app::battle::load_data(&p).unwrap();
    let l = Launcher::new(&p, Settings::default(), true);
    let picks = l.picks().to_vec();
    let chars: Vec<&str> = picks.iter().map(|k| k.character.as_str()).fold(vec![], |mut v, c| {
        if !v.contains(&c) {
            v.push(c);
        }
        v
    });
    assert_eq!(chars, ["Goku", "Gohan", "Goten", "Trunks", "Vegeta", "Gotenks", "Vegito", "Gogeta", "Hercule", "Videl", "Pikkon"]);
    for (i, a) in picks.iter().enumerate() {
        for b in &picks[i + 1..] {
            assert!((&a.character, &a.form, &a.outfit) != (&b.character, &b.form, &b.outfit), "duplicate row {a:?}");
            assert!((&a.fighter, a.form_id) != (&b.fighter, b.form_id), "one record twice {a:?} {b:?}");
        }
        // up / down lead to a row of the same character, with the same look when that form has it
        let f = data.form(a.form_id).unwrap();
        for to in [f.up, f.down].into_iter().filter(|&t| t != 255) {
            let to = [(25, 24)].iter().find(|x| x.0 == to).map(|x| x.1).unwrap_or(to);
            let t = picks.iter().find(|q| q.fighter == a.fighter && q.form_id == to).unwrap_or_else(|| panic!("{a:?}: transforms into {to}, not on the picker"));
            let looks: Vec<&str> = picks.iter().filter(|q| q.character == t.character && q.form == t.form).map(|q| q.outfit.as_str()).collect();
            if looks.contains(&a.outfit.as_str()) {
                assert_eq!(t.outfit, a.outfit, "{a:?} transforms into {t:?}");
            }
        }
    }
    for k in &picks {
        let mut s = Settings::default();
        s.battle.fighter = k.fighter.clone();
        s.battle.form = Some(k.form_id);
        s.battle.enemy = "t68".into();
        s.battle.count = 2;
        let l = Launcher::new(&p, s, true);
        let cfg = l.battle_config();
        assert_eq!((cfg.players[0].fighter.as_str(), cfg.players[0].form), (k.fighter.as_str(), k.form_id));
        let mut ses = wf_core::session::Session::new(data.clone(), cfg);
        let hp = |w: &wf_core::world::World| -> i64 { w.ents.iter().filter_map(|e| if let wf_core::world::Body::Enemy(n) = &e.body { Some(n.hp1024) } else { None }).sum() };
        let hp0 = hp(&ses.world);
        {
            let w = &ses.world;
            let pl = w.pl_ref(w.player_idx(0).unwrap());
            assert_eq!(pl.form, k.form_id);
            let sp = pl.a.sprite.and_then(|s| w.data.sprite(s)).unwrap_or_else(|| panic!("{k:?}: no sprite"));
            for raw in [1, 2, 3, 4, 5, 6, 7, 8, 10] {
                assert!(sp.anim_raw(raw).is_some(), "{k:?}: no animation {raw}");
            }
        }
        // walk in, punch, Energy Blasts
        for f in 0..900usize {
            let keys: u16 = match (f / 30) % 3 {
                0 => 0x10,
                1 => (f % 6 < 2) as u16,
                _ => if f % 10 < 2 { 2 } else { 0 },
            };
            ses.step(&[keys]);
        }
        assert!(hp(&ses.world) < hp0, "{k:?}: no hit landed");
    }
    // saved settings: a record the picker does not list, and no form at all
    for (fighter, form, want) in [("goku", Some(1), 0), ("gohan", Some(11), 10), ("gogeta", Some(44), 41), ("goku", None, 2), ("f48", Some(48), 0)] {
        let mut s = Settings::default();
        s.battle.fighter = fighter.into();
        s.battle.form = form;
        let l = Launcher::new(&p, s, true);
        assert_eq!(l.battle_config().players[0].form, want, "{fighter} {form:?}");
    }
}
