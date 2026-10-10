//! The real simulation (`wf-core`) through the frontend, headless and muted: an autopilot that only looks at the
//! `RenderView` (like a player looking at the screen) walks to the enemy and attacks. Checks that the battle
//! renders (player + enemy sprites from content, no missing sheets), that hits produce the game's SFX events and
//! that the muted sound driver plays them. `WF_SHOT_DIR=<dir>` writes PPM frames.

use wf_app::session::Session;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_app::App;
use wf_view::{buttons as b, AudioEvent, RenderView};

fn packs() -> Option<Packs> {
    let rom = wf_rom::testutil::rom()?;
    let opts = wf_extract::Options { only: ["tables", "sprites", "effects", "items", "maps", "audio", "ui"].map(String::from).to_vec() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    Some(Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap())
}

/// (player, nearest enemy) positions from the view: the player is the actor nearest the camera's focus point
fn actors(v: &RenderView) -> Option<((i32, i32), Option<(i32, i32)>)> {
    let cam = v.cameras.first()?;
    let focus = (cam.x + 120, cam.y + 84);
    let bodies: Vec<(u32, i32, i32)> = v.sprites.iter().filter(|s| s.sheet.starts_with("sprites/")).map(|s| (s.entity, s.x, s.y)).collect();
    let d = |p: (i32, i32), q: (i32, i32)| (p.0 - q.0).pow(2) + (p.1 - q.1).pow(2);
    let player = *bodies.iter().min_by_key(|s| d((s.1, s.2), focus))?;
    let enemy = bodies.iter().filter(|s| s.0 != player.0).min_by_key(|s| d((s.1, s.2), (player.1, player.2))).map(|s| (s.1, s.2));
    Some(((player.1, player.2), enemy))
}

#[test]
fn battle_renders_and_plays_sfx() {
    let Some(p) = packs() else { return };
    let cfg = wf_app::battle::config("forest", "t68", 1, "goku", None, None, 1, 7);
    let bank = wf_app::wf_content::audio::load_bank(&p).ok().map(|b| &*Box::leak(Box::new(b)));
    let app = App::battle(p, cfg).expect("battle");
    let mut s = Session::new(app, bank);
    let shots = std::env::var("WF_SHOT_DIR").ok();
    let mut saw_enemy = false;
    let mut max_voices = 0;
    let mut first_hit = None;
    // when blocked (no movement for 8 frames), move along the other axis for a while
    let (mut last, mut still, mut detour) = ((0, 0), 0, 0u32);
    let mut detour_dir = 0u16;
    for f in 0..3600u32 {
        let mut v = RenderView::default();
        s.app.sim.view(&mut v);
        let mut keys = 0;
        if f % 300 == 0 {
            eprintln!("f{f} {:?} sprites {:?}", actors(&v), v.sprites.iter().map(|s| (s.entity, s.sheet.as_str(), s.x, s.y)).collect::<Vec<_>>());
        }
        if let Some((pl, Some(en))) = actors(&v) {
            saw_enemy = true;
            let (dx, dy) = (en.0 - pl.0, en.1 - pl.1);
            still = if pl == last { still + 1 } else { 0 };
            last = pl;
            if still > 8 && detour == 0 && (dx.abs() > 20 || dy.abs() > 6) {
                detour = 48;
                detour_dir = if dx.abs() > 20 { if dy >= 0 { b::DOWN } else { b::UP } } else if dx >= 0 { b::RIGHT } else { b::LEFT };
                if (f / 97) % 2 == 1 {
                    detour_dir = match detour_dir {
                        b::DOWN => b::UP,
                        b::UP => b::DOWN,
                        b::LEFT => b::RIGHT,
                        _ => b::LEFT,
                    };
                }
            }
            if detour > 0 {
                detour -= 1;
                keys = detour_dir;
            } else if dx.abs() > 20 {
                keys |= if dx > 0 { b::RIGHT } else { b::LEFT };
            } else if dy.abs() > 6 {
                keys |= if dy > 0 { b::DOWN } else { b::UP };
            } else {
                // face the enemy and punch
                keys |= if dx > 0 { b::RIGHT } else { b::LEFT };
                if f % 12 < 2 {
                    keys = b::A;
                }
            }
        }
        s.step(&[keys]);
        max_voices = max_voices.max(s.sfx_voices());
        if first_hit.is_none() && s.audio_log.iter().any(|(_, e)| matches!(e, AudioEvent::PlaySfx { id: 48 | 49, .. })) {
            first_hit = Some(f);
            if let Some(d) = &shots {
                let shot = s.screenshot(0, 960, 640);
                std::fs::write(format!("{d}/battle_hit.ppm"), shot.ppm(2)).unwrap();
            }
        }
        if let Some(d) = &shots {
            if f % 600 == 0 {
                let shot = s.screenshot(0, 960, 640);
                std::fs::write(format!("{d}/battle_{f:04}.ppm"), shot.ppm(2)).unwrap();
            }
        }
    }
    let sfx: std::collections::BTreeSet<u8> = s.audio_log.iter().filter_map(|(_, e)| if let AudioEvent::PlaySfx { id, .. } = e { Some(*id) } else { None }).collect();
    println!("enemy seen {saw_enemy}, first hit frame {first_hit:?}, sfx ids {sfx:?}, max busy SFX voices {max_voices}");
    assert!(saw_enemy, "no enemy sprite in the view");
    assert!(s.app.renderer.errors.is_empty(), "{:?}", s.app.renderer.errors);
    assert!(matches!(s.audio_log.first(), Some((_, AudioEvent::PlaySong(_)))));
    assert!(first_hit.is_some(), "no hit SFX (48/49) in {sfx:?}");
    assert!(max_voices > 0, "the muted driver never started an SFX voice");
}
