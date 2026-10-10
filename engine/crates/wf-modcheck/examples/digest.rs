//! Native side of the native == wasm mod check: `cargo run --release -p wf-modcheck --example digest -- <content/base>
//! <mod dir>... [--frames N]`. The packs are read into memory exactly as `engine/web/mods_check.mjs` feeds the wasm
//! module (every file under each folder), then the same scenarios run; compare the printed digests.

use std::collections::BTreeMap;
use std::path::Path;

use wf_app::wf_content::{FileSource, MemSource, Packs};

fn read_dir(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut v: Vec<_> = std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        v.sort();
        for p in v {
            if p.is_dir() {
                walk(root, &p, out);
            } else if let (Ok(rel), Ok(b)) = (p.strip_prefix(root), std::fs::read(&p)) {
                out.insert(rel.to_string_lossy().replace('\\', "/"), b);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut frames = 1200;
    if let Some(k) = args.iter().position(|a| a == "--frames") {
        frames = args.get(k + 1).and_then(|v| v.parse().ok()).unwrap_or(frames);
        args.drain(k..(k + 2).min(args.len()));
    }
    if args.is_empty() {
        eprintln!("usage: digest <content/base> <mod dir>... [--frames N]");
        std::process::exit(2);
    }
    let srcs: Vec<Box<dyn FileSource>> = args.iter().map(|a| Box::new(MemSource(read_dir(Path::new(a)))) as Box<dyn FileSource>).collect();
    let packs = Packs::new(srcs).expect("packs");
    match wf_modcheck::digest(&packs, frames) {
        Ok(s) => print!("{s}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
