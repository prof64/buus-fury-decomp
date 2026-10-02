//! Enemy AI behaviour trees: state table 0x0870F514 ({root def*, aux*} per state id), enemy type +8 =
//! initial state. A node definition is a ROM word array `[create_fn, f1, f2, params from +0xC.., children]`;
//! node names / parameter layouts / semantics come from the hand-decoded registry (annotation `data/ai.json`).

use crate::enemies::{ENEMY_TYPES, N_ENEMY};
use crate::util::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};

pub const STATE_TABLE: u32 = 0x0870_F514;
const GOTO_STATE: u32 = 0x0801_890B;
const SUB_STATE: u32 = 0x0801_9285;

const A_AI: &str = include_str!("../data/ai.json");

pub struct Registry {
    pub a: Value,
    pub map: HashMap<u32, Value>,
}

pub fn parse_hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

impl Registry {
    pub fn load() -> Registry {
        let a = annot(A_AI);
        let map = obj(&a["registry"]).into_iter().map(|(k, v)| (parse_hex(&k), v)).collect();
        Registry { a, map }
    }
}

pub fn is_def(c: &Ctx, d: u32) -> bool {
    in_rom_range(d) && (0x0800_0000..0x0810_0000).contains(&c.u32(d)) && c.u32(d) & 1 == 1
}

fn child_offsets(c: &Ctx, reg: &Registry, fnc: u32, d: u32) -> Vec<u32> {
    match reg.map.get(&fnc).and_then(|r| r.get("children")) {
        Some(Value::String(_)) => {
            let n = c.u32(d + 0xC).min(40);
            (0..n).map(|k| 0x10 + 4 * k).collect()
        }
        Some(Value::Array(v)) => v.iter().map(|x| x.as_u64().unwrap() as u32).collect(),
        _ => Vec::new(),
    }
}

fn convert(unit: &str, v: u32) -> Map<String, Value> {
    let x = v as f64;
    let j = match unit {
        "dist2" => json!({"px": pyround(x.sqrt(), 2)}),
        "frames" => json!({"frames": v, "seconds": pyround(x / 60.0, 3)}),
        "prob16" => json!({"percent_per_frame": pyround(x * 100.0 / 65536.0, 3),
                           "mean_frames": if v != 0 { json!(pyround(65536.0 / x, 1)) } else { Value::Null }}),
        "speed256" => json!({"px_per_frame": pyround(x / 256.0, 4)}),
        "frac16" => json!({"percent": pyround(x * 100.0 / 65536.0, 2)}),
        _ => json!({"value": v}),
    };
    obj(&j)
}

pub fn decode(c: &Ctx, reg: &Registry, d: u32, depth: u32) -> Value {
    if !is_def(c, d) || depth > 40 {
        return json!({"addr": ph(d as i64), "invalid": true});
    }
    let fnc = c.u32(d);
    let r = reg.map.get(&fnc);
    let mut node = Map::new();
    node.insert("addr".into(), json!(ph(d as i64)));
    node.insert("fn".into(), json!(ph(fnc as i64)));
    node.insert("name".into(), r.map(|r| r["name"].clone()).unwrap_or_else(|| json!(format!("node_{fnc:08x}"))));
    node.insert("kind".into(), r.map(|r| r["kind"].clone()).unwrap_or_else(|| json!("?")));
    if let Some(r) = r {
        let mut ps = Map::new();
        for p in r.get("params").and_then(|p| p.as_array()).cloned().unwrap_or_default() {
            let off = p[0].as_u64().unwrap() as u32;
            let name = p[1].as_str().unwrap();
            let unit = p[2].as_str().unwrap();
            let v = c.u32(d + off);
            let mut m = Map::new();
            m.insert("raw".into(), json!(v));
            m.insert("unit".into(), json!(unit));
            m.extend(convert(unit, v));
            ps.insert(name.into(), Value::Object(m));
        }
        if !ps.is_empty() {
            node.insert("params".into(), Value::Object(ps));
        }
    } else {
        // not hand-decoded: keep the first words (create fn, f1, f2) raw
        node.insert("raw_words".into(), json!((0..3).map(|k| ph(c.u32(d + 4 * k) as i64)).collect::<Vec<_>>()));
    }
    let kids: Vec<u32> =
        child_offsets(c, reg, fnc, d).into_iter().map(|o| c.u32(d + o)).filter(|&k| is_def(c, k)).collect();
    if !kids.is_empty() {
        node.insert("children".into(), json!(kids.iter().map(|&k| decode(c, reg, k, depth + 1)).collect::<Vec<_>>()));
    }
    Value::Object(node)
}

