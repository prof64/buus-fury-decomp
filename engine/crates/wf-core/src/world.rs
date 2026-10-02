//! The world: one plain-data struct holding every piece of simulation state (entities in the game's update order,
//! the RNG, the battle globals, the enemy collision layer, cameras, the frame's events). `World: Clone + Hash`;
//! static game data is shared behind an `Arc` and excluded from both.
//!
//! Update order (the Webfoot World::update): entities last to first; entities added during a frame wait until the
//! next; removed entities are dropped after the pass. Ids increase monotonically (the players sit behind the initial actors).

use crate::content::{GameData, MapCollision, GRID};
use crate::damage::{AttackType, CombatStats};
use crate::enemy::Enemy;
use crate::fx::Fx;
use crate::geom::{Dir, Rect};
use crate::input::InputFrame;
use crate::player::Player;
use crate::rng::Rng;
use crate::shots::Shot;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub type EntityId = u32;
pub const MAX_PLAYERS: usize = 4;
pub const GBA_W: i32 = 240;
pub const GBA_H: i32 = 160;

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub enum Body {
    Player(Box<Player>),
    Enemy(Box<Enemy>),
    Shot(Box<Shot>),
    Fx(Box<Fx>),
    /// Broly's arena rock (0x08036A44): 32x32 solid block at +16..+48
    Rock { x: i32, y: i32, hp: i32 },
    /// Janemba hand (type 0x36) rising under the player
    Hand(Box<Hand>),
    /// harness hook: scheduled attack messages injected at the start of a frame (lockstep / tests)
    Injector(Box<Injector>),
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub removed: bool,
    pub body: Body,
}

