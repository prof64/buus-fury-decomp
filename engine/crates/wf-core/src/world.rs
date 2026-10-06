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
    Rock {
        x: i32,
        y: i32,
        hp: i32,
    },
    /// Janemba hand (type 0x36) rising under the player
    Hand(Box<Hand>),
    /// harness hook: scheduled attack messages injected at the start of a frame (lockstep / tests)
    Injector(Box<Injector>),
    /// field: an item on the ground (map item or loot drop)
    Pickup(Box<crate::fieldobj::Pickup>),
    /// field: a breakable map prop
    Breakable(Box<crate::fieldobj::Breakable>),
    /// field: a bobbing map sprite
    MapSprite(Box<crate::fieldobj::MapSprite>),
    /// a scripted actor (cutscene actors, `spawn_actor`, `change_sprite`; scene.rs)
    Npc(Box<crate::scene::Npc>),
    /// a mod projectile / effect (`projectiles/*.json`, modkit.rs)
    Mod(Box<crate::modkit::ModShot>),
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
            Body::Pickup(_) => "pickup",
            Body::Breakable(_) => "breakable",
            Body::MapSprite(_) => "map_sprite",
            Body::Npc(_) => "npc",
            Body::Mod(_) => "mod",
        }
    }
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Hand {
    pub a: crate::actor::Actor,
    pub t: i32,
    pub hit: bool,
    /// field: the enemy-grid tiles its constructor marked (cleared by the arm's strike controller end, holes and all)
    #[serde(default)]
    pub marked: Option<[i32; 4]>,
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
    /// the harness frame the hits are scheduled in (0 = the world frame); differs when the recording has lag frames
    pub now: u32,
    /// injected AI starts (harness frame, enemy, state): GotoState(state) at the start of that frame (a frame with a
    /// hit injects only the hit)
    #[serde(default)]
    pub ai: Vec<(u32, EntityId, u16)>,
}

