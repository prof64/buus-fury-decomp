//! Mod content that runs inside the deterministic core (engine/docs/modding.md §4b): data-defined projectile /
//! effect kinds (`projectiles/*.json` -> `Body::Mod`), scripted skill behaviours (`skills/*.json` `engine.script`),
//! and scripted AI nodes (`ModCondition` / `ModAction` in `ai/state_*.json`).
//!
//! The core never runs a script engine itself: scripts are reached through [`ModHooks`] (implemented with Rhai by
//! `wf-modscript`, attached to `GameData.hooks` by the frontend). A hook is a pure function of a [`Ctx`] (plain
//! integers read from the world) that returns an integer and a list of [`Cmd`]s, which the core then executes in
//! order. Hooks run to completion inside the frame that calls them and keep no state of their own: what must
//! survive a frame lives in [`ModState`] (skill timers, cooldowns, 8 script variables per actor) or in the
//! `ModShot` entities, all plain data in the `World` (cloned / hashed / byte-snapshotted with it). Everything is
//! integer math and the game's RNG is never drawn by this module, so a mod's game is identical on every target and
//! replays / rollback work unchanged. Without mod content nothing here runs and `ModState` hashes as nothing.

use crate::content::{AnimRef, ContentSource, LoadError};
use crate::damage::atk;
use crate::geom::{centered, facing_toward, Rect, DIR_VEC};
use crate::world::{deliver, Body, EntityId, Event, HitMsg, Layer, World};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

/// `GameData::skill_kind` of a skill with a script (`engine.script`)
pub const SCRIPTED: i32 = 1000;
/// the player state of a running scripted skill (`Gs::Skill(SCRIPT_GS)`)
pub const SCRIPT_GS: u8 = 200;
/// script variables per actor (`s.vars[k]`, `set_var(k, v)`)
pub const NVARS: usize = 8;

// ---- the hook interface ---------------------------------------------------------------------------------------------

/// The scripting backend (wf-modscript). `call` must be a pure function of its arguments.
pub trait ModHooks: Send + Sync {
    /// does `script` define `func`
    fn has(&self, script: &str, func: &str) -> bool;
    /// run `func(s)` of `script` with the context map `s`; returns the function's int result (0 for none) and
    /// appends the commands it issued
    fn call(&self, script: &str, func: &str, ctx: &Ctx, out: &mut Vec<Cmd>) -> Result<i32, String>;
    /// a fingerprint of the scripts (content ids: peers / replays must run the same hooks)
    fn digest(&self) -> u64 {
        0
    }
}

/// what a hook sees: named integers, the actor's script variables and the node / call arguments
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    pub vals: Vec<(&'static str, i32)>,
    pub vars: [i32; NVARS],
    pub args: Vec<i32>,
}
impl Ctx {
    fn set(&mut self, k: &'static str, v: i32) {
        self.vals.push((k, v));
    }
}

/// what a hook asks the engine to do (executed in order after the hook returns)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// spawn projectile `proj` at the actor's hand / centre + (ox, oy) px, flying along (vx, vy) (any length; None =
    /// the facing direction)
    Fire {
        proj: String,
        ox: i32,
        oy: i32,
        v: Option<(i32, i32)>,
    },
    /// play an animation of the actor's sprite by name (looping or once-and-hold)
    Anim {
        name: String,
        looping: bool,
    },
    Sfx(i32),
    SfxStop(i32),
    /// skill: end it (back to idle); AI action: (the result decides)
    End,
    SetVar(usize, i32),
    /// spend EP (scaled like the game's skill costs) / drain per frame in 1/256 EP
    Spend(i32),
    Drain(i32),
    /// move the actor to (x, y) px (footprint position) if it fits there
    Teleport(i32, i32),
    /// a hit box of w x h px in front of the actor's body, damage multiplier in 1/1024
    Strike {
        w: i32,
        h: i32,
        mult: i32,
    },
    FaceTarget,
    Face(i32),
    /// AI stack
    Goto(i32),
    Push(i32),
    Pop,
    /// whole HP
    Heal(i32),
    /// spawn an enemy of `ty` at (x, y) px relative to the actor
    Summon {
        ty: i32,
        x: i32,
        y: i32,
    },
    Shake(i32, i32),
    /// an explosion effect at (x, y) px relative to the actor
    Explode(i32, i32),
    /// white hit flash for n frames
    Flash(i32),
    /// the actor cannot be hit (1) / can (0)
    Hittable(bool),
}

// ---- projectile / effect kinds (content) ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallMode {
    /// end (and explode) at a projectile wall
    #[default]
    Explode,
    /// end without an explosion
    Vanish,
    /// fly through walls
    Pass,
    /// reflect off walls
    Bounce,
}

/// one `projectiles/<id>.json`
#[derive(Clone, Debug, Default)]
pub struct ProjDef {
    pub id: String,
    /// drawn from this sheet (`effects/<id>` or `sprites/<id>`)
    pub sheet: String,
    /// a named animation of the sheet (looping), else `frames`
    pub anim: Option<String>,
    /// cels per direction (down, up, left, right) or one list for every direction
    pub frames: Vec<Vec<u16>>,
    /// frames per cel
    pub rate: i32,
    /// cels shown after the end (a spark), for `end_time` frames
    pub end_frames: Vec<u16>,
    pub end_time: i32,
    pub priority: u8,
    /// 1/256 px per frame; acceleration per frame, capped at `max_speed`
    pub speed: i32,
    pub accel: i32,
    pub max_speed: i32,
    /// homing: turn per frame (1/256 of the unit vector), target search radius px, frames before it homes
    pub homing: i32,
    pub homing_range: i32,
    pub homing_delay: i32,
    /// frames of flight (0 = until it leaves the camera)
    pub life: i32,
    pub hit_box: (i32, i32),
    /// extra targets a shot passes through (0 = ends on the first hit)
    pub pierce: i32,
    pub wall: WallMode,
    /// keep flying off camera (default: removed like the game's shots)
    pub offscreen: bool,
    /// damage multiplier in 1/1024 and the attack sub-type (ComputeDamage's reaction / resist class)
    pub damage: i32,
    pub attack_sub: u8,
    /// explosion at the end: area hit radius px (0 = none), its multiplier, the explosion effect, on expiry too
    pub explode_radius: i32,
    pub explode_damage: i32,
    pub explode_fx: bool,
    pub explode_on_expire: bool,
    pub sfx_spawn: i32,
    pub sfx_hit: i32,
    pub sfx_end: i32,
    /// hooks: the script and its functions (`on_hit(s)`, `on_end(s)`, `on_frame(s)`)
    pub script: Option<String>,
}

