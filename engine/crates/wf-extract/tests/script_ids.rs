//! Every script id referenced by the world content (map objects, triggers, NPC talk, on-defeat, area hooks)
//! resolves to a decompiled file under `scripts/`.
use std::collections::BTreeSet;

fn collect(v: &serde_json::Value, out: &mut BTreeSet<String>) {
    match v {
        serde_json::Value::String(s) => {
            let is_event = s.len() == 4 && s.starts_with('s') && s[1..].chars().all(|c| c.is_ascii_digit());
            let is_inline = s.len() == 17 && s.starts_with("inline_0x");
            if is_event || is_inline {
                out.insert(s.clone());
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        serde_json::Value::Object(m) => m.values().for_each(|x| collect(x, out)),
        _ => {}
    }
}

#[test]
fn world_script_references_resolve() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let opts = wf_extract::Options { only: vec!["world".into(), "scripts".into()], ..Default::default() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    let mut ids = BTreeSet::new();
    for (p, b) in &c.files {
        if p.ends_with(".json") && !p.starts_with("scripts/") {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(b) {
                collect(&v, &mut ids);
            }
        }
    }
    assert!(ids.len() > 500, "{}", ids.len());
    let missing: Vec<&String> = ids
        .iter()
        .filter(|id| {
            let p = if id.starts_with('s') { format!("scripts/events/{id}.rhai") } else { format!("scripts/inline/{id}.rhai") };
            !c.files.contains_key(&p)
        })
        .collect();
    assert!(missing.is_empty(), "{} unresolved: {:?}", missing.len(), &missing[..missing.len().min(20)]);
}
