//! "PC wide (auto)" zoom over the gated segments (headless): how often each rule changes the zoom, per window size.
//! `per_frame` = `wf_render::auto_zoom` on every frame's map (the old rule), `held` = `wf_render::AutoZoom` (the
//! zoom picked when the map changes, kept through frames without a map). Prints the changes and the frames between
//! them.
//!   cargo run --release -p wf-app --example zoom_stats [segment-name-filter]

use std::collections::BTreeMap;

use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::Renderer;
use wf_view::RenderView;

fn main() {
    let repo = wf_rom::testutil::repo_root();
    let filter = std::env::args().nth(1).unwrap_or_default();
    let rom = wf_rom::testutil::rom().expect("rom");
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    if let Ok(b) = std::fs::read(content.join("scripts/bytecode.json")) {
        wf_import::host::set_step_code(&b);
    }
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, _) = wf_script::export::export(&rom, &set);
    let mut r = Renderer::new();
    let wins = [(1920u32, 1080u32), (1280, 800), (2560, 1440)];
    // per window: (old changes, new changes, frames)
    let mut tot: BTreeMap<(u32, u32), (usize, usize, usize, usize, usize)> = BTreeMap::new();
    let mut names: Vec<String> = std::fs::read_dir(repo.join("data/segments"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".json") && !n.contains("pixels") && !n.contains("cel_exact") && n.contains(filter.as_str()))
        .collect();
    names.sort();
    for name in names {
        let Ok(doc) = wf_import::segment::load(&repo.join("data/segments").join(&name)) else { continue };
        // the map shown per frame (None: a menu / a black load / no field)
        let mut maps: Vec<Option<(String, (i32, i32))>> = vec![];
        wf_import::segment::replay_with(&data, &files, &doc, &mut |f| {
            if let wf_import::segment::Frame::After(_, w) = f {
                let mut v = RenderView::default();
                wf_core::view::render_view(w, &mut v);
                let m = if v.menu.is_some() {
                    None
                } else {
                    v.map.as_ref().filter(|m| !m.id.is_empty()).and_then(|m| r.map(&packs, &m.id).map(|g| (m.id.clone(), (g.world_w, g.world_h))))
                };
                maps.push(m);
            }
        });
        for &(ww, wh) in &wins {
            let (mut old_prev, mut new_prev) = (0, 0);
            let (mut co, mut cn) = (0, 0);
            // a variant for comparison: the held zoom capped to one step above the window's base zoom
            let base = wf_render::auto_zoom(ww, wh, None);
            let (mut cap_prev, mut cc) = (0, 0);
            let mut az = wf_render::AutoZoom::default();
            for m in &maps {
                let zo = wf_render::auto_zoom(ww, wh, m.as_ref().map(|m| m.1));
                let zn = az.zoom(ww, wh, m.as_ref().map(|m| (m.0.as_str(), m.1)));
                if old_prev != 0 && zo != old_prev {
                    co += 1;
                }
                if new_prev != 0 && zn != new_prev {
                    cn += 1;
                    if std::env::var_os("ZOOM_TRACE").is_some() && (ww, wh) == wins[0] {
                        println!("  {name}: held {new_prev} -> {zn} at map {:?}", m);
                    }
                }
                let zc = zn.clamp(base, base + 1);
                if cap_prev != 0 && zc != cap_prev {
                    cc += 1;
                }
                cap_prev = zc;
                old_prev = zo;
                new_prev = zn;
            }
            let t = tot.entry((ww, wh)).or_default();
            t.0 += co;
            t.1 += cn;
            t.2 += maps.len();
            t.3 += 1;
            t.4 += cc;
            if co > 0 && (ww, wh) == wins[0] {
                println!("{name}: {} frames, zoom changes per frame rule {co}, held rule {cn}", maps.len());
            }
        }
    }
    for ((w, h), (co, cn, n, segs, cc)) in tot {
        println!("window {w}x{h}: {segs} segments, {n} frames: zoom changes per-frame rule {co}, held rule {cn}, held + capped to base..base+1 {cc}");
    }
}