fn int(v: &Value) -> i32 {
    crate::content::int(v) as i32
}
fn opt_i(v: &Value, k: &str, d: i32) -> i32 {
    v.get(k).filter(|x| !x.is_null()).map(int).unwrap_or(d)
}
fn cels(v: &Value) -> Vec<u16> {
    v.as_array()
        .map(|a| a.iter().map(|x| int(x) as u16).collect())
        .unwrap_or_default()
}

pub fn parse_projectile(id: &str, v: &Value) -> ProjDef {
    let frames = match &v["frames"] {
        Value::Array(a) if a.first().is_some_and(|x| x.is_array()) => a.iter().map(cels).collect(),
        Value::Array(_) => vec![cels(&v["frames"])],
        Value::Null => vec![],
        x => vec![vec![int(x) as u16]],
    };
    let ex = &v["explode"];
    let speed = opt_i(v, "speed", 768);
    ProjDef {
        id: v["id"].as_str().unwrap_or(id).to_string(),
        sheet: v["sheet"]
            .as_str()
            .unwrap_or("effects/fx_0871370c")
            .to_string(),
        anim: v["anim"].as_str().map(String::from),
        frames,
        rate: opt_i(v, "rate", 4).max(1),
        end_frames: cels(&v["end_frames"]),
        end_time: opt_i(v, "end_time", 0).max(0),
        priority: opt_i(v, "priority", 3).clamp(0, 3) as u8,
        speed,
        accel: opt_i(v, "accel", 0),
        max_speed: opt_i(v, "max_speed", speed.max(1) * 4),
        homing: opt_i(v, "homing", 0).max(0),
        homing_range: opt_i(v, "homing_range", 200),
        homing_delay: opt_i(v, "homing_delay", 0),
        life: opt_i(v, "life", 0),
        hit_box: v["hit_box"]
            .as_array()
            .filter(|a| a.len() >= 2)
            .map(|a| (int(&a[0]), int(&a[1])))
            .unwrap_or((12, 12)),
        pierce: opt_i(v, "pierce", 0),
        wall: match v["wall"].as_str() {
            Some("vanish") => WallMode::Vanish,
            Some("pass") => WallMode::Pass,
            Some("bounce") => WallMode::Bounce,
            _ => WallMode::Explode,
        },
        offscreen: v["offscreen"].as_bool().unwrap_or(false),
        damage: opt_i(v, "damage", 1024),
        attack_sub: opt_i(v, "attack_sub", 4) as u8,
        explode_radius: if ex.is_object() {
            opt_i(ex, "radius", 0)
        } else {
            0
        },
        explode_damage: if ex.is_object() {
            opt_i(ex, "damage", 512)
        } else {
            0
        },
        explode_fx: if ex.is_object() {
            ex["fx"].as_bool().unwrap_or(true)
        } else {
            v["explode_fx"].as_bool().unwrap_or(true)
        },
        explode_on_expire: if ex.is_object() {
            ex["on_expire"].as_bool().unwrap_or(false)
        } else {
            false
        },
        sfx_spawn: opt_i(v, "sfx_spawn", -1),
        sfx_hit: opt_i(v, "sfx_hit", -1),
        sfx_end: opt_i(v, "sfx_end", -1),
        script: v["script"].as_str().map(String::from),
    }
}

/// `projectiles/*.json` (a mod category; the base pack has none)
pub fn load_projectiles(
    src: &dyn ContentSource,
    all: &[String],
) -> Result<BTreeMap<String, ProjDef>, LoadError> {
    let mut out = BTreeMap::new();
    for p in all.iter().filter(|p| {
        p.starts_with("projectiles/") && p.ends_with(".json") && !p.ends_with("index.json")
    }) {
        let b = src
            .read(p)
            .ok_or_else(|| LoadError(format!("missing content file {p}")))?;
        let v: Value = serde_json::from_slice(&b).map_err(|e| LoadError(format!("{p}: {e}")))?;
        let id = p["projectiles/".len()..p.len() - 5].to_string();
        let d = parse_projectile(&id, &v);
        out.insert(d.id.clone(), d);
    }
    Ok(out)
}

