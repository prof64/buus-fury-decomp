//! Skill audit - the gate's packer (headless, muted): `cargo run --release -p wf-app --example skill_audit --
//! work/skillaudit/segs/sa_*.json`. Each emulator-recorded skill segment is replayed and compared with the game's
//! screen of every video frame (`PIX_FB` dir, default work/superbuu/pix/fb: `work/superbuu/pix/dump.py --every 1`) by
//! the gate's comparer (`tests/audit/skill.rs`); per segment one line: frames, the harness's first state difference,
//! the first frame whose picture differs and the count of such frames. `SA_PACK=dir` writes the gate's data
//! (`data/audits/skill/NAME.json.z`: the recording and the hash of every game picture - no pictures;
//! tests/audits.rs). To look at differing pictures: `examples/pixels.rs` with `PIX_ALL=1 PIX_NOPROPS=1 SEGMENT_VRAM=0`.

#[path = "../tests/audit/pack.rs"]
mod pack;
#[path = "../tests/audit/skill.rs"]
mod skill;

use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use wf_app::wf_content::{DirSource, FileSource, Packs};

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = wf_rom::testutil::repo_root();
    let fb_dir = std::env::var("PIX_FB").map(PathBuf::from).unwrap_or_else(|_| repo.join("work/superbuu/pix/fb"));
    let pack_dir = std::env::var("SA_PACK").ok().map(PathBuf::from);
    if let Some(p) = &pack_dir {
        std::fs::create_dir_all(p).unwrap();
    }
    // the segment harness's shared extraction (as examples/pixels.rs)
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
    let mut renderer = wf_render::Renderer::new();
    let mut tear_renderer = wf_render::Renderer::new();
    for a in &args {
        let Ok(mut doc) = wf_import::segment::load(Path::new(a)) else {
            println!("{a}: no segment");
            continue;
        };
        let name = doc["name"].as_str().unwrap_or("?").to_string();
        let Some((want, game)) = read_fb(&fb_dir.join(format!("{name}.fb"))) else {
            println!("{name}: no screen dump");
            continue;
        };
        // (the gate's data is every video frame's picture, frame 0 first)
        if want.iter().enumerate().any(|(i, &k)| i != k) {
            println!("{name}: the screen dump is not of every frame");
            continue;
        }
        skill::strip_vram(&mut doc);
        // (a frame the tear rule explains - `pack::torn_spec` - counts as the game's, its line specs kept for the gate)
        let mut torn = serde_json::Map::new();
        let r = skill::check(&data, &files, &packs, &mut renderer, &mut tear_renderer, &doc, &mut |k, ours, cand| {
            let g = game.get(k)?;
            if g.as_slice() == ours {
                return Some(true);
            }
            if k == 0 || std::env::var_os("SA_NOTEAR").is_some() {
                return Some(false);
            }
            let cands: Vec<(String, Vec<u32>)> = skill::TEARS.into_iter().map(|l| (l.to_string(), cand(l))).filter(|c| c.1 != ours).collect();
            match pack::torn_spec(g, ours, &cands) {
                Some(spec) => {
                    torn.insert(k.to_string(), serde_json::json!(spec));
                    Some(true)
                }
                None => Some(false),
            }
        });
        println!("{name}: {} frames | state {} | picture {} | picture-diff frames {}{}", r.frames, r.state.as_ref().map(|s| format!("{} ({})", s.0, s.2)).unwrap_or("-".into()), r.picture.first().map(|f| f.to_string()).unwrap_or("-".into()), r.picture.len(), if torn.is_empty() { String::new() } else { format!(" | torn (hardware) {}", torn.len()) });
        if let Some(p) = &pack_dir {
            let h: Vec<u64> = game.iter().map(|g| pack::hash(g)).collect();
            doc["pic"] = serde_json::json!({"hash": pack::hex(&h), "torn": torn});
            pack::write(&p.join(format!("{name}.json.z")), &doc);
        }
    }
}
