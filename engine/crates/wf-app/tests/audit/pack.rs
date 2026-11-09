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

const W: usize = 240;
const H: usize = 160;

/// The tear rule (the pixel gate's, core.md known differences): the game rewrites the palette / BG scroll registers
/// and copies the map's animated tiles from its main loop while the screen is being drawn, so the lines above a
/// write show the previous frame's state. -> per line "" (our picture), a candidate's code (the audit's
/// `with_prev`), or a mix inside a tile copy still running: a scroll state's code + "~" + a hex mask of the pixels
/// that show the old tiles under it (the others the new ones). Accepted only when
/// every line of the game's picture is one of them AND the lines only an old state explains are all above the lines
/// only our new picture explains, with at least one of each (a picture old on every line would be a frame late, not
/// torn); the HUD's own rule below.
pub fn torn_spec(game: &[u32], ours: &[u32], cands: &[(String, Vec<u32>)]) -> Option<Vec<String>> {
    let pic = |l: &str| cands.iter().find(|c| c.0 == l).map(|c| c.1.as_slice()).unwrap_or(ours);
    let _ = &pic;
    let mix = |y: usize, a: &[u32], b: &[u32]| -> Option<String> {
        let mut m = [0u8; W / 8];
        for x in 0..W {
            let k = y * W + x;
            if game[k] == a[k] {
            } else if game[k] == b[k] {
                m[x / 8] |= 1 << (x % 8);
            } else {
                return None;
            }
        }
        Some(m.iter().map(|b| format!("{b:02x}")).collect())
    };
    let mut spec: Vec<String> = Vec::with_capacity(H);
    let mut first_new = None;
    for y in 0..H {
        let rg = y * W..(y + 1) * W;
        let new = game[rg.clone()] == ours[rg.clone()];
        let old = cands.iter().find(|c| c.1[rg.clone()] == game[rg.clone()]).map(|c| c.0.clone());
        match (new, old) {
            (true, None) => {
                first_new.get_or_insert(y);
                spec.push(String::new());
            }
            (true, Some(_)) => spec.push(String::new()),
            (false, Some(l)) => spec.push(l),
            // (inside the animated tiles' copy: each pixel the old or the new tiles' under one scroll state)
            (false, None) => {
                let base = std::iter::once("").chain(cands.iter().map(|c| c.0.as_str()).filter(|c| !c.contains('t') && !c.contains('h') && *c != "o" && *c != "u"));
                let m = base.filter(|b| cands.iter().any(|c| c.0 == format!("{b}t"))).find_map(|b| Some(format!("{b}~{}", mix(y, pic(b), pic(&format!("{b}t")))?)));
                spec.push(m?);
            }
        }
    }
    // the HUD's tile copy alone ("h"): nothing below the gauge changes with it, so no line under it is new-only - it
    // is accepted on the lines above scanline 27, where the copy was measured on the frames that differ (playtest.md
    // "HUD gauge steps a frame late": emulator breakpoint at 0x0800BCBC, VCOUNT 26-27 against 215 on the others)
    let last_hud = spec.iter().rposition(|l| l == "h");
    let last_old = spec.iter().rposition(|l| !l.is_empty() && l != "h");
    let hud_ok = last_hud.is_none_or(|y| y < 27);
    let old_ok = match (last_old, first_new) {
        (None, _) => true,
        (Some(a), Some(b)) => a < b,
        (Some(_), None) => false,
    };
    (hud_ok && old_ok && (last_hud.is_some() || last_old.is_some())).then_some(spec)
}

/// a torn frame's picture from ours, the candidates and its line specs
pub fn compose(ours: &mut [u32], spec: &[String], cand: &mut dyn FnMut(&str) -> Vec<u32>) {
    let mut pics: std::collections::BTreeMap<String, Vec<u32>> = std::collections::BTreeMap::new();
    let mut get = |l: &str| pics.entry(l.to_string()).or_insert_with(|| cand(l)).clone();
    for (y, sp) in spec.iter().enumerate().take(H) {
        let rg = y * W..(y + 1) * W;
        if sp.is_empty() {
            continue;
        }
        // (a mix: the scroll state's code, "~", the 60 hex digits of the mask of the pixels with the old tiles)
        if let Some((base, mask)) = sp.split_once('~') {
            let (a, b) = ((!base.is_empty()).then(|| get(base)), get(&format!("{base}t")));
            for x in 0..W {
                let bit = u8::from_str_radix(mask.get(x / 8 * 2..2 + x / 8 * 2).unwrap_or("00"), 16).unwrap_or(0) >> (x % 8) & 1;
                if bit != 0 {
                    ours[y * W + x] = b[y * W + x];
                } else if let Some(a) = &a {
                    ours[y * W + x] = a[y * W + x];
                }
            }
        } else {
            let p = get(sp);
            ours[rg.clone()].copy_from_slice(&p[rg]);
        }
    }
}
