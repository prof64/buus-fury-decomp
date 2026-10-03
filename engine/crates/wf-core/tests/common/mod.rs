//! Test content: the extractor's base pack, built from the user's ROM (roms/buu.gba) once and cached under the
//! cargo target dir; `WF_CONTENT=<content/base dir>` uses an existing extraction instead. Recorded traces are read
//! from the repository (`WF_DATA` overrides the data/battle directory).
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use wf_core::content::{ContentSource, DirSource, GameData};

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn battle_data(name: &str) -> PathBuf {
    match std::env::var("WF_DATA") {
        Ok(d) => PathBuf::from(d).join(name),
        Err(_) => repo().join("data/battle").join(name),
    }
}

pub fn json(name: &str) -> serde_json::Value {
    let p = battle_data(name);
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

fn content_dir() -> PathBuf {
    extracted("wf-core-content", &["tables", "sprites", "effects", "maps"])
}

/// the field tests need the world objects, items and script bytecode too (`WF_FIELD_CONTENT` overrides)
fn field_content_dir() -> PathBuf {
    if let Ok(d) = std::env::var("WF_FIELD_CONTENT") {
        return PathBuf::from(d);
    }
    extracted("wf-core-field-content-2", &["tables", "sprites", "effects", "maps", "world", "items", "scripts"])
}

pub fn field_data() -> Arc<GameData> {
    static D: OnceLock<Arc<GameData>> = OnceLock::new();
    D.get_or_init(|| {
        let src = DirSource { root: field_content_dir() };
        Arc::new(GameData::load(&src).expect("load content"))
    })
    .clone()
}

fn extracted(name: &str, only: &[&str]) -> PathBuf {
    if name == "wf-core-content" {
        if let Ok(d) = std::env::var("WF_CONTENT") {
            return PathBuf::from(d);
        }
    }
    // re-extracted when the extractor's source hash in the `.complete` marker differs (wf_extract::cache)
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    wf_extract::cache::ensure(&dir, &repo().join("roms/buu.gba"), only)
}

pub fn data() -> Arc<GameData> {
    static D: OnceLock<Arc<GameData>> = OnceLock::new();
    D.get_or_init(|| {
        let src = DirSource { root: content_dir() };
        let _ = src.list();
        Arc::new(GameData::load(&src).expect("load content"))
    })
    .clone()
}