impl Entity {
    pub fn kind(&self) -> &'static str {
        match &self.body {
            Body::Player(_) => "player",
            Body::Enemy(_) => "enemy",
            Body::Shot(s) => s.kind_name(),
            Body::Fx(f) => f.kind_name(),
            Body::Rock { .. } => "rock",
            Body::Hand(_) => "hand",
            Body::Injector(_) => "injector",
        }
    }
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Hand {
    pub a: crate::actor::Actor,
    pub t: i32,
    pub hit: bool,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct InjectedHit {
    pub at: u32,
    pub dir: Dir,
    pub atk: u8,
    pub mult: i32,
    pub str_: i32,
    pub pow: i32,
}
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Injector {
    pub hits: Vec<InjectedHit>,
    pub player: EntityId,
}

/// sounds and other presentation events of one frame (data; the sim never plays anything)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// PlaySfx(id, volume 0..64)
    Sfx { id: i32, vol: i32 },
    SfxStop { id: i32 },
    Song { id: i32 },
    /// a player attack connected (the reference's 'hit' cue)
    Hit,
    SkillStart { id: i32 },
    SkillEnd,
    /// a ComputeDamage result (target entity, 1/1024 HP)
    Damage { target: EntityId, attacker: EntityId, dmg: i64, crit: bool },
    Info { text: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Stats {
    pub frames: u32,
    pub damage_dealt: i64,
    pub damage_taken: i64,
    pub hits_dealt: u32,
    pub hits_taken: u32,
    pub crits: u32,
    pub kills: u32,
    pub ep_spent: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shake {
    pub left: i32,
    pub intensity: i32,
    pub period: i32,
    pub cnt: i32,
}

/// the few global words of the game the AI / skills read (re/buu/notes.md)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Globals {
    /// 0x03002FE4: candy frames left
    pub candy: i32,
    pub candy_frame: i32,
    /// 0x0300302C
    pub buu_pacified: i32,
    /// 0x030032E4: 0 active, 1 destroyed, 2 fading, 3 expired
    pub sphere_state: i32,
    /// 0x030032EC
    pub waypoint: i32,
    /// 0x03003228
    pub tele_chain: i32,
    /// 0x030032E5: re-plan phase of the next path follower
    pub path_stagger: i32,
    /// HUD +0x10 low-HP beep frame
    pub last_beep: i64,
    pub shakes: Vec<Shake>,
    pub shake_off: (i32, i32),
}

/// live collision of the battle: the map's static walk / shot grids + the private enemy layer (map object +0xB0)
#[derive(Clone, Debug)]
pub struct Collision {
    pub map: Arc<MapCollision>,
    pub enemy: Vec<u8>,
}
impl Hash for Collision {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.enemy.hash(h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Walk,
    Shot,
    Enemy,
}

impl Collision {
    pub fn new(map: Arc<MapCollision>) -> Collision {
        let enemy = map.walk.clone();
        Collision { map, enemy }
    }
    fn grid(&self, l: Layer) -> &[u8] {
        match l {
            Layer::Walk => &self.map.walk,
            Layer::Shot => &self.map.shot,
            Layer::Enemy => &self.enemy,
        }
    }
    pub fn tile(&self, l: Layer, tx: i32, ty: i32) -> bool {
        if tx < 0 || ty < 0 || tx >= GRID as i32 || ty >= GRID as i32 {
            return false;
        }
        self.grid(l)[ty as usize * GRID + tx as usize] == 1
    }
    /// 0x08009324(box, grid): false if the box leaves 0..2048 or any 8x8 tile under [x0, x1) x [y0, y1) is blocked
    pub fn test(&self, l: Layer, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        if x0 < 0 || y0 < 0 || x1 > 2048 || y1 > 2048 {
            return false;
        }
        let g = self.grid(l);
        let tx1 = (x1 + 7) >> 3;
        let ty1 = (y1 + 7) >> 3;
        for ty in (y0 >> 3)..ty1 {
            for tx in (x0 >> 3)..tx1 {
                if tx >= GRID as i32 || ty >= GRID as i32 || g[ty as usize * GRID + tx as usize] != 1 {
                    return false;
                }
            }
        }
        true
    }
    pub fn test_rect(&self, l: Layer, r: &Rect) -> bool {
        self.test(l, r.x0, r.y0, r.x1, r.y1)
    }
    fn span(x0: i32, y0: i32, x1: i32, y1: i32) -> (i32, i32, i32, i32) {
        let g = GRID as i32;
        ((x0 >> 3).max(0), (y0 >> 3).max(0), g.min(if x1 >= 2048 { 256 } else { (x1 + 7) >> 3 }), g.min(if y1 >= 2048 { 256 } else { (y1 + 7) >> 3 }))
    }
    /// 0x08009930: set every enemy-layer tile under the box (clipped)
    pub fn mark_enemy(&mut self, r: [i32; 4], value: u8) {
        let (tx0, ty0, tx1, ty1) = Self::span(r[0], r[1], r[2], r[3]);
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                self.enemy[ty as usize * GRID + tx as usize] = value;
            }
        }
    }
    pub fn save_enemy(&self, r: [i32; 4]) -> Vec<u8> {
        let (tx0, ty0, tx1, ty1) = Self::span(r[0], r[1], r[2], r[3]);
        let mut out = Vec::new();
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                out.push(self.enemy[ty as usize * GRID + tx as usize]);
            }
        }
        out
    }
    pub fn load_enemy(&mut self, r: [i32; 4], v: &[u8]) {
        let (tx0, ty0, tx1, ty1) = Self::span(r[0], r[1], r[2], r[3]);
        let mut k = 0;
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                self.enemy[ty as usize * GRID + tx as usize] = v[k];
                k += 1;
            }
        }
    }
    /// nearest position (8 px spiral) where a w x h footprint is free on `l`, inside `inside` and with its
    /// top-left tile in `region`
    #[allow(clippy::too_many_arguments)]
    pub fn free_spot(&self, l: Layer, x: i32, y: i32, w: i32, h: i32, inside: Option<&Rect>, region: Option<&[u8]>) -> (i32, i32) {
        let ok = |px: i32, py: i32| {
            self.test(l, px, py, px + w, py + h)
                && inside.is_none_or(|b| px >= b.x0 && py >= b.y0 && px + w <= b.x1 && py + h <= b.y1)
                && region.is_none_or(|m| {
                    let i = (py >> 3) * GRID as i32 + (px >> 3);
                    i >= 0 && (i as usize) < m.len() && m[i as usize] == 1
                })
        };
        if ok(x, y) {
            return (x, y);
        }
        for r in 1i32..64 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    if ok(x + dx * 8, y + dy * 8) {
                        return (x + dx * 8, y + dy * 8);
                    }
                }
            }
        }
        (x, y)
    }
}

