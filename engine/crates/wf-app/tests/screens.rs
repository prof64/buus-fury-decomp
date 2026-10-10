//! The story screens' hand-overs, headless + muted: Switch Character (native 0x8D) rebuilds the leader as the chosen
//! character; `return_to_title` (0x8E, the game over after script #0) restarts the session on the title screen with
//! the title song and the save slots kept.

use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_core::field::FieldConfig;
use wf_core::session::Session;
use wf_view::{buttons as b, Sim};

fn session() -> Option<Session> {
    let rom = wf_rom::testutil::rom()?;
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).unwrap();
    Some(Session::new_field(data, FieldConfig { world: 1, area: 1, ..Default::default() }))
}

fn press(s: &mut Session, k: u16, wait: usize) {
    s.step(&[k]);
    s.step(&[k]);
    for _ in 0..wait {
        s.step(&[0]);
    }
}

#[test]
fn switch_character_rebuilds_the_leader() {
    let Some(mut s) = session() else { return };
    let vegeta = 3;
    s.world.story.chars[vegeta].flags |= 3;
    wf_core::menu::charsel::open_charsel(&mut s.world);
    for _ in 0..40 {
        s.step(&[0]);
    }
    press(&mut s, b::RIGHT, 20);
    press(&mut s, b::A, 60);
    assert_eq!(s.world.story.current, vegeta as i32);
    assert_eq!(wf_core::menu::take_result(&mut s.world), Some(2), "chosen");
    // what native 0x8D does with result 2 (`wf-script-core` poll)
    wf_script_core::natives::switch_character(&mut s.world, vegeta as i32).unwrap();
    let i = s.world.find(s.world.players[0]).unwrap();
    let form = s.world.pl_ref(i).form;
    assert_eq!(s.world.data.form(form).map(|f| f.char_), Some(vegeta as i32));
}

#[test]
fn death_runs_the_hook_and_returns_to_the_title() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let mut s = wf_app::story::StorySim::new(&packs, (1, 1), vec![], 3).expect("story");
    let mut ev = vec![];
    for _ in 0..30 {
        s.step(&[0], &mut ev);
    }
    // the KO (what `after_hp_change` leaves)
    let w = &mut s.session.world;
    let i = w.player_idx(0).unwrap();
    let f = w.frame as i64;
    w.pl(i).rec.hp = 0;
    w.pl(i).dead = true;
    w.pl(i).ko_at = f;
    let mut titled = None;
    for k in 0..3000u32 {
        // "You have died!" waits for A
        s.step(&[if k % 40 < 2 { b::A } else { 0 }], &mut ev);
        if matches!(s.session.world.menu.stack.last(), Some(wf_core::menu::Screen::Title(_))) {
            titled = Some(k);
            break;
        }
    }
    assert!(s.errors.is_empty(), "script errors: {:?}", s.errors);
    assert!(titled.is_some(), "back on the title");
}

#[test]
fn return_to_title_restarts_on_the_title() {
    let Some(mut s) = session() else { return };
    s.world.saves.generation = 7;
    s.world.scene.requests.push(wf_core::scene::Request::ReturnToTitle);
    let ev = s.step(&[0]).to_vec();
    assert!(matches!(s.world.menu.stack.last(), Some(wf_core::menu::Screen::Title(_))), "title screen");
    assert!(ev.iter().any(|e| matches!(e, wf_core::world::Event::Song { id: 45 })), "title song");
    assert_eq!(s.world.saves.generation, 7, "save slots kept");
    for _ in 0..30 {
        s.step(&[0]);
    }
    assert!(wf_core::menu::active(&s.world));
}
