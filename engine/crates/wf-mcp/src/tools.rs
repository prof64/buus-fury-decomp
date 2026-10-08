//! The transport-free tool API: every MCP tool is one `Server::call(name, args)`; the MCP binary, the C ABI and
//! tests all go through here.

use crate::catalog;
use crate::controller::Controller;
use crate::env::{outcome_str, Agent, Replay};
use crate::macros::{self as m, parse_buttons, run_policy, Policy, SkillOpts};
use crate::obs::{ascii_map, observe, ObsOpts};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit, FIGHTERS};
use wf_core::content::GameData;
use crate::content::ModSet;
use std::path::PathBuf;

/// `mods`: an array of mod folder paths (`[]` = explicitly none); absent = None
fn mods_arg(a: &Value) -> Result<Option<Vec<PathBuf>>, String> {
    match a.get("mods") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(v)) => v.iter().map(|x| x.as_str().map(PathBuf::from).ok_or_else(|| "mods: an array of mod folder paths".to_string())).collect::<Result<_, _>>().map(Some),
        Some(Value::String(s)) => Ok(Some(std::env::split_paths(s).filter(|p| !p.as_os_str().is_empty()).collect())),
        _ => Err("mods: an array of mod folder paths".into()),
    }
}

/// The content of a call (tools and the C ABI's gym): `mods` (mod folder paths; `[]` = none) or else `WF_MODS`, else
/// the base pack. The game data and the mod set (None = no mods).
pub fn content_of(a: &Value) -> Result<(Arc<GameData>, Option<Arc<ModSet>>), String> {
    let dirs = mods_arg(a)?.unwrap_or_else(crate::content::default_mods);
    match crate::content::mod_set(&dirs)? {
        Some(m) => Ok((Arc::clone(&m.data), Some(m))),
        None => Ok((crate::content::game_data()?, None)),
    }
}

/// the mod list recorded for messages: ids, or "none"
fn mod_list(m: &[String]) -> String {
    if m.is_empty() {
        "none".into()
    } else {
        m.join(", ")
    }
}

pub enum Out {
    Json(Value),
    /// PNG bytes + a JSON caption
    Image(Vec<u8>, Value),
}

pub struct Server {
    pub sessions: BTreeMap<String, Agent>,
    next: u32,
    renderer: Option<wf_render::Renderer>,
    /// the current call is a macro that should hand control back when a story event takes the field (not press /
    /// wait: they drive dialogue and menus)
    event_stops: bool,
}

impl Default for Server {
    fn default() -> Self {
        Server::new()
    }
}

// ---- argument helpers ------------------------------------------------------------------------------------------------

fn s<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(|v| v.as_str())
}
fn i(a: &Value, k: &str) -> Option<i64> {
    a.get(k).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).or_else(|| v.as_str().and_then(|s| s.parse().ok())))
}
fn b(a: &Value, k: &str) -> Option<bool> {
    a.get(k).and_then(|v| v.as_bool())
}
fn frames(a: &Value, k: &str, d: i64, max: i64) -> u32 {
    i(a, k).unwrap_or(d).clamp(0, max) as u32
}
fn stop_on(a: &Value, d: &[&str]) -> Result<Vec<String>, String> {
    match a.get("stop_on") {
        None | Some(Value::Null) => Ok(d.iter().map(|x| x.to_string()).collect()),
        Some(Value::String(x)) => stop_on(&json!({"stop_on": x.split([',', '+', ' ']).filter(|s| !s.is_empty()).collect::<Vec<_>>()}), d),
        Some(Value::Array(v)) => v
            .iter()
            .map(|x| {
                let x = x.as_str().unwrap_or("");
                if m::STOP_ON.contains(&x) {
                    Ok(x.to_string())
                } else {
                    Err(format!("stop_on: unknown condition \"{x}\" (use {})", m::STOP_ON.join(", ")))
                }
            })
            .collect(),
        _ => Err("stop_on: a list like [\"hurt\", \"kill\"]".into()),
    }
}

fn png_rgb(px: &[u32], w: usize, h: usize, scale: usize) -> Vec<u8> {
    let s = scale.max(1);
    let mut rgb = Vec::with_capacity(w * h * 3 * s * s);
    for y in 0..h * s {
        for x in 0..w * s {
            let p = px[(y / s) * w + x / s];
            rgb.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, (w * s) as u32, (h * s) as u32);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&rgb).unwrap();
    }
    out
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}

/// fighter setup from {fighter, form, level, skill_level, stat_spend, growth, equip}
fn fighter_setup(d: &GameData, a: &Value, base: Option<&FighterSetup>) -> Result<FighterSetup, String> {
    let mut f = base.cloned().unwrap_or(FighterSetup { level: 80, skill_level: 1, auto_spend: "even".into(), ..Default::default() });
    if let Some(name) = s(a, "fighter") {
        let q = catalog::norm(name);
        let fd = FIGHTERS
            .iter()
            .find(|x| x.0 == name || x.0 == q)
            .or_else(|| FIGHTERS.iter().find(|x| d.chars.iter().any(|c| c.index == x.1 && catalog::norm(&c.name) == q && !x.0.starts_with('f'))))
            .ok_or_else(|| format!("unknown fighter \"{name}\" (have: {})", FIGHTERS.iter().filter(|x| !x.0.starts_with('f')).map(|x| x.0).collect::<Vec<_>>().join(", ")))?;
        if f.fighter != fd.0 {
            f.form = fd.4;
        }
        f.fighter = fd.0.to_string();
    } else if base.is_none() {
        f.form = 2;
    }
    if let Some(v) = i(a, "form") {
        let fd = FIGHTERS.iter().find(|x| x.0 == f.fighter).unwrap();
        if !fd.3.contains(&(v as i32)) {
            return Err(format!("form {v} is not one of {}'s forms {:?} (list_fighters)", f.fighter, fd.3));
        }
        f.form = v as i32;
    }
    if let Some(v) = i(a, "level") {
        f.level = v.clamp(50, 200) as i32;
    }
    if let Some(v) = i(a, "skill_level") {
        f.skill_level = v.clamp(0, 2) as i32;
    }
    if let Some(v) = s(a, "stat_spend") {
        f.auto_spend = v.to_string();
    }
    if let Some(v) = s(a, "growth") {
        f.growth = v.to_string();
    }
    if let Some(e) = a.get("equip").and_then(|v| v.as_array()) {
        for (k, x) in e.iter().enumerate().take(4) {
            f.equip[k] = x.as_i64().unwrap_or(f.equip[k] as i64) as i32;
        }
    }
    if let Some(h) = b(a, "holy_water") {
        f.holy_water = h;
    }
    Ok(f)
}

