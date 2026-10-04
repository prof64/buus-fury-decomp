//! The picture against the game (headless, muted): `cargo run --release -p wf-app --example pixels -- SEG.json..`.
//! Each segment is replayed by the segment harness (`wf_import::segment::replay_with`, the same steps as the gates);
//! at the sampled video frames of `<fb dir>/<name>.fb` (the emulator's screen: `work/superbuu/pix/dump.py`) our view
//! (`wf_core::view::render_view` + the presenter's one-frame OBJ delay) is rendered by wf-render at 240x160 and
//! compared pixel for pixel. Only frames inside the segment's matched prefix count (an actor difference is the
//! harness's, not the picture's). Per segment: frames compared, pixel-exact frames, and the differing pixels sorted
//! into classes; `PIX_OUT=dir` writes `game | ours | diff` PNGs of differing frames (`PIX_MAX` per segment, 4),
//! `PIX_FB=dir` the .fb directory, `PIX_JSON=file` the per-frame results, `PIX_LOCK=file` the pixel gate's lock
//! (`data/segments/pixels.json`, read by `tests/pixels.rs`): every segment whose sampled frames are all exact, with
//! each frame's hash of the game's picture.

use std::collections::BTreeMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::Renderer;
use wf_view::RenderView;

const W: usize = 240;
const H: usize = 160;

fn read_fb(p: &Path) -> Option<(Vec<usize>, Vec<Vec<u32>>)> {
    let f = std::fs::File::open(p).ok()?;
    let mut r = std::io::BufReader::new(f);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let head: serde_json::Value = serde_json::from_str(&line).ok()?;
    let frames: Vec<usize> = head["frames"].as_array()?.iter().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
    let sizes: Vec<usize> = head["sizes"].as_array()?.iter().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
    let mut out = vec![];
    for &n in &sizes {
        let mut z = vec![0u8; n];
        r.read_exact(&mut z).ok()?;
        let b = miniz_oxide::inflate::decompress_to_vec_zlib(&z).ok()?;
        out.push(b.chunks(3).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect());
    }
    Some((frames, out))
}

/// FNV-1a 64 over the picture's ARGB words (little-endian): the pixel gate's frame hash (tests/pixels.rs)
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

/// the view's register state the game rewrites from its main loop while the screen is drawn: the palette (the
/// field's fade / scale / BLDY, a menu screen's own) and the BG scroll (the cameras, the map's drift / shake)
#[derive(Clone, PartialEq)]
struct Pal {
    fade: Option<wf_view::PaletteFade>,
    scale: Option<wf_view::PaletteFade>,
    black: u8,
    menu: Option<(Option<wf_view::PaletteFade>, u8)>,
    cameras: Vec<wf_view::CameraView>,
    map: Option<wf_view::MapView>,
}

fn pal_of(v: &RenderView) -> Pal {
    Pal { fade: v.palette_fade, scale: v.palette_scale, black: v.fade_black, menu: v.menu.as_ref().map(|m| (m.fade, m.fade_black)), cameras: v.cameras.clone(), map: v.map.clone() }
}

fn with_pal(v: &RenderView, p: &Pal) -> RenderView {
    let mut o = v.clone();
    o.palette_fade = p.fade;
    o.palette_scale = p.scale;
    o.fade_black = p.black;
    if let (Some(m), Some((f, b))) = (o.menu.as_mut(), p.menu) {
        m.fade = f;
        m.fade_black = b;
    }
    o.cameras = p.cameras.clone();
    o.map = p.map.clone();
    o
}

