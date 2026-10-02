//! Runtime side of the content format (`engine/CONTENT.md`): a stack of packs (`base` + enabled mods) behind
//! one read API, and typed loaders the simulation and the renderer share.
//!
//! * [`Packs`] — load order (dependencies first, ties by id), per-path override, `*.patch.json` RFC 7396 merge
//!   patches, `*.delete` removal. Packs come from any [`FileSource`] (a directory, an in-memory map, or the
//!   browser host through `wf-web`'s imports).
//! * [`sprite`] — sprite / effect definitions + sheets, animation step lookup.
//! * [`map`] — Tiled maps decoded to tile layers, indexed tilesets with GBA tile-animation timing, collision.
//! * [`audio`] — the `wf-audio` [`wf_audio::bank::SoundBank`] rebuilt from `audio/*` files (no ROM needed).

pub mod audio;
pub mod b64;
pub mod image;
pub mod map;
pub mod sprite;

use std::collections::{BTreeMap, BTreeSet};

/// A source of files for one pack. Paths are relative to the pack root, `/`-separated.
pub trait FileSource: Send + Sync {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    fn exists(&self, path: &str) -> bool {
        self.read(path).is_some()
    }
    /// Every path in the pack (used for `index.json` regeneration / listing). Default: the pack's
    /// `files.json` if present.
    fn list(&self) -> Vec<String> {
        self.read("files.json")
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .unwrap_or_default()
    }
}

/// In-memory pack.
#[derive(Default)]
pub struct MemSource(pub BTreeMap<String, Vec<u8>>);

impl FileSource for MemSource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.get(path).cloned()
    }
    fn exists(&self, path: &str) -> bool {
        self.0.contains_key(path)
    }
    fn list(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
}

/// A pack directory on disk.
#[cfg(not(target_arch = "wasm32"))]
pub struct DirSource(pub std::path::PathBuf);

#[cfg(not(target_arch = "wasm32"))]
impl FileSource for DirSource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        if path.split('/').any(|c| c == "..") {
            return None;
        }
        std::fs::read(self.0.join(path)).ok()
    }
    fn exists(&self, path: &str) -> bool {
        !path.split('/').any(|c| c == "..") && self.0.join(path).is_file()
    }
    fn list(&self) -> Vec<String> {
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(root, &p, out);
                    } else if let Ok(r) = p.strip_prefix(root) {
                        out.push(r.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let mut v = Vec::new();
        walk(&self.0, &self.0, &mut v);
        v.sort();
        v
    }
}

/// `pack.json` of a pack.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct PackInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub format_version: u32,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub load_after: Vec<String>,
    #[serde(default)]
    pub generated_by: String,
}

pub struct Pack {
    pub info: PackInfo,
    pub src: Box<dyn FileSource>,
}

/// The merged view of `base` + mods.
pub struct Packs {
    /// In load order (base first; a later pack wins).
    pub packs: Vec<Pack>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Error(m.into()))
}

impl Packs {
    /// `sources[0]` must be the base pack. The others are mods in any order; they are sorted by their
    /// `depends` / `load_after` (then id). A mod whose dependency is missing is skipped with a warning.
    pub fn new(sources: Vec<Box<dyn FileSource>>) -> Result<Packs> {
        let mut warnings = Vec::new();
        let mut packs: Vec<Pack> = Vec::new();
        for (i, src) in sources.into_iter().enumerate() {
            let info = match src.read("pack.json") {
                Some(b) => serde_json::from_slice::<PackInfo>(&b).map_err(|e| Error(format!("pack {i}: pack.json: {e}")))?,
                None if i == 0 => return err("the base content pack has no pack.json (extract the ROM first)"),
                None => PackInfo { id: format!("mod{i}"), ..Default::default() },
            };
            packs.push(Pack { info, src });
        }
        if packs.is_empty() {
            return err("no content packs");
        }
        let base = packs.remove(0);
        // deterministic order: Kahn's algorithm over depends + load_after, ready set ordered by id
        let ids: BTreeSet<String> = packs.iter().map(|p| p.info.id.clone()).chain([base.info.id.clone()]).collect();
        let mut keep = Vec::new();
        for p in packs {
            if let Some(d) = p.info.depends.iter().find(|d| !ids.contains(*d)) {
                warnings.push(format!("mod {}: missing dependency {d}, not loaded", p.info.id));
            } else {
                keep.push(p);
            }
        }
        let mut placed: BTreeSet<String> = [base.info.id.clone()].into();
        let mut ordered = vec![base];
        while !keep.is_empty() {
            let present: BTreeSet<String> = keep.iter().map(|p| p.info.id.clone()).collect();
            let mut ready: Vec<usize> = (0..keep.len())
                .filter(|&i| {
                    let p = &keep[i].info;
                    p.depends.iter().chain(&p.load_after).all(|d| placed.contains(d) || !present.contains(d))
                })
                .collect();
            if ready.is_empty() {
                warnings.push("mods: dependency cycle, remaining mods loaded by id".into());
                ready = (0..keep.len()).collect();
            }
            ready.sort_by(|&a, &b| keep[a].info.id.cmp(&keep[b].info.id));
            let i = ready[0];
            let p = keep.remove(i);
            placed.insert(p.info.id.clone());
            ordered.push(p);
        }
        Ok(Packs { packs: ordered, warnings })
    }