/// sounds and other presentation events of one frame (data; the sim never plays anything)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// PlaySfx(id, volume 0..64)
    Sfx {
        id: i32,
        vol: i32,
    },
    SfxStop {
        id: i32,
    },
    /// StopMatching 0x0803E32C: stop the voices playing that effect
    SfxStopMatch {
        id: i32,
    },
    /// StopAllSfx 0x0803E2EC (script native 0x22)
    SfxStopAll,
    /// the music sequencer stopped (script native 0x24)
    SongStop,
    /// the Options page changed the master volumes (raw 0..8: `0x0803E42A` / `0x0803F18C`)
    MasterVolumes {
        music: i32,
        sfx: i32,
    },
    Song {
        id: i32,
    },
    /// a player attack connected (the reference's 'hit' cue)
    Hit,
    SkillStart {
        id: i32,
    },
    SkillEnd,
    /// a ComputeDamage result (target entity, 1/1024 HP)
    Damage {
        target: EntityId,
        attacker: EntityId,
        dmg: i64,
        crit: bool,
    },
    /// a blocked hit's ComputeDamage before the block factor (0x0800EABE, what the lockstep logs); precedes its Damage
    DamageRoll {
        target: EntityId,
        dmg: i64,
    },
    Info {
        text: String,
    },
    /// field game: area changes, scripts to run, pickups, defeated placed enemies (field.rs)
    Field(crate::field::FieldEvent),
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
    /// the shake offset before this frame's shake update (`tick_shakes`): the BG layers show it - their registers are
    /// copied before the shake object's update (rkb_168 14-1 row 3,300: the game's BGs at the row-3,299 offset)
    #[serde(default)]
    pub shake_shown: (i32, i32),
    /// a shake ended in this frame's update: the camera slot is still held for the frame (field cameras)
    #[serde(default)]
    pub shake_ended: bool,
    /// the world frame of the last shake-task tick (a script's shake made later in the same frame: scene.rs)
    #[serde(default)]
    pub shakes_ticked: u32,
    /// field: the frame whose shake tick is due after the script phase (session.rs `step_post`)
    #[serde(default)]
    pub shake_due: u32,
    /// field: a held frame's shake tick, due after the frame's script phase and the rest of an interrupted object pass
    /// (session.rs: the task list runs after the objects - run 3 row 1,097,649: a drop script's hold ends, the pass
    /// resumes with an enemy's hit and its 48 + rand(2), then the shake's two draws)
    #[serde(default)]
    pub shake_held: bool,
    /// the game's frame counter 0x03000F94 at frame 0 (`clock` = fc0 + frame; nodes that test the counter itself,
    /// e.g. VampireGrab's drain every 16 ticks)
    #[serde(default)]
    pub fc0: i64,
    /// the logic counter at this frame's start (set by the update, taken by the post step): every logic frame
    /// outside the menu screens counts exactly once on it, whatever object passes ran
    #[serde(default)]
    pub clock_mark: Option<i64>,
    /// the sim's own lag frames (`Session::lag_step`): an area load asked for its iteration in this logic frame
    /// (`load_pending`); `lag` = the video frames of that iteration still to come, each without a logic step (the
    /// sound hardware runs on); `lag_cam` = the remaining count at whose frame the load's closing camera update shows
    #[serde(default)]
    pub load_pending: bool,
    #[serde(default)]
    pub lag: u16,
    #[serde(default)]
    pub lag_cam: u16,
    /// the input object 0x03003300 (0x0804023C) per player slot: this frame's new presses and the press ring
    #[serde(default)]
    pub presses: [PressLog; MAX_PLAYERS],
    /// the logic count the input object last recorded presses at (`note_input`)
    #[serde(default)]
    pub presses_at: i64,
    /// 0x030030BC: the next stuck mummy bandage's slot (0..7)
    #[serde(default)]
    pub wrap_slot: i32,
    /// the soldier-throw controller 0x03002FCC is one object for every soldier: +4 holds the wave the last throw
    /// spawned (an entity id here). A throw ends (tick >= 32, 0x08026618) once THAT wave is gone, whoever threw it
    /// (Babidi's ship 8-34: a soldier keeps its throw pose until the other soldier's wave dies)
    #[serde(default)]
    pub soldier_wave: i32,
    /// the Buu fragment's state object 0x03003158 (one IWRAM singleton shared by every fragment): (mode, vx, vy)
    #[serde(default)]
    pub frag: (i32, i32, i32),
}

/// the input object's record of new presses (0x0804023C): +4 the keys newly pressed this frame; a 64-entry ring of
/// press events (+8 keys, +0x88 frame counter)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PressLog {
    pub pressed: u16,
    pub ring: std::collections::VecDeque<(u16, i64)>,
}

impl Globals {
    /// the frame counter 0x03000F94 now
    pub fn clock(&self, frame: u32) -> i64 {
        self.fc0 + frame as i64
    }
    /// record a frame's new presses (stamped `at`, the counter during that frame's update)
    pub fn note_input(&mut self, slot: usize, at: i64, pressed: u16) {
        self.presses_at = at;
        let Some(l) = self.presses.get_mut(slot) else {
            return;
        };
        l.pressed = pressed;
        if pressed == 0 {
            return;
        }
        l.ring.push_back((pressed, at));
        if l.ring.len() > 64 {
            l.ring.pop_front();
        }
    }
    /// the keys newly pressed this frame (input +4)
    pub fn pressed(&self, slot: usize) -> u16 {
        self.presses.get(slot).map(|l| l.pressed).unwrap_or(0)
    }
    /// 0x08040284(input, mask, n, window): a press this frame and, walking the ring back from the newest event, the
    /// last n events with a `mask` key all stamped >= now - window
    pub fn press_run(&self, slot: usize, mask: u16, n: u32, window: i64, frame: u32) -> bool {
        let Some(l) = self.presses.get(slot) else {
            return false;
        };
        if l.pressed == 0 {
            return false;
        }
        let start = self.clock(frame) - window;
        let mut n = n;
        for &(keys, at) in l.ring.iter().rev() {
            if n == 0 {
                break;
            }
            if keys & mask == 0 {
                continue;
            }
            if at < start {
                return false;
            }
            n -= 1;
        }
        n == 0
    }
}

