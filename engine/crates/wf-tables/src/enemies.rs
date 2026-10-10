//! Enemy types (0x08711274, 150 x 28 B), the Capsule Corp database (0x0874D380), spawn records from the area
//! table (0x0808E2E0), drop scripts and their simulated loot.

use crate::script::{dis, Drop, Vm};
use crate::util::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const ENEMY_TYPES: u32 = 0x0871_1274;
pub const N_ENEMY: u32 = 0x96;
pub const ENEMY_DB: u32 = 0x0874_D380;
pub const N_DB: u32 = 232;
pub const AI_STATES: u32 = 0x0870_F514;
pub const GFX_TABLE: u32 = 0x086B_6BB4;
pub const AREAS: u32 = 0x0808_E2E0;
pub const N_AREAS: u32 = 0x1C4;
const ENEMY_CTOR: u32 = 0x0800_FD55;

const A_ENEMIES: &str = include_str!("../data/enemies.json");

#[derive(Clone, Debug)]
pub struct Spawn {
    pub area_index: u32,
    pub world: u8,
    pub map: u8,
    pub area: String,
    pub list: u32,
    pub record: u32,
    pub typ: u32,
    pub x: i16,
    pub y: i16,
    pub gfx: u32,
    pub cond: u32,
    pub extra: u32,
}

/// Every enemy spawn record (ctor 0x0800FD55) in the five actor lists of every area record.
pub fn spawns(c: &Ctx) -> Vec<Spawn> {
    let mut out = Vec::new();
    for i in 0..N_AREAS {
        let a = AREAS + 0x38 * i;
        let nm = c.u32(a + 0xc);
        let name = if nm != 0 { c.s_str(nm as i64).unwrap_or_default() } else { String::new() };
        for li in 0..5u32 {
            let cnt = c.u8(a + 3 + li) as u32;
            let lp = c.u32(a + 0x10 + 4 * li);
            if !in_rom_range(lp) || cnt == 0 || cnt > 100 {
                continue;
            }
            for k in 0..cnt {
                let rec = c.u32(lp + 4 * k);
                if !in_rom_range(rec) || c.u32(rec) != ENEMY_CTOR {
                    continue;
                }
                let typ = c.u32(rec + 8);
                if typ > 0x95 {
                    continue;
                }
                out.push(Spawn {
                    area_index: i,
                    world: c.u8(a),
                    map: c.u8(a + 1),
                    area: name.clone(),
                    list: li,
                    record: rec,
                    typ,
                    x: c.s16(rec + 0xc),
                    y: c.s16(rec + 0xe),
                    gfx: c.u32(rec + 0x10),
                    cond: c.u32(rec + 4),
                    extra: c.u32(rec + 0x14),
                });
            }
        }
    }
    out
}

pub struct DbEntry {
    pub index: u32,
    pub name: Value,
    pub description: Value,
    pub gfx_id: u32,
    pub portrait: i64,
    pub link: i32,
}

pub fn db_entries(c: &Ctx) -> Vec<DbEntry> {
    (0..N_DB)
        .map(|i| {
            let a = ENEMY_DB + 16 * i;
            let y = c.u32(a + 8);
            DbEntry {
                index: i,
                name: c.s(c.u16(a) as i64),
                description: c.s(c.u16(a + 2) as i64),
                gfx_id: c.u32(a + 4),
                portrait: if y != 0xffff_ffff { y as i64 } else { -1 },
                link: c.s32(a + 12),
            }
        })
        .collect()
}

pub fn enemy_type(c: &Ctx, t: u32) -> Map<String, Value> {
    let a = ENEMY_TYPES + 28 * t;
    let w: Vec<u32> = (0..7).map(|k| c.u32(a + 4 * k)).collect();
    let b = c.bytes(a, 28);
    obj(&json!({
        "type": t, "address": ph(a as i64), "exp": w[0], "hp": w[1], "ai_state": w[2], "level_byte": b[0xc],
        "STR": b[0xd], "POW": b[0xe], "END": b[0xf], "resist_melee": b[0x10], "resist_energy": b[0x11],
        "byte_12": b[0x12], "byte_13": b[0x13], "hit_reaction": ph(w[5] as i64), "drop_script": ph(w[6] as i64),
    }))
}

/// Monte-Carlo of loot script #497 (`re/buu/loot.py`), Python-identical RNG stream.
pub struct LootSim {
    pub avg_drops: f64,
    pub avg_zenie: f64,
    /// (key, probability per kill) sorted by frequency (ties: first seen first)
    pub per_kill: Vec<(String, f64)>,
}

