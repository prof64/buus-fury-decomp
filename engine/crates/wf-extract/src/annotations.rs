//! Our own research shipped with the extractor (not ROM data): curated names / categories / animation
//! names (`engine/annotations/sprite_names.json`, a copy of `tools/extract/data/names_buu.json`) and
//! emulator-measured animation timing (`anim_timing_measured.json`, condensed from
//! `tools/extract/data/anim_obs_buu.json` by `engine/tools/make_anim_timing_annotation.py`).
//!
//! `anim_timing_decoded.json` is the hook for the per-animation timing decoded from the game code (being
//! produced separately; expected in `data/battle/buu_anim_timing.json`). Format, keyed by anim struct address:
//! `{"0850C1DC": {"durations": [..per frame..], "loop": bool, "source": "decoded ..."}}`. Entries there win
//! over measured and curated defaults. TODO: fill it when that work lands (it is `{}` until then).

use serde_json::{Map, Value};
use std::collections::HashMap;

const NAMES: &str = include_str!("../../../annotations/sprite_names.json");
const MEASURED: &str = include_str!("../../../annotations/anim_timing_measured.json");
const DECODED: &str = include_str!("../../../annotations/anim_timing_decoded.json");

pub struct Names {
    d: Value,
    used: HashMap<(u32, String), usize>,
}

pub struct EntityInfo {
    pub name: String,
    pub slug: String,
    pub category: String,
    pub confidence: String,
    pub notes: Value,
}

impl Names {
    pub fn load() -> Names {
        Names { d: serde_json::from_str(NAMES).expect("sprite_names.json"), used: HashMap::new() }
    }
    fn ent(&self, idx: u32) -> &Value {
        &self.d["entities"][idx.to_string()]
    }
    pub fn entity(&self, idx: u32) -> EntityInfo {
        let e = self.ent(idx);
        let name = e["name"].as_str().map(String::from).unwrap_or(format!("Sprite {idx}"));
        let slug = e["slug"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("{idx:03}_{}", crate::sheet::slugify(&name)));
        EntityInfo {
            slug,
            category: e["category"].as_str().unwrap_or("npc").into(),
            confidence: e["confidence"]
                .as_str()
                .unwrap_or(if e.get("name").is_some() { "medium" } else { "low" })
                .into(),
            notes: e.get("notes").cloned().unwrap_or(Value::Null),
            name,
        }
    }
    pub fn anim_name(&mut self, idx: u32, ai: usize) -> String {
        let e = self.ent(idx);
        let k = ai.to_string();
        let mut nm = e["anims"][&k].as_str().map(String::from);
        if nm.is_none() {
            if let Some(set) = e["anim_set"].as_str() {
                nm = self.d["anim_sets"][set][&k].as_str().map(String::from);
            }
        }
        let mut nm = nm.unwrap_or(format!("anim{ai:02}"));
        if let Some(&prev) = self.used.get(&(idx, nm.clone())) {
            if prev != ai {
                nm = format!("{nm}_{ai:02}");
            }
        }
        self.used.insert((idx, nm.clone()), ai);
        nm
    }
    /// Curated per-entity timing, else the entity's animation-set timing.
    pub fn default_timing(&self, idx: u32, ai: usize) -> Option<Map<String, Value>> {
        let e = self.ent(idx);
        let k = ai.to_string();
        if let Some(t) = e["timing"][&k].as_object() {
            return Some(t.clone());
        }
        let set = e["anim_set"].as_str()?;
        self.d["set_timing"][set][&k].as_object().cloned()
    }
    pub fn portrait(&self, i: usize) -> (Option<String>, String) {
        let p = &self.d["portraits"][i.to_string()];
        (p["name"].as_str().map(String::from), p["confidence"].as_str().unwrap_or("low").into())
    }
}

pub struct Timing {
    measured: Value,
    decoded: Value,
}

impl Timing {
    pub fn load() -> Timing {
        Timing {
            measured: serde_json::from_str(MEASURED).expect("anim_timing_measured.json"),
            decoded: serde_json::from_str(DECODED).expect("anim_timing_decoded.json"),
        }
    }
    /// Decoded timing (from game code) for the anim struct at `addr`, if available.
    pub fn decoded(&self, addr: u32) -> Option<Map<String, Value>> {
        self.decoded[format!("{addr:08X}")].as_object().cloned()
    }
    /// Measured timing like `sprites_wf_export.load_timing`: per-frame medians, missing frames filled
    /// with the median of the known ones.
    pub fn measured(&self, addr: u32, nframes: usize) -> Option<Map<String, Value>> {
        let rec = self.measured.get(format!("{addr:08X}"))?;
        let med = rec["medians"].as_object()?;
        let durs: Vec<Option<i64>> =
            (0..nframes).map(|k| med.get(&k.to_string()).and_then(|v| v.as_i64()).filter(|&v| v != 0)).collect();
        let mut known: Vec<i64> = durs.iter().flatten().copied().collect();
        if known.is_empty() {
            return None;
        }
        known.sort();
        let fill = known[known.len() / 2];
        let mut m = Map::new();
        m.insert("durations".into(), durs.iter().map(|d| d.unwrap_or(fill)).collect::<Vec<_>>().into());
        m.insert(
            "source".into(),
            format!("measured (emulator, {} segments)", rec["count"].as_i64().unwrap_or(0)).into(),
        );
        m.insert("observed_sequences".into(), rec["observed_sequences"].clone());
        Some(m)
    }
}
