//! Playable characters, forms, growth, XP table, formulas/RNG documentation.

use crate::util::*;
use serde_json::{json, Map, Value};

pub const STRTAB: u32 = 0x0806_BCE8;
pub const CHAR_FORMS: u32 = 0x086B_82EC;
pub const N_FORMS: u32 = 54;
pub const GROWTH: u32 = 0x086B_72E0;
pub const XP_TABLE: u32 = 0x086B_7084;
pub const INIT_RECORDS: u32 = 0x086F_DA4C;
pub const SKILLS: u32 = 0x0871_DAF8;

const A_CHARACTERS: &str = include_str!("../data/characters.json");
const A_LEVELS: &str = include_str!("../data/levels.json");
const A_FORMULAS: &str = include_str!("../data/formulas.json");
const A_RNG: &str = include_str!("../data/rng.json");

pub fn growth(c: &Ctx, ci: u32) -> Vec<u32> {
    (0..8).map(|k| c.u32(GROWTH + 32 * ci + 4 * k)).collect()
}

pub fn xp_for_level(c: &Ctx, level: u32) -> u32 {
    c.u32(XP_TABLE + 4 * (level - 50))
}

pub fn skill_name(c: &Ctx, sid: u32) -> Value {
    c.s(c.u16(SKILLS + 24 * sid + 12) as i64)
}

/// levels.json: XP table 0x086B7084.
pub fn levels(c: &Ctx) -> Value {
    let a = annot(A_LEVELS);
    let mut rows: Vec<Map<String, Value>> = Vec::new();
    for lv in 50..=200u32 {
        let xp = if lv > 50 { xp_for_level(c, lv) } else { 0 };
        let mut m = Map::new();
        m.insert("level".into(), json!(lv));
        m.insert("total_xp".into(), json!(xp));
        rows.push(m);
    }
    let n = rows.len();
    for i in 0..n {
        let next = if i + 1 < n {
            json!(rows[i + 1]["total_xp"].as_u64().unwrap() - rows[i]["total_xp"].as_u64().unwrap())
        } else {
            Value::Null
        };
        rows[i].insert("to_next".into(), next);
    }
    json!({"meta": a["meta"], "min_level": 50, "max_level": 200, "levels": rows})
}

pub fn rng(_c: &Ctx) -> Value {
    annot(A_RNG)
}

pub fn formulas(c: &Ctx) -> Value {
    let mut f = annot(A_FORMULAS);
    let rows: Map<String, Value> = (0..5).map(|ci| (ci.to_string(), json!(growth(c, ci)))).collect();
    let lu = f["level_up"].as_object_mut().unwrap();
    // keep the documented key order: function, growth_rows, per_level, status
    let mut m = Map::new();
    for (k, v) in lu.iter() {
        m.insert(k.clone(), v.clone());
        if k == "function" {
            m.insert("growth_rows".into(), Value::Object(rows.clone()));
        }
    }
    *lu = m;
    f
}

fn form(c: &Ctx, a_flags: &Map<String, Value>, measured: &Map<String, Value>, i: u32) -> Value {
    let a = CHAR_FORMS + 24 * i;
    let b = c.bytes(a, 24);
    let f0 = u16::from_le_bytes([b[0], b[1]]);
    let nm = u16::from_le_bytes([b[2], b[3]]);
    let big = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let ptr = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
    let gfx = u16::from_le_bytes([b[14], b[15]]);
    let mut m = Map::new();
    m.insert("index".into(), json!(i));
    m.insert("address".into(), json!(ph(a as i64)));
    m.insert("flags".into(), json!(f0));
    m.insert("name_string".into(), json!(nm));
    m.insert("name".into(), c.s(nm as i64));
    m.insert("upper_name".into(), c.s(big as i64));
    m.insert("controller_slot".into(), json!(ph(ptr as i64)));
    m.insert("char_index".into(), json!(b[12]));
    m.insert("unk_0d".into(), json!(b[13]));
    m.insert("gfx_id".into(), json!(gfx));
    m.insert("portrait_or_anim_bytes".into(), json!(b[16..22].to_vec()));
    m.insert("transform_up".into(), json!(b[22]));
    m.insert("transform_down".into(), json!(b[23]));
    let bits: Vec<Value> = a_flags
        .iter()
        .filter(|(k, _)| f0 as u64 & k.parse::<u64>().unwrap() != 0)
        .map(|(_, v)| v.clone())
        .collect();
    m.insert("flag_bits".into(), Value::Array(bits));
    if let Some(mb) = measured.get(&i.to_string()) {
        m.insert("measured_bonus".into(), mb.clone());
    }
    Value::Object(m)
}