/// new_battle arguments -> (config, label)
pub fn battle_config(d: &GameData, a: &Value) -> Result<(BattleConfig, String), String> {
    let mut players = Vec::new();
    let p0 = fighter_setup(d, a, None)?;
    match a.get("players") {
        Some(Value::Array(list)) => {
            for (k, x) in list.iter().enumerate().take(4) {
                let mut f = fighter_setup(d, x, Some(&p0))?;
                f.growth_seed = k as u32 * 2;
                players.push(f);
            }
        }
        Some(v) if v.is_number() => {
            for k in 0..(v.as_i64().unwrap_or(1).clamp(1, 4)) {
                let mut f = p0.clone();
                f.growth_seed = k as u32 * 2;
                players.push(f);
            }
        }
        _ => players.push(p0),
    }
    let (phases, mut arena): (Vec<Phase>, Option<String>) = if let Some(p) = s(a, "preset") {
        let pr = catalog::preset(p).ok_or_else(|| format!("unknown preset \"{p}\" (list_presets)"))?;
        (catalog::preset_phases(pr), Some(pr.arena.to_string()))
    } else {
        let e = catalog::resolve_enemy(d, s(a, "enemy").unwrap_or("t68"))?;
        for (t, _) in catalog::parse_ids(&e) {
            if d.enemy(t).is_none() {
                return Err(format!("no enemy t{t} (list_enemies)"));
            }
        }
        let arena = catalog::default_arena(d, &e).map(String::from);
        (vec![Phase { enemy: e, count: i(a, "count").unwrap_or(1).clamp(1, 12) as i32 }], arena)
    };
    if let Some(x) = s(a, "arena") {
        if !wf_core::battle::ARENAS.iter().any(|r| r.0 == x) {
            return Err(format!("unknown arena \"{x}\" (list_arenas)"));
        }
        arena = Some(x.to_string());
    }
    let seed = i(a, "seed").unwrap_or(1) as u32;
    let cfg = BattleConfig { arena: arena.unwrap_or_else(|| "crater".into()), players, phases, rng: RngInit::Seed(seed), xp: b(a, "xp").unwrap_or(false), spawn_near: None };
    let foe: Vec<String> = cfg
        .phases
        .iter()
        .map(|p| {
            let n: Vec<String> = catalog::parse_ids(&p.enemy).iter().map(|(t, _)| d.enemy(*t).map(|e| e.name.clone()).unwrap_or_default()).collect();
            format!("{}{}", if p.count > 1 { format!("{}x ", p.count) } else { String::new() }, n.join("+"))
        })
        .collect();
    let me: Vec<String> = cfg.players.iter().map(|f| format!("{} Lv{}", f.fighter, f.level)).collect();
    let label = format!("{} vs {} @ {}", me.join(" & "), foe.join(" -> "), cfg.arena);
    Ok((cfg, label))
}

