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
        let p: PathBuf = repo.join(format!("data/segments/{name}.json"));
        let Ok(doc) = wf_import::segment::load(&p) else {
            fails.push(format!("{name}: no segment"));
            continue;
        };
        let mut presenter = wf_core::view::Presenter::default();
        let mut bad = vec![];
        let mut seen = 0;
        wf_import::segment::replay_with(&data, &files, &doc, &mut |f| match f {
            wf_import::segment::Frame::Before(_, w) => presenter.before_step(w),
            wf_import::segment::Frame::After(k, w) => {
                let Some(&h) = want.get(&k) else { return };
                seen += 1;
                let mut view = RenderView::default();
                wf_core::view::render_view(w, &mut view);
                presenter.apply(&mut view);
                if hash(renderer.render(&packs, &view, 0, 240, 160)) != h {
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
