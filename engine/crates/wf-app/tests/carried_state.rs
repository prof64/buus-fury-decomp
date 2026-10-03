//! Regressions for state the app carries where the game does not (or loses where the game keeps it): each was found
//! by the story-mode soak of the real route (engine/docs/segments.md §11) and is invisible to the segment harness,
//! which re-imports that state from the game's RAM at every cut. Headless, muted; no soak needed.

use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_core::field::AreaId;
use wf_view::Sim;

fn packs() -> Option<Packs> {
    let rom = wf_rom::testutil::rom()?;
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    Some(Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap())
}

fn story_sim(app: &wf_app::App) -> &StorySim {
    app.sim.as_any().and_then(|a| a.downcast_ref::<StorySim>()).expect("story sim")
}

/// The title's New Game replaces the world: the new field must still be the script host's (talk / defeat / level-up
/// events hold the field only with the mark).
#[test]
fn new_game_from_the_title_keeps_the_host_mark() {
    let Some(p) = packs() else { return };
    let mut app = wf_app::launcher::title_app(p, &[], 1).expect("title app");
    // START / A alternately until the prologue's first scene replaces the placeholder world
    let mut entered = false;
    for i in 0..2400u32 {
        app.inputs[0] = (if i % 40 < 4 { if (i / 40) % 2 == 0 { 8u16 } else { 1 } } else { 0 }).into();
        app.step();
        let w = story_sim(&app).world();
        if !wf_core::menu::active(w) && w.field.as_ref().is_some_and(|f| f.area.world == 0) {
            entered = true;
            break;
        }
    }
    assert!(entered, "the title did not start a new game");
    app.inputs[0] = 0u16.into();
    app.step();
    assert!(story_sim(&app).world().field.as_ref().is_some_and(|f| f.host), "the new game's field lost the host mark");
}

/// A load builds a new player object and a new form controller: the hit-damage remainder, the regen count and the
/// Super Saiyan drain fraction start at 0; a script fade-in pending from an earlier `warp_fade` does not survive it.
#[test]
fn a_load_resets_the_player_objects_accumulators() {
    let Some(p) = packs() else { return };
    let mut s = StorySim::new(&p, (1, 1), vec![], 3).expect("story");
    let mut ev = vec![];
    for _ in 0..40 {
        s.step(&[0], &mut ev);
    }
    {
        let w = &mut s.session.world;
        let pi = w.player_idx(0).expect("leader");
        let pl = w.pl(pi);
        pl.hp_acc.acc = 700;
        pl.regen_timer = 0x7000;
        pl.ss_acc = 0x8d43;
        w.field.as_mut().unwrap().script_fade_in = true;
        wf_core::field::load_area(w, AreaId { world: 1, area: 2, variant: 1 }, 40, 160, Some(3));
        // (the load's on-enter script is the host's to run inside a step; here the arrival is completed directly)
        wf_core::field::finish_arrival(w);
    }
    let w = s.world();
    let pl = w.pl_ref(w.player_idx(0).expect("leader after the load"));
    assert_eq!(pl.hp_acc.acc, 0, "hit-damage remainder carried across a load");
    assert!(pl.regen_timer < 0x1000, "regen count carried across a load: {}", pl.regen_timer);
    assert_eq!(pl.ss_acc, 0, "Super Saiyan drain fraction carried across a load");
    assert!(!w.field.as_ref().unwrap().script_fade_in, "script fade-in flag survived a load");
}

/// `set_character` takes the new character's own selected skill slot (record +0x15), not the previous leader's.
#[test]
fn set_character_takes_the_characters_own_skill_slot() {
    let Some(p) = packs() else { return };
    let mut s = StorySim::new(&p, (1, 1), vec![], 3).expect("story");
    let mut ev = vec![];
    for _ in 0..40 {
        s.step(&[0], &mut ev);
    }
    let w = &mut s.session.world;
    let pi = w.player_idx(0).expect("leader");
    w.pl(pi).sel = 2;
    let want = w.story.chars[1].sel.max(0) as usize;
    assert_ne!(want, 2, "the test needs Gohan's stored slot to differ");
    wf_script_core::natives::switch_character(w, 1).expect("set_character(1)");
    let pi = w.player_idx(0).expect("leader");
    assert_eq!(w.pl_ref(pi).sel, want, "the previous leader's skill slot followed to the new character");
}