fn describe(n: &Value) -> String {
    let mut s = n["name"].as_str().unwrap().to_string();
    if let Some(ps) = n.get("params").and_then(|p| p.as_object()) {
        let bits: Vec<String> = ps
            .iter()
            .map(|(k, p)| match p["unit"].as_str().unwrap() {
                "dist2" => format!("{k}={}px", pyfloat(p["px"].as_f64().unwrap())),
                "frames" => format!("{}f", p["frames"]),
                "prob16" => format!("{}%/f", pyfloat(p["percent_per_frame"].as_f64().unwrap())),
                "speed256" => format!("{}px/f", pyfloat(p["px_per_frame"].as_f64().unwrap())),
                "frac16" => format!("{}%", pyfloat(p["percent"].as_f64().unwrap())),
                "state" => format!("state {:#x}", p["raw"].as_u64().unwrap()),
                _ => format!("{k}={}", p["raw"]),
            })
            .collect();
        s += &format!("({})", bits.join(", "));
    } else if let Some(w) = n.get("raw_words").and_then(|w| w.as_array()) {
        let ws: Vec<&str> = w[1..].iter().map(|x| x.as_str().unwrap()).collect();
        s += &format!(" [{}]", ws.join(" "));
    }
    s
}

pub fn pretty(n: &Value, indent: usize, out: &mut Vec<String>) {
    out.push(format!("{}{}  @{}", "  ".repeat(indent), describe(n), n["addr"].as_str().unwrap()));
    for ch in n.get("children").and_then(|c| c.as_array()).into_iter().flatten() {
        pretty(ch, indent + 1, out);
    }
}

pub fn state_root(c: &Ctx, s: u32) -> u32 {
    c.u32(STATE_TABLE + 8 * s)
}

pub fn initial_state(c: &Ctx, t: u32) -> u32 {
    c.u32(ENEMY_TYPES + 28 * t + 8)
}

/// States reachable from `start` through GotoState / SubState (BFS order).
pub fn walk_states(c: &Ctx, reg: &Registry, start: u32) -> Vec<u32> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut todo = std::collections::VecDeque::from([start]);
    while let Some(s) = todo.pop_front() {
        if seen.contains(&s) || s > 0x400 {
            continue;
        }
        seen.insert(s);
        order.push(s);
        let mut stack = vec![state_root(c, s)];
        let mut vis = BTreeSet::new();
        while let Some(d) = stack.pop() {
            if vis.contains(&d) || !is_def(c, d) {
                continue;
            }
            vis.insert(d);
            let fnc = c.u32(d);
            if fnc == GOTO_STATE || fnc == SUB_STATE {
                todo.push_back(c.u32(d + 0xC));
                continue;
            }
            for o in child_offsets(c, reg, fnc, d) {
                stack.push(c.u32(d + o));
            }
        }
    }
    order
}

pub fn iter_nodes<'v>(n: &'v Value, out: &mut Vec<&'v Value>) {
    out.push(n);
    for ch in n.get("children").and_then(|c| c.as_array()).into_iter().flatten() {
        iter_nodes(ch, out);
    }
}

/// Sorted, de-duplicated list of numbers.
fn sorted_nums(v: &[f64]) -> Vec<Value> {
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup();
    v.into_iter()
        .map(|x| if x.fract() == 0.0 && x.abs() < 1e15 && !x.is_sign_negative() { json!(x) } else { json!(x) })
        .collect()
}

#[derive(Default)]
struct Multi(Vec<(String, Vec<f64>)>);
impl Multi {
    fn add(&mut self, k: &str, v: f64) {
        if let Some(e) = self.0.iter_mut().find(|e| e.0 == k) {
            e.1.push(v);
        } else {
            self.0.push((k.into(), vec![v]));
        }
    }
    fn to_json(&self) -> Value {
        Value::Object(self.0.iter().map(|(k, v)| (k.clone(), json!(sorted_nums(v)))).collect())
    }
}

