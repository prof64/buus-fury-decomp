//! Player moves: melee, energy skills (0x0871DAF8), EP cost table (0x0871DA98), attack types (0x086B8088),
//! beam level defs, Big Bang charge rates.

use crate::characters::{CHAR_FORMS, SKILLS};
use crate::util::*;
use serde_json::{json, Map, Value};

pub const SKILL_COSTS: u32 = 0x0871_DA98;
pub const ATTACK_TYPES: u32 = 0x086B_8088;
pub const MELEE_RECT: u32 = 0x083C_344C;
pub const KI_DIR: u32 = 0x0871_9C40;
pub const BEAM_DEFS: u32 = 0x0871_DA68;
pub const BEAM_DRAIN: u32 = 0x0871_E148;
pub const BIGBANG_RATE: u32 = 0x0871_DDBC;
const FACING: [&str; 4] = ["down", "up", "left", "right"];

const A_MOVES: &str = include_str!("../data/moves.json");
const A_ATTACKS: &str = include_str!("../data/attacks.json");

/// Build an object with keys in `keys` order, taking each value from the first map that has it.
pub fn ordered(keys: &Value, sources: &[&Map<String, Value>]) -> Value {
    let mut m = Map::new();
    for k in keys.as_array().unwrap() {
        let k = k.as_str().unwrap();
        if let Some(v) = sources.iter().find_map(|s| s.get(k)) {
            m.insert(k.into(), v.clone());
        }
    }
    Value::Object(m)
}

pub fn skill(c: &Ctx, i: u32) -> Value {
    let a = SKILLS + 24 * i;
    let mx = c.u32(a);
    json!({
        "id": i, "address": ph(a as i64), "name": c.s(c.u16(a + 12) as i64), "description": c.s(c.u16(a + 14) as i64),
        "max_level": if mx != 0xff { json!(mx) } else { Value::Null },
        "can_use_fn": ph(c.u32(a + 16) as i64), "activate_fn": ph(c.u32(a + 20) as i64),
        "icon": ph(c.u32(a + 4) as i64), "icon2": ph(c.u32(a + 8) as i64),
    })
}

pub fn attack_type(c: &Ctx, i: u32) -> Value {
    let a = ATTACK_TYPES + 8 * i;
    let k = c.u8(a);
    json!({"sub": i, "address": ph(a as i64), "kind": k, "kind_name": if k == 0 { "melee" } else { "energy" },
           "id_byte": c.u8(a + 1), "power": c.u32(a + 4)})
}

fn rect(c: &Ctx, base: u32, d: u32) -> Vec<i32> {
    (0..4).map(|k| c.s32(base + 16 * d + 4 * k)).collect()
}

fn cost_vals(c: &Ctx, kind: &str, off: u32) -> Vec<u32> {
    if kind == "bytes" {
        (0..3).map(|i| c.u8(SKILL_COSTS + off + i) as u32).collect()
    } else {
        (0..3).map(|i| c.u32(SKILL_COSTS + off + 4 * i)).collect()
    }
}

/// moves.json
pub fn moves(c: &Ctx) -> Value {
    let a = annot(A_MOVES);
    let mut skills = Vec::new();
    for sid in 1..0x18u32 {
        let mut s = obj(&skill(c, sid));
        let mut rule = obj(a["cost_rules"].get(sid.to_string()).unwrap_or(&json!({})));
        if let Some(lt) = rule.remove("levels_table") {
            let kind = lt[0].as_str().unwrap();
            let off = lt[1].as_u64().unwrap() as u32;
            rule.insert("ep_cost_by_level".into(), json!(cost_vals(c, kind, off)));
            rule.insert("cost_table_address".into(), json!(ph((SKILL_COSTS + off) as i64)));
        }
        rule.insert("ep_cost_scaled_by".into(), a["ep_cost_scaled_by"].clone());
        s.insert("ep".into(), Value::Object(rule));
        s.insert("measured".into(), a["measured"][sid.to_string()].clone());
        skills.push(Value::Object(s));
    }
    let mut melee = obj(&a["melee"]);
    let rects: Map<String, Value> = (0..4).map(|d| (FACING[d as usize].to_string(), json!(rect(c, MELEE_RECT, d)))).collect();
    melee.insert("rects_relative_to_player_pos".into(), Value::Object(rects));
    melee.insert("damage_type".into(), attack_type(c, 2));
    let melee = ordered(&a["melee_keys"], &[&melee]);
    json!({
        "meta": a["meta"], "melee": melee, "skills": skills,
        "attack_types": (0..18).map(|i| attack_type(c, i)).collect::<Vec<_>>(),
        "skill_levels": a["skill_levels"],
    })
}

