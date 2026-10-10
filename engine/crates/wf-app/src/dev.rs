//! Dev mode (`buus-fury --dev`): hot reload of mod files while the app runs (engine/docs/modding.md §7).
//!
//! The host polls [`Dev::poll`] **between frames** (never inside a step). Every [`Dev::every`] calls it fingerprints
//! the loaded mod folders (path, size and modification time of every file: std only, no watcher dependency); when
//! that changed it runs the mod checker ([`crate::modcheck::check`]) on the base pack + the mods:
//! * errors -> [`Reload::Rejected`] with the checker's messages: the running content and scene stay as they are;
//! * clean -> [`Reload::Reloaded`] with freshly loaded [`Packs`]; the host rebuilds its scene from scratch with them
//!   ([`restart`]: the same builder it started with - a battle with the same config, story mode at the same start
//!   area / title), so the new content runs from a clean, deterministic start. Nothing of the old world is carried
//!   over (a save state of old content in new content would not be meaningful).
//!
//! Determinism: nothing here runs unless the host enables dev mode; the simulation never sees the watcher (reloads
//! happen between steps and replace the whole scene). The native shell refuses `--dev` together with netplay and
//! replay playback / `--record`, and a reload starts a fresh recording (the replay before it was made with the old
//! content and is dropped).
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use wf_content::Packs;

use crate::App;

/// What one poll found.
pub enum Reload {
    /// no file changed (or not time to look yet)
    Unchanged,
    /// files changed but the checker found errors: keep the old content (the checker's report text)
    Rejected(String),
    /// files changed and the mods check clean: the new packs (rebuild the scene with them)
    Reloaded(Packs),
}

pub struct Dev {
    pub base: PathBuf,
    pub mods: Vec<PathBuf>,
    /// poll the files every this many calls (frames): 30 = twice a second at game speed
    pub every: u32,
    n: u32,
    stamp: u64,
}

/// FNV-1a over (relative path, size, mtime) of every file under the mod folders, sorted
fn fingerprint(dirs: &[PathBuf]) -> u64 {
    fn walk(root: &Path, d: &Path, out: &mut Vec<(String, u64, u128)>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                walk(root, &p, out);
            } else {
                let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
                out.push((p.strip_prefix(root).unwrap_or(&p).to_string_lossy().into_owned(), m.len(), t));
            }
        }
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, d) in dirs.iter().enumerate() {
        let mut v = Vec::new();
        walk(d, d, &mut v);
        v.sort();
        for (p, len, t) in v {
            for b in format!("{i}\0{p}\0{len}\0{t}\n").bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    h
}

impl Dev {
    /// watch `mods` (the folders the app loaded on top of `base`)
    pub fn new(base: &Path, mods: &[PathBuf]) -> Dev {
        Dev { base: base.to_path_buf(), mods: mods.to_vec(), every: 30, n: 0, stamp: fingerprint(mods) }
    }

    /// call once per frame, between steps: every `every` calls, look at the files
    pub fn poll(&mut self) -> Reload {
        self.n += 1;
        if self.n < self.every {
            return Reload::Unchanged;
        }
        self.n = 0;
        self.check_now()
    }

    /// look at the files now: changed -> check and load (see the module doc)
    pub fn check_now(&mut self) -> Reload {
        let s = fingerprint(&self.mods);
        if s == self.stamp {
            return Reload::Unchanged;
        }
        // (a broken edit is reported once; the next save of the file is looked at again)
        self.stamp = s;
        let rep = crate::modcheck::check(&self.base, &self.mods);
        if !rep.ok() {
            return Reload::Rejected(rep.text());
        }
        match crate::mods::load(&self.base, &self.mods) {
            Ok(p) => Reload::Reloaded(p),
            Err(e) => Reload::Rejected(e.to_string()),
        }
    }
}

/// The scene rebuilt with new packs by the host's own builder (the one it started with), the presentation settings
/// of `old` kept (view mode, original pacing, auto zoom).
pub fn restart(old: &App, packs: Packs, build: impl FnOnce(Packs) -> Result<App, String>) -> Result<App, String> {
    let mut app = build(packs)?;
    app.mode = old.mode;
    app.original_pacing = old.original_pacing;
    app.auto_zoom.max = old.auto_zoom.max;
    Ok(app)
}
