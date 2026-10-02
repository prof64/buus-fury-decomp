//! Area-name banner vs the emulator (headless; skipped without the ROM or without
//! `crates/wf-render/re/banner_capture.py`'s output in `<repo>/work/frontend/banner/`):
//! * the 240x16 surface (palette indices) equals the game's OBJ VRAM copy;
//! * on screen, rows 128..144 equal the emulator at every frame of the banner's life (t = capture frame),
//!   including the collapse of the last 16 frames and its disappearance.

use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_render::banner::{Banner, FRAMES, H, W, Y};

fn load_png(p: &std::path::Path) -> Vec<u32> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()));
    let mut r = d.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    buf.chunks(ch).take(240 * 160).map(|c| (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect()
}

#[test]
fn banner_matches_emulator() {
    let dir = std::env::var("WF_PARITY_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| wf_rom::testutil::repo_root().join("work/frontend"))
        .join("banner");
    if !dir.join("frames.json").exists() {
        return;
    }
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let opts = wf_extract::Options { only: vec!["ui".into()] };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let mut b = Banner::load(&packs).expect("ui/banner content");
    let text = "Southwest Forest";

    // 1. the surface: compare the t = 0 frame's colours with the VRAM surface mapped through the UI palette
    let surf = std::fs::read(dir.join("surface.bin")).unwrap();
    let pal: serde_json::Value = packs.json("ui/palette.json").unwrap();
    let pal: Vec<u32> = pal["colors"].as_array().unwrap().iter().map(|c| u32::from_str_radix(&c.as_str().unwrap()[1..], 16).unwrap()).collect();
    let ours = b.frame(text, 0).unwrap();
    let surf_diff = (0..W * H).filter(|&i| ours[i] & 0xFFFFFF != pal[surf[i] as usize]).count();

    // 2. the screen, every captured frame
    let frames: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("frames.json")).unwrap()).unwrap();
    let n = frames.as_array().unwrap().len();
    let mut worst = (0usize, 0usize);
    let mut total = 0;
    for k in 1..n {
        let emu = load_png(&dir.join(format!("f{k:03}.png")));
        let t = k as u32;
        let ours = b.frame(text, t);
        let mut d = 0;
        for y in 0..H {
            for x in 0..W {
                let e = emu[(Y as usize + y) * 240 + x];
                let o = ours.as_ref().map(|v| v[y * W + x]).filter(|c| c >> 24 != 0).map(|c| c & 0xFFFFFF);
                // where our banner is transparent the emulator must not show banner colours we failed to draw:
                // compare against the frame after the banner is gone (same map / camera)
                let want = o.unwrap_or_else(|| load_cached_bg(&dir, x, y));
                if want != e {
                    d += 1;
                }
            }
        }
        // frames 0..30 are the area's fade-in (black screen): judged from 31 on
        if std::env::var("BANNER_DEBUG").is_ok() && d > 0 {
            eprintln!("k {k} t {t}: {d}");
        }
        if k >= 31 {
            total += d;
            if d > worst.1 {
                worst = (k, d);
            }
        }
    }
    println!("banner: surface {surf_diff} px differ; screen rows 128..144, frames 31..{n}: {total} px differ (worst k {} = {} px), lifetime {FRAMES}", worst.0, worst.1);
    assert_eq!(surf_diff, 0);
    assert_eq!(total, 0);
}

/// rows 128..144 of the first capture frame without the banner (the map behind it; the camera is static)
fn load_cached_bg(dir: &std::path::Path, x: usize, y: usize) -> u32 {
    use std::sync::OnceLock;
    static BG: OnceLock<Vec<u32>> = OnceLock::new();
    let bg = BG.get_or_init(|| load_png(&dir.join("f299.png")));
    bg[(Y as usize + y) * 240 + x]
}