/// 4-connected component of walk tiles around (x, y) px: tile mask + bounding box
pub fn walk_region(m: &MapCollision, x: i32, y: i32) -> Option<(Vec<u8>, Rect)> {
    let g = GRID as i32;
    let (sx, sy) = (x >> 3, y >> 3);
    if sx < 0 || sy < 0 || sx >= g || sy >= g || m.walk[(sy * g + sx) as usize] == 0 {
        return None;
    }
    let mut mask = vec![0u8; GRID * GRID];
    let mut stack = vec![(sy * g + sx) as usize];
    mask[stack[0]] = 1;
    let (mut x0, mut x1, mut y0, mut y1) = (sx, sx, sy, sy);
    while let Some(i) = stack.pop() {
        let tx = (i % GRID) as i32;
        let ty = (i / GRID) as i32;
        x0 = x0.min(tx);
        x1 = x1.max(tx);
        y0 = y0.min(ty);
        y1 = y1.max(ty);
        let nb = [if tx > 0 { i as i64 - 1 } else { -1 }, if tx < g - 1 { i as i64 + 1 } else { -1 }, if ty > 0 { i as i64 - g as i64 } else { -1 }, if ty < g - 1 { i as i64 + g as i64 } else { -1 }];
        for j in nb {
            if j >= 0 {
                let j = j as usize;
                if mask[j] == 0 && m.walk[j] != 0 {
                    mask[j] = 1;
                    stack.push(j);
                }
            }
        }
    }
    Some((mask, Rect::new(x0 * 8, y0 * 8, (x1 + 1) * 8, (y1 + 1) * 8)))
}

#[derive(Clone, Debug)]
pub struct ArenaRt {
    pub id: String,
    pub map: String,
    /// walkable bounding box (an entity's footprint must stay inside)
    pub bounds: Rect,
    /// walkable component around the spawn (tile mask)
    pub region: Option<Arc<Vec<u8>>>,
    /// what the camera may show (the map's world size)
    pub view: Option<Rect>,
}
impl Hash for ArenaRt {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h);
        self.bounds.hash(h);
    }
}

/// the game camera (0x080077EC) of one player: 22.10 position easing toward the footprint centre
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Camera {
    pub x: i32,
    pub y: i32,
    pub init: bool,
    pub rect: Rect,
}

/// a hit message: box + hit struct {dir, mult, attack type, attacker} (notes §8b)
#[derive(Clone, Copy, Debug)]
pub struct HitMsg {
    /// 0 = enemy attack (hits players), 1 = player attack (hits enemies), 2 = hits every actor
    pub team: u8,
    pub rect: Rect,
    pub dir: Dir,
    /// 0x400 = x1.0
    pub mult: i32,
    pub atk: AttackType,
    /// attacking entity (0 = none, e.g. an injected hit)
    pub attacker: EntityId,
    pub att_stats: CombatStats,
    pub att_is_player: bool,
    /// the object that sent it (0 = the attacker itself)
    pub source: EntityId,
    pub crit: bool,
    pub raw: i64,
    pub dmg: i64,
}

#[derive(Clone)]
pub struct World {
    pub data: Arc<GameData>,
    pub frame: u32,
    pub ents: Vec<Entity>,
    pub next_id: u32,
    pub rng: Rng,
    /// input of the current frame per player slot
    pub input: [InputFrame; MAX_PLAYERS],
    pub cameras: [Camera; MAX_PLAYERS],
    /// a modal loop of the game (Instant Transmission target pick): only these entities update
    pub modal: Option<Vec<EntityId>>,
    pub g: Globals,
    pub col: Option<Collision>,
    pub arena: ArenaRt,
    pub stats: Stats,
    pub events: Vec<Event>,
    /// player entities by slot
    pub players: Vec<EntityId>,
    /// enemies in spawn order (the reference's ctx.enemies; primary ones stay after removal)
    pub enemies: Vec<EntityId>,
    /// serial for AI stack entries / skill instances (object identity of the reference)
    pub serial: u32,
    pub opts: Options,
    pub battle: crate::battle::BattleState,
}

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub xp: bool,
}

impl Hash for World {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.frame.hash(h);
        self.ents.hash(h);
        self.next_id.hash(h);
        self.rng.hash(h);
        self.input.hash(h);
        self.cameras.hash(h);
        self.modal.hash(h);
        self.g.hash(h);
        self.col.hash(h);
        self.arena.hash(h);
        self.stats.hash(h);
        self.players.hash(h);
        self.enemies.hash(h);
        self.serial.hash(h);
        self.opts.hash(h);
        self.battle.hash(h);
    }
}

/// FNV-1a 64 with platform-independent integer widths (usize hashed as u64): the per-frame state hash
pub struct StableHasher(u64);
impl Default for StableHasher {
    fn default() -> Self {
        StableHasher(0xcbf2_9ce4_8422_2325)
    }
}
impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        // 8 bytes per step (an Fx-style mix), the tail bytewise (FNV-1a); fixed little-endian: platform independent
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let v = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
            self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
        for &b in chunks.remainder() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_usize(&mut self, i: usize) {
        self.write(&(i as u64).to_le_bytes())
    }
    fn write_isize(&mut self, i: isize) {
        self.write(&(i as i64).to_le_bytes())
    }
}

