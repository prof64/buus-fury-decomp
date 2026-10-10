//! Story mode headless + muted: a new game at Snakeway with the game's scripts (Rhai, from content) runs, renders
//! without missing content, plays the area song and stays deterministic.

use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_view::{buttons as b, AudioEvent, RenderView, Sim, SimEvent};

#[test]
fn story_new_game_runs_scripts_and_renders() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let run = || {
        let mut s = StorySim::new(&packs, (1, 1), vec![], 3).expect("story");
        let mut ev = vec![];
        for f in 0..600u32 {
            let k = match f % 240 {
                0..=59 => b::UP,
                60..=119 => b::RIGHT,
                120..=125 => b::A,
                _ => b::LEFT,
            };
            s.step(&[k], &mut ev);
        }
        (s, ev)
    };
    let (s, ev) = run();
    let (s2, _) = run();
    assert_eq!(s.state_hash(), s2.state_hash());
    assert!(s.errors.is_empty(), "script errors: {:?}", s.errors);
    assert!(ev.iter().any(|e| matches!(e, SimEvent::Audio(AudioEvent::PlaySong(15)))), "Snakeway song");
    let mut v = RenderView::default();
    s.view(&mut v);
    assert!(v.map.is_some() && !v.sprites.is_empty());
    let mut r = wf_render::Renderer::new();
    r.render(&packs, &v, 0, 240, 160);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}