    pub fn base(&self) -> &PackInfo {
        &self.packs[0].info
    }

    /// The merged file at `path`: the last pack that has it wins; a later `<path>.delete` (or a `.delete`
    /// marker on any parent directory, e.g. `sprites/106_goku.delete`) removes it; for `x.json`, later packs'
    /// `x.patch.json` are merge-patched onto it.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let patch = path.strip_suffix(".json").map(|s| format!("{s}.patch.json"));
        let mut cur: Option<Vec<u8>> = None;
        for p in &self.packs {
            if p.src.exists(&format!("{path}.delete"))
                || path.match_indices('/').any(|(i, _)| p.src.exists(&format!("{}.delete", &path[..i])))
            {
                cur = None;
            }
            if let Some(b) = p.src.read(path) {
                cur = Some(b);
            } else if let (Some(pp), Some(c)) = (&patch, &cur) {
                if let Some(pb) = p.src.read(pp) {
                    if let (Ok(mut doc), Ok(pv)) =
                        (serde_json::from_slice::<serde_json::Value>(c), serde_json::from_slice::<serde_json::Value>(&pb))
                    {
                        merge_patch(&mut doc, &pv);
                        cur = serde_json::to_vec(&doc).ok();
                    }
                }
            }
        }
        cur
    }

    pub fn read_str(&self, path: &str) -> Result<String> {
        let b = self.read(path).ok_or_else(|| Error(format!("missing content file {path}")))?;
        String::from_utf8(b).map_err(|_| Error(format!("{path}: not UTF-8")))
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let b = self.read(path).ok_or_else(|| Error(format!("missing content file {path}")))?;
        serde_json::from_slice(&b).map_err(|e| Error(format!("{path}: {e}")))
    }

    pub fn value(&self, path: &str) -> Result<serde_json::Value> {
        self.json(path)
    }

    /// Every path of the merged content (union of the packs' listings minus deletions).
    pub fn list(&self) -> Vec<String> {
        let mut all = BTreeSet::new();
        for p in &self.packs {
            for f in p.src.list() {
                if let Some(d) = f.strip_suffix(".delete") {
                    all.retain(|x: &String| x != d && !x.starts_with(&format!("{d}/")));
                } else if let Some(s) = f.strip_suffix(".patch.json") {
                    all.insert(format!("{s}.json"));
                } else {
                    all.insert(f);
                }
            }
        }
        all.into_iter().collect()
    }
}

/// RFC 7396 JSON Merge Patch.
pub fn merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    use serde_json::Value;
    if let Value::Object(pm) = patch {
        if !target.is_object() {
            *target = Value::Object(Default::default());
        }
        let tm = target.as_object_mut().unwrap();
        for (k, v) in pm {
            if v.is_null() {
                tm.remove(k);
            } else {
                merge_patch(tm.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
    } else {
        *target = patch.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(files: &[(&str, &str)]) -> Box<dyn FileSource> {
        Box::new(MemSource(files.iter().map(|(k, v)| (k.to_string(), v.as_bytes().to_vec())).collect()))
    }

    #[test]
    fn overlay_patch_delete_order() {
        let base = mem(&[
            ("pack.json", r#"{"id":"base"}"#),
            ("a.json", r#"{"x":1,"y":{"z":2}}"#),
            ("b.json", "1"),
            ("sprites/s/sprite.json", "{}"),
        ]);
        let m2 = mem(&[("pack.json", r#"{"id":"b_mod","depends":["base"],"load_after":["a_mod"]}"#), ("a.patch.json", r#"{"y":{"z":3}}"#)]);
        let m1 = mem(&[
            ("pack.json", r#"{"id":"a_mod"}"#),
            ("a.json", r#"{"x":5,"y":{"z":9}}"#),
            ("b.json.delete", ""),
            ("sprites/s.delete", ""),
        ]);
        let p = Packs::new(vec![base, m2, m1]).unwrap();
        let order: Vec<&str> = p.packs.iter().map(|p| p.info.id.as_str()).collect();
        assert_eq!(order, ["base", "a_mod", "b_mod"]);
        let a: serde_json::Value = p.json("a.json").unwrap();
        assert_eq!(a, serde_json::json!({"x":5,"y":{"z":3}}));
        assert!(p.read("b.json").is_none());
        assert!(p.read("sprites/s/sprite.json").is_none());
    }

    #[test]
    fn merge_patch_rfc() {
        let mut t = serde_json::json!({"a":"b","c":{"d":"e","f":"g"}});
        merge_patch(&mut t, &serde_json::json!({"a":"z","c":{"f":null}}));
        assert_eq!(t, serde_json::json!({"a":"z","c":{"d":"e"}}));
    }
}