/// every script file the loaded content refers to (skills, projectiles, AI nodes), for the hook backend
pub fn referenced_scripts(d: &crate::content::GameData) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for s in d.skills.values() {
        if let Some(p) = s.engine.as_ref().and_then(|e| e.script.clone()) {
            v.push(p);
        }
    }
    for p in d.projectiles.values() {
        if let Some(s) = &p.script {
            v.push(s.clone());
        }
    }
    for t in d.states.values().chain(d.aux.values()) {
        for n in &t.nodes {
            if let Some((f, _)) = n.call.split_once(':') {
                v.push(f.to_string());
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

// ---- runtime state ----------------------------------------------------------------------------------------------------

/// a running scripted skill (one per player)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillRt {
    pub player: EntityId,
    pub skill: i32,
    /// frames since the start
    pub t: i32,
    pub held: bool,
    /// frame of the B release (-1 = still held)
    pub released: i32,
    pub looping: bool,
    pub shift: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModState {
    pub skills: Vec<SkillRt>,
    /// (player, skill, the first frame it can start again)
    pub cooldowns: Vec<(EntityId, i32, u32)>,
    /// script variables per actor
    pub vars: Vec<(EntityId, [i32; NVARS])>,
}
impl ModState {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.cooldowns.is_empty() && self.vars.is_empty()
    }
}
impl Hash for ModState {
    fn hash<H: Hasher>(&self, h: &mut H) {
        // nothing while empty: games without scripted mod content hash exactly as before
        if !self.is_empty() {
            self.skills.hash(h);
            self.cooldowns.hash(h);
            self.vars.hash(h);
        }
    }
}

fn vars_of(w: &World, id: EntityId) -> [i32; NVARS] {
    w.modk
        .vars
        .iter()
        .find(|v| v.0 == id)
        .map(|v| v.1)
        .unwrap_or_default()
}
fn set_var(w: &mut World, id: EntityId, k: usize, v: i32) {
    if k >= NVARS {
        return;
    }
    match w.modk.vars.iter_mut().find(|x| x.0 == id) {
        Some(x) => x.1[k] = v,
        None => {
            let mut a = [0; NVARS];
            a[k] = v;
            w.modk.vars.push((id, a));
        }
    }
}

/// a mod projectile in flight (or its end spark)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModShot {
    /// projectile kind (`projectiles/<id>.json`)
    pub def: String,
    pub owner: EntityId,
    /// 0 hits players, 1 hits enemies, 2 everything
    pub team: u8,
    /// position 24.8
    pub x: i32,
    pub y: i32,
    /// flight direction, unit vector in 1/256
    pub ux: i32,
    pub uy: i32,
    /// 1/256 px per frame
    pub speed: i32,
    pub t: i32,
    pub target: EntityId,
    pub hits: Vec<EntityId>,
    /// -1 flying, else frames since the end
    pub dead: i32,
    pub mult: i32,
    pub sub: u8,
    /// the skill that fired it (0 = none)
    pub skill: i32,
}

// ---- helpers -------------------------------------------------------------------------------------------------------------

fn isqrt(n: i64) -> i64 {
    crate::fixed::isqrt(n.max(0))
}
/// (x, y) scaled to length 256 (integer); (0, 0) stays (0, 0)
fn unit(x: i32, y: i32) -> (i32, i32) {
    let l = isqrt(x as i64 * x as i64 + y as i64 * y as i64);
    if l == 0 {
        return (0, 0);
    }
    ((x as i64 * 256 / l) as i32, (y as i64 * 256 / l) as i32)
}

fn report(w: &mut World, what: &str, e: String) {
    w.events.push(Event::Info {
        text: format!("mod script {what}: {e}"),
    });
}

fn split_call(call: &str) -> Option<(&str, &str)> {
    call.split_once(':')
}

/// run a hook if the backend has it: Some(result, commands)
fn run(w: &mut World, script: &str, func: &str, ctx: &Ctx) -> Option<(i32, Vec<Cmd>)> {
    let hooks = w.data.hooks.clone()?;
    if !hooks.has(script, func) {
        return None;
    }
    let mut out = Vec::new();
    match hooks.call(script, func, ctx, &mut out) {
        Ok(r) => Some((r, out)),
        Err(e) => {
            report(w, &format!("{script}:{func}"), e);
            None
        }
    }
}

/// the actor fields of a player / enemy
fn actor_of(w: &World, i: usize) -> Option<&crate::actor::Actor> {
    match &w.ents[i].body {
        Body::Player(p) => Some(&p.a),
        Body::Enemy(e) => Some(&e.a),
        _ => None,
    }
}

/// nearest live enemy to (x, y) within `range` px (0 = any), in spawn order on ties
fn nearest_enemy(w: &World, x: i32, y: i32, range: i32) -> Option<usize> {
    let mut best: Option<(i64, usize)> = None;
    for k in w.live_enemies() {
        let e = &w.en_ref(k).a;
        if !e.hittable {
            continue;
        }
        let (cx, cy) = e.center();
        let d = (cx - x) as i64 * (cx - x) as i64 + (cy - y) as i64 * (cy - y) as i64;
        if range > 0 && d > range as i64 * range as i64 {
            continue;
        }
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, k));
        }
    }
    best.map(|b| b.1)
}
/// nearest live player (enemy shots)
fn nearest_player(w: &World, x: i32, y: i32, range: i32) -> Option<usize> {
    let mut best: Option<(i64, usize)> = None;
    for s in 0..w.players.len() {
        let Some(k) = w.player_idx(s) else { continue };
        let p = w.pl_ref(k);
        if p.dead || !p.a.hittable || w.ents[k].removed {
            continue;
        }
        let (cx, cy) = p.a.center();
        let d = (cx - x) as i64 * (cx - x) as i64 + (cy - y) as i64 * (cy - y) as i64;
        if range > 0 && d > range as i64 * range as i64 {
            continue;
        }
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, k));
        }
    }
    best.map(|b| b.1)
}

fn target_vals(w: &World, ctx: &mut Ctx, from: (i32, i32), k: Option<usize>) {
    match k.and_then(|k| actor_of(w, k).map(|a| (k, a))) {
        Some((k, a)) => {
            let (cx, cy) = a.center();
            let (dx, dy) = (cx - from.0, cy - from.1);
            ctx.set("target", w.ents[k].id as i32);
            ctx.set("tx", a.px());
            ctx.set("ty", a.py());
            ctx.set("tcx", cx);
            ctx.set("tcy", cy);
            ctx.set("tdx", dx);
            ctx.set("tdy", dy);
            ctx.set(
                "tdist",
                isqrt(dx as i64 * dx as i64 + dy as i64 * dy as i64) as i32,
            );
            ctx.set("tfacing", a.facing as i32);
            let (hp, max) = match &w.ents[k].body {
                Body::Enemy(e) => ((e.hp1024 >> 10) as i32, (e.max_hp1024 >> 10) as i32),
                Body::Player(p) => (p.hp(), p.max_hp()),
                _ => (0, 0),
            };
            ctx.set("thp", hp);
            ctx.set("tmax_hp", max);
        }
        None => ctx.set("target", 0),
    }
}

