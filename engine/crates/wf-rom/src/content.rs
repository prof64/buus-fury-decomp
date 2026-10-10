//! In-memory content tree written by the extractors: relative path (`/`-separated, under the content
//! root, e.g. `sprites/goku/sheet.png`) → bytes. The CLI writes it to disk; the wasm build hands it to JS.

use std::collections::BTreeMap;

#[derive(Default)]
pub struct Content {
    pub files: BTreeMap<String, Vec<u8>>,
    /// Non-fatal problems (a record that failed to decode, a missing optional input); written to
    /// `extract_report.json`.
    pub warnings: Vec<String>,
}

impl Content {
    pub fn new() -> Content {
        Content::default()
    }
    pub fn put(&mut self, path: impl Into<String>, data: Vec<u8>) {
        self.files.insert(path.into(), data);
    }
    /// Pretty JSON with a trailing newline (diff-friendly for modders).
    pub fn put_json(&mut self, path: impl Into<String>, v: &serde_json::Value) {
        let mut s = serde_json::to_vec_pretty(v).expect("json");
        s.push(b'\n');
        self.put(path, s);
    }
    pub fn warn(&mut self, w: impl Into<String>) {
        self.warnings.push(w.into());
    }
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|v| v.as_slice())
    }
    pub fn json(&self, path: &str) -> Option<serde_json::Value> {
        self.get(path).and_then(|b| serde_json::from_slice(b).ok())
    }
    pub fn total_bytes(&self) -> usize {
        self.files.values().map(|v| v.len()).sum()
    }
    /// Move every file of `other` under `prefix/`.
    pub fn merge(&mut self, prefix: &str, other: Content) {
        for (k, v) in other.files {
            let p = if prefix.is_empty() { k } else { format!("{prefix}/{k}") };
            self.files.insert(p, v);
        }
        self.warnings.extend(other.warnings);
    }
}
