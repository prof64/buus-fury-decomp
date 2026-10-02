//! Decompile everything to Rhai source files (content-pack layout `scripts/…`).

use crate::analysis::decompile_set;
use crate::rhai_out::{self, Names};
use crate::rom::{inline_list, ScriptId, ScriptSet};
use std::collections::BTreeMap;
use wf_rom::Rom;

#[derive(Clone, Debug, serde::Deserialize, Default)]
pub struct EventInfo {
    pub event: u16,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

/// Purpose / referrer summary per event (`engine/annotations/script_events.json`, from the catalog).
pub fn event_infos() -> BTreeMap<u16, EventInfo> {
    let v: Vec<EventInfo> =
        serde_json::from_str(include_str!("../../../annotations/script_events.json")).unwrap_or_default();
    v.into_iter().map(|e| (e.event, e)).collect()
}

/// Returns `path → source`: `scripts/events/sNNN.rhai`, `scripts/inline/inline_0xADDR.rhai` (ids as in the catalog / wf-world).
pub fn export(rom: &Rom, set: &ScriptSet) -> (BTreeMap<String, String>, Vec<(ScriptId, String)>) {
    let strings = wf_rom::text::StringTable::load(rom);
    let lookup = |i: i32| -> Option<String> {
        if i < 0 {
            return None;
        }
        strings.get(i as usize).map(|s| s.chars().take(60).collect())
    };
    let names = Names { string: &lookup };
    let (progs, _, errors) = decompile_set(set);
    let infos = event_infos();
    let mut out = BTreeMap::new();
    for ev in set.events.values() {
        let info = infos.get(&ev.index);
        let c = match info {
            Some(i) => format!(
                "Purpose: {}. Referenced by: {}.",
                i.purpose.as_deref().unwrap_or("?"),
                if i.refs.is_empty() { "nothing static".to_string() } else { i.refs.join(", ") }
            ),
            None => String::new(),
        };
        out.insert(format!("scripts/events/s{:03}.rhai", ev.index), rhai_out::event(ev, set, &progs, &names, &c));
    }
    let inl: BTreeMap<u32, _> = inline_list()
        .into_iter()
        .filter_map(|i| u32::from_str_radix(i.addr.trim_start_matches("0x"), 16).ok().map(|a| (a, i)))
        .collect();
    for (id, p) in &progs {
        if let ScriptId::Inline(a) = id {
            let mut s = String::new();
            if let Some(i) = inl.get(a) {
                s.push_str(&format!(
                    "// Inline script 0x{a:08X}. Purpose: {}. Referenced by: {}.\n",
                    i.purpose.as_deref().unwrap_or("?"),
                    i.r#where.join("; ")
                ));
            }
            s.push_str(&rhai_out::program(p, "", &names));
            out.insert(format!("scripts/inline/inline_0x{a:08X}.rhai"), s);
        }
    }
    (out, errors)
}
