//! The pixel gate (headless, muted; skipped without the ROM): every segment in `data/segments/pixels.json` (and
//! `pixels_coverage.json`, the coverage pass's scenes; written
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

/// a torn frame from our pictures and the lock's line specs: "" the new line, "v" the picture before the running
/// iteration's step, "k" black (a fade to black's last step before a long iteration), "o" / "s" / "b" the line with the
/// previous palette / BG scroll / both, else a hex mask of the pixels showing the previous palette ("o"'s picture)
fn compose(r: &mut Renderer, packs: &Packs, view: &RenderView, prev: &RenderView, iter: Option<&RenderView>, new: &[u32], masks: &[String]) -> Vec<u32> {
    let mut pics: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    let mut pic = |r: &mut Renderer, l: &'static str| -> Vec<u32> {
        pics.entry(l)
            .or_insert_with(|| {
                let v = match l {
                    "s" => with_prev(view, prev, false, true),
                    "b" => with_prev(view, prev, true, true),
                    "v" => iter.cloned().unwrap_or_else(|| view.clone()),
                    // the timer's previous value (its digits written into the tiles while the screen is drawn)
                    "t" => {
                        let mut v = view.clone();
                        if prev.timer.is_some() {
                            v.timer = prev.timer;
                        }
                        // (the energy sphere's tilemap is rewritten during the frame too: its previous radius)
                        v.sphere = prev.sphere.clone();
                        v
                    }
                    // both at once: the previous timer / sphere and the previous palette (a screen_flash's step in
                    // the same frame as the sphere's tilemap - examples/pixels.rs "u")
                    "u" => {
                        let mut v = view.clone();
                        if prev.timer.is_some() {
                            v.timer = prev.timer;
                        }
                        v.sphere = prev.sphere.clone();
                        with_prev(&v, prev, true, false)
                    }
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
            // (the late present of a fade to black's last step: the line black)
            "k" => out[rg].iter_mut().for_each(|p| *p = 0xFF00_0000),
            "o" | "s" | "b" | "v" | "t" | "u" => {
                let l: &'static str = match m.as_str() {
                    "o" => "o",
                    "s" => "s",
                    "v" => "v",
                    "t" => "t",
                    "u" => "u",
                    _ => "b",
                };
                let p = pic(r, l);
                out[rg.clone()].copy_from_slice(&p[rg]);
            }
            _ => {
                // per pixel: the previous palette's picture, then (after ";t") the timer's previous value
                let (mo, mt) = m.split_once(";t").unwrap_or((m.as_str(), ""));
                for (mk, l) in [(mo, "o"), (mt, "t")] {
                    if mk.is_empty() {
                        continue;
                    }
                    let p = pic(r, l);
                    for x in 0..240 {
                        let b = u8::from_str_radix(mk.get(x / 8 * 2..x / 8 * 2 + 2).unwrap_or("00"), 16).unwrap_or(0);
                        if b >> (x % 8) & 1 != 0 {
                            out[y * 240 + x] = p[y * 240 + x];
                        }
                    }
                }
            }
        }
    }
    out
}

/// the stat-points mark with this frame's cel: its draw (0x0800B55C) copies the new cel's tiles to VRAM at once, so a
/// draw made before the beam reaches the mark's lines (y 9..15) shows on this frame under last frame's OAM (as the
/// timer's digits); `None` when the cel did not change
pub fn stat_now(view: &RenderView, cur: &[Option<u8>]) -> Option<RenderView> {
    let mut v = view.clone();
    let mut changed = false;
    for (h, c) in v.hud.iter_mut().zip(cur) {
        if h.stat_mark.is_some() && c.is_some() && h.stat_mark != *c {
            h.stat_mark = *c;
            changed = true;
        }
    }
    changed.then_some(v)
}

