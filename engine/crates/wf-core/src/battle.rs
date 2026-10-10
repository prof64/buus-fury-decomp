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
use crate::world::{
    walk_region, ArenaRt, Body, Collision, EntityId, Event, World, GBA_H, GBA_W, MAX_PLAYERS,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// curated arena list: map, player spawn (walkable footprint), fallback bounds when a map has no collision
pub const ARENAS: &[(&str, &str, &str, (i32, i32), [i32; 4])] = &[
    (
        "crater",
        "8-71",
        "Giant Crater",
        (96, 216),
        [56, 72, 424, 360],
    ),
    (
        "volcano",
        "15-32",
        "Volcano Summit",
        (80, 216),
        [40, 96, 440, 376],
    ),
    ("hfil", "18-14", "HFIL", (64, 248), [24, 80, 300, 304]),
    // (the start right of the rock pillar at (104..152, 104..208): from (88, 232) a wide beam to the right - Final
    // Flash - met its foot at once and fizzled; the corridor y 216..248 is open from x 160 to 296)
    (
        "wasteland",
        "20-7",
        "Barren Wasteland",
        (160, 232),
        [40, 72, 440, 420],
    ),
    (
        "demon",
        "8-68",
        "Demon World",
        (120, 184),
        [96, 76, 400, 330],
    ),
    (
        "canyon",
        "8-70",
        "Rocky Canyon",
        (120, 280),
        [88, 168, 470, 400],
    ),
    (
        "yakon",
        "8-40",
        "Yakon's Planet",
        (112, 200),
        [72, 92, 380, 390],
    ),
    (
        "crater2",
        "6-16",
        "Giant Crater (after the blast)",
        (96, 216),
        [56, 72, 424, 360],
    ),
    (
        "htc",
        "6-9",
        "Hyperbolic Time Chamber",
        (104, 152),
        [72, 64, 328, 240],
    ),
    (
        "stadium",
        "7-6",
        "World Tournament Stadium",
        (264, 384),
        [232, 256, 488, 512],
    ),
    (
        "forest",
        "8-3",
        "Southwest Forest",
        (96, 248),
        [40, 40, 520, 520],
    ),
    (
        "plateau",
        "19-3",
        "Rocky Plateau",
        (240, 280),
        [40, 200, 660, 432],
    ),
    (
        "village",
        "15-1",
        "Nataday Village",
        (200, 304),
        [96, 120, 760, 560],
    ),
    (
        "kamehouse",
        "5-1",
        "Kame House island",
        (160, 472),
        [72, 368, 648, 560],
    ),
    // the Worm Head's own place (Stomach 21-9, a corridor walkable 40..192 x 32..400): the worm only wanders (rand(200)
    // turns, a wall turns it) and hits by contact - in the game it meets the player because the corridor holds it; in
    // the open Giant Crater it walked away and never fought (playtest #37). The start is screen-near the story's worm
    // at (152, 248), which `spawn_phase` then uses.
    (
        "stomach",
        "21-9",
        "Buu's Stomach",
        (88, 296),
        [40, 32, 192, 400],
    ),
    // the link-play Dojo (title -> Multiplayer -> Dojo; area 0-90, its walled room: walkable 24..296 x 80..304)
    ("dojo", "0-90", "Dojo", (120, 176), [24, 80, 296, 304]),
];
fn arena_for_map<'a>(d: &'a GameData, map: &str) -> Option<&'a str> {
    if map == "20-5" {
        return Some("wasteland");
    }
    d.arenas
        .iter()
        .find(|a| a.map == map)
        .map(|a| a.id.as_str())
}