pub fn simulate(c: &Ctx, level: i64, forced: i64, n: u32, seed: u32) -> LootSim {
    let mut vm = Vm::new(c, seed);
    let mut counts: Vec<(Drop, u64)> = Vec::new();
    let mut idx: HashMap<Drop, usize> = HashMap::new();
    let mut zenie: i64 = 0;
    let mut ndrops_total: u64 = 0;
    for _ in 0..n {
        vm.drops.clear();
        vm.g = [0; 4];
        vm.p = [level, forced, 0, 0];
        vm.run(497);
        ndrops_total += vm.drops.len() as u64;
        for d in vm.drops.clone() {
            if let Drop::Zenie(z) = d {
                zenie += z;
            }
            let i = *idx.entry(d).or_insert_with(|| {
                counts.push((d, 0));
                counts.len() - 1
            });
            counts[i].1 += 1;
        }
    }
    let mut order: Vec<usize> = (0..counts.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(counts[i].1)); // stable
    let per_kill = order
        .into_iter()
        .map(|i| {
            let (d, k) = counts[i];
            let key = match d {
                Drop::Zenie(z) => format!("zenie:{z}"),
                Drop::Supply(s) => format!("supply:{s}"),
                Drop::Equip(e) => format!("equip:{e}"),
            };
            (key, pyround(k as f64 / n as f64, 5))
        })
        .collect();
    LootSim { avg_drops: ndrops_total as f64 / n as f64, avg_zenie: zenie as f64 / n as f64, per_kill }
}

fn drop_params(c: &Ctx, addr: u32) -> Value {
    if addr == 0 {
        return Value::Null;
    }
    let lines: Vec<String> = dis(c, addr, 40).into_iter().map(|(_, s)| s).collect();
    if let Some(l) = lines.first() {
        if let Some(r) = l.strip_prefix("pushn [497,") {
            let rest = format!("497,{r}");
            let v: Vec<i64> = rest.trim_end_matches(']').split(',').map(|x| x.trim().parse().unwrap()).collect();
            return json!({"script": ph(addr as i64), "loot_level": v[1],
                          "forced_item": if v[2] >= 0 { json!(v[2]) } else { Value::Null }});
        }
    }
    json!({"script": ph(addr as i64), "raw": lines})
}