pub fn tool_list() -> Value {
    let sess = json!({"type": "string", "description": "session id from new_battle"});
    let slot = json!({"type": "integer", "description": "player slot you act as (multiplayer; default 0)"});
    let stop = json!({"type": "array", "items": {"type": "string", "enum": m::STOP_ON}, "description": "return early when: hurt (you lost HP), kill, hit (you dealt damage), no_damage (your hit did nothing), phase, danger (a hit on you is <= 12 frames away if you stand still), event (story: an event, dialogue box or menu took the field), area (an exit was taken); event and area are the defaults of every macro but press / wait"});
    let btn = json!({"type": "string", "description": "buttons held, e.g. \"RIGHT+A\" (A punch, B skill, L cycle skill, R block, UP/DOWN/LEFT/RIGHT)"});
    let mods = json!({"type": "array", "items": {"type": "string"}, "description": "mod folders to load over the base game (engine/docs/modding.md; e.g. [\"engine/mods-examples/story-tweaks\"]); [] = none; default: the WF_MODS env var (a :-separated path list), else none. Content is cached per mod set and rebuilt when the mod's files change"});
    let t = |name: &str, desc: &str, props: Value, req: &[&str]| json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": req}});
    json!([
        t("list_fighters", "Playable fighters: forms, skills with EP cost by skill level, the buttons.", json!({"mods": mods}), &[]),
        t("list_enemies", "Enemy types: id (t18), group (Boss/Enemy/Helper), HP, stats, resistances, presets and default arena.", json!({"search": {"type": "string"}, "group": {"type": "string"}, "limit": {"type": "integer"}, "mods": mods}), &[]),
        t("list_arenas", "Arenas (maps) a battle can use.", json!({}), &[]),
        t("list_presets", "Story fights (multi-phase chains such as Janemba -> Super Janemba) with their arena.", json!({"mods": mods}), &[]),
        t("modcheck", "Check mod folders (engine/docs/modding.md §7): every problem with its file, what is wrong and a hint; ok / exit_status (0 ok, 1 errors) and the summary lines (what each mod adds, replaces, patches).", json!({"mods": mods}), &[]),
        t("list_mod_content", "What the loaded mods add and change, per mod: skills, enemies, characters, items, equipment, areas, events, scripts, art - each file added / replaced / patched / derived / deleted, with the record's id and name.", json!({"mods": mods}), &[]),
        t("new_battle", "Start a battle session (paused at frame 0). Returns the session id, the resolved config and a full observation with an ASCII map. Level 50..200 (Lv80+ comfortable vs regular enemies, bosses want 100+). `mods`: play with mod folders loaded.", json!({
            "mods": mods,
            "fighter": {"type": "string", "description": "goku, gohan, goten, trunks, vegeta, gotenks, vegito, gogeta"},
            "form": {"type": "integer"}, "level": {"type": "integer", "description": "default 80"},
            "enemy": {"type": "string", "description": "enemy id (t68) or name"}, "count": {"type": "integer"},
            "preset": {"type": "string", "description": "preset id (list_presets), overrides enemy"},
            "arena": {"type": "string"}, "seed": {"type": "integer"},
            "skill_level": {"type": "integer", "description": "0..2 skill upgrade tier (EP cost tier); default 1"},
            "stat_spend": {"type": "string", "description": "even (default) | STR | POW | END | SPD | none"},
            "players": {"description": "co-op: number of players (copies of the fighter) or a list of {fighter, form, level, ...}; you act as `slot`, other slots run their autopilot"},
        }), &[]),
        t("list_areas", "Field areas (overworld) for new_field: world-area id, name, map variants, placed enemies.", json!({"search": {"type": "string"}, "limit": {"type": "integer"}, "mods": mods}), &[]),
        t("new_field", "Start a field (overworld) session in an area: walk, take exits to other areas, fight the placed enemies, break props, collect pickups (scripts / dialogue are not run). Same action tools as battles.", json!({
            "mods": mods,
            "area": {"type": "string", "description": "\"8-3\" (world-area, list_areas) or a name"}, "variant": {"type": "integer"},
            "chars": {"type": "array", "items": {"type": "integer"}, "description": "character per player slot (0 Goku, 1 Gohan, 2 Goten, 3 Trunks, 4 Vegeta, ...); default the new game's"},
            "x": {"type": "integer"}, "y": {"type": "integer"}, "seed": {"type": "integer"},
        }), &["area"]),
        t("new_game", "Start the story (the whole game with its script host: events, dialogue, NPCs, shops, save points, the title and its New Game / Continue). Default: from the title through New Game into the prologue; title=true stops on the title screen (press START, then A through Single Player / Select Game). Observations gain `story`: the menu screen and its text, the dialogue box, event_running, the save slots. Every action tool, save_state / load_state / rewind and replays work on it; `frame` counts steps (one game logic iteration each).", json!({
            "seed": {"type": "integer"}, "title": {"type": "boolean", "description": "stop on the title screen"}, "mods": mods,
            "saves": {"type": "string", "description": "a directory of the app's save slot files (slot1.sav .., options.sav - a native app's --data DIR/saves): the cartridge at the start (with title=true: START, A, then the slot on Select Game continues it)"},
        }), &[]),
        t("list_sessions", "Open sessions.", json!({}), &[]),
        t("close_session", "Close a session.", json!({"session": sess}), &["session"]),
        t("fork_session", "Copy a session (state, inputs, saves) into a new independent session.", json!({"session": sess}), &["session"]),
        t("observe", "Observation: player, enemies (melee hint, AI intent, attacking, takes = what damages it now), threats with ETA, danger forecast, walls, events since the last observation.", json!({"session": sess, "slot": slot, "full": {"type": "boolean"}, "map": {"type": "boolean"}}), &["session"]),
        t("map", "ASCII map of the arena ('#' wall, 'P' you, letters = enemies).", json!({"session": sess, "slot": slot, "cell": {"type": "integer", "description": "px per character, default 16"}}), &["session"]),
        t("screenshot", "PNG of the game's own renderer. view: gba (240x160 camera), wide (480x320, default), arena (whole arena). Enemy ids drawn in yellow above them.", json!({"session": sess, "slot": slot, "view": {"type": "string", "enum": ["gba", "wide", "arena"]}, "scale": {"type": "integer"}, "labels": {"type": "boolean"}, "save_to": {"type": "string"}}), &["session"]),
        t("press", "Hold buttons for frames (60 frames = 1 s). steps = [{buttons, frames}, ...] for sequences. A press acts on the frame the button goes down: holding A punches once.", json!({"session": sess, "slot": slot, "buttons": btn, "frames": {"type": "integer"}, "steps": {"type": "array", "items": {"type": "object", "properties": {"buttons": btn, "frames": {"type": "integer"}}}}, "stop_on": stop}), &["session"]),
        t("wait", "Release all buttons for frames.", json!({"session": sess, "slot": slot, "frames": {"type": "integer"}, "stop_on": stop}), &["session", "frames"]),
        t("move_to", "Walk to (x, y) (footprint centre, world px, like observation positions) along a path on the walk grid. stop_on defaults to [hurt].", json!({"session": sess, "slot": slot, "x": {"type": "integer"}, "y": {"type": "integer"}, "max_frames": {"type": "integer"}, "stop_on": stop}), &["session", "x", "y"]),
        t("take_exit", "Field / story: leave the area through an exit (observation field.exits; `to` = its destination \"1-4\" or name, default the first listed = nearest): walks to it and into it (doors sit in walls, edge exits at the map's border) until the area changes.", json!({"session": sess, "slot": slot, "to": {"type": "string"}, "max_frames": {"type": "integer"}, "stop_on": stop}), &["session"]),
        t("talk", "Field / story: talk to an NPC (observation field.npcs; `npc` = its id, default the nearest that talks), or use a trigger that wants A (`trigger` = its index in field.triggers, e.g. a desk or a sign: activation \"press A inside it\"): walks up to it, turns, presses A; ends when its event starts (then read story.dialog and press A through it).", json!({"session": sess, "slot": slot, "npc": {"type": "integer"}, "trigger": {"type": "integer"}, "max_frames": {"type": "integer"}, "stop_on": stop}), &["session"]),
        t("face", "Turn toward a target enemy or a direction without moving.", json!({"session": sess, "slot": slot, "target": {"type": "string"}, "direction": {"type": "string", "enum": m::DIRS}}), &["session"]),
        t("attack", "Melee macro: walk to a punching spot of the target (default nearest), turn, punch on every new press until the target falls / hits reached / max_frames (300).", json!({"session": sess, "slot": slot, "target": {"type": "string", "description": "enemy id (3 or #3), name, or nearest"}, "hits": {"type": "integer"}, "max_frames": {"type": "integer"}, "stop_on": stop}), &["session"]),
        t("use_skill", "Select a skill (L), optionally line up with and face a target, then hold B for charge_frames, taps times (gap frames apart; Energy Blast refires every ~26 frames). steer = d-pad held while charging (Flame Kamehameha whip, Dragon Fist head, Energy Sword).", json!({"session": sess, "slot": slot, "skill": {"description": "slot number or name"}, "charge_frames": {"type": "integer"}, "taps": {"type": "integer"}, "gap": {"type": "integer"}, "target": {"type": "string"}, "align": {"type": "boolean"}, "steer": btn, "settle": {"type": "integer"}, "max_frames": {"type": "integer"}, "stop_on": stop}), &["session", "skill"]),
        t("block", "Hold R (frontal hits do ~1/16 damage); shield=true holds R+B (energy shield, drains EP).", json!({"session": sess, "slot": slot, "frames": {"type": "integer"}, "shield": {"type": "boolean"}, "stop_on": stop}), &["session", "frames"]),
        t("dash", "Dash (double tap from idle, then hold) in a direction for frames.", json!({"session": sess, "slot": slot, "direction": {"type": "string", "enum": m::DIRS}, "frames": {"type": "integer"}, "stop_on": stop}), &["session", "direction"]),
        t("save_state", "Snapshot the session under a name (instant).", json!({"session": sess, "name": {"type": "string"}}), &["session", "name"]),
        t("load_state", "Restore a named snapshot (later inputs of the current branch are discarded).", json!({"session": sess, "name": {"type": "string"}}), &["session", "name"]),
        t("rewind", "Go back `frames` frames (or `to_frame`) on the current branch.", json!({"session": sess, "frames": {"type": "integer"}, "to_frame": {"type": "integer"}}), &["session"]),
        t("get_replay", "Replay of the session: config + run-length input log + state hash (optionally written to save_to).", json!({"session": sess, "save_to": {"type": "string"}}), &["session"]),
        t("play_replay", "Re-simulate a replay (object, JSON string or file path) in a new session and check the state hash. A replay made with mods records their ids, files hash and folders: it loads those folders (or `mods`) and is refused when the loaded mods differ.", json!({"replay": {"description": "replay object / JSON string"}, "path": {"type": "string"}, "keep": {"type": "boolean"}, "mods": mods}), &[]),
        t("run_controller", "Run your Rhai controller every frame (deterministic sandbox, metered). The script body sees `obs` (observation), `mem` (persistent map), `frame`, and returns buttons (\"RIGHT+A\" / \"\"). Helpers: toward(from, to), away(from, to), dist(a, b), btn(s), names(mask), log(x), stop(). Returns the summary, controller stats/logs and the observation.", json!({"session": sess, "slot": slot, "code": {"type": "string"}, "frames": {"type": "integer"}, "every": {"type": "integer", "description": "decide every N frames (hold between)"}, "max_ops": {"type": "integer", "description": "operation budget per decision (default 200000)"}, "forecast": {"type": "integer", "description": "look-ahead frames for obs.danger (default 20, 0 = off: faster)"}, "stop_on": stop}), &["session", "code", "frames"]),
        t("evaluate_controller", "Run a controller from frame 0 over many seeds (the session's config and content, or new_battle args - `mods` included - in `battle`) and report win rate, HP lost, time. Runs in parallel, fully deterministic.", json!({"session": sess, "battle": {"type": "object"}, "code": {"type": "string"}, "seeds": {"type": "array", "items": {"type": "integer"}}, "n_seeds": {"type": "integer"}, "frames": {"type": "integer", "description": "max frames per run (default 7200)"}, "every": {"type": "integer"}, "max_ops": {"type": "integer"}, "forecast": {"type": "integer"}}), &["code"]),
        t("set_autopilot", "Co-op: who drives a player slot when you are not acting as it: idle, attack (melee the nearest enemy), or a Rhai controller (code).", json!({"session": sess, "slot": slot, "mode": {"type": "string", "enum": ["idle", "attack", "controller"]}, "code": {"type": "string"}}), &["session", "slot", "mode"]),
    ])
}

