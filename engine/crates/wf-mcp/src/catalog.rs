//! Discovery: fighters (forms, skills), enemies (group, stats, resistances), arenas and the story presets.
//! Presets and enemy groups mirror the reference ruleset's lists (site/src/battle/rulesets/buu/index.ts).

use serde_json::{json, Value};
use wf_core::battle::{Phase, ARENAS, FIGHTERS};
use wf_core::content::GameData;

pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub phases: &'static [(&'static str, i32)],
    pub arena: &'static str,
}

macro_rules! preset {
    ($id:expr, $name:expr, $note:expr, [$(($e:expr, $n:expr)),*], $arena:expr) => {
        Preset { id: $id, name: $name, note: $note, phases: &[$(($e, $n)),*], arena: $arena }
    };
}

pub const PRESETS: &[Preset] = &[
    preset!("buu_saga", "Majin Buu saga", "Majin Buu (West City) -> Super Buu (Lookout, two-phase) -> Super Buu (Brain) -> Kid Buu", [("t28", 1), ("t116", 1), ("t121", 1), ("t60", 1)], "crater"),
    preset!("majin_buu", "Majin Buu & Babidi (Giant Crater) - survive", "Buu takes no damage (story encounter): win by surviving 60 s; candy beam, stretch punch, eat", [("t26+~t29", 1)], "crater"),
    preset!("majin_buu2", "Majin Buu (2nd encounter) - survive", "invulnerable (story encounter): survive 60 s", [("t27", 1)], "crater"),
    preset!("super_buu", "Super Buu (two-phase, absorbs ki blasts)", "", [("t116", 1)], "crater2"),
    preset!("super_buu_waste", "Super Buu: Barren Wasteland", "minion swarm, then the scripted waypoint fight: he takes no damage, bat his big ball back three times (punch while he glides)", [("t118", 1), ("t119", 1)], "wasteland"),
    preset!("kid_buu", "Kid Buu", "splits in two when you keep your distance, merges back after 1800 frames", [("t60", 1)], "wasteland"),
    preset!("janemba", "Janemba -> Super Janemba (HFIL)", "hand strikes, airborne summons of Mini-Janembas; then Super Janemba (teleports, energy wave)", [("t51", 1), ("t122", 1)], "hfil"),
    preset!("broly", "Broly (Volcano Summit)", "charged punch, jump smash; landing on a rock stuns him", [("t10", 1)], "volcano"),
    preset!("babidi_ship", "Babidi's ship: PuiPui -> Yakon -> Dabura", "", [("t96", 1), ("t145", 1), ("t18", 1)], "demon"),
    preset!("yakon", "Yakon", "absorbs energy: energy attacks heal him except in the ~60-frame pause after his 6-punch combo (AI state 0x323) where they do full damage; melee x0.05", [("t145", 1)], "yakon"),
    preset!("dabura", "Dabura", "dark sphere, stone spit", [("t18", 1)], "demon"),
    preset!("majin_vegeta", "Majin Vegeta (Final Flash)", "", [("t70", 1)], "canyon"),
    preset!("tournament", "World Tournament: Spopovich & Yamu", "bear hug + EP drain", [("t111+t147", 1)], "stadium"),
    preset!("htc", "Hyperbolic Time Chamber sparring -> Super Buu", "", [("t39", 1), ("t130", 1), ("t115", 1)], "htc"),
    preset!("inside_buu", "Inside Buu: Gotenks -> Gohan -> Piccolo -> Super Buu", "", [("t40", 1), ("t35", 1), ("t90", 1), ("t121", 1)], "htc"),
    preset!("pikkon", "Pikkon (Other World tournament)", "", [("t91", 1)], "stadium"),
    preset!("totenhotep", "Pharoah Totenhotep (mummy wraps)", "", [("t129", 1)], "crater"),
    preset!("ninja_boss", "Ninja Boss", "", [("t80", 1)], "forest"),
    preset!("guardian", "Pilaf's Guardian", "", [("t63", 1)], "forest"),
    preset!("warlord", "Airship Warlord", "", [("t0", 1)], "forest"),
    preset!("blister", "Blister (spawns Enzymes)", "", [("t6", 1)], "crater"),
    preset!("worm", "Worm Head (+ 5 Worm Body segments)", "", [("t142", 1)], "stomach"),
    preset!("majin_fighters", "Southwest Forest: 4 Majin Fighters", "", [("t68", 4)], "forest"),
];

const BOSS_TYPES: &[i32] = &[
    0x00, 0x06, 0x0a, 0x0b, 0x12, 0x1a, 0x1b, 0x1c, 0x23, 0x25, 0x27, 0x28, 0x33, 0x3c, 0x3d, 0x3f, 0x46, 0x50, 0x51, 0x5a, 0x5b, 0x60, 0x64, 0x6f, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
    0x7a, 0x81, 0x82, 0x8e, 0x91, 0x93, 0x2d, 0x31, 0x55, 0x6d, 0x6e, 0x70, 0x94, 0x84, 0x87, 0x3b, 0x26, 0x4, 0x19, 0x1d,
];
const HELPER_TYPES: &[i32] = &[0x0d, 0x0e, 0x22, 0x34, 0x35, 0x36, 0x37, 0x54, 0x72, 0x7e, 0x8d, 0x88, 0x89];