/// The tear rule (core.md, known differences): the game rewrites the palette from its main loop while the screen is
/// drawn, so the lines drawn before (or during) the rewrite show the frame's old palette - a CPU-time artifact the
/// app does not recreate. A frame whose palette state changed since the previous frame is accepted when every line
/// equals our render with the new palette or with the old one; the per-line hashes of the game's picture are what
/// the gate locks for it. Returns those hashes when the frame is torn-accepted (some line only matches the old)
/// our pictures of the frame with the previous frame's register state: "o" the palette, "s" the BG scroll, "b" both
/// (the ones that differ from the new state)
fn tear_candidates(renderer: &mut Renderer, packs: &Packs, view: &RenderView, prev: &Pal) -> Vec<(&'static str, Vec<u32>)> {
    let now = pal_of(view);
    let scroll_moved = (&prev.cameras, &prev.map) != (&now.cameras, &now.map);
    let pal_moved = (prev.fade, prev.scale, prev.black, prev.menu) != (now.fade, now.scale, now.black, now.menu);
    let mut out = vec![];
    if pal_moved {
        let p = Pal { cameras: now.cameras.clone(), map: now.map.clone(), ..prev.clone() };
        out.push(("o", renderer.render(packs, &with_pal(view, &p), 0, W, H).to_vec()));
    }
    if scroll_moved {
        let s = Pal { fade: now.fade, scale: now.scale, black: now.black, menu: now.menu, ..prev.clone() };
        out.push(("s", renderer.render(packs, &with_pal(view, &s), 0, W, H).to_vec()));
    }
    if scroll_moved && pal_moved {
        out.push(("b", renderer.render(packs, &with_pal(view, prev), 0, W, H).to_vec()));
    }
    out
}

fn torn(game: &[u32], new: &[u32], olds: &[(&str, Vec<u32>)]) -> Option<Vec<String>> {
    let mut any_old = false;
    // a whole line may show a previous register state: the candidates in order - "o" the previous palette, "s" the
    // previous BG scroll, "b" both. The line drawn while the palette rewrite runs mixes the two palettes per colour
    // (each pixel's entry flips at its own time): per pixel then against the previous palette's picture ("o") - a
    // hex mask of the pixels showing it
    let mut masks = vec![];
    let px_old = olds.iter().find(|c| c.0 == "o").map(|c| &c.1);
    'line: for y in 0..H {
        let (g, nw) = (&game[y * W..(y + 1) * W], &new[y * W..(y + 1) * W]);
        if g == nw {
            masks.push(String::new());
            continue;
        }
        for (label, o) in olds {
            if g == &o[y * W..(y + 1) * W] {
                any_old = true;
                masks.push((*label).to_string());
                continue 'line;
            }
        }
        let Some(old) = px_old else { return None };
        let od = &old[y * W..(y + 1) * W];
        let mut bits = vec![0u8; W / 8];
        for x in 0..W {
            if g[x] == nw[x] {
                continue;
            }
            if g[x] != od[x] {
                return None;
            }
            bits[x / 8] |= 1 << (x % 8);
        }
        any_old = true;
        masks.push(bits.iter().map(|b| format!("{b:02x}")).collect());
    }
    any_old.then_some(masks)
}