impl Server {
    pub fn new() -> Server {
        Server { sessions: BTreeMap::new(), next: 1, renderer: None, event_stops: false }
    }

    fn data(&self) -> Result<Arc<GameData>, String> {
        crate::content::game_data()
    }

    /// the content of a call: `mods` (mod folder paths; `[]` = none) or else `WF_MODS`, else the base pack.
    /// Returns the game data and the mod set (None = no mods).
    fn content(&self, a: &Value) -> Result<(Arc<GameData>, Option<Arc<ModSet>>), String> {
        content_of(a)
    }

    fn sess(&mut self, a: &Value) -> Result<&mut Agent, String> {
        let id = s(a, "session").ok_or("missing \"session\" (create one with new_battle)")?;
        let have = self.sessions.keys().cloned().collect::<Vec<_>>().join(", ");
        self.sessions.get_mut(id).ok_or_else(|| format!("no session \"{id}\" (open: {}; create one with new_battle)", if have.is_empty() { "none".into() } else { have }))
    }

    fn new_id(&mut self) -> String {
        let id = format!("s{}", self.next);
        self.next += 1;
        if self.sessions.len() >= 64 {
            let oldest = self.sessions.keys().next().cloned().unwrap();
            self.sessions.remove(&oldest);
        }
        id
    }