fn summarize(reg: &Registry, sts: &[u32], trees: &HashMap<u32, Value>) -> Value {
    let attack_nodes: Vec<String> =
        reg.a["attack_nodes"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().into()).collect();
    let move_nodes: Vec<String> = reg.a["move_nodes"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().into()).collect();
    let (mut aggro, mut within, mut farther, mut waits, mut cds, mut tis, mut hps, mut counters) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut speeds = Multi::default();
    let mut chances = Multi::default();
    let mut attacks: Vec<(String, String, Value)> = Vec::new();
    let mut special: BTreeSet<String> = BTreeSet::new();
    let mut blocks = false;
    for sid in sts {
        let mut nodes = Vec::new();
        iter_nodes(&trees[sid], &mut nodes);
        for (i, n) in nodes.iter().enumerate() {
            let nm = n["name"].as_str().unwrap();
            let p = n.get("params").cloned().unwrap_or(json!({}));
            let pf = |k: &str, f: &str| p[k][f].as_f64().unwrap();
            if nm == "IfPlayerWithin2" && i <= 2 {
                aggro.push(pf("r2", "px"));
            } else if nm == "IfPlayerWithin" || nm == "IfPlayerWithin2" {
                within.push(pf("r2", "px"));
            } else if nm == "IfPlayerFarther" {
                farther.push(pf("r2", "px"));
            } else if move_nodes.iter().any(|m| m == nm) && p.get("speed").is_some() {
                speeds.add(nm, pf("speed", "px_per_frame"));
            } else if nm == "Wait" {
                waits.push(pf("frames", "frames"));
            } else if nm == "IfCooldownOver" {
                cds.push(pf("frames", "frames"));
            } else if nm == "IfTimeInState" {
                tis.push(pf("frames", "frames"));
            } else if nm == "IfHpBelow" {
                hps.push(pf("frac", "percent"));
            } else if nm == "Chance" {
                let kid = n.get("children").and_then(|c| c.get(0)).cloned().unwrap_or(json!({}));
                let mut what = kid.get("name").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                if what == "GotoState" {
                    what += &format!(" {:#x}", kid["params"]["state"]["raw"].as_u64().unwrap());
                }
                chances.add(&what, pf("p", "percent_per_frame"));
            } else if nm == "Block" {
                blocks = true;
            } else if nm == "IfCounterEq" {
                counters.push(p["n"]["raw"].as_f64().unwrap());
            }
            if attack_nodes.iter().any(|a| a == nm) {
                let st = ph(*sid as i64);
                if !attacks.iter().any(|(a, s, _)| a == nm && *s == st) {
                    attacks.push((nm.into(), st, n["addr"].clone()));
                }
            } else if nm.starts_with("node_") {
                special.insert(n["fn"].as_str().unwrap().into());
            }
        }
    }
    let ints = |v: &[f64]| -> Vec<Value> {
        let mut v: Vec<i64> = v.iter().map(|&x| x as i64).collect();
        v.sort();
        v.dedup();
        v.into_iter().map(|x| json!(x)).collect()
    };
    json!({
        "aggro_radius_px": sorted_nums(&aggro), "within_px": sorted_nums(&within), "farther_px": sorted_nums(&farther),
        "speeds": speeds.to_json(),
        "attacks": attacks.iter().map(|(a, s, ad)| json!({"node": a, "state": s, "addr": ad})).collect::<Vec<_>>(),
        "waits_frames": ints(&waits), "cooldowns_frames": ints(&cds), "time_in_state_frames": ints(&tis),
        "hp_thresholds_percent": sorted_nums(&hps), "chances": chances.to_json(),
        "special_nodes": special.into_iter().collect::<Vec<_>>(), "blocks": blocks, "counters": ints(&counters),
    })
}

