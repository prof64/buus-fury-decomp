//! Content packs from disk: the extracted base pack plus mod folders (engine/docs/modding.md). Used by the native
//! shell (`content/mods/*`, `--mod DIR`, `--no-mods`), tests and tools. The browser shell builds the same stack from
//! uploaded folders (web/app/app.js); the load order and overlay rules are `wf_content::Packs`'s, so both agree.

use wf_content::Packs;

/// The mod folders inside `dir` (sub-directories, sorted by name; `Packs` then orders them by dependencies / id).
/// A folder without `pack.json` still loads (its id is then made up), but the lobby / replays name mods by id, so
/// every mod should have one.
#[cfg(not(target_arch = "wasm32"))]
pub fn mod_dirs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    v.sort();
    v
}

/// `base` (a `content/base` directory) + the given mod directories.
#[cfg(not(target_arch = "wasm32"))]
pub fn load(base: &std::path::Path, mods: &[std::path::PathBuf]) -> wf_content::Result<Packs> {
    use wf_content::{DirSource, FileSource};
    let mut src: Vec<Box<dyn FileSource>> = vec![Box::new(DirSource(base.to_path_buf()))];
    for m in mods {
        if !m.is_dir() {
            return wf_content::err(format!("mod folder {} not found", m.display()));
        }
        src.push(Box::new(DirSource(m.clone())));
    }
    Packs::new(src)
}

/// One line per loaded pack (`id (name)`), for logs and the window title.
pub fn describe(packs: &Packs) -> Vec<String> {
    packs.packs.iter().map(|p| if p.info.name.is_empty() { p.info.id.clone() } else { format!("{} ({})", p.info.id, p.info.name) }).collect()
}
