//! `WF_EXTRACT_HASH`: a hash of every source and data file the extractor's output depends on (this crate, the
//! extraction crates it calls, and the annotation files they `include_str!`). Content caches record it in their
//! `.complete` marker (`wf_extract::cache`), so an extractor change re-extracts them instead of serving stale content.

use std::path::{Path, PathBuf};

const INPUTS: &[&str] = &[
    "../wf-extract/src",
    "../wf-rom/src",
    "../wf-audio/src",
    "../wf-maps/src",
    "../wf-tables/src",
    "../wf-tables/data",
    "../wf-ui/src",
    "../wf-world/src",
    "../wf-script/src",
    "../../annotations",
];

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut all = vec![];
    for d in INPUTS {
        let p = root.join(d);
        println!("cargo:rerun-if-changed={}", p.display());
        files(&p, &mut all);
    }
    all.sort();
    // FNV-1a 64 over (relative path, contents)
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for &x in b {
            h ^= x as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for p in &all {
        let rel = p.strip_prefix(&root).unwrap_or(p);
        eat(rel.to_string_lossy().as_bytes());
        eat(&std::fs::read(p).unwrap_or_default());
    }
    println!("cargo:rustc-env=WF_EXTRACT_HASH={h:016x}");
}