fn save(p: &Path, imgs: &[&[u32]]) {
    let (w, h) = (W * imgs.len(), H);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for im in imgs {
            for x in 0..W {
                let c = im[y * W + x];
                rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    let f = std::fs::File::create(p).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

/// the class of a differing frame (the largest share of its differing pixels)
fn classify(view: &RenderView, game: &[u32], ours: &[u32]) -> (String, usize) {
    let mut n = 0;
    let mut in_sprite = 0;
    let mut in_hud = 0;
    let mut in_dialog = 0;
    let cam = view.cameras.first().copied().unwrap_or_default();
    // the sprites' anchor boxes (a generous 64x64 around the anchor: the renderer's own rect is not exposed)
    let boxes: Vec<(i32, i32, i32, i32)> = view.sprites.iter().map(|s| (s.x - cam.x - 32, s.y - cam.y - 56, s.x - cam.x + 32, s.y - cam.y + 16)).collect();
    let mut rows = vec![0usize; H];
    let game_flat = {
        let c0 = game[0];
        game.iter().all(|&c| c == c0)
    };
    let ours_flat = {
        let c0 = ours[0];
        ours.iter().all(|&c| c == c0)
    };
    for y in 0..H {
        for x in 0..W {
            let k = y * W + x;
            if game[k] == ours[k] {
                continue;
            }
            n += 1;
            rows[y] += 1;
            let (xi, yi) = (x as i32, y as i32);
            if !view.hud.is_empty() && y < 24 {
                in_hud += 1;
            } else if view.dialog.is_some() && (y >= 104 || y < 56) {
                in_dialog += 1;
            } else if boxes.iter().any(|b| xi >= b.0 && xi < b.2 && yi >= b.1 && yi < b.3) {
                in_sprite += 1;
            }
        }
    }
    if n == 0 {
        return ("exact".into(), 0);
    }
    let class = if game_flat || ours_flat {
        "flat screen (fade / black / load)"
    } else if n * 2 > W * H {
        "whole screen (palette / fade / BG scroll)"
    } else if in_hud * 2 > n {
        "HUD"
    } else if in_dialog * 2 > n {
        "text box / dialog"
    } else if in_sprite * 2 > n {
        "sprites (OBJ)"
    } else {
        "background (BG layers)"
    };
    (class.to_string(), n)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = wf_rom::testutil::repo_root();
    let fb_dir = std::env::var("PIX_FB").map(PathBuf::from).unwrap_or_else(|_| repo.join("work/superbuu/pix/fb"));
    let out_dir = std::env::var("PIX_OUT").ok().map(PathBuf::from);
    let max_png: usize = std::env::var("PIX_MAX").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    if let Some(o) = &out_dir {
        std::fs::create_dir_all(o).unwrap();
    }
    // the segment harness's shared extraction (re-extracted when the extractor changed, as tests/segments.rs)
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
    let rom = wf_rom::testutil::rom().expect("rom");
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, _) = wf_script::export::export(&rom, &set);
    let mut renderer = Renderer::new();
    let mut all: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut json_out = vec![];
    let mut lock = serde_json::Map::new();
    let (mut tot_frames, mut tot_exact, mut tot_torn) = (0usize, 0usize, 0usize);
    for a in &args {
        let p = PathBuf::from(a);
        let Ok(doc) = wf_import::segment::load(&p) else {
            println!("{a}: no segment");
            continue;
        };
        let name = doc["name"].as_str().unwrap_or("?").to_string();
        let Some((want, game)) = read_fb(&fb_dir.join(format!("{name}.fb"))) else {
            println!("{name}: no screen dump");
            continue;
        };
        let at: BTreeMap<usize, usize> = want.iter().enumerate().map(|(i, &k)| (k, i)).collect();
        let mut presenter = wf_core::view::Presenter::default();
        let mut results: Vec<(usize, String, usize)> = vec![];
        let mut pngs = 0;
        // the previous video frame's palette state (the tear rule) and the torn-accepted frames' line hashes
        let mut prev_pal: Option<Pal> = None;
        let mut torn_rows: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        // the late present (tear rule): the picture shown before the running iteration's step (the previous video
        // frame's, presenter applied) and the frame the last step ran in - a sampled frame after it with no step is
        // inside the iteration, whose redraw the game shows only at its end
        let mut last_view: Option<RenderView> = None;
        let mut iter_view: Option<RenderView> = None;
        let mut stepped = usize::MAX;
        let rep = wf_import::segment::replay_with(&data, &files, &doc, &mut |f| match f {
            wf_import::segment::Frame::Before(k, w) => {
                presenter.before_step(w);
                iter_view = last_view.clone();
                stepped = k;
            }
            wf_import::segment::Frame::After(k, w) => {
                // PIX_TRACE=a-b: the view's screen-space extras per frame (boss bars) for frame-by-frame checks
                if let Some((a, b)) = std::env::var("PIX_TRACE").ok().and_then(|v| {
                    let (a, b) = v.split_once('-')?;
                    Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
                }) {
                    if (a..=b).contains(&k) {
                        let mut view = RenderView::default();
                        wf_core::view::render_view(w, &mut view);
                        for (i, e) in w.ents.iter().enumerate().filter(|(_, e)| !e.removed) {
                            if let Some(a) = w.actor(i) {
                                eprintln!("  ent {} sprite {:?} anim {:?} t {} facing {} shown {:?}", e.id, a.sprite, a.anim, a.anim_t, a.facing, wf_core::scene::shown_anim(w, i));
                            }
                        }
                        eprintln!("trace {k} frame {} fc0 {} play {} menu {} anim {:?} stat {:?} boss {:?} screen {:?} fade_black {} pal {:?}", w.frame, w.g.fc0, w.story.play_frames, view.menu.is_some(), view.map.as_ref().map(|m| (m.anim_frame, m.tile_frame)), (view.hud.first().map(|h| h.stat_mark), w.field.as_ref().and_then(|f| f.vram.as_ref()).map(|v| v.stat_at)), view.boss_bars, w.scene.screen, view.fade_black, view.palette_fade);
                    }
                }
                let mut view = RenderView::default();
                wf_core::view::render_view(w, &mut view);
                let old_pal = prev_pal.replace(pal_of(&view));
                if std::env::var_os("PIX_NOPRES").is_none() {
                    presenter.apply(&mut view);
                }
                last_view = Some(view.clone());
                let Some(&i) = at.get(&k) else { return };
                if std::env::var_os("PIX_NOSPR").is_some() {
                    view.sprites.clear();
                }
                if std::env::var("PIX_VIEW").ok().and_then(|v| v.parse::<usize>().ok()) == Some(k) {
                    eprintln!("{view:#?}");
                }
                // PIX_TIMERFIT: which value offset makes the timer display (168..232, 8..24) match the game's
                if let (Some(t), true) = (view.timer, std::env::var_os("PIX_TIMERFIT").is_some()) {
                    let fit: Vec<i32> = (-4..=4)
                        .filter(|d| {
                            let mut v2 = view.clone();
                            v2.timer = Some(t + d);
                            let px = renderer.render(&packs, &v2, 0, W, H).to_vec();
                            (8..24).all(|y| (168..232).all(|x| px[y * W + x] == game[i][y * W + x]))
                        })
                        .collect();
                    eprintln!("timerfit {name} f{k} t {t} offsets {fit:?}");
                }
                let ours = renderer.render(&packs, &view, 0, W, H).to_vec();
                let (mut class, mut n) = classify(&view, &game[i], &ours);
                // the tear rule: a palette / BG scroll change this frame - lines matching the previous frame's are the
                // hardware's
                if n > 0 && std::env::var_os("PIX_NOTEAR").is_none() {
                    let mut olds = match old_pal.as_ref().filter(|p| **p != pal_of(&view)) {
                        Some(op) => tear_candidates(&mut renderer, &packs, &view, op),
                        None => vec![],
                    };
                    // ("v": inside a long iteration, the picture from before its step)
                    if stepped != k {
                        if let Some(v) = iter_view.as_ref() {
                            olds.push(("v", renderer.render(&packs, v, 0, W, H).to_vec()));
                        }
                    }
                    if !olds.is_empty() {
                        if let Some(rows) = torn(&game[i], &ours, &olds) {
                            torn_rows.insert(k, rows);
                            class = "torn (hardware)".into();
                            n = 0;
                        }
                    }
                }
                // what the world shows that the view may not carry (the picture's likely cause)
                if n > 0 {
                    let sc = &w.scene.screen;
                    let vram_has = |o: wf_core::vram::Owner| w.field.as_ref().and_then(|f| f.vram.as_ref()).is_some_and(|v| v.owners.iter().any(|x| x.1 == o));
                    let mut tags = vec![];
                    if sc.white > 0 { tags.push("white fade"); }
                    if sc.mosaic > 0 { tags.push("mosaic"); }
                    if sc.flash.is_some() { tags.push("palette flash"); }
                    let dl = wf_core::scene::Layers::default();
                    if sc.layers.visible != dl.visible { tags.push("BG hidden"); }
                    if sc.layers.priority != dl.priority { tags.push("BG priority"); }
                    if sc.layers.blend != dl.blend || sc.layers.alpha != dl.alpha { tags.push("BG blend"); }
                    if sc.layers.offset != dl.offset { tags.push("BG offset"); }
                    if w.scene.boss_bar.is_some() { tags.push("boss bar"); }
                    if vram_has(wf_core::vram::Owner::Timer) { tags.push("timer HUD"); }
                    // a fade: split by what our palette state did this frame - changed (a tear the rule did not
                    // accept: a line matching neither palette) or steady (the game rewrote the palette, ours did not:
                    // a fade level / timing difference or another palette writer)
                    if sc.black > 0 || sc.dim > 0 || view.palette_fade.is_some() || view.palette_scale.is_some() {
                        let now = pal_of(&view);
                        let changed = old_pal.as_ref().is_some_and(|p| (p.fade, p.scale, p.black, p.menu) != (now.fade, now.scale, now.black, now.menu));
                        tags.push(if changed { "fade (palette changed)" } else { "fade (palette steady)" });
                    }
                    if view.dialog.is_some() { tags.push("dialog"); }
                    if view.menu.is_some() { tags.push("menu"); }
                    if view.banner.is_some() { tags.push("banner"); }
                    if !tags.is_empty() {
                        class = format!("{class} [{}]", tags.join(", "));
                    }
                }
                let at_ok = std::env::var("PIX_AT").map(|v| v.split(',').any(|x| x.parse::<usize>().ok() == Some(k))).unwrap_or(true);
                if n > 0 && pngs < max_png && at_ok {
                    if let Some(o) = &out_dir {
                        let diff: Vec<u32> = game[i].iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                        save(&o.join(format!("{name}_f{k}.png")), &[&game[i], &ours, &diff]);
                        pngs += 1;
                    }
                }
                results.push((k, class, n));
            }
        });
        // frames past the actor-level divergence are the harness's
        let ok_to = rep.compared;
        let counted: Vec<&(usize, String, usize)> = results.iter().filter(|r| r.0 < ok_to).collect();
        let exact = counted.iter().filter(|r| r.2 == 0).count();
        let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
        for r in &counted {
            if r.2 > 0 {
                *classes.entry(r.1.as_str()).or_default() += 1;
                all.entry(r.1.clone()).or_default().0 += 1;
            }
        }
        if let Some(first) = counted.iter().find(|r| r.2 > 0) {
            all.entry(format!("first: {}", first.1)).or_default().1 += 1;
        }
        tot_frames += counted.len();
        tot_exact += exact;
        println!(
            "{name}: {} / {} frames pixel-exact ({:.1}%), actor match to {} | first diff {:?} | {:?}",
            exact,
            counted.len(),
            if counted.is_empty() { 0.0 } else { 100.0 * exact as f64 / counted.len() as f64 },
            ok_to,
            counted.iter().find(|r| r.2 > 0).map(|r| (r.0, r.1.clone(), r.2)),
            classes
        );
        let torn_n = counted.iter().filter(|r| r.1 == "torn (hardware)").count();
        tot_torn += torn_n;
        if torn_n > 0 {
            println!("  {name}: {torn_n} of the exact frames torn (hardware: palette rewritten while drawn)");
        }
        if !counted.is_empty() && exact == counted.len() {
            // (a torn frame: its lines' hashes too - each must equal our new- or old-palette line)
            let fr: Vec<serde_json::Value> = counted
                .iter()
                .map(|r| match torn_rows.get(&r.0) {
                    Some(rows) => serde_json::json!([r.0, format!("{:016x}", hash(&game[at[&r.0]])), rows]),
                    None => serde_json::json!([r.0, format!("{:016x}", hash(&game[at[&r.0]]))]),
                })
                .collect();
            lock.insert(name.clone(), serde_json::Value::Array(fr));
        }
        json_out.push(serde_json::json!({"segment": name, "frames": counted.len(), "exact": exact, "results": counted.iter().map(|r| serde_json::json!([r.0, r.1, r.2])).collect::<Vec<_>>()}));
    }
    println!("total: {tot_exact} / {tot_frames} sampled frames pixel-exact ({tot_torn} of them torn (hardware): accepted by the tear rule)");
    let mut errs = renderer.errors.clone();
    errs.sort();
    errs.dedup();
    for e in errs.iter().take(20) {
        println!("  renderer: {e}");
    }
    for (c, (frames, firsts)) in &all {
        println!("  {c}: {frames} frames{}", if *firsts > 0 { format!(", first of {firsts} segments") } else { String::new() });
    }
    if let Ok(p) = std::env::var("PIX_LOCK") {
        let doc = serde_json::json!({
            "note": "pixel gate (engine/crates/wf-app/tests/pixels.rs): segments whose sampled frames all render exactly as the game's screen; per segment [video frame, FNV-1a 64 of the game's 240x160 ARGB picture]. Written by `PIX_LOCK=data/segments/pixels.json cargo run --release -p wf-app --example pixels -- data/segments/*.json` (screens: work/superbuu/pix/dump.py)",
            "segments": lock,
        });
        std::fs::write(p, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    }
    if let Ok(p) = std::env::var("PIX_JSON") {
        std::fs::write(p, serde_json::to_string(&json_out).unwrap()).unwrap();
    }
}
