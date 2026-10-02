//! Supplies / story items (0x083BD2F8) and equipment (0x086F82EC) with their scripts disassembled and the
//! effects summarised.

use crate::script::dis;
use crate::util::*;
use serde_json::{json, Map, Value};

pub const SUPPLIES: u32 = 0x083B_D2F8;
pub const N_SUPPLIES: u32 = 83;
pub const EQUIPMENT: u32 = 0x086F_82EC;
pub const N_EQUIP: u32 = 162;
const SLOTS: [&str; 4] = ["body", "hands", "feet", "accessory"];

const A_ITEMS: &str = include_str!("../data/items.json");

fn native_effect(n: u8, v: i64) -> Option<(&'static str, Value)> {
    Some(match n {
        0xdf => ("hp_regen_per_frame_256ths", json!(v * 0x445 >> 8)),
        0xe0 => ("thorns_damage", json!(v * 0xa4 >> 6)),
        0xe1 => ("END_and_END_energy", json!(v)),
        0xe2 => ("POW", json!(v)),
        0xe3 => ("SPD", json!(v)),
        0xe4 => ("STR", json!(v)),
        0xe5 => ("block_damage_factor", json!("x1/2 (0x1000 -> 0x800 of 65536)")),
        0xe6 => ("energy_attack_cost", json!("x1/2")),
        0xe7 => ("energy_block_cost", json!("x1/2")),
        0xe8 => ("ss_cooldown_rate", json!("x7/8")),
        0xf5 => ("crit_chance_percent", json!(v)),
        _ => return None,
    })
}

fn lines(c: &Ctx, addr: u32, n: usize) -> Vec<String> {
    dis(c, addr, n).into_iter().map(|(_, s)| s).collect()
}

/// Equipment effect script → (disassembly, effects) like `extract_items.effect_summary`.
fn effect_summary(c: &Ctx, addr: u32) -> (Value, Value) {
    if !in_rom_range(addr) {
        return (Value::Null, json!([]));
    }
    let txt = lines(c, addr, 200);
    let mut effects = Vec::new();
    let mut last_push: Option<i64> = None;
    for s in &txt {
        if let Some(r) = s.strip_prefix("push ") {
            last_push = r.split_whitespace().next().and_then(|x| x.parse().ok());
        } else if let Some(r) = s.strip_prefix("native ") {
            let n = u8::from_str_radix(r.split_whitespace().next().unwrap().trim_start_matches("0x"), 16).unwrap();
            if let Some((k, v)) = native_effect(n, last_push.unwrap_or(0)) {
                let mut m = Map::new();
                m.insert(k.into(), v);
                effects.push(Value::Object(m));
            }
        }
    }
    let conditional = txt.iter().any(|s| s.starts_with("jz"));
    let eff = if conditional { json!([{"conditional": true, "natives": effects}]) } else { json!(effects) };
    (json!(txt), eff)
}