fn actor_vals(w: &World, i: usize, ctx: &mut Ctx) {
    let Some(a) = actor_of(w, i) else { return };
    let (cx, cy) = a.center();
    ctx.set("me", w.ents[i].id as i32);
    ctx.set("x", a.px());
    ctx.set("y", a.py());
    ctx.set("cx", cx);
    ctx.set("cy", cy);
    ctx.set("facing", a.facing as i32);
    ctx.set("frame", w.frame as i32);
    ctx.set("enemies", w.live_enemies().len() as i32);
    match &w.ents[i].body {
        Body::Player(p) => {
            ctx.set("hp", p.hp());
            ctx.set("max_hp", p.max_hp());
            ctx.set("ep", p.ep());
            ctx.set("max_ep", p.max_ep());
            ctx.set("level", p.rec.level);
            ctx.set("slot", p.slot as i32);
        }
        Body::Enemy(e) => {
            ctx.set("hp", (e.hp1024 >> 10) as i32);
            ctx.set("max_hp", (e.max_hp1024 >> 10) as i32);
            ctx.set("type", e.ty);
            ctx.set("counter", e.counter as i32);
            ctx.set(
                "children",
                e.children
                    .iter()
                    .filter(|&&c| {
                        w.enemy_by_id(c)
                            .is_some_and(|(k, e)| !w.ents[k].removed && !e.defeated)
                    })
                    .count() as i32,
            );
        }
        _ => {}
    }
}

// ---- commands ----------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Src {
    Skill(i32),
    Ai,
    Shot,
}

/// execute commands for the actor at `i`; returns true when an `End` was issued
fn apply(w: &mut World, i: usize, src: Src, cmds: Vec<Cmd>) -> bool {
    let mut ended = false;
    let me = w.ents[i].id;
    let is_player = matches!(w.ents[i].body, Body::Player(_));
    for c in cmds {
        if w.ents.get(i).is_none_or(|e| e.id != me || e.removed) {
            break;
        }
        match c {
            Cmd::Fire { proj, ox, oy, v } => {
                let Some(def) = w.data.projectiles.get(&proj).cloned() else {
                    report(w, "fire", format!("unknown projectile {proj}"));
                    continue;
                };
                let Some(a) = actor_of(w, i).cloned() else {
                    continue;
                };
                let (hx, hy) = match &w.ents[i].body {
                    Body::Player(p) => p.hand_point(),
                    _ => a.center(),
                };
                let (dx, dy) = DIR_VEC[a.facing as usize & 3];
                let (ux, uy) = match v.map(|(x, y)| unit(x, y)).filter(|u| *u != (0, 0)) {
                    Some(u) => u,
                    None => (dx * 256, dy * 256),
                };
                let (mult, sub, skill) = match src {
                    Src::Skill(id) => {
                        let lv = crate::player::mod_skill_lv(w, i, id);
                        let e = w.data.skill_engine(id);
                        (
                            e.and_then(|e| crate::content::SkillEngine::pick(&e.damage, lv))
                                .unwrap_or(def.damage),
                            e.and_then(|e| e.attack_sub).unwrap_or(def.attack_sub),
                            id,
                        )
                    }
                    _ => (def.damage, def.attack_sub, 0),
                };
                let team = if is_player { 1 } else { 0 };
                let s = ModShot {
                    def: def.id.clone(),
                    owner: me,
                    team,
                    x: (hx + ox) << 8,
                    y: (hy + oy) << 8,
                    ux,
                    uy,
                    speed: def.speed,
                    t: 0,
                    target: 0,
                    hits: vec![],
                    dead: -1,
                    mult,
                    sub,
                    skill,
                };
                w.add(Body::Mod(Box::new(s)));
                if def.sfx_spawn >= 0 {
                    w.sfx(def.sfx_spawn, 64);
                }
            }
            Cmd::Anim { name, looping } => anim(w, i, &name, looping),
            Cmd::Sfx(id) => w.sfx(id, 64),
            Cmd::SfxStop(id) => w.sfx_stop(id),
            Cmd::End => ended = true,
            Cmd::SetVar(k, v) => set_var(w, me, k, v),
            Cmd::Spend(n) if is_player => crate::player::mod_spend(w, i, n),
            Cmd::Drain(n) if is_player => crate::player::mod_drain(w, i, n),
            Cmd::Spend(_) | Cmd::Drain(_) => {}
            Cmd::Teleport(x, y) => {
                // the point, else the nearest free spot around it (rings of 4 px, up to 24 px, fixed order)
                let is_enemy = matches!(w.ents[i].body, Body::Enemy(_));
                if is_enemy {
                    crate::enemy::unmark_self(w, i);
                }
                let mut spots = vec![(x, y)];
                for r in 1..=6 {
                    let d = 4 * r;
                    spots.extend([
                        (x - d, y),
                        (x + d, y),
                        (x, y - d),
                        (x, y + d),
                        (x - d, y - d),
                        (x + d, y - d),
                        (x - d, y + d),
                        (x + d, y + d),
                    ]);
                }
                for (sx, sy) in spots {
                    let ok = if is_player {
                        crate::player::mod_fits(w, i, sx, sy)
                    } else {
                        is_enemy && crate::enemy::fits(w, i, sx, sy)
                    };
                    if ok {
                        match &mut w.ents[i].body {
                            Body::Player(p) => (p.a.x, p.a.y) = (sx << 8, sy << 8),
                            Body::Enemy(e) => (e.a.x, e.a.y) = (sx << 8, sy << 8),
                            _ => {}
                        }
                        break;
                    }
                }
                if is_enemy {
                    crate::enemy::mark_self(w, i);
                }
            }
            Cmd::Strike { w: sw, h: sh, mult } => {
                let Some(a) = actor_of(w, i).cloned() else {
                    continue;
                };
                let b = a.body_box();
                let (cx, cy) = b.center();
                let rect = match a.facing & 3 {
                    3 => Rect::new(b.x1, cy - sh / 2, b.x1 + sw, cy + sh - sh / 2),
                    2 => Rect::new(b.x0 - sw, cy - sh / 2, b.x0, cy + sh - sh / 2),
                    0 => Rect::new(cx - sw / 2, b.y1, cx + sw - sw / 2, b.y1 + sh),
                    _ => Rect::new(cx - sw / 2, b.y0 - sh, cx + sw - sw / 2, b.y0),
                };
                let sub = match src {
                    Src::Skill(id) => w
                        .data
                        .skill_engine(id)
                        .and_then(|e| e.attack_sub)
                        .unwrap_or(2),
                    _ => 2,
                };
                let mut m = HitMsg {
                    team: if is_player { 1 } else { 0 },
                    rect,
                    dir: a.facing,
                    mult,
                    atk: atk(sub),
                    attacker: me,
                    att_stats: a.stats,
                    att_is_player: is_player,
                    source: me,
                    crit: false,
                    raw: 0,
                    dmg: 0,
                    no_react: false,
                    foot: false,
                };
                // (the receivers report their own `Hit` events)
                crate::world::broadcast(w, &mut m, false);
            }
            Cmd::FaceTarget => {
                let Some(a) = actor_of(w, i).cloned() else {
                    continue;
                };
                let (cx, cy) = a.center();
                let t = if is_player {
                    nearest_enemy(w, cx, cy, 0)
                } else {
                    crate::enemy::player(w, i)
                };
                if let Some(k) = t {
                    let (tx, ty) = actor_of(w, k).map(|t| t.center()).unwrap_or((cx, cy));
                    face(w, i, facing_toward(tx - cx, ty - cy));
                }
            }
            Cmd::Face(d) => face(w, i, (d & 3) as u8),
            Cmd::Goto(s) if !is_player => crate::nodes::replace_top(w, i, s as u16),
            Cmd::Push(s) if !is_player => crate::nodes::push(w, i, s as u16),
            Cmd::Pop if !is_player => crate::nodes::pop(w, i),
            Cmd::Goto(_) | Cmd::Push(_) | Cmd::Pop => {}
            Cmd::Heal(n) => {
                if matches!(w.ents[i].body, Body::Enemy(_)) {
                    crate::enemy::heal(w, i, n as i64 * 1024);
                } else if let Body::Player(p) = &mut w.ents[i].body {
                    p.rec.hp = p.rec.max_hp.min(p.rec.hp + n.max(0));
                }
            }
            Cmd::Summon { ty, x, y } => {
                let Some(a) = actor_of(w, i).cloned() else {
                    continue;
                };
                let parent = if is_player { 0 } else { me };
                crate::battle::spawn_enemy(w, ty, a.px() + x, a.py() + y, parent, None);
            }
            Cmd::Shake(d, n) => w.shake(d, n),
            Cmd::Explode(x, y) => {
                let Some(a) = actor_of(w, i).cloned() else {
                    continue;
                };
                crate::fx::explosion(w, a.px() + x, a.py() + y, 0x2000);
            }
            Cmd::Flash(n) => match &mut w.ents[i].body {
                Body::Player(p) => p.a.flash = n,
                Body::Enemy(e) => e.a.flash = n,
                _ => {}
            },
            Cmd::Hittable(b) => match &mut w.ents[i].body {
                Body::Player(p) => p.a.hittable = b,
                Body::Enemy(e) => e.a.hittable = b,
                _ => {}
            },
        }
        let _ = src == Src::Shot;
    }
    ended
}

