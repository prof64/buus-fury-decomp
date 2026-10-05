//! (temporary, user-bug investigation) quick start + an input script, PNG shots in the GBA and PC views.
//! `userbugs OUTDIR SCRIPT SHOTFRAMES` - SCRIPT as wf-native's --input (`L3,0:10,B30`), SHOTFRAMES `100,120,..`.
use std::path::Path;
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_render::ViewMode;

fn save(p: &Path, px: &[u32], w: usize, h: usize) {
    let f = std::fs::File::create(p).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let mut wr = e.write_header().unwrap();
    let b: Vec<u8> = px.iter().flat_map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect();
    wr.write_image_data(&b).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let mut script: Vec<u16> = Vec::new();
    for item in a[1].split(',').filter(|s| !s.is_empty()) {
        let k = item.trim_end_matches(|c: char| c.is_ascii_digit());
        let n: usize = item[k.len()..].parse().unwrap_or(1);
        use wf_view::buttons as b;
        let bits = k.chars().fold(0, |acc, c| {
            acc | match c {
                'R' => b::RIGHT,
                'L' => b::LEFT,
                'U' => b::UP,
                'D' => b::DOWN,
                'A' => b::A,
                'B' => b::B,
                'S' => b::START,
                'E' => b::SELECT,
                'l' => b::L,
                'r' => b::R,
                _ => 0,
            }
        });
        script.extend(std::iter::repeat_n(bits, n));
    }
    let mut shots: Vec<usize> = Vec::new();
    for x in a.get(2).map(|s| s.as_str()).unwrap_or("").split(',').filter(|s| !s.is_empty()) {
        // `a-b/s` = a, a+s, .. < b
        if let Some((r, st)) = x.split_once('/') {
            let (p, q) = r.split_once('-').unwrap();
            let (p, q, st): (usize, usize, usize) = (p.parse().unwrap(), q.parse().unwrap(), st.parse().unwrap());
            shots.extend((p..q).step_by(st));
        } else if let Ok(v) = x.parse() {
            shots.push(v);
        }
    }
    let repo = wf_rom::testutil::repo_root();
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target-userbugs/content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let t0 = std::time::Instant::now();
    let mut app = if std::env::var("UB_AREA").is_ok() {
        let s = std::env::var("UB_AREA").unwrap();
        wf_app::story::app(packs, wf_app::story::parse_area(&s).unwrap(), vec![], 1).unwrap()
    } else {
        wf_app::launcher::story_app(packs, &[], None, 1).unwrap()
    };
    eprintln!("built in {:?}", t0.elapsed());
    let n = script.len().max(shots.iter().copied().max().unwrap_or(0) + 1);
    let (pw, ph) = (960u32, 640u32);
    for f in 0..n {
        app.inputs[0] = script.get(f).copied().unwrap_or(0);
        app.step();
        if shots.contains(&f) {
            {
                let s = app.sim.as_any().unwrap().downcast_ref::<wf_app::story::StorySim>().unwrap();
                let w = s.world();
                let ents: Vec<String> = w.ents.iter().filter(|e| !e.removed).map(|e| format!("{:?}", std::mem::discriminant(&e.body))).collect();
                eprintln!("f{f}: area {:?} depth {} ents {}", w.field.as_ref().map(|x| (x.area.world, x.area.area)), w.scene.depth, ents.len());
            }
            for (tag, mode, ww, wh) in [("gba", ViewMode::Gba { scale: 1 }, 240, 160), ("pc", ViewMode::Pc { zoom: 2 }, pw, ph)] {
                app.mode = mode;
                app.sim.view(&mut app.view);
                let fit = app.render(0, ww, wh);
                save(&out.join(format!("{tag}_{f:05}.png")), &app.renderer.fb, fit.view_w as usize, fit.view_h as usize);
            }
        }
    }
}