fn supply_effect(c: &Ctx, addr: u32) -> (Value, Value) {
    if addr == 0 {
        return (Value::Null, Value::Null);
    }
    let ls = lines(c, addr, 300);
    let any = |p: &str| ls.iter().any(|l| l.starts_with(p));
    let pushes: Vec<i64> = ls
        .iter()
        .filter_map(|x| x.strip_prefix("push "))
        .filter_map(|r| {
            let t = r.split_whitespace().next()?;
            let d = t.trim_start_matches('-');
            if !d.is_empty() && d.chars().all(|ch| ch.is_ascii_digit()) {
                t.parse().ok()
            } else {
                None
            }
        })
        .collect();
    if any("native 0x0d") && any("native 0x0f") {
        return (
            json!(ls),
            json!({"full_restore_hp_ep": true, "note": "HP = maxHP (native 0x0D -> setter 0x0C), EP = maxEP (0x0F -> 0x0E)"}),
        );
    }
    let mut eff = Map::new();
    if ls.iter().any(|l| l == "native 0x0c ") || any("native 0x0c") {
        if any("set 0x0c") && any("native 0x0d") {
            eff.insert("full_restore".into(), json!(true));
        } else if pushes.len() >= 3 {
            eff.insert("restore_hp".into(), json!(pushes[2]));
            eff.insert("restore_hp_with_chopsticks".into(), json!(pushes[1]));
        }
    }
    if any("native 0x0e") && pushes.len() >= 3 && eff.is_empty() {
        eff.insert("restore_ep".into(), json!(pushes[2]));
        eff.insert("restore_ep_with_straw".into(), json!(pushes[1]));
        eff.insert(
            "bug".into(),
            json!("with the Primordial Twisty-Straw owned the game sets HP = EP + bonus instead of EP += bonus"),
        );
    }
    if any("native 0xf2") {
        eff = obj(&json!({"holy_water": true, "duration_seconds": 30, "STR": 25, "POW": 25, "END": 25, "SPD": 25,
            "note": "native 0xF2 pushes (30, 25,25,25,25); bonuses live in GameData+0x4C0 and are applied by 0x080046AE"}));
    }
    (json!(ls), Value::Object(eff))
}

pub fn supply(c: &Ctx, i: u32) -> Value {
    let a = SUPPLIES + 0x34 * i;
    let f0 = c.u16(a);
    let us = c.u32(a + 0x30);
    let (disasm, eff) = supply_effect(c, us);
    json!({
        "id": i, "address": ph(a as i64), "story_item": f0 != 0, "short_name": c.s(c.u16(a + 2) as i64),
        "name": c.s(c.u16(a + 4) as i64), "plural": c.s(c.u16(a + 6) as i64), "description": c.s(c.u32(a + 8) as i64),
        "price": c.s32(a + 0x2c), "use_script": ph(us as i64), "icon": ph(c.u32(a + 0x18) as i64),
        "use_script_disasm": disasm, "effect": eff,
    })
}

pub fn equipment(c: &Ctx, extra: &Value, i: u32) -> Value {
    let a = EQUIPMENT + 0x2c * i;
    let b = c.bytes(a, 0x2c).to_vec();
    let u16at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let u32at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let es = u32at(0x28);
    let bonus = u32at(0x1c);
    let flags = u32at(0x24);
    let (txt, eff) = effect_summary(c, es);
    let mut m = obj(&json!({
        "id": i, "address": ph(a as i64), "name": c.s(u16at(0) as i64), "description": c.s(u16at(2) as i64),
        "slot": SLOTS[b[0x14] as usize & 3], "required_level": b[0x15],
        "STR": b[0x16] as i8, "POW": b[0x17] as i8, "END": b[0x18] as i8, "END_energy": b[0x19] as i8, "SPD": b[0x1a] as i8,
        "byte_1b": b[0x1b], "exp_bonus_256ths": bonus, "price": u32at(0x20) as i32,
        "flags_0x24": flags, "effect_script": ph(es as i64), "icon": ph(u32at(0x10) as i64), "raw": hexbytes(&b),
        "effect_script_disasm": txt, "script_effects": eff,
        "other_effects": extra.get(i.to_string()).cloned().unwrap_or(Value::Null),
    }));
    if bonus != 0 {
        m.insert("exp_multiplier".into(), json!((256.0 + bonus as f64) / 256.0));
    }
    m.insert("recalc_flag".into(), json!(flags != 0));
    Value::Object(m)
}

/// items.json
pub fn items(c: &Ctx) -> Value {
    let a = annot(A_ITEMS);
    json!({
        "meta": a["meta"],
        "supplies": (0..N_SUPPLIES).map(|i| supply(c, i)).collect::<Vec<_>>(),
        "equipment": (0..N_EQUIP).map(|i| equipment(c, &a["extra_effects"], i)).collect::<Vec<_>>(),
    })
}
