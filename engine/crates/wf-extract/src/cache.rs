//! Extracted-content caches for tests and tools (not wasm): a directory holds one extraction plus a `.complete`
//! marker that records [`stamp`] (the extractor's source hash, `build.rs`, and the categories). A marker from another
//! extractor version (or the old plain "ok") does not match: the directory is re-extracted, so content can never go
//! stale after an extractor change.

use crate::{extract, Options};
use std::path::{Path, PathBuf};

/// the extractor's source hash (all extraction crates + annotation files)
pub const EXTRACTOR_HASH: &str = env!("WF_EXTRACT_HASH");

/// the `.complete` marker's content for an extraction of these categories (empty = all)
pub fn stamp(only: &[&str]) -> String {
    format!("wf-extract {EXTRACTOR_HASH} only={}", only.join(","))
}

/// true when `dir` holds an extraction of `only` by this extractor version
pub fn fresh(dir: &Path, only: &[&str]) -> bool {
    std::fs::read_to_string(dir.join(".complete")).is_ok_and(|s| s.trim() == stamp(only))
}

/// `dir` with an up-to-date extraction of `only` from the ROM at `rom` (extracted when missing or stale). The new
/// tree is written next to `dir` and renamed into place (concurrent test binaries never see a half-written one).
pub fn ensure(dir: &Path, rom: &Path, only: &[&str]) -> PathBuf {
    if fresh(dir, only) {
        return dir.to_path_buf();
    }
    let bytes = std::fs::read(rom).unwrap_or_else(|e| panic!("ROM {}: {e}", rom.display()));
    let opts = Options { only: only.iter().map(|s| s.to_string()).collect() };
    let (content, _) = extract(bytes, &opts).expect("extract");
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = dir.with_file_name(format!("{name}.tmp{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    for (path, data) in &content.files {
        let p = tmp.join(path.strip_prefix("base/").unwrap_or(path));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
    }
    std::fs::write(tmp.join(".complete"), stamp(only)).unwrap();
    // another process may have finished first: keep the fresh one
    if fresh(dir, only) {
        let _ = std::fs::remove_dir_all(&tmp);
        return dir.to_path_buf();
    }
    let old = dir.with_file_name(format!("{name}.old{}", std::process::id()));
    if dir.exists() {
        let _ = std::fs::rename(dir, &old);
    }
    if std::fs::rename(&tmp, dir).is_err() {
        // lost a race against another writer: use whatever fresh tree is there, else ours
        if !fresh(dir, only) {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::rename(&tmp, dir).expect("rename the extraction into place");
        } else {
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
    let _ = std::fs::remove_dir_all(&old);
    dir.to_path_buf()
}
