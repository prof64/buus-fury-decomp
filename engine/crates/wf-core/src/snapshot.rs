//! Serialized snapshots (save states, rewind, rollback across processes): the world without its static data
//! (game data, map grids, compiled AI trees), encoded with postcard. `World::clone()` is the in-process snapshot;
//! this is the byte form. Static parts are re-attached from the `GameData` on restore.

use crate::battle::{arena_runtime, BattleState};
use crate::content::GameData;
use crate::input::InputFrame;
use crate::rng::Rng;
use crate::world::{Body, Camera, Collision, Entity, EntityId, Globals, Options, Stats, World, MAX_PLAYERS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const MAGIC: u32 = 0x5746_4331; // "WFC1"

#[derive(Serialize, Deserialize)]
struct WorldSer {
    magic: u32,
    frame: u32,
    ents: Vec<Entity>,
    next_id: u32,
    rng: Rng,
    input: [InputFrame; MAX_PLAYERS],
    cameras: [Camera; MAX_PLAYERS],
    modal: Option<Vec<EntityId>>,
    g: Globals,
    enemy_layer: Option<Vec<u8>>,
    arena: String,
    stats: Stats,
    players: Vec<EntityId>,
    enemies: Vec<EntityId>,
    serial: u32,
    opts: Options,
    battle: BattleState,
}

pub fn to_bytes(w: &World) -> Vec<u8> {
    let s = WorldSer {
        magic: MAGIC,
        frame: w.frame,
        ents: w.ents.clone(),
        next_id: w.next_id,
        rng: w.rng,
        input: w.input,
        cameras: w.cameras.clone(),
        modal: w.modal.clone(),
        g: w.g.clone(),
        enemy_layer: w.col.as_ref().map(|c| c.enemy.clone()),
        arena: w.arena.id.clone(),
        stats: w.stats.clone(),
        players: w.players.clone(),
        enemies: w.enemies.clone(),
        serial: w.serial,
        opts: w.opts.clone(),
        battle: w.battle.clone(),
    };
    postcard::to_allocvec(&s).unwrap_or_default()
}

pub fn from_bytes(data: &Arc<GameData>, bytes: &[u8]) -> Option<World> {
    let s: WorldSer = postcard::from_bytes(bytes).ok()?;
    if s.magic != MAGIC {
        return None;
    }
    let (arena, _) = arena_runtime(data, &s.arena);
    let mut w = World::new(Arc::clone(data), s.rng, arena);
    w.col = match (s.enemy_layer, data.maps.get(&w.arena.map)) {
        (Some(layer), Some(m)) => {
            let mut c = Collision::new(Arc::clone(m));
            c.enemy = layer;
            Some(c)
        }
        _ => None,
    };
    w.frame = s.frame;
    w.ents = s.ents;
    w.next_id = s.next_id;
    w.input = s.input;
    w.cameras = s.cameras;
    w.modal = s.modal;
    w.g = s.g;
    w.stats = s.stats;
    w.players = s.players;
    w.enemies = s.enemies;
    w.serial = s.serial;
    w.opts = s.opts;
    w.battle = s.battle;
    for e in w.ents.iter_mut() {
        if let Body::Enemy(en) = &mut e.body {
            for st in en.ai.stack.iter_mut() {
                st.tree = data.states.get(&st.state).cloned().unwrap_or_else(|| Arc::new(crate::ai::compile_behaviour(st.state, &Default::default())));
            }
        }
    }
    Some(w)
}
