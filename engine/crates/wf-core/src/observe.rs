//! Structured observations for agents (serde-serializable, stable field names). Read-only: built from `&World`.

use crate::world::{Body, World};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ActorObs {
    pub id: u32,
    pub kind: &'static str,
    /// position in px (footprint top-left) and the 24.8 raw values
    pub x: i32,
    pub y: i32,
    pub x256: i32,
    pub y256: i32,
    pub z: i32,
    pub facing: u8,
    pub body: [i32; 4],
    pub foot: [i32; 4],
    pub hittable: bool,
    pub blocking: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlayerObs {
    pub slot: u8,
    pub actor: ActorObs,
    pub hp: i32,
    pub hp_max: i32,
    pub ep: i32,
    pub ep_max: i32,
    pub form: i32,
    pub state: String,
    pub coarse: &'static str,
    pub level: i32,
    pub skills: Vec<i32>,
    pub selected_skill: i32,
    pub ss_gauge: i32,
    pub ss3_gauge: i64,
    pub ss_cooldown: i32,
    pub dead: bool,
    pub held_by: u32,
    pub wraps: i32,
    /// skill instance: id, frames since start
    pub skill: Option<(i32, i32)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EnemyObs {
    pub actor: ActorObs,
    pub ty: i32,
    pub name: String,
    pub hp: i64,
    pub hp_max: i64,
    pub defeated: bool,
    pub primary: bool,
    pub stunned: bool,
    /// AI stack (state ids, top last)
    pub ai: Vec<u16>,
    /// frames since the current AI state started; cooldown marks
    pub in_state: i64,
    pub target: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShotObs {
    pub id: u32,
    pub kind: &'static str,
    pub owner: u32,
    pub team: u8,
    pub dir: u8,
    pub x: i32,
    pub y: i32,
    pub rect: [i32; 4],
    pub t: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ArenaObs {
    pub id: String,
    pub map: String,
    pub bounds: [i32; 4],
    /// walk grid around the player camera: 8 px tiles, row-major, '1' passable / '0' blocked, origin (x0, y0) px
    pub walk_window: String,
    pub window_origin: (i32, i32),
    pub window_tiles: (i32, i32),
}

#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub frame: u32,
    pub me: Option<PlayerObs>,
    pub players: Vec<PlayerObs>,
    pub enemies: Vec<EnemyObs>,
    pub projectiles: Vec<ShotObs>,
    pub rocks: Vec<[i32; 4]>,
    pub camera: [i32; 4],
    pub arena: ArenaObs,
    pub rng: [u32; 2],
    pub candy: i32,
}

fn actor_obs(id: u32, kind: &'static str, a: &crate::actor::Actor) -> ActorObs {
    let b = a.body_box();
    let f = a.foot_box();
    ActorObs { id, kind, x: a.px(), y: a.py(), x256: a.x, y256: a.y, z: a.z, facing: a.facing, body: [b.x0, b.y0, b.x1, b.y1], foot: [f.x0, f.y0, f.x1, f.y1], hittable: a.hittable, blocking: a.blocking }
}

pub fn player_obs(w: &World, i: usize) -> PlayerObs {
    let p = w.pl_ref(i);
    PlayerObs {
        slot: p.slot,
        actor: actor_obs(w.ents[i].id, "player", &p.a),
        hp: p.hp(),
        hp_max: p.max_hp(),
        ep: p.ep(),
        ep_max: p.max_ep(),
        form: p.form,
        state: p.gs.name(),
        coarse: match p.state {
            crate::player::PState::Idle => "idle",
            crate::player::PState::Walk => "walk",
            crate::player::PState::Dash => "dash",
            crate::player::PState::Punch => "punch",
            crate::player::PState::Block => "block",
            crate::player::PState::Hurt => "hurt",
            crate::player::PState::Dying => "dying",
            crate::player::PState::Skill => "skill",
            crate::player::PState::Transform => "transform",
            crate::player::PState::Held => "held",
        },
        level: p.rec.level,
        skills: p.skills.clone(),
        selected_skill: p.skill(),
        ss_gauge: p.ss_gauge,
        ss3_gauge: p.ss3_gauge >> 16,
        ss_cooldown: p.ss_cooldown,
        dead: p.dead,
        held_by: p.held_by,
        wraps: p.wraps,
        skill: p.sk.as_ref().map(|s| (s.id, s.t)),
    }
}

pub fn observe(w: &World, player: usize) -> Observation {
    let players: Vec<PlayerObs> = w.players.iter().filter_map(|&id| w.find(id)).map(|i| player_obs(w, i)).collect();
    let me = w.player_idx(player).map(|i| player_obs(w, i));
    let mut enemies = Vec::new();
    let mut projectiles = Vec::new();
    let mut rocks = Vec::new();
    for (i, e) in w.ents.iter().enumerate() {
        if e.removed {
            continue;
        }
        match &e.body {
            Body::Enemy(en) if !en.part => {
                let t = w.data.enemy(en.ty);
                enemies.push(EnemyObs {
                    actor: actor_obs(e.id, "enemy", &en.a),
                    ty: en.ty,
                    name: t.map(|t| t.name.clone()).unwrap_or_default(),
                    hp: en.hp(),
                    hp_max: en.max_hp1024 / 1024,
                    defeated: en.defeated,
                    primary: en.primary,
                    stunned: en.stunned,
                    ai: en.ai.states(),
                    in_state: w.frame as i64 - en.state_stamp,
                    target: en.target,
                });
            }
            Body::Shot(s) => {
                let r = crate::shots::rect(w, i);
                projectiles.push(ShotObs { id: e.id, kind: s.kind_name(), owner: s.owner, team: s.team, dir: s.dir, x: s.px(), y: s.py(), rect: [r.x0, r.y0, r.x1, r.y1], t: s.t });
            }
            Body::Rock { x, y, .. } => rocks.push([x + 16, y + 16, x + 48, y + 48]),
            _ => {}
        }
    }
    let cam = w.camera(player);
    let (tx0, ty0) = ((cam.x0 >> 3) - 4, (cam.y0 >> 3) - 4);
    let (tw, th) = ((240 >> 3) + 8, (160 >> 3) + 8);
    let mut walk = String::with_capacity((tw * th) as usize);
    for ty in ty0..ty0 + th {
        for tx in tx0..tx0 + tw {
            let ok = w.col.as_ref().map(|c| c.tile(crate::world::Layer::Walk, tx, ty)).unwrap_or(true);
            walk.push(if ok { '1' } else { '0' });
        }
    }
    let b = w.arena.bounds;
    Observation {
        frame: w.frame,
        me,
        players,
        enemies,
        projectiles,
        rocks,
        camera: [cam.x0, cam.y0, cam.x1, cam.y1],
        arena: ArenaObs { id: w.arena.id.clone(), map: w.arena.map.clone(), bounds: [b.x0, b.y0, b.x1, b.y1], walk_window: walk, window_origin: (tx0 * 8, ty0 * 8), window_tiles: (tw, th) },
        rng: w.rng.state(),
        candy: w.g.candy,
    }
}
