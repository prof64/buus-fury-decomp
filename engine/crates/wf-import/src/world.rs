//! World state against the game's (report-only, `SEGMENT_WORLD=1`; a gate with `"world_exact": true` fails on any
//! difference): `tools/segments/world.py` writes the game's state outside the character records along a segment
//! (data/segments/world/<name>.json, delta rows) - GameData's game bits, the 1,144 story flags (event progress, opened
//! chests, defeated bosses, map-visited), the countdown / stopwatch, the Holy Water timer and bonuses, the timed
//! special, the script words L0..L3 / G0..G3 and the variant byte; the replay compares ours at those rows while the
//! segment matches. The character records are `recs.rs`'s.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use wf_core::world::World;
use wf_script_core::HostState;

/// the game's state rows for a segment document (by name; a tile reads its parent's rows from its offset):
/// data/segments/world/<name>.json, else `SEGMENT_WORLD_DIR`/<name>.json
pub fn load(doc: &Value) -> Option<Vec<(usize, BTreeMap<String, Value>)>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (name, off) = match doc["source"]["kind"].as_str() {
        Some("tile") => (doc["source"]["of"].as_str()?.to_string(), doc["source"]["at"].as_i64()? as usize),
        _ => (doc["name"].as_str()?.to_string(), 0),
    };
    let mut paths = vec![root.join("data/segments/world").join(format!("{name}.json"))];
    if let Ok(d) = std::env::var("SEGMENT_WORLD_DIR") {
        paths.insert(0, PathBuf::from(d).join(format!("{name}.json")));
    }
    let v: Value = paths.iter().find_map(|p| serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok())?;
    let mut rows = vec![];
    let mut cur: BTreeMap<String, Value> = BTreeMap::new();
    for r in v["rows"].as_array()? {
        let k = r[0].as_u64()? as usize;
        for (f, val) in r[1].as_object()? {
            if f == "fl" {
                let hex = cur.get("flags").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let mut b = unhex(&hex);
                for (ix, by) in val.as_object()? {
                    if let (Ok(ix), Some(by)) = (ix.parse::<usize>(), by.as_u64()) {
                        if ix < b.len() {
                            b[ix] = by as u8;
                        }
                    }
                }
                cur.insert("flags".into(), Value::String(b.iter().map(|x| format!("{x:02x}")).collect()));
            } else {
                cur.insert(f.clone(), val.clone());
            }
        }
        rows.push((k, cur.clone()));
    }
    if off > 0 {
        let first = rows.iter().rev().find(|r| r.0 <= off)?.1.clone();
        let mut out = vec![(0, first)];
        out.extend(rows.iter().filter(|r| r.0 > off).map(|r| (r.0 - off, r.1.clone())));
        return Some(out);
    }
    Some(rows)
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|j| u8::from_str_radix(&s[2 * j..2 * j + 2], 16).ok()).collect()
}

fn ints(v: Option<&Value>) -> Vec<i64> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default()
}

/// the fields that differ (`field: ours vs game`; a story flag by its number)
pub fn diff(w: &World, hs: &HostState, running: bool, last: bool, game: &BTreeMap<String, Value>) -> Vec<String> {
    let mut out = vec![];
    let st = &w.story;
    let gi = |f: &str| game.get(f).and_then(|x| x.as_i64()).unwrap_or(0);
    // the game bits: HUD shown (bit 1) is the scene's, the countdown / stopwatch running (bits 2 / 4) the host's timers
    let t = &w.scene.timers;
    let bits = (st.game_bits as i64 & 0xFF & !0x16) | if w.scene.hud_hidden { 0 } else { 2 } | if t.countdown_on { 4 } else { 0 } | if t.stopwatch_on { 0x10 } else { 0 };
    if bits != gi("bits") {
        out.push(format!("game bits: {bits:#x} vs {:#x}", gi("bits")));
    }
    let gf = unhex(game.get("flags").and_then(|x| x.as_str()).unwrap_or(""));
    let mut fl = vec![];
    for (ix, &g) in gf.iter().enumerate() {
        let o = st.flags.get(ix).copied().unwrap_or(0);
        for b in 0..8 {
            if (o ^ g) >> b & 1 != 0 {
                fl.push(format!("{}{}", if g >> b & 1 != 0 { "-" } else { "+" }, ix * 8 + b));
            }
        }
    }
    if !fl.is_empty() {
        // "+n": set in ours only, "-n": set in the game's only
        out.push(format!("story flags: {}", fl.join(" ")));
    }
    // (the timers' values are kept at the game's change rows only: not at the segment's last row)
    if t.countdown_on && t.countdown as i64 != gi("cd") && !last {
        out.push(format!("countdown: {} vs {}", t.countdown, gi("cd")));
    }
    if t.stopwatch_on && t.stopwatch as i64 != gi("sw") && !last {
        out.push(format!("stopwatch: {} vs {}", t.stopwatch, gi("sw")));
    }
    if let Some(i) = w.player_idx(0) {
        let p = w.pl_ref(i);
        let holy = ints(game.get("holy"));
        // (the bonuses only while it runs: both keep their last values after it, read only while it runs)
        let ours = [p.holy_left as i64, p.boost[0] as i64, p.boost[1] as i64, p.boost[2] as i64, p.boost[3] as i64];
        let n = if p.holy_left > 0 || holy.first().copied().unwrap_or(0) > 0 { 5 } else { 1 };
        if holy.len() == 5 && ours[..n] != holy[..n] {
            out.push(format!("holy water: {ours:?} vs {holy:?}"));
        }
    }
    if hs.boost[5] as i64 != gi("spec") {
        out.push(format!("timed special: {} vs {}", hs.boost[5], gi("spec")));
    }
    let (gl, gg) = (ints(game.get("l")), ints(game.get("g")));
    let (ol, og): (Vec<i64>, Vec<i64>) = (st.l.iter().map(|&x| x as i64).collect(), st.g.iter().map(|&x| x as i64).collect());
    if gl.len() == 4 && ol != gl {
        out.push(format!("words L: {ol:?} vs {gl:?}"));
    }
    // (G0..G3 are the scripts' scratch: ours are written back when a script ends - compared with none running)
    if gg.len() == 4 && og != gg && !running && !last {
        out.push(format!("globals G: {og:?} vs {gg:?}"));
    }
    if st.variant_byte as i64 != gi("var") {
        out.push(format!("variant byte: {} vs {}", st.variant_byte, gi("var")));
    }
    out
}

/// the report of one replay (as `recs::Tally`)
#[derive(Default, Debug)]
pub struct Tally {
    pub checks: usize,
    pub bad_rows: usize,
    pub first: Option<(usize, Vec<String>)>,
    pub fields: BTreeMap<String, usize>,
}

impl Tally {
    /// `last`: the segment's last row, not one of the game's change rows (its scratch values may be older)
    pub fn check(&mut self, k: usize, w: &World, hs: &HostState, running: bool, last: bool, game: &BTreeMap<String, Value>) {
        self.checks += 1;
        let d = diff(w, hs, running, last, game);
        if !d.is_empty() && std::env::var("SEGMENT_WORLD").is_ok_and(|v| v == "all") {
            eprintln!("    world f{k} {d:?}");
        }
        if d.is_empty() {
            return;
        }
        self.bad_rows += 1;
        for x in &d {
            *self.fields.entry(x.split(':').next().unwrap_or("").to_string()).or_default() += 1;
        }
        if self.first.is_none() {
            self.first = Some((k, d));
        }
    }
}