/// live collision of the battle: the map's static walk / shot grids + the private enemy layer (map object +0xB0)
#[derive(Clone, Debug)]
pub struct Collision {
    pub map: Arc<MapCollision>,
    pub enemy: Vec<u8>,
    /// the live walk grid once something marked it (field: talk NPCs, breakables, map items; copy-on-write of
    /// `map.walk`); None = the map's static grid
    pub walk: Option<Vec<u8>>,
}
impl Hash for Collision {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.enemy.hash(h);
        self.walk.hash(h);
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
        Collision {
            map,
            enemy,
            walk: None,
        }
    }
    fn grid(&self, l: Layer) -> &[u8] {
        match l {
            Layer::Walk => self.walk.as_deref().unwrap_or(&self.map.walk),
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
                if tx >= GRID as i32
                    || ty >= GRID as i32
                    || g[ty as usize * GRID + tx as usize] != 1
                {
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
        (
            (x0 >> 3).max(0),
            (y0 >> 3).max(0),
            g.min(if x1 >= 2048 { 256 } else { (x1 + 7) >> 3 }),
            g.min(if y1 >= 2048 { 256 } else { (y1 + 7) >> 3 }),
        )
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
    /// 0x08009930 on the walk grid (copy-on-write of the map's grid)
    pub fn mark_walk(&mut self, r: [i32; 4], value: u8) {
        let (tx0, ty0, tx1, ty1) = Self::span(r[0], r[1], r[2], r[3]);
        if tx0 >= tx1 || ty0 >= ty1 {
            return;
        }
        let g = self.walk.get_or_insert_with(|| self.map.walk.clone());
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                g[ty as usize * GRID + tx as usize] = value;
            }
        }
    }
    /// mark both the walk grid and the enemy layer (the field objects' blocking marks)
    pub fn mark_both(&mut self, r: [i32; 4], value: u8) {
        self.mark_walk(r, value);
        self.mark_enemy(r, value);
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
    pub fn free_spot(
        &self,
        l: Layer,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        inside: Option<&Rect>,
        region: Option<&[u8]>,
    ) -> (i32, i32) {
        let ok = |px: i32, py: i32| {
            self.test(l, px, py, px + w, py + h)
                && inside
                    .is_none_or(|b| px >= b.x0 && py >= b.y0 && px + w <= b.x1 && py + h <= b.y1)
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
        let nb = [
            if tx > 0 { i as i64 - 1 } else { -1 },
            if tx < g - 1 { i as i64 + 1 } else { -1 },
            if ty > 0 { i as i64 - g as i64 } else { -1 },
            if ty < g - 1 { i as i64 + g as i64 } else { -1 },
        ];
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
    /// the logical camera the game logic uses this frame
    pub rect: Rect,
    /// what the screen shows this frame: the previous frame's camera (the game displays the camera one frame late)
    pub shown: Rect,
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
    /// damage only (ComputeDamage + 0x08010BF2): no reaction, no i-frames (the flames 0x080247C0)
    pub no_react: bool,
    /// message kind 1: tested against the receiver's footprint (0x0800FDFA) instead of its body box
    pub foot: bool,
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
    /// persistent game state (flags, inventory, party records, play time); default in battle mode
    pub story: crate::story::Story,
    /// the field game (area, transitions); None = battle mode
    pub field: Option<Box<crate::field::Field>>,
    /// cutscene systems: command tracks, director, script camera, screen effects, dialog, menus (scene.rs)
    pub scene: crate::scene::Scene,
    /// mod runtime state: scripted skills, cooldowns, AI script variables (modkit.rs; empty without mods)
    pub modk: crate::modkit::ModState,
    /// menu screens (pause menu pages, shops, save, ...): menu/mod.rs; idle = nothing
    pub menu: crate::menu::Menu,
    /// the save slots (menu/save.rs): given at session start by the frontend, written by the Save Game screen,
    /// read by the title's Select Game; the frontend persists a slot when `generation` changes
    pub saves: crate::menu::save::Saves,
    /// the sound driver and the hardware's play position (sndsim.rs); idle without sound content
    pub sound: crate::sndsim::SoundSim,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub xp: bool,
    /// multiplayer rule set beyond the original: players' attacks hit other players (`battle::set_pvp`)
    #[serde(default)]
    pub pvp: bool,
    /// the sim makes the game's lag frames itself (`Session::lag_step`: area loads so far) instead of a host
    /// replaying them from a recording
    #[serde(default)]
    pub sim_lag: bool,
    /// no lag frames at all (the app's target: hardware slowdowns are not recreated) - menu redraws take no frames
    /// beyond their iteration, the boot screens count their logic frames only; one step = one game logic iteration
    #[serde(default)]
    pub no_lag: bool,
}
impl Hash for Options {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.xp.hash(h);
        // only hashed when on: single-player / co-op hashes stay what they were
        if self.pvp {
            self.pvp.hash(h);
        }
        if self.sim_lag {
            self.sim_lag.hash(h);
        }
        if self.no_lag {
            self.no_lag.hash(h);
        }
    }
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
        self.story.hash(h);
        self.field.hash(h);
        self.scene.hash(h);
        // hashes nothing while empty (games without scripted mod content)
        self.modk.hash(h);
        // only hashed while a menu is up or has left a result: menu-less hashes stay what they were
        if self.menu != crate::menu::Menu::default() {
            self.menu.hash(h);
        }
        if self.saves != crate::menu::save::Saves::default() {
            self.saves.hash(h);
        }
        // only with sound content
        if self.sound.drv.is_some() {
            self.sound.hash(h);
        }
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
            g: Globals {
                last_beep: -1000,
                ..Default::default()
            },
            col: None,
            arena,
            stats: Stats::default(),
            events: Vec::new(),
            players: Vec::new(),
            enemies: Vec::new(),
            serial: 0,
            opts: Options::default(),
            battle: Default::default(),
            story: Default::default(),
            field: None,
            scene: Default::default(),
            modk: Default::default(),
            menu: Default::default(),
            saves: Default::default(),
            sound: Default::default(),
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
        self.ents.push(Entity {
            id,
            removed: false,
            body,
        });
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
            Body::Npc(n) => Some(&n.a),
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
        self.g.shakes.push(Shake {
            left: duration,
            intensity,
            period: (100 - intensity) / 16 + 1,
            cnt: 1,
        });
    }
    /// the field camera does not follow while a shake holds its slot, nor in the frame the shake ended (story
    /// playthrough: a 15-frame hit shake holds the camera 16 frames); consumes the end mark
    pub fn camera_held_by_shake(&mut self) -> bool {
        let held = !self.g.shakes.is_empty() || self.g.shake_ended;
        self.g.shake_ended = false;
        held
    }
    /// shake tasks: every `period` frames two RNG draws pick the offset ((next - 0x8000) * intensity) >> 19
    pub fn tick_shakes(&mut self) {
        let mut shakes = std::mem::take(&mut self.g.shakes);
        // field: the screen-effect slot holds one shake at a time; a newer one suspends the older until it ends
        // (sc_janemba: the summon shake pauses through a crit shake, then goes on)
        let n = shakes.len();
        self.g.shake_shown = self.g.shake_off;
        self.g.shakes_ticked = self.frame;
        let stacked = self.field.is_some();
        for (k, s) in shakes.iter_mut().enumerate() {
            if stacked && k + 1 != n {
                continue;
            }
            s.left -= 1;
            if s.left == 0 {
                self.g.shake_off = (0, 0);
                self.g.shake_ended = true;
                continue;
            }
            s.cnt -= 1;
            if s.cnt == 0 {
                s.cnt = s.period;
                let dx =
                    ((self.rng.next().wrapping_sub(0x8000) as i32).wrapping_mul(s.intensity)) >> 19;
                let dy =
                    ((self.rng.next().wrapping_sub(0x8000) as i32).wrapping_mul(s.intensity)) >> 19;
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
        if self.field.is_some() {
            self.step_entities_list();
            return;
        }
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
        crate::vram::reap(self);
        self.ents.retain(|e| !e.removed);
    }

    /// the field's world list (0x080115B0): updated from the last object to the first by index into a list that
    /// compacts at once when an object is destroyed; objects created during the pass are appended (not updated this
    /// frame). An object that destroys one before it in the list therefore gets its update again (it moved down
    /// into the next index): story playthrough, the player's punch that destroys a fighter loaded before it ticks
    /// its animation twice in that frame.
    fn step_entities_list(&mut self) {
        self.step_entities_list_pre();
        self.step_entities_list_rest(None);
    }

    /// the rest of an object pass a cut interrupted (the game's update loop stood at entity `id`'s list place, its
    /// update not done: the objects from it down to the list's first are updated, as in the game's iteration the cut stands
    /// in - wf-import `finish_interrupted_pass`)
    pub fn finish_pass_from(&mut self, id: EntityId) {
        self.step_entities_list_rest(Some((id, true)));
    }

    /// [`finish_pass_from`] without `id`'s own update (the caller ran it)
    pub fn finish_pass_after(&mut self, id: EntityId) {
        self.step_entities_list_rest(Some((id, false)));
    }

    /// one entity's update, as the pass gives it (wf-import: the object a cut interrupted)
    pub fn update_one(&mut self, id: EntityId) {
        if let Some(i) = self.find(id) {
            self.update_entity(i);
            crate::scene::track_after_update(self, id);
        }
    }

    fn step_entities_list_pre(&mut self) {
        // the death script's loop (KO + 199) begins before the object pass: no object is updated in that frame but the
        // player's own (the modal), every actor gets message 0xF first (run 2 row 1,909,571, 19-3: a Kid Buu copy's
        // beam, newer in the list than the player, hit the KO'd player once more in ours; the game's frame has only
        // the walking Kid Buu's message-0xF idle draw)
        crate::player::ko_freeze(self);
        // the travel menu's label entity (the Other World map 0-51; created by a script, alone in the world list)
        if self.scene.dest_label.is_some() {
            crate::field::dest_menu(self);
        }
    }

    fn step_entities_list_rest(&mut self, from: Option<(EntityId, bool)>) {
        // the list as the game holds it: our live entities, and in their places the list objects that are no entities
        // (`vram::list_marks`, None here: nothing to update) - an update that destroys objects before it shifts the
        // next update onto whatever object moved into its index, which may be the HUD or the stat-points mark (run 2
        // Diablo Desert 11-4: a punch destroys two bandits; the game updates the stat mark again, not a bomb)
        // (a damage number deletes itself in its own update, at its own place: it never shifts another object, so the
        // pass keeps the slots it started with - ours ages them out from inside other objects' updates; ship 8-3
        // whole-run cut 631,836: a popup expiring there re-ran a Saibaman's update)
        let popups0: Vec<u32> = self
            .field
            .as_ref()
            .and_then(|f| f.vram.as_ref())
            .map(|v| v.popup_at.iter().map(|p| p.1).collect())
            .unwrap_or_default();
        let live = |w: &World| -> Vec<Option<usize>> {
            let ids: Vec<usize> = (0..w.ents.len()).filter(|&k| !w.ents[k].removed).collect();
            let mut marks = crate::vram::list_marks_objs(w);
            marks.extend(popups0.iter().copied());
            marks.sort();
            let mut out = Vec::with_capacity(ids.len() + marks.len());
            let mut m = 0;
            for k in ids {
                while m < marks.len() && marks[m] <= w.ents[k].id {
                    out.push(None);
                    m += 1;
                }
                out.push(Some(k));
            }
            out.extend(std::iter::repeat_n(None, marks.len() - m));
            out
        };
        let start = live(self).len();
        let mut c = match from {
            Some((id, own)) => match live(self).iter().position(|e| e.is_some_and(|k| self.ents[k].id == id)) {
                Some(p) => p as i64 - if own { 0 } else { 1 },
                None => return,
            },
            None => start as i64 - 1,
        };
        // an item-get that starts in this pass (a map item / dropped item picked up with A in the player's update)
        // runs the presentation's loop from there: the objects after it are not updated this frame (4-30's capsule:
        // the scientist's idle tick stops for that frame)
        let item_get_before = self.field.as_ref().is_some_and(|f| f.item_get.is_some());
        while c >= 0 {
            let l = live(self);
            let Some(&Some(i)) = l.get(c as usize) else {
                c -= 1;
                continue;
            };
            // a purge's suspended objects are out of the list (vram.rs); a cutscene actor keeps its animation
            // (the scene drives it: segment sb 4-24, a suspended actor's idle tick runs on)
            // (the death script's modal - the leader alone, `player::ko_freeze` - leaves the actors a script it runs
            // commands: run 3 row 1,796,422, 0-15, Gotenks KO'd by Majin Buu - the lose scene s021's play_anim on Buu
            // ends with his idle draw in the game; ours froze him)
            let ko_modal = crate::player::ko_modal(self);
            let skip = self.modal.as_ref().is_some_and(|m| {
                !m.contains(&self.ents[i].id) && !(ko_modal && crate::scene::is_puppet(self, self.ents[i].id))
            })
                || crate::vram::suspended(self, self.ents[i].id)
                    && !matches!(self.ents[i].body, Body::Npc(_))
                    // the 3D world map's patrols are updated while purged out of sprite memory: their idle animation
                    // ticks on through an event's purge (rkb_168 14-1 f1613 .. f1685: the airships out of the
                    // recording's actor list for 59 logic iterations come back 60 ticks on; ours had frozen them -
                    // the 860 differing cel frames of the airships in rkb_168, sb_041, rkb_001)
                    && !(matches!(self.ents[i].body, Body::Enemy(_)) && crate::fieldworld::flying(self));
            crate::vram::popups_tick(self, Some(self.ents[i].id));
            crate::fieldobj::debris_end_before(self, self.ents[i].id);
            crate::scene::emotes_end_before(self, self.ents[i].id);
            let talk_before = self.field.as_ref().is_some_and(|f| f.talk_pass.is_some());
            if !skip {
                let id = self.ents[i].id;
                self.update_entity(i);
                crate::scene::track_after_update(self, id);
                // a talk started in this update runs its event as the game's nested loop: the rest of this frame's pass
                // (the older objects) is not updated (db7_010 14-10 f258: man 2's talk - the room's three enemies go
                // idle with a fresh rand(255) tick and do not tick it in that frame; ours had, a tick ahead for 400
                // frames)
                if !talk_before && self.field.as_ref().is_some_and(|f| f.talk_pass.is_some()) {
                    break;
                }
                // it destroyed an object before it: the list compacted, the same object is at the next index
                if self
                    .field
                    .as_mut()
                    .is_some_and(|f| std::mem::take(&mut f.list_redo))
                {
                    if let Some(&Some(j)) = live(self).get(c as usize) {
                        let id = self.ents[j].id;
                        self.update_entity(j);
                        crate::scene::track_after_update(self, id);
                    }
                    if let Some(f) = self.field.as_mut() {
                        f.list_redo = false;
                    }
                }
            }
            c -= 1;
            if !item_get_before && self.field.as_ref().is_some_and(|f| f.item_get.is_some()) {
                // (the objects left: their update comes when the presentation's loop returns into this pass)
                let rest: Vec<EntityId> = live(self)
                    .into_iter()
                    .take((c + 1).max(0) as usize)
                    .flatten()
                    .map(|k| self.ents[k].id)
                    .collect();
                if let Some(f) = self.field.as_mut() {
                    f.item_get_rest = rest;
                }
                break;
            }
        }
        // the HUD (an early object of the area's list: updated after the actors)
        // (a cut's interrupted pass: the HUD is a later list object here, its update was done - `finish_pass_from`)
        if from.is_none() {
            crate::field::hud_tick(self);
            // the task list (screen shakes) runs after the objects; field: with the camera after the frame's scripts
            // and commands (session.rs `step_post`)
            if self.field.is_none() {
                self.tick_shakes();
            } else {
                self.g.shake_due = self.frame;
            }
        }
        crate::vram::reap(self);
        self.ents.retain(|e| !e.removed);
        // a player object created in this pass is the list's newest object
        if self
            .field
            .as_mut()
            .is_some_and(|f| std::mem::take(&mut f.player_new))
        {
            if let Some(k) = self.player_idx(0) {
                let e = self.ents.remove(k);
                self.ents.push(e);
            }
        }
    }

    fn update_entity(&mut self, i: usize) {
        match &self.ents[i].body {
            Body::Player(_) => crate::player::update(self, i),
            Body::Enemy(_) => crate::enemy::update(self, i),
            Body::Shot(_) => crate::shots::update(self, i),
            Body::Fx(_) => crate::fx::update(self, i),
            Body::Rock { .. } => {}
            Body::Pickup(_) => crate::fieldobj::pickup_update(self, i),
            Body::Breakable(_) => crate::fieldobj::breakable_update(self, i),
            Body::MapSprite(_) => crate::fieldobj::map_sprite_update(self, i),
            Body::Npc(_) => crate::scene::npc_update(self, i),
            Body::Mod(_) => crate::modkit::shot_update(self, i),
            Body::Hand(_) => crate::enemy::hand_update(self, i),
            Body::Injector(_) => {
                let (hits, pid, ai) = match &self.ents[i].body {
                    Body::Injector(j) => {
                        let now = if j.now != 0 { j.now } else { self.frame };
                        (
                            j.hits
                                .iter()
                                .filter(|h| h.at == now)
                                .cloned()
                                .collect::<Vec<_>>(),
                            j.player,
                            j.ai.iter()
                                .filter(|a| a.0 == now)
                                .cloned()
                                .collect::<Vec<_>>(),
                        )
                    }
                    _ => unreachable!(),
                };
                if hits.is_empty() {
                    for (_, id, st) in ai {
                        if let Some((k, _)) = self.enemy_by_id(id) {
                            crate::nodes::replace_top(self, k, st);
                        }
                    }
                }
                for h in hits {
                    if let Some(pi) = self.find(pid) {
                        let a = &self.pl_ref(pi).a;
                        let (px, py) = (a.px(), a.py());
                        let stats = CombatStats {
                            str_: h.str_,
                            pow: h.pow,
                            end: 0,
                            end2: 0,
                            res_m: 128,
                            res_e: 128,
                            crit_bonus: 0,
                            block_factor: 0x1000,
                        };
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
                            no_react: false,
                            foot: false,
                        };
                        broadcast(self, &mut m, false);
                        // the emulator injects before the frame's script phase: the death script counts from this
                        // frame
                        let f = self.frame as i64;
                        if let Some(pi) = self.find(pid) {
                            let p = self.pl(pi);
                            if p.dead && p.ko_at == f + 1 {
                                p.ko_at = f;
                            }
                        }
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
    // the field game delivers newest object first (story playthrough: Goku's punch on two fighters, game list
    // [player, hud, 209, 211] -> 211 hit first); battles keep the order the TS-sim cross-check pins
    // (TODO(core): verify the battle order against the game itself)
    let field = w.field.is_some();
    for j in 0..n {
        let k = if field { n - 1 - j } else { j };
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
        Body::Breakable(_) => crate::fieldobj::breakable_message(w, k, msg),
        Body::Hand(_) if w.field.is_some() => crate::enemy::hand_message(w, k, msg),
        _ => false,
    }
}
