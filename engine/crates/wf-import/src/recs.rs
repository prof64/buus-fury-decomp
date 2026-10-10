//! Per-character state against the game's records (report-only, `SEGMENT_RECS=1`): `tools/segments/recs.py` writes
//! the game's state along a segment (work/recs/<name>.json: [[row, state], ...] at row 0 and every change - current
//! character, zenie, supplies, the equipment list, and per character record XP, HP, max HP, EP, max EP, the four
//! stats, points, the Super Saiyan cooldown, form, selected skill, skills, equipment); the replay compares ours at
//! those rows. The harness compares the leader's HP every frame; the rest drifted unseen (app soak, Earth 7-3:
//! Gohan's EP 16 vs 12 through 15,000 rows of exact segments).

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use wf_core::world::World;

/// a tracked sidecar (`data/segments/recs/<name>.json`, `tools/segments/recs_pack.py`): delta rows back to whole
/// states in the recs.py layout
fn unpack(v: &Value) -> Option<Vec<(usize, Value)>> {
    let mut out = vec![];
    let mut cur: Option<Value> = None;
    for r in v["rows"].as_array()? {
        let k = r[0].as_u64()? as usize;
        let d = &r[1];
        let mut s = match cur.take() {
            None => serde_json::json!({ "cur": d["cur"], "zenie": d["zenie"], "equip_list": d["eq"], "supplies": d["sup"], "chars": d["c"] }),
            Some(mut s) => {
                if !d["cur"].is_null() {
                    s["cur"] = d["cur"].clone();
                }
                if !d["zenie"].is_null() {
                    s["zenie"] = d["zenie"].clone();
                }
                if !d["eq"].is_null() {
                    s["equip_list"] = d["eq"].clone();
                }
                if let Some(m) = d["sup"].as_object() {
                    let hex = s["supplies"].as_str().unwrap_or("").to_string();
                    let mut b: Vec<u8> = (0..hex.len() / 2).filter_map(|j| u8::from_str_radix(&hex[2 * j..2 * j + 2], 16).ok()).collect();
                    for (idx, val) in m {
                        if let (Ok(ix), Some(vv)) = (idx.parse::<usize>(), val.as_u64()) {
                            if ix < b.len() {
                                b[ix] = vv as u8;
                            }
                        }
                    }
                    s["supplies"] = Value::String(b.iter().map(|x| format!("{x:02x}")).collect());
                }
                if let Some(m) = d["c"].as_object() {
                    for (c, ch) in m {
                        let (Ok(c), Some(ch)) = (c.parse::<usize>(), ch.as_array()) else { continue };
                        for p in ch.chunks(2) {
                            if let (Some(f), Some(val)) = (p[0].as_u64(), p.get(1)) {
                                s["chars"][c][f as usize] = val.clone();
                            }
                        }
                    }
                }
                s
            }
        };
        out.push((k, s.clone()));
        cur = Some(std::mem::take(&mut s));
    }
    Some(out)
}

/// the game's states for a segment document (by its name; a tile reads its parent's rows from its offset): the
/// tracked sidecar data/segments/recs/<name>.json first, else work/recs/<name>.json (`SEGMENT_RECS_DIR`)
pub fn load(doc: &Value) -> Option<Vec<(usize, Value)>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let dir = std::env::var("SEGMENT_RECS_DIR").map(PathBuf::from).unwrap_or_else(|_| root.join("work/recs"));
    let (name, off) = match doc["source"]["kind"].as_str() {
        Some("tile") => (doc["source"]["of"].as_str()?.to_string(), doc["source"]["at"].as_i64()? as usize),
        _ => (doc["name"].as_str()?.to_string(), 0),
    };
    let side = root.join("data/segments/recs").join(format!("{name}.json"));
    let rows: Vec<(usize, Value)> = match std::fs::read_to_string(&side).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        Some(v) => unpack(&v)?,
        None => {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).ok()?).ok()?;
            v.as_array()?.iter().filter_map(|r| Some((r[0].as_u64()? as usize, r[1].clone()))).collect()
        }
    };
    // a tile: the parent's state current at the offset becomes row 0, later changes shift
    if off > 0 {
        let first = rows.iter().rev().find(|r| r.0 <= off)?.1.clone();
        let mut out = vec![(0, first)];
        out.extend(rows.iter().filter(|r| r.0 > off).map(|r| (r.0 - off, r.1.clone())));
        return Some(out);
    }
    Some(rows)
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

