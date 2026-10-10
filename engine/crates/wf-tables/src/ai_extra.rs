//! On-hit ("aux") trees of every AI state (state table word +4), the generic reaction states 0x00..0x11 and the
//! enemy-type death nodes (+0x14) — port of `tools/battle/gen_buu_ai_extra.py`
//! (reference `data/battle/buu_ai_extra.json`).

use crate::ai::{is_def, parse_hex, Registry, STATE_TABLE};
use crate::enemies::{ENEMY_TYPES, N_ENEMY};
use crate::util::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};

const A_EXTRA: &str = include_str!("../data/ai_extra.json");

struct Tabs {
    reg: Registry,
    extra: HashMap<u32, Value>,
    aux_only: HashMap<u32, Value>,
}

fn is_aux_def(c: &Ctx, t: &Tabs, d: u32) -> bool {
    in_rom_range(d) && (is_def(c, d) || (c.u32(d) == 0 && t.aux_only.contains_key(&c.u32(d + 4))))
}

fn convert(unit: &str, v: u32) -> Map<String, Value> {
    // same unit conversions as the main decoder
    let x = v as f64;
    obj(&match unit {
        "dist2" => json!({"px": pyround(x.sqrt(), 2)}),
        "frames" => json!({"frames": v, "seconds": pyround(x / 60.0, 3)}),
        "prob16" => json!({"percent_per_frame": pyround(x * 100.0 / 65536.0, 3),
                           "mean_frames": if v != 0 { json!(pyround(65536.0 / x, 1)) } else { Value::Null }}),
        "speed256" => json!({"px_per_frame": pyround(x / 256.0, 4)}),
        "frac16" => json!({"percent": pyround(x * 100.0 / 65536.0, 2)}),
        _ => json!({"value": v}),
    })
}

fn decode(c: &Ctx, t: &Tabs, d: u32, depth: u32) -> Value {
    if !is_aux_def(c, t, d) || depth > 40 {
        return json!({"addr": ph(d as i64), "invalid": true});
    }
    let fnc = c.u32(d);
    let on_hit = c.u32(d + 4);
    let (r, aux_only) = if fnc == 0 {
        (t.aux_only.get(&on_hit), true)
    } else {
        (t.reg.map.get(&fnc).or_else(|| t.extra.get(&fnc)), false)
    };
    let mut node = Map::new();
    node.insert("addr".into(), json!(ph(d as i64)));
    node.insert("fn".into(), json!(ph(fnc as i64)));
    node.insert("onHit".into(), json!(ph(on_hit as i64)));
    let Some(r) = r else {
        node.insert("name".into(), json!(format!("node_{fnc:08x}")));
        node.insert("raw_words".into(), json!((0..6).map(|k| ph(c.u32(d + 4 * k) as i64)).collect::<Vec<_>>()));
        return Value::Object(node);
    };
    node.insert("name".into(), r["name"].clone());
    let mut ps = Map::new();
    for p in r.get("params").and_then(|p| p.as_array()).cloned().unwrap_or_default() {
        let unit = p[2].as_str().unwrap();
        let v = c.u32(d + p[0].as_u64().unwrap() as u32);
        let mut m = Map::new();
        m.insert("raw".into(), json!(v));
        m.insert("unit".into(), json!(unit));
        m.extend(convert(unit, v));
        ps.insert(p[1].as_str().unwrap().into(), Value::Object(m));
    }
    if !ps.is_empty() {
        node.insert("params".into(), Value::Object(ps));
    }
    let offs: Vec<u32> = if aux_only || t.extra.contains_key(&fnc) {
        r.get("children").and_then(|x| x.as_array()).map(|v| v.iter().map(|o| o.as_u64().unwrap() as u32).collect()).unwrap_or_default()
    } else {
        match r.get("children") {
            Some(Value::String(_)) => (0..c.u32(d + 0xC).min(40)).map(|k| 0x10 + 4 * k).collect(),
            Some(Value::Array(v)) => v.iter().map(|o| o.as_u64().unwrap() as u32).collect(),
            _ => vec![],
        }
    };
    let kids: Vec<Value> = offs
        .into_iter()
        .map(|o| c.u32(d + o))
        .filter(|&k| is_aux_def(c, t, k))
        .map(|k| decode(c, t, k, depth + 1))
        .collect();
    if !kids.is_empty() {
        node.insert("children".into(), json!(kids));
    }
    Value::Object(node)
}

fn scan(n: &Value, todo: &mut Vec<u32>) {
    if let Some(p) = n.get("params").and_then(|p| p.get("state")) {
        if matches!(n["name"].as_str(), Some("PushState" | "GotoState" | "SubState")) {
            todo.push(p["raw"].as_u64().unwrap() as u32);
        }
    }
    for ch in n.get("children").and_then(|x| x.as_array()).into_iter().flatten() {
        scan(ch, todo);
    }
}

/// buu_ai_extra.json; `main_states` = the state ids decoded in enemy_ai.json.
pub fn ai_extra(c: &Ctx, main_states: &BTreeSet<u32>) -> Value {
    let a = annot(A_EXTRA);
    let tabs = Tabs {
        reg: Registry::load(),
        extra: obj(&a["extra_nodes"]).into_iter().map(|(k, v)| (parse_hex(&k), v)).collect(),
        aux_only: obj(&a["aux_only"]).into_iter().map(|(k, v)| (parse_hex(&k), v)).collect(),
    };
    let mut states: BTreeSet<u32> = main_states.clone();
    states.extend(0..0x12);
    let mut todo: Vec<u32> = states.into_iter().collect();
    let mut seen = BTreeSet::new();
    let (mut aux, mut reactions, mut death) = (Map::new(), Map::new(), Map::new());
    while let Some(s) = todo.pop() {
        if seen.contains(&s) || s > 0x400 {
            continue;
        }
        seen.insert(s);
        let t = decode(c, &tabs, c.u32(STATE_TABLE + 8 * s + 4), 0);
        scan(&t, &mut todo);
        aux.insert(ph(s as i64), t);
        if s < 0x12 || !main_states.contains(&s) {
            let r = decode(c, &tabs, c.u32(STATE_TABLE + 8 * s), 0);
            scan(&r, &mut todo);
            reactions.insert(ph(s as i64), r);
        }
    }
    for t in 0..N_ENEMY {
        let dd = c.u32(ENEMY_TYPES + 28 * t + 0x14);
        let w: Vec<u32> = (0..4).map(|k| c.u32(dd + 4 * k)).collect();
        let f = w[0];
        let name = tabs
            .reg
            .map
            .get(&f)
            .or_else(|| tabs.extra.get(&f))
            .map(|r| r["name"].clone())
            .unwrap_or_else(|| json!(if f != 0 { format!("node_{f:08x}") } else { "special".into() }));
        death.insert(
            ph(t as i64),
            json!({"addr": ph(dd as i64), "create": ph(f as i64), "name": name, "f2": ph(w[2] as i64), "param": w[3]}),
        );
    }
    json!({"source": a["source"], "onhit_protocol": a["onhit_protocol"], "onhit_fns": a["onhit_fns"],
           "aux_only": a["aux_only"], "extra_nodes": a["extra_nodes"], "aux": aux, "reactions": reactions, "death": death})
}
