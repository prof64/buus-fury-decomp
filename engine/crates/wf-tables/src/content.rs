//! Split the reference-shaped documents into the per-id content files (layout: `engine/docs/tables.md`).
//! [`crate::reassemble`] is the exact inverse and is what the parity tests compare against the references.

use crate::util::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use wf_rom::content::Content;

/// Documents by reference path (relative to the repository's `data/`).
pub type Docs = BTreeMap<String, Value>;

/// `{"id": slug, ...v}`; a numeric `id` field of `v` (game index) moves to `id_field`.
fn with_id(id: &str, v: &Value, extra: &[(&str, Value)]) -> Value {
    with_id_as(id, v, "game_id", extra)
}

fn with_id_as(id: &str, v: &Value, id_field: &str, extra: &[(&str, Value)]) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), json!(id));
    for (k, x) in obj(v) {
        if k == "id" {
            m.insert(id_field.into(), x);
        } else {
            m.insert(k, x);
        }
    }
    for (k, x) in extra {
        m.insert((*k).into(), x.clone());
    }
    Value::Object(m)
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    Value::Object(keys.iter().filter_map(|k| v.get(*k).map(|x| ((*k).to_string(), x.clone()))).collect())
}

fn idx_slug(i: u64, width: usize, name: Option<&str>, fallback: &str) -> String {
    let s = name.map(slugify).unwrap_or_default();
    if s.is_empty() {
        format!("{i:0width$}_{fallback}")
    } else {
        format!("{i:0width$}_{s}")
    }
}

pub fn character_id(c: &Value) -> String {
    slugify(c["name"].as_str().unwrap_or("character"))
}
pub fn form_id(f: &Value) -> String {
    idx_slug(f["index"].as_u64().unwrap(), 2, f["name"].as_str(), "form")
}
pub fn skill_id(s: &Value) -> String {
    idx_slug(s["id"].as_u64().unwrap(), 2, s["name"].as_str(), "skill")
}
pub fn attack_id(sub: u64, name: Option<&str>) -> String {
    idx_slug(sub, 2, name, "attack")
}
pub fn supply_id(s: &Value) -> String {
    idx_slug(s["id"].as_u64().unwrap(), 3, s["name"].as_str(), "item")
}
pub fn enemy_id(e: &Value) -> String {
    let t = e["type"].as_u64().unwrap();
    match e["display_name"].as_str().map(slugify) {
        Some(s) if !s.is_empty() => s,
        _ => format!("type_{t:02x}"),
    }
}
pub fn state_file(s: u32) -> String {
    format!("state_{s:03x}")
}

fn rename_icon(v: &Value, kind: &str) -> Value {
    let mut m = Map::new();
    for (k, x) in obj(v) {
        if k == "icon" {
            m.insert("icon".into(), json!(format!("icons/{kind}/{:03}.png", v["id"].as_u64().unwrap())));
            m.insert("icon_rom".into(), x);
        } else {
            m.insert(k, x);
        }
    }
    Value::Object(m)
}