fn face(w: &mut World, i: usize, d: u8) {
    match &mut w.ents[i].body {
        Body::Player(p) => p.a.facing = d,
        Body::Enemy(e) => e.a.facing = d,
        _ => {}
    }
}

/// an animation of the actor's own sprite by name
fn anim(w: &mut World, i: usize, name: &str, looping: bool) {
    let Some(a) = actor_of(w, i) else { return };
    let Some(sp) = a.sprite.and_then(|s| w.data.sprite(s)) else {
        return;
    };
    let Some(&k) = sp.by_name.get(name) else {
        return;
    };
    let raw = sp.anims[k as usize].raw;
    match &w.ents[i].body {
        Body::Player(_) => {
            crate::player::mod_set_anim_raw(w, i, raw);
            let id = w.ents[i].id;
            if let Some(s) = w.modk.skills.iter_mut().find(|s| s.player == id) {
                s.looping = looping;
            }
        }
        Body::Enemy(_) => crate::enemy::set_anim(w, i, &[AnimRef::N(raw - 1)], true, Some(looping)),
        _ => {}
    }
}

// ---- scripted skills ---------------------------------------------------------------------------------------------------

fn skill_script(w: &World, real: i32) -> Option<String> {
    w.data.skill_engine(real).and_then(|e| e.script.clone())
}

fn skill_ctx(w: &World, i: usize, rt: &SkillRt) -> Ctx {
    let mut c = Ctx::default();
    actor_vals(w, i, &mut c);
    c.set("skill", rt.skill);
    c.set("t", rt.t);
    c.set("held", rt.held as i32);
    c.set("released", rt.released);
    c.set("skill_level", crate::player::mod_skill_lv(w, i, rt.skill));
    let a = actor_of(w, i).cloned().unwrap_or_default();
    let (cx, cy) = a.center();
    let k = nearest_enemy(w, cx, cy, 0);
    target_vals(w, &mut c, (cx, cy), k);
    c.vars = vars_of(w, w.ents[i].id);
    c
}

fn on_cooldown(w: &World, id: EntityId, real: i32) -> bool {
    w.modk
        .cooldowns
        .iter()
        .any(|c| c.0 == id && c.1 == real && w.frame < c.2)
}