impl World {
    pub fn new(data: Arc<GameData>, rng: Rng, arena: ArenaRt) -> World {
        World {
            data,
            frame: 0,
            ents: Vec::new(),
            next_id: 1,
            rng,
            input: [InputFrame::default(); MAX_PLAYERS],
            cameras: Default::default(),
            modal: None,
            g: Globals { last_beep: -1000, ..Default::default() },
            col: None,
            arena,
            stats: Stats::default(),
            events: Vec::new(),
            players: Vec::new(),
            enemies: Vec::new(),
            serial: 0,
            opts: Options::default(),
            battle: Default::default(),
        }
    }

    /// the per-frame state hash (desync detection, replay verification)
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::default();
        self.hash(&mut h);
        h.finish()
    }

    pub fn add(&mut self, body: Body) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        self.ents.push(Entity { id, removed: false, body });
        id
    }
    pub fn next_serial(&mut self) -> u32 {
        self.serial += 1;
        self.serial
    }

    /// index of a live (present, maybe flagged removed) entity
    pub fn find(&self, id: EntityId) -> Option<usize> {
        if id == 0 {
            return None;
        }
        // the list is in update order (mostly by id; the players are moved behind the initial actors)
        self.ents.iter().position(|e| e.id == id)
    }
    /// present and not flagged removed
    pub fn alive(&self, id: EntityId) -> bool {
        self.find(id).is_some_and(|i| !self.ents[i].removed)
    }
    pub fn is_removed(&self, id: EntityId) -> bool {
        !self.alive(id)
    }

    pub fn en(&mut self, i: usize) -> &mut Enemy {
        match &mut self.ents[i].body {
            Body::Enemy(e) => e,
            _ => panic!("entity {i} is not an enemy"),
        }
    }
    pub fn en_ref(&self, i: usize) -> &Enemy {
        match &self.ents[i].body {
            Body::Enemy(e) => e,
            _ => panic!("entity {i} is not an enemy"),
        }
    }
    pub fn enemy_by_id(&self, id: EntityId) -> Option<(usize, &Enemy)> {
        let i = self.find(id)?;
        match &self.ents[i].body {
            Body::Enemy(e) => Some((i, e)),
            _ => None,
        }
    }
    pub fn pl(&mut self, i: usize) -> &mut Player {
        match &mut self.ents[i].body {
            Body::Player(p) => p,
            _ => panic!("entity {i} is not a player"),
        }
    }
    pub fn pl_ref(&self, i: usize) -> &Player {
        match &self.ents[i].body {
            Body::Player(p) => p,
            _ => panic!("entity {i} is not a player"),
        }
    }
    pub fn shot(&mut self, i: usize) -> &mut Shot {
        match &mut self.ents[i].body {
            Body::Shot(s) => s,
            _ => panic!("entity {i} is not a shot"),
        }
    }
    pub fn shot_ref(&self, i: usize) -> &Shot {
        match &self.ents[i].body {
            Body::Shot(s) => s,
            _ => panic!("entity {i} is not a shot"),
        }
    }
    /// player entity index of a slot
    pub fn player_idx(&self, slot: usize) -> Option<usize> {
        self.players.get(slot).and_then(|&id| self.find(id))
    }
    /// the actor fields of a player or enemy entity
    pub fn actor(&self, i: usize) -> Option<&crate::actor::Actor> {
        match &self.ents[i].body {
            Body::Player(p) => Some(&p.a),
            Body::Enemy(e) => Some(&e.a),
            Body::Hand(h) => Some(&h.a),
            _ => None,
        }
    }
    pub fn actor_by_id(&self, id: EntityId) -> Option<&crate::actor::Actor> {
        self.find(id).and_then(|i| self.actor(i))
    }

    /// enemies in spawn order that are present, not removed and not defeated (ctx.liveEnemies)
    pub fn live_enemies(&self) -> Vec<usize> {
        self.enemies
            .iter()
            .filter_map(|&id| self.find(id))
            .filter(|&i| !self.ents[i].removed && !self.en_ref(i).defeated)
            .collect()
    }

    pub fn sfx(&mut self, id: i32, vol: i32) {
        self.events.push(Event::Sfx { id, vol });
    }
    pub fn sfx_stop(&mut self, id: i32) {
        self.events.push(Event::SfxStop { id });
    }

    /// camera rect of a player slot (slot 0 = the game's camera)
    pub fn camera(&self, slot: usize) -> Rect {
        self.cameras[slot.min(MAX_PLAYERS - 1)].rect
    }
    /// the union test the projectiles use: a rect is on camera when it overlaps any player's camera
    pub fn on_any_camera(&self, r: &Rect) -> bool {
        let n = self.players.len().clamp(1, MAX_PLAYERS);
        (0..n).any(|s| crate::geom::overlaps(r, &self.cameras[s].rect))
    }

    /// 0x0800797C screen shake (duration, intensity)
    pub fn shake(&mut self, duration: i32, intensity: i32) {
        self.g.shakes.push(Shake { left: duration, intensity, period: (100 - intensity) / 16 + 1, cnt: 1 });
    }
    /// shake tasks: every `period` frames two RNG draws pick the offset ((next - 0x8000) * intensity) >> 19
    pub fn tick_shakes(&mut self) {
        let mut shakes = std::mem::take(&mut self.g.shakes);
        for s in shakes.iter_mut() {
            s.left -= 1;
            if s.left == 0 {
                self.g.shake_off = (0, 0);
                continue;
            }
            s.cnt -= 1;
            if s.cnt == 0 {
                s.cnt = s.period;
                let dx = ((self.rng.next().wrapping_sub(0x8000) as i32).wrapping_mul(s.intensity)) >> 19;
                let dy = ((self.rng.next().wrapping_sub(0x8000) as i32).wrapping_mul(s.intensity)) >> 19;
                self.g.shake_off = (dx, dy);
            }
        }
        shakes.retain(|s| s.left > 0);
        self.g.shakes = shakes;
    }

    /// Advance one frame: entities last to first (modal filter), then drop the removed ones.
    pub fn step_entities(&mut self) {
        self.frame += 1;
        self.stats.frames += 1;
        let n = self.ents.len();
        for i in (0..n).rev() {
            if self.ents[i].removed {
                continue;
            }
            if let Some(m) = &self.modal {
                if !m.contains(&self.ents[i].id) {
                    continue;
                }
            }
            self.update_entity(i);
        }
        self.ents.retain(|e| !e.removed);
    }

    fn update_entity(&mut self, i: usize) {
        match &self.ents[i].body {
            Body::Player(_) => crate::player::update(self, i),
            Body::Enemy(_) => crate::enemy::update(self, i),
            Body::Shot(_) => crate::shots::update(self, i),
            Body::Fx(_) => crate::fx::update(self, i),
            Body::Rock { .. } => {}
            Body::Hand(_) => crate::enemy::hand_update(self, i),
            Body::Injector(_) => {
                let (hits, pid) = match &self.ents[i].body {
                    Body::Injector(j) => (j.hits.iter().filter(|h| h.at == self.frame).cloned().collect::<Vec<_>>(), j.player),
                    _ => unreachable!(),
                };
                for h in hits {
                    if let Some(pi) = self.find(pid) {
                        let a = &self.pl_ref(pi).a;
                        let (px, py) = (a.px(), a.py());
                        let stats = CombatStats { str_: h.str_, pow: h.pow, end: 0, end2: 0, res_m: 128, res_e: 128, crit_bonus: 0, block_factor: 0x1000 };
                        let mut m = HitMsg {
                            team: 0,
                            rect: Rect::new(px - 32, py - 64, px + 48, py + 32),
                            dir: h.dir,
                            mult: h.mult,
                            atk: crate::damage::atk(h.atk),
                            attacker: 0,
                            att_stats: stats,
                            att_is_player: false,
                            source: 0,
                            crit: false,
                            raw: 0,
                            dmg: 0,
                        };
                        broadcast(self, &mut m, false);
                    }
                }
            }
        }
    }
}

/// 0x08011C70: send to every object present when the broadcast starts; `first` stops at the first acceptor
pub fn broadcast(w: &mut World, msg: &mut HitMsg, first: bool) -> bool {
    let n = w.ents.len();
    let mut any = false;
    for k in 0..n {
        if k >= w.ents.len() || w.ents[k].removed {
            continue;
        }
        if deliver(w, k, msg) {
            any = true;
            if first {
                break;
            }
        }
    }
    any
}

/// one receiver's message handler
pub fn deliver(w: &mut World, k: usize, msg: &mut HitMsg) -> bool {
    match &w.ents[k].body {
        Body::Player(_) => crate::player::on_message(w, k, msg),
        Body::Enemy(_) => crate::enemy::on_message(w, k, msg),
        Body::Shot(_) => crate::shots::on_message(w, k, msg),
        _ => false,
    }
}
