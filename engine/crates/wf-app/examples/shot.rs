//! Headless run of the demo: `cargo run --release -p wf-app --example shot -- <content dir> <out.png> [frames]
//! [view WxH] [inputs]` — steps the demo world with a scripted walk, renders a view and writes a PNG (2x scale).
//! Also prints step / render timings. `<content dir>` holds `base/` (+ optional `mods/*`).

use std::time::Instant;

use wf_app::demo::{DemoConfig, DemoWorld};
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_view::{buttons as b, RenderView, Sim};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::path::PathBuf::from(a.first().map(String::as_str).unwrap_or("content"));
    let out = a.get(1).cloned().unwrap_or("shot.png".into());
    let frames: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(240);
    let (vw, vh) = a
        .get(3)
        .and_then(|s| s.split_once('x'))
        .map(|(w, h)| (w.parse().unwrap(), h.parse().unwrap()))
        .unwrap_or((480usize, 320usize));
    let mut sources: Vec<Box<dyn FileSource>> = vec![Box::new(DirSource(dir.join("base")))];
    if let Ok(rd) = std::fs::read_dir(dir.join("mods")) {
        for e in rd.flatten() {
            sources.push(Box::new(DirSource(e.path())));
        }
    }
    let t0 = Instant::now();
    let packs = Packs::new(sources).expect("packs");
    let mut w = DemoWorld::new(&packs, &DemoConfig::default()).expect("demo");
    println!("load demo world: {:.1} ms", t0.elapsed().as_secs_f64() * 1e3);
    // optional input script: "R100,D70,U5,L3" (button letters: R L U D A B; frames each), then idle
    let mut script: Vec<u16> = Vec::new();
    for item in a.get(4).map(String::as_str).unwrap_or("R60,D60,RU31").split(',').filter(|s| !s.is_empty()) {
        let k = item.trim_end_matches(|c: char| c.is_ascii_digit());
        let n: usize = item[k.len()..].parse().unwrap_or(1);
        let bits = k.chars().fold(0, |acc, c| acc | match c { 'R' => b::RIGHT, 'L' => b::LEFT, 'U' => b::UP, 'D' => b::DOWN, 'A' => b::A, 'B' => b::B, _ => 0 });
        script.extend(std::iter::repeat_n(bits, n));
    }
    let mut ev = Vec::new();
    let t1 = Instant::now();
    for f in 0..frames as usize {
        w.step(&[script.get(f).copied().unwrap_or(0)], &mut ev);
    }
    println!("{frames} steps: {:.3} ms/step, events {:?}", t1.elapsed().as_secs_f64() * 1e3 / frames as f64, ev);
    let mut view = RenderView::default();
    w.view(&mut view);
    if std::env::args().any(|x| x == "--front") {
        for s in view.sprites.iter_mut() {
            s.priority = 0;
        }
    }
    println!("sprites at {:?} camera {:?}", view.sprites.iter().map(|s| (s.x, s.y)).collect::<Vec<_>>(), view.cameras);
    let mut r = wf_render::Renderer::new();
    r.render(&packs, &view, 0, vw, vh); // warm (loads assets)
    let t2 = Instant::now();
    let n = 60;
    for _ in 0..n {
        r.render(&packs, &view, 0, vw, vh);
    }
    println!("render {vw}x{vh}: {:.3} ms/frame; errors {:?}", t2.elapsed().as_secs_f64() * 1e3 / n as f64, r.errors);
    println!("fb hash {:016x} state hash {:016x}", wf_render::hash_fb(&r.fb), w.state_hash());
    // write 2x PNG
    let s = 2;
    let mut rgb = Vec::with_capacity(vw * vh * 3 * s * s);
    for y in 0..vh * s {
        for x in 0..vw * s {
            let p = r.fb[(y / s) * vw + x / s];
            rgb.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let f = std::fs::File::create(&out).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), (vw * s) as u32, (vh * s) as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
    println!("wrote {out}");
}