/// can_use_skill for a scripted skill: not cooling down, enough energy for `check` (else `cost`)
pub fn skill_ready(w: &World, i: usize, real: i32) -> bool {
    if on_cooldown(w, w.ents[i].id, real) {
        return false;
    }
    let lv = crate::player::mod_skill_lv(w, i, real);
    let e = w.data.skill_engine(real);
    let need = e
        .and_then(|e| crate::content::SkillEngine::pick(&e.check, lv))
        .unwrap_or_else(|| crate::player::mod_skill_cost(w, i, real));
    crate::player::mod_can_afford(w, i, need)
}

/// start_skill for a scripted skill: spend the cost, the skill state, its start animation, `on_start`
pub fn skill_start(w: &mut World, i: usize, real: i32, _lv: i32) {
    let cost = crate::player::mod_skill_cost(w, i, real);
    crate::player::mod_spend(w, i, cost);
    crate::player::mod_enter_skill(w, i, real);
    let id = w.ents[i].id;
    let e = w.data.skill_engine(real).cloned().unwrap_or_default();
    w.modk.skills.retain(|s| s.player != id);
    w.modk.skills.push(SkillRt {
        player: id,
        skill: real,
        t: 0,
        held: true,
        released: -1,
        looping: false,
        shift: e.anim_shift.unwrap_or(3).clamp(0, 6) as u8,
    });
    let names: Vec<&'static str> = if e.anim.is_empty() {
        vec!["kiblast"]
    } else {
        e.anim.clone()
    };
    crate::player::mod_set_anim(w, i, &names);
    let Some(script) = skill_script(w, real) else {
        return;
    };
    let rt = w.modk.skills.last().cloned().unwrap();
    let ctx = skill_ctx(w, i, &rt);
    if let Some((_, cmds)) = run(w, &script, "on_start", &ctx) {
        if apply(w, i, Src::Skill(real), cmds) {
            crate::player::mod_to_idle(w, i);
        }
    }
}

/// upd_skill for a scripted skill: the B hold / release, `on_release` once, `on_frame` every frame
pub fn skill_update(w: &mut World, i: usize, held_now: bool) {
    let id = w.ents[i].id;
    let Some(k) = w.modk.skills.iter().position(|s| s.player == id) else {
        return crate::player::mod_to_idle(w, i);
    };
    let released_now = {
        let s = &mut w.modk.skills[k];
        s.t += 1;
        let r = s.held && !held_now;
        if r {
            s.held = false;
            s.released = s.t;
        }
        r
    };
    let real = w.modk.skills[k].skill;
    let Some(script) = skill_script(w, real) else {
        return crate::player::mod_to_idle(w, i);
    };
    for f in [
        if released_now {
            Some("on_release")
        } else {
            None
        },
        Some("on_frame"),
    ]
    .into_iter()
    .flatten()
    {
        let Some(rt) = w.modk.skills.iter().find(|s| s.player == id).cloned() else {
            return;
        };
        let ctx = skill_ctx(w, i, &rt);
        if let Some((r, cmds)) = run(w, &script, f, &ctx) {
            let ended = apply(w, i, Src::Skill(real), cmds);
            // the hook's result 1 also ends the skill
            if ended || (f == "on_frame" && r == 1) {
                return crate::player::mod_to_idle(w, i);
            }
        }
        if crate::player::mod_energy_left(w, i) <= 0
            && w.data
                .skill_engine(real)
                .is_some_and(|e| !e.drain.is_empty())
        {
            return crate::player::mod_to_idle(w, i);
        }
    }
    // a skill without on_frame ends with its animation
    if !w
        .data
        .hooks
        .as_ref()
        .is_some_and(|h| h.has(&script, "on_frame"))
        && crate::player::mod_anim_done(w, i)
    {
        crate::player::mod_to_idle(w, i);
    }
}

/// end_skill for a scripted skill (any way out of the skill state): `on_end`, the cooldown
pub fn skill_end(w: &mut World, i: usize) {
    let id = w.ents[i].id;
    let Some(k) = w.modk.skills.iter().position(|s| s.player == id) else {
        return;
    };
    let rt = w.modk.skills.remove(k);
    if let Some(cd) = w
        .data
        .skill_engine(rt.skill)
        .and_then(|e| e.cooldown)
        .filter(|&c| c > 0)
    {
        w.modk.cooldowns.retain(|c| !(c.0 == id && c.1 == rt.skill));
        w.modk.cooldowns.push((id, rt.skill, w.frame + cd as u32));
    }
    w.modk.cooldowns.retain(|c| c.2 > w.frame);
    if let Some(script) = skill_script(w, rt.skill) {
        let ctx = skill_ctx(w, i, &rt);
        if let Some((_, cmds)) = run(w, &script, "on_end", &ctx) {
            // (already leaving the state: `End` means nothing here)
            apply(
                w,
                i,
                Src::Skill(rt.skill),
                cmds.into_iter().filter(|c| *c != Cmd::End).collect(),
            );
        }
    }
}

/// the running skill's animation mode: (looping, frames-per-cel shift)
pub fn anim_mode(w: &World, i: usize) -> (bool, i32) {
    let id = w.ents[i].id;
    w.modk
        .skills
        .iter()
        .find(|s| s.player == id)
        .map(|s| (s.looping, s.shift as i32))
        .unwrap_or((true, 3))
}

// ---- scripted AI nodes -------------------------------------------------------------------------------------------------

/// a ModCondition / ModAction node: `call` = "file.rhai:fn", `f` = frames since the action entered (0 for
/// conditions); returns the hook's result (condition: non-zero = true; action: non-zero = done)
pub fn ai_hook(w: &mut World, i: usize, call: &str, args: &[i32], f: i32) -> i32 {
    let Some((script, func)) = split_call(call) else {
        return 0;
    };
    let mut c = Ctx::default();
    actor_vals(w, i, &mut c);
    c.set("f", f);
    let a = actor_of(w, i).cloned().unwrap_or_default();
    let p = crate::enemy::player(w, i);
    target_vals(w, &mut c, a.center(), p);
    c.vars = vars_of(w, w.ents[i].id);
    c.args = args.to_vec();
    let (script, func) = (script.to_string(), func.to_string());
    match run(w, &script, &func, &c) {
        Some((r, cmds)) => {
            let ended = apply(w, i, Src::Ai, cmds);
            if ended {
                1
            } else {
                r
            }
        }
        None => 0,
    }
}

