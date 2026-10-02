//! Battle setup and the per-frame battle rules around the world update: arenas (curated: which map, spawn point,
//! fallback bounds), fighters (record at a level, stat points, equipment), enemy spawning (spawn points, free spots
//! on the enemy grid), phases, the game camera (0x080077EC), the HUD's low-HP beep and the outcome.
//! Reference: site/src/battle/rulesets/buu/index.ts (createBattle).

use crate::content::GameData;
use crate::enemy::{self, Enemy};
use crate::geom::Rect;
use crate::player::{Player, PlayerInit};
use crate::progression::{fuse, record_at_level, GrowthMode, Record};
use crate::rng::Rng;
use crate::world::{walk_region, ArenaRt, Body, Collision, EntityId, Event, World, GBA_H, GBA_W, MAX_PLAYERS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// curated arena list: map, player spawn (walkable footprint), fallback bounds when a map has no collision
pub const ARENAS: &[(&str, &str, &str, (i32, i32), [i32; 4])] = &[
    ("crater", "8-71", "Giant Crater", (96, 216), [56, 72, 424, 360]),
    ("volcano", "15-32", "Volcano Summit", (80, 216), [40, 96, 440, 376]),
    ("hfil", "18-14", "HFIL", (64, 248), [24, 80, 300, 304]),
    ("wasteland", "20-7", "Barren Wasteland", (88, 232), [40, 72, 440, 420]),
    ("demon", "8-68", "Demon World", (120, 184), [96, 76, 400, 330]),
    ("canyon", "8-70", "Rocky Canyon", (120, 280), [88, 168, 470, 400]),
    ("yakon", "8-40", "Yakon's Planet", (112, 200), [72, 92, 380, 390]),
    ("crater2", "6-16", "Giant Crater (after the blast)", (96, 216), [56, 72, 424, 360]),
    ("htc", "6-9", "Hyperbolic Time Chamber", (104, 152), [72, 64, 328, 240]),
    ("stadium", "7-6", "World Tournament Stadium", (264, 384), [232, 256, 488, 512]),
    ("forest", "8-3", "Southwest Forest", (96, 248), [40, 40, 520, 520]),
    ("plateau", "19-3", "Rocky Plateau", (240, 280), [40, 200, 660, 432]),
    ("village", "15-1", "Nataday Village", (200, 304), [96, 120, 760, 560]),
    ("kamehouse", "5-1", "Kame House island", (160, 472), [72, 368, 648, 560]),
];
fn arena_for_map(map: &str) -> Option<&'static str> {
    if map == "20-5" {
        return Some("wasteland");
    }
    ARENAS.iter().find(|a| a.1 == map).map(|a| a.0)
}

