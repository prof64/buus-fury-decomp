//! The audits' committed data (`data/audits/{opp,skill}/NAME.json.z`): a scenario's recording as zlib'd JSON, with
//! `pic.hash` = the FNV-1a 64 of the game's picture of every frame (16 hex digits each, frame 0 first). No pictures.
#![allow(dead_code)]

use serde_json::Value;
use std::path::Path;

pub fn read(p: &Path) -> Result<Value, String> {
    let z = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let b = miniz_oxide::inflate::decompress_to_vec_zlib(&z).map_err(|e| format!("{}: {e:?}", p.display()))?;
    serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn write(p: &Path, doc: &Value) {
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&serde_json::to_vec(doc).unwrap(), 9);
    std::fs::write(p, z).unwrap();
}

pub fn hex(h: &[u64]) -> String {
    h.iter().map(|h| format!("{h:016x}")).collect()
}

pub fn hashes(doc: &Value) -> Vec<u64> {
    let s = doc["pic"]["hash"].as_str().unwrap_or("");
    (0..s.len() / 16).map(|k| u64::from_str_radix(&s[k * 16..k * 16 + 16], 16).unwrap_or(0)).collect()
}

/// FNV-1a 64 over the picture's ARGB words (little-endian), as the pixel gate's (tests/pixels.rs)
pub fn hash(px: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in px {
        for b in c.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}
