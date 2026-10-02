//! Game data for the simulation, loaded from a content pack (the extractor's `content/base` tree, plus mods
//! layered by the caller). Everything numeric the sim uses comes from these files; what is ours (curated) is marked.
//!
//! Loaded: `enemies/*.json`, `ai/state_*.json`, `characters/*.json`, `forms/*.json`, `skills/*.json`,
//! `equipment/*.json`, `levels.json`, `sprites/*/sprite.json` (animation steps, body boxes, footprints),
//! `effects/*/effect.json` (record geometry by ROM address, beam sets), `maps/<area>/{collision,map}.json`.

use crate::ai::{compile_behaviour, compile_onhit, BehTree, HitTree};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// where content files come from (a directory, an in-memory pack, a layered mod stack)
pub trait ContentSource {
    /// the bytes of a pack-relative path (`enemies/blister.json`), None if missing
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// every pack-relative path (sorted)
    fn list(&self) -> Vec<String>;
}

/// a `content/base`-style directory
pub struct DirSource {
    pub root: std::path::PathBuf,
}
impl ContentSource for DirSource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(path)).ok()
    }
    fn list(&self) -> Vec<String> {
        if let Some(b) = self.read("files.json") {
            if let Ok(Value::Array(a)) = serde_json::from_slice::<Value>(&b) {
                let mut v: Vec<String> = a.iter().filter_map(|x| x.as_str().map(String::from)).collect();
                v.sort();
                return v;
            }
            if let Ok(Value::Object(o)) = serde_json::from_slice::<Value>(&b) {
                if let Some(Value::Array(a)) = o.get("files") {
                    let mut v: Vec<String> = a.iter().filter_map(|x| x.as_str().map(String::from)).collect();
                    v.sort();
                    return v;
                }
            }
        }
        let mut out = Vec::new();
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(root, &p, out);
                    } else if let Ok(r) = p.strip_prefix(root) {
                        out.push(r.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out
    }
}

