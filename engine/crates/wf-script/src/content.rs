//! Content-pack output (`wf-extract` category `scripts`):
//!
//! ```text
//! scripts/index.json                 every event / inline script: id, purpose, referrers, entry kind, files
//! scripts/events/sNNN.rhai           decompiled event (graph + node code)        ← what the engine runs
//! scripts/inline/inline_0xADDR.rhai  decompiled inline / drop script
//! scripts/bytecode.json              original bytecode (hex) + event graphs, for verification / the oracle VM
//! ```
//!
//! Ids are the catalog / wf-world ids (`sNNN`, `inline_0xADDR`), so `maps/<id>/objects.json` references
//! resolve to these files. Mods override a script by shipping the same path (or add new `sNNN`/named files).

use crate::analysis::decompile_set;
use crate::export::event_infos;
use crate::rhai_out::{self, Names};
use crate::rom::{enemy_drop_scripts, inline_list, Entry, NodeBody, ScriptId, ScriptSet};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use wf_rom::content::Content;
use wf_rom::Rom;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unit_json(set: &ScriptSet, id: ScriptId) -> Value {
    let sc = &set.scripts[&id];
    json!({
        "id": id.slug(),
        "code_addr": sc.code_addr.map(|a| format!("0x{a:08X}")),
        "stream_addr": sc.stream_addr.map(|a| format!("0x{a:08X}")),
        "compressed": sc.compressed,
        "code": hex(&sc.code),
    })
}

pub fn extract(rom: &Rom, out: &mut Content) -> wf_rom::Result<()> {
    let set = ScriptSet::load_all(rom);
    for (id, e) in &set.errors {
        out.warn(format!("scripts: {}: {e}", id.slug()));
    }
    let strings = wf_rom::text::StringTable::load(rom);
    let lookup = |i: i32| -> Option<String> {
        if i < 0 {
            return None;
        }
        strings.get(i as usize).map(|s| s.chars().take(60).collect())
    };
    let names = Names { string: &lookup };
    let (progs, _, errors) = decompile_set(&set);
    for (id, e) in &errors {
        out.warn(format!("scripts: decompile {}: {e}", id.slug()));
    }
    let infos = event_infos();
    let mut index_events = Vec::new();
    let mut graphs = BTreeMap::new();
    for ev in set.events.values() {
        let info = infos.get(&ev.index);
        let comment = info
            .map(|i| format!("Purpose: {}. Referenced by: {}.", i.purpose.as_deref().unwrap_or("?"), i.refs.join(", ")))
            .unwrap_or_default();
        let path = format!("scripts/events/s{:03}.rhai", ev.index);
        out.put(path.clone(), rhai_out::event(ev, &set, &progs, &names, &comment).into_bytes());
        let mut nodes = Vec::new();
        for (k, e) in &ev.entries {
            nodes.push(match e {
                Entry::Terminal(v) => json!({"node": k, "end": v}),
                Entry::Node(n) => {
                    let mut o = json!({"node": k, "kind": n.kind, "arg": n.arg, "next": n.next, "addr": format!("0x{:08X}", n.addr)});
                    match &n.body {
                        NodeBody::Code(id) => o["code"] = unit_json(&set, *id),
                        NodeBody::TextArgs { text, ref_, args } => {
                            o["text"] = json!(text);
                            o["text_ref"] = json!(format!("0x{ref_:08X}"));
                            o["code"] = unit_json(&set, *args);
                        }
                        NodeBody::Text { text, ref_ } => {
                            o["text"] = json!(text);
                            o["text_ref"] = json!(format!("0x{ref_:08X}"));
                        }
                        NodeBody::Choice { text } => o["choice"] = json!(text),
                        NodeBody::Goto { header, event } => {
                            o["goto"] = json!(event.map(|e| format!("s{e:03}")));
                            o["goto_header"] = json!(format!("0x{header:08X}"));
                        }
                        NodeBody::Unknown { kind } => o["unknown_kind"] = json!(kind),
                    }
                    o
                }
            });
        }
        graphs.insert(format!("s{:03}", ev.index), json!({"header": format!("0x{:08X}", ev.header), "nodes": nodes}));
        let entry = match progs.get(&ScriptId::Node { event: ev.index, node: 0 }) {
            Some(p) => rhai_out::entry_kind(p),
            None => "graph",
        };
        index_events.push(json!({
            "id": format!("s{:03}", ev.index), "event": ev.index, "file": path,
            "purpose": info.and_then(|i| i.purpose.clone()), "referenced_by": info.map(|i| i.refs.clone()).unwrap_or_default(),
            "node0": entry, "dynamic_branch": ev.dynamic_branch,
        }));
    }
    let inl: BTreeMap<u32, _> = inline_list()
        .into_iter()
        .filter_map(|i| u32::from_str_radix(i.addr.trim_start_matches("0x"), 16).ok().map(|a| (a, i)))
        .collect();
    let drops: BTreeMap<u32, Vec<u32>> = enemy_drop_scripts(rom).into_iter().fold(BTreeMap::new(), |mut m, (t, a)| {
        m.entry(a).or_default().push(t);
        m
    });
    let mut index_inline = Vec::new();
    let mut inline_code = BTreeMap::new();
    for (id, p) in &progs {
        let ScriptId::Inline(a) = id else { continue };
        let slug = id.slug();
        let path = format!("scripts/inline/{slug}.rhai");
        let (purpose, refs, wh) = match (inl.get(a), drops.get(a)) {
            (Some(i), _) => (i.purpose.clone(), i.refs.clone(), i.r#where.clone()),
            (None, Some(t)) => (
                Some("enemy_drop".to_string()),
                vec!["enemy_type_drop".to_string()],
                t.iter().map(|x| format!("enemy type 0x{x:02X} +0x18")).collect(),
            ),
            _ => (None, vec![], vec![]),
        };
        let mut src = format!(
            "// Inline script 0x{a:08X}. Purpose: {}. Referenced by: {}.\n",
            purpose.as_deref().unwrap_or("?"),
            wh.join("; ")
        );
        src.push_str(&rhai_out::program(p, "", &names));
        out.put(path.clone(), src.into_bytes());
        index_inline.push(json!({"id": slug, "file": path, "purpose": purpose, "referenced_by": refs, "where": wh,
                                 "entry": rhai_out::entry_kind(p)}));
        inline_code.insert(slug, unit_json(&set, *id));
    }
    out.put_json(
        "scripts/index.json",
        &json!({
            "format": "Rhai, run-to-completion handlers + action queue; see engine/docs/scripting.md",
            "ids": "sNNN = event table 0x0879B704 index; inline_0xADDR = raw bytecode at ADDR (same ids as maps/<id>/objects.json)",
            "events": index_events, "inline": index_inline,
        }),
    );
    out.put_json(
        "scripts/bytecode.json",
        &json!({
            "about": "original bytecode for verification (wf_script::vm is the oracle); code = hex bytes from offset 0",
            "events": graphs, "inline": inline_code,
        }),
    );
    Ok(())
}
