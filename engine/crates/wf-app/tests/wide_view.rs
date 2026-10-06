//! The zoomed-out PC view against the GBA view (headless, muted; skipped without the ROM). For every pixel-gate
//! segment (`data/segments/pixels.json`) at its locked frames:
//! 1. the wide view's centre 240x160 is the GBA view's picture, pixel for pixel. The corner-anchored overlays are
//!    placed for the wide screen on purpose and left out of both pictures: the HUD, the boss bars, the timer, the
//!    area banner, a top / bottom text box.
//! 2. nothing the world shows is culled or frozen for being off the GBA screen. Every live actor with a sprite is in
//!    the view's sprite list, and its drawn cel is the one `actor_cel` gives - the animation the game shows, wherever
//!    the actor stands (`render_view` never looks at the camera).
//! (user report 2026-10-05, "odd animations in the zoomed-out view": segments.md §12)

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::world::Body;
use wf_render::Renderer;
use wf_view::{FrameSel, RenderView};

/// the wide view in world pixels: the GBA screen centred with 80 / 64 px around it
const WIDE: (usize, usize) = (400, 288);

fn strip_overlays(v: &RenderView) -> RenderView {
    let mut o = v.clone();
    o.hud.clear();
    o.boss_bars.clear();
    o.timer = None;
    // the area banner and a top / bottom text box anchor to the wide view's edges (wf-render), as the HUD
    o.banner = None;
    // (the world map's bottom label stays at the wide view's bottom edge, as the other corner overlays)
    if let Some(m7) = o.map.as_mut().and_then(|m| m.mode7.as_mut()) {
        m7.label = None;
    }
    if o.dialog.as_ref().is_some_and(|d| d.position != wf_view::DialogPos::Middle) {
        o.dialog = None;
        o.dialog_anim = None;
    }
    o
}

#[test]
fn wide_view_centre_is_the_gba_view_and_nothing_is_culled() {
    let repo = wf_rom::testutil::repo_root();
    let Ok(lock) = std::fs::read_to_string(repo.join("data/segments/pixels.json")) else { return };
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    if let Ok(f) = wf_render::ui::Font::load(&packs, "dialog") {
        wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
    }
    if let Ok(b) = std::fs::read(content.join("scripts/bytecode.json")) {
        wf_import::host::set_step_code(&b);
    }
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, _) = wf_script::export::export(&rom, &set);
    // one renderer per picture size (each keeps its own caches)
    let mut gba = Renderer::new();
    let mut wide = Renderer::new();
    let (ox, oy) = ((WIDE.0 - 240) / 2, (WIDE.1 - 160) / 2);
    let mut fails = vec![];
    let (mut frames, mut offscreen) = (0usize, 0usize);
    // actors with no sprite at all (hidden by the scene / the game): [on screen, off screen]
    let mut undrawn = [0usize; 2];
    for (name, locked) in lock["segments"].as_object().unwrap() {
        let want: BTreeSet<usize> = locked.as_array().unwrap().iter().filter_map(|f| f[0].as_u64().map(|k| k as usize)).collect();
        let p: PathBuf = repo.join(format!("data/segments/{name}.json"));
        let Ok(doc) = wf_import::segment::load(&p) else {
            fails.push(format!("{name}: no segment"));
            continue;
        };
        let mut presenter = wf_core::view::Presenter::default();
        let mut bad: BTreeMap<usize, String> = BTreeMap::new();
        wf_import::segment::replay_with(&data, &files, &doc, &mut |f| match f {
            wf_import::segment::Frame::Before(_, w) => presenter.before_step(w),
            wf_import::segment::Frame::After(k, w) => {
                let mut view = RenderView::default();
                wf_core::view::render_view(w, &mut view);
                // (2) every live actor with a sprite is drawn, with its own cel (before the presenter's OBJ delay)
                let cam = view.cameras.first().copied().unwrap_or_default();
                for i in 0..w.ents.len() {
                    let e = &w.ents[i];
                    // (an object the sprite-memory purge took out of the world list is not drawn in the game either -
                    // `vram::suspended`; the pixel gate's per-frame lock reaches such frames: dr4_042 15-32 f180 ..)
                    if e.removed || !want.contains(&k) || wf_core::vram::suspended(w, e.id) {
                        continue;
                    }
                    let actor = matches!(e.body, Body::Player(_) | Body::Enemy(_) | Body::Npc(_));
                    let Some((cel, flip, _)) = actor.then(|| wf_core::view::actor_cel(w, i)).flatten() else { continue };
                    let draws: Vec<_> = view.sprites.iter().filter(|s| s.entity == e.id && s.sheet.starts_with("sprites/")).collect();
                    let Some(a) = w.actor(i) else { continue };
                    let off = a.px() < cam.x - 16 || a.px() > cam.x + 256 || a.py() < cam.y - 16 || a.py() > cam.y + 176;
                    if off {
                        offscreen += 1;
                    }
                    // an actor with no sprite at all: allowed on screen (the scene / the game hides it), never only
                    // because it stands outside the GBA screen
                    if draws.is_empty() {
                        undrawn[off as usize] += 1;
                        if off {
                            bad.entry(k).or_insert_with(|| format!("entity {} off-screen at ({}, {}) has no sprite", e.id, a.px(), a.py()));
                        }
                        continue;
                    }
                    let shown = draws.iter().any(|s| matches!(s.frame, FrameSel::Frame { index, flip: fl } if index == cel && fl == flip));
                    if !shown {
                        bad.entry(k).or_insert_with(|| format!("entity {} {} cel {cel} not drawn (draws {:?})", e.id, if off { "off-screen" } else { "on-screen" }, draws.iter().map(|s| &s.frame).collect::<Vec<_>>()));
                    }
                }
                presenter.apply(&mut view);
                if !want.contains(&k) {
                    return;
                }
                frames += 1;
                // (1) the wide picture's centre
                let v = strip_overlays(&view);
                let g = gba.render(&packs, &v, 0, 240, 160).to_vec();
                let wd = wide.render(&packs, &v, 0, WIDE.0, WIDE.1).to_vec();
                let mut first = None;
                let mut n = 0;
                for y in 0..160 {
                    for x in 0..240 {
                        if g[y * 240 + x] != wd[(y + oy) * WIDE.0 + x + ox] {
                            n += 1;
                            first.get_or_insert((x, y));
                        }
                    }
                }
                if n > 0 {
                    bad.entry(k).or_insert_with(|| format!("centre differs in {n} px, first at {:?}", first.unwrap()));
                }
            }
        });
        for (k, why) in bad.iter().take(3) {
            fails.push(format!("{name} f{k}: {why}"));
        }
    }
    eprintln!("wide view: {frames} frames, {offscreen} off-screen actor checks, actors without a sprite on / off screen {undrawn:?}");
    assert!(frames > 0);
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}