    pub fn call(&mut self, name: &str, a: &Value) -> Result<Out, String> {
        let a = if a.is_null() { &json!({}) } else { a };
        let j = |v: Value| Ok(Out::Json(v));
        self.event_stops = !matches!(name, "press" | "wait");
        match name {
            "list_fighters" => j(catalog::fighters(&*self.content(a)?.0)),
            "list_enemies" => j(catalog::enemies(&*self.content(a)?.0, s(a, "search"), s(a, "group"), i(a, "limit").unwrap_or(25).max(1) as usize)),
            "list_arenas" => j(catalog::arenas()),
            "list_presets" => j(catalog::presets(&*self.content(a)?.0)),
            "modcheck" => {
                let dirs = mods_arg(a)?.unwrap_or_else(crate::content::default_mods);
                if dirs.is_empty() {
                    return Err("modcheck: pass `mods` (mod folder paths) or set WF_MODS".into());
                }
                let base = crate::content::content_dir()?;
                let r = wf_app::modcheck::check(&base, &dirs);
                let errors: Vec<Value> = r.errors.iter().map(|e| json!({"file": e.file, "what": e.what, "hint": e.hint})).collect();
                j(json!({"ok": r.ok(), "exit_status": if r.ok() { 0 } else { 1 }, "errors": errors, "lines": r.lines}))
            }
            "list_mod_content" => {
                let (_, set) = self.content(a)?;
                let set = set.ok_or("list_mod_content: pass `mods` (mod folder paths) or set WF_MODS")?;
                j(catalog::mod_content(&set))
            }
            "new_battle" => {
                let (d, set) = self.content(a)?;
                let (cfg, label) = battle_config(&d, a)?;
                let id = self.new_id();
                let mut ag = Agent::new(id.clone(), d, cfg.clone(), label.clone(), a.clone());
                ag.content = set.clone();
                let obs = observe(&mut ag, 0, ObsOpts { full: true, map: true, ..Default::default() });
                self.sessions.insert(id.clone(), ag);
                j(json!({"session": id, "label": label, "config": cfg, "mods": set.map(|m| m.ids()).unwrap_or_default(), "observation": obs}))
            }
            "list_areas" => {
                let d = self.content(a)?.0;
                let fd = d.field.as_ref().ok_or("this content pack has no field data (re-extract with a current wf-extract)")?;
                let q = s(a, "search").map(catalog::norm).unwrap_or_default();
                let lim = i(a, "limit").unwrap_or(40).max(1) as usize;
                let all: Vec<Value> = fd
                    .areas
                    .values()
                    .filter(|ar| q.is_empty() || catalog::norm(ar.name.as_deref().unwrap_or("")).contains(&q) || catalog::norm(&ar.key).contains(&q) || format!("{}-{}", ar.world, ar.area) == q)
                    .map(|ar| {
                        let enemies: Vec<String> = ar
                            .objects
                            .iter()
                            .filter_map(|o| match &o.kind {
                                wf_core::fieldcontent::ObjKind::Enemy { ty, .. } => d.enemy(*ty).map(|e| e.name.clone()),
                                _ => None,
                            })
                            .collect();
                        let mut uniq: Vec<String> = enemies.clone();
                        uniq.sort();
                        uniq.dedup();
                        json!({"area": format!("{}-{}", ar.world, ar.area), "name": ar.name, "variants": ar.maps.len(), "placed_enemies": enemies.len(), "enemy_types": uniq})
                    })
                    .collect();
                j(json!({"total": all.len(), "areas": all.into_iter().take(lim).collect::<Vec<_>>()}))
            }
            "new_field" => {
                let (d, set) = self.content(a)?;
                let fd = d.field.as_ref().ok_or("this content pack has no field data")?;
                let r = s(a, "area").map(String::from).or_else(|| i(a, "area").map(|x| x.to_string())).ok_or("area required (list_areas)")?;
                let (wd, ar) = match r.split_once('-').and_then(|(x, y)| Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))) {
                    Some(p) => p,
                    None => {
                        let q = catalog::norm(&r);
                        let hit = fd.areas.values().find(|x| catalog::norm(x.name.as_deref().unwrap_or("")) == q).or_else(|| fd.areas.values().find(|x| catalog::norm(x.name.as_deref().unwrap_or("")).contains(&q)));
                        let h = hit.ok_or_else(|| format!("no area matches \"{r}\" (list_areas)"))?;
                        (h.world, h.area)
                    }
                };
                let area = fd.area(wd, ar).ok_or_else(|| format!("no area {wd}-{ar} (list_areas)"))?;
                let name = area.name.clone().unwrap_or_default();
                let mut cfg = wf_core::field::FieldConfig { world: wd, area: ar, variant: i(a, "variant").unwrap_or(0) as i32, rng: RngInit::Seed(i(a, "seed").unwrap_or(1) as u32), ..Default::default() };
                if let (Some(x), Some(y)) = (i(a, "x"), i(a, "y")) {
                    cfg.pos = Some((x as i32 - 8, y as i32 - 4));
                }
                if let Some(c) = a.get("chars").and_then(|v| v.as_array()) {
                    cfg.chars = c.iter().filter_map(|x| x.as_i64()).map(|x| x as i32).take(4).collect();
                }
                let id = self.new_id();
                let label = format!("field {wd}-{ar} {name}");
                let mut ag = Agent::new_field(id.clone(), Arc::clone(&d), cfg.clone(), label.clone(), a.clone());
                ag.content = set.clone();
                let obs = observe(&mut ag, 0, ObsOpts { full: true, map: true, ..Default::default() });
                self.sessions.insert(id.clone(), ag);
                j(json!({"session": id, "label": label, "config": cfg, "mods": set.map(|m| m.ids()).unwrap_or_default(), "observation": obs}))
            }
            "new_game" => {
                let (d, set) = self.content(a)?;
                // (the base pack's story files also set the dialogue font and the event bytecode, once per process)
                let mut files = story_files()?;
                if let Some(m) = &set {
                    files = m.story_files()?;
                }
                let id = self.new_id();
                // saves: a directory with the app's slot files (`slot1.sav` .. `slot3.sav`, `options.sav`), e.g. a native
                // app's `--data DIR/saves`: the title's Continue loads them
                let slots = match s(a, "saves") {
                    Some(dir) => {
                        let dir = std::path::Path::new(dir);
                        let mut v: Vec<String> = (1..=wf_core::menu::save::SLOTS)
                            .map(|k| dir.join(format!("slot{k}.sav")))
                            .chain([dir.join("options.sav")])
                            .map(|p| std::fs::read(p).unwrap_or_default().iter().map(|x| format!("{x:02x}")).collect())
                            .collect();
                        while v.last().is_some_and(|x| x.is_empty()) {
                            v.pop();
                        }
                        v
                    }
                    None => vec![],
                };
                let start = crate::env::StoryStart { seed: i(a, "seed").unwrap_or(1) as u32, slots };
                let mut ag = Agent::new_story(id.clone(), d, files, start)?;
                ag.content = set.clone();
                let mut steps = 0;
                if !b(a, "title").unwrap_or(false) {
                    steps = title_new_game(&mut ag)?;
                }
                let label = "story".to_string();
                let obs = observe(&mut ag, 0, ObsOpts { full: true, ..Default::default() });
                self.sessions.insert(id.clone(), ag);
                let mods = self.sessions.get(&id).and_then(|s| s.content.as_ref()).map(|m| m.ids()).unwrap_or_default();
                j(json!({"session": id, "label": label, "title_steps": steps, "mods": mods, "observation": obs}))
            }
            "list_sessions" => j(Value::Array(
                self.sessions
                    .values()
                    .map(|s| json!({"session": s.id, "label": s.label, "frame": s.frame(), "outcome": outcome_str(s.outcome()), "saves": s.named.keys().collect::<Vec<_>>(), "slots": s.slots(), "mods": s.content.as_ref().map(|m| m.ids()).unwrap_or_default()}))
                    .collect(),
            )),
            "close_session" => {
                let id = s(a, "session").unwrap_or("");
                j(json!({"closed": self.sessions.remove(id).is_some()}))
            }
            "fork_session" => {
                let id = self.new_id();
                let f = self.sess(a)?.fork(id.clone());
                let fr = f.frame();
                self.sessions.insert(id.clone(), f);
                j(json!({"session": id, "frame": fr}))
            }
            "observe" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let o = ObsOpts { full: b(a, "full").unwrap_or(false), map: b(a, "map").unwrap_or(false), ..Default::default() };
                let ag = self.sess(a)?;
                check_slot(ag, slot)?;
                j(observe(ag, slot, o))
            }
            "map" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let cell = i(a, "cell").unwrap_or(16).clamp(4, 64) as i32;
                let ag = self.sess(a)?;
                j(ascii_map(ag, slot, cell))
            }
            "screenshot" => self.screenshot(a),
            "press" => {
                let steps: Vec<(u16, u32)> = match a.get("steps").and_then(|v| v.as_array()) {
                    Some(v) if !v.is_empty() => v.iter().map(|x| Ok((parse_buttons(x.get("buttons").unwrap_or(&Value::Null))?, frames(x, "frames", 1, 3600)))).collect::<Result<_, String>>()?,
                    _ => vec![(parse_buttons(a.get("buttons").unwrap_or(&Value::Null))?, frames(a, "frames", 1, 3600))],
                };
                let total = steps.iter().map(|x| x.1).sum::<u32>();
                self.act(a, m::sequence(steps), total, &[], json!({}))
            }
            "wait" => {
                let n = frames(a, "frames", 1, 3600);
                self.act(a, m::hold(0, n), n, &[], json!({}))
            }
            "move_to" => {
                let (x, y) = (i(a, "x").ok_or("x required")? as i32, i(a, "y").ok_or("y required")? as i32);
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let ag = self.sess(a)?;
                let p = ag.player(slot).ok_or("no such player slot")?;
                let (fw, fh) = (p.a.foot_w, p.a.foot_h);
                let mf = frames(a, "max_frames", 600, 3600);
                self.act(a, m::walk_to(x - (fw >> 1), y - (fh >> 1), 1), mf, &["hurt"], json!({}))
            }
            "take_exit" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let ag = self.sess(a)?;
                let p = ag.player(slot).ok_or("no such player slot")?;
                let (fw, fh) = (p.a.foot_w, p.a.foot_h);
                let f = crate::obs::field_json(ag, slot);
                let exits = f["exits"].as_array().cloned().unwrap_or_default();
                let want = s(a, "to").map(String::from).or_else(|| i(a, "to").map(|x| x.to_string()));
                let e = match &want {
                    Some(t) => exits.iter().find(|e| e["to"] == t.as_str() || e["to_name"].as_str().is_some_and(|n| catalog::norm(n) == catalog::norm(t))),
                    None => exits.first(),
                };
                let names = exits.iter().map(|e| e["to"].as_str().unwrap_or("?").to_string()).collect::<Vec<_>>().join(", ");
                let e = e.ok_or_else(|| format!("no such exit here (observation field.exits: {names})"))?;
                let r: Vec<i32> = e["rect"].as_array().map(|v| v.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default();
                let to = e["to"].clone();
                let mf = frames(a, "max_frames", 900, 3600);
                self.act(a, m::take_exit([r[0], r[1], r[2], r[3]], fw, fh), mf, &[], json!({"exit_to": to}))
            }
            "talk" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let ag = self.sess(a)?;
                let f = crate::obs::field_json(ag, slot);
                if let Some(k) = i(a, "trigger") {
                    let t = f["triggers"].get(k as usize).cloned().ok_or_else(|| format!("no trigger {k} here (observation field.triggers)"))?;
                    let r: Vec<i32> = t["rect"].as_array().map(|v| v.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default();
                    let act = t["activation"].as_str().unwrap_or("");
                    let facing = m::DIRS.iter().position(|d| act.ends_with(&format!("facing {d}")));
                    let mf = frames(a, "max_frames", 900, 3600);
                    return self.act(a, m::use_trigger([r[0], r[1], r[2], r[3]], facing), mf, &[], json!({"trigger": t}));
                }
                let npcs = f["npcs"].as_array().cloned().unwrap_or_default();
                let id = match i(a, "npc") {
                    Some(n) => n as u32,
                    None => npcs.iter().find(|n| n["talks"] == true).or(npcs.first()).and_then(|n| n["id"].as_u64()).ok_or("no NPCs here (observation field.npcs)")? as u32,
                };
                if m::npc_pos(ag, id).is_none() {
                    return Err(format!("no NPC #{id} here (observation field.npcs)"));
                }
                let mf = frames(a, "max_frames", 900, 3600);
                self.act(a, m::talk(id), mf, &[], json!({"npc": id}))
            }
            "face" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let ag = self.sess(a)?;
                let (dir, tgt) = match s(a, "direction") {
                    Some(d) => (Some(m::dir_index(d)?), None),
                    None => (None, Some(m::find_enemy(ag, slot, s(a, "target")).ok_or("no such live enemy")?)),
                };
                self.act(a, m::face(dir, tgt), 70, &[], json!({}))
            }
            "attack" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let ag = self.sess(a)?;
                let tr = target_ref(a);
                let hits = i(a, "hits").map(|h| h.max(1) as u32);
                let mf = frames(a, "max_frames", 300, 3600);
                let e = m::find_enemy(ag, slot, tr.as_deref());
                let between_phases = e.is_none() && tr.is_none() && ag.w().battle.next_phase_at > 0;
                if between_phases {
                    // the next phase spawns soon: wait for it, then attack its nearest enemy
                    return self.act(a, m::chain(vec![m::wait_enemy(240), m::auto_attack_once(hits)]), mf, &[], json!({"target": "next phase"}));
                }
                let e = e.ok_or_else(|| if tr.is_some() { format!("no live enemy matches \"{}\"", tr.clone().unwrap()) } else { "no live enemies".into() })?;
                let label = crate::env::describe(ag.w(), e, None);
                self.act(a, m::attack(e, hits), mf, &[], json!({"target": label}))
            }
            "use_skill" => {
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let ag = self.sess(a)?;
                let id = m::resolve_skill(ag, slot, a.get("skill").unwrap_or(&Value::Null))?;
                let tr = target_ref(a);
                let target = match &tr {
                    Some(t) => Some(m::find_enemy(ag, slot, Some(t)).ok_or_else(|| format!("no live enemy matches \"{t}\""))?),
                    None => None,
                };
                let name = catalog::skill_name(ag.data(), id);
                let mut extra = json!({"skill": name});
                if let Some(t) = target {
                    extra["target"] = json!(crate::env::describe(ag.w(), t, None));
                }
                let o = SkillOpts {
                    charge: frames(a, "charge_frames", 1, 3600),
                    taps: frames(a, "taps", 1, 50).max(1),
                    gap: frames(a, "gap", 26, 600),
                    target,
                    align: b(a, "align").unwrap_or(false),
                    steer: parse_buttons(a.get("steer").unwrap_or(&Value::Null))?,
                    settle: frames(a, "settle", 240, 3600),
                };
                let mf = frames(a, "max_frames", 900, 7200);
                self.act(a, m::use_skill(id, o), mf, &[], extra)
            }
            "block" => {
                let n = frames(a, "frames", 30, 3600);
                self.act(a, m::block(n, b(a, "shield").unwrap_or(false)), n + 1, &[], json!({}))
            }
            "dash" => {
                let d = m::dir_index(s(a, "direction").ok_or("direction required")?)?;
                let n = frames(a, "frames", 20, 600);
                self.act(a, m::dash(d, n), n + 64, &[], json!({}))
            }
            "save_state" => {
                let name = s(a, "name").ok_or("name required")?.to_string();
                let ag = self.sess(a)?;
                ag.save(&name);
                j(json!({"saved": name, "frame": ag.frame(), "saves": ag.named.keys().collect::<Vec<_>>()}))
            }
            "load_state" => {
                let name = s(a, "name").ok_or("name required")?.to_string();
                let ag = self.sess(a)?;
                let t0 = Instant::now();
                ag.load(&name)?;
                let us = t0.elapsed().as_micros();
                j(json!({"loaded": name, "frame": ag.frame(), "restore_us": us as u64, "observation": observe(ag, 0, ObsOpts::default())}))
            }
            "rewind" => {
                let ag = self.sess(a)?;
                let target = match i(a, "to_frame") {
                    Some(f) => f.max(0) as u32,
                    None => ag.frame().saturating_sub(i(a, "frames").unwrap_or(60).max(0) as u32),
                };
                let t0 = Instant::now();
                ag.rewind_to(target);
                let us = t0.elapsed().as_micros();
                j(json!({"frame": ag.frame(), "restore_us": us as u64, "observation": observe(ag, 0, ObsOpts::default())}))
            }
            "get_replay" => {
                let ag = self.sess(a)?;
                let r = ag.replay();
                let v = serde_json::to_value(&r).map_err(|e| e.to_string())?;
                if let Some(p) = s(a, "save_to") {
                    std::fs::write(p, serde_json::to_vec(&v).unwrap()).map_err(|e| format!("{p}: {e}"))?;
                    return j(json!({"saved_to": p, "frames": r.frames, "hash": r.hash, "outcome": r.outcome}));
                }
                j(v)
            }
            "play_replay" => {
                let v: Value = match (a.get("replay"), s(a, "path")) {
                    (Some(Value::String(t)), _) => serde_json::from_str(t).map_err(|e| format!("replay: {e}"))?,
                    (Some(v), _) if v.is_object() => v.clone(),
                    (_, Some(p)) => serde_json::from_slice(&std::fs::read(p).map_err(|e| format!("{p}: {e}"))?).map_err(|e| format!("replay: {e}"))?,
                    _ => return Err("pass `replay` (from get_replay) or `path`".into()),
                };
                let r: Replay = serde_json::from_value(v).map_err(|e| format!("not a wf-agent-replay/1 object: {e}"))?;
                // the content: `mods` if given, else the folders the replay was made with (not WF_MODS: a replay
                // knows its mods); then the recorded mod list / file hash must match the loaded one
                let dirs = mods_arg(a)?.unwrap_or_else(|| r.mod_dirs.iter().map(PathBuf::from).collect());
                let set = crate::content::mod_set(&dirs)?;
                let (rec_mods, rec_hash) = r.content.as_ref().map(|c| (c.mods.clone(), c.mods_hash.clone())).unwrap_or_default();
                let (now_mods, now_hash) = set.as_ref().map(|m| (m.content_id.mods.clone(), m.content_id.mods_hash.clone())).unwrap_or_default();
                if rec_mods != now_mods || (!rec_mods.is_empty() && rec_hash != now_hash) {
                    return Err(if rec_mods == now_mods {
                        format!("replay refused: made with the same mods ({}) but other versions of their files - load the exact mod folders it was made with", mod_list(&rec_mods))
                    } else {
                        format!("replay refused: made with mods: {}; loaded now: {} - pass `mods` with the folders it was made with", mod_list(&rec_mods), mod_list(&now_mods))
                    });
                }
                let d = match &set {
                    Some(m) => Arc::clone(&m.data),
                    None => self.data()?,
                };
                let id = self.new_id();
                let t0 = Instant::now();
                let mut ag = match (&r.story, &r.field) {
                    (Some(st), _) => {
                        let mut files = story_files()?;
                        if let Some(m) = &set {
                            files = m.story_files()?;
                        }
                        Agent::new_story(id.clone(), d, files, st.clone())?
                    }
                    (None, Some(f)) => Agent::new_field(id.clone(), d, f.clone(), "replay".into(), json!({"replay": true})),
                    (None, None) => Agent::new(id.clone(), d, r.config.clone(), "replay".into(), json!({"replay": true})),
                };
                ag.content = set;
                ag.record = false;
                for h in r.held() {
                    ag.step_held(h, 0);
                }
                ag.record = true;
                let ms = t0.elapsed().as_secs_f64() * 1e3;
                let hash = ag.hash();
                let out = json!({"session": if b(a, "keep").unwrap_or(true) { json!(id) } else { Value::Null }, "frames": ag.frame(), "outcome": outcome_str(ag.outcome()), "hash": hash, "expected": r.hash, "deterministic": hash == r.hash, "resim_ms": (ms * 100.0).round() / 100.0});
                if b(a, "keep").unwrap_or(true) {
                    self.sessions.insert(id, ag);
                }
                j(out)
            }
            "run_controller" => {
                let code = s(a, "code").ok_or("code required")?;
                let code = load_code(code)?;
                let mut c = Controller::new(&code, i(a, "max_ops").unwrap_or(200_000).max(1000) as u64, frames(a, "every", 1, 600))?;
                c.forecast = frames(a, "forecast", 20, 120);
                let n = frames(a, "frames", 600, 72_000);
                let slot = i(a, "slot").unwrap_or(0) as usize;
                let so = stop_on(a, &[])?;
                let ag = self.sess(a)?;
                check_slot(ag, slot)?;
                let t0 = Instant::now();
                let sum = run_policy(ag, slot, &mut c, n, &so);
                let ms = t0.elapsed().as_secs_f64() * 1e3;
                let mut r = sum.to_json();
                r["controller"] = c.report();
                if let Some(e) = &c.error {
                    r["error"] = json!(e);
                }
                r["wall_ms"] = json!((ms * 10.0).round() / 10.0);
                j(json!({"result": r, "observation": observe(ag, slot, ObsOpts::default())}))
            }
            "evaluate_controller" => self.evaluate(a),
            "set_autopilot" => {
                let slot = i(a, "slot").ok_or("slot required")? as usize;
                let mode = s(a, "mode").unwrap_or("idle").to_string();
                let code = s(a, "code").map(load_code).transpose()?;
                let ag = self.sess(a)?;
                check_slot(ag, slot)?;
                let (p, desc): (Option<Box<dyn Policy>>, String) = match mode.as_str() {
                    "idle" => (None, "idle".into()),
                    "attack" => (Some(m::auto_attack()), "attack".into()),
                    "controller" => {
                        let c = Controller::new(code.as_deref().ok_or("mode controller needs code")?, 200_000, 1)?;
                        (Some(Box::new(c)), "controller".into())
                    }
                    x => return Err(format!("mode \"{x}\": idle | attack | controller")),
                };
                ag.autopilot[slot] = p;
                ag.autopilot_desc[slot] = desc.clone();
                j(json!({"slot": slot, "autopilot": desc}))
            }
            _ => Err(format!("unknown tool \"{name}\"")),
        }
    }

    fn act(&mut self, a: &Value, mut p: Box<dyn Policy>, max: u32, default_stop: &[&str], extra: Value) -> Result<Out, String> {
        let slot = i(a, "slot").unwrap_or(0) as usize;
        let mut so = stop_on(a, default_stop)?;
        let ev = self.event_stops && a.get("stop_on").is_none();
        let ag = self.sess(a)?;
        check_slot(ag, slot)?;
        // a macro (not press / wait) stops when it takes an exit, and in story sessions when an event / dialogue /
        // menu takes the field
        if ev {
            so.push("area".into());
            if ag.story.is_some() {
                so.push("event".into());
            }
        }
        let sum = run_policy(ag, slot, p.as_mut(), max, &so);
        let mut r = sum.to_json();
        if let Value::Object(x) = extra {
            for (k, v) in x {
                r[k] = v;
            }
        }
        Ok(Out::Json(json!({"result": r, "observation": observe(ag, slot, ObsOpts::default())})))
    }

    fn screenshot(&mut self, a: &Value) -> Result<Out, String> {
        let slot = i(a, "slot").unwrap_or(0) as usize;
        let view_kind = s(a, "view").unwrap_or("wide").to_string();
        let labels = b(a, "labels").unwrap_or(true);
        let ag = self.sessions.get(s(a, "session").ok_or("session required")?).ok_or("no such session")?;
        // (a session with mods draws its mods' art)
        let packs: &wf_content::Packs = match &ag.content {
            Some(m) => &m.packs,
            None => crate::content::packs()?,
        };
        let renderer = self.renderer.get_or_insert_with(wf_render::Renderer::new);
        let w = ag.w();
        let mut view = wf_view::RenderView::default();
        wf_core::view::render_view(w, &mut view);
        let (vw, vh) = match view_kind.as_str() {
            "gba" => (240usize, 160usize),
            "arena" => {
                let bd = w.arena.bounds;
                let (aw, ah) = (((bd.x1 - bd.x0 + 32) as usize).min(1024), ((bd.y1 - bd.y0 + 48) as usize).min(1024));
                let cx = (bd.x0 + bd.x1) / 2 - 120;
                let cy = (bd.y0 + bd.y1) / 2 - 80 - 8;
                for c in view.cameras.iter_mut() {
                    c.x = cx;
                    c.y = cy;
                }
                (aw, ah)
            }
            _ => (480, 320),
        };
        let scale = i(a, "scale").unwrap_or(if view_kind == "gba" { 2 } else { 1 }).clamp(1, 4) as usize;
        renderer.render(packs, &view, slot, vw, vh);
        let (ox, oy, _, _) = renderer.origin;
        let mut fb = renderer.fb.clone();
        if labels {
            for id in m::live_enemies(ag) {
                let (_, e) = w.enemy_by_id(id).unwrap();
                let bb = e.a.body_box();
                let t = format!("{id}");
                let tx = (bb.x0 + bb.x1) / 2 - wf_render::font::text_width(&t) / 2 - ox;
                wf_render::font::draw_text(&mut fb, vw, vh, tx, bb.y0 - oy - 9, &t, 0xFFFF_E040);
            }
        }
        let png = png_rgb(&fb, vw, vh, scale);
        let mut cap = json!({"session": ag.id, "frame": ag.frame(), "view": view_kind, "size": [vw * scale, vh * scale], "world_origin": [ox, oy], "scale": scale});
        if let Some(p) = s(a, "save_to") {
            std::fs::write(p, &png).map_err(|e| format!("{p}: {e}"))?;
            cap["saved_to"] = json!(p);
        }
        Ok(Out::Image(png, cap))
    }

    fn evaluate(&mut self, a: &Value) -> Result<Out, String> {
        let code = load_code(s(a, "code").ok_or("code required")?)?;
        // compile once up front for a clear error
        Controller::new(&code, 1000, 1)?;
        // (`battle`: new_battle arguments, its `mods` included; else the session's config and content)
        let (d, cfg) = match a.get("battle") {
            Some(bv) if bv.is_object() => {
                let d = self.content(bv)?.0;
                let cfg = battle_config(&d, bv)?.0;
                (d, cfg)
            }
            _ => {
                let ag = self.sess(a).map_err(|e| format!("{e} (or pass `battle`: new_battle arguments)"))?;
                if ag.field_cfg.is_some() {
                    return Err("evaluate_controller runs battles: pass `battle` (new_battle arguments)".into());
                }
                (Arc::clone(&ag.s.data), ag.s.config.clone())
            }
        };
        let seeds: Vec<u32> = match a.get("seeds").and_then(|v| v.as_array()) {
            Some(v) => v.iter().filter_map(|x| x.as_u64()).map(|x| x as u32).collect(),
            None => (1..=i(a, "n_seeds").unwrap_or(8).clamp(1, 256) as u32).collect(),
        };
        let maxf = frames(a, "frames", 7200, 72_000);
        let every = frames(a, "every", 1, 600);
        let max_ops = i(a, "max_ops").unwrap_or(200_000).max(1000) as u64;
        let fc = frames(a, "forecast", 20, 120);
        let t0 = Instant::now();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(seeds.len()).max(1);
        let mut results: Vec<(u32, Value)> = Vec::new();
        std::thread::scope(|sc| {
            let chunks: Vec<Vec<u32>> = (0..threads).map(|t| seeds.iter().copied().skip(t).step_by(threads).collect()).collect();
            let hs: Vec<_> = chunks
                .into_iter()
                .map(|ch| {
                    let (d, cfg, code) = (Arc::clone(&d), cfg.clone(), code.clone());
                    sc.spawn(move || {
                        let mut out = Vec::new();
                        for seed in ch {
                            let mut c2 = cfg.clone();
                            c2.rng = RngInit::Seed(seed);
                            let mut ag = Agent::new(format!("eval{seed}"), Arc::clone(&d), c2, String::new(), Value::Null);
                            let mut c = Controller::new(&code, max_ops, every).unwrap();
                            c.forecast = fc;
                            let sum = run_policy(&mut ag, 0, &mut c, maxf, &[]);
                            let p = ag.player(0);
                            let mut v = json!({"seed": seed, "outcome": outcome_str(ag.outcome()), "frames": ag.frame(), "hp_lost": sum.hp_lost, "hp_left": p.map(|p| p.hp()), "dealt": sum.dealt, "stopped": sum.stopped, "hash": ag.hash()});
                            if let Some(e) = &c.error {
                                v["error"] = json!(e);
                            }
                            out.push((seed, v));
                        }
                        out
                    })
                })
                .collect();
            for h in hs {
                results.extend(h.join().unwrap());
            }
        });
        results.sort_by_key(|r| r.0);
        let runs: Vec<Value> = results.into_iter().map(|r| r.1).collect();
        let n = runs.len().max(1) as f64;
        let wins: Vec<&Value> = runs.iter().filter(|r| r["outcome"] == "won").collect();
        let mean = |k: &str, l: &[&Value]| if l.is_empty() { Value::Null } else { json!((l.iter().map(|r| r[k].as_f64().unwrap_or(0.0)).sum::<f64>() / l.len() as f64 * 10.0).round() / 10.0) };
        let all: Vec<&Value> = runs.iter().collect();
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let total_frames: u64 = runs.iter().map(|r| r["frames"].as_u64().unwrap_or(0)).sum();
        Ok(Out::Json(json!({
            "runs": runs.len(), "wins": wins.len(), "win_rate": (wins.len() as f64 / n * 1000.0).round() / 1000.0,
            "mean_hp_lost": mean("hp_lost", &all), "mean_frames_to_win": mean("frames", &wins), "mean_dealt": mean("dealt", &all),
            "errors": runs.iter().filter(|r| r.get("error").is_some()).count(),
            "per_seed": runs, "wall_ms": ms.round(), "frames_simulated": total_frames, "threads": threads,
        })))
    }
}