/// (id, record character, fusion parts, forms, default form)
pub const FIGHTERS: &[(&str, i32, Option<(i32, i32)>, &[i32], i32)] = &[
    ("goku", 0, None, &[2, 0, 1, 3, 4, 5, 6, 7, 8], 2),
    (
        "gohan",
        1,
        None,
        &[10, 9, 11, 12, 13, 14, 15, 16, 17, 18],
        10,
    ),
    ("goten", 2, None, &[19, 20], 19),
    ("trunks", 3, None, &[21, 22, 23, 24, 25], 21),
    ("vegeta", 4, None, &[26, 27, 28, 29, 30], 26),
    (
        "gotenks",
        5,
        Some((2, 3)),
        &[31, 32, 33, 34, 35, 36, 37, 38],
        31,
    ),
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
    /// start a Super Saiyan / SS3 form with its gauge full (= max EP), as every way into the form leaves it in the
    /// game (the transformation's end fills it, `player::finish_transform`; the record keeps it - run 1's Kid Buu
    /// fight: Goku's SS and SS3 gauges 1434 = max EP). Off (the reference's and the lockstep scenarios' setup): the
    /// gauge starts at 0 - every skill of the form is greyed out (its cost is paid from the gauge) and a draining
    /// form reverts within a few frames.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub full_gauge: bool,
    /// the skill list (menu order) instead of the fighter's / character's new-game one: the skills the story has
    /// given by then (the launcher: Super Saiyan 3 learned in events #27 / #228 / #233, the Saiyaman skill of Teen
    /// Gohan); None = the new-game list. A fixed record's own list still wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<i32>>,
    /// the opponents come for the players from the start (`Options::engaged`, [`engaged_state`]): a story encounter
    /// already begun. Off (the reference's and the lockstep scenarios' setup): they wait until a player comes near.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub engage: bool,
    /// a record given as is (link play's Dojo): replaces what the level / growth / points give (`FixedRecord`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<FixedRecord>,
}

/// A fighter record given as is - the link Dojo's fighters (ROM: the Dojo fight's record builders, `0x0801339C` from
/// the Balanced table `0x0872BFCC` / `0x080229B0` from a save's packed record): HP and EP (current = max), STR, POW,
/// END, SPD, and (optionally) the skill list.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FixedRecord {
    pub hp: i32,
    pub ep: i32,
    /// STR, POW, END, SPD
    pub stats: [i32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<i32>>,
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
        FighterSetup {
            fighter: "goku".into(),
            form: 2,
            level: 60,
            growth: d_growth(),
            growth_seed: 0,
            auto_spend: d_spend(),
            bonus: [0; 4],
            equip: d_equip(),
            skill_level: 0,
            holy_water: false,
            full_gauge: false,
            skills: None,
            engage: false,
            fixed: None,
        }
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
    /// the opponents start in view: each phase's enemies `gap` px in front of the player (in formation as usual),
    /// a story spawn point only when it is that close (the launcher's battles; None = the reference's placement:
    /// the enemy's own spawn point in its arena, else 120 px in front - TS mirrors that)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_near: Option<i32>,
    /// Battle mode's arena rules (`Options::arena`): a plain arena fighter, not the story (the launcher's battles).
    /// Off: the game's own rules (the reference, the lockstep and cross-check scenarios).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub arena_rules: bool,
    /// the arena rules' even match is set for the players at this level (the launcher: the opponent's level - a
    /// fighter set higher or lower than it is that much stronger or weaker); None = the players' own level
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub even_level: Option<i32>,
}
impl Default for BattleConfig {
    fn default() -> Self {
        BattleConfig {
            arena: "forest".into(),
            players: vec![FighterSetup::default()],
            phases: vec![Phase {
                enemy: "t68".into(),
                count: 1,
            }],
            rng: RngInit::Seed(1),
            xp: false,
            spawn_near: None,
            arena_rules: false,
            even_level: None,
        }
    }
}

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleState {
    pub phases: Vec<(Vec<(i32, bool)>, i32)>,
    pub phase: usize,
    pub next_phase_at: u32,
    pub phase_start: u32,
    /// the damage numbers on show (the field keeps them with its sprite-memory model, `vram::damage_number_shown`;
    /// a battle has no memory limit): (target, frame made, end frame, x8, y8, damage in 1/1024 HP, crit, its place in
    /// the world list - the entity id the next object got when it was made)
    #[serde(default)]
    pub popups: Vec<(EntityId, u32, u32, i32, i32, i64, bool, EntityId)>,
    /// the entity the object pass is updating (0 outside it): a number older than that object has had this frame's
    /// update when it is restarted (`vram::damage_number_shown`'s `pending`, as the field's)
    #[serde(default)]
    pub pass_at: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Running,
    Won,
    Lost,
}

