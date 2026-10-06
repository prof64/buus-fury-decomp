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

// ---------------------------------------------------------------------------------------------------------------
// mods (engine/docs/modding.md): the base pack + mod folders, per session

/// The base pack plus a list of mod folders, loaded once per (folders, their files' sizes and times): an agent that
/// edits a mod and starts a new session gets the edited content. Sessions keep the set they were made with.
pub struct ModSet {
    /// the folders (canonical paths)
    pub dirs: Vec<PathBuf>,
    pub packs: wf_content::Packs,
    /// the core's data with the mod scripts' hooks attached (scripted skills, projectiles, AI nodes)
    pub data: Arc<GameData>,
    /// the mods' ids in load order and the hash of their files (as replays / save states / the lobby record them)
    pub content_id: wf_replay::ContentId,
    stamp: u64,
    files: OnceLock<Result<Arc<std::collections::BTreeMap<String, String>>, String>>,
}

impl ModSet {
    /// the story scripts of this content (as `tools::story_files`, over these packs)
    pub fn story_files(&self) -> Result<Arc<std::collections::BTreeMap<String, String>>, String> {
        self.files
            .get_or_init(|| {
                let files: std::collections::BTreeMap<String, String> = self
                    .packs
                    .list()
                    .into_iter()
                    .filter(|p| p.starts_with("scripts/") && (p.ends_with(".rhai") || p.ends_with(".text.json")))
                    .filter_map(|p| self.packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
                    .collect();
                if files.is_empty() {
                    return Err("this content has no scripts/ (re-extract with the current extractor)".into());
                }
                Ok(Arc::new(files))
            })
            .clone()
    }
    /// the mod ids in load order
    pub fn ids(&self) -> Vec<String> {
        self.content_id.mods.clone()
    }
}

/// the default mod folders of new sessions: `WF_MODS` (a path list, `:`-separated like PATH); none when unset
pub fn default_mods() -> Vec<PathBuf> {
    std::env::var_os("WF_MODS").map(|v| std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect()).unwrap_or_default()
}

/// sizes + modification times of every file under the folders (a cheap "did the mod change" stamp)
fn stamp(dirs: &[PathBuf]) -> u64 {
    fn mix(h: &mut u64, x: u64) {
        *h = (*h ^ x).wrapping_mul(0x100_0000_01B3);
    }
    fn walk(d: &Path, h: &mut u64) {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        v.sort();
        for p in v {
            mix(h, p.to_string_lossy().bytes().fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)));
            if p.is_dir() {
                walk(&p, h);
            } else if let Ok(m) = std::fs::metadata(&p) {
                mix(h, m.len());
                if let Some(t) = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
                    mix(h, t.as_nanos() as u64);
                }
            }
        }
    }
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for d in dirs {
        walk(d, &mut h);
    }
    h
}

/// The content for these mod folders (`None` = no mods: the plain base pack, `game_data()` / `packs()`).
pub fn mod_set(dirs: &[PathBuf]) -> Result<Option<Arc<ModSet>>, String> {
    use std::sync::Mutex;
    if dirs.is_empty() {
        return Ok(None);
    }
    let dirs: Vec<PathBuf> = dirs.iter().map(|d| d.canonicalize().map_err(|e| format!("mod folder {}: {e}", d.display()))).collect::<Result<_, _>>()?;
    let st = stamp(&dirs);
    static CACHE: OnceLock<Mutex<std::collections::BTreeMap<Vec<PathBuf>, Arc<ModSet>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(m) = cache.lock().unwrap().get(&dirs) {
        if m.stamp == st {
            return Ok(Some(Arc::clone(m)));
        }
    }
    let base = content_dir()?;
    let packs = wf_app::mods::load(&base, &dirs).map_err(|e| format!("mods: {e}"))?;
    let data = wf_app::battle::load_data(&packs)?;
    let content_id = wf_app::battle::content_id(&packs);
    let m = Arc::new(ModSet { dirs: dirs.clone(), packs, data, content_id, stamp: st, files: OnceLock::new() });
    cache.lock().unwrap().insert(dirs, Arc::clone(&m));
    Ok(Some(m))
}

/// the content id of the plain base pack (no mods), for replay checks
pub fn base_content_id() -> Result<wf_replay::ContentId, String> {
    Ok(wf_app::battle::content_id(packs()?))
}
