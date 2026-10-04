//! Area-local state against the game's (report-only, `SEGMENT_AREA=1`; a gate with `"area_exact": true` fails on any
//! difference): `tools/segments/area.py` (the soak agent's) writes the game's field lists along a segment
//! (data/segments/area/<name>.json, delta rows): the trigger list in list order (class, rect, an event trigger's resume
//! node; coin touch triggers present only) and the walk-grid tiles changed since the segment's start (broken
//! breakables, opened doors, script-set blocks; null once the segment left its first area). The replay compares
//! ours at the rows where the game's changed while the segment matches.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use wf_core::world::{Body, World};

const VT_COIN_TOUCH: i64 = 0x0804_A0C0;
const VT_EVENT: i64 = 0x0804_9BD4;

pub fn load(doc: &Value) -> Option<Vec<(usize, BTreeMap<String, Value>)>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (name, off) = match doc["source"]["kind"].as_str() {
        Some("tile") => (doc["source"]["of"].as_str()?.to_string(), doc["source"]["at"].as_i64()? as usize),
        _ => (doc["name"].as_str()?.to_string(), 0),
    };
    // (a tile's walk-grid changes are relative to its parent's start: not comparable - tiles compare triggers only)
    let mut paths = vec![root.join("data/segments/area").join(format!("{name}.json"))];
    if let Ok(d) = std::env::var("SEGMENT_AREA_DIR") {
        paths.insert(0, PathBuf::from(d).join(format!("{name}.json")));
    }
    let v: Value = paths.iter().find_map(|p| serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok())?;
    let mut rows = vec![];
    let mut cur: BTreeMap<String, Value> = BTreeMap::new();
    for r in v["rows"].as_array()? {
        let k = r[0].as_u64()? as usize;
        for (f, val) in r[1].as_object()? {
            cur.insert(f.clone(), val.clone());
        }
        rows.push((k, cur.clone()));
    }
    if off > 0 {
        let mut first = rows.iter().rev().find(|r| r.0 <= off)?.1.clone();
        first.insert("wg".into(), Value::Null);
        let mut out = vec![(0, first)];
        out.extend(rows.iter().filter(|r| r.0 > off).map(|r| {
            let mut s = r.1.clone();
            s.insert("wg".into(), Value::Null);
            (r.0 - off, s)
        }));
        return Some(out);
    }
    Some(rows)
}

/// the walk grid (1 = free) of the current area, for the change list against the segment's start
pub fn walk_grid(w: &World) -> Option<Vec<u8>> {
    let c = w.col.as_ref()?;
    Some(c.walk.clone().unwrap_or_else(|| c.map.walk.clone()))
}

/// the fields that differ
pub fn diff(w: &World, g0: Option<&[u8]>, game: &BTreeMap<String, Value>) -> Vec<String> {
    let mut out = vec![];
    let Some(f) = w.field.as_ref() else { return out };
    // triggers: ours in list order (event triggers with their resume node), then the coins' touch triggers
    let ours: Vec<(i64, i64, i64, i64, i64)> = f
        .triggers
        .iter()
        .map(|t| {
            let ev = wf_core::field::is_event_trigger(w, t.obj);
            (t.rect.x0 as i64, t.rect.y0 as i64, t.rect.x1 as i64, t.rect.y1 as i64, if ev { t.resume as i64 } else { 0 })
        })
        .collect();
    let coins = w.ents.iter().filter(|e| !e.removed && matches!(&e.body, Body::Pickup(p) if !p.by_a())).count();
    let mut theirs = vec![];
    let mut their_coins = 0;
    for t in game.get("trig").and_then(|x| x.as_array()).into_iter().flatten() {
        let a: Vec<i64> = t.as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default();
        if a.len() < 6 {
            continue;
        }
        if a[0] == VT_COIN_TOUCH {
            their_coins += 1;
            continue;
        }
        let _ = VT_EVENT;
        theirs.push((a[1], a[2], a[3], a[4], a[5]));
    }
    // (ours is the game's list reversed: the game scans its list last to first, ours first to last - the same order)
    let mut ours = ours;
    ours.reverse();
    if ours != theirs {
        let fmt = |v: &[(i64, i64, i64, i64, i64)]| v.iter().map(|t| format!("({},{},{},{} r{})", t.0, t.1, t.2, t.3, t.4)).collect::<Vec<_>>().join(" ");
        // the first difference and the counts (whole lists are long)
        let i = ours.iter().zip(theirs.iter()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
        out.push(format!(
            "triggers: {} vs {} - from #{i}: {} vs {}",
            ours.len(),
            theirs.len(),
            fmt(&ours[i.min(ours.len())..(i + 2).min(ours.len())]),
            fmt(&theirs[i.min(theirs.len())..(i + 2).min(theirs.len())])
        ));
    }
    if coins != their_coins {
        out.push(format!("coin touch triggers: {coins} vs {their_coins}"));
    }
    // the map NPCs' talk resume nodes (the nonzero ones, sorted)
    if let Some(gn) = game.get("npc").and_then(|x| x.as_array()) {
        let theirs: Vec<i64> = gn.iter().filter_map(|x| x.as_i64()).collect();
        let mut ours: Vec<i64> = w
            .ents
            .iter()
            .filter(|e| !e.removed)
            .filter_map(|e| match &e.body {
                Body::Npc(n) if n.behaviour.is_some() && n.resume != 0 => Some(n.resume as i64),
                _ => None,
            })
            .collect();
        ours.sort();
        if ours != theirs {
            out.push(format!("npc talk resume nodes: {ours:?} vs {theirs:?}"));
        }
    }
    // the walk grid's changes since the segment's start
    if let (Some(gw), Some(g0), Some(g)) = (game.get("wg").and_then(|x| x.as_array()), g0, walk_grid(w)) {
        let theirs: Vec<i64> = gw.iter().filter_map(|x| x.as_i64()).collect();
        let ours: Vec<i64> = (0..g.len().min(g0.len())).filter(|&i| g[i] != g0[i]).map(|i| i as i64).collect();
        if ours != theirs {
            let only_ours: Vec<i64> = ours.iter().filter(|x| !theirs.contains(x)).copied().take(6).collect();
            let only_theirs: Vec<i64> = theirs.iter().filter(|x| !ours.contains(x)).copied().take(6).collect();
            out.push(format!("walk grid: {} vs {} changed tiles - ours only {only_ours:?} game only {only_theirs:?} (ty * 256 + tx)", ours.len(), theirs.len()));
        }
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
    pub fn check(&mut self, k: usize, w: &World, g0: Option<&[u8]>, game: &BTreeMap<String, Value>) {
        self.checks += 1;
        let d = diff(w, g0, game);
        if !d.is_empty() && std::env::var("SEGMENT_AREA").is_ok_and(|v| v == "all") {
            eprintln!("    area f{k} {d:?}");
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