/// the content's story scripts (and, once per process, the dialogue font's advances and the event bytecode the
/// script host reads - as the app's `StorySim`)
fn story_files() -> Result<Arc<BTreeMap<String, String>>, String> {
    static F: std::sync::OnceLock<Result<Arc<BTreeMap<String, String>>, String>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let packs = crate::content::packs()?;
        if let Ok(f) = wf_render::ui::Font::load(packs, "dialog") {
            wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
        }
        if let Some(b) = packs.read("scripts/bytecode.json") {
            wf_import::host::set_step_code(&b);
        }
        let files: BTreeMap<String, String> = packs
            .list()
            .into_iter()
            .filter(|p| p.starts_with("scripts/") && (p.ends_with(".rhai") || p.ends_with(".text.json")))
            .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
            .collect();
        if files.is_empty() {
            return Err("this content has no scripts/ (re-extract with the current extractor)".into());
        }
        Ok(Arc::new(files))
    })
    .clone()
}

/// the title screen -> START -> Single Player -> Select Game -> New Game (the first slot): taps (one frame down, one
/// up) until the title's menu has closed and the prologue's field is up. Returns the steps taken.
fn title_new_game(ag: &mut Agent) -> Result<u32, String> {
    let f0 = ag.frame();
    // the title's intro: START until the menu reacts, then A through the pages
    let mut k = 0u32;
    while wf_core::menu::active(ag.w()) {
        if ag.frame() - f0 > 3000 {
            return Err("the title did not reach New Game in 3000 steps".into());
        }
        let key = if k % 2 == 1 { 0 } else if k < 60 { wf_core::input::START } else { wf_core::input::A };
        ag.step(0, key);
        k += 1;
    }
    Ok(ag.frame() - f0)
}

fn target_ref(a: &Value) -> Option<String> {
    match a.get("target") {
        Some(Value::String(x)) => Some(x.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn check_slot(ag: &Agent, slot: usize) -> Result<(), String> {
    if slot >= ag.slots() {
        return Err(format!("slot {slot}: this session has {} player slot(s)", ag.slots()));
    }
    Ok(())
}

/// `@file:path` loads the code from a file
fn load_code(c: &str) -> Result<String, String> {
    match c.strip_prefix("@file:") {
        Some(p) => std::fs::read_to_string(p.trim()).map_err(|e| format!("{p}: {e}")),
        None => Ok(c.to_string()),
    }
}