pub struct AiDoc {
    pub doc: Value,
    /// state id -> (text lines, tree)
    pub trees: HashMap<u32, (Vec<String>, Value)>,
    /// enemy type -> (initial state, reachable states)
    pub enemy_states: Vec<(u32, Vec<u32>)>,
}

/// enemy_ai.json. `names[t]` = enemy name (enemies.json).
pub fn enemy_ai(c: &Ctx, names: &[Value]) -> AiDoc {
    let reg = Registry::load();
    let mut trees: HashMap<u32, Value> = HashMap::new();
    let mut texts: HashMap<u32, Vec<String>> = HashMap::new();
    let mut users: HashMap<u32, BTreeSet<u32>> = HashMap::new();
    let mut counts: Vec<(u32, u64)> = Vec::new();
    let mut enemies = Vec::new();
    let mut enemy_states = Vec::new();
    for t in 0..N_ENEMY {
        let s0 = initial_state(c, t);
        let sts = walk_states(c, &reg, s0);
        for &sid in &sts {
            if !trees.contains_key(&sid) {
                let tr = decode(c, &reg, state_root(c, sid), 0);
                let mut tx = Vec::new();
                pretty(&tr, 0, &mut tx);
                trees.insert(sid, tr);
                texts.insert(sid, tx);
            }
        }
        for &sid in &sts {
            let mut nodes = Vec::new();
            iter_nodes(&trees[&sid], &mut nodes);
            for n in nodes {
                let f = parse_hex(n["fn"].as_str().unwrap());
                users.entry(f).or_default().insert(t);
                match counts.iter_mut().find(|e| e.0 == f) {
                    Some(e) => e.1 += 1,
                    None => counts.push((f, 1)),
                }
            }
        }
        enemies.push(json!({"type": t, "name": names.get(t as usize).cloned().unwrap_or(Value::Null),
                            "initial_state": ph(s0 as i64),
                            "states": sts.iter().map(|&s| ph(s as i64)).collect::<Vec<_>>(),
                            "summary": summarize(&reg, &sts, &trees)}));
        enemy_states.push((s0, sts));
    }
    counts.sort_by_key(|e| std::cmp::Reverse(e.1));
    let unit_desc = &reg.a["unit_desc"];
    let mut node_types = Vec::new();
    for (f, n) in &counts {
        let mut e = Map::new();
        e.insert("create_fn".into(), json!(ph(*f as i64)));
        e.insert("uses".into(), json!(n));
        e.insert("enemy_types".into(), json!(users[f].iter().collect::<Vec<_>>()));
        if let Some(r) = reg.map.get(f) {
            e.insert("name".into(), r["name"].clone());
            e.insert("kind".into(), r["kind"].clone());
            e.insert("status".into(), r["status"].clone());
            e.insert("semantics".into(), r["sem"].clone());
            let ps: Vec<Value> = r
                .get("params")
                .and_then(|p| p.as_array())
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    let u = p[2].as_str().unwrap();
                    json!({"offset": ph(p[0].as_i64().unwrap()), "name": p[1], "unit": u,
                           "unit_desc": unit_desc.get(u).cloned().unwrap_or(Value::Null)})
                })
                .collect();
            e.insert("params".into(), json!(ps));
            let ch = match r.get("children") {
                Some(Value::String(_)) => json!("count @+0xC, defs from +0x10"),
                Some(Value::Array(v)) => json!(v.iter().map(|o| ph(o.as_i64().unwrap())).collect::<Vec<_>>()),
                _ => json!([]),
            };
            e.insert("children".into(), ch);
        } else {
            e.insert("name".into(), json!(format!("node_{f:08x}")));
            e.insert("status".into(), json!("AUTO"));
        }
        node_types.push(Value::Object(e));
    }
    let mut ids: Vec<u32> = trees.keys().copied().collect();
    ids.sort();
    let states: Map<String, Value> =
        ids.iter().map(|&k| (ph(k as i64), json!({"text": texts[&k], "tree": trees[&k]}))).collect();
    let doc = json!({"meta": reg.a["meta"], "node_types": node_types, "states": states, "enemies": enemies});
    let trees = ids.iter().map(|&k| (k, (texts[&k].clone(), trees[&k].clone()))).collect();
    AiDoc { doc, trees, enemy_states }
}