/// our state in the recs.py layout: the records with the live fighters written back (`story::sync_from_players`)
fn ours(w: &World) -> (i64, i64, Vec<u8>, Vec<i64>, Vec<Vec<i64>>) {
    let mut c = w.clone();
    wf_core::story::sync_from_players(&mut c);
    let st = &c.story;
    // the game's list marks a worn entry with its wearer (top nibble; recs.py keeps the unmarked ones): ours derives
    // the marks from the characters' slots
    let marks = wf_core::menu::equip::marks(&c);
    let mut eq: Vec<i64> = st.equipment.iter().zip(marks.iter().chain(std::iter::repeat(&0))).filter(|(_, &m)| m == 0).map(|(&x, _)| x as i64).collect();
    eq.sort();
    let chars = st
        .chars
        .iter()
        .take(9)
        .map(|ch| {
            let r = &ch.rec;
            vec![r.xp, r.hp as i64, r.max_hp as i64, r.ep as i64, r.max_ep as i64, r.str_ as i64, r.pow as i64, r.end as i64, r.spd as i64, r.points as i64, ch.ss_cooldown as i64, ch.form as i64, ch.sel as i64]
        })
        .collect();
    // GameData +0 during a fusion component's level-up hook is the component (`Story::levelup_char`: ours keeps the
    // fusion as `current` and the component beside it; real run 6-1 r033_002 f27,267 .. 27,508)
    (st.levelup_char.unwrap_or(st.current) as i64, st.zenie as i64, st.supplies.iter().take(83).copied().collect(), eq, chars)
}

const FIELDS: [&str; 13] = ["xp", "hp", "max_hp", "ep", "max_ep", "str", "pow", "end", "spd", "points", "ss_cooldown", "form", "sel"];

/// the fields that differ at this row (`field: ours vs game`)
pub fn diff(w: &World, game: &Value) -> Vec<String> {
    let (cur, zenie, sup, eq, chars) = ours(w);
    let mut out = vec![];
    if cur != i(&game["cur"]) {
        out.push(format!("current: {cur} vs {}", i(&game["cur"])));
    }
    if zenie != i(&game["zenie"]) {
        out.push(format!("zenie: {zenie} vs {}", i(&game["zenie"])));
    }
    let gsup: Vec<u8> = game["supplies"].as_str().map(|s| (0..s.len() / 2).filter_map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).ok()).collect()).unwrap_or_default();
    if sup != gsup {
        let d: Vec<String> = sup.iter().zip(gsup.iter()).enumerate().filter(|(_, (a, b))| a != b).map(|(k, (a, b))| format!("#{k} {a} vs {b}")).take(4).collect();
        out.push(format!("supplies: {}", d.join(", ")));
    }
    let geq: Vec<i64> = game["equip_list"].as_array().map(|a| a.iter().map(i).collect()).unwrap_or_default();
    if eq != geq {
        out.push(format!("equipment list: {eq:?} vs {geq:?}"));
    }
    for (c, g) in game["chars"].as_array().into_iter().flatten().enumerate() {
        let Some(o) = chars.get(c) else { continue };
        let g: Vec<i64> = g.as_array().map(|a| a.iter().take(13).map(i).collect()).unwrap_or_default();
        // (a character the game has no record for yet)
        if g.get(2).copied().unwrap_or(0) == 0 && g.first().copied().unwrap_or(0) == 0 {
            continue;
        }
        for (k, f) in FIELDS.iter().enumerate() {
            if o.get(k) != g.get(k) {
                out.push(format!("char {c} {f}: {} vs {}", o.get(k).copied().unwrap_or(-1), g.get(k).copied().unwrap_or(-1)));
            }
        }
    }
    out
}

/// the report of one replay: checks made, rows with a difference, the first one, per field the rows it differed in
#[derive(Default, Debug)]
pub struct Tally {
    pub checks: usize,
    pub bad_rows: usize,
    pub first: Option<(usize, Vec<String>)>,
    pub fields: BTreeMap<String, usize>,
}

impl Tally {
    pub fn check(&mut self, k: usize, w: &World, game: &Value) {
        self.checks += 1;
        let d = diff(w, game);
        // SEGMENT_RECS=all: every differing row
        if !d.is_empty() && std::env::var("SEGMENT_RECS").is_ok_and(|v| v == "all") {
            eprintln!("    recs f{k} {d:?}");
        }
        if d.is_empty() {
            return;
        }
        self.bad_rows += 1;
        for x in &d {
            let key = x.split(':').next().unwrap_or("").to_string();
            *self.fields.entry(key).or_default() += 1;
        }
        if self.first.is_none() {
            self.first = Some((k, d));
        }
    }
}