/// enemies.json
pub fn enemies(c: &Ctx) -> Value {
    let a = annot(A_ENEMIES);
    let samples = a["loot_samples"].as_u64().unwrap() as u32;
    let seed = a["loot_seed"].as_u64().unwrap() as u32;
    let db = db_entries(c);
    let mut by_link: HashMap<i64, Vec<&DbEntry>> = HashMap::new();
    let mut by_gfx: HashMap<u32, Vec<&DbEntry>> = HashMap::new();
    for d in &db {
        by_link.entry(d.link as i64).or_default().push(d);
        by_gfx.entry(d.gfx_id).or_default().push(d);
    }
    let sp = spawns(c);
    let mut sp_by_type: HashMap<u32, Vec<&Spawn>> = HashMap::new();
    for s in &sp {
        sp_by_type.entry(s.typ).or_default().push(s);
    }
    let mut loot_cache: HashMap<(i64, i64), LootSim> = HashMap::new();
    let mut enemies: Vec<Map<String, Value>> = Vec::new();
    let empty = Vec::new();
    for t in 0..N_ENEMY {
        let mut en = enemy_type(c, t);
        let tsp = sp_by_type.get(&t).unwrap_or(&empty);
        let gfx_ids: BTreeSet<u32> = tsp.iter().map(|s| s.gfx).collect();
        let mut names: Vec<&DbEntry> = by_link.get(&(t as i64)).cloned().unwrap_or_default();
        let mut name_source = if names.is_empty() { Value::Null } else { json!("database") };
        if names.is_empty() {
            for g in &gfx_ids {
                if let Some(v) = by_gfx.get(g) {
                    names = v.clone();
                    name_source = json!("gfx");
                    break;
                }
            }
        }
        en.insert("name".into(), names.first().map(|d| d.name.clone()).unwrap_or(Value::Null));
        en.insert("name_source".into(), name_source);
        let database: Vec<Value> = by_link
            .get(&(t as i64))
            .map(|v| {
                v.iter()
                    .map(|d| json!({"index": d.index, "name": d.name, "gfx_id": d.gfx_id, "portrait": d.portrait,
                                    "description": d.description}))
                    .collect()
            })
            .unwrap_or_default();
        en.insert("database".into(), json!(database));
        en.insert("gfx_ids".into(), json!(gfx_ids.iter().collect::<Vec<_>>()));
        let defs: Map<String, Value> = gfx_ids
            .iter()
            .filter(|&&g| g < 0x200)
            .map(|&g| (g.to_string(), json!(ph(c.u32(GFX_TABLE + 4 * g) as i64))))
            .collect();
        en.insert("gfx_defs".into(), Value::Object(defs));
        let hr = en["hit_reaction"].as_str().unwrap().to_string();
        let hr_int = i64::from_str_radix(hr.trim_start_matches("0x"), 16).unwrap();
        if let Some(r) = a["reactions"].get(hr_int.to_string()) {
            en.insert("hit_reaction".into(), r.clone());
        }
        let ds = u32::from_str_radix(en["drop_script"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
        let mut dp = drop_params(c, ds);
        if let Some(lv) = dp.get("loot_level").and_then(|v| v.as_i64()) {
            let forced = dp["forced_item"].as_i64().unwrap_or(-1);
            let sim = loot_cache.entry((lv, forced)).or_insert_with(|| simulate(c, lv, forced, samples, seed));
            let top: Map<String, Value> = sim.per_kill.iter().take(15).map(|(k, v)| (k.clone(), json!(v))).collect();
            dp.as_object_mut().unwrap().insert(
                "simulated_per_kill".into(),
                json!({"avg_drops": pyround(sim.avg_drops, 3), "avg_zenie": pyround(sim.avg_zenie, 2), "top": top}),
            );
        }
        en.insert("drops".into(), dp);
        let ai_state = en["ai_state"].as_u64().unwrap() as u32;
        en.insert(
            "ai".into(),
            json!({"initial_state": ai_state, "state_def": ph(c.u32(AI_STATES.wrapping_add(8u32.wrapping_mul(ai_state))) as i64),
                   "see": "enemy_ai.json"}),
        );
        let mut locs: BTreeMap<(String, u8, u8), u32> = BTreeMap::new();
        for s in tsp {
            *locs.entry((s.area.clone(), s.world, s.map)).or_default() += 1;
        }
        en.insert(
            "spawns".into(),
            json!(locs
                .into_iter()
                .map(|((ar, w, m), n)| json!({"area": ar, "world": w, "map": m, "count": n}))
                .collect::<Vec<_>>()),
        );
        en.insert(
            "spawn_points".into(),
            json!(tsp
                .iter()
                .map(|s| json!({"world": s.world, "map": s.map, "x": s.x, "y": s.y, "gfx": s.gfx,
                                "condition": ph(s.cond as i64), "record": ph(s.record as i64)}))
                .collect::<Vec<_>>()),
        );
        en.insert("stats_note".into(), a["stats_note"].clone());
        enemies.push(en);
    }
    // unnamed types that share XP/HP with a neighbouring named type are variants of it (INFERRED)
    for i in 0..enemies.len() {
        if !enemies[i]["name"].is_null() {
            continue;
        }
        for j in [i as i64 + 1, i as i64 - 1] {
            if j < 0 || j as usize >= enemies.len() {
                continue;
            }
            let o = &enemies[j as usize];
            if !o["name"].is_null() && o["exp"] == enemies[i]["exp"] && o["hp"] == enemies[i]["hp"] && enemies[i]["hp"] != 0 {
                let nm = format!("{} (variant)", o["name"].as_str().unwrap());
                let src = format!("inferred: same XP/HP as type {}", ph(o["type"].as_i64().unwrap()));
                enemies[i].insert("name".into(), json!(nm));
                enemies[i].insert("name_source".into(), json!(src));
                break;
            }
        }
        if enemies[i]["name"].is_null() && enemies[i]["hp"] == 0 {
            enemies[i].insert("name_source".into(), json!("no HP: scripted/non-combat actor"));
        }
    }
    for en in enemies.iter_mut() {
        let t = en["type"].as_u64().unwrap().to_string();
        if en["name"].is_null() {
            if let Some(r) = a["rom_text_names"].get(&t) {
                en.insert("name".into(), c.s(r["string_id"].as_i64().unwrap()));
                en.insert("name_source".into(), r["evidence"].clone());
            }
        }
        let (dn, dsrc) = if let Some(q) = a["qualified"].get(&t) {
            (q.clone(), "game name + qualifier")
        } else if let Some(d) = a["descriptive"].get(&t) {
            (d.clone(), "descriptive")
        } else if !en["name"].is_null() && en["name_source"].as_str().map(|s| s.starts_with("inferred")).unwrap_or(false) {
            (en["name"].clone(), "descriptive")
        } else {
            (en["name"].clone(), "game")
        };
        en.insert("display_name".into(), dn);
        en.insert("display_name_source".into(), json!(dsrc));
    }
    json!({"meta": a["meta"], "enemies": enemies})
}
