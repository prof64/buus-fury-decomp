//! The picture against the game (headless, muted): `cargo run --release -p wf-app --example pixels -- SEG.json..`.
//! Each segment is replayed by the segment harness (`wf_import::segment::replay_with`, the same steps as the gates);
//! at the sampled video frames of `<fb dir>/<name>.fb` (the emulator's screen: `work/superbuu/pix/dump.py`) our view
//! (`wf_core::view::render_view` + the presenter's one-frame OBJ delay) is rendered by wf-render at 240x160 and
//! compared pixel for pixel. Only frames inside the segment's matched prefix count (an actor difference is the
//! harness's, not the picture's). Per segment: frames compared, pixel-exact frames, and the differing pixels sorted
//! into classes; `PIX_OUT=dir` writes `game | ours | diff` PNGs of differing frames (`PIX_MAX` per segment, 4),
//! `PIX_FB=dir` the .fb directory, `PIX_JSON=file` the per-frame results.

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
    let (mut tot_frames, mut tot_exact) = (0usize, 0usize);
    for a in &args {
        let p = PathBuf::from(a);
        let doc = wf_import::segment::load(&p).expect("segment");
        let name = doc["name"].as_str().unwrap_or("?").to_string();
        let Some((want, game)) = read_fb(&fb_dir.join(format!("{name}.fb"))) else {
            println!("{name}: no screen dump");
            continue;
        };
        let at: BTreeMap<usize, usize> = want.iter().enumerate().map(|(i, &k)| (k, i)).collect();
        let mut presenter = wf_core::view::Presenter::default();
        let mut results: Vec<(usize, String, usize)> = vec![];
        let mut pngs = 0;
        let rep = wf_import::segment::replay_with(&data, &files, &doc, &mut |f| match f {
            wf_import::segment::Frame::Before(_, w) => presenter.before_step(w),
            wf_import::segment::Frame::After(k, w) => {
                let Some(&i) = at.get(&k) else { return };
                let mut view = RenderView::default();
                wf_core::view::render_view(w, &mut view);
                presenter.apply(&mut view);
                if std::env::var_os("PIX_NOSPR").is_some() {
                    view.sprites.clear();
                }
                if std::env::var_os("PIX_VIEW").is_some() && k == want[1] {
                    eprintln!("{view:#?}");
                }
                let ours = renderer.render(&packs, &view, 0, W, H).to_vec();
                let (class, n) = classify(&view, &game[i], &ours);
                if n > 0 && pngs < max_png {
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
        json_out.push(serde_json::json!({"segment": name, "frames": counted.len(), "exact": exact, "results": counted.iter().map(|r| serde_json::json!([r.0, r.1, r.2])).collect::<Vec<_>>()}));
    }
    println!("total: {tot_exact} / {tot_frames} sampled frames pixel-exact");
    for (c, (frames, firsts)) in &all {
        println!("  {c}: {frames} frames{}", if *firsts > 0 { format!(", first of {firsts} segments") } else { String::new() });
    }
    if let Ok(p) = std::env::var("PIX_JSON") {
        std::fs::write(p, serde_json::to_string(&json_out).unwrap()).unwrap();
    }
}
