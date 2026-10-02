//! Bosses / special enemies: hand-decoded behaviour facts (annotation `data/bosses.json`, every claim tagged
//! VERIFIED / DISASM / INFERRED) joined with the ROM stats and the decoded AI state texts.

use crate::ai::{AiDoc, Registry};
use crate::util::*;
use serde_json::{json, Map, Value};

const A_BOSSES: &str = include_str!("../data/bosses.json");

/// bosses.json. `enemies` = the enemies.json document, `ai` = the decoded AI.
pub fn bosses(enemies: &Value, ai: &AiDoc) -> Value {
    let a = annot(A_BOSSES);
    let reg = Registry::load();
    let en = enemies["enemies"].as_array().unwrap();
    let ai_en = ai.doc["enemies"].as_array().unwrap();
    let states = &ai.doc["states"];
    let (mut bosses, mut projectiles, mut specials) = (Map::new(), Map::new(), Map::new());
    let mut base = Value::Null;
    for g in a["group_order"].as_array().unwrap() {
        let g = g.as_str().unwrap();
        let grp = &a["groups"][g];
        for (k, v) in obj(&grp["projectiles"]) {
            let mut m = obj(&v);
            m.insert("group".into(), json!(g));
            projectiles.insert(k, Value::Object(m));
        }
        for (k, v) in obj(&grp["specials"]) {
            let v = if let Value::Object(mut m) = v {
                m.insert("group".into(), json!(g));
                Value::Object(m)
            } else {
                v
            };
            specials.insert(k, v);
        }
        if !grp["projectile_base"].is_null() {
            base = grp["projectile_base"].clone();
        }
        for (th, b) in obj(&grp["bosses"]) {
            let t = crate::ai::parse_hex(&th) as usize;
            let e = &en[t];
            let ae = &ai_en[t];
            let stats: Map<String, Value> = a["stat_keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| (k.as_str().unwrap().to_string(), e[k.as_str().unwrap()].clone()))
                .collect();
            let ai_states: Map<String, Value> = ae["states"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s.as_str().unwrap().to_string(), states[s.as_str().unwrap()]["text"].clone()))
                .collect();
            let name = match b.get("name") {
                Some(n) if !n.is_null() && n != "" => n.clone(),
                _ => e["name"].clone(),
            };
            let mut m = Map::new();
            m.insert("type".into(), json!(t));
            m.insert("type_hex".into(), json!(ph(t as i64)));
            m.insert("group".into(), json!(g));
            m.insert("name".into(), name);
            m.insert("stats".into(), Value::Object(stats));
            m.insert("stats_units".into(), a["stats_units"].clone());
            m.insert("initial_state".into(), ae["initial_state"].clone());
            m.insert("ai_states".into(), Value::Object(ai_states));
            for (k, v) in obj(&b) {
                if k != "name" {
                    m.insert(k, v);
                }
            }
            bosses.insert(ph(t as i64), Value::Object(m));
        }
    }
    let mut fns: Vec<&u32> = reg.map.keys().collect();
    fns.sort();
    let nodes: Map<String, Value> = fns
        .into_iter()
        .map(|f| {
            let r = &reg.map[f];
            let mut m = Map::new();
            for k in ["name", "kind", "status"] {
                m.insert(k.into(), r[k].clone());
            }
            m.insert("sem".into(), r["sem"].clone());
            let ps: Vec<Value> = r
                .get("params")
                .and_then(|p| p.as_array())
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|p| json!({"offset": ph(p[0].as_i64().unwrap()), "name": p[1], "unit": p[2]}))
                .collect();
            m.insert("params".into(), json!(ps));
            for k in ["timing", "projectile"] {
                if let Some(v) = r.get(k) {
                    m.insert(k.into(), v.clone());
                }
            }
            (ph(*f as i64), Value::Object(m))
        })
        .collect();
    json!({"meta": a["meta"], "projectile_base": base, "projectiles": projectiles,
           "regular_enemy_specials": specials, "bosses": bosses, "node_registry": nodes})
}
