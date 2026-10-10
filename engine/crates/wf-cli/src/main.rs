//! `wf-extract <rom.gba> <outdir> [--only cat1,cat2]`
//!
//! Verifies the ROM (Buu's Fury USA, SHA-1 e65738e9…) and writes the base content pack to `<outdir>/base/`.
//! Mods go next to it in `<outdir>/mods/<name>/` (see engine/CONTENT.md).

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let mut only = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--only" => {
                i += 1;
                only = args.get(i).map(|s| s.split(',').map(String::from).collect()).unwrap_or_default();
            }
            "-h" | "--help" => {
                usage();
                return;
            }
            a => pos.push(a.to_string()),
        }
        i += 1;
    }
    if pos.len() != 2 {
        usage();
        std::process::exit(2);
    }
    let rom_path = PathBuf::from(&pos[0]);
    let out_dir = PathBuf::from(&pos[1]).join("base");
    let bytes = match std::fs::read(&rom_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {}: {e}", rom_path.display());
            std::process::exit(1);
        }
    };
    let t0 = Instant::now();
    let rom = match wf_extract::Rom::new(bytes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let opts = wf_extract::Options { only };
    let (content, timings) = wf_extract::extract_rom(&rom, &opts, &mut |c| eprintln!("extracting {c}..."));
    let t_extract = t0.elapsed();
    if out_dir.exists() {
        let _ = std::fs::remove_dir_all(&out_dir);
    }
    for (path, data) in &content.files {
        let p = out_dir.join(path);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).expect("mkdir");
        }
        std::fs::write(&p, data).expect("write");
    }
    for t in &timings {
        eprintln!("  {:<10} {:>6} files {:>9.1} ms", t.category, t.files, t.millis);
    }
    for w in &content.warnings {
        eprintln!("warning: {w}");
    }
    eprintln!(
        "{} files, {:.1} MB in {} — extract {:.2} s, total {:.2} s",
        content.files.len(),
        content.total_bytes() as f64 / 1e6,
        out_dir.display(),
        t_extract.as_secs_f64(),
        t0.elapsed().as_secs_f64()
    );
}

fn usage() {
    eprintln!("usage: wf-extract <buus_fury.gba> <outdir> [--only {}]", wf_extract::CATEGORIES.join(","));
}