pub fn group(ty: i32) -> &'static str {
    if BOSS_TYPES.contains(&ty) {
        "Boss"
    } else if HELPER_TYPES.contains(&ty) {
        "Helper"
    } else {
        "Enemy"
    }
}

pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

pub fn skill_name(d: &GameData, id: i32) -> String {
    d.skills.get(&id).map(|s| s.name.clone()).unwrap_or_else(|| format!("skill {id}"))
}

pub fn form_name(d: &GameData, id: i32) -> String {
    let Some(f) = d.form(id) else { return format!("form {id}") };
    let mut n = f.name.clone();
    if f.ss3 {
        n = format!("SS3 {n}");
    } else if f.ss {
        n = format!("SS {n}");
    }
    n
}

pub fn fighters(d: &GameData) -> Value {
    let list: Vec<Value> = FIGHTERS
        .iter()
        .filter(|f| !f.0.starts_with('f'))
        .map(|f| {
            let ch = d.chars.iter().find(|c| c.index == f.1);
            let skills: Vec<Value> = ch
                .map(|c| c.skills.iter().map(|&s| {
                    let sk = d.skills.get(&s);
                    json!({"id": s, "name": skill_name(d, s), "ep_by_skill_level": sk.map(|k| k.costs.clone()).unwrap_or_default()})
                }).collect())
                .unwrap_or_default();
            json!({
                "id": f.0,
                "name": ch.map(|c| c.name.clone()).unwrap_or_default(),
                "default_form": f.4,
                "forms": f.3.iter().map(|&x| format!("{x}: {}", form_name(d, x))).collect::<Vec<_>>(),
                "fusion_of": f.2.map(|(a, b)| vec![a, b]),
                "new_game_level": ch.map(|c| c.level),
                "skills": skills,
            })
        })
        .collect();
    json!({
        "fighters": list,
        "buttons": {"A": "punch", "B": "use selected skill (hold to charge)", "L": "cycle skill", "R": "block (R+B energy shield)", "double-tap d-pad": "dash"},
    })
}

fn presets_using(ty: i32) -> Vec<&'static str> {
    PRESETS
        .iter()
        .filter(|p| p.phases.iter().any(|(e, _)| parse_ids(e).iter().any(|(t, _)| *t == ty)))
        .map(|p| p.id)
        .collect()
}

pub fn parse_ids(s: &str) -> Vec<(i32, bool)> {
    s.split('+').filter_map(|p| p.trim_start_matches('~').trim_start_matches('t').parse::<i32>().ok().map(|t| (t, !p.starts_with('~')))).collect()
}

pub fn enemies(d: &GameData, search: Option<&str>, grp: Option<&str>, limit: usize) -> Value {
    let q = search.map(norm).unwrap_or_default();
    let all: Vec<&wf_core::content::EnemyType> = d
        .enemies
        .iter()
        .filter(|e| e.hp > 0 || e.ai != 0)
        .filter(|e| q.is_empty() || norm(&e.name).contains(&q) || format!("t{}", e.ty) == search.unwrap_or(""))
        .filter(|e| grp.is_none_or(|g| norm(group(e.ty)).contains(&norm(g))))
        .collect();
    let list: Vec<Value> = all
        .iter()
        .take(limit)
        .map(|e| {
            let mut v = json!({
                "id": format!("t{}", e.ty), "name": e.name, "group": group(e.ty), "hp": e.hp,
                "stats": format!("STR {} POW {} END {}", e.str_, e.pow, e.end),
            });
            if e.res_m != 128 || e.res_e != 128 {
                v["resist"] = json!(format!("melee x{:.2} energy x{:.2}", e.res_m as f64 / 128.0, e.res_e as f64 / 128.0));
            }
            let ps = presets_using(e.ty);
            if !ps.is_empty() {
                v["presets"] = json!(ps);
            }
            if let Some(a) = default_arena(d, &format!("t{}", e.ty)) {
                v["arena"] = json!(a);
            }
            v
        })
        .collect();
    json!({"total": all.len(), "enemies": list})
}

pub fn arenas() -> Value {
    Value::Array(ARENAS.iter().map(|a| json!({"id": a.0, "name": a.2, "map": a.1})).collect())
}

pub fn presets(d: &GameData) -> Value {
    Value::Array(
        PRESETS
            .iter()
            .map(|p| {
                let phases: Vec<String> = p
                    .phases
                    .iter()
                    .map(|(e, n)| {
                        let names: Vec<String> = e
                            .split('+')
                            .map(|x| {
                                let t: i32 = x.trim_start_matches('~').trim_start_matches('t').parse().unwrap_or(-1);
                                let nm = d.enemy(t).map(|t| t.name.clone()).unwrap_or_else(|| x.to_string());
                                if x.starts_with('~') {
                                    format!("{nm} (helper)")
                                } else {
                                    nm
                                }
                            })
                            .collect();
                        format!("{n}x {} ({e})", names.join(" + "))
                    })
                    .collect();
                let mut v = json!({"id": p.id, "name": p.name, "arena": p.arena, "phases": phases});
                if !p.note.is_empty() {
                    v["note"] = json!(p.note);
                }
                v
            })
            .collect(),
    )
}