// ---- projectiles -------------------------------------------------------------------------------------------------------

fn shot(w: &World, i: usize) -> &ModShot {
    match &w.ents[i].body {
        Body::Mod(m) => m,
        _ => panic!("entity {i} is not a mod shot"),
    }
}
fn shot_mut(w: &mut World, i: usize) -> &mut ModShot {
    match &mut w.ents[i].body {
        Body::Mod(m) => m,
        _ => panic!("entity {i} is not a mod shot"),
    }
}
fn shot_rect(d: &ProjDef, s: &ModShot) -> Rect {
    centered(s.x >> 8, s.y >> 8, d.hit_box.0, d.hit_box.1)
}
fn owner_gone(w: &World, owner: EntityId) -> bool {
    match w.find(owner) {
        None => true,
        Some(k) => w.ents[k].removed || matches!(&w.ents[k].body, Body::Enemy(e) if e.defeated),
    }
}

/// hit message from the shot to every receiver under `rect` not hit before (the broadcast's order); returns the
/// receivers that accepted
fn hit(
    w: &mut World,
    i: usize,
    rect: Rect,
    mult: i32,
    limit: usize,
    explosion: bool,
) -> Vec<EntityId> {
    let s = shot(w, i).clone();
    if owner_gone(w, s.owner) && s.team != 1 {
        return vec![];
    }
    let (stats, is_player) = match w.find(s.owner) {
        Some(k) => (
            actor_of(w, k).map(|a| a.stats).unwrap_or_default(),
            matches!(w.ents[k].body, Body::Player(_)),
        ),
        None => (Default::default(), false),
    };
    let dir = facing_toward(s.ux, s.uy);
    let me = w.ents[i].id;
    let mut got = Vec::new();
    let n = w.ents.len();
    let field = w.field.is_some();
    for j in 0..n {
        let k = if field { n - 1 - j } else { j };
        if k >= w.ents.len() || w.ents[k].removed || k == i {
            continue;
        }
        let id = w.ents[k].id;
        if (!explosion && s.hits.contains(&id)) || id == s.owner {
            continue;
        }
        let mut m = HitMsg {
            team: s.team,
            rect,
            dir,
            mult,
            atk: atk(s.sub),
            attacker: s.owner,
            att_stats: stats,
            att_is_player: is_player,
            source: me,
            crit: false,
            raw: 0,
            dmg: 0,
            no_react: false,
            foot: false,
        };
        if deliver(w, k, &mut m) {
            got.push(id);
            if got.len() >= limit {
                break;
            }
        }
    }
    got
}

fn shot_hook(w: &mut World, i: usize, func: &str, other: EntityId) {
    let s = shot(w, i).clone();
    let Some(d) = w.data.projectiles.get(&s.def) else {
        return;
    };
    let script = match &d.script {
        Some(x) => x.clone(),
        None => match w.data.skill_engine(s.skill).and_then(|e| e.script.clone()) {
            Some(x) => x,
            None => return,
        },
    };
    let mut c = Ctx::default();
    c.set("shot", w.ents[i].id as i32);
    c.set("x", s.x >> 8);
    c.set("y", s.y >> 8);
    c.set("t", s.t);
    c.set("owner", s.owner as i32);
    c.set("other", other as i32);
    c.set("hits", s.hits.len() as i32);
    c.set("skill", s.skill);
    c.set("frame", w.frame as i32);
    c.vars = vars_of(w, s.owner);
    if let Some((_, cmds)) = run(w, &script, func, &c) {
        // commands act for the owner (a hit can chain: fire from the owner, sounds, variables)
        if let Some(k) = w
            .find(s.owner)
            .filter(|&k| !w.ents[k].removed && actor_of(w, k).is_some())
        {
            let src = if s.skill != 0 {
                Src::Skill(s.skill)
            } else {
                Src::Shot
            };
            apply(
                w,
                k,
                src,
                cmds.into_iter().filter(|c| *c != Cmd::End).collect(),
            );
        } else {
            for c in cmds {
                if let Cmd::Sfx(id) = c {
                    w.sfx(id, 64);
                }
            }
        }
    }
}

/// the end of a flight: explosion (area hit + effect) when `explode`, the end sound, `on_end`, the spark
fn end(w: &mut World, i: usize, explode: bool) {
    let s = shot(w, i).clone();
    let Some(d) = w.data.projectiles.get(&s.def).cloned() else {
        w.ents[i].removed = true;
        return;
    };
    shot_mut(w, i).dead = 0;
    if explode {
        if d.sfx_end >= 0 {
            w.sfx(d.sfx_end, 64);
        }
        if d.explode_radius > 0 {
            let r = centered(
                s.x >> 8,
                s.y >> 8,
                d.explode_radius * 2,
                d.explode_radius * 2,
            );
            hit(w, i, r, d.explode_damage, usize::MAX, true);
        }
        if d.explode_fx {
            crate::fx::explosion(w, s.x >> 8, s.y >> 8, 0x2000);
        }
    }
    shot_hook(w, i, "on_end", 0);
    if d.end_time <= 0 {
        w.ents[i].removed = true;
    }
}