/// characters.json: forms 0x086B82EC, new-game records 0x086FDA4C, growth rows 0x086B72E0.
pub fn characters(c: &Ctx) -> Value {
    let a = annot(A_CHARACTERS);
    let flags = obj(&a["flag_bits"]);
    let measured = obj(&a["measured_bonus"]);
    let forms: Vec<Value> = (0..N_FORMS).map(|i| form(c, &flags, &measured, i)).collect();
    let mut chars = Vec::new();
    for ci in 0..9u32 {
        let ra = INIT_RECORDS + 18 * ci;
        let b = c.bytes(ra, 18).to_vec();
        let n = b[11] as usize;
        let skills: Vec<u8> = b[12..(12 + n).min(18)].to_vec();
        let level = b[0] as u32;
        let rec = json!({
            "level": b[0], "hp": u16::from_le_bytes([b[2], b[3]]), "ep": u16::from_le_bytes([b[4], b[5]]),
            "STR": b[6], "POW": b[7], "END": b[8], "SPD": b[9], "form": b[10], "skills": skills,
            "skill_names": skills.iter().map(|&s| skill_name(c, s as u32)).collect::<Vec<_>>(),
            "xp": if (50..=200).contains(&level) { xp_for_level(c, level) } else { 0 },
        });
        let g = growth(c, ci);
        let gdoc = if ci < 5 {
            if g.iter().all(|&x| x == 0) {
                Value::Null
            } else {
                let gf: Vec<f64> = g.iter().map(|&x| x as f64).collect();
                let mut m = Map::new();
                m.insert("raw_u32".into(), json!(g));
                for (k, v) in obj(&a["growth_doc"]) {
                    m.insert(k, v);
                }
                m.insert("hp_add_range".into(), json!([gf[1] / 65536.0, (gf[0] * 255.0 / 256.0 + gf[1]) / 65536.0]));
                m.insert("hp_pct_range".into(), json!([gf[3] / 65536.0, (gf[2] * 255.0 / 256.0 + gf[3]) / 65536.0]));
                m.insert("ep_add_range".into(), json!([gf[5] / 65536.0, (gf[4] * 255.0 + gf[5]) / 65536.0]));
                m.insert("ep_pct_range".into(), json!([gf[7] / 65536.0, (gf[6] * 255.0 / 256.0 + gf[7]) / 65536.0]));
                Value::Object(m)
            }
        } else {
            json!({"raw_u32": g, "note": a["growth_unused_note"]})
        };
        let fl: Vec<u32> = forms
            .iter()
            .filter(|f| f["char_index"].as_u64() == Some(ci as u64))
            .map(|f| f["index"].as_u64().unwrap() as u32)
            .collect();
        chars.push(json!({
            "index": ci, "name": a["char_names"][ci as usize], "record_ram": ph(0x0300_1560 + 0x68 * ci as i64),
            "new_game": rec, "growth": gdoc, "forms": fl,
        }));
    }
    json!({
        "meta": a["meta"], "level_range": a["level_range"], "stat_points": a["stat_points"],
        "level_up": a["level_up"], "movement": a["movement"], "fusion": a["fusion"],
        "characters": chars, "forms": forms, "form_flag_bits": a["form_flag_bits"],
        "record_layout": a["record_layout"],
    })
}