/// an in-memory pack (e.g. the extractor's output in the browser); a `base/` prefix on paths is ignored
pub struct MapSource {
    pub files: BTreeMap<String, Vec<u8>>,
}
impl MapSource {
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> MapSource {
        let files = files.into_iter().map(|(k, v)| (k.strip_prefix("base/").map(String::from).unwrap_or(k), v)).collect();
        MapSource { files }
    }
}
impl ContentSource for MapSource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }
    fn list(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AnimStep {
    pub frame: u16,
    pub dur: u32,
    /// ROM frame record (the lockstep compares it)
    pub rom: u32,
    pub flip: bool,
    /// position of the attachment (halo) relative to the entity position
    pub attach: Option<(i32, i32)>,
    /// the frame record's box (record +4..+7: ox, oy, w, h relative to the integer position; 0x0800FE34)
    pub fbox: Option<[i32; 4]>,
}

#[derive(Clone, Debug)]
pub struct Anim {
    pub name: String,
    /// engine animation number + 1 (sprites.json `raw_index`)
    pub raw: i32,
    pub looped: bool,
    /// steps per direction 0..3 (`any` copied in)
    pub dirs: [Vec<AnimStep>; 4],
    /// the lowest raw index whose animation has the same ROM records (the emulator names a struct by its first slot)
    pub canon_raw: i32,
}

impl Anim {
    pub fn steps(&self, dir: u8) -> &[AnimStep] {
        let d = &self.dirs[dir as usize & 3];
        if !d.is_empty() {
            return d;
        }
        self.dirs.iter().find(|d| !d.is_empty()).map(|v| v.as_slice()).unwrap_or(&[])
    }
    pub fn length(&self, dir: u8) -> u32 {
        self.steps(dir).iter().map(|s| s.dur).sum()
    }
    /// index of the step shown `t` frames after the start (loops or clamps on the last step)
    pub fn step_index_at(&self, dir: u8, t: i32, looped: bool) -> Option<usize> {
        let steps = self.steps(dir);
        if steps.is_empty() {
            return None;
        }
        let len = (steps.iter().map(|s| s.dur).sum::<u32>() as i32).max(1);
        let mut k = if looped { t.rem_euclid(len) } else { t.clamp(0, len - 1) };
        for (i, s) in steps.iter().enumerate() {
            if k < s.dur as i32 {
                return Some(i);
            }
            k -= s.dur as i32;
        }
        Some(steps.len() - 1)
    }
    /// nframes of the animation struct (max steps over the directions)
    pub fn nframes(&self) -> i32 {
        self.dirs.iter().map(|d| d.len() as i32).max().unwrap_or(0).max(1)
    }
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub index: i32,
    pub id: String,
    pub anims: Vec<Anim>,
    /// raw index -> first animation with it
    pub by_raw: BTreeMap<i32, u16>,
    pub by_name: BTreeMap<String, u16>,
    /// body (hurt) box per direction [x0, y0, x1, y1] (an empty entry stays empty: unhittable from that side)
    pub body: [[i32; 4]; 4],
    /// collision footprint (w, h)
    pub foot: (i32, i32),
    /// effect id of the ground shadow
    pub shadow: Option<String>,
}

impl Sprite {
    /// first animation that exists among numbers (engine anim n -> raw n + 1) and names; then 'idle'
    pub fn pick(&self, refs: &[AnimRef]) -> Option<u16> {
        for r in refs {
            let a = match r {
                AnimRef::N(n) => self.by_raw.get(&(n + 1)).copied(),
                AnimRef::Name(s) => self.by_name.get(*s).copied(),
            };
            if a.is_some() {
                return a;
            }
        }
        self.by_name.get("idle").copied()
    }
    pub fn anim_raw(&self, raw: i32) -> Option<&Anim> {
        self.by_raw.get(&raw).map(|&i| &self.anims[i as usize])
    }
    /// the frame record box (ox, oy, w, h) of animation `raw`, direction, frame index (clamped); 0x0800FE34
    pub fn frame_box(&self, raw: i32, dir: u8, frame: i32) -> Option<[i32; 4]> {
        let steps = self.anim_raw(raw)?.steps(dir);
        if steps.is_empty() {
            return None;
        }
        steps[(frame.max(0) as usize).min(steps.len() - 1)].fbox
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AnimRef {
    N(i32),
    Name(&'static str),
}

#[derive(Clone, Debug)]
pub struct EnemyType {
    pub ty: i32,
    pub name: String,
    pub hp: i64,
    pub exp: i64,
    pub ai: u16,
    pub str_: i32,
    pub pow: i32,
    pub end: i32,
    pub res_m: i32,
    pub res_e: i32,
    pub gfx: Option<i32>,
    /// sprite index (gfx - 9) when the bank has it
    pub sprite: Option<i32>,
    pub death_name: String,
    pub death_param: i64,
    pub spawns: Vec<(i32, i32, i32, i32)>,
    /// the type's drop script (def +0x18, bytecode address; 0 = none): run when a placed field enemy is defeated
    pub drop_script: u32,
    /// mods: a new type animates like this type (its recorded animation rules), default its own
    pub anim_as: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct Character {
    pub index: i32,
    pub name: String,
    pub level: i32,
    pub hp: i32,
    pub ep: i32,
    pub str_: i32,
    pub pow: i32,
    pub end: i32,
    pub spd: i32,
    pub form: i32,
    pub skills: Vec<i32>,
    pub xp: i64,
    pub growth: [i64; 8],
    pub forms: Vec<i32>,
    /// story mode (mods): equipment slots at a new game (default the game's [0, 44, 77, 112]) and whether the
    /// character starts in the party (`new_game.equip`, `new_game.in_party`)
    pub equip: Option<[i32; 4]>,
    pub in_party: bool,
}

#[derive(Clone, Debug)]
pub struct Form {
    pub id: i32,
    pub name: String,
    pub char_: i32,
    pub flags: i32,
    pub gfx: i32,
    pub up: i32,
    pub down: i32,
    pub b_str: i32,
    pub b_pow: i32,
    pub b_end: i32,
    pub b_end2: i32,
    pub b_walk: i32,
    pub b_dash: i32,
    pub ss: bool,
    pub ss3: bool,
    pub fusion: bool,
    pub controller: u32,
    /// dialogue portraits of this form (form table +0x10..+0x15, `portrait_or_anim_bytes`): text node `arg` 1..6
    /// shows `portraits[arg - 1] - 8` (`portraits/index.json` portrait_index; `0x08012012`)
    pub portraits: [u8; 6],
}

#[derive(Clone, Debug)]
pub struct Skill {
    pub id: i32,
    pub name: String,
    pub max_level: i32,
    pub costs: Vec<i32>,
    pub min_ep: i32,
    /// mod parameters (`skills/<id>.json` `engine`); None for the game's own skills unless a mod patches them
    pub engine: Option<SkillEngine>,
}

/// A skill's simulation parameters a content file may set (`skills/<id>.json` → `"engine": {...}`). Every field
/// is optional: what is not given keeps the built-in value of the behaviour (`base`). Levels index the arrays
/// (the last entry repeats).
#[derive(Clone, Debug, Default)]
pub struct SkillEngine {
    /// the built-in skill whose state machine runs this one (default: the skill's own id)
    pub base: Option<i32>,
    /// EP needed to start, per level
    pub check: Vec<i32>,
    /// EP spent on activation (and per refire for the Energy Blast family), per level
    pub cost: Vec<i32>,
    /// damage multiplier in 1/1024, per level
    pub damage: Vec<i32>,
    /// attack sub-type (the reaction / resist class, 0x0800E9D8's sub)
    pub attack_sub: Option<u8>,
    /// Energy Blast family: frame of the shot (16), frames until a refire / the end (40), shot speed in 1/256 px
    /// per frame (768), shot hit box (w, h) centred on it (16, 16)
    pub fire_frame: Option<i32>,
    pub recovery: Option<i32>,
    pub speed: Option<i32>,
    pub hit_box: Option<(i32, i32)>,
    /// beams: EP drain per frame (1/256 EP), per level
    pub drain: Vec<i32>,
    /// start animation (names tried in order; default: the base skill's). Interned once per content load.
    pub anim: Vec<&'static str>,
    /// a scripted behaviour (modkit.rs): the Rhai file whose hooks (`on_start`, `on_frame`, `on_release`, `on_hit`,
    /// `on_end`) run the skill; the skill's kind is then [`crate::modkit::SCRIPTED`] instead of a built-in one
    pub script: Option<String>,
    /// frames after the end before the skill can start again (scripted skills)
    pub cooldown: Option<i32>,
    /// scripted skills: the start animation's frame rate (frames per cel = 1 << shift, default 3)
    pub anim_shift: Option<i32>,
}

impl SkillEngine {
    pub fn pick(v: &[i32], lv: i32) -> Option<i32> {
        if v.is_empty() {
            return None;
        }
        Some(v[(lv.max(0) as usize).min(v.len() - 1)])
    }
}

fn int_list(v: &Value) -> Vec<i32> {
    match v {
        Value::Array(a) => a.iter().map(|x| int(x) as i32).collect(),
        Value::Null => vec![],
        x => vec![int(x) as i32],
    }
}

fn skill_engine(e: &Value) -> Option<SkillEngine> {
    if !e.is_object() {
        return None;
    }
    let opt = |k: &str| e.get(k).filter(|v| !v.is_null()).map(|v| int(v) as i32);
    Some(SkillEngine {
        base: opt("base"),
        check: int_list(&e["check"]),
        cost: int_list(&e["cost"]),
        damage: int_list(&e["damage"]),
        attack_sub: opt("attack_sub").map(|v| v as u8),
        fire_frame: opt("fire_frame"),
        recovery: opt("recovery"),
        speed: opt("speed"),
        hit_box: e["hit_box"].as_array().filter(|a| a.len() >= 2).map(|a| (int(&a[0]) as i32, int(&a[1]) as i32)),
        drain: int_list(&e["drain"]),
        anim: e["anim"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(|s| &*Box::leak(s.to_string().into_boxed_str()))).collect()).unwrap_or_default(),
        script: e["script"].as_str().map(String::from),
        cooldown: opt("cooldown"),
        anim_shift: opt("anim_shift"),
    })
}

/// A battle fighter choice (`battle/fighters/<id>.json`; the game's own come from [`crate::battle::FIGHTERS`]).
#[derive(Clone, Debug, PartialEq)]
pub struct FighterDef {
    pub id: String,
    pub name: String,
    /// the character record (`characters/*.json` `index`) the stats and growth come from
    pub character: i32,
    /// a fusion: the two characters whose records are fused
    pub fusion: Option<(i32, i32)>,
    /// forms the fighter may take (`forms/*.json` `index`); the first is not special, `default_form` is
    pub forms: Vec<i32>,
    pub default_form: i32,
    /// skill list (None = the character's new-game skills)
    pub skills: Option<Vec<i32>>,
}

/// A battle arena (`battle/arenas/<id>.json`; the game's own come from [`crate::battle::ARENAS`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ArenaDef {
    pub id: String,
    pub name: String,
    /// map key "world-area" (collision from `maps/area_WW_AAA/`)
    pub map: String,
    /// the player's spawn point (px)
    pub spawn: (i32, i32),
    /// fallback bounds when the map has no collision [x0, y0, x1, y1]
    pub bounds: [i32; 4],
}

#[derive(Clone, Debug, Default)]
pub struct Equip {
    pub id: i32,
    pub slot: String,
    pub level: i32,
    pub str_: i32,
    pub pow: i32,
    pub end: i32,
    pub end2: i32,
    pub spd: i32,
    pub xp: i32,
    pub crit_pct: i32,
    pub block_half: bool,
    pub energy_cost_half: bool,
    pub energy_block_half: bool,
    pub ss_cooldown78: bool,
    pub thorns: i32,
    pub hp_regen256: i32,
    pub melee_ep: i32,
    pub melee_hp_instead: bool,
}

/// one map's collision: 256 x 256 tiles of 8 px, 1 = passable (the game's full grids, map object +0xA8 / +0xAC)
#[derive(Clone, Debug)]
pub struct MapCollision {
    pub walk: Vec<u8>,
    pub shot: Vec<u8>,
    /// world size in px (map header; `map.json` properties world_w_px / world_h_px)
    pub world_w: i32,
    pub world_h: i32,
}

pub const GRID: usize = 256;
/// enemy / form graphics id -> sprite index
pub const GFX_OFFSET: i32 = 9;

#[derive(Clone)]
pub struct GameData {
    pub sprites: BTreeMap<i32, Sprite>,
    /// effect / OBJ records by ROM address: (ox, oy, w, h)
    pub records: BTreeMap<u32, [i32; 4]>,
    /// effect ids present (beam sets etc.)
    pub effects: std::collections::BTreeSet<String>,
    /// OBJ record by ROM address -> (effect id, frame index): what the renderer draws for a record address
    pub record_frames: BTreeMap<u32, (String, u16)>,
    /// area key ("8-71") -> (first map id, song)
    pub areas: BTreeMap<String, (String, Option<u16>)>,
    pub enemies: Vec<EnemyType>,
    pub states: BTreeMap<u16, Arc<BehTree>>,
    pub aux: BTreeMap<u16, Arc<HitTree>>,
    pub chars: Vec<Character>,
    pub forms: Vec<Form>,
    pub skills: BTreeMap<i32, Skill>,
    pub equipment: Vec<Equip>,
    pub xp_table: Vec<i64>,
    /// collision per map id "world-map" (only the maps loaded: the arenas, plus `load_map`)
    pub maps: BTreeMap<String, Arc<MapCollision>>,
    /// enemy animation state rules (eanim.rs)
    pub anim: crate::eanim::AnimRules,
    /// the field game's static content (areas, map objects, all maps' collision on demand, supplies); None when the
    /// pack has no `world/` category
    pub field: Option<Arc<crate::fieldcontent::FieldData>>,
    /// battle fighters: the built-in list, then `battle/fighters/*.json` (same id replaces, new ids appended)
    pub fighters: Vec<FighterDef>,
    /// battle arenas: the built-in list, then `battle/arenas/*.json`
    pub arenas: Vec<ArenaDef>,
    /// mod projectile / effect kinds (`projectiles/*.json`, modkit.rs); empty for the base game
    pub projectiles: BTreeMap<String, crate::modkit::ProjDef>,
    /// the script hooks of mod content (skills, projectiles, AI nodes), attached by the frontend after loading
    /// (`wf-modscript`); None = no scripted content
    pub hooks: Option<Arc<dyn crate::modkit::ModHooks>>,
}

#[derive(Debug)]
pub struct LoadError(pub String);
impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn json(src: &dyn ContentSource, path: &str) -> Result<Value, LoadError> {
    let b = src.read(path).ok_or_else(|| LoadError(format!("missing content file {path}")))?;
    serde_json::from_slice(&b).map_err(|e| LoadError(format!("{path}: {e}")))
}

pub(crate) fn int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Value::String(s) => {
            let s = s.trim();
            if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                i64::from_str_radix(h, 16).unwrap_or(0)
            } else {
                s.parse().unwrap_or(0)
            }
        }
        Value::Bool(b) => *b as i64,
        _ => 0,
    }
}
fn geti(v: &Value, k: &str) -> i64 {
    v.get(k).map(int).unwrap_or(0)
}
fn gets(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn files_under(all: &[String], dir: &str, suffix: &str) -> Vec<String> {
    all.iter().filter(|p| p.starts_with(dir) && p.ends_with(suffix)).cloned().collect()
}

/// the arena maps loaded with the data
pub const ARENA_MAPS: [&str; 15] = ["8-71", "15-32", "18-14", "20-7", "8-68", "8-70", "8-40", "6-16", "6-9", "7-6", "8-3", "19-3", "15-1", "5-1", "20-5"];

/// the player animation durations the game's states use (data/battle/buu_anim_timing.json `durations`; our decode
/// of the state code, applied to the sprites the player forms use - the reference does the same). TODO: move into
/// the extractor's `annotations/anim_timing_decoded.json` hook.
const PLAYER_TIMING: &str = include_str!("../data/anim_timing_player.json");
/// per enemy type, the animation state rules recorded in the emulator (data/battle/buu_enemy_anim_sim.json,
/// tools/battle/ENEMY_ANIM.md; our research data like the timing above)
const ENEMY_ANIM: &str = include_str!("../data/enemy_anim_sim.json");

impl GameData {
    pub fn load(src: &dyn ContentSource) -> Result<GameData, LoadError> {
        let all = src.list();
        // ---- characters / forms / skills / equipment / levels
        let mut chars: Vec<Character> = Vec::new();
        for p in files_under(&all, "characters/", ".json") {
            if p.ends_with("index.json") {
                continue;
            }
            let c = json(src, &p)?;
            let ng = &c["new_game"];
            let mut growth = [0i64; 8];
            if let Some(a) = c["growth"]["raw_u32"].as_array() {
                for (i, g) in a.iter().take(8).enumerate() {
                    growth[i] = int(g);
                }
            }
            chars.push(Character {
                index: geti(&c, "index") as i32,
                name: gets(&c, "name"),
                level: geti(ng, "level") as i32,
                hp: geti(ng, "hp") as i32,
                ep: geti(ng, "ep") as i32,
                str_: geti(ng, "STR") as i32,
                pow: geti(ng, "POW") as i32,
                end: geti(ng, "END") as i32,
                spd: geti(ng, "SPD") as i32,
                form: geti(ng, "form") as i32,
                skills: ng["skills"].as_array().map(|a| a.iter().map(|x| int(x) as i32).collect()).unwrap_or_default(),
                xp: geti(ng, "xp"),
                growth,
                forms: c["forms"].as_array().map(|a| a.iter().map(|x| int(x) as i32).collect()).unwrap_or_default(),
                equip: ng["equip"].as_array().filter(|a| a.len() == 4).map(|a| [int(&a[0]) as i32, int(&a[1]) as i32, int(&a[2]) as i32, int(&a[3]) as i32]),
                in_party: ng["in_party"].as_bool().unwrap_or(false),
            });
        }
        chars.sort_by_key(|c| c.index);
        let mut forms: Vec<Form> = Vec::new();
        for p in files_under(&all, "forms/", ".json") {
            if p.ends_with("index.json") {
                continue;
            }
            let f = json(src, &p)?;
            let mb = &f["measured_bonus"];
            let flags = geti(&f, "flags") as i32;
            forms.push(Form {
                id: geti(&f, "index") as i32,
                name: gets(&f, "name"),
                char_: geti(&f, "char_index") as i32,
                flags,
                gfx: geti(&f, "gfx_id") as i32,
                up: geti(&f, "transform_up") as i32,
                down: geti(&f, "transform_down") as i32,
                b_str: geti(mb, "STR") as i32,
                b_pow: geti(mb, "POW") as i32,
                b_end: geti(mb, "END") as i32,
                b_end2: geti(mb, "END_energy") as i32,
                b_walk: geti(mb, "walk_speed") as i32,
                b_dash: geti(mb, "dash_speed") as i32,
                ss: flags & 1 != 0,
                ss3: flags & 2 != 0,
                fusion: flags & 4 != 0,
                controller: int(&f["controller_slot"]) as u32,
                portraits: {
                    let mut p = [0u8; 6];
                    for (k, v) in f["portrait_or_anim_bytes"].as_array().into_iter().flatten().take(6).enumerate() {
                        p[k] = int(v) as u8;
                    }
                    p
                },
            });
        }
        forms.sort_by_key(|f| f.id);
        let mut skills = BTreeMap::new();
        for p in files_under(&all, "skills/", ".json") {
            if p.ends_with("index.json") {
                continue;
            }
            let s = json(src, &p)?;
            let ep = &s["ep"];
            let costs: Vec<i32> = if let Some(a) = ep["ep_cost_by_level"].as_array() {
                a.iter().map(|x| int(x) as i32).collect()
            } else {
                let v = ep.get("canuse_min_ep").filter(|v| !v.is_null()).or_else(|| ep.get("spend").filter(|v| !v.is_null())).map(int).unwrap_or(0);
                vec![v as i32]
            };
            let min_ep = ep.get("canuse_min_ep").filter(|v| !v.is_null()).map(|v| int(v) as i32).unwrap_or(costs.first().copied().unwrap_or(0));
            let id = geti(&s, "skill") as i32;
            skills.insert(id, Skill { id, name: gets(&s, "name"), max_level: geti(&s, "max_level") as i32, costs, min_ep, engine: skill_engine(&s["engine"]) });
        }
        let mut equipment = Vec::new();
        for p in files_under(&all, "equipment/", ".json") {
            if p.ends_with("index.json") {
                continue;
            }
            let e = json(src, &p)?;
            if e.get("item").is_none() {
                continue;
            }
            // effect scripts flattened (the reference derives the same flags from the script summary + text)
            let mut eff = serde_json::Map::new();
            if let Some(a) = e["script_effects"].as_array() {
                for x in a {
                    if let Some(o) = x.as_object() {
                        for (k, v) in o {
                            eff.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            let truthy = |k: &str| match eff.get(k) {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(Value::Number(n)) => n.as_f64() != Some(0.0),
                Some(Value::String(s)) => !s.is_empty(),
                Some(_) => true,
            };
            let other = gets(&e, "other_effects");
            equipment.push(Equip {
                id: geti(&e, "item") as i32,
                slot: gets(&e, "slot"),
                level: geti(&e, "required_level") as i32,
                str_: geti(&e, "STR") as i32,
                pow: geti(&e, "POW") as i32,
                end: geti(&e, "END") as i32,
                end2: geti(&e, "END_energy") as i32,
                spd: geti(&e, "SPD") as i32,
                xp: geti(&e, "exp_bonus_256ths") as i32,
                crit_pct: eff.get("crit_chance_percent").map(int).unwrap_or(0) as i32,
                block_half: truthy("block_damage_factor"),
                energy_cost_half: truthy("energy_attack_cost"),
                energy_block_half: truthy("energy_block_cost"),
                ss_cooldown78: truthy("ss_cooldown_rate"),
                thorns: eff.get("thorns_damage").map(int).unwrap_or(0) as i32,
                hp_regen256: eff.get("hp_regen_per_frame_256ths").map(int).unwrap_or(0) as i32,
                melee_ep: if other.contains("EP +2 instead") {
                    2
                } else if other.contains("extra EP +1") {
                    1
                } else {
                    0
                },
                melee_hp_instead: other.contains("HP +1 instead"),
            });
        }
        equipment.sort_by_key(|e| e.id);
        let levels = json(src, "levels.json")?;
        let xp_table: Vec<i64> = levels["levels"].as_array().map(|a| a.iter().map(|l| geti(l, "total_xp")).collect()).unwrap_or_default();

        // ---- sprites (+ the player-state timing on the sprites the forms use)
        let timing: Value = serde_json::from_str(PLAYER_TIMING).unwrap_or(Value::Null);
        let form_sprites: std::collections::BTreeSet<i32> = forms.iter().map(|f| f.gfx - 9).collect();
        let mut sprites = BTreeMap::new();
        for p in files_under(&all, "sprites/", "/sprite.json") {
            let s = json(src, &p)?;
            let index = geti(&s, "sprite_index") as i32;
            let spr = parse_sprite(&s, index, if form_sprites.contains(&index) { Some(&timing["durations"]) } else { None });
            sprites.insert(index, spr);
        }
        // ---- effects: record geometry by ROM address
        let mut records = BTreeMap::new();
        let mut record_frames = BTreeMap::new();
        let mut effects = std::collections::BTreeSet::new();
        for p in files_under(&all, "effects/", "/effect.json") {
            let s = json(src, &p)?;
            let eid = gets(&s, "id");
            effects.insert(eid.clone());
            if let Some(anims) = s["animations"].as_object() {
                for a in anims.values() {
                    if let Some(dirs) = a["directions"].as_object() {
                        for steps in dirs.values() {
                            for st in steps.as_array().into_iter().flatten() {
                                if let Some(obj) = st["obj"].as_array() {
                                    let rom = int(&st["rom"]) as u32;
                                    record_frames.entry(rom).or_insert((eid.clone(), geti(st, "frame") as u16));
                                    if rom != 0 && obj.len() >= 4 {
                                        records.entry(rom).or_insert([int(&obj[0]) as i32, int(&obj[1]) as i32, int(&obj[2]) as i32, int(&obj[3]) as i32]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // ---- enemies (gfx resolution as the reference data loader)
        let mut raw_en: Vec<Value> = Vec::new();
        for p in files_under(&all, "enemies/", ".json") {
            if p.ends_with("index.json") {
                continue;
            }
            let e = json(src, &p)?;
            if e.get("type").is_some() {
                raw_en.push(e);
            }
        }
        raw_en.sort_by_key(|e| geti(e, "type"));
        let mut enemies: Vec<EnemyType> = raw_en
            .iter()
            .map(|e| {
                let ty = geti(e, "type") as i32;
                let name = e.get("display_name").and_then(|v| v.as_str()).or_else(|| e.get("name").and_then(|v| v.as_str())).map(String::from).unwrap_or_else(|| format!("Type 0x{ty:X}"));
                let death = &e["death"];
                let (dn, dp) = if death.is_object() { (gets(death, "name"), geti(death, "param")) } else { ("SelfDestruct".to_string(), 8) };
                EnemyType {
                    ty,
                    name,
                    hp: geti(e, "hp"),
                    exp: geti(e, "exp"),
                    ai: geti(e, "ai_state") as u16,
                    str_: geti(e, "STR") as i32,
                    pow: geti(e, "POW") as i32,
                    end: geti(e, "END") as i32,
                    res_m: geti(e, "resist_melee") as i32,
                    res_e: geti(e, "resist_energy") as i32,
                    gfx: e["gfx_ids"].as_array().and_then(|a| a.first()).map(|v| int(v) as i32),
                    sprite: None,
                    death_name: dn,
                    death_param: dp,
                    spawns: e["spawn_points"].as_array().map(|a| a.iter().map(|s| (geti(s, "world") as i32, geti(s, "map") as i32, geti(s, "x") as i32, geti(s, "y") as i32)).collect()).unwrap_or_default(),
                    drop_script: e["drop_script"].as_str().and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0),
                    anim_as: e.get("anim_as").filter(|v| !v.is_null()).map(|v| int(v) as i32),
                }
            })
            .collect();
        resolve_enemy_gfx(&mut enemies, &sprites);

        // ---- AI
        let mut states = BTreeMap::new();
        let mut aux_json: BTreeMap<u16, Value> = BTreeMap::new();
        let mut beh_json: BTreeMap<u16, Value> = BTreeMap::new();
        for p in files_under(&all, "ai/state_", ".json") {
            let d = json(src, &p)?;
            let id = geti(&d, "state") as u16;
            if let Some(t) = d.get("tree").filter(|t| t.is_object()) {
                beh_json.insert(id, t.clone());
            } else if let Some(t) = d.get("reaction").filter(|t| t.is_object()) {
                beh_json.insert(id, t.clone());
            }
            if let Some(t) = d.get("aux").filter(|t| t.is_object()) {
                aux_json.insert(id, t.clone());
            }
        }
        for &id in beh_json.keys() {
            states.insert(id, Arc::new(compile_behaviour(id, &beh_json)));
        }
        let aux = aux_json.iter().map(|(&k, v)| (k, Arc::new(compile_onhit(v)))).collect();

        let mut areas = BTreeMap::new();
        if let Ok(a) = json(src, "areas.json") {
            for x in a["areas"].as_array().into_iter().flatten() {
                let key = gets(x, "key");
                let map = x["maps"].get(0).and_then(|m| m.as_str()).unwrap_or("").to_string();
                areas.insert(key, (map, x["song"].as_u64().map(|v| v as u16)));
            }
        }
        let (fighters, arenas) = battle_defs(src, &all)?;
        let mut d = GameData { sprites, records, record_frames, areas, effects, enemies, states, aux, chars, forms, skills, equipment, xp_table, maps: BTreeMap::new(), anim: crate::eanim::AnimRules::load(ENEMY_ANIM), field: crate::fieldcontent::FieldData::load(src, &all)?.map(Arc::new), fighters, arenas, projectiles: crate::modkit::load_projectiles(src, &all)?, hooks: None };
        // new enemy types animate like the type they name (`anim_as`)
        let borrow: Vec<(i32, i32)> = d.enemies.iter().filter_map(|e| e.anim_as.map(|b| (e.ty, b))).collect();
        for (ty, base) in borrow {
            if let Some(m) = d.anim.types.get(&base).cloned() {
                d.anim.types.insert(ty, m);
            }
        }
        for m in ARENA_MAPS {
            d.load_map(src, m)?;
        }
        // arenas added by content (their maps' collision)
        let extra: Vec<String> = d.arenas.iter().map(|a| a.map.clone()).filter(|m| !d.maps.contains_key(m)).collect();
        for m in extra {
            d.load_map(src, &m)?;
        }
        Ok(d)
    }

    pub fn fighter(&self, id: &str) -> Option<&FighterDef> {
        self.fighters.iter().find(|f| f.id == id)
    }
    pub fn arena(&self, id: &str) -> Option<&ArenaDef> {
        self.arenas.iter().find(|a| a.id == id)
    }
    /// the skill state machine a skill runs (`engine.base`, default its own id)
    pub fn skill_kind(&self, id: i32) -> i32 {
        let e = self.skills.get(&id).and_then(|s| s.engine.as_ref());
        if e.is_some_and(|e| e.script.is_some()) {
            return crate::modkit::SCRIPTED;
        }
        e.and_then(|e| e.base).unwrap_or(id)
    }
    pub fn skill_engine(&self, id: i32) -> Option<&SkillEngine> {
        self.skills.get(&id).and_then(|s| s.engine.as_ref())
    }

    /// load one map's collision ("world-map", e.g. "8-71", or a map directory id such as "mod_cave")
    pub fn load_map(&mut self, src: &dyn ContentSource, map: &str) -> Result<(), LoadError> {
        let dir = match map.split_once('-').and_then(|(w, m)| Some((w.parse::<i32>().ok()?, m.parse::<i32>().ok()?))) {
            Some((w, m)) => format!("maps/area_{w:02}_{m:03}"),
            None => format!("maps/{map}"),
        };
        let c = rows_collision(json(src, &format!("{dir}/collision.json"))?);
        let cw = geti(&c, "width").max(1) as usize;
        let ch = geti(&c, "height").max(1) as usize;
        let walk = unrle_grid(&gets(&c, "walk"), cw, ch);
        let shot = unrle_grid(&gets(&c, "shot"), cw, ch);
        let (mut ww, mut wh) = (0, 0);
        if let Ok(mj) = json(src, &format!("{dir}/map.json")) {
            for p in mj["properties"].as_array().into_iter().flatten() {
                match p["name"].as_str() {
                    Some("world_w_px") => ww = int(&p["value"]) as i32,
                    Some("world_h_px") => wh = int(&p["value"]) as i32,
                    _ => {}
                }
            }
            if ww == 0 {
                ww = (geti(&mj, "width") * geti(&mj, "tilewidth").max(8)) as i32;
                wh = (geti(&mj, "height") * geti(&mj, "tileheight").max(8)) as i32;
            }
        }
        self.maps.insert(map.to_string(), Arc::new(MapCollision { walk, shot, world_w: ww, world_h: wh }));
        Ok(())
    }

    pub fn sprite(&self, index: i32) -> Option<&Sprite> {
        self.sprites.get(&index)
    }
    pub fn form(&self, id: i32) -> Option<&Form> {
        self.forms.iter().find(|f| f.id == id)
    }
    pub fn enemy(&self, ty: i32) -> Option<&EnemyType> {
        if ty < 0 {
            return None;
        }
        // the game's types are a dense table; types added by mods may leave gaps
        self.enemies.get(ty as usize).filter(|e| e.ty == ty).or_else(|| self.enemies.iter().find(|e| e.ty == ty))
    }
    /// effect / OBJ record geometry by ROM address
    pub fn record(&self, addr: u32) -> Option<[i32; 4]> {
        self.records.get(&addr).copied()
    }
}

fn parse_sprite(s: &Value, index: i32, timing: Option<&Value>) -> Sprite {
    const DIRS: [&str; 4] = ["down", "up", "left", "right"];
    let mut anims = Vec::new();
    if let Some(o) = s["animations"].as_object() {
        for (name, a) in o {
            let raw = geti(a, "raw_index") as i32;
            let mut looped = a["loop"].as_bool().unwrap_or(false);
            let any = a["directions"].get("any");
            let td = timing.and_then(|t| t.get(raw.to_string())).filter(|v| v.is_object());
            if let Some(td) = td {
                if let Some(l) = td["loop"].as_bool() {
                    looped = l;
                }
            }
            let dirs: [Vec<AnimStep>; 4] = std::array::from_fn(|i| {
                let steps = a["directions"].get(DIRS[i]).filter(|v| !v.is_null()).or(any).and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let n = steps.len();
                steps
                    .iter()
                    .enumerate()
                    .map(|(k, st)| {
                        let mut dur = geti(st, "duration_frames");
                        if let Some(td) = td {
                            let two = td["two_frame"].as_array().filter(|_| n == 2);
                            if let Some(two) = two {
                                dur = int(&two[k]);
                            } else if let Some(e) = td.get("each").filter(|v| !v.is_null()) {
                                dur = int(e);
                            } else if let Some(e) = td.get("other").filter(|v| !v.is_null()) {
                                dur = int(e);
                            }
                        }
                        AnimStep { frame: geti(st, "frame") as u16, dur: dur.max(1) as u32, rom: int(&st["rom"]) as u32, flip: st["flip"].as_bool().unwrap_or(false), attach: st["attach"].as_array().filter(|a| a.len() >= 2).map(|a| (int(&a[0]) as i32, int(&a[1]) as i32)), fbox: st["box"].as_array().filter(|a| a.len() >= 4).map(|a| [int(&a[0]) as i32, int(&a[1]) as i32, int(&a[2]) as i32, int(&a[3]) as i32]) }
                    })
                    .collect()
            });
            anims.push(Anim { name: name.clone(), raw, looped, dirs, canon_raw: raw });
        }
    }
    // canonical raw: the lowest raw index with the same ROM records in every direction listed
    let keys: Vec<Vec<Vec<u32>>> = s["animations"]
        .as_object()
        .map(|o| o.values().map(|a| a["directions"].as_object().map(|d| d.values().map(|v| v.as_array().into_iter().flatten().map(|q| int(&q["rom"]) as u32).collect()).collect()).unwrap_or_default()).collect())
        .unwrap_or_default();
    for i in 0..anims.len() {
        let mut best = anims[i].raw;
        for j in 0..anims.len() {
            if anims[j].raw < best && keys[j] == keys[i] {
                best = anims[j].raw;
            }
        }
        anims[i].canon_raw = best;
    }
    let mut by_raw = BTreeMap::new();
    let mut by_name = BTreeMap::new();
    for (i, a) in anims.iter().enumerate() {
        by_raw.entry(a.raw).or_insert(i as u16);
        by_name.entry(a.name.clone()).or_insert(i as u16);
    }
    let mut body = [[0, -22, 16, 8]; 4];
    if let Some(bb) = s["body_box"].as_object() {
        for (i, d) in DIRS.iter().enumerate() {
            let b = bb.get(*d).or_else(|| bb.values().next());
            if let Some(a) = b.and_then(|v| v.as_array()) {
                if a.len() >= 4 {
                    let r = [int(&a[0]) as i32, int(&a[1]) as i32, int(&a[2]) as i32, int(&a[3]) as i32];
                    // an empty rect stays empty (0x0800FE78: nothing hits that facing, e.g. the Enzyme's body exists
                    // only facing down); only a sprite without any box keeps the default
                    body[i] = r;
                }
            }
        }
    }
    let mut foot = (16, 8);
    if let Some(f) = s["footprint"].as_array() {
        if f.len() >= 2 {
            let w = int(&f[0]) as i32;
            let h = int(&f[1]) as i32;
            foot = (if w != 0 { w } else { 16 }, if h != 0 { h } else { 8 });
        }
    }
    let shadow = s["shadow"].as_str().map(String::from);
    Sprite { index, id: gets(s, "id"), anims, by_raw, by_name, body, foot, shadow }
}

/// the reference loader's graphics fallbacks (data.ts): Janemba body parts from their spawn records, placeholders
/// for types without graphics (curated), same-name / previous-type borrowing
fn resolve_enemy_gfx(en: &mut [EnemyType], sprites: &BTreeMap<i32, Sprite>) {
    let part_gfx = |t: i32| match t {
        0x34 => Some(0x93),
        0x35 => Some(0x94),
        0x37 => Some(0x98),
        _ => None,
    };
    let placeholder = |t: i32| match t {
        0x4a => Some(72),
        0x71 => Some(31),
        0x89 => Some(282),
        0x87 => Some(36),
        0x1e | 0x22 | 0x72 | 0x7e => Some(173),
        _ => None,
    };
    let strip = |s: &str| match s.find(" (variant)") {
        Some(i) => s[..i].to_string(),
        None => s.to_string(),
    };
    for i in 0..en.len() {
        let ty = en[i].ty;
        if let Some(g) = part_gfx(ty) {
            en[i].gfx = Some(g);
        }
        if matches!(en[i].gfx, Some(g) if g < 9) {
            en[i].gfx = None;
        }
        if en[i].gfx.is_none() {
            if let Some(p) = placeholder(ty) {
                en[i].gfx = Some(p);
            }
        }
        if en[i].gfx.is_none() {
            let me = strip(&en[i].name);
            let same = en.iter().find(|o| o.gfx.is_some() && strip(&o.name) == me).and_then(|o| o.gfx);
            let name = en[i].name.clone();
            let prev = if ty >= 1 { en.get((ty - 1) as usize).filter(|p| p.gfx.is_some()).and_then(|p| p.gfx) } else { None };
            let borrow = ["variant", "Kid Buu", "Broly", "Super Buu", "Worm"].iter().any(|k| name.contains(k));
            en[i].gfx = same.or(if borrow { prev } else { None });
            if en[i].gfx.is_none() && (ty == 0x8f || ty == 0x90) {
                en[i].gfx = en.get(0x8e).and_then(|e| e.gfx);
            }
        }
        if let Some(g) = en[i].gfx {
            if sprites.contains_key(&(g - 9)) {
                en[i].sprite = Some(g - 9);
            }
        }
    }
}

/// the built-in fighters / arenas, then the content's `battle/fighters/*.json` / `battle/arenas/*.json`
fn battle_defs(src: &dyn ContentSource, all: &[String]) -> Result<(Vec<FighterDef>, Vec<ArenaDef>), LoadError> {
    let mut fighters: Vec<FighterDef> = crate::battle::FIGHTERS
        .iter()
        .map(|f| FighterDef { id: f.0.to_string(), name: f.0.to_string(), character: f.1, fusion: f.2, forms: f.3.to_vec(), default_form: f.4, skills: if f.1 == 8 { Some(vec![7]) } else { None } })
        .collect();
    let mut arenas: Vec<ArenaDef> =
        crate::battle::ARENAS.iter().map(|a| ArenaDef { id: a.0.to_string(), name: a.2.to_string(), map: a.1.to_string(), spawn: a.3, bounds: a.4 }).collect();
    for p in files_under(all, "battle/fighters/", ".json") {
        if p.ends_with("index.json") {
            continue;
        }
        let v = json(src, &p)?;
        let id = v["id"].as_str().map(String::from).unwrap_or_else(|| p["battle/fighters/".len()..p.len() - 5].to_string());
        let old = fighters.iter().position(|f| f.id == id);
        let prev = old.map(|k| fighters[k].clone());
        let forms = v.get("forms").map(int_list).or_else(|| prev.as_ref().map(|f| f.forms.clone())).unwrap_or_default();
        let f = FighterDef {
            name: v["name"].as_str().map(String::from).or_else(|| prev.as_ref().map(|f| f.name.clone())).unwrap_or_else(|| id.clone()),
            character: v.get("character").map(|x| int(x) as i32).or(prev.as_ref().map(|f| f.character)).unwrap_or(0),
            fusion: v["fusion"].as_array().filter(|a| a.len() == 2).map(|a| (int(&a[0]) as i32, int(&a[1]) as i32)).or(prev.as_ref().and_then(|f| f.fusion)),
            default_form: v.get("default_form").map(|x| int(x) as i32).or(prev.as_ref().map(|f| f.default_form)).or(forms.first().copied()).unwrap_or(0),
            skills: v.get("skills").filter(|x| !x.is_null()).map(int_list).or(prev.as_ref().and_then(|f| f.skills.clone())),
            forms,
            id,
        };
        if f.forms.is_empty() {
            return Err(LoadError(format!("{p}: a fighter needs \"forms\"")));
        }
        match old {
            Some(k) => fighters[k] = f,
            None => fighters.push(f),
        }
    }
    for p in files_under(all, "battle/arenas/", ".json") {
        if p.ends_with("index.json") {
            continue;
        }
        let v = json(src, &p)?;
        let id = v["id"].as_str().map(String::from).unwrap_or_else(|| p["battle/arenas/".len()..p.len() - 5].to_string());
        let sp = &v["spawn"];
        let b = int_list(&v["bounds"]);
        let a = ArenaDef {
            name: v["name"].as_str().unwrap_or(&id).to_string(),
            map: v["map"].as_str().ok_or_else(|| LoadError(format!("{p}: an arena needs \"map\"")))?.to_string(),
            spawn: (int(&sp[0]) as i32, int(&sp[1]) as i32),
            bounds: if b.len() == 4 { [b[0], b[1], b[2], b[3]] } else { [0, 0, 2048, 2048] },
            id,
        };
        match arenas.iter().position(|x| x.id == a.id) {
            Some(k) => arenas[k] = a,
            None => arenas.push(a),
        }
    }
    Ok((fighters, arenas))
}

/// `collision.json` in the mod-friendly row form (`"rows": ["##..", ...]`: '.' open, '#' wall, '~' blocks
/// walking but not shots) converted to the extractor's run-length form; other files are returned unchanged
pub(crate) fn rows_collision(c: Value) -> Value {
    let Some(rows) = c["rows"].as_array() else { return c };
    let rows: Vec<&str> = rows.iter().filter_map(|r| r.as_str()).collect();
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let h = rows.len().max(1);
    let grid = |open: &dyn Fn(u8) -> bool| -> String {
        let mut flat = vec![0u8; w * h];
        for (y, r) in rows.iter().enumerate() {
            for (x, ch) in r.bytes().enumerate() {
                flat[y * w + x] = open(ch) as u8;
            }
        }
        rle_b64(&flat)
    };
    serde_json::json!({ "width": w, "height": h, "walk": grid(&|ch| ch == b'.'), "shot": grid(&|ch| ch == b'.' || ch == b'~') })
}

/// the extractor's grid encoding: run lengths (first run blocked) as LEB128, base64
fn rle_b64(flat: &[u8]) -> String {
    let mut bytes = Vec::new();
    let (mut cur, mut i) = (0u8, 0usize);
    while i < flat.len() {
        let mut n = 0usize;
        while i < flat.len() && flat[i] == cur {
            n += 1;
            i += 1;
        }
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                bytes.push(b);
                break;
            }
            bytes.push(b | 0x80);
        }
        cur ^= 1;
    }
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            s.push(T[(v >> (18 - 6 * k)) as usize & 63] as char);
        }
    }
    s
}

fn b64(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

/// run lengths (LEB128) over w*h tiles, first run blocked -> a 256 x 256 grid (outside w x h blocked)
pub(crate) fn unrle_grid(s: &str, w: usize, h: usize) -> Vec<u8> {
    let b = b64(s);
    let n = w * h;
    let mut flat = vec![0u8; n];
    let (mut i, mut p, mut cur) = (0usize, 0usize, 0u8);
    while i < b.len() {
        let mut v = 0usize;
        let mut sh = 0;
        loop {
            let c = b[i];
            i += 1;
            v += ((c & 0x7f) as usize) << sh;
            sh += 7;
            if c & 0x80 == 0 || i >= b.len() {
                break;
            }
        }
        if cur == 1 {
            for x in flat.iter_mut().take(n.min(p + v)).skip(p) {
                *x = 1;
            }
        }
        p += v;
        cur ^= 1;
    }
    let mut g = vec![0u8; GRID * GRID];
    for y in 0..h.min(GRID) {
        for x in 0..w.min(GRID) {
            g[y * GRID + x] = flat[y * w + x];
        }
    }
    g
}