/// (id, record character, fusion parts, forms, default form)
pub const FIGHTERS: &[(&str, i32, Option<(i32, i32)>, &[i32], i32)] = &[
    ("goku", 0, None, &[2, 0, 1, 3, 4, 5, 6, 7, 8], 2),
    ("gohan", 1, None, &[10, 9, 11, 12, 13, 14, 15, 16, 17, 18], 10),
    ("goten", 2, None, &[19, 20], 19),
    ("trunks", 3, None, &[21, 22, 23, 24, 25], 21),
    ("vegeta", 4, None, &[26, 27, 28, 29, 30], 26),
    ("gotenks", 5, Some((2, 3)), &[31, 32, 33, 34, 35, 36, 37, 38], 31),
    ("vegito", 6, Some((0, 4)), &[39, 40], 39),
    ("gogeta", 7, Some((0, 4)), &[41, 42, 43, 44], 41),
    ("f46", 8, None, &[46], 46),
    ("f47", 8, None, &[47], 47),
    ("f48", 8, None, &[48], 48),
    ("f49", 8, None, &[49], 49),
    ("f50", 8, None, &[50], 50),
    ("f51", 8, None, &[51], 51),
    ("f52", 8, None, &[52], 52),
    ("f53", 8, None, &[53], 53),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FighterSetup {
    pub fighter: String,
    pub form: i32,
    pub level: i32,
    /// "rolled" | "average" | "min" | "max"
    #[serde(default = "d_growth")]
    pub growth: String,
    #[serde(default)]
    pub growth_seed: u32,
    /// "even" | "STR" | "POW" | "END" | "SPD" | "none"
    #[serde(default = "d_spend")]
    pub auto_spend: String,
    /// points on STR, POW, END, SPD
    #[serde(default)]
    pub bonus: [i32; 4],
    /// equipment ids body, hands, feet, accessory
    #[serde(default = "d_equip")]
    pub equip: [i32; 4],
    #[serde(default = "d_lv")]
    pub skill_level: i32,
    #[serde(default)]
    pub holy_water: bool,
}
fn d_growth() -> String {
    "average".into()
}
fn d_spend() -> String {
    "none".into()
}
fn d_equip() -> [i32; 4] {
    [0, 44, 77, 112]
}
fn d_lv() -> i32 {
    0
}
impl Default for FighterSetup {
    fn default() -> Self {
        FighterSetup { fighter: "goku".into(), form: 2, level: 60, growth: d_growth(), growth_seed: 0, auto_spend: d_spend(), bonus: [0; 4], equip: d_equip(), skill_level: 0, holy_water: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Phase {
    /// "t68" (type 0x44), "t111+t147" (several), "~t29" (helper, not needed for victory)
    pub enemy: String,
    pub count: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RngInit {
    /// the reference's seed convention: seed % 100000 steps from (0, 0)
    Seed(u32),
    /// explicit generator state
    State(u32, u32),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BattleConfig {
    pub arena: String,
    pub players: Vec<FighterSetup>,
    pub phases: Vec<Phase>,
    pub rng: RngInit,
    #[serde(default)]
    pub xp: bool,
}
impl Default for BattleConfig {
    fn default() -> Self {
        BattleConfig { arena: "forest".into(), players: vec![FighterSetup::default()], phases: vec![Phase { enemy: "t68".into(), count: 1 }], rng: RngInit::Seed(1), xp: false }
    }
}

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleState {
    pub phases: Vec<(Vec<(i32, bool)>, i32)>,
    pub phase: usize,
    pub next_phase_at: u32,
    pub phase_start: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Running,
    Won,
    Lost,
}

fn parse_enemy_id(s: &str) -> Vec<(i32, bool)> {
    s.split('+').filter_map(|p| p.trim_start_matches('~').trim_start_matches('t').parse::<i32>().ok().map(|t| (t, !p.starts_with('~')))).collect()
}

/// a fighter's record + skills at a level (index.ts build())
pub fn build_record(d: &GameData, f: &FighterSetup) -> (Record, Vec<i32>, Vec<i32>) {
    let fd = FIGHTERS.iter().find(|x| x.0 == f.fighter).unwrap_or(&FIGHTERS[0]);
    let mode = match f.growth.as_str() {
        "rolled" => GrowthMode::Rolled,
        "min" => GrowthMode::Min,
        "max" => GrowthMode::Max,
        _ => GrowthMode::Average,
    };
    let lv = f.level.clamp(50, 200);
    let ch = |i: i32| d.chars.iter().find(|c| c.index == i).expect("character");
    let mut rec = match fd.2 {
        Some((a, b)) => {
            let ra = record_at_level(ch(a), lv, &d.xp_table, mode, f.growth_seed);
            let rb = record_at_level(ch(b), lv, &d.xp_table, mode, f.growth_seed + 1);
            let mut r = fuse(&ra, &rb);
            r.level = ra.level.max(rb.level);
            r.points = (ra.points + rb.points) / 2;
            r
        }
        None => record_at_level(ch(fd.1), lv, &d.xp_table, mode, f.growth_seed),
    };
    let mut free = rec.points;
    let mut spent = [0i32; 4];
    for k in 0..4 {
        let n = f.bonus[k].clamp(0, free.max(0));
        spent[k] += n;
        free -= n;
    }
    match f.auto_spend.as_str() {
        "even" => {
            let mut k = 0;
            while free > 0 {
                spent[k] += 1;
                k = (k + 1) & 3;
                free -= 1;
            }
        }
        "STR" | "POW" | "END" | "SPD" => {
            let k = ["STR", "POW", "END", "SPD"].iter().position(|x| *x == f.auto_spend).unwrap();
            spent[k] += free;
            free = 0;
        }
        _ => {}
    }
    rec.str_ = 255.min(rec.str_ + spent[0]);
    rec.pow = 255.min(rec.pow + spent[1]);
    rec.end = 255.min(rec.end + spent[2]);
    rec.spd = 255.min(rec.spd + spent[3]);
    rec.points = free;
    let skills = if fd.1 == 8 { vec![7] } else { ch(fd.1).skills.clone() };
    (rec, skills, fd.3.to_vec())
}

pub fn arena_runtime(d: &GameData, id: &str) -> (ArenaRt, Option<(i32, i32)>) {
    let a = ARENAS.iter().find(|a| a.0 == id).unwrap_or(&ARENAS[0]);
    let map = d.maps.get(a.1);
    let mut bounds = Rect::new(a.4[0], a.4[1], a.4[2], a.4[3]);
    let mut region = None;
    let mut view = None;
    if let Some(m) = map {
        if let Some((mask, b)) = walk_region(m, a.3 .0, a.3 .1) {
            bounds = b;
            region = Some(Arc::new(mask));
        }
        if m.world_w > 0 {
            view = Some(Rect::new(0, 0, m.world_w, m.world_h));
        }
    }
    (ArenaRt { id: a.0.to_string(), map: a.1.to_string(), bounds, region, view }, Some(a.3))
}

/// spawn an enemy of `ty` at px (x, y); parent = 0 for none; start: None = the type's AI state, Some(-1) = no AI
pub fn spawn_enemy(w: &mut World, ty: i32, x: i32, y: i32, parent: EntityId, start: Option<i32>) -> Option<usize> {
    w.data.enemy(ty)?;
    let mut e = Enemy::new(w, ty);
    e.a.x = x << 8;
    e.a.y = y << 8;
    let id = w.add(Body::Enemy(Box::new(e)));
    let i = w.find(id).unwrap();
    enemy::clamp_inside(w, i);
    if parent == 0 {
        if let Some(c) = &w.col {
            let a = &w.en_ref(i).a;
            let region = w.arena.region.clone();
            let (fx, fy) = c.free_spot(crate::world::Layer::Enemy, a.px(), a.py(), a.foot_w, a.foot_h, Some(&w.arena.bounds), region.as_deref().map(|v| v.as_slice()));
            let e = w.en(i);
            e.a.x = fx << 8;
            e.a.y = fy << 8;
        }
    } else {
        w.en(i).parent = parent;
        if let Some((pi, _)) = w.enemy_by_id(parent) {
            w.en(pi).children.push(id);
            let t = w.en_ref(pi).target;
            w.en(i).target = t;
        }
    }
    w.enemies.push(id);
    // the enemy constructor's SetAnim(0): the idle state's enter draws its start tick (rand(255))
    enemy::set_anim(w, i, &[crate::content::AnimRef::Name("idle")], true, Some(true));
    enemy::mark_self(w, i);
    if start != Some(-1) {
        let st = start.map(|s| s as u16).unwrap_or(w.data.enemy(ty).unwrap().ai);
        let f = w.frame as i64;
        {
            // obj+0x1E8 / +0x1EC are stamped with the frame counter at creation (obj+0x210 only by Block's exit)
            let e = w.en(i);
            e.mark1 = f;
            e.mark2 = f;
        }
        crate::nodes::push(w, i, st);
    }
    Some(i)
}

/// build the world for a battle (createBattle)
pub fn create(data: Arc<GameData>, cfg: &BattleConfig) -> World {
    let rng = match cfg.rng {
        RngInit::Seed(s) => Rng::from_seed(s),
        RngInit::State(a, b) => Rng::new(a, b),
    };
    let (arena, spawn) = arena_runtime(&data, &cfg.arena);
    let mut w = World::new(Arc::clone(&data), rng, arena);
    w.opts.xp = cfg.xp;
    if let Some(m) = data.maps.get(&w.arena.map) {
        w.col = Some(Collision::new(Arc::clone(m)));
    }
    let b = w.arena.bounds;
    for (slot, f) in cfg.players.iter().enumerate().take(MAX_PLAYERS) {
        let (rec, skills, forms) = build_record(&data, f);
        let fd = FIGHTERS.iter().find(|x| x.0 == f.fighter).unwrap_or(&FIGHTERS[0]);
        let form = if forms.contains(&f.form) { f.form } else { fd.4 };
        let mut pl = Player::new(&w, PlayerInit { slot: slot as u8, form, rec, equip: f.equip, skills, skill_level: f.skill_level, sel: 0, holy_water: f.holy_water, forms_allowed: forms });
        let (sx, sy) = match (&w.col, spawn) {
            (Some(c), Some((x, y))) => {
                let region = w.arena.region.clone();
                c.free_spot(crate::world::Layer::Walk, x + 24 * slot as i32, y, pl.a.foot_w, pl.a.foot_h, Some(&b), region.as_deref().map(|v| v.as_slice()))
            }
            _ => (b.x0 + 32 + 24 * slot as i32, (b.y0 + b.y1) >> 1),
        };
        pl.a.x = sx << 8;
        pl.a.y = sy << 8;
        pl.a.facing = 3;
        let id = w.add(Body::Player(Box::new(pl)));
        w.players.push(id);
    }
    w.battle.phases = cfg.phases.iter().map(|p| (parse_enemy_id(&p.enemy), p.count)).collect();
    spawn_phase(&mut w, 0);
    // the game's object list (0x08011A1E) updates newest first and the player object is added after the area's
    // actors: the players update before the enemies placed at the start (objects spawned later update before them)
    for &pid in &w.players.clone() {
        if let Some(k) = w.ents.iter().position(|e| e.id == pid) {
            let e = w.ents.remove(k);
            w.ents.push(e);
        }
    }
    for s in 0..w.players.len() {
        camera(&mut w, s);
        w.cameras[s].init = false;
    }
    w
}

fn spawn_phase(w: &mut World, ph: usize) {
    let Some((parts, count)) = w.battle.phases.get(ph).cloned() else { return };
    let n = count.clamp(1, 12);
    let b = w.arena.bounds;
    let Some(pi) = w.player_idx(0) else { return };
    let (ppx, ppy) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    let mut k = 0;
    let nplayers = w.players.len().max(1);
    for &(ty, primary) in &parts {
        let Some(t) = w.data.enemy(ty).cloned() else { continue };
        for _ in 0..n {
            let sp = t.spawns.iter().find(|s| arena_for_map(&format!("{}-{}", s.0, s.1)) == Some(w.arena.id.as_str()) && s.2 > 0 && s.3 > 0);
            let (x, y) = match sp {
                Some(s) if n == 1 && s.2 >= b.x0 && s.2 < b.x1 && s.3 >= b.y0 && s.3 < b.y1 && (s.2 - ppx).abs() > 60 => (s.2, s.3),
                _ => {
                    let x = (b.x1 - 40).min(ppx + 120 + (k / 4) * 30);
                    let base = if w.col.is_some() { ppy } else { (b.y0 + b.y1) >> 1 };
                    // ((k % 4) - (min(n * parts, 4) - 1) / 2) * 34, rounded as the reference (Math.round)
                    let m = (n as i64 * parts.len() as i64).min(4);
                    let num = (2 * (k as i64 % 4) - (m - 1)) * 34;
                    let y = crate::fixed::round_div(base as i64 * 2 + num, 2) as i32;
                    (x, y)
                }
            };
            if let Some(ei) = spawn_enemy(w, ty, x, y, 0, None) {
                let e = w.en(ei);
                e.primary = primary && t.hp > 0;
                e.a.facing = 2;
                e.target = (k as usize % nplayers) as u8;
            }
            k += 1;
        }
    }
}

/// camera 0x080077EC: eases toward the footprint centre (screen point 120, 84) by 3/64 horizontally and 9/128
/// vertically per frame (22.10), clamped to the map
pub fn camera(w: &mut World, slot: usize) {
    if slot == 0 && crate::scene::camera(w) {
        return;
    }
    let Some(pi) = w.player_idx(slot) else { return };
    let a = &w.pl_ref(pi).a;
    let tx = ((a.px() * 2 + a.foot_w) >> 1) - 120;
    let ty = ((a.py() * 2 + a.foot_h) >> 1) - 84;
    let view = w.arena.view;
    let c = &mut w.cameras[slot];
    c.shown = if c.init { c.rect } else { Rect::default() };
    if !c.init {
        c.x = tx << 10;
        c.y = ty << 10;
        c.init = true;
    } else {
        c.x += (((tx << 10) - c.x) * 3) >> 6;
        c.y += (((ty << 10) - c.y) * 9) >> 7;
    }
    if let Some(v) = view {
        c.x = (v.x0 << 10).max(c.x.min(v.x0.max(v.x1 - GBA_W) << 10));
        c.y = (v.y0 << 10).max(c.y.min(v.y0.max(v.y1 - GBA_H) << 10));
    }
    let (cx, cy) = (c.x >> 10, c.y >> 10);
    c.rect = Rect::new(cx, cy, cx + GBA_W, cy + GBA_H);
    if c.shown == Rect::default() {
        c.shown = c.rect;
    }
}
/// lockstep: copy the emulator's camera (22.10)
pub fn set_camera(w: &mut World, slot: usize, x: i32, y: i32) {
    let c = &mut w.cameras[slot];
    c.x = x;
    c.y = y;
    c.init = true;
    let (cx, cy) = (x >> 10, y >> 10);
    c.rect = Rect::new(cx, cy, cx + GBA_W, cy + GBA_H);
    c.shown = c.rect;
}

fn live_primary(w: &World) -> Vec<EntityId> {
    w.enemies.iter().copied().filter(|&id| w.enemy_by_id(id).is_some_and(|(i, e)| e.primary && !e.defeated && !w.ents[i].removed)).collect()
}

/// before the world update: enemy grid marks follow positions set outside the enemy steps
pub fn pre_step(w: &mut World) {
    // the input object 0x03003300 (0x0804023C) records this frame's presses, stamped with the frame counter
    let at = w.g.clock(w.frame) + 1;
    for s in 0..w.players.len().clamp(1, crate::world::MAX_PLAYERS) {
        let p = w.input[s].pressed;
        w.g.note_input(s, at, p);
    }
    if w.col.is_none() {
        return;
    }
    for id in w.enemies.clone() {
        if let Some((i, _)) = w.enemy_by_id(id) {
            enemy::sync_mark(w, i);
        }
    }
}

/// after the world update: cameras, the HUD low-HP beep, phases
pub fn post_step(w: &mut World) {
    for s in 0..w.players.len() {
        camera(w, s);
    }
    if let Some(pi) = w.player_idx(0) {
        let (hp, max) = (w.pl_ref(pi).hp(), w.pl_ref(pi).max_hp());
        if hp != 0 && 5 * hp <= max && w.frame as i64 - w.g.last_beep > 60 {
            w.g.last_beep = w.frame as i64;
            w.events.push(Event::Sfx { id: 41, vol: 32 });
        }
    }
    let all_dead = w.players.iter().all(|&id| w.find(id).is_none_or(|i| w.pl_ref(i).dead));
    let nph = w.battle.phases.len();
    if live_primary(w).is_empty() && w.battle.phase + 1 < nph && !all_dead {
        if w.battle.next_phase_at == 0 {
            w.battle.next_phase_at = w.frame + 120;
            for id in w.enemies.clone() {
                if let Some((i, e)) = w.enemy_by_id(id) {
                    if !e.primary && !w.ents[i].removed && !e.defeated {
                        enemy::die(w, i, false);
                    }
                }
            }
        } else if w.frame >= w.battle.next_phase_at {
            w.battle.next_phase_at = 0;
            w.battle.phase += 1;
            w.battle.phase_start = w.frame;
            let ph = w.battle.phase;
            spawn_phase(w, ph);
        }
    }
    let keep: Vec<EntityId> = w.enemies.iter().copied().filter(|&id| w.enemy_by_id(id).is_some_and(|(i, e)| !w.ents[i].removed || e.primary) || false).collect();
    // primary enemies stay listed after removal (the reference keeps the object); removed ones are gone here
    w.enemies = keep;
}

/// Multiplayer PvP rule set (never in the original; off by default, so single-player and co-op are untouched):
/// players' attacks also hit the other players through the same message / ComputeDamage pipeline as enemy
/// attacks (`player::on_message`); the outcome becomes "last fighter standing" ([`pvp_winner`]).
pub fn set_pvp(w: &mut World, on: bool) {
    w.opts.pvp = on;
}

/// PvP: the slot of the last player standing once the match is decided (None while running / all down)
pub fn pvp_winner(w: &World) -> Option<usize> {
    if !w.opts.pvp || outcome(w) != Outcome::Won {
        return None;
    }
    w.players.iter().position(|&id| w.find(id).is_some_and(|i| !w.pl_ref(i).dead))
}

const INVULNERABLE: [i32; 2] = [0x1a, 0x1b];
const SURVIVE: u32 = 3600;

pub fn outcome(w: &World) -> Outcome {
    let players: Vec<usize> = w.players.iter().filter_map(|&id| w.find(id)).collect();
    if !players.is_empty() && players.iter().all(|&i| w.pl_ref(i).dead) {
        let ko = players.iter().map(|&i| w.pl_ref(i).ko_at).max().unwrap_or(-1);
        return if ko < 0 || w.frame as i64 - ko >= 200 { Outcome::Lost } else { Outcome::Running };
    }
    if w.opts.pvp {
        // last fighter standing (200 frames after the last KO, like the loss); enemies, if any, do not count
        let alive = players.iter().filter(|&&i| !w.pl_ref(i).dead).count();
        let ko = players.iter().map(|&i| w.pl_ref(i).ko_at).max().unwrap_or(-1);
        return if players.len() > 1 && alive <= 1 && ko >= 0 && w.frame as i64 - ko >= 200 { Outcome::Won } else { Outcome::Running };
    }
    let live = live_primary(w);
    if w.battle.phase + 1 >= w.battle.phases.len() {
        if live.is_empty() {
            return Outcome::Won;
        }
        let all_inv = live.iter().all(|&id| w.enemy_by_id(id).is_some_and(|(_, e)| INVULNERABLE.contains(&e.ty)));
        if all_inv && w.frame - w.battle.phase_start >= SURVIVE {
            return Outcome::Won;
        }
    }
    Outcome::Running
}