/// enemy reference -> id string ("t18", "t26+~t29"); names match exactly first, then by substring (bosses first)
pub fn resolve_enemy(d: &GameData, r: &str) -> Result<String, String> {
    let r = r.trim();
    if r.split('+').all(|p| {
        let p = p.trim_start_matches('~');
        p.starts_with('t') && p[1..].parse::<i32>().is_ok()
    }) {
        return Ok(r.to_string());
    }
    if let Ok(n) = r.parse::<i32>() {
        return Ok(format!("t{n}"));
    }
    let q = norm(r);
    let rank = |ty: i32| match group(ty) {
        "Boss" => 0,
        "Enemy" => 1,
        _ => 2,
    };
    let mut exact: Vec<&wf_core::content::EnemyType> = d.enemies.iter().filter(|e| norm(&e.name) == q).collect();
    exact.sort_by_key(|e| rank(e.ty));
    let mut part: Vec<&wf_core::content::EnemyType> = d.enemies.iter().filter(|e| norm(&e.name).contains(&q)).collect();
    part.sort_by_key(|e| rank(e.ty));
    exact.first().or(part.first()).map(|e| format!("t{}", e.ty)).ok_or_else(|| format!("no enemy matches \"{r}\" (try list_enemies)"))
}

pub fn default_arena(d: &GameData, enemy: &str) -> Option<&'static str> {
    let t = parse_ids(enemy).first()?.0;
    let et = d.enemy(t)?;
    for s in &et.spawns {
        let key = format!("{}-{}", s.0, s.1);
        if key == "20-5" {
            return Some("wasteland");
        }
        if let Some(a) = ARENAS.iter().find(|a| a.1 == key) {
            return Some(a.0);
        }
    }
    None
}

pub fn preset(r: &str) -> Option<&'static Preset> {
    let q = norm(r);
    PRESETS.iter().find(|p| p.id == r).or_else(|| PRESETS.iter().find(|p| norm(p.name).starts_with(&q))).or_else(|| PRESETS.iter().find(|p| norm(p.name).contains(&q)))
}

pub fn preset_phases(p: &Preset) -> Vec<Phase> {
    p.phases.iter().map(|(e, n)| Phase { enemy: e.to_string(), count: *n }).collect()
}

/// What each loaded mod adds or changes (`list_mod_content`): its files grouped by content kind, each with how it
/// applies (added / replaced / patch / derived / deleted / dialogue) and, for JSON records, the merged record's id
/// and name.
pub fn mod_content(set: &crate::content::ModSet) -> Value {
    let packs = &set.packs;
    let base = &packs.packs[0].src;
    let id_keys = ["skill", "type", "item", "index", "key", "id", "sprite_index", "state"];
    let mut mods = Vec::new();
    for p in &packs.packs[1..] {
        let mut files = p.src.list();
        files.sort();
        let mut groups: std::collections::BTreeMap<String, Vec<Value>> = std::collections::BTreeMap::new();
        for f in files {
            if f == "pack.json" || f.ends_with("README.md") {
                continue;
            }
            let (how, target) = if let Some(t) = f.strip_suffix(".patch.json") {
                ("patch", format!("{t}.json"))
            } else if let Some(t) = f.strip_suffix(".delete") {
                ("deleted", t.to_string())
            } else if let Some(t) = f.strip_suffix(".derive.json") {
                ("derived", t.to_string())
            } else if f.ends_with(".text.json") {
                ("dialogue", f.clone())
            } else if base.read(&f).is_some() {
                ("replaced", f.clone())
            } else {
                ("added", f.clone())
            };
            let kind = match target.split('/').next().unwrap_or("") {
                "world" if target.starts_with("world/areas/") => "areas".to_string(),
                "scripts" if target.starts_with("scripts/events/") => "events".to_string(),
                "hooks" => "scripts".to_string(),
                k => k.to_string(),
            };
            let mut e = json!({"file": f, "how": how});
            if target != f {
                e["target"] = json!(target);
            }
            if target.ends_with(".json") && how != "deleted" && how != "dialogue" {
                if let Some(v) = packs.read(&target).and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
                    if let Some(n) = v.get("name").and_then(|x| x.as_str()) {
                        e["name"] = json!(n);
                    }
                    if let Some((k, x)) = id_keys.iter().find_map(|k| v.get(*k).filter(|x| x.is_number() || x.is_string()).map(|x| (*k, x.clone()))) {
                        e["id"] = json!({k: x});
                    }
                }
            }
            groups.entry(kind).or_default().push(e);
        }
        mods.push(json!({"id": p.info.id, "name": p.info.name, "files": groups.values().map(|v| v.len()).sum::<usize>(), "content": groups}));
    }
    json!({"mods": mods, "mods_hash": set.content_id.mods_hash, "folders": set.dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>()})
}