/// attacks_detail.json
pub fn attacks(c: &Ctx) -> Value {
    let a = annot(A_ATTACKS);
    // attack types until the first entry that is not {kind 0/1, 0 pad}
    let mut types = Vec::new();
    for i in 0..0x14u32 {
        let ad = ATTACK_TYPES + 8 * i;
        let (k, sub, pad, pw) = (c.u8(ad), c.u8(ad + 1), c.u16(ad + 2), c.u32(ad + 4));
        if k > 1 || pad != 0 {
            break;
        }
        types.push(json!({"addr": ph(ad as i64), "kind": (if k == 0 { "melee" } else { "energy" }), "sub": sub, "power": pw}));
    }
    let mut melee = obj(&a["melee"]);
    let rects: Vec<Value> = (0..4u32)
        .map(|i| {
            let r = rect(c, MELEE_RECT, i);
            json!({"facing": FACING[i as usize], "x0": r[0], "y0": r[1], "x1": r[2], "y1": r[3]})
        })
        .collect();
    melee.insert("rect_table".into(), json!(ph(MELEE_RECT as i64)));
    melee.insert("rects_rel_player".into(), json!(rects));
    // measured per-form melee data; form names from the form table
    if let Some(Value::Array(forms)) = melee.get_mut("forms") {
        for f in forms.iter_mut() {
            let fi = f["form"].as_u64().unwrap() as u32;
            let name = c.s(c.u16(CHAR_FORMS + 24 * fi + 2) as i64);
            let mut m = Map::new();
            for (k, v) in obj(f) {
                m.insert(k.clone(), v);
                if k == "form" {
                    m.insert("name".into(), name.clone());
                }
            }
            *f = Value::Object(m);
        }
    }
    let melee = ordered(&a["melee_keys"], &[&melee]);
    let sub_names: Map<String, Value> = obj(&a["attack_sub_names"])
        .into_iter()
        .map(|(k, v)| {
            let t = match v.get("skill") {
                Some(sid) => c.s(c.u16(SKILLS + 24 * sid.as_u64().unwrap() as u32 + 12) as i64),
                None => v["text"].clone(),
            };
            (k, t)
        })
        .collect();
    let mut ki = obj(&a["ki_blast"]);
    ki.insert("dir_table".into(), json!(ph(KI_DIR as i64)));
    let vel: Vec<Value> = (0..4u32)
        .map(|i| {
            json!({"facing": FACING[i as usize], "vx": c.s32(KI_DIR + 8 * i) as f64 / 256.0,
                   "vy": c.s32(KI_DIR + 8 * i + 4) as f64 / 256.0})
        })
        .collect();
    ki.insert("velocity_px_per_frame".into(), json!(vel));
    let ki = ordered(&a["ki_blast_keys"], &[&ki]);
    let mut beams = obj(&a["beams"]);
    let defs: Vec<Value> = (0..3u32)
        .map(|lv| {
            let ad = BEAM_DEFS + 16 * lv;
            json!({"addr": ph(ad as i64), "level": lv, "w": c.u8(ad), "h": c.u8(ad + 1), "mult": c.u16(ad + 2),
                   "unk4": c.u16(ad + 4), "attack_sub": c.u16(ad + 6), "gfx": ph(c.u32(ad + 8) as i64),
                   "anim": ph(c.u32(ad + 12) as i64),
                   "drain_ep_per_frame": pyround(c.u32(BEAM_DRAIN + 4 * lv) as f64 / 256.0, 3)})
        })
        .collect();
    beams.insert("level_defs".into(), json!(defs));
    beams.insert("drain_table".into(), json!(ph(BEAM_DRAIN as i64)));
    let beams = ordered(&a["beams_keys"], &[&beams]);
    let mut bb = obj(&a["big_bang"]);
    bb.insert(
        "charge_rate".into(),
        json!((0..3).map(|i| pyround(c.u32(BIGBANG_RATE + 4 * i) as f64 / 256.0, 3)).collect::<Vec<_>>()),
    );
    let bb = ordered(&a["big_bang_keys"], &[&bb]);

    let mut skills = Vec::new();
    for sk in 1..0x18u32 {
        let ad = SKILLS + 24 * sk;
        let name = c.s(c.u16(ad + 12) as i64);
        if name.as_str().map(|s| s.is_empty()).unwrap_or(true) {
            continue;
        }
        let key = sk.to_string();
        let cost = match a["cost"].get(&key) {
            Some(cv) => {
                let mut m = Map::new();
                for (k, v) in obj(cv) {
                    if k == "levels" {
                        let kind = v[0].as_str().unwrap();
                        let off = v[1].as_u64().unwrap() as u32;
                        let kind = if kind == "bytes" { "bytes" } else { "u32" };
                        m.insert("levels".into(), json!(cost_vals(c, kind, off)));
                        m.insert("addr".into(), json!(ph((SKILL_COSTS + off) as i64)));
                    } else {
                        m.insert(k, v);
                    }
                }
                Value::Object(m)
            }
            None => Value::Null,
        };
        let mut d = Map::new();
        d.insert("id".into(), json!(sk));
        d.insert("name".into(), name);
        d.insert("table_addr".into(), json!(ph(ad as i64)));
        d.insert("raw".into(), json!(hexbytes(c.bytes(ad, 24))));
        d.insert("max_level".into(), json!(c.u32(ad)));
        d.insert("kind".into(), a["kind"].get(&key).cloned().unwrap_or(Value::Null));
        d.insert("cost".into(), cost);
        if let Some(vt) = a["state_vt"].get(&key) {
            let vt = u32::from_str_radix(vt.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
            let fns: Vec<String> =
                (0..5).map(|i| ph((vt.wrapping_add(c.s32(vt + 4 * i) as u32)) as i64)).collect();
            d.insert(
                "state_class".into(),
                json!({"vtable": ph(vt as i64), "fns": fns, "slots": "enter, update, input, destroy, misc", "status": "verified"}),
            );
        }
        d.insert("measured".into(), a["skill_measured"].get(&key).cloned().unwrap_or(json!([])));
        skills.push(Value::Object(d));
    }
    let mut all = Map::new();
    all.insert("melee".into(), melee);
    for k in ["block", "energy_block", "dash", "hit_struct"] {
        all.insert(k.into(), a[k].clone());
    }
    all.insert("attack_types".into(), json!(types));
    all.insert("attack_sub_names".into(), Value::Object(sub_names));
    all.insert("ki_blast".into(), ki);
    all.insert("beams".into(), beams);
    all.insert("big_bang".into(), bb);
    all.insert("skills".into(), json!(skills));
    ordered(&a["top_keys"], &[&all])
}