#[test]
fn locked_segments_render_as_the_game() {
    let repo = wf_rom::testutil::repo_root();
    let Ok(lock) = std::fs::read_to_string(repo.join("data/segments/pixels.json")) else { return };
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let mut lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    // the coverage pass's scenes (`data/segments/cov_*.json`, engine/docs/coverage.md): locked in their own file,
    // written the same way, until the main lock is re-made with them (a segment in both is the main lock's)
    if let Some(more) = std::fs::read_to_string(repo.join("data/segments/pixels_coverage.json")).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
        let segs = lock["segments"].as_object_mut().unwrap();
        for (k, v) in more["segments"].as_object().into_iter().flatten() {
            segs.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
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
    // segments in parallel (each worker its own renderers and pack reader; every exact sampled frame is locked, so the
    // gate replays most segments: ~18,600 frames)
    let names: Vec<(&String, &serde_json::Value)> = lock["segments"].as_object().unwrap().iter().collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let fails = std::sync::Mutex::new(Vec::<String>::new());
    let checked = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(12);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| {
                let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
                let mut renderer = Renderer::new();
                // (the tear pictures of earlier states have their own renderer: the main one's caches stay the frame's)
                let mut tear_renderer = Renderer::new();
                loop {
                    let k0 = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&(name, frames)) = names.get(k0) else { break };
                    let (seen, msgs) = check_segment(&repo, &data, &files, &packs, &mut renderer, &mut tear_renderer, name, frames);
                    checked.fetch_add(seen, std::sync::atomic::Ordering::Relaxed);
                    fails.lock().unwrap().extend(msgs);
                }
            });
        }
    });
    let checked = checked.into_inner();
    let mut fails = fails.into_inner().unwrap();
    fails.sort();
    eprintln!("pixel gate: {checked} frames of {} segments", lock["segments"].as_object().unwrap().len());
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

/// one locked segment: (frames reached, failure messages)
#[allow(clippy::too_many_arguments)]
fn check_segment(repo: &std::path::Path, data: &std::sync::Arc<wf_core::content::GameData>, files: &BTreeMap<String, String>, packs: &Packs, renderer: &mut Renderer, tear_renderer: &mut Renderer, name: &str, frames: &serde_json::Value) -> (usize, Vec<String>) {
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
        return (0, vec![format!("{name}: no segment")]);
    };
    let mut presenter = wf_core::view::Presenter::default();
    let mut bad = vec![];
    let mut seen = 0;
    let mut prev: Option<RenderView> = None;
    // the late present ("v" lines): the picture before the running iteration's step (as examples/pixels.rs)
    let mut last_view: Option<RenderView> = None;
    let mut iter_view: Option<RenderView> = None;
    wf_import::segment::replay_with(&data, &files, &doc, &mut |f| match f {
        wf_import::segment::Frame::Before(_, w) => {
            presenter.before_step(w);
            iter_view = last_view.clone();
        }
        wf_import::segment::Frame::After(k, w) => {
            let mut view = RenderView::default();
            wf_core::view::render_view(w, &mut view);
            let before = prev.replace(view.clone());
            let cur_stat: Vec<Option<u8>> = view.hud.iter().map(|h| h.stat_mark).collect();
            presenter.apply(&mut view);
            last_view = Some(view.clone());
            let Some(&h) = want.get(&k) else { return };
            seen += 1;
            let new = renderer.render(packs, &view, 0, 240, 160).to_vec();
            let ok = hash(&new) == h
                || stat_now(&view, &cur_stat).is_some_and(|v| hash(&renderer.render(packs, &v, 0, 240, 160).to_vec()) == h)
                || match (masks.get(&k), before) {
                    // (the torn lines over our picture - or over the stat mark's early-copy picture, which the board
                    // takes first when it is closer)
                    (Some(m), Some(b)) => {
                        hash(&compose(tear_renderer, packs, &view, &b, iter_view.as_ref(), &new, m)) == h
                            || stat_now(&view, &cur_stat).is_some_and(|v| {
                                let sn = renderer.render(packs, &v, 0, 240, 160).to_vec();
                                hash(&compose(tear_renderer, packs, &view, &b, iter_view.as_ref(), &sn, m)) == h
                            })
                    }
                    _ => false,
                };
            if !ok {
                bad.push(k);
            }
        }
    });
    let mut fails = vec![];
    if seen != want.len() {
        fails.push(format!("{name}: reached {seen} of {} locked frames", want.len()));
    }
    if !bad.is_empty() {
        fails.push(format!("{name}: frames {bad:?} differ from the game's picture"));
    }
    (seen, fails)
}
