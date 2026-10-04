//! The pixel gate (headless, muted; skipped without the ROM): every segment in `data/segments/pixels.json` (written
//! by `examples/pixels.rs` with `PIX_LOCK`: the segments whose sampled frames all matched the emulator's screen) is
//! replayed by the segment harness and rendered by wf-render at its locked video frames (the view + the presenter's
//! one-frame OBJ delay, 240x160); each picture's hash must equal the game's. A segment leaves the lock only by
//! regenerating it - a picture that stops matching fails here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::Renderer;
use wf_view::RenderView;

/// FNV-1a 64 over the ARGB words (little-endian), as `examples/pixels.rs`
fn hash(px: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in px {
        for b in c.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// `view` with `from`'s register state (as `examples/pixels.rs` `with_pal` / `tear_candidates`): `pal` the palette
/// (the field's fade / scale / BLDY, a menu's), `scroll` the BG scroll (cameras, the map's drift / shake)
fn with_prev(view: &RenderView, from: &RenderView, pal: bool, scroll: bool) -> RenderView {
    let mut o = view.clone();
    if pal {
        o.palette_fade = from.palette_fade;
        o.palette_scale = from.palette_scale;
        o.fade_black = from.fade_black;
        if let (Some(m), Some(f)) = (o.menu.as_mut(), from.menu.as_ref()) {
            m.fade = f.fade;
            m.fade_black = f.fade_black;
        }
    }
    if scroll {
        o.cameras = from.cameras.clone();
        o.map = from.map.clone();
    }
    o
}

/// a torn frame from our pictures and the lock's line specs: "" the new line, "o" / "s" / "b" the line with the
/// previous palette / BG scroll / both, else a hex mask of the pixels showing the previous palette ("o"'s picture)
fn compose(r: &mut Renderer, packs: &Packs, view: &RenderView, prev: &RenderView, new: &[u32], masks: &[String]) -> Vec<u32> {
    let mut pics: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    let mut pic = |r: &mut Renderer, l: &'static str| -> Vec<u32> {
        pics.entry(l)
            .or_insert_with(|| {
                let v = match l {
                    "s" => with_prev(view, prev, false, true),
                    "b" => with_prev(view, prev, true, true),
                    _ => with_prev(view, prev, true, false),
                };
                r.render(packs, &v, 0, 240, 160).to_vec()
            })
            .clone()
    };
    let mut out = new.to_vec();
    for (y, m) in masks.iter().enumerate().take(160) {
        let rg = y * 240..(y + 1) * 240;
        match m.as_str() {
            "" => {}
            "o" | "s" | "b" => {
                let l: &'static str = match m.as_str() {
                    "o" => "o",
                    "s" => "s",
                    _ => "b",
                };
                let p = pic(r, l);
                out[rg.clone()].copy_from_slice(&p[rg]);
            }
            _ => {
                let p = pic(r, "o");
                for x in 0..240 {
                    let b = u8::from_str_radix(m.get(x / 8 * 2..x / 8 * 2 + 2).unwrap_or("00"), 16).unwrap_or(0);
                    if b >> (x % 8) & 1 != 0 {
                        out[y * 240 + x] = p[y * 240 + x];
                    }
                }
            }
        }
    }
    out
}

#[test]
fn locked_segments_render_as_the_game() {
    let repo = wf_rom::testutil::repo_root();
    let Ok(lock) = std::fs::read_to_string(repo.join("data/segments/pixels.json")) else { return };
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    // the segment harness's shared extraction (as tests/segments.rs)
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
    let mut renderer = Renderer::new();
    let mut fails = vec![];
    let mut checked = 0;
    for (name, frames) in lock["segments"].as_object().unwrap() {
        let want: BTreeMap<usize, u64> = frames
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f[0].as_u64().unwrap() as usize, u64::from_str_radix(f[1].as_str().unwrap(), 16).unwrap()))
            .collect();
        // torn frames (the tear rule, core.md known differences): per line "" = our new-palette line, "o" = our
        // old-palette line (the previous frame's palette state), else a hex mask of the pixels showing the old one
        let masks: BTreeMap<usize, Vec<String>> = frames
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| Some((f[0].as_u64()? as usize, f.get(2)?.as_array()?.iter().map(|m| m.as_str().unwrap_or("").to_string()).collect())))
            .collect();
        let p: PathBuf = repo.join(format!("data/segments/{name}.json"));
        let Ok(doc) = wf_import::segment::load(&p) else {
            fails.push(format!("{name}: no segment"));
            continue;
        };
        let mut presenter = wf_core::view::Presenter::default();
        let mut bad = vec![];
        let mut seen = 0;
        let mut prev: Option<RenderView> = None;
        wf_import::segment::replay_with(&data, &files, &doc, &mut |f| match f {
            wf_import::segment::Frame::Before(_, w) => presenter.before_step(w),
            wf_import::segment::Frame::After(k, w) => {
                let mut view = RenderView::default();
                wf_core::view::render_view(w, &mut view);
                let before = prev.replace(view.clone());
                let Some(&h) = want.get(&k) else { return };
                seen += 1;
                presenter.apply(&mut view);
                let new = renderer.render(&packs, &view, 0, 240, 160).to_vec();
                let ok = hash(&new) == h
                    || match (masks.get(&k), before) {
                        (Some(m), Some(b)) => hash(&compose(&mut renderer, &packs, &view, &b, &new, m)) == h,
                        _ => false,
                    };
                if !ok {
                    bad.push(k);
                }
            }
        });
        checked += seen;
        if seen != want.len() {
            fails.push(format!("{name}: reached {seen} of {} locked frames", want.len()));
        }
        if !bad.is_empty() {
            fails.push(format!("{name}: frames {bad:?} differ from the game's picture"));
        }
    }
    eprintln!("pixel gate: {checked} frames of {} segments", lock["segments"].as_object().unwrap().len());
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}
