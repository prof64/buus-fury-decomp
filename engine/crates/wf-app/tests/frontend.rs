//! End-to-end checks of the frontend against freshly extracted content (skipped without the ROM):
//! * the sound bank rebuilt from `audio/*` content plays bit-identically to the bank read from the ROM;
//! * the demo world is deterministic (same inputs -> same state hash and the same pixels);
//! * the renderer never mutates the simulation (render twice -> same frame, same state).

use wf_app::demo::{DemoConfig, DemoWorld};
use wf_app::wf_content::{audio::load_bank, FileSource, MemSource, Packs};
use wf_view::{buttons as b, RenderView, Sim};

fn packs(only: &str) -> Option<Packs> {
    let rom = wf_rom::testutil::rom()?;
    let (c, _) = wf_extract::extract_rom(&rom, &wf_extract::Options { only: only.split(',').map(String::from).collect() }, &mut |_| {});
    Some(Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap())
}

#[test]
fn content_bank_plays_like_rom_bank() {
    let Some(p) = packs("audio") else { return };
    let rom = wf_rom::testutil::rom().unwrap();
    let a = wf_audio::bank::SoundBank::from_rom(&rom);
    let b = load_bank(&p).unwrap();
    assert_eq!(a.songs.len(), b.songs.len());
    assert_eq!(a.sfx.len(), b.sfx.len());
    for id in [1usize, 28, 44] {
        assert_eq!(wf_audio::render_song(&a, id, 6.0), wf_audio::render_song(&b, id, 6.0), "song {id}");
    }
    for id in [20u8, 48, 59] {
        assert_eq!(wf_audio::render_sfx(&a, id, 64, 2.0), wf_audio::render_sfx(&b, id, 64, 2.0), "sfx {id}");
    }
}

#[test]
fn demo_is_deterministic_and_render_is_pure() {
    let Some(p) = packs("tables,sprites,effects,items,maps,ui") else { return };
    let script = |f: u32| match f % 200 {
        0..=49 => b::RIGHT,
        50..=99 => b::DOWN | b::LEFT,
        100..=104 | 110..=150 => b::UP,
        _ => 0,
    };
    let run = || {
        let mut w = DemoWorld::new(&p, &DemoConfig { players: 2, ..Default::default() }).unwrap();
        let mut ev = Vec::new();
        for f in 0..600 {
            w.step(&[script(f), script(f + 37)], &mut ev);
        }
        w
    };
    let (w1, w2) = (run(), run());
    assert_eq!(w1.state_hash(), w2.state_hash());
    let mut v = RenderView::default();
    w1.view(&mut v);
    assert_eq!(v.cameras.len(), 2);
    let before = w1.state_hash();
    let mut r = wf_render::Renderer::new();
    let h1 = wf_render::hash_fb(r.render(&p, &v, 0, 426, 240));
    let h2 = wf_render::hash_fb(r.render(&p, &v, 0, 426, 240));
    assert_eq!(h1, h2);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(before, w1.state_hash());
    // player 2's viewport differs (own camera)
    let h3 = wf_render::hash_fb(r.render(&p, &v, 1, 426, 240));
    assert_ne!(h1, h3);
}