/// one frame of a mod projectile
pub fn shot_update(w: &mut World, i: usize) {
    let s = shot(w, i).clone();
    let Some(d) = w.data.projectiles.get(&s.def).cloned() else {
        w.ents[i].removed = true;
        return;
    };
    if s.dead >= 0 {
        shot_mut(w, i).dead += 1;
        if s.dead + 1 >= d.end_time {
            w.ents[i].removed = true;
        }
        return;
    }
    let t = s.t + 1;
    let (mut ux, mut uy, mut speed, mut target) = (s.ux, s.uy, s.speed, s.target);
    speed = (speed + d.accel).clamp(0, d.max_speed.max(d.speed));
    // homing: keep / pick the nearest target in range, turn the unit vector toward it by at most `homing`
    if d.homing > 0 && t > d.homing_delay {
        let (px, py) = (s.x >> 8, s.y >> 8);
        let alive = |w: &World, id: EntityId| {
            w.find(id).is_some_and(|k| {
                !w.ents[k].removed
                    && match &w.ents[k].body {
                        Body::Enemy(e) => !e.defeated && e.a.hittable,
                        Body::Player(p) => !p.dead && p.a.hittable,
                        _ => false,
                    }
            })
        };
        if target == 0 || !alive(w, target) {
            let k = if s.team == 1 {
                nearest_enemy(w, px, py, d.homing_range)
            } else {
                nearest_player(w, px, py, d.homing_range)
            };
            target = k.map(|k| w.ents[k].id).unwrap_or(0);
        }
        if let Some(a) = w.find(target).and_then(|k| actor_of(w, k)) {
            let (tx, ty) = a.center();
            let (dx, dy) = unit(tx - px, ty - py);
            if (dx, dy) != (0, 0) {
                ux += (dx - ux).clamp(-d.homing, d.homing);
                uy += (dy - uy).clamp(-d.homing, d.homing);
                let u = unit(ux, uy);
                if u != (0, 0) {
                    (ux, uy) = u;
                }
            }
        }
    }
    let (vx, vy) = (ux * speed / 256, uy * speed / 256);
    {
        let m = shot_mut(w, i);
        m.t = t;
        m.ux = ux;
        m.uy = uy;
        m.speed = speed;
        m.target = target;
        m.x += vx;
        m.y += vy;
    }
    if let Some(sc) = &d.script {
        if w.data.hooks.as_ref().is_some_and(|h| h.has(sc, "on_frame")) {
            shot_hook(w, i, "on_frame", 0);
        }
    }
    if d.life > 0 && t > d.life {
        return end(w, i, d.explode_on_expire);
    }
    let r = shot_rect(&d, shot(w, i));
    if !d.offscreen && !w.on_any_camera(&r) {
        w.ents[i].removed = true;
        return;
    }
    let blocked = |w: &World, r: &Rect| match &w.col {
        Some(c) => !c.test_rect(Layer::Shot, r),
        None => {
            let b = w.arena.bounds;
            let (cx, cy) = r.center();
            cx < b.x0 - 8 || cx > b.x1 + 8 || cy < b.y0 - 40 || cy > b.y1 + 8
        }
    };
    if d.wall != WallMode::Pass && blocked(w, &r) {
        match d.wall {
            WallMode::Explode => return end(w, i, true),
            WallMode::Vanish => return end(w, i, false),
            _ => {
                // bounce: undo the step, reflect the blocked axis (x first, then y)
                let m = shot_mut(w, i);
                m.x -= vx;
                m.y -= vy;
                let s2 = shot(w, i).clone();
                let rx = shot_rect(
                    &d,
                    &ModShot {
                        x: s2.x + vx,
                        ..s2.clone()
                    },
                );
                let ry = shot_rect(
                    &d,
                    &ModShot {
                        y: s2.y + vy,
                        ..s2.clone()
                    },
                );
                let (bx, by) = (blocked_rect(w, &rx), blocked_rect(w, &ry));
                let m = shot_mut(w, i);
                if bx {
                    m.ux = -m.ux;
                }
                if by {
                    m.uy = -m.uy;
                }
            }
        }
    }
    let left =
        (d.pierce + 1).max(1) as usize - shot(w, i).hits.len().min((d.pierce + 1).max(1) as usize);
    if left == 0 {
        return;
    }
    let got = hit(w, i, r, shot(w, i).mult, left, false);
    if !got.is_empty() {
        if d.sfx_hit >= 0 {
            w.sfx(d.sfx_hit, 64);
        }
        for id in &got {
            shot_mut(w, i).hits.push(*id);
            shot_hook(w, i, "on_hit", *id);
        }
        if shot(w, i).hits.len() as i32 > d.pierce {
            end(w, i, true);
        }
    }
}

fn blocked_rect(w: &World, r: &Rect) -> bool {
    match &w.col {
        Some(c) => !c.test_rect(Layer::Shot, r),
        None => false,
    }
}

/// the shot's OBJ
pub fn draw(w: &World, id: u32, m: &ModShot, out: &mut wf_view::RenderView) {
    let Some(d) = w.data.projectiles.get(&m.def) else {
        return;
    };
    let dir = facing_toward(m.ux, m.uy);
    let frame = if m.dead >= 0 {
        if d.end_frames.is_empty() {
            return;
        }
        let k = ((m.dead / d.rate) as usize).min(d.end_frames.len() - 1);
        wf_view::FrameSel::Frame {
            index: d.end_frames[k],
            flip: false,
        }
    } else if let Some(a) = &d.anim {
        wf_view::FrameSel::Anim {
            anim: a.clone(),
            dir,
            t: m.t.max(0) as u32,
            looping: true,
        }
    } else {
        let list = d
            .frames
            .get(dir as usize)
            .filter(|_| d.frames.len() == 4)
            .or(d.frames.first());
        let Some(list) = list.filter(|l| !l.is_empty()) else {
            return;
        };
        wf_view::FrameSel::Frame {
            index: list[((m.t / d.rate) as usize) % list.len()],
            flip: false,
        }
    };
    out.sprites.push(wf_view::SpriteDraw {
        entity: id,
        sheet: d.sheet.clone(),
        frame,
        x: m.x >> 8,
        y: m.y >> 8,
        priority: d.priority,
        mirror: false,
        effect: wf_view::SpriteEffect::None,
    });
}