pub fn split(docs: &Docs, strings: &[String], out: &mut Content) {
    let d = |k: &str| &docs[k];
    for k in ["levels", "formulas", "rng"] {
        out.put_json(format!("{k}.json"), d(&format!("buu/{k}.json")));
    }
    out.put_json("text/strings.json", &json!(strings));

    // ---- characters + forms
    let ch = d("buu/characters.json");
    let forms = ch["forms"].as_array().unwrap();
    let chars = ch["characters"].as_array().unwrap();
    let char_slug: HashMap<u64, String> =
        chars.iter().map(|c| (c["index"].as_u64().unwrap(), character_id(c))).collect();
    let mut cidx = Vec::new();
    for c in chars {
        let id = character_id(c);
        let fids: Vec<String> = c["forms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| form_id(&forms[i.as_u64().unwrap() as usize]))
            .collect();
        out.put_json(format!("characters/{id}.json"), &with_id(&id, c, &[("form_ids", json!(fids))]));
        cidx.push(json!({"id": id, "index": c["index"], "name": c["name"]}));
    }
    let mut ci = obj(&pick(
        ch,
        &["meta", "level_range", "stat_points", "level_up", "movement", "fusion", "form_flag_bits", "record_layout"],
    ));
    ci.insert("characters".into(), json!(cidx));
    out.put_json("characters/index.json", &Value::Object(ci));
    let mut fidx = Vec::new();
    for f in forms {
        let id = form_id(f);
        let owner = char_slug.get(&f["char_index"].as_u64().unwrap()).cloned();
        out.put_json(format!("forms/{id}.json"), &with_id(&id, f, &[("character", json!(owner))]));
        fidx.push(json!({"id": id, "index": f["index"], "name": f["name"], "character": owner}));
    }
    out.put_json("forms/index.json", &json!({"forms": fidx}));

    // ---- player moves, skills, attack types
    let mv = d("buu/moves.json");
    let ad = d("buu/attacks_detail.json");
    let mut moves = obj(&pick(mv, &["meta", "melee", "skill_levels"]));
    moves.insert(
        "attacks_detail".into(),
        pick(ad, &["melee", "block", "energy_block", "dash", "hit_struct", "ki_blast", "beams", "big_bang"]),
    );
    out.put_json("moves.json", &Value::Object(moves));
    let mut sidx = Vec::new();
    for s in mv["skills"].as_array().unwrap() {
        let id = skill_id(s);
        let det = ad["skills"].as_array().unwrap().iter().find(|x| x["id"] == s["id"]).cloned().unwrap_or(Value::Null);
        out.put_json(format!("skills/{id}.json"), &with_id_as(&id, s, "skill", &[("detail", det)]));
        sidx.push(json!({"id": id, "skill": s["id"], "name": s["name"]}));
    }
    out.put_json("skills/index.json", &json!({"skills": sidx}));
    let names = obj(&ad["attack_sub_names"]);
    let mt = mv["attack_types"].as_array().unwrap();
    let at = ad["attack_types"].as_array().unwrap();
    let mut aidx = Vec::new();
    for sub in 0..mt.len().max(at.len()) {
        let name = names.get(&sub.to_string()).cloned();
        let id = attack_id(sub as u64, name.as_ref().and_then(|n| n.as_str()));
        let mut m = Map::new();
        m.insert("id".into(), json!(id));
        m.insert("sub".into(), json!(sub));
        if let Some(n) = &name {
            m.insert("name".into(), n.clone());
        }
        m.insert("type".into(), mt.get(sub).cloned().unwrap_or(Value::Null));
        m.insert("detail".into(), at.get(sub).cloned().unwrap_or(Value::Null));
        out.put_json(format!("attacks/{id}.json"), &Value::Object(m));
        aidx.push(json!({"id": id, "sub": sub, "name": name}));
    }
    out.put_json("attacks/index.json", &json!({"attacks": aidx}));

    // ---- items / equipment
    let it = d("buu/items.json");
    let mut iidx = Vec::new();
    for s in it["supplies"].as_array().unwrap() {
        let id = supply_id(s);
        out.put_json(format!("items/{id}.json"), &with_id_as(&id, &rename_icon(s, "supply"), "item", &[]));
        iidx.push(json!({"id": id, "item": s["id"], "name": s["name"]}));
    }
    out.put_json("items/index.json", &json!({"meta": it["meta"], "supplies": iidx}));
    let mut eidx = Vec::new();
    for e in it["equipment"].as_array().unwrap() {
        let id = supply_id(e);
        out.put_json(format!("equipment/{id}.json"), &with_id_as(&id, &rename_icon(e, "equipment"), "item", &[]));
        eidx.push(json!({"id": id, "item": e["id"], "name": e["name"], "slot": e["slot"]}));
    }
    out.put_json("equipment/index.json", &json!({"equipment": eidx}));

    // ---- enemies (+ their AI summary, death node, boss link)
    let en = d("buu/enemies.json");
    let ai = d("buu/enemy_ai.json");
    let ex = d("battle/buu_ai_extra.json");
    let bo = d("buu/bosses.json");
    let ens = en["enemies"].as_array().unwrap();
    let eslug: Vec<String> = ens.iter().map(enemy_id).collect();
    let mut nidx = Vec::new();
    for (t, e) in ens.iter().enumerate() {
        let id = &eslug[t];
        let a = &ai["enemies"][t];
        let th = ph(t as i64);
        let boss = if bo["bosses"].get(&th).is_some() { json!(id) } else { Value::Null };
        let extra = [
            ("ai_tree", pick(a, &["initial_state", "states", "summary"])),
            ("death", ex["death"][&th].clone()),
            ("boss", boss),
        ];
        out.put_json(format!("enemies/{id}.json"), &with_id(id, e, &extra));
        nidx.push(json!({"id": id, "type": t, "name": e["display_name"]}));
    }
    out.put_json("enemies/index.json", &json!({"meta": en["meta"], "enemies": nidx}));

    // ---- AI states
    let main = obj(&ai["states"]);
    let mut ids: Vec<u32> = main.keys().map(|k| crate::ai::parse_hex(k)).collect();
    for k in obj(&ex["aux"]).keys().chain(obj(&ex["reactions"]).keys()) {
        ids.push(crate::ai::parse_hex(k));
    }
    ids.sort();
    ids.dedup();
    let mut stidx = Vec::new();
    for s in ids {
        let h = ph(s as i64);
        let mut m = Map::new();
        m.insert("id".into(), json!(state_file(s)));
        m.insert("state".into(), json!(s));
        m.insert("state_hex".into(), json!(h));
        if let Some(st) = main.get(&h) {
            m.insert("text".into(), st["text"].clone());
            m.insert("tree".into(), st["tree"].clone());
        }
        for (k, src) in [("aux", &ex["aux"]), ("reaction", &ex["reactions"])] {
            if let Some(v) = src.get(&h) {
                m.insert(k.into(), v.clone());
            }
        }
        stidx.push(json!({"id": state_file(s), "state": s, "behaviour": main.contains_key(&h),
                          "aux": m.contains_key("aux"), "reaction": m.contains_key("reaction")}));
        out.put_json(format!("ai/{}.json", state_file(s)), &Value::Object(m));
    }
    let mut ai_index = obj(&pick(ai, &["meta", "node_types"]));
    ai_index.insert(
        "on_hit".into(),
        pick(ex, &["source", "onhit_protocol", "onhit_fns", "aux_only", "extra_nodes"]),
    );
    ai_index.insert("states".into(), json!(stidx));
    out.put_json("ai/index.json", &Value::Object(ai_index));

    // ---- bosses
    let mut bidx = Vec::new();
    for (th, b) in obj(&bo["bosses"]) {
        let t = b["type"].as_u64().unwrap() as usize;
        let id = &eslug[t];
        out.put_json(format!("bosses/{id}.json"), &with_id(id, &b, &[("enemy", json!(id))]));
        bidx.push(json!({"id": id, "type": t, "type_hex": th, "name": b["name"]}));
    }
    let mut bi = obj(&pick(bo, &["meta", "projectile_base", "projectiles", "regular_enemy_specials", "node_registry"]));
    bi.insert("bosses".into(), json!(bidx));
    out.put_json("bosses/index.json", &Value::Object(bi));
}

fn strip(v: &Value, drop: &[&str]) -> Value {
    Value::Object(obj(v).into_iter().filter(|(k, _)| !drop.contains(&k.as_str())).collect())
}

/// Undo [`with_id_as`]: drop the slug, put the game id back as `id`, drop `extra` keys, undo the icon rename.
fn back(v: &Value, id_field: &str, extra: &[&str]) -> Value {
    let mut m = Map::new();
    for (k, x) in obj(v) {
        if k == "id" || extra.contains(&k.as_str()) || k == "icon_rom" {
            continue;
        }
        if k == id_field {
            m.insert("id".into(), x);
        } else if k == "icon" && v.get("icon_rom").is_some() {
            m.insert("icon".into(), v["icon_rom"].clone());
        } else {
            m.insert(k, x);
        }
    }
    Value::Object(m)
}

/// Rebuild the reference-shaped documents from the content files alone (inverse of [`split`]).
pub fn reassemble(get: &dyn Fn(&str) -> Option<Value>) -> Docs {
    let g = |p: &str| get(p).unwrap_or_else(|| panic!("missing content file {p}"));
    let list = |index: &str, key: &str, dir: &str| -> Vec<Value> {
        g(index)[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| g(&format!("{dir}/{}.json", e["id"].as_str().unwrap())))
            .collect()
    };
    let mut docs = Docs::new();
    for k in ["levels", "formulas", "rng"] {
        docs.insert(format!("buu/{k}.json"), g(&format!("{k}.json")));
    }
    // characters
    let ci = g("characters/index.json");
    let chars: Vec<Value> =
        list("characters/index.json", "characters", "characters").iter().map(|c| strip(c, &["id", "form_ids"])).collect();
    let forms: Vec<Value> =
        list("forms/index.json", "forms", "forms").iter().map(|f| strip(f, &["id", "character"])).collect();
    let mut cd = obj(&strip(&ci, &["characters"]));
    cd.insert("characters".into(), json!(chars));
    cd.insert("forms".into(), json!(forms));
    docs.insert("buu/characters.json".into(), Value::Object(cd));
    // moves + attacks
    let mv = g("moves.json");
    let skills = list("skills/index.json", "skills", "skills");
    let atks = list("attacks/index.json", "attacks", "attacks");
    docs.insert(
        "buu/moves.json".into(),
        json!({
            "meta": mv["meta"], "melee": mv["melee"],
            "skills": skills.iter().map(|s| back(s, "skill", &["detail"])).collect::<Vec<_>>(),
            "attack_types": atks.iter().filter(|a| !a["type"].is_null()).map(|a| a["type"].clone()).collect::<Vec<_>>(),
            "skill_levels": mv["skill_levels"],
        }),
    );
    let mut a = obj(&mv["attacks_detail"]);
    a.insert(
        "attack_types".into(),
        json!(atks.iter().filter(|x| !x["detail"].is_null()).map(|x| x["detail"].clone()).collect::<Vec<_>>()),
    );
    a.insert(
        "attack_sub_names".into(),
        Value::Object(atks.iter().filter_map(|x| x.get("name").map(|n| (x["sub"].to_string(), n.clone()))).collect()),
    );
    a.insert(
        "skills".into(),
        json!(skills.iter().filter(|s| !s["detail"].is_null()).map(|s| s["detail"].clone()).collect::<Vec<_>>()),
    );
    docs.insert("buu/attacks_detail.json".into(), Value::Object(a));
    // items
    let ii = g("items/index.json");
    let sup: Vec<Value> = list("items/index.json", "supplies", "items").iter().map(|s| back(s, "item", &[])).collect();
    let eq: Vec<Value> =
        list("equipment/index.json", "equipment", "equipment").iter().map(|s| back(s, "item", &[])).collect();
    docs.insert("buu/items.json".into(), json!({"meta": ii["meta"], "supplies": sup, "equipment": eq}));
    // enemies + ai
    let ei = g("enemies/index.json");
    let ens: Vec<Value> = ei["enemies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| g(&format!("enemies/{}.json", e["id"].as_str().unwrap())))
        .collect();
    docs.insert(
        "buu/enemies.json".into(),
        json!({"meta": ei["meta"], "enemies": ens.iter().map(|e| strip(e, &["id", "ai_tree", "death", "boss"])).collect::<Vec<_>>()}),
    );
    let ai_i = g("ai/index.json");
    let states: Vec<Value> = ai_i["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| g(&format!("ai/{}.json", s["id"].as_str().unwrap())))
        .collect();
    let mut st = Map::new();
    let (mut aux, mut rea) = (Map::new(), Map::new());
    for s in &states {
        let h = s["state_hex"].as_str().unwrap().to_string();
        if s.get("tree").is_some() {
            st.insert(h.clone(), json!({"text": s["text"], "tree": s["tree"]}));
        }
        if let Some(x) = s.get("aux") {
            aux.insert(h.clone(), x.clone());
        }
        if let Some(x) = s.get("reaction") {
            rea.insert(h, x.clone());
        }
    }
    let ai_en: Vec<Value> = ens
        .iter()
        .map(|e| {
            let t = &e["ai_tree"];
            json!({"type": e["type"], "name": e["name"], "initial_state": t["initial_state"], "states": t["states"],
                   "summary": t["summary"]})
        })
        .collect();
    docs.insert(
        "buu/enemy_ai.json".into(),
        json!({"meta": ai_i["meta"], "node_types": ai_i["node_types"], "states": st, "enemies": ai_en}),
    );
    let oh = &ai_i["on_hit"];
    let death: Map<String, Value> = ens.iter().map(|e| (ph(e["type"].as_i64().unwrap()), e["death"].clone())).collect();
    docs.insert(
        "battle/buu_ai_extra.json".into(),
        json!({"source": oh["source"], "onhit_protocol": oh["onhit_protocol"], "onhit_fns": oh["onhit_fns"],
               "aux_only": oh["aux_only"], "extra_nodes": oh["extra_nodes"], "aux": aux, "reactions": rea,
               "death": death}),
    );
    // bosses
    let bi = g("bosses/index.json");
    let bosses: Map<String, Value> = bi["bosses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (b["type_hex"].as_str().unwrap().to_string(),
             strip(&g(&format!("bosses/{}.json", b["id"].as_str().unwrap())), &["id", "enemy"]))
        })
        .collect();
    let mut bd = obj(&strip(&bi, &["bosses"]));
    bd.insert("bosses".into(), Value::Object(bosses));
    docs.insert("buu/bosses.json".into(), Value::Object(bd));
    docs
}
