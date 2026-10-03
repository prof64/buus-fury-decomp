//! Content for the agent tools: the extractor's base pack, built from the user's ROM once and cached.
//!
//! Resolution order: `WF_CONTENT` (an extracted `content/base` directory) → the cache
//! (`WF_CACHE`, default `~/Library/Caches/wf-buu/agent` on macOS, `$XDG_CACHE_HOME/wf-buu/agent` or `~/.cache/...`
//! elsewhere) when its `pack.json` was written by this build's extractor → extract from the ROM (`WF_ROM`, else
//! `roms/buu.gba` searched upward from the current directory and from the executable) into the cache (~5 s).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use wf_core::content::{DirSource, GameData};

/// the workspace shares one version, so this is the extractor's `generated_by`
const EXTRACTOR: &str = concat!("wf-extract ", env!("CARGO_PKG_VERSION"));

fn find_up(start: &Path, rel: &str) -> Option<PathBuf> {
    let mut d = Some(start);
    while let Some(p) = d {
        let c = p.join(rel);
        if c.exists() {
            return Some(c);
        }
        d = p.parent();
    }
    None
}

pub fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WF_ROM") {
        return Some(PathBuf::from(p));
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(p) = find_up(&cwd, "roms/buu.gba") {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    find_up(exe.parent()?, "roms/buu.gba")
}

pub fn cache_root() -> PathBuf {
    if let Ok(p) = std::env::var("WF_CACHE") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    if cfg!(target_os = "macos") {
        return home.join("Library/Caches/wf-buu/agent");
    }
    match std::env::var("XDG_CACHE_HOME") {
        Ok(x) if !x.is_empty() => PathBuf::from(x).join("wf-buu/agent"),
        _ => home.join(".cache/wf-buu/agent"),
    }
}

fn pack_ok(dir: &Path) -> bool {
    std::fs::read(dir.join("pack.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["generated_by"] == EXTRACTOR && wf_extract::cache::fresh(dir, &[]))
}

/// the content/base directory (extracting when needed)
pub fn content_dir() -> Result<PathBuf, String> {
    if let Ok(d) = std::env::var("WF_CONTENT") {
        return Ok(PathBuf::from(d));
    }
    let dir = cache_root().join("base");
    if pack_ok(&dir) {
        return Ok(dir);
    }
    let rom = rom_path().ok_or("no ROM: set WF_ROM=/path/to/buu.gba (Buu's Fury USA) or WF_CONTENT=<extracted content/base>")?;
    let bytes = std::fs::read(&rom).map_err(|e| format!("ROM {}: {e}", rom.display()))?;
    let (content, _) = wf_extract::extract(bytes, &wf_extract::Options { only: vec![] }).map_err(|e| format!("extract: {e:?}"))?;
    let tmp = cache_root().join(format!("base.tmp{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    for (path, data) in &content.files {
        let rel = path.strip_prefix("base/").unwrap_or(path);
        let p = tmp.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, data).map_err(|e| e.to_string())?;
    }
    std::fs::write(tmp.join(".complete"), wf_extract::cache::stamp(&[])).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// game data (loaded once per process)
pub fn game_data() -> Result<Arc<GameData>, String> {
    static D: OnceLock<Result<Arc<GameData>, String>> = OnceLock::new();
    D.get_or_init(|| {
        let dir = content_dir()?;
        GameData::load(&DirSource { root: dir }).map(Arc::new).map_err(|e| e.to_string())
    })
    .clone()
}

/// render packs (screenshots), loaded on first use
pub fn packs() -> Result<&'static wf_content::Packs, String> {
    static P: OnceLock<Result<wf_content::Packs, String>> = OnceLock::new();
    P.get_or_init(|| {
        let dir = content_dir()?;
        wf_content::Packs::new(vec![Box::new(wf_content::DirSource(dir))]).map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(|e| e.clone())
}