fn parse_enemy_id(s: &str) -> Vec<(i32, bool)> {
    s.split('+')
        .filter_map(|p| {
            p.trim_start_matches('~')
                .trim_start_matches('t')
                .parse::<i32>()
                .ok()
                .map(|t| (t, !p.starts_with('~')))
        })
        .collect()
}

/// a fighter's record + skills at a level (index.ts build())
pub fn build_record(d: &GameData, f: &FighterSetup) -> (Record, Vec<i32>, Vec<i32>) {
    let fd = d.fighter(&f.fighter).unwrap_or(&d.fighters[0]);
    let mode = match f.growth.as_str() {
        "rolled" => GrowthMode::Rolled,
        "min" => GrowthMode::Min,
        "max" => GrowthMode::Max,
        _ => GrowthMode::Average,
    };
    let lv = f.level.clamp(50, 200);
    let ch = |i: i32| d.chars.iter().find(|c| c.index == i).expect("character");
    let mut rec = match fd.fusion {
        Some((a, b)) => {
            let ra = record_at_level(ch(a), lv, &d.xp_table, mode, f.growth_seed);
            let rb = record_at_level(ch(b), lv, &d.xp_table, mode, f.growth_seed + 1);
            let mut r = fuse(&ra, &rb);
            r.level = ra.level.max(rb.level);
            r.points = (ra.points + rb.points) / 2;
            r
        }
        None => record_at_level(ch(fd.character), lv, &d.xp_table, mode, f.growth_seed),
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
            let k = ["STR", "POW", "END", "SPD"]
                .iter()
                .position(|x| *x == f.auto_spend)
                .unwrap();
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
    let mut skills = f.skills.clone().unwrap_or_else(|| {
        fd.skills
            .clone()
            .unwrap_or_else(|| ch(fd.character).skills.clone())
    });
    if let Some(fx) = &f.fixed {
        rec.hp = fx.hp;
        rec.max_hp = fx.hp;
        rec.ep = fx.ep;
        rec.max_ep = fx.ep;
        rec.str_ = fx.stats[0];
        rec.pow = fx.stats[1];
        rec.end = fx.stats[2];
        rec.spd = fx.stats[3];
        rec.points = 0;
        if let Some(s) = &fx.skills {
            skills = s.clone();
        }
    }
    (rec, skills, fd.forms.clone())
}

pub fn arena_runtime(d: &GameData, id: &str) -> (ArenaRt, Option<(i32, i32)>) {
    let a = d.arena(id).unwrap_or(&d.arenas[0]);
    let map = d.maps.get(&a.map);
    let mut bounds = Rect::new(a.bounds[0], a.bounds[1], a.bounds[2], a.bounds[3]);
    let mut region = None;
    let mut view = None;
    if let Some(m) = map {
        if let Some((mask, b)) = walk_region(m, a.spawn.0, a.spawn.1) {
            bounds = b;
            region = Some(Arc::new(mask));
        }
        if m.world_w > 0 {
            view = Some(Rect::new(0, 0, m.world_w, m.world_h));
        }
    }
    (
        ArenaRt {
            id: a.id.clone(),
            map: a.map.clone(),
            bounds,
            region,
            view,
        },
        Some(a.spawn),
    )
}

/// spawn an enemy of `ty` at px (x, y); parent = 0 for none; start: None = the type's AI state, Some(-1) = no AI
/// a new enemy's first AI state (its constructor's push); returns the enemy's index after it. Objects the state's
/// enter creates (Worm Head's segments) are appended to the world list before the constructor's caller appends the
/// enemy itself (0x0802CBD4 appends each segment via 0x0801191A; VERIFIED 21-9's list: segments 0..4, then the head),
/// so the enemy moves after them (the list updates from its end: the head first)
pub fn push_initial(w: &mut World, i: usize, st: u16) -> usize {
    let before = w.ents.len();
    crate::nodes::push(w, i, st);
    if w.ents.len() > before {
        let e = w.ents.remove(i);
        w.ents.push(e);
        return w.ents.len() - 1;
    }
    i
}

pub fn spawn_enemy(
    w: &mut World,
    ty: i32,
    x: i32,
    y: i32,
    parent: EntityId,
    start: Option<i32>,
) -> Option<usize> {
    spawn_enemy_at(w, ty, x, y, parent, start, false)
}

/// spawn_enemy; `exact`: at (x, y) as given, no free-spot search (the field's placed enemies: the constructor
/// 0x0800FD55 takes the def position as it is, VERIFIED Babidi's ship 8-12 / 8-21 / 8-42, where a def overlaps a
/// marked tile)
pub fn spawn_enemy_at(
    w: &mut World,
    ty: i32,
    x: i32,
    y: i32,
    parent: EntityId,
    start: Option<i32>,
    exact: bool,
) -> Option<usize> {
    w.data.enemy(ty)?;
    let mut e = Enemy::new(w, ty);
    e.a.x = x << 8;
    e.a.y = y << 8;
    let id = w.add(Body::Enemy(Box::new(e)));
    let i = w.find(id).unwrap();
    crate::vram::actor_created(w, i);
    enemy::clamp_inside(w, i);
    if parent == 0 && !exact {
        if let Some(c) = &w.col {
            let a = &w.en_ref(i).a;
            let region = w.arena.region.clone();
            let (fx, fy) = c.free_spot(
                crate::world::Layer::Enemy,
                a.px(),
                a.py(),
                a.foot_w,
                a.foot_h,
                Some(&w.arena.bounds),
                region.as_deref().map(|v| v.as_slice()),
            );
            let e = w.en(i);
            e.a.x = fx << 8;
            e.a.y = fy << 8;
        }
    } else if parent != 0 {
        w.en(i).parent = parent;
        if let Some((pi, _)) = w.enemy_by_id(parent) {
            w.en(pi).children.push(id);
            let t = w.en_ref(pi).target;
            w.en(i).target = t;
        }
    }
    w.enemies.push(id);
    // the enemy constructor's SetAnim(0): the idle state's enter draws its start tick (rand(255))
    enemy::set_anim(
        w,
        i,
        &[crate::content::AnimRef::Name("idle")],
        true,
        Some(true),
    );
    enemy::mark_self(w, i);
    if start != Some(-1) {
        let st = start
            .map(|s| s as u16)
            .unwrap_or(w.data.enemy(ty).unwrap().ai);
        let f = w.g.clock(w.frame);
        {
            // obj+0x1E8 / +0x1EC are stamped with the frame counter at creation (obj+0x210 only by Block's exit)
            let e = w.en(i);
            e.mark1 = f;
            e.mark2 = f;
        }
        return Some(push_initial(w, i, st));
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
    w.opts.spawn_near = cfg.spawn_near;
    w.opts.arena = cfg.arena_rules;
    w.opts.engaged = cfg.players.iter().any(|f| f.engage);
    if let Some(m) = data.maps.get(&w.arena.map) {
        w.col = Some(Collision::new(Arc::clone(m)));
    }
    let b = w.arena.bounds;
    for (slot, f) in cfg.players.iter().enumerate().take(MAX_PLAYERS) {
        let (rec, skills, forms) = build_record(&data, f);
        let fd = data.fighter(&f.fighter).unwrap_or(&data.fighters[0]);
        let form = if forms.contains(&f.form) {
            f.form
        } else {
            fd.default_form
        };
        let mut pl = Player::new(
            &w,
            PlayerInit {
                slot: slot as u8,
                form,
                rec,
                equip: f.equip,
                skills,
                skill_level: f.skill_level,
                sel: 0,
                holy_water: f.holy_water,
                forms_allowed: forms,
            },
        );
        // the story's unstable Trunks (forms 24 / 25): the form object's countdown is set when the object is made
        // (`player::new_form_object`: 24 -> 120 + rand(180) frames to Super Saiyan, 25 -> 240 + rand(120) back); a
        // battle that starts in one makes it with the player (it was 0: the transformation came on the first frame)
        match data.form(form).map(|x| x.controller) {
            Some(0x3001b88) => pl.story_cnt = 120 + w.rng.rand(180),
            Some(0x3001b8c) => pl.story_cnt = 240 + w.rng.rand(120),
            _ => {}
        }
        if f.full_gauge {
            let (ss, ss3) = data.form(form).map(|x| (x.ss, x.ss3)).unwrap_or((false, false));
            if ss3 {
                pl.ss3_gauge = pl.rec.max_ep as i64 * 65536;
            } else if ss {
                pl.ss_gauge = pl.rec.max_ep;
            }
        }
        let (sx, sy) = match (&w.col, spawn) {
            (Some(c), Some((x, y))) => {
                let region = w.arena.region.clone();
                c.free_spot(
                    crate::world::Layer::Walk,
                    x + 24 * slot as i32,
                    y,
                    pl.a.foot_w,
                    pl.a.foot_h,
                    Some(&b),
                    region.as_deref().map(|v| v.as_slice()),
                )
            }
            _ => (b.x0 + 32 + 24 * slot as i32, (b.y0 + b.y1) >> 1),
        };
        pl.a.x = sx << 8;
        pl.a.y = sy << 8;
        pl.a.facing = 3;
        let id = w.add(Body::Player(Box::new(pl)));
        w.players.push(id);
    }
    w.battle.phases = cfg
        .phases
        .iter()
        .map(|p| (parse_enemy_id(&p.enemy), p.count))
        .collect();
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
    if cfg.arena_rules {
        w.opts.arena_scale = arena_scale(&w, cfg);
    }
    w
}

/// Battle mode's even match (arena rules): the story's opponents were made for a player with the story's equipment,
/// items and skills - at the same level a boss took 100+ punches and KO'd the fighter in 6 - 10 hits (Super Buu with
/// Gohan absorbed: in one). Every hit is scaled so a standard hit of each side takes a fixed share of the other's HP:
/// the fighter's standard hit (its stronger of a STR punch or a POW ki hit, x1) 1/16 of the opponent's, the opponent's standard hit (its stronger of a STR or
/// a POW hit, x1) 1/16 of the fighter's - measured with the fighter at `even_level` (the opponent's level), so a
/// fighter set above or below it keeps the difference. Its special attacks keep their weight over that hit, up to
/// [`ARENA_HIT_CAP`]. [damage to the enemies, to the players] in 1/256.
pub fn arena_scale(w: &World, cfg: &BattleConfig) -> [i32; 2] {
    use crate::damage::{compute_damage_raw, Draw};
    let Some((ei, e)) = live_primary(w).first().and_then(|&id| w.enemy_by_id(id)) else {
        return [256, 256];
    };
    let _ = ei;
    let (es, ehp) = (e.a.stats, e.max_hp1024.max(1024));
    let Some(f0) = cfg.players.first() else { return [256, 256] };
    let mut f = f0.clone();
    if let Some(lv) = cfg.even_level {
        f.level = lv;
        f.fixed = None;
    }
    let (rec, skills, forms) = build_record(&w.data, &f);
    let php = (rec.max_hp as i64 * 1024).max(1024);
    let fd = w.data.fighter(&f.fighter).unwrap_or(&w.data.fighters[0]);
    let form = if forms.contains(&f.form) { f.form } else { fd.default_form };
    let pl = Player::new(
        w,
        PlayerInit { slot: 0, form, rec, equip: f.equip, skills, skill_level: f.skill_level, sel: 0, holy_water: f.holy_water, forms_allowed: forms },
    );
    let ps = pl.a.stats;
    // the mean hit: no crit, the middle of rand(26)
    let hit = |kind: u8, sub: u8, s: i32, cb: i32, d: i32, res: i32| {
        compute_damage_raw(kind, sub, 1024, 0x400, s as i64, cb as i64, d as i64, res as i64, false, 0, &mut |dr| match dr {
            Draw::Chance(_) => 0,
            Draw::Rand(_) => 13,
        })
        .dmg
        .max(1)
    };
    // (the fighter's stronger of a punch and a ki hit: some opponents take nothing from punches - armour, resist 0)
    let to_enemy = hit(0, 0, ps.str_, ps.crit_bonus, es.end, es.res_m).max(hit(1, 1, ps.pow, 0, es.end2, es.res_e));
    let to_player = hit(0, 0, es.str_, es.crit_bonus, ps.end, ps.res_m).max(hit(1, 1, es.pow, 0, ps.end2, ps.res_e));
    let scale = |hp: i64, share: i64, d: i64| ((hp / share) * 256 / d).clamp(8, 256 * 64) as i32;
    [scale(ehp, 16, to_enemy), scale(php, 16, to_player)]
}

/// arena rules: no single hit takes more than a tenth of a fighter's max HP (no one-hit KO, either way); an
/// opponent's guard lets a quarter of the hit through
pub const ARENA_HIT_CAP: i64 = 10;

/// A hit's damage under the arena rules ([`arena_scale`], [`ARENA_HIT_CAP`]); the game's damage otherwise.
/// `to_player`: the target is a player hit by an enemy (else an enemy hit by a player); `max_hp1024` the target's.
pub fn arena_damage(w: &World, mut r: crate::damage::DamageResult, to_player: bool, max_hp1024: i64) -> crate::damage::DamageResult {
    if !w.opts.arena {
        return r;
    }
    let s = w.opts.arena_scale[to_player as usize] as i64;
    let cap = (max_hp1024 / ARENA_HIT_CAP).max(1024);
    // an opponent's guard takes at most three quarters of a hit (Yakon guards most hits: 1/16 of each got through -
    // ~1.3% of his HP per hit, a fight with no end); the fighter's own guard is the game's
    if !to_player && r.dmg < r.unblocked / 4 {
        r.dmg = r.unblocked / 4;
    }
    let f = |d: i64| if d <= 0 { d } else { ((d * s) >> 8).clamp(1, cap) };
    r.dmg = f(r.dmg);
    r.unblocked = f(r.unblocked);
    r
}

/// The state an opponent's AI starts in when the battle is engaged ([`FighterSetup::engage`]): the state its "wait
/// for the player" state goes to. Those initial states are `Selector[IfPlayerWithin | IfPlayerWithin2 (r) ->
/// GotoState(S), RunMotionController]` - the enemy stands still until a player comes within r (Kid Buu, Majin
/// Fighters 100 px, Super Janemba / Majin Buu 72 px; in the story the player walks up to it); an engaged battle
/// starts as if that had just happened. Any other initial state (spawning parts, timers) is kept: None.
pub fn engaged_state(d: &GameData, init: u16) -> Option<u16> {
    use crate::ai::{K, P};
    let t = d.states.get(&init)?;
    let root = t.nodes.first()?;
    if root.k != K::Selector || root.kids.len() != 2 {
        return None;
    }
    let near = t.nodes.get(root.kids[0] as usize)?;
    let wait = t.nodes.get(root.kids[1] as usize)?;
    if !matches!(near.k, K::IfPlayerWithin | K::IfPlayerWithin2) || wait.k != K::RunMotionController || near.kids.len() != 1 {
        return None;
    }
    let go = t.nodes.get(near.kids[0] as usize)?;
    (go.k == K::GotoState).then(|| go.p.g(P::State) as u16)
}

/// Battle mode's arena rules: [`engaged_state`] for every opponent that waits for the player - through the initial
/// `GotoState` hops (Annihilator, Assassin, Goon ... start in a state that only jumps to the waiting one) and with any
/// fallback beside the `IfPlayerWithin` branch (Destroyer's `FacePlayer`, Hooligan's `StandStill`, Knight Destroyer's
/// sub-state): they come for the player from the start instead of standing until he walks up
pub fn arena_engaged_state(d: &GameData, init: u16) -> Option<u16> {
    use crate::ai::{K, P};
    let mut st = init;
    for _ in 0..4 {
        let t = d.states.get(&st)?;
        let root = t.nodes.first()?;
        if root.k == K::GotoState {
            st = root.p.g(P::State) as u16;
            continue;
        }
        if root.k != K::Selector {
            return None;
        }
        for &k in &root.kids {
            let near = t.nodes.get(k as usize)?;
            if matches!(near.k, K::IfPlayerWithin | K::IfPlayerWithin2) && near.kids.len() == 1 {
                let go = t.nodes.get(near.kids[0] as usize)?;
                return (go.k == K::GotoState).then(|| go.p.g(P::State) as u16);
            }
        }
        return None;
    }
    None
}

fn spawn_phase(w: &mut World, ph: usize) {
    let Some((parts, count)) = w.battle.phases.get(ph).cloned() else {
        return;
    };
    let n = count.clamp(1, 12);
    let b = w.arena.bounds;
    let Some(pi) = w.player_idx(0) else { return };
    let (ppx, ppy) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    let mut k = 0;
    let nplayers = w.players.len().max(1);
    for &(ty, primary) in &parts {
        let Some(t) = w.data.enemy(ty).cloned() else {
            continue;
        };
        for _ in 0..n {
            let sp = t.spawns.iter().find(|s| {
                arena_for_map(&w.data, &format!("{}-{}", s.0, s.1)) == Some(w.arena.id.as_str())
                    && s.2 > 0
                    && s.3 > 0
            });
            let gap = w.opts.spawn_near;
            // (spawn_near: a story point only when it is in view of the player's screen)
            let near = |s: &(i32, i32, i32, i32)| gap.is_none_or(|g| (s.2 - ppx).abs() <= g + 24 && (s.3 - ppy).abs() <= 56);
            let (x, y) = match sp {
                Some(s)
                    if n == 1
                        && s.2 >= b.x0
                        && s.2 < b.x1
                        && s.3 >= b.y0
                        && s.3 < b.y1
                        && (s.2 - ppx).abs() > 60
                        && near(s) =>
                {
                    (s.2, s.3)
                }
                _ => {
                    let x = (b.x1 - 40).min(ppx + gap.unwrap_or(120) + (k / 4) * 30);
                    let base = if w.col.is_some() {
                        ppy
                    } else {
                        (b.y0 + b.y1) >> 1
                    };
                    // ((k % 4) - (min(n * parts, 4) - 1) / 2) * 34, rounded as the reference (Math.round)
                    let m = (n as i64 * parts.len() as i64).min(4);
                    let num = (2 * (k as i64 % 4) - (m - 1)) * 34;
                    let y = crate::fixed::round_div(base as i64 * 2 + num, 2) as i32;
                    (x, y)
                }
            };
            let start = if w.opts.engaged && w.opts.arena {
                arena_engaged_state(&w.data, t.ai).map(|s| s as i32)
            } else if w.opts.engaged {
                engaged_state(&w.data, t.ai).map(|s| s as i32)
            } else {
                None
            };
            if let Some(ei) = spawn_enemy(w, ty, x, y, 0, start) {
                let e = w.en(ei);
                e.primary = primary && t.hp > 0;
                // facing the player (left) - unless the sprite has no body facing left: Janemba, Blister, Enzymes,
                // blood cells and Buu fragments have a body box facing down only (the way the game shows them), and
                // turned left nothing could hit them (playtest: Janemba never took damage). They keep their own facing.
                let lb = e.a.body[2];
                if lb[0] < lb[2] && lb[1] < lb[3] {
                    e.a.facing = 2;
                }
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
    // a screen shake replaces the camera controller (the shake object 0x0804A720 takes the mode's camera slot): the
    // follow does not run while it lasts, nor in the frame it ended (as the field's `field::camera`; TS mirrors it)
    let shaking = w.camera_held_by_shake();
    let c = &mut w.cameras[slot];
    c.shown = if c.init { c.rect } else { Rect::default() };
    c.shown_fine = (c.x, c.y);
    if !c.init {
        c.x = tx << 10;
        c.y = ty << 10;
        c.init = true;
    } else if shaking {
    } else {
        c.x += (((tx << 10) - c.x) * 3) >> 6;
        c.y += (((ty << 10) - c.y) * 9) >> 7;
    }
    // (the clamp 0x08009744 as the field's `clamp_cam`: past the last whole pixel the position snaps to it, a
    // position on it keeps its fraction - the next ease and the OBJs' shake rounding (`view::obj_shake_baked`) see
    // it; TS mirrors it)
    if let Some(v) = view {
        let clamp = |p: i32, lo: i32, hi: i32| if p < lo << 10 { lo << 10 } else if (p >> 10) > hi { hi << 10 } else { p };
        c.x = clamp(c.x, v.x0, v.x0.max(v.x1 - GBA_W));
        c.y = clamp(c.y, v.y0, v.y0.max(v.y1 - GBA_H));
    }
    let (cx, cy) = (c.x >> 10, c.y >> 10);
    c.rect = Rect::new(cx, cy, cx + GBA_W, cy + GBA_H);
    if c.shown == Rect::default() {
        c.shown = c.rect;
        c.shown_fine = (c.x, c.y);
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
    c.shown_fine = (c.x, c.y);
}

fn live_primary(w: &World) -> Vec<EntityId> {
    w.enemies
        .iter()
        .copied()
        .filter(|&id| {
            w.enemy_by_id(id)
                .is_some_and(|(i, e)| e.primary && !e.defeated && !w.ents[i].removed)
        })
        .collect()
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
    // the task list (screen shakes) after the whole object pass, before the camera (as the field's step_post: the
    // OBJs take the new offset, the BG layers the one before - `view::render_view`); the TS reference mirrors it
    w.tick_shakes();
    for s in 0..w.players.len() {
        camera(w, s);
    }
    let now = w.frame;
    w.battle.popups.retain(|p| p.2 > now);
    if let Some(pi) = w.player_idx(0) {
        let (hp, max) = (w.pl_ref(pi).hp(), w.pl_ref(pi).max_hp());
        if hp != 0 && 5 * hp <= max && w.frame as i64 - w.g.last_beep > 60 {
            w.g.last_beep = w.frame as i64;
            w.events.push(Event::Sfx { id: 41, vol: 32 });
        }
    }
    let all_dead = w
        .players
        .iter()
        .all(|&id| w.find(id).is_none_or(|i| w.pl_ref(i).dead));
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
    let keep: Vec<EntityId> = w
        .enemies
        .iter()
        .copied()
        .filter(|&id| {
            w.enemy_by_id(id)
                .is_some_and(|(i, e)| !w.ents[i].removed || e.primary)
                || false
        })
        .collect();
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
    w.players
        .iter()
        .position(|&id| w.find(id).is_some_and(|i| !w.pl_ref(i).dead))
}

const INVULNERABLE: [i32; 2] = [0x1a, 0x1b];
/// Battle mode's arena rules: frames from the last player's KO to the loss (the KO animation, then the end screen)
pub const ARENA_KO: i64 = 120;
const SURVIVE: u32 = 3600;

/// A survive fight's frames left: every live opponent invulnerable (Majin Buu under Babidi's spell, 0x1A / 0x1B -
/// the story's encounters the player cannot win), [`outcome`]'s win after `SURVIVE` frames of the phase. None in
/// any other fight (and in the field). The view shows it as the game's countdown display (`RenderView::timer`).
pub fn survive_left(w: &World) -> Option<u32> {
    if w.field.is_some() || w.opts.pvp || w.battle.phase + 1 < w.battle.phases.len() {
        return None;
    }
    let live = live_primary(w);
    let all_inv = !live.is_empty()
        && live.iter().all(|&id| {
            w.enemy_by_id(id)
                .is_some_and(|(_, e)| INVULNERABLE.contains(&e.ty))
        });
    all_inv.then(|| SURVIVE.saturating_sub(w.frame.wrapping_sub(w.battle.phase_start)))
}

pub fn outcome(w: &World) -> Outcome {
    let players: Vec<usize> = w.players.iter().filter_map(|&id| w.find(id)).collect();
    if !players.is_empty() && players.iter().all(|&i| w.pl_ref(i).dead) {
        let ko = players
            .iter()
            .map(|&i| w.pl_ref(i).ko_at)
            .max()
            .unwrap_or(-1);
        // (the game's death script takes over at KO + 199; Battle mode's arena rules: lost once the KO pose has
        // played, 120 frames - the end screen follows)
        let after = if w.opts.arena { ARENA_KO } else { 200 };
        return if ko < 0 || w.frame as i64 - ko >= after {
            Outcome::Lost
        } else {
            Outcome::Running
        };
    }
    if w.opts.pvp {
        // last fighter standing (200 frames after the last KO, like the loss); enemies, if any, do not count
        let alive = players.iter().filter(|&&i| !w.pl_ref(i).dead).count();
        let ko = players
            .iter()
            .map(|&i| w.pl_ref(i).ko_at)
            .max()
            .unwrap_or(-1);
        return if players.len() > 1 && alive <= 1 && ko >= 0 && w.frame as i64 - ko >= 200 {
            Outcome::Won
        } else {
            Outcome::Running
        };
    }
    let live = live_primary(w);
    if w.battle.phase + 1 >= w.battle.phases.len() {
        if live.is_empty() {
            return Outcome::Won;
        }
        let all_inv = live.iter().all(|&id| {
            w.enemy_by_id(id)
                .is_some_and(|(_, e)| INVULNERABLE.contains(&e.ty))
        });
        if all_inv && w.frame - w.battle.phase_start >= SURVIVE {
            return Outcome::Won;
        }
    }
    Outcome::Running
}
