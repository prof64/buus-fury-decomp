//! Enemies (enemy table 0x08711274): type stats, HP stored <<10 (obj+0x28C), the AI controller (ai.rs / nodes.rs),
//! the hit path (aux tree -> ApplyDamage -> ComputeDamage 0x0800E9D8 -> HP; death node = type +0x14 word +8),
//! enemy grid marks (map object +0xB0), the verified steps (path follower, back-off, step-away, wander, straight
//! walk), knockback / critical launch / wall splat, and the helpers the AI action nodes use.
//! Every function takes (world, entity index); the reference is site/src/battle/rulesets/buu/enemy.ts.

use serde::{Deserialize, Serialize};
use crate::actor::Actor;
use crate::ai::AiCtl;
use crate::content::AnimRef;
use crate::damage::{atk, compute_damage, AttackType};
use crate::fixed::round_div_sqrt;
use crate::geom::{facing_toward, overlaps, Dir, Rect, DIR_VEC};
use crate::player::{self, HoldKind};
use crate::shots::{self, OnHit, Shot, ShotKind, StraightOpts};
use crate::world::{broadcast, Body, EntityId, Event, Hand, HitMsg, Layer, World};
use std::sync::Arc;

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Enemy {
    pub a: Actor,
    pub ty: i32,
    pub hp1024: i64,
    pub max_hp1024: i64,
    pub ai: AiCtl,
    pub counter: i64,
    pub mark1: i64,
    pub mark2: i64,
    pub state_stamp: i64,
    pub stunned: bool,
    pub last_hit_dir: Dir,
    /// obj+0x294: the last accepted hit was critical
    pub crit_hit: bool,
    /// obj+0x298: the last accepted hit used attack sub 3 (a wall-splat hit)
    pub splat_hit: bool,
    pub vx: i32,
    pub vy: i32,
    pub moving: bool,
    /// a movement step this frame found the walk not running (its SetAnim(1) is due; 0x0803B9DA tests IsAnim(1) at the
    /// step, so a node that sets another animation after it - a GotoState into StandStill - keeps that one)
    #[serde(default)]
    pub walk_due: bool,
    pub slot: i32,
    pub children: Vec<EntityId>,
    pub parent: EntityId,
    pub defeated: bool,
    pub ko: i32,
    pub powered: bool,
    pub beam: EntityId,
    pub drain_done: bool,
    pub spawned: i32,
    pub enz_timer: i32,
    pub shield: i32,
    pub volleyball: bool,
    pub part: bool,
    pub primary: bool,
    pub next: EntityId,
    pub prev: EntityId,
    pub cmds: Vec<Dir>,
    pub worm_dir: Dir,
    pub contact_cd: i32,
    pub part_offset: (i32, i32),
    /// field Janemba part 0x37 (PartMirrorAnim): the body animation (raw) it saw on its previous update - its table
    /// follows an animation switch one frame late (sc_janemba)
    #[serde(default)]
    pub mirror_raw: i32,
    /// ... and the body's cel then (under the ascent's controller animation, raw 20, the frame byte it reads lags one
    /// frame more); `mirror_prev`: the body's raw on this part's previous update (every part: the hurt mirror)
    #[serde(default)]
    pub mirror_cel: i32,
    #[serde(default)]
    pub mirror_prev: i32,
    /// field Janemba arm part (0x34 / 0x35): the hand strike's controller ticks (0 = none) and its side (A = 0x34)
    #[serde(default)]
    pub strike: (i32, bool),
    /// ... and the hand it made (0 = none)
    #[serde(default)]
    pub strike_hand: EntityId,
    pub script_hits: i32,
    /// tiles this enemy marked blocked on the enemy grid
    pub marked: Option<[i32; 4]>,
    pub stepped: bool,
    /// the player slot this enemy plays against (the original has one player: slot 0)
    pub target: u8,
    /// the game's animation timing (eanim.rs): obj+0x50 raw / +0x11 frame / +0x12 tick under the state rule
    pub ea: crate::eanim::EnemyAnim,
    /// the last one-shot ran out (its end callback fired)
    pub anim_ended: bool,
    /// the current animation runs under a node's own state object (a forced rule, e.g. the back-off's reversed walk
    /// 0x03002B9C), not the SetAnim table's: IsAnim (vfunc +0x34, 0x0800FAC6) is false
    pub own_state: bool,
    /// a node's own state object with per-frame code (SetState; tick + think run in the actor update before the AI;
    /// any SetAnim replaces it): 0 none, else a [`Hook`]
    pub hook: Hook,
    /// Dabura's spit controller 0x030030C8 (tick 0x0802DE7C): ticks since its SetState, -1 = not the current state
    pub spit_ctl: i32,
    /// +1 per state entered through SetAnim (a node can tell its controller was replaced)
    pub anim_seq: u32,
    /// the shown graphics set when MechTransform swapped it (gfx id; -1 = the type's)
    pub gfx_now: i32,
    /// obj+0x291 set by a node (0x0800F924 from the Detach node's enter 0x0804E53C - the Ghost's first state): off
    /// the enemy grid for good (no marks, every spot fits)
    #[serde(default)]
    pub detached: bool,
    /// JumpSmash's controller 0x0300325C goes on for two ticks after the landing (ticks 249 / 250): three dust
    /// clouds each, before the AI (field; Volcano Summit 15-32)
    #[serde(default)]
    pub smash_puffs: u8,
    /// a RandomStep node is moving the actor around its kept position: the grid mark stays at that position (field)
    #[serde(default)]
    pub jitter: bool,
    /// field: the beam of a left KidBuuKamehameha stops at this enemy's second update after (nodes.rs)
    #[serde(default)]
    pub beam_stop_due: u8,
    /// the running motion-controller leaf (state serial, node): its controller ticks with the animation, also while
    /// an event holds the field and the AI does not think (nodes.rs `busy_controller`)
    #[serde(default)]
    pub ctl: (u32, u16),
}

/// the node-installed state objects with per-frame code (`Enemy.stateHook` of the reference)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hook {
    #[default]
    None,
    /// LatchDrain's controller (0x0804C314): glued to the player at the offset kept by its enter (24.8)
    Latch { ox: i32, oy: i32 },
    /// the Buu fragment's PlayAnim0 state 0x03003158
    Fragment,
    /// the Buu fragment's FreezePlayer state 0x0300314C
    Freeze,
}

impl Enemy {
    pub fn new(w: &World, ty: i32) -> Enemy {
        let d = &w.data;
        let t = d.enemy(ty).expect("enemy type");
        let mut a = Actor { stats: crate::damage::CombatStats { str_: t.str_, pow: t.pow, end: t.end, end2: t.end, res_m: t.res_m, res_e: t.res_e, crit_bonus: 0, block_factor: 0x1000 }, ..Default::default() };
        a.set_sprite(d, t.sprite);
        let mut e = Enemy {
            a,
            ty,
            hp1024: t.hp * 1024,
            max_hp1024: t.hp * 1024,
            ai: AiCtl::default(),
            counter: 0,
            mark1: -100000,
            mark2: -100000,
            // obj+0x210: set only by the Block node's exit (never set = long ago)
            state_stamp: -100000,
            stunned: false,
            last_hit_dir: 0,
            crit_hit: false,
            splat_hit: false,
            vx: 0,
            vy: 0,
            moving: false,
            walk_due: false,
            slot: 0,
            children: vec![],
            parent: 0,
            defeated: false,
            ko: 0,
            powered: false,
            beam: 0,
            drain_done: false,
            spawned: 0,
            enz_timer: 120,
            shield: 0,
            volleyball: false,
            part: false,
            primary: false,
            next: 0,
            prev: 0,
            cmds: vec![],
            worm_dir: 0,
            contact_cd: 0,
            part_offset: (0, 0),
            mirror_raw: 0,
            mirror_cel: 0,
            mirror_prev: 0,
            strike: (0, false),
            strike_hand: 0,
            script_hits: 0,
            marked: None,
            stepped: false,
            target: 0,
            ea: Default::default(),
            anim_ended: false,
            own_state: false,
            hook: Hook::None,
            spit_ctl: -1,
            anim_seq: 0,
            gfx_now: -1,
            detached: false,
            smash_puffs: 0,
            jitter: false,
            beam_stop_due: 0,
            ctl: (0, 0),
        };
        // the constructor's SetAnim(0) runs before the object is in the world: no RNG draw
        e.set_anim_with(d, &[AnimRef::Name("idle")], true, Some(true), None, None, &mut Vec::new());
        e
    }
    /// SetAnim / SetAnimRaw + SetState: the state rule for the raw comes from the type's recorded rules, else the
    /// enemy SetAnim table, else a generic loop / one-shot; `rule` forces one. Re-entering restarts the state.
    #[allow(clippy::too_many_arguments)]
    pub fn set_anim_with(&mut self, d: &crate::content::GameData, refs: &[AnimRef], restart: bool, looped: Option<bool>, rule: Option<u32>, rng: Option<&mut crate::rng::Rng>, events: &mut Vec<Event>) {
        self.spit_ctl = -1;
        self.hook = Hook::None;
        let prev = self.a.anim;
        // the sprite lacks the requested animation: the game's state object still runs (no idle state, no rand(255));
        // the idle animation is only shown (lockstep f02_vs_enemy_gun: GunShot on a sprite without raw 18)
        let idle_ref = |r: &AnimRef| matches!(r, AnimRef::Name("idle") | AnimRef::N(0));
        let missing = match self.a.sprite.and_then(|s| d.sprite(s)) {
            Some(sp) => {
                !refs.iter().any(idle_ref)
                    && !refs.iter().any(|r| match r {
                        AnimRef::N(n) => sp.by_raw.contains_key(&(n + 1)),
                        AnimRef::Name(s) => sp.by_name.contains_key(*s),
                    })
            }
            None => false,
        };
        self.a.set_anim(d, refs, restart, looped);
        let Some(raw) = self.a.anim_raw(d) else { return };
        if self.a.anim == prev && !restart && rule.is_none() {
            return;
        }
        let ar = &d.anim;
        // SetAnim(id) 0x0800ECF0: the state object comes from the enemy table 0x080726B8 by id, only the raw from the
        // sprite def: SetAnim(0) is always the idle state, SetAnim(1) the walk loop (a type's recorded walk
        // controller 0x0804A1D0@0x03001B58 when there is one)
        let plain = rule.is_none() && !missing;
        let single = if refs.len() == 1 { Some(refs[0]) } else { None };
        let by_id = match single {
            Some(AnimRef::Name("idle")) | Some(AnimRef::N(0)) if plain => Some(ar.setanim[&1]),
            Some(AnimRef::Name("walk")) | Some(AnimRef::N(1)) if plain => Some(ar.types.get(&self.ty).and_then(|m| m.get("0x0804A1D0@0x03001B58")).copied().unwrap_or(ar.setanim[&2])),
            _ => None,
        };
        let r = if missing { Some(ar.hold) } else { None }
            .or(rule)
            .or(by_id)
            .or_else(|| ar.types.get(&self.ty).and_then(|m| m.get(&raw.to_string())).copied())
            .or_else(|| ar.setanim.get(&raw).copied())
            .unwrap_or(if looped == Some(false) { ar.fallback_once } else { ar.fallback_loop });
        self.anim_ended = false;
        self.own_state = rule.is_some();
        self.anim_seq = self.anim_seq.wrapping_add(1);
        let mut h = EHost { d, sprite: self.a.sprite, facing: self.a.facing, rng, events };
        self.ea.enter(&ar.rules, raw, r, &mut h);
    }
    /// IsAnim(id) (vfunc +0x34, 0x0800FAC6): the state object is SetAnim(id)'s and the raw matches (a forced rule or a
    /// node's own state object is not)
    pub fn is_anim(&self, id: i32) -> bool {
        !self.own_state && self.hook == Hook::None && self.ea.pos.raw == id + 1
    }
    /// IsAnim(1): the walk loop state is running (not the back-off's reversed walk showing the same raw)
    pub fn walking(&self) -> bool {
        self.is_anim(1)
    }
    /// SetAnimRaw 0x0800ECDA: obj+0x50 only (the state object and its frame / tick stay)
    pub fn set_raw(&mut self, d: &crate::content::GameData, raw: i32) {
        self.ea.pos.raw = raw;
        if let Some(&k) = self.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.by_raw.get(&raw)) {
            self.a.anim = Some(k);
        }
    }
    /// the state's tick before the AI; a one-shot's end callback is SetAnim(0)
    pub fn tick_anim(&mut self, d: &crate::content::GameData, rng: &mut crate::rng::Rng, events: &mut Vec<Event>) {
        self.a.tick_anim();
        if self.a.anim.is_none() {
            return;
        }
        let res = {
            let mut h = EHost { d, sprite: self.a.sprite, facing: self.a.facing, rng: Some(&mut *rng), events };
            self.ea.tick(&d.anim.rules, &mut h)
        };
        match res {
            crate::eanim::Tick::Idle => {
                self.set_anim_with(d, &[AnimRef::Name("idle")], true, Some(true), None, Some(rng), events);
                self.anim_ended = true;
            }
            crate::eanim::Tick::None => {
                if self.a.anim_raw(d) != Some(self.ea.pos.raw) {
                    if let Some(&k) = self.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.by_raw.get(&self.ea.pos.raw)) {
                        self.a.anim = Some(k);
                    }
                }
            }
        }
    }
    pub fn anim_done(&self, d: &crate::content::GameData) -> bool {
        if self.a.anim.is_none() {
            return true;
        }
        use crate::eanim::RuleKind::*;
        match d.anim.rules[self.ea.rule as usize].kind {
            Once | OnceStop | Seq => self.ea.done || self.anim_ended,
            _ => self.anim_ended || self.a.anim_done(d),
        }
    }
    /// obj+0x11
    pub fn anim_frame(&self) -> i32 {
        if self.a.anim.is_some() {
            self.ea.pos.frame
        } else {
            0
        }
    }
    /// MeleeAttack 0x0802A0C4: the number of punch animations (raw 5 / 6 / 7 present, create fn 0x0802A288)
    pub fn punch_anims(&self, d: &crate::content::GameData) -> u32 {
        let Some(s) = self.a.sprite.and_then(|s| d.sprite(s)) else { return 0 };
        if s.anim_raw(7).is_some() {
            3
        } else if s.anim_raw(6).is_some() {
            2
        } else if s.anim_raw(5).is_some() {
            1
        } else {
            0
        }
    }
    pub fn hp(&self) -> i64 {
        (self.hp1024 + 1023).div_euclid(1024)
    }
}

struct EHost<'a> {
    d: &'a crate::content::GameData,
    sprite: Option<i32>,
    facing: Dir,
    rng: Option<&'a mut crate::rng::Rng>,
    events: &'a mut Vec<Event>,
}
impl crate::eanim::Host for EHost<'_> {
    fn nframes(&mut self, raw: i32) -> i32 {
        self.sprite.and_then(|s| self.d.sprite(s)).and_then(|s| s.anim_raw(raw)).map(|a| a.steps(self.facing).len() as i32).unwrap_or(0)
    }
    fn rand(&mut self, n: u32) -> i32 {
        match self.rng.as_mut() {
            Some(r) => r.rand(n),
            None => 0,
        }
    }
    fn sfx(&mut self, id: i32, vol: i32) {
        self.events.push(Event::Sfx { id, vol });
    }
}

pub fn set_anim(w: &mut World, i: usize, refs: &[AnimRef], restart: bool, looped: Option<bool>) {
    set_anim_rule(w, i, refs, restart, looped, None)
}
/// a movement step: `moving`, and the walk's SetAnim(1) is due unless it is running now
pub fn mark_moving(w: &mut World, i: usize) {
    let walking = w.en_ref(i).walking();
    let e = w.en(i);
    e.moving = true;
    if !walking {
        e.walk_due = true;
    }
}

pub fn set_anim_rule(w: &mut World, i: usize, refs: &[AnimRef], restart: bool, looped: Option<bool>, rule: Option<u32>) {
    let d = Arc::clone(&w.data);
    let mut ev = std::mem::take(&mut w.events);
    let mut rng = w.rng;
    w.en(i).set_anim_with(&d, refs, restart, looped, rule, Some(&mut rng), &mut ev);
    w.rng = rng;
    w.events = ev;
}
pub fn tick_anim(w: &mut World, i: usize) {
    let d = Arc::clone(&w.data);
    let mut ev = std::mem::take(&mut w.events);
    let mut rng = w.rng;
    w.en(i).tick_anim(&d, &mut rng, &mut ev);
    w.rng = rng;
    w.events = ev;
}
/// the sound of an accepted melee hit on the player (0x0802A1D4): gfx 0x60 / 0xE5 / 0xDC -> 30, 0xEE / 0xF4 / 0xED
/// -> 62, else 48 + rand(2)
pub fn melee_hit_sfx(w: &mut World, i: usize) {
    let g = w.data.enemy(w.en_ref(i).ty).and_then(|t| t.gfx).unwrap_or(-1);
    if g == 0x60 || g == 0xe5 || g == 0xdc {
        w.sfx(30, 64);
    } else if g == 0xee || g == 0xf4 || g == 0xed {
        w.sfx(62, 64);
    } else {
        let s = 48 + w.rng.rand(2);
        w.sfx(s, 64);
    }
}
fn n(v: i32) -> AnimRef {
    AnimRef::N(v)
}
fn nm(s: &'static str) -> AnimRef {
    AnimRef::Name(s)
}

/// the target player object ([0x03001B94]): present and not removed - still there after a KO (the AI's distance
/// tests keep using its position)
pub fn player(w: &World, i: usize) -> Option<usize> {
    let slot = w.en_ref(i).target as usize;
    let pi = w.player_idx(slot).or_else(|| w.player_idx(0))?;
    if w.ents[pi].removed {
        None
    } else {
        Some(pi)
    }
}
/// the target player even when dead (ctx.player)
pub fn ctx_player(w: &World, i: usize) -> Option<usize> {
    let slot = w.en_ref(i).target as usize;
    w.player_idx(slot).or_else(|| w.player_idx(0))
}

// ---- update ----------------------------------------------------------------------------------------------------

pub fn update(w: &mut World, i: usize) {
    if crate::scene::puppet_update(w, i) {
        return;
    }
    if w.en_ref(i).beam_stop_due > 0 {
        w.en(i).beam_stop_due -= 1;
        if w.en_ref(i).beam_stop_due == 0 {
            crate::nodes::stop_beam_of(w, i);
        }
    }
    // while an event holds the field only the animation ticks (like the map NPCs; story playthrough: the
    // fighters a cutscene hands over with become_map_object start their AI when the event ends)
    // a load waiting for its on-enter script holds them too (like the map NPCs; segment sb 21-4: the Blister's spawn
    // timer starts after the 2 held frames)
    // (the objects a load made inside the event run on - the 3D world map's patrols only: a 2D area's enemies are
    // held like any others; 0-21, s026's warp_fade: Majin Buu's tree draws nothing until the event ends)
    let id = w.ents[i].id;
    // (through the map's arrival fade only: the frame after it the event holds them too - sb 21-25 f64759, the world
    // map after Kid Buu: the patrols stop when the 3D fade-in ends while s-script still runs)
    let own_load = crate::fieldworld::flying(w)
        && w.field.as_ref().is_some_and(|f| f.load_ids.0 <= id && id < f.load_ids.1 && matches!(f.mode, crate::field::Mode::FadeIn { .. }));
    // (the item-get presentation holds them the same way, all 60 frames: Diablo Desert Express 11-22 - a Mad
    // Bomber's Wait(90) does not count while the player holds a dropped item up; shots fly on)
    if w.field.as_ref().is_some_and(|f| f.busy && !own_load || f.arrival.is_some() || f.item_get.as_ref().is_some_and(|g| g.left > 0)) {
        tick_anim(w, i);
        if w.en_ref(i).part && matches!(w.en_ref(i).ty, 0x34 | 0x35 | 0x37) {
            part_hurt_end(w, i);
        }
        crate::nodes::busy_controller(w, i);
        return;
    }
    // the player's death script (0x08799D0C) blocks every object update at KO + 199; from KO + 200 an enemy only runs
    // its state tick (caller 0x0800F5E0: no AI)
    let ko_at = if w.players.len() == 1 { ctx_player(w, i).map(|pi| w.pl_ref(pi).ko_at).unwrap_or(-1) } else { -1 };
    if ko_at >= 0 && w.frame as i64 - ko_at >= 199 {
        if w.frame as i64 - ko_at > 199 {
            tick_anim(w, i);
        }
        return;
    }
    tick_anim(w, i);
    if w.en_ref(i).smash_puffs > 0 {
        w.en(i).smash_puffs -= 1;
        smash_dust(w, i);
    }
    update_rest(w, i);
}

/// the landing's three dust clouds (0x08035D04 ..: each rate 0x1000 + rand(0x1000))
pub fn smash_dust(w: &mut World, i: usize) {
    for k in -1..=1 {
        let (cx, _) = w.en_ref(i).a.center();
        let py = w.en_ref(i).a.py();
        crate::fx::puff_rng(w, cx + k * 12, py + 4);
    }
}

/// a held update on its own (field.rs `break_event`: the frames around a breakable callback's nested loop): the
/// animation tick and the running controller, no AI
pub fn held_update(w: &mut World, i: usize) {
    if crate::scene::is_puppet(w, w.ents[i].id) {
        return;
    }
    tick_anim(w, i);
    crate::nodes::busy_controller(w, i);
}

/// the frame an event ended in (field): an enemy that only ticked its animation while the event held the field runs
/// the rest of its update (the game's event exits before the objects update; sc_janemba: Janemba's parts are placed by
/// their AI node in the exit frame)
pub fn release_update(w: &mut World, i: usize) {
    if crate::scene::is_puppet(w, w.ents[i].id) {
        return;
    }
    update_rest(w, i);
}

fn update_rest(w: &mut World, i: usize) {
    if w.en_ref(i).spit_ctl >= 0 {
        let c = w.en_ref(i).spit_ctl;
        w.en(i).spit_ctl = c + 1;
        if c == 0 {
            shot(w, i, ShotType::Spit);
        }
    }
    {
        let e = w.en(i);
        if e.contact_cd > 0 {
            e.contact_cd -= 1;
        }
        if e.defeated && e.ko < 0 {
            // field Idle death: no fade, no removal (its animation ticked above); the object's AI runs on - an
            // enemy the player's update just killed still takes this pass's step (real route 8-71: Majin Buu's
            // slide moves once more in the frame of the last punch; the defeat's event holds the later frames)
        } else if e.defeated {
            e.ko += 1;
            if e.ko > 150 {
                e.a.alpha = (256 - (e.ko - 150) * 256 / 30).max(0);
            }
            if e.ko > 180 {
                w.ents[i].removed = true;
            }
            return;
        }
    }
    if w.en_ref(i).part && w.field.is_some() && matches!(w.en_ref(i).ty, 0x34 | 0x35 | 0x37) {
        return janemba_part_field(w, i);
    }
    if w.en_ref(i).part {
        let par = w.en_ref(i).parent;
        let pinfo = w.enemy_by_id(par).filter(|(pi, p)| !w.ents[*pi].removed && !p.defeated).map(|(_, p)| (p.a.x, p.a.y, p.a.z, p.a.facing));
        match pinfo {
            None => w.ents[i].removed = true,
            Some((x, y, z, f)) => {
                let e = w.en(i);
                e.a.x = x + (e.part_offset.0 << 8);
                e.a.y = y + (e.part_offset.1 << 8);
                e.a.z = z;
                e.a.facing = f;
            }
        }
        return;
    }
    w.en(i).moving = false;
    w.en(i).walk_due = false;
    run_hook(w, i);
    crate::nodes::tick(w, i);
    let d = Arc::clone(&w.data);
    let e = w.en_ref(i);
    let name = e.a.anim_name(&d);
    // a movement step SetAnim(1)s unless already walking (0x0803B9DA: IsAnim(1)); nothing switches back to idle when
    // the movement stops - the next node's own SetAnim does
    if (e.a.anim.is_none() || matches!(name, Some("idle") | Some("walk"))) && e.walk_due && !e.walking() {
        set_anim(w, i, &[nm("walk")], true, Some(true));
    }
}

/// the node-installed state object's tick (vtable +4) and think (+8), before the AI
fn run_hook(w: &mut World, i: usize) {
    match w.en_ref(i).hook {
        Hook::None => {}
        Hook::Latch { ox, oy } => {
            let Some(pi) = player(w, i) else { return };
            let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
            // the think `0x080298A2` writes the position only: the enemy grid keeps the mark where the enzyme
            // latched on (VERIFIED real route 21-6: an enzyme dragged through a wall leaves the wall's tiles blocked)
            // (the game's free / mark `0x0800F740` works on the current footprint: once the enzyme lets go, its next
            // step frees the tiles where it is, not where it latched on - so its mark is taken to be the current
            // footprint, the grid untouched)
            let e = w.en(i);
            e.a.x = px + ox;
            e.a.y = py + oy;
            if e.marked.is_some() {
                let (x, y) = (e.a.px(), e.a.py());
                e.marked = Some([x, y, x + e.a.foot_w, y + e.a.foot_h]);
            }
        }
        Hook::Fragment => {
            frag_tick(w, i);
            frag_think(w, i);
        }
        Hook::Freeze => freeze_tick(w, i),
    }
}

/// SetState(a node's own state object): no tick of its own (the hook runs it); frame / tick kept
fn use_state(w: &mut World, i: usize, h: Hook) {
    let d = Arc::clone(&w.data);
    let e = w.en(i);
    let raw = e.ea.pos.raw;
    let mut ev = Vec::new();
    let mut hh = EHost { d: &d, sprite: e.a.sprite, facing: e.a.facing, rng: None, events: &mut ev };
    e.ea.enter(&d.anim.rules, raw, d.anim.keep, &mut hh);
    e.hook = h;
}
pub fn set_raw(w: &mut World, i: usize, raw: i32) {
    let d = Arc::clone(&w.data);
    w.en(i).set_raw(&d, raw);
}

/// LatchDrain's enter (0x080298D8): SetAnimRaw(1), SetState(controller 0x0804C314: loop, shift 3) whose think
/// (0x080298A2) glues the enzyme to the player at the offset kept by its enter (0x08029860)
pub fn latch_enter(w: &mut World, i: usize) {
    let d = Arc::clone(&w.data);
    set_raw(w, i, 1);
    {
        let e = w.en(i);
        let mut ev = Vec::new();
        let mut hh = EHost { d: &d, sprite: e.a.sprite, facing: e.a.facing, rng: None, events: &mut ev };
        e.ea.enter(&d.anim.rules, 1, d.anim.loop3, &mut hh);
    }
    let Some(pi) = player(w, i) else { return };
    let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
    let e = w.en(i);
    e.hook = Hook::Latch { ox: e.a.x - px, oy: e.a.y - py };
}

// ---- Buu fragment (type 0x0E): PlayAnim0 / FreezePlayer ---------------------------------------------------------------

/// PlayAnim0 enter 0x0804E5E8: frame = tick = 0, SetAnimRaw(0), SetState(0x03003158), whose enter 0x08030618 runs the
/// think once
pub fn frag_enter(w: &mut World, i: usize) {
    {
        let e = w.en(i);
        e.ea.pos.frame = 0;
        e.ea.pos.tick = 0;
    }
    set_raw(w, i, 1);
    use_state(w, i, Hook::Fragment);
    frag_think(w, i);
}
/// state tick 0x080307FA: mode 0 frame 0; mode 1 frame 1 at tick 0x30, 2 at 0x50; tick = (tick + 8) & 0x7F (the height
/// bob is cosmetic and not modelled)
fn frag_tick(w: &mut World, i: usize) {
    let mode = w.g.frag.0;
    let pos = &mut w.en(i).ea.pos;
    let t = pos.tick;
    if mode == 0 {
        pos.frame = 0;
    } else if mode == 1 {
        if t == 0x30 {
            pos.frame = 1;
        } else if t == 0x50 {
            pos.frame = 2;
        }
    }
    pos.tick = (t + 8) & 0x7f;
}
/// state think 0x08030630 (the state object is one IWRAM singleton: its mode and step are shared by every fragment)
fn frag_think(w: &mut World, i: usize) {
    let Some(pi) = player(w, i) else { return };
    if w.en_ref(i).ea.pos.tick == 0 {
        let (pcx, pcy) = w.pl_ref(pi).a.fc();
        let (ecx, ecy) = w.en_ref(i).a.fc();
        let dx = pcx - ecx;
        let dy = pcy - ecy;
        let vx = if dx > 0 { 256 } else { -256 };
        let vy = if dy > 0 { 256 } else { -256 };
        let rx = pcx - (ecx + if dx > 0 { 32 } else { -32 });
        let ry = pcy - (ecy + if dy > 0 { 32 } else { -32 });
        let sx = if rx > 0 { 256 } else { -256 };
        let sy = if ry > 0 { 256 } else { -256 };
        let mode = ((vx * sx) >> 8 < 0 && (vy * sy) >> 8 < 0) as i32;
        w.g.frag = (mode, vx, vy);
        w.en(i).ea.pos.frame = 0;
        if mode == 1 {
            set_raw(w, i, 15);
        } else {
            w.en(i).ea.pos.tick = 0;
            set_raw(w, i, 1);
        }
    }
    let (mode, vx, vy) = w.g.frag;
    let k = if mode != 0 { 2 } else { 1 };
    let (nx, ny) = (w.en_ref(i).a.x + vx * k, w.en_ref(i).a.y + vy * k);
    unmark_self(w, i);
    if fits(w, i, nx >> 8, ny >> 8) {
        let e = w.en(i);
        e.a.x = nx;
        e.a.y = ny;
    }
    mark_self(w, i);
    let no_z = |a: &Actor| {
        let b = a.body_box();
        Rect::new(b.x0, b.y0 - a.z, b.x1, b.y1 - a.z)
    };
    if overlaps(&no_z(&w.en_ref(i).a), &no_z(&w.pl_ref(pi).a)) {
        let d = Arc::clone(&w.data);
        let e = w.en(i);
        e.hook = Hook::None;
        let raw = e.ea.pos.raw;
        let mut ev = Vec::new();
        let mut hh = EHost { d: &d, sprite: e.a.sprite, facing: e.a.facing, rng: None, events: &mut ev };
        e.ea.enter(&d.anim.rules, raw, d.anim.keep, &mut hh);
    }
}
/// FreezePlayer enter 0x080305AA, fragment side: SetAnimRaw(0x12) + SetState(0x0300314C): tick = frame = 0, at the
/// player's position + (0, 1 px)
pub fn freeze_enter(w: &mut World, i: usize) {
    set_raw(w, i, 19);
    {
        let e = w.en(i);
        e.ea.pos.frame = 0;
        e.ea.pos.tick = 0;
    }
    if let Some(pi) = player(w, i) {
        let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
        unmark_self(w, i);
        {
            let e = w.en(i);
            e.a.x = px;
            e.a.y = py + 256;
        }
        mark_self(w, i);
    }
    use_state(w, i, Hook::Freeze);
}
/// the freeze state: tick 0x08030584 (frames 4 / 5 loop) and think 0x08030538 (a d-pad press -> tick + 2; the last 16
/// A/B/d-pad presses within 90 frames, one now -> the player SetAnim(0), frame = tick = 0, SetAnim(0))
fn freeze_tick(w: &mut World, i: usize) {
    {
        let pos = &mut w.en(i).ea.pos;
        pos.tick += 1;
        if pos.tick > 7 {
            pos.tick = 0;
            pos.frame += 1;
            if pos.frame > 5 {
                pos.frame = 4;
            }
        }
    }
    let slot = w.en_ref(i).target as usize;
    if w.g.pressed(slot) & 0xf0 != 0 {
        w.en(i).ea.pos.tick += 2;
    }
    if w.g.press_run(slot, 0xf3, 16, 90, w.frame) {
        if let Some(pi) = player(w, i) {
            player::release(w, pi);
        }
        {
            let pos = &mut w.en(i).ea.pos;
            pos.frame = 0;
            pos.tick = 0;
        }
        set_anim(w, i, &[nm("idle")], true, Some(true));
    }
}

// ---- geometry ----------------------------------------------------------------------------------------------------

/// footprint centre to footprint centre (0x0800FDFA box: the distance conditions 0x08029C90 / 0x08029BE0 compare
/// dx*dx + dy*dy <= r2)
pub fn to_player(w: &World, i: usize) -> (i32, i32) {
    let Some(pi) = player(w, i) else { return (9999, 9999) };
    let (px, py) = w.pl_ref(pi).a.foot_center();
    let (x, y) = w.en_ref(i).a.foot_center();
    (px - x, py - y)
}
pub fn dist2(w: &World, i: usize) -> i64 {
    if player(w, i).is_none() {
        return i64::MAX;
    }
    let (dx, dy) = to_player(w, i);
    dx as i64 * dx as i64 + dy as i64 * dy as i64
}
pub fn face_player(w: &mut World, i: usize) {
    if player(w, i).is_none() {
        return;
    }
    // the field: the game's routine 0x0801599C (the nodes' enters: FlameBreath 0x08023FA8, TurretFire, SoldierThrow,
    // CyborgShot ...; Babidi's ship Destroyers / turrets); battles keep the footprint rule cross_ts pins
    if w.field.is_some() {
        return face_player_exact(w, i);
    }
    let (dx, dy) = to_player(w, i);
    w.en(i).a.facing = facing_toward(dx, dy);
}
/// FacePlayer node 0x0801D047 -> 0x0801599C (the cutscene face_actor routine): the 24.8 position difference, the
/// dominant axis with ties horizontal, no turn when equal (story playthrough: a fighter turns 2 frames later than the
/// footprint-pixel rule)
pub fn face_player_exact(w: &mut World, i: usize) {
    let Some(pi) = player(w, i) else { return };
    let (dx, dy) = (w.pl_ref(pi).a.x - w.en_ref(i).a.x, w.pl_ref(pi).a.y - w.en_ref(i).a.y);
    let d = if dx.abs() >= dy.abs() {
        match dx.signum() {
            1 => 3,
            -1 => 2,
            _ => return,
        }
    } else if dy > 0 {
        0
    } else {
        1
    };
    w.en(i).a.facing = d;
}
pub fn muzzle(w: &World, i: usize, d: i32) -> (i32, i32) {
    let a = &w.en_ref(i).a;
    let (cx, cy) = a.center();
    let (dx, dy) = DIR_VEC[a.facing as usize];
    (cx + dx * d, cy + dy * d)
}
const WAVE_SOLDIER: [(i32, i32); 4] = [(2, 8), (9, -28), (-16, -11), (24, -11)];
const WAVE_TURRET: [(i32, i32); 4] = [(4, 4), (4, -25), (-13, -14), (22, -13)];
pub fn muzzle_fix(w: &World, i: usize, turret: bool) -> (i32, i32) {
    let a = &w.en_ref(i).a;
    let (ox, oy) = if turret { WAVE_TURRET } else { WAVE_SOLDIER }[a.facing as usize];
    ((a.px() + ox) << 8, (a.py() + oy) << 8)
}
/// 0x08029ED8: the body boxes must overlap the enemy's box grown to x0 - 4 .. x1 + 3, y0 .. y1 - 6; then the enemy
/// turns toward the player like MeleeAttack (strike_rect) and the result is whether the player's body box overlaps
/// that strike rect (the turn stays when it does not)
pub fn in_melee_range(w: &mut World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let p = &w.pl_ref(pi).a;
    let b = w.en_ref(i).a.body_box();
    if !p.hittable || !overlaps(&p.body_box(), &Rect::new(b.x0 - 4, b.y0, b.x1 + 3, b.y1 - 6)) {
        return false;
    }
    let Some(r) = strike_rect(w, i, true) else { return false };
    overlaps(&w.pl_ref(pi).a.body_box(), &r)
}
/// 0x0800FF44: the shot origin on the body box: down (cx, position y), up (cx, cy), left (x0, cy), right (x1, cy)
pub fn hand_point(w: &World, i: usize) -> (i32, i32) {
    let a = &w.en_ref(i).a;
    let b = a.body_box();
    let (cx, cy) = ((b.x0 + b.x1) >> 1, (b.y0 + b.y1) >> 1);
    match a.facing {
        0 => (cx, a.py()),
        1 => (cx, cy),
        2 => (b.x0, cy),
        _ => (b.x1, cy),
    }
}
pub fn player_attacking_me(w: &World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let p = w.pl_ref(pi);
    p.attacking() && overlaps(&p.a.body_box().inflate(8), &w.en_ref(i).a.body_box())
}
pub fn player_in_front(w: &mut World, i: usize, reach: i32) -> bool {
    let Some(pi) = player(w, i) else { return false };
    if !w.pl_ref(pi).a.hittable {
        return false;
    }
    let (dx, dy) = to_player(w, i);
    let f = facing_toward(dx, dy);
    let b = w.en_ref(i).a.body_box();
    let r = match f {
        3 => Rect::new(b.x1, b.y0, b.x1 + reach, b.y1),
        2 => Rect::new(b.x0 - reach, b.y0, b.x0, b.y1),
        0 => Rect::new(b.x0, b.y1, b.x1, b.y1 + reach),
        _ => Rect::new(b.x0, b.y0 - reach, b.x1, b.y0),
    };
    if !overlaps(&r, &w.pl_ref(pi).a.body_box()) {
        return false;
    }
    w.en(i).a.facing = f;
    true
}
fn corridor(b: &Rect, dir: Dir) -> Rect {
    let (dx, dy) = DIR_VEC[dir as usize];
    if dx > 0 {
        Rect::new(b.x1, b.y0 - 4, b.x1 + 240, b.y1 + 4)
    } else if dx < 0 {
        Rect::new(b.x0 - 240, b.y0 - 4, b.x0, b.y1 + 4)
    } else if dy > 0 {
        Rect::new(b.x0 - 4, b.y1, b.x1 + 4, b.y1 + 240)
    } else {
        Rect::new(b.x0 - 4, b.y0 - 240, b.x1 + 4, b.y0)
    }
}
pub fn in_player_beam_line(w: &World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let p = w.pl_ref(pi);
    if !p.charging() {
        return false;
    }
    overlaps(&corridor(&p.a.body_box(), p.a.facing), &w.en_ref(i).a.body_box())
}
pub fn blast_incoming(w: &World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let bid = w.pl_ref(pi).blast;
    let Some(bi) = w.find(bid) else { return false };
    if w.ents[bi].removed {
        return false;
    }
    let s = w.shot_ref(bi);
    let e = &w.en_ref(i).a;
    if (s.dir ^ 1) != e.facing {
        return false;
    }
    overlaps(&corridor(&e.body_box(), e.facing), &shots::rect(w, bi))
}

// ---- map collision (enemy grid) -------------------------------------------------------------------------------

pub fn attached(w: &World, i: usize) -> bool {
    let e = w.en_ref(i);
    let held = ctx_player(w, i).is_some_and(|pi| w.pl_ref(pi).held_by == w.ents[i].id && w.ents[i].id != 0);
    // field: the height does not detach (sc_janemba: a hopping Mini-Janemba keeps marking the enemy grid)
    // a field enemy whose death node is Idle stays the object it was (`ko < 0`): still on the enemy grid, so the
    // step it takes in the pass of the killing hit is collision-checked like the ones before (0-21: Majin Buu's
    // slide, blocked at the wall, stays blocked on the killing frame; 8-71: a free slide moves once more)
    let gone = e.defeated && !(w.field.is_some() && e.ko < 0);
    !e.part && e.prev == 0 && !gone && !e.detached && !w.ents[i].removed && (e.a.z == 0 || w.field.is_some()) && !held
}
pub fn mark_self(w: &mut World, i: usize) {
    if w.col.is_none() || w.en_ref(i).marked.is_some() || !attached(w, i) {
        return;
    }
    let a = &w.en_ref(i).a;
    let m = [a.px(), a.py(), a.px() + a.foot_w, a.py() + a.foot_h];
    w.en(i).marked = Some(m);
    w.col.as_mut().unwrap().mark_enemy(m, 0);
}
pub fn unmark_self(w: &mut World, i: usize) {
    let Some(m) = w.en_ref(i).marked else { return };
    if let Some(c) = w.col.as_mut() {
        c.mark_enemy(m, 1);
    }
    w.en(i).marked = None;
}
pub fn sync_mark(w: &mut World, i: usize) {
    if w.field.is_some() && crate::fieldpatrol::holds_mark(w, i) {
        return;
    }
    // RandomStep (0x0804B1C0) writes the position only: the game's enemy grid keeps the mark of the kept position
    // while the bones rattle (the real route, Volcano 15-14: a Skeleton's pile next to a wall - a mark following the
    // jitter would free the wall's tile when it moves back)
    if w.field.is_some() && w.en_ref(i).jitter && w.en_ref(i).marked.is_some() {
        return;
    }
    let e = w.en_ref(i);
    if let Some(m) = e.marked {
        if attached(w, i) && m[0] == e.a.px() && m[1] == e.a.py() {
            return;
        }
    }
    unmark_self(w, i);
    mark_self(w, i);
}
/// footprint on the enemy grid; a detached enemy always fits
pub fn passable(w: &mut World, i: usize, x: i32, y: i32) -> bool {
    if w.col.is_none() || !attached(w, i) {
        return true;
    }
    let (fw, fh) = (w.en_ref(i).a.foot_w, w.en_ref(i).a.foot_h);
    let marked = w.en_ref(i).marked;
    let c = w.col.as_mut().unwrap();
    match marked {
        None => c.test(Layer::Enemy, x, y, x + fw, y + fh),
        Some(m) => {
            let saved = c.save_enemy(m);
            c.mark_enemy(m, 1);
            let ok = c.test(Layer::Enemy, x, y, x + fw, y + fh);
            c.load_enemy(m, &saved);
            ok
        }
    }
}
/// footprint inside the arena bounds and passable
pub fn fits(w: &mut World, i: usize, x: i32, y: i32) -> bool {
    // field: a detached enemy (obj+0x291: a Ghost) fits anywhere - the game's test is the enemy grid alone, which
    // it is off for good (Pilaf's Castle 10-27: a Ghost chases straight at a player standing in the doorway
    // outside the walk bounds); battles keep the arena bounds the TypeScript reference tests
    if w.field.is_some() && w.en_ref(i).detached {
        return true;
    }
    let b = w.arena.bounds;
    let a = &w.en_ref(i).a;
    x >= b.x0 && y >= b.y0 && x + a.foot_w <= b.x1 && y + a.foot_h <= b.y1 && passable(w, i, x, y)
}
pub fn clamp_inside(w: &mut World, i: usize) {
    let b = w.arena.bounds;
    let a = &mut w.en(i).a;
    let px = b.x0.max((b.x1 - a.foot_w).min(a.px()));
    let py = b.y0.max((b.y1 - a.foot_h).min(a.py()));
    if px != a.px() {
        a.x = px << 8;
    }
    if py != a.py() {
        a.y = py << 8;
    }
}
/// enemy step: unmark, the full move, else the x part (fx), else the y part (fy), re-mark; true = full move
pub fn try_move(w: &mut World, i: usize, vx: i32, vy: i32, fx: i32, fy: i32) -> bool {
    unmark_self(w, i);
    let mut ok = false;
    w.en(i).stepped = true;
    let (x, y) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    if fits(w, i, (x + vx) >> 8, (y + vy) >> 8) {
        let a = &mut w.en(i).a;
        a.x += vx;
        a.y += vy;
        ok = true;
    } else if fx != 0 && fits(w, i, (x + fx) >> 8, y >> 8) {
        w.en(i).a.x += fx;
    } else if fy != 0 && fits(w, i, x >> 8, (y + fy) >> 8) {
        w.en(i).a.y += fy;
    } else {
        w.en(i).stepped = false;
    }
    mark_self(w, i);
    ok
}
pub fn move_by(w: &mut World, i: usize, vx: i32, vy: i32) -> bool {
    try_move(w, i, vx, vy, vx, vy)
}
/// walkBy with already-rounded components; `nonzero` = the unrounded vector was not zero
pub fn walk_by(w: &mut World, i: usize, vx: i32, vy: i32, nonzero: bool) -> bool {
    if !nonzero {
        return true;
    }
    mark_moving(w, i);
    move_by(w, i, vx, vy)
}

// ---- verified steps ----------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct KnockState {
    pub vx: i32,
    pub vy: i32,
    pub n: i32,
    /// animation index + 1 (0 = none)
    pub anim: i32,
}
const KB_SIGN: [i32; 4] = [-8, 8, 8, -8];

/// KnockbackSlide enter 0x08018A08
pub fn knock_start(w: &mut World, i: usize, mut k: i32, kb: &mut KnockState) {
    kb.n = 16;
    if w.en_ref(i).crit_hit {
        k *= 4;
    }
    let d = w.en_ref(i).last_hit_dir ^ 1;
    let s = (((w.rng.rand(64) + 96) as i64 * k as i64) as u32 >> 7) as i32;
    let main = KB_SIGN[d as usize] * s;
    let perp = ((w.rng.rand(0x8000) - 0x4000) * s) >> 12;
    if d >> 1 != 0 {
        kb.vx = main;
        kb.vy = perp;
    } else {
        kb.vx = perp;
        kb.vy = main;
    }
    w.en(i).a.facing = d;
    kb.anim = 0;
    if !w.en_ref(i).a.blocking {
        set_anim(w, i, &[n(9)], true, Some(false));
        kb.anim = w.en_ref(i).a.anim.map(|a| a as i32 + 1).unwrap_or(0);
    }
}
/// update 0x08018A92; true = finished
pub fn knock_step(w: &mut World, i: usize, kb: &mut KnockState) -> bool {
    let d = Arc::clone(&w.data);
    if w.en_ref(i).a.blocking {
        if w.en_ref(i).a.anim.is_none() || w.en_ref(i).a.anim_raw(&d) != Some(8) {
            set_anim(w, i, &[n(7)], false, Some(true));
        }
        kb.n -= 1;
        if kb.n == 0 {
            return true;
        }
    } else {
        let a = &w.en_ref(i).a;
        if kb.anim == 0 || a.anim.map(|x| x as i32 + 1) != Some(kb.anim) || w.en_ref(i).anim_done(&d) {
            return true;
        }
    }
    unmark_self(w, i);
    {
        let a = &mut w.en(i).a;
        a.x += kb.vx;
        a.y += kb.vy;
    }
    let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
    if fits(w, i, px, py) {
        mark_self(w, i);
        return false;
    }
    let (fw, fh) = (w.en_ref(i).a.foot_w, w.en_ref(i).a.foot_h);
    let rect = Rect::new(px, py, px + fw, py + fh);
    {
        let a = &mut w.en(i).a;
        a.x -= kb.vx;
        a.y -= kb.vy;
    }
    mark_self(w, i);
    if !w.en_ref(i).crit_hit && !w.en_ref(i).splat_hit {
        return false;
    }
    let was = w.en_ref(i).a.hittable;
    w.en(i).a.hittable = false;
    let id = w.ents[i].id;
    let mut m = HitMsg { team: 1, rect, dir: w.en_ref(i).last_hit_dir, mult: 409, atk: atk(3), attacker: id, att_stats: w.en_ref(i).a.stats, att_is_player: false, source: id, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
    broadcast(w, &mut m, false);
    w.en(i).a.hittable = was;
    true
}

/// back-off 0x08018C3A (face toward) / step-away 0x0801CBA1: true when no try moved
pub fn step_from_player(w: &mut World, i: usize, speed: i32, face_toward: bool) -> bool {
    let Some(pi) = player(w, i) else { return true };
    let (px, py) = w.pl_ref(pi).a.fc();
    let (x, y) = w.en_ref(i).a.fc();
    let dx = px - x;
    let dy = py - y;
    let (adx, ady) = (dx.abs(), dy.abs());
    let m;
    {
        let e = w.en(i);
        if adx > ady {
            m = adx;
            e.a.facing = if face_toward { if dx < 0 { 2 } else { 3 } } else if dx > 0 { 2 } else { 3 };
        } else {
            m = ady;
            e.a.facing = if face_toward { if dy < 0 { 1 } else { 0 } } else if dy > 0 { 1 } else { 0 };
        }
    }
    if m == 0 {
        return false;
    }
    // the back-off runs under its own reversed-walk state (set by its enter): its step makes no SetAnim(1). The
    // step-away's step SetAnim(1)s unless walking, whatever plays (story playthrough: a fighter's hurt animation
    // gives way to the walk in the step's frame)
    if !face_toward {
        mark_moving(w, i);
        if !w.en_ref(i).walking() {
            set_anim(w, i, &[nm("walk")], true, Some(true));
        }
    } else if w.en_ref(i).ea.rule != w.data.anim.reverse {
        // the back-off's step with its reversed-walk state gone (a hurt state pushed over it and popped): the state
        // comes back with the step (story playthrough: Pikkon's back-off after a hit restarts the reversed walk, 4
        // cels from 3 down, instead of finishing the hurt animation)
        let rev = w.data.anim.reverse;
        set_anim_rule(w, i, &[AnimRef::N(1)], true, Some(true), Some(rev));
    }
    let vx = -((dx as i64 * speed as i64) / m as i64) as i32;
    let vy = -((dy as i64 * speed as i64) / m as i64) as i32;
    try_move(w, i, vx, vy, if dx >= 0 { -speed } else { speed }, if dy >= 0 { -speed } else { speed });
    !w.en_ref(i).stepped
}

/// CirclePlayer update 0x0802F23A (Cyborgs): face the player on the major axis; the minor component of d is negated,
/// then pos -= trunc(d' * speed / m): away on the major axis, toward on the minor one. true = nothing moved
pub fn circle_player(w: &mut World, i: usize, speed: i32) -> bool {
    let Some(pi) = player(w, i) else { return true };
    let (px, py) = w.pl_ref(pi).a.fc();
    let (x, y) = w.en_ref(i).a.fc();
    let (mut dx, mut dy) = (px - x, py - y);
    let (adx, ady) = (dx.abs(), dy.abs());
    let m;
    {
        let e = w.en(i);
        if adx > ady {
            m = adx;
            e.a.facing = if dx < 0 { 2 } else { 3 };
            dy = -dy;
        } else {
            m = ady;
            e.a.facing = if dy < 0 { 1 } else { 0 };
            dx = -dx;
        }
    }
    if m == 0 {
        return false;
    }
    mark_moving(w, i);
    let vx = -((dx as i64 * speed as i64) / m as i64) as i32;
    let vy = -((dy as i64 * speed as i64) / m as i64) as i32;
    try_move(w, i, vx, vy, if dx >= 0 { -speed } else { speed }, if dy >= 0 { -speed } else { speed });
    !w.en_ref(i).stepped
}

/// FlyToPlayer update 0x08034ED8 (vampires): faces the major axis (ties vertical); m < 8 -> 0; else SetAnim(1) unless
/// walking, pos + trunc(d * speed / m) if it fits (placed -> 0, blocked -> 1)
pub fn fly_to_player(w: &mut World, i: usize, speed: i32) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let (px, py) = w.pl_ref(pi).a.fc();
    let (x, y) = w.en_ref(i).a.fc();
    let (dx, dy) = (px - x, py - y);
    let (adx, ady) = (dx.abs(), dy.abs());
    let m;
    {
        let e = w.en(i);
        if adx > ady {
            m = adx;
            e.a.facing = if dx > 0 { 3 } else { 2 };
        } else {
            m = ady;
            e.a.facing = if dy > 0 { 0 } else { 1 };
        }
    }
    if m < 8 {
        return false;
    }
    if !w.en_ref(i).walking() {
        set_anim(w, i, &[n(1)], true, Some(true));
    }
    let nx = w.en_ref(i).a.x + ((dx as i64 * speed as i64) / m as i64) as i32;
    let ny = w.en_ref(i).a.y + ((dy as i64 * speed as i64) / m as i64) as i32;
    unmark_self(w, i);
    let ok = fits(w, i, nx >> 8, ny >> 8);
    if ok {
        let a = &mut w.en(i).a;
        a.x = nx;
        a.y = ny;
    }
    mark_self(w, i);
    !ok
}

/// WalkToPlayer update 0x0802CDE8 (`st` = the detour counter)
pub fn walk_to_player(w: &mut World, i: usize, speed: i32, st: &mut i32, anim_frame: i32) -> bool {
    if anim_frame == 1 || anim_frame == 4 {
        return false;
    }
    let Some(pi) = player(w, i) else { return false };
    let (px, py) = w.pl_ref(pi).a.fc();
    let (x, y) = w.en_ref(i).a.fc();
    let dx = px - x;
    let dy = py - y;
    let (adx, ady) = (dx.abs(), dy.abs());
    let m;
    {
        let e = w.en(i);
        if adx > ady {
            m = adx;
            e.a.facing = if dx > 0 { 3 } else { 2 };
        } else {
            m = ady;
            e.a.facing = if dy > 0 { 0 } else { 1 };
        }
    }
    if m < 8 {
        return false;
    }
    mark_moving(w, i);
    unmark_self(w, i);
    let qx = ((dx as i64 * speed as i64) / m as i64) as i32;
    let qy = ((dy as i64 * speed as i64) / m as i64) as i32;
    let mut moved = false;
    let (ex, ey) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    if *st == 0 {
        if fits(w, i, (ex + qx) >> 8, (ey + qy) >> 8) {
            let a = &mut w.en(i).a;
            a.x += qx;
            a.y += qy;
            moved = true;
        } else if qx.abs() > 0x400 && fits(w, i, (ex + qx) >> 8, ey >> 8) {
            w.en(i).a.x += qx;
            moved = true;
        } else if qy.abs() > 0x400 && fits(w, i, ex >> 8, (ey + qy) >> 8) {
            w.en(i).a.y += qy;
            moved = true;
        }
    }
    if !moved {
        if *st == 0 {
            *st = w.rng.rand(32) - 16;
        }
        let (mut nx, mut ny) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
        let s = if *st < 0 { -speed } else { speed };
        *st += if *st < 0 { 1 } else { -1 };
        if adx >= ady {
            ny += s;
        } else {
            nx += s;
        }
        if fits(w, i, nx >> 8, ny >> 8) {
            let a = &mut w.en(i).a;
            a.x = nx;
            a.y = ny;
        } else {
            *st = 0;
        }
    }
    mark_self(w, i);
    false
}

/// Wander update 0x0802B708
pub fn wander_step(w: &mut World, i: usize, speed: i32, turn_p: i64) {
    // field: the update starts with SetAnim(1) unless IsAnim(1) (0x0802B710..): whatever is showing is replaced - a
    // hurt animation still running when the reaction state has popped (Northern Wilderness 15-8: a Ghoul walks on
    // without the hurt's end going through idle). Battles keep the reference (the walk follows the move)
    if w.field.is_some() && !w.en_ref(i).is_anim(1) {
        set_anim(w, i, &[AnimRef::N(1)], true, Some(true));
    }
    mark_moving(w, i);
    if w.rng.chance(turn_p) {
        let d = w.rng.rand(4) as Dir;
        w.en(i).a.facing = d;
    }
    let (dx, dy) = DIR_VEC[w.en_ref(i).a.facing as usize];
    let vx = (dx * 256 * speed) >> 8;
    let vy = (dy * 256 * speed) >> 8;
    unmark_self(w, i);
    let (x, y) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    if fits(w, i, (x + vx) >> 8, (y + vy) >> 8) {
        let a = &mut w.en(i).a;
        a.x += vx;
        a.y += vy;
    } else {
        let d = w.rng.rand(4) as Dir;
        w.en(i).a.facing = d;
    }
    mark_self(w, i);
}

/// normalised move toward the player (exact integer form of round(d * speed / hypot)); true within `stop` px
pub fn move_toward(w: &mut World, i: usize, speed: i32, stop: i32, face: bool, ignore_walls: bool) -> bool {
    if player(w, i).is_none() {
        return false;
    }
    let (dx, dy) = to_player(w, i);
    let s = dx as i64 * dx as i64 + dy as i64 * dy as i64;
    if face {
        w.en(i).a.facing = facing_toward(dx, dy);
    }
    if s <= stop as i64 * stop as i64 {
        return true;
    }
    let vx = round_div_sqrt(dx as i64 * speed as i64, s) as i32;
    let vy = round_div_sqrt(dy as i64 * speed as i64, s) as i32;
    if ignore_walls {
        mark_moving(w, i);
        let a = &mut w.en(i).a;
        a.x += vx;
        a.y += vy;
        clamp_inside(w, i);
    } else {
        walk_by(w, i, vx, vy, true);
    }
    false
}
pub fn move_to_point(w: &mut World, i: usize, x: i32, y: i32, speed: i32) -> bool {
    let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
    let dx = x - px;
    let dy = y - py;
    if dx.abs() <= 8 && dy.abs() <= 8 {
        return true;
    }
    let s = dx as i64 * dx as i64 + dy as i64 * dy as i64;
    w.en(i).a.facing = facing_toward(dx, dy);
    let (ox, oy) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    let vx = round_div_sqrt(dx as i64 * speed as i64, s) as i32;
    let vy = round_div_sqrt(dy as i64 * speed as i64, s) as i32;
    if !walk_by(w, i, vx, vy, true) {
        let b = w.arena.bounds;
        let (fw, fh) = (w.en_ref(i).a.foot_w, w.en_ref(i).a.foot_h);
        if x < b.x0 || y < b.y0 || x + fw > b.x1 || y + fh > b.y1 {
            return true;
        }
        if w.en_ref(i).a.x == ox && w.en_ref(i).a.y == oy {
            return true;
        }
    }
    false
}
/// 0x0800FE34: the shown frame record's box at the integer position; the body box when the record is unknown
pub fn frame_box(w: &World, i: usize) -> Rect {
    let e = w.en_ref(i);
    let d = &w.data;
    if let (Some(sp), Some(raw)) = (e.a.sprite.and_then(|s| d.sprite(s)), e.a.anim_raw(d)) {
        if let Some(b) = sp.frame_box(raw, e.a.facing, e.anim_frame()) {
            let (px, py) = (e.a.px(), e.a.py());
            return Rect::new(px + b[0], py + b[1], px + b[0] + b[2], py + b[1] + b[3]);
        }
    }
    e.a.body_box()
}
/// footstep sound 52 by sprite (def = gfx): gfx 0x4E / 0x11 / 0x4C on walk ticks 0 / 16 (the path follower 0x0803BB48
/// after its step, AlignWithPlayer 0x0801CEEC before its move); gfx 0xA4 / 0x44 / 0xBF on ticks 8 / 24 (path follower)
pub fn footstep(w: &mut World, i: usize, path: bool) {
    let d = Arc::clone(&w.data);
    let mut g = d.enemy(w.en_ref(i).ty).and_then(|t| t.gfx).unwrap_or(-1);
    let e = w.en_ref(i);
    // field: the object's own graphics (a placement's sprite: Pilaf's Castle 10-14 has a Destroyer drawn with 0x4F,
    // which makes no step sound, beside one with the type's 0x4E)
    if w.field.is_some() {
        if let Some(s) = e.a.sprite {
            g = s + crate::content::GFX_OFFSET;
        }
    }
    // the game's step SetAnim(1)s before this test: a first walking step counts as walk tick 0
    let t = if e.moving && e.a.anim_name(&d) != Some("walk") { 0 } else { e.ea.pos.tick };
    if (g == 0x4e || g == 0x11 || g == 0x4c) && (t == 0 || t == 16) {
        w.sfx(52, 64);
    } else if path && (g == 0xa4 || g == 0x44 || g == 0xbf) && (t == 8 || t == 24) {
        w.sfx(52, 64);
    }
}
/// MechTransform 0x080314DC: swap to the paired graphics set (0xBB / 0xBC, 0xE6 / 0xE7, 0x85 / 0x86, 0xCA / 0xCB)
/// via 0x0800EDF8
pub fn swap_gfx(w: &mut World, i: usize) {
    let d = Arc::clone(&w.data);
    let g = if w.en_ref(i).gfx_now >= 0 { Some(w.en_ref(i).gfx_now) } else { d.enemy(w.en_ref(i).ty).and_then(|t| t.gfx) };
    let to = match g {
        Some(0xbb) => 0xbc,
        Some(0xbc) => 0xbb,
        Some(0xe6) => 0xe7,
        Some(0xe7) => 0xe6,
        Some(0x85) => 0x86,
        Some(0x86) => 0x85,
        // the Nosferatu's bat form (field: nodes VampireSummon / VampireSummon2)
        Some(0xca) => 0xcb,
        Some(0xcb) => 0xca,
        _ => return,
    };
    let idx = to - crate::content::GFX_OFFSET;
    if d.sprite(idx).is_none() {
        return;
    }
    let e = w.en(i);
    e.gfx_now = to;
    let old = e.a.sprite;
    e.a.set_sprite(&d, Some(idx));
    crate::vram::sprite_changed(w, i, old);
}
/// message 0xF (0x0800F44A, the death script's freeze): the walk state's vfunc +0x10 SetAnim(0)s (0x0800E050); other
/// states: INFERRED nothing
pub fn stop_msg(w: &mut World, i: usize) {
    let d = Arc::clone(&w.data);
    if w.en_ref(i).a.anim_name(&d) == Some("walk") {
        set_anim(w, i, &[n(0)], true, Some(true));
    }
}
/// the end of a one-shot animation controller (0x0800EE78): the walk state
pub fn set_walk_state(w: &mut World, i: usize) {
    set_anim(w, i, &[n(1)], true, Some(true));
}
/// a node that ends a movement: SetAnim(0) if walking (IsAnim(1), 0x0801CE58)
pub fn stop_walk(w: &mut World, i: usize) {
    if w.en_ref(i).walking() {
        set_anim(w, i, &[nm("idle")], true, Some(true));
    }
}
/// AlignWithPlayer (0x0801CF6E): a step that fits SetAnims(1) unless IsAnim(1); a blocked step ends the node with
/// SetAnim(0) if walking
pub fn align(w: &mut World, i: usize, speed: i32) -> bool {
    if player(w, i).is_none() {
        return false;
    }
    let (dx, dy) = to_player(w, i);
    let blocked = |w: &mut World| {
        stop_walk(w, i);
        // (no SetAnim(1) for a step that did not fit: real route 8-45, a Destroyer's last align step is blocked, then
        // StandStill's idle - the walk must not come back after it)
        let e = w.en(i);
        e.moving = false;
        e.walk_due = false;
        true
    };
    let walk = |w: &mut World| {
        if !w.en_ref(i).is_anim(1) {
            set_anim(w, i, &[nm("walk")], true, Some(true));
        }
    };
    if if dx.abs() > dy.abs() { dy.abs() >= 3 } else { dx.abs() >= 3 } {
        footstep(w, i, false);
    }
    if dx.abs() > dy.abs() {
        if dy.abs() < 3 {
            stop_walk(w, i);
            w.en(i).a.facing = if dx > 0 { 3 } else { 2 };
            return true;
        }
        w.en(i).a.facing = if dy > 0 { 0 } else { 1 };
        // a blocked try-move (0x0801CFA8): SetAnim(0) if walking, done; the facing stays the step's (VERIFIED Babidi's
        // ship 8-31 / 8-42 segments)
        if !walk_by(w, i, 0, dy.signum() * speed, dy != 0 && speed != 0) {
            return blocked(w);
        }
        walk(w);
    } else {
        if dx.abs() < 3 {
            stop_walk(w, i);
            w.en(i).a.facing = if dy > 0 { 0 } else { 1 };
            return true;
        }
        w.en(i).a.facing = if dx > 0 { 3 } else { 2 };
        if !walk_by(w, i, dx.signum() * speed, 0, dx != 0 && speed != 0) {
            return blocked(w);
        }
        walk(w);
    }
    false
}

// ---- attacks -------------------------------------------------------------------------------------------------------

fn msg_from(w: &World, i: usize, team: u8, rect: Rect, dir: Dir, mult: i32, a: AttackType) -> HitMsg {
    let id = w.ents[i].id;
    HitMsg { team, rect, dir, mult, atk: a, attacker: id, att_stats: w.actor(i).map(|x| x.stats).unwrap_or_default(), att_is_player: false, source: id, crit: false, raw: 0, dmg: 0, no_react: false, foot: false }
}

/// 0x0802A0C4: face the player on the dominant axis, the half-box on that side
pub fn strike_rect(w: &mut World, i: usize, face: bool) -> Option<Rect> {
    let pi = player(w, i)?;
    let a = &w.en_ref(i).a;
    let bb = a.body_box();
    let ff = a.foot_box();
    let (cx, cy) = a.center();
    let mut f = a.facing;
    if face {
        // 0x0802A0EE..0x0802A19A: the BODY box centres (of the facing before the turn), not the footprints
        let (px, py) = w.pl_ref(pi).a.center();
        let (dx, dy) = (px - cx, py - cy);
        f = if dx.abs() > dy.abs() {
            if dx > 0 {
                3
            } else {
                2
            }
        } else if dy > 0 {
            0
        } else {
            1
        };
        w.en(i).a.facing = f;
    }
    // vertical (0x0802A176): x from the footprint, y from the body box: down = body y0 .. body y1 - 6, up = body
    // y0 .. footprint y0
    Some(match f {
        3 => Rect::new(cx, cy, bb.x1 + 3, bb.y1 + 3),
        2 => Rect::new(bb.x0 - 4, cy, cx, bb.y1 + 3),
        0 => Rect::new(ff.x0, bb.y0, ff.x1, bb.y1 - 6),
        _ => Rect::new(ff.x0, bb.y0, ff.x1, ff.y0),
    })
}
pub fn melee_strike(w: &mut World, i: usize, mult: i32, face: bool, adjacent: bool, a: AttackType) -> bool {
    let Some(pi) = player(w, i) else { return false };
    if !w.pl_ref(pi).a.hittable {
        return false;
    }
    let mut rect = strike_rect(w, i, face);
    if adjacent {
        let e = &w.en_ref(i).a;
        let b = e.body_box();
        let h = 16;
        rect = Some(match e.facing {
            3 => Rect::new(b.x1, b.y0, b.x1 + h, b.y1),
            2 => Rect::new(b.x0 - h, b.y0, b.x0, b.y1),
            0 => Rect::new(b.x0, b.y1 - 8, b.x1, b.y1 + h),
            _ => Rect::new(b.x0, b.y0 - h, b.x1, b.y0 + 8),
        });
    }
    let Some(rect) = rect else { return false };
    let mut m = msg_from(w, i, 0, rect, w.en_ref(i).a.facing, mult, a);
    player::on_message(w, pi, &mut m)
}
/// The hit nodes' condition 0x080361B8 (IfContactHit 0x0803634F / IfMeleeHit 0x08036303, field): the strike rect
/// toward the player on the body-box centres' dominant axis (as `strike_rect` with the turn), the message carries
/// that direction; `restore` (node +0x10: IfContactHit, a tank's crush) puts the enemy's own facing back before the
/// message is sent; accepted -> sound 48 + rand(2)
pub fn contact_strike(w: &mut World, i: usize, mult: i32, restore: bool) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let saved = w.en_ref(i).a.facing;
    let Some(rect) = strike_rect(w, i, true) else { return false };
    let dir = w.en_ref(i).a.facing;
    if restore {
        w.en(i).a.facing = saved;
    }
    let mut m = msg_from(w, i, 0, rect, dir, mult, atk(2));
    if player::on_message(w, pi, &mut m) {
        let s = 48 + w.rng.rand(2);
        w.sfx(s, 64);
        return true;
    }
    false
}
/// HeavyStrike 0x0802F77C: sound 62 first (0x0802F7B2), the hit; accepted -> 48 + rand(2) (0x0802F8AC)
pub fn heavy_strike(w: &mut World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    if !w.pl_ref(pi).a.hittable {
        return false;
    }
    w.sfx(62, 64);
    // field: 0x0802F7B6.. is the standard strike - the turn and the rect from the body-box centres' dominant axis
    // (`strike_rect`; Northern Wilderness 15-12: a Knight Destroyer, 64 px wide, turns left toward a player its
    // top-left position has above it). Battles keep the reference's rect
    if w.field.is_some() {
        let Some(rect) = strike_rect(w, i, true) else { return true };
        let dir = w.en_ref(i).a.facing;
        let mut m = msg_from(w, i, 0, rect, dir, 0x400, atk(2));
        if player::on_message(w, pi, &mut m) {
            let s = 48 + w.rng.rand(2);
            w.sfx(s, 64);
        }
        return true;
    }
    face_player(w, i);
    let a = &w.en_ref(i).a;
    let b = a.body_box();
    let (cx, cy) = a.center();
    const R: i32 = 57;
    let rect = match a.facing {
        3 => Rect::new(cx, b.y0, cx + R, b.y1),
        2 => Rect::new(cx - R, b.y0, cx, b.y1),
        0 => Rect::new(b.x0, cy, b.x1, cy + R),
        _ => Rect::new(b.x0, cy - R, b.x1, cy),
    };
    let mut m = msg_from(w, i, 0, rect, a.facing, 0x400, atk(2));
    if player::on_message(w, pi, &mut m) {
        let s = 48 + w.rng.rand(2);
        w.sfx(s, 64);
    }
    true
}
pub fn contact_hit(w: &mut World, i: usize) -> bool {
    contact_hit_kind(w, i, false)
}
/// the worm's contact (0x0802D5F2 / 0x0802D756): message kind 1, the body rect against the player's footprint
pub fn contact_hit_foot(w: &mut World, i: usize) -> bool {
    contact_hit_kind(w, i, true)
}
fn contact_hit_kind(w: &mut World, i: usize, foot: bool) -> bool {
    let Some(pi) = player(w, i) else { return false };
    if !w.pl_ref(pi).a.hittable || w.en_ref(i).contact_cd > 0 {
        return false;
    }
    let bb = w.en_ref(i).a.body_box();
    if !overlaps(&bb, &if foot { w.pl_ref(pi).a.foot_box() } else { w.pl_ref(pi).a.body_box() }) {
        return false;
    }
    let mut m = msg_from(w, i, 0, bb, w.en_ref(i).a.facing, 0x400, atk(2));
    m.foot = foot;
    let ok = player::on_message(w, pi, &mut m);
    if ok {
        // the worm's counter is decremented instead of the contact test (20 frames without a test, segment sb 21-14);
        // the update's own decrement runs first
        w.en(i).contact_cd = if foot { 21 } else { 20 };
    }
    ok
}
pub fn contact_explode(w: &mut World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let bb = w.en_ref(i).a.body_box();
    if !overlaps(&bb, &w.pl_ref(pi).a.body_box()) {
        return false;
    }
    let mut m = msg_from(w, i, 0, bb, w.en_ref(i).a.facing, 0x1000, atk(1));
    player::on_message(w, pi, &mut m)
}
pub fn area_hit(w: &mut World, i: usize, rect: Rect, a: AttackType, mult: i32) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let mut m = msg_from(w, i, 0, rect, w.en_ref(i).a.facing, mult, a);
    player::on_message(w, pi, &mut m)
}
/// launched volleyball: hits the other enemies (attacker: the player)
pub fn area_hit_enemies(w: &mut World, i: usize, rect: Rect, pi: usize) {
    let pid = w.ents[pi].id;
    let mut m = HitMsg { team: 1, rect, dir: w.en_ref(i).last_hit_dir, mult: 409, atk: atk(3), attacker: pid, att_stats: w.pl_ref(pi).a.stats, att_is_player: true, source: w.ents[i].id, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
    broadcast(w, &mut m, false);
}

const SHELL_MUZZLE: [(i32, i32); 4] = [(30, 60), (34, -12), (-16, 24), (60, 24)];
const SHELL_BOX: [[i32; 4]; 4] = [[8, 14, 16, 16], [8, 2, 16, 16], [2, 8, 16, 16], [14, 8, 16, 16]];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotType {
    Gun,
    Rifle,
    Star,
    Wrap,
    Burst,
    Shell,
    Spit,
}
const GUN_MUZZLE: [(i32, i32); 4] = [(4, 4), (13, -24), (-8, -8), (24, -8)];
const GUN_BOX: [[i32; 4]; 4] = [[3, 10, 8, 16], [3, 5, 8, 16], [5, 4, 16, 8], [10, 4, 16, 8]];
const RIFLE_BOX: [[i32; 4]; 4] = [[8, 14, 16, 16], [8, 2, 16, 16], [2, 8, 16, 16], [14, 8, 16, 16]];
/// MummyWrapBurst spawn offsets by facing: straight (x, y), diagonal (x, y) (0x08756AA4)
const WRAP_BURST_OFF: [[i32; 4]; 4] = [[0, 36, -12, 12], [0, -36, 12, -12], [-36, 0, -12, -12], [36, 0, 12, 12]];

pub fn shot(w: &mut World, i: usize, kind: ShotType) {
    let a = w.en_ref(i).a.clone();
    let f = a.facing;
    let (mx, my) = GUN_MUZZLE[f as usize];
    let at = ((a.px() + mx) << 8, (a.py() + my) << 8);
    let (dx, dy) = DIR_VEC[f as usize];
    let owner = w.ents[i].id;
    // the firing node's own sound (gun 0x080264EA / rifle 0x0802676A / tank 0x0801C900: 28; star 61) on the spawn frame
    let mk = |w: &mut World, dir: Dir, x: i32, y: i32, vx: i32, vy: i32, b: [i32; 4], o: StraightOpts| {
        let s = Shot::new(owner, dir, 0, x, y, ShotKind::Straight(Box::new(StraightOpts { vx, vy, b, ..o })));
        let id = w.add(Body::Shot(Box::new(s)));
        if let Some(k) = w.find(id) {
            shots::spawn_rng(w, k);
        }
    };
    let base = StraightOpts { a: atk(1), mult: 1024, ..Default::default() };
    match kind {
        ShotType::Gun => mk(w, f, at.0, at.1, dx * 672, dy * 672, GUN_BOX[f as usize], StraightOpts { sfx_spawn: 28, fragile: true, name: shots::SName::Gun, ..base }),
        // rifle bullet 0x080276A8: an accepted hit plays 48 + rand(2), the destroy 0x0802770C sound 46
        ShotType::Rifle => mk(w, f, at.0, at.1, dx * 448, dy * 448, RIFLE_BOX[f as usize], StraightOpts { sfx_spawn: 28, rng_hit: 2, sfx_hit: 48, sfx_end: 46, fragile: true, name: shots::SName::Rifle, trail: Some(shots::Trail { fx: shots::FxId::Fx0874e614, rec: 4, n: 3, rate: 0x2aaa }), ..base }),
        // tank shell 0x0801C900 from the tank controller 0x0802E278: the position + 0x08757354 [facing], box = the
        // record's (0x0871E630 + 12 * dir); an accepted hit plays 48 + rand(2) (3-14: the shell that hits Gohan)
        ShotType::Shell => mk(w, f, (a.px() + SHELL_MUZZLE[f as usize].0) << 8, (a.py() + SHELL_MUZZLE[f as usize].1) << 8, dx * 448, dy * 448, SHELL_BOX[f as usize], StraightOpts { sfx_spawn: 28, rng_hit: 2, sfx_hit: 48, fragile: true, name: shots::SName::Shell, trail: Some(shots::Trail { fx: shots::FxId::Fx0871e630, rec: 4, n: 3, rate: 0x2aaa }), ..base }),
        // throw controller 0x0802C08A: the star at the position + the muzzle table 0x087548F0, then PlaySfx(61)
        ShotType::Star => mk(w, f, at.0, at.1, dx * 448, dy * 448, [4, 4, 8, 8], StraightOpts { sfx_spawn: 61, fragile: true, explode: false, name: shots::SName::Star, ..base }),
        // MummyWrapShot 0x0802D0DC: one straight bandage at the position + 0x08756A74 [facing]; MummyWrapBurst
        // 0x0802D214: per facing one straight and one diagonal at the position + 0x08756AA4 [facing]
        ShotType::Wrap => {
            shots::add(w, shots::mummy_wrap(owner, f, at.0, at.1, false));
        }
        ShotType::Burst => {
            for d in 0..4u8 {
                let o = WRAP_BURST_OFF[d as usize];
                shots::add(w, shots::mummy_wrap(owner, d, (a.px() + o[0]) << 8, (a.py() + o[1]) << 8, false));
                shots::add(w, shots::mummy_wrap(owner, d, (a.px() + o[2]) << 8, (a.py() + o[3]) << 8, true));
            }
        }
        ShotType::Spit => {
            // Dabura's spit 0x0802E444: origin = the position + (0, -32); target = the player's footprint centre;
            // velocity d * 512 / max(|dx|, |dy|) (truncating); box (8, 8, 16, 16); accepted -> the stone state
            let Some(pi) = player(w, i) else { return };
            let (ox, oy) = (a.px(), a.py() - 32);
            let (tx, ty) = w.pl_ref(pi).a.foot_center();
            let ddx = tx - ox;
            let ddy = ty - oy;
            let m = ddx.abs().max(ddy.abs()).max(1);
            let sector = if 2 * ddx < -5 * ddy {
                0
            } else if 12 * ddx < -5 * ddy {
                2
            } else if 2 * ddx > 5 * ddy {
                8
            } else if 12 * ddx > 5 * ddy {
                6
            } else {
                4
            };
            let vx = (ddx * 512) / m;
            let vy = (ddy * 512) / m;
            mk(w, f, ox << 8, oy << 8, vx, vy, [8, 8, 16, 16], StraightOpts { explode: false, on_hit: OnHit::Stone, name: shots::SName::Spit, sector, ..base });
        }
    }
}

// ---- teleports / special ---------------------------------------------------------------------------------------

pub fn ninja_teleport(w: &mut World, i: usize) {
    for _ in 0..16 {
        let dx = w.rng.rand(201) - 100;
        let dy = w.rng.rand(201) - 100;
        if dx.abs().max(dy.abs()) < 70 {
            continue;
        }
        let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
        if !fits(w, i, px + dx, py + dy) {
            continue;
        }
        let (cx, cy) = w.en_ref(i).a.center();
        crate::fx::puff_rng(w, cx, cy);
        let a = &mut w.en(i).a;
        a.x += dx << 8;
        a.y += dy << 8;
        let (cx, cy) = w.en_ref(i).a.center();
        crate::fx::puff_rng(w, cx, cy);
        return;
    }
}
pub fn teleport_beside(w: &mut World, i: usize, side: i32) {
    let Some(pi) = player(w, i) else { return };
    const OFF: [(i32, i32); 4] = [(0, 32), (0, -32), (-32, 0), (32, 0)];
    let (ox, oy) = OFF[(side & 3) as usize];
    let (cx, cy) = w.en_ref(i).a.center();
    crate::fx::puff(w, cx, cy, 0x1800);
    let (pfx, pfy) = w.pl_ref(pi).a.foot_center();
    // field (sc_janemba): the player's whole-pixel position + the side offset; battle keeps the reference's
    // footprint-centred placement
    let at = w.field.is_some().then(|| (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py()));
    {
        let a = &mut w.en(i).a;
        match at {
            Some((px, py)) => {
                a.x = (px + ox) << 8;
                a.y = (py + oy) << 8;
            }
            None => {
                a.x = (pfx + ox - (a.foot_w >> 1)) << 8;
                a.y = (pfy + oy - (a.foot_h >> 1)) << 8;
            }
        }
    }
    if at.is_some() {
        free_spot(w, i);
    } else {
        clamp_inside(w, i);
    }
    // field: the landing keeps the facing (the sequence's FacePlayer turns it). The grid is not touched: the old
    // footprint stays marked for good and the enemy takes the new spot as its own (its next move unmarks tiles it
    // never marked; sc_janemba: Super Janemba's teleports leave holes)
    if at.is_none() {
        face_player(w, i);
    } else {
        let a = &w.en_ref(i).a;
        let m = [a.px(), a.py(), a.px() + a.foot_w, a.py() + a.foot_h];
        w.en(i).marked = Some(m);
    }
    set_anim(w, i, &[n(19)], true, Some(false));
    let (cx, cy) = w.en_ref(i).a.center();
    crate::fx::puff(w, cx, cy, 0x1800);
}
/// 0x08009816: a rect that does not fit the walk layer moves to the first free tile-aligned spot of a square spiral
/// around its tile (sc_janemba: Super Janemba's teleport against the arena's diagonal wall; the scripts' 0xCB / 0xCC)
pub fn free_rect(w: &World, x: i32, y: i32, rw: i32, rh: i32) -> (i32, i32) {
    let Some(c) = w.col.as_ref() else { return (x, y) };
    if c.test(Layer::Walk, x, y, x + rw, y + rh) {
        return (x, y);
    }
    let (bx, by) = (x & !7, y & !7);
    for k in 0..4096i64 {
        let s = crate::fixed::isqrt(k);
        let (dx, dy) = if s & 1 == 1 {
            if s * s + s >= k {
                ((s + 1) >> 1, k - s * s - ((s + 1) >> 1) + 1)
            } else {
                (((s + 1) >> 1) - k + s * s + s, (s + 1) >> 1)
            }
        } else if s * s + s >= k {
            ((-s) >> 1, ((-s) >> 1) - k + s * s + s)
        } else {
            (k - s * s - s + ((-s) >> 1), (-s) >> 1)
        };
        let (nx, ny) = (bx + dx as i32 * 8, by + dy as i32 * 8);
        if c.test(Layer::Walk, nx, ny, nx + rw, ny + rh) {
            return (nx, ny);
        }
    }
    (x, y)
}
fn free_spot(w: &mut World, i: usize) {
    let (x, y, fw, fh) = {
        let a = &w.en_ref(i).a;
        (a.px(), a.py(), a.foot_w, a.foot_h)
    };
    let (nx, ny) = free_rect(w, x, y, fw, fh);
    if (nx, ny) != (x, y) {
        let a = &mut w.en(i).a;
        a.x = nx << 8;
        a.y = ny << 8;
    }
}
pub fn kid_buu_split(w: &mut World, i: usize) {
    for _ in 0..16 {
        // 0x08032BB8: d in 1/256 px (the 24.8 position +- d); 0x08009904: both footprints on walkable map tiles
        let dx = w.rng.rand(192) - 96;
        let dy = w.rng.rand(192) - 96;
        let (x, y) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
        let (fw, fh) = (w.en_ref(i).a.foot_w, w.en_ref(i).a.foot_h);
        let ok = |w: &World, px: i32, py: i32| w.col.as_ref().is_none_or(|c| c.test(Layer::Walk, px, py, px + fw, py + fh));
        if !ok(w, (x + dx) >> 8, (y + dy) >> 8) || !ok(w, (x - dx) >> 8, (y - dy) >> 8) {
            continue;
        }
        // the puff 0x086FD9D0 (rate 0x1000 + rand(0x1000)) at the sprite box centre
        let (cx, cy) = w.en_ref(i).a.center();
        crate::fx::puff_rng(w, cx, cy);
        unmark_self(w, i);
        let hp;
        {
            let e = w.en(i);
            e.a.x = x + dx;
            e.a.y = y + dy;
            e.hp1024 = (e.hp1024 + 1024) >> 1;
            hp = e.hp1024;
        }
        mark_self(w, i);
        if let Some(ci) = crate::battle::spawn_enemy(w, 0x3d, (x - dx) >> 8, (y - dy) >> 8, 0, Some(0x12a)) {
            // (the copy is created at the PIXEL of the 24.8 position - d: it has no sub-pixel part; the real route,
            // Kid Buu's arena 21-25: original (279.99, 410.37) + (42, 68) / 256, copy at (279, 410) exactly)
            unmark_self(w, ci);
            {
                let c = w.en(ci);
                c.a.x = ((x - dx) >> 8) << 8;
                c.a.y = ((y - dy) >> 8) << 8;
            }
            mark_self(w, ci);
            w.en(ci).hp1024 = hp;
        }
        return;
    }
}
pub fn kid_buu_merge(w: &mut World, i: usize) {
    let me = w.ents[i].id;
    let other = w.live_enemies().into_iter().find(|&o| w.ents[o].id != me && matches!(w.en_ref(o).ty, 0x3c | 0x3d));
    let Some(o) = other else { return };
    let ohp = w.en_ref(o).hp1024;
    w.en(i).hp1024 += ohp;
    // the midpoint of the two 24.8 positions (the real route, Kid Buu's arena 21-25: (261.63, 311.74) and (252.70,
    // 390.58) merge at (257.17, 351.16))
    let mx = (w.en_ref(i).a.x + w.en_ref(o).a.x) >> 1;
    let my = (w.en_ref(i).a.y + w.en_ref(o).a.y) >> 1;
    // 0x08009904 on the enemy grid as it stands - both marks still on it: a midpoint on either footprint's tiles
    // fails and the survivor stays where it is (21-25: two halves 0.1 px apart merge in place)
    let (fw, fh) = (w.en_ref(i).a.foot_w, w.en_ref(i).a.foot_h);
    if w.col.as_ref().is_none_or(|c| c.test(Layer::Enemy, mx >> 8, my >> 8, (mx >> 8) + fw, (my >> 8) + fh)) {
        let a = &mut w.en(i).a;
        a.x = mx;
        a.y = my;
    }
    // two puffs 0x086FD9D0, each rate 0x1000 + rand(0x1000) (draws at 0x08032EAC / 0x08032F0A)
    let (cx, cy) = w.en_ref(i).a.center();
    crate::fx::puff_rng(w, cx, cy);
    let (ox, oy) = w.en_ref(o).a.center();
    crate::fx::puff_rng(w, ox, oy);
    // (the absorbed copy leaves the enemy grid: its mark would stand in the path follower's way)
    unmark_self(w, o);
    w.ents[o].removed = true;
    let oe = w.en(o);
    oe.defeated = true;
    oe.primary = false;
}
/// field Janemba's body parts (sc_janemba segment, emulator: 0x0803D0C4 / 0x0803CE94): part 0x37's AI node
/// (PartMirrorAnim) places the chain every frame its AI runs (not while an event holds the field: they keep their
/// place) at the body + (16, -40) / (-12, -21) / (44, -21), plus a y delta by the body's animation cel read before
/// the body's own tick: hurt (raw 10) [-1, -2, -1], raw 20 [15, -3, -5], else [0, 1, 0, -1] px. The parts are
/// created at (-16, -16) / (48, -16) / (16, -40) (the spawn's own frame). Battle keeps the measured fixed offsets
/// (the TypeScript cross-check reference; engine/docs/segments.md §7).
/// a Janemba part's hurt animation (JanembaPartFlash) ends with SetAnim(0) (the idle's rand(255)) - also while an
/// event holds the field (sc_janemba: the defeat scene's first frames)
fn part_hurt_end(w: &mut World, i: usize) {
    if w.en_ref(i).ea.pos.raw == 10 && w.en_ref(i).ea.done {
        set_anim(w, i, &[n(0)], true, Some(true));
    }
}
fn janemba_part_field(w: &mut World, i: usize) {
    let par = w.en_ref(i).parent;
    let Some((pi, (raw, cel, x, y, z, f))) = w
        .enemy_by_id(par)
        .filter(|(pi, p)| !w.ents[*pi].removed && !p.defeated)
        .map(|(pi, p)| (pi, (p.ea.pos.raw, p.ea.pos.frame, p.a.x, p.a.y, p.a.z, p.a.facing)))
    else {
        w.ents[i].removed = true;
        return;
    };
    {
        let e = w.en(i);
        e.a.z = z;
        e.a.facing = f;
    }
    part_hurt_end(w, i);
    // the hand strike's controller on the arm part (set by JanembaHandStrike's first update): the hand on its 16th
    // tick, before the player's update (sc_janemba: a punch that hurts Janemba in that frame still meets the hand);
    // on its 56th the arm's SetAnim(0)
    let (st, a_side) = w.en_ref(i).strike;
    if st > 0 {
        let st = st + 1;
        w.en(i).strike.0 = if st >= 57 { 0 } else { st };
        if st == 17 {
            let h = hand_strike(w, pi, a_side);
            w.en(i).strike_hand = h.unwrap_or(0);
        }
        if st >= 57 {
            // the controller's end: its hand goes idle (SetAnim(0)) and frees the tiles its constructor marked
            // (whatever else had marked them: holes); the arm part is older than the minis, so a mini hopping into
            // them in this frame is still blocked (sc_janemba)
            let h = std::mem::take(&mut w.en(i).strike_hand);
            if let Some(hk) = (h != 0).then(|| w.find(h)).flatten() {
                w.rng.rand(255);
                unmark_hand(w, hk);
            }
            set_anim(w, i, &[n(0)], true, Some(true));
        }
    }
    if w.en_ref(i).ty != 0x37 {
        return;
    }
    let seen = std::mem::replace(&mut w.en(i).mirror_raw, raw);
    // the cel seen on the previous update (under the ascent's controller animation the frame byte lags a frame more)
    let last_cel = std::mem::replace(&mut w.en(i).mirror_cel, cel as i32);
    // a switch into the ascent's controller animation (raw 20) shows in the table a frame late
    let cur_raw = raw;
    let raw = if raw == 20 && seen != 0 { seen } else { raw };
    let cel = if (raw == 20 && seen == raw) || (cur_raw == 20 && seen != 20 && seen != 0) { last_cel as usize } else { cel as usize };
    let table: &[i32] = match raw {
        10 => &[-1, -2, -1],
        20 => &[15, -3, -5],
        _ => &[0, 1, 0, -1],
    };
    let dy = table.get(cel).copied().unwrap_or(0);
    let parts: Vec<(usize, i32)> = (0..w.ents.len())
        .filter(|&k| !w.ents[k].removed)
        .filter_map(|k| match &w.ents[k].body {
            Body::Enemy(e) if e.part && e.parent == par => Some((k, e.ty)),
            _ => None,
        })
        .collect();
    for (k, t) in parts {
        let (ox, oy) = match t {
            0x37 => (16, -40),
            0x34 => (-12, -21),
            0x35 => (44, -21),
            _ => continue,
        };
        let e = w.en(k);
        e.a.x = x + (ox << 8);
        e.a.y = y + ((oy + dy) << 8);
    }
}
/// Janemba's arm part for a hand strike: A = 0x34 (screen left), B = 0x35
/// JanembaPartFlash (on hit): every body part plays the hurt animation (raw 10)
pub fn janemba_parts_hurt(w: &mut World, i: usize) {
    let me = w.ents[i].id;
    let parts: Vec<usize> = (0..w.ents.len()).filter(|&k| !w.ents[k].removed && matches!(&w.ents[k].body, Body::Enemy(e) if e.part && e.parent == me)).collect();
    for k in parts {
        // field: an arm part in a hand strike - the strike controller's exit puts its hand into the idle controller
        // (SetAnim(0): rand(255)) and destroys it (sc_janemba: Janemba punched in the frame the hand appears)
        if w.field.is_some() && w.en_ref(k).strike.0 > 0 {
            let h = std::mem::take(&mut w.en(k).strike_hand);
            w.en(k).strike.0 = 0;
            if let Some(hk) = (h != 0).then(|| w.find(h)).flatten() {
                w.rng.rand(255);
                unmark_hand(w, hk);
                w.ents[hk].removed = true;
            }
        }
        set_anim(w, k, &[n(9)], true, Some(false));
    }
}
fn unmark_hand(w: &mut World, k: usize) {
    let m = match &mut w.ents[k].body {
        Body::Hand(h) => h.marked.take(),
        _ => None,
    };
    if let (Some(m), Some(c)) = (m, w.col.as_mut()) {
        c.mark_enemy(m, 1);
    }
}
pub fn janemba_arm(w: &World, i: usize, a: bool) -> Option<usize> {
    let me = w.ents[i].id;
    let t = if a { 0x34 } else { 0x35 };
    (0..w.ents.len()).find(|&k| !w.ents[k].removed && matches!(&w.ents[k].body, Body::Enemy(e) if e.part && e.parent == me && e.ty == t))
}
pub fn janemba_parts(w: &mut World, i: usize) {
    let me = w.ents[i].id;
    let field = w.field.is_some();
    for (t, ox, oy) in [(0x34, -12, -22), (0x35, 44, -22), (0x37, 16, -41)] {
        let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
        // field: the parts appear at their spawn offsets; 0x37's node places them from the next frame
        let (sx, sy) = if field {
            match t {
                0x34 => (-16, -16),
                0x35 => (48, -16),
                _ => (16, -40),
            }
        } else {
            (ox, oy)
        };
        if let Some(pi) = crate::battle::spawn_enemy_at(w, t, px + sx, py + sy, me, Some(-1), field) {
            let p = w.en(pi);
            p.part = true;
            p.part_offset = (ox, oy);
            p.a.hittable = false;
            // field: the enemy constructor marks the footprint on the enemy grid where the part is created; the
            // mirror node moves the parts without marks, so that mark stays (sc_janemba: minis hop around it)
            if field {
                let a = &w.en_ref(pi).a;
                let m = [a.px(), a.py(), a.px() + a.foot_w, a.py() + a.foot_h];
                if let Some(c) = w.col.as_mut() {
                    c.mark_enemy(m, 0);
                }
                // nothing unmarks it later (no re-sync for a part)
                w.en(pi).marked = None;
            }
        }
    }
}
pub fn hand_strike(w: &mut World, i: usize, a_side: bool) -> Option<EntityId> {
    let pi = player(w, i)?;
    let t = w.data.enemy(0x36).cloned()?;
    // an enemy object: its constructor's SetAnim(0) draws the idle start tick
    w.rng.rand(255);
    let (mut ppx, mut ppy) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    // field: the arm part's controller makes the hand before the player's update (its frame-start position)
    let slot = w.pl_ref(pi).slot as usize;
    if let Some(&(x, y)) = w.field.as_ref().and_then(|f| f.frame_start_pos.get(slot)) {
        ppx = x >> 8;
        ppy = y >> 8;
    }
    let mut a = Actor { x: (ppx + if a_side { 4 } else { -20 }) << 8, y: ppy << 8, facing: if a_side { 2 } else { 3 }, ..Default::default() };
    a.stats = crate::damage::CombatStats { str_: t.str_, pow: t.pow, end: t.end, end2: t.end, res_m: 128, res_e: 128, crit_bonus: 0, block_factor: 0x1000 };
    let d = Arc::clone(&w.data);
    // the strike's spawn record 0x0877B050 gives the hand gfx 0x96 (the type table's 0x97 is the HFIL field hand's;
    // sc_janemba: the strike hand is sprite 150)
    let gfx = 0x96 - crate::content::GFX_OFFSET;
    a.set_sprite(&d, if w.field.is_some() && d.sprite(gfx).is_some() { Some(gfx) } else { t.sprite });
    a.set_anim(&d, &[n(18)], true, Some(false));
    a.hittable = false;
    // field: an enemy object - its constructor marks its footprint on the enemy grid (sc_janemba)
    let marked = if w.field.is_some() {
        let m = [a.px(), a.py(), a.px() + a.foot_w, a.py() + a.foot_h];
        if let Some(c) = w.col.as_mut() {
            c.mark_enemy(m, 0);
        }
        Some(m)
    } else {
        None
    };
    Some(w.add(Body::Hand(Box::new(Hand { a, t: 0, hit: false, marked }))))
}
/// field: a strike hand is an enemy object (type 0x36, 50 HP, Idle): the player's hits reach it (sc_janemba: a punch
/// on Janemba and the hand computes both - crit chance and roll each; the hand's reaction is the flash's: see
/// janemba_parts_hurt)
pub fn hand_message(w: &mut World, i: usize, msg: &mut HitMsg) -> bool {
    let Body::Hand(h) = &w.ents[i].body else { return false };
    // only until the arm's strike controller ends (it frees the hand's tiles then: `marked`); the fading hand is out
    // of the enemy list (sc_janemba: a blast through it at tick 46 computes nothing)
    if msg.team == 0 || h.marked.is_none() || !overlaps(&msg.rect, &h.a.body_box()) {
        return false;
    }
    let def = h.a.stats;
    let res = compute_damage(msg.atk, msg.mult, &msg.att_stats, &def, false, &mut w.rng);
    msg.crit = res.crit;
    true
}
/// Janemba hand (type 0x36): one hit during its frames 16..55, gone after 57
pub fn hand_update(w: &mut World, i: usize) {
    let (t, hit) = {
        let Body::Hand(h) = &mut w.ents[i].body else { return };
        h.a.tick_anim();
        h.t += 1;
        (h.t, h.hit)
    };
    // the rising controller (0x0804C338) ends on tick 40 with SetAnim(0) (rand(255)); hits from tick 16
    // battle: tick 40 SetAnim(0)s it (field: the arm part's strike controller does, at its end)
    if t == 40 && w.field.is_none() {
        w.rng.rand(255);
    }
    let pi = w.player_idx(0);
    if let Some(pi) = pi {
        if !hit && !w.pl_ref(pi).dead && (16..40).contains(&t) {
            let Body::Hand(h) = &w.ents[i].body else { return };
            let id = w.ents[i].id;
            let mut m = HitMsg { team: 0, rect: h.a.body_box(), dir: h.a.facing, mult: 0x400, atk: atk(2), attacker: id, att_stats: h.a.stats, att_is_player: false, source: id, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
            if player::on_message(w, pi, &mut m) {
                if let Body::Hand(h) = &mut w.ents[i].body {
                    h.hit = true;
                }
                // 0x0803A0F2: the hit sound 48 + rand(2)
                let s = 48 + w.rng.rand(2);
                w.sfx(s, 64);
            }
        }
    }
    if let Body::Hand(h) = &mut w.ents[i].body {
        h.a.alpha = if t < 12 { t * 256 / 12 } else if t > 50 { ((57 - t) * 256 / 7).max(0) } else { 256 };
    }
    if t >= 57 {
        w.ents[i].removed = true;
    }
}
const MINI_PTS: [(i32, i32); 5] = [(104, 112), (120, 136), (152, 144), (184, 136), (200, 112)];
/// field: the puffs announcing the minis the next top-up creates (one per missing mini, at its spot)
/// returns the missing count, which the top-up 16 frames later creates as it is (sc_janemba: a mini killed in
/// between is not replaced - 4 new minis with none left)
pub fn mini_puffs(w: &mut World) -> usize {
    let live = w.live_enemies().into_iter().filter(|&o| w.en_ref(o).ty == 0x38).count();
    let n = 5usize.saturating_sub(live);
    for &(x, y) in MINI_PTS.iter().take(n) {
        crate::fx::puff_rng(w, x + 8, y + 8);
    }
    n
}
/// field: the top-up of `mini_puffs`'s count (at the fixed spots, in order)
pub fn mini_create(w: &mut World, i: usize, n: usize) {
    let b = w.arena.bounds;
    let me = w.ents[i].id;
    for &(x, y) in MINI_PTS.iter().take(n) {
        crate::battle::spawn_enemy(w, 0x38, b.x0.max((b.x1 - 16).min(x)), b.y0.max((b.y1 - 16).min(y)), me, None);
    }
}
pub fn top_up_minis(w: &mut World, i: usize) {
    const PTS: [(i32, i32); 5] = MINI_PTS;
    let live = w.live_enemies().into_iter().filter(|&o| w.en_ref(o).ty == 0x38).count();
    let b = w.arena.bounds;
    let me = w.ents[i].id;
    for &(x, y) in PTS.iter().take(5usize.saturating_sub(live)) {
        crate::battle::spawn_enemy(w, 0x38, b.x0.max((b.x1 - 16).min(x)), b.y0.max((b.y1 - 16).min(y)), me, None);
    }
}
pub fn land_on_rock(w: &mut World, i: usize) -> bool {
    let fb = w.en_ref(i).a.foot_box();
    let (cx, cy) = w.en_ref(i).a.center();
    for k in 0..w.ents.len() {
        if w.ents[k].removed {
            continue;
        }
        let Body::Rock { x, y, .. } = w.ents[k].body else { continue };
        if !overlaps(&Rect::new(x + 16, y + 16, x + 48, y + 48), &fb) {
            continue;
        }
        let hp = if let Body::Rock { hp, .. } = &mut w.ents[k].body {
            *hp -= 1;
            *hp
        } else {
            0
        };
        crate::fx::puff(w, cx, cy, 0x1800);
        if hp <= 0 {
            w.ents[k].removed = true;
            return true;
        }
    }
    false
}
pub fn worm_spawn_body(w: &mut World, i: usize) {
    let me = w.ents[i].id;
    let mut chain = vec![me];
    for k in 1..=5 {
        let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
        let Some(si) = crate::battle::spawn_enemy(w, 0x8f, px, py - 12 * k, me, None) else { break };
        // the 12-slot ring (0x0802D650) replays the slot it overwrites when a command arrives: a body moves only on
        // a received command (VERIFIED segment sb 21-9: each segment starts one frame after the one before it)
        let wd = w.en_ref(i).worm_dir;
        w.en(si).cmds = vec![wd; 12];
        chain.push(w.ents[si].id);
    }
    for k in 1..chain.len() {
        if let Some(a) = w.find(chain[k - 1]) {
            w.en(a).next = chain[k];
        }
        if let Some(b) = w.find(chain[k]) {
            w.en(b).prev = chain[k - 1];
        }
    }
}
/// the head and the segments stay in the idle state while they move (no SetAnim(1): VERIFIED segment sb 21-11, only
/// the tails' walk state takes the event's stop message)
pub fn worm_head_step(w: &mut World, i: usize) {
    if w.rng.rand(200) == 0 {
        let mut d = w.en_ref(i).worm_dir ^ 2;
        // 0x0802D546: the extra quarter turn when rand(2) == 0
        if w.rng.rand(2) == 0 {
            d ^= 1;
        }
        w.en(i).worm_dir = d;
    }
    for _ in 0..4 {
        let (dx, dy) = DIR_VEC[w.en_ref(i).worm_dir as usize];
        let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
        if fits(w, i, px + dx, py + dy) {
            let e = w.en(i);
            e.a.x += dx << 8;
            e.a.y += dy << 8;
            break;
        }
        let e = w.en(i);
        e.worm_dir = (e.worm_dir + 1) & 3;
    }
    let wd = w.en_ref(i).worm_dir;
    w.en(i).a.facing = wd;
    let next = w.en_ref(i).next;
    if let Some((ni, _)) = w.enemy_by_id(next) {
        w.en(ni).cmds.push(wd);
    }
    contact_hit_foot(w, i);
}
pub fn worm_body_step(w: &mut World, i: usize) {
    // the last segment (no next, 0x0802D674): sprite def 0x12A (Worm Tail) set again every frame (0x0800EDF8: the
    // animation restarts) + SetAnim(1)
    if w.enemy_by_id(w.en_ref(i).next).is_none_or(|(k, _)| w.ents[k].removed) {
        let d = Arc::clone(&w.data);
        let idx = 0x12A - crate::content::GFX_OFFSET;
        if d.sprite(idx).is_some() {
            let e = w.en(i);
            e.gfx_now = 0x12A;
            e.a.set_sprite(&d, Some(idx));
            set_anim(w, i, &[n(1)], true, Some(true));
        }
    }
    if w.en_ref(i).cmds.len() > 12 {
        let d = w.en(i).cmds.remove(0);
        let (dx, dy) = DIR_VEC[d as usize];
        {
            let e = w.en(i);
            e.a.x += dx << 8;
            e.a.y += dy << 8;
            e.a.facing = d;
        }
        let next = w.en_ref(i).next;
        if let Some((ni, _)) = w.enemy_by_id(next) {
            w.en(ni).cmds.push(d);
        }
    }
    contact_hit_foot(w, i);
}

// ---- damage --------------------------------------------------------------------------------------------------------

/// enemy message handler (0x0800FAFA kind 0): ComputeDamage on every accepted hit, the player's melee hooks, then
/// the top state's on-hit tree (whose ApplyDamage takes the HP)
pub fn on_message(w: &mut World, i: usize, msg: &mut HitMsg) -> bool {
    // field: Janemba's parts are enemy objects too - a hit under one computes its damage (crit chance + roll) and
    // nothing else (sc_janemba: a punch under the lowered arm in the ascent)
    if w.field.is_some() && msg.team != 0 && !w.ents[i].removed && matches!(w.en_ref(i).ty, 0x34 | 0x35 | 0x37) && w.en_ref(i).part {
        let e = w.en_ref(i);
        if msg.foot || !overlaps(&msg.rect, &e.a.body_box_ground()) {
            return false;
        }
        let def = e.a.stats;
        let res = compute_damage(msg.atk, msg.mult, &msg.att_stats, &def, false, &mut w.rng);
        msg.crit = res.crit;
        return true;
    }
    {
        let e = w.en_ref(i);
        // (the handler `0x0800FBE6` takes sender classes 1 and 2)
        if !matches!(msg.team, 1 | 2) || e.part || e.defeated || w.ents[i].removed || !e.a.hittable {
            return false;
        }
        // field: the body box (0x0800FE78) is at the ground position - the height does not lift it (sc_janemba:
        // a punch lands on Janemba 66 px up in its ascent)
        let body = if w.field.is_some() { e.a.body_box_ground() } else { e.a.body_box() };
        if !overlaps(&msg.rect, &if msg.foot { e.a.foot_box() } else { body }) {
            return false;
        }
    }
    let e = w.en_ref(i);
    let blocking = e.a.blocking && (msg.dir ^ 1) == e.a.facing;
    let def = e.a.stats;
    let res = compute_damage(msg.atk, msg.mult, &msg.att_stats, &def, blocking, &mut w.rng);
    msg.crit = res.crit;
    msg.raw = res.raw;
    msg.dmg = res.dmg;
    if res.unblocked != res.dmg {
        w.events.push(Event::DamageRoll { target: w.ents[i].id, dmg: res.unblocked });
    }
    w.events.push(Event::Damage { target: w.ents[i].id, attacker: msg.attacker, dmg: res.dmg, crit: res.crit });
    if msg.att_is_player {
        w.events.push(Event::Hit);
        if res.dmg > 0 {
            if let Some(pi) = w.find(msg.attacker) {
                if matches!(w.ents[pi].body, Body::Player(_)) {
                    player::on_dealt(w, pi, msg, res.dmg);
                }
            }
        }
    }
    {
        let e = w.en(i);
        e.crit_hit = res.crit;
        e.splat_hit = msg.atk.sub == 3;
    }
    crate::nodes::on_hit(w, i, msg);
    true
}

/// ApplyDamage -> vfunc +0x58 (0x0800FA2A): true when this hit killed the enemy
pub fn take_hit(w: &mut World, i: usize, msg: &HitMsg) -> bool {
    // the ApplyDamage node (0x080187A4) stores the hit's direction (obj+0x290) before the call: an on-hit tree
    // without it keeps the old one (real route 8-71: Babidi's hit state slides him along his first direction)
    w.en(i).last_hit_dir = msg.dir;
    // 0x0800FA2A: only HP == 0 counts as already dead; a negative HP (see heal) still takes the hit and dies
    if w.en_ref(i).hp1024 == 0 {
        return true;
    }
    // the damage number's object is made here (`0x0800E8E6`), not at the hit: an on-hit tree without ApplyDamage
    // shows none (segment sb 19-3: Kid Buu's teleport on a hit)
    crate::vram::damage_number(w, w.ents[i].id);
    let dmg = msg.dmg;
    let before = w.en_ref(i).hp().max(0);
    w.en(i).hp1024 -= dmg;
    let shown = before - w.en_ref(i).hp().max(0);
    if dmg > 0 {
        w.en(i).a.flash = 12;
        if msg.att_is_player {
            w.stats.hits_dealt += 1;
            w.stats.damage_dealt += shown;
            if msg.crit {
                w.stats.crits += 1;
            }
        }
    }
    if w.en_ref(i).hp1024 <= 0 {
        w.en(i).hp1024 = 0;
        on_death(w, i);
        return true;
    }
    false
}
pub fn take_direct(w: &mut World, i: usize, n1024: i64) {
    if w.en_ref(i).defeated {
        return;
    }
    w.en(i).hp1024 -= n1024;
    if w.en_ref(i).hp1024 <= 0 {
        w.en(i).hp1024 = 0;
        on_death(w, i);
    }
}
/// vfunc +0x60 (0x0800FA80): HP += ceil(n / 1024) whole HP, capped at max HP; no death check (a negative amount from
/// HealFraction's 32-bit wrap can leave HP <= 0 with the enemy alive until its next ApplyDamage)
pub fn heal(w: &mut World, i: usize, n1024: i64) {
    let add = ((n1024 + 0x3ff) >> 10) << 10;
    if add == 0 {
        return;
    }
    let e = w.en(i);
    e.hp1024 = e.max_hp1024.min(e.hp1024 + add);
}
pub fn scripted_defeat(w: &mut World, i: usize) {
    w.en(i).hp1024 = 0;
    on_death(w, i);
}
/// type +0x14 node word +8
fn on_death(w: &mut World, i: usize) {
    let ty = w.en_ref(i).ty;
    let death = w.data.enemy(ty).map(|t| t.death_name.clone()).unwrap_or_default();
    let me = w.ents[i].id;
    if death == "special" && (ty == 0x3c || ty == 0x3d) {
        let other = w.live_enemies().into_iter().find(|&o| w.ents[o].id != me && matches!(w.en_ref(o).ty, 0x3c | 0x3d));
        if let Some(o) = other {
            {
                let e = w.en(i);
                e.defeated = true;
                e.primary = false;
            }
            w.ents[i].removed = true;
            // (the puff 0x086FD9D0 at rate 0x1000 + rand(0x1000) - the real route's Kid Buu, 19-3: a punch on both
            // copies, the first one's death draws its rate after its on-hit tree)
            let (cx, cy) = w.en_ref(i).a.center();
            w.en(o).primary = true;
            if w.field.is_some() {
                // (the death node 0x08032D44 with copies left - count by message 5 > 1 - takes the object out of the
                // world list without its defeat handler: no defeat, no on-defeat callback; a placed copy hands its
                // placement (callback, drops) to the survivor - the real route 19-3 f13415: the punched copy was the
                // placed one, the game starts no s460)
                let oid = w.ents[o].id;
                if let Some(f) = w.field.as_mut() {
                    if let Some(p) = f.placed.iter_mut().find(|p| p.0 == me) {
                        p.0 = oid;
                    }
                    for l in f.placed_loot.iter_mut().filter(|l| l.0 == me) {
                        l.0 = oid;
                    }
                }
                crate::fx::puff_rng(w, cx, cy);
                // (field: the survivor keeps its AI state - the real route, 19-3: the same punch then hits it through
                // its own on-hit tree, three chances, no idle draw; battles keep the reference's phase-1 reset)
            } else {
                crate::fx::puff(w, cx, cy, 0x1800);
                let st = w.data.enemy(0x3c).map(|t| t.ai).unwrap_or(0);
                crate::nodes::reset(w, o, st);
            }
            return;
        }
    }
    {
        let e = w.en(i);
        e.defeated = true;
        e.a.hittable = false;
        e.a.blocking = false;
    }
    w.stats.kills += 1;
    // a placed field enemy's loot block as its death leaves it (footprint centre x / bottom y, the killing hit's
    // direction; field.rs defeats)
    {
        let e = w.en_ref(i);
        let (cx, cy) = (e.a.px() + (e.a.foot_w >> 1), e.a.py() + e.a.foot_h);
        let v = crate::geom::DIR_VEC[e.last_hit_dir as usize & 3];
        if let Some(l) = w.field.as_mut().and_then(|f| f.placed_loot.iter_mut().find(|l| l.0 == me)) {
            *l = (me, cx, cy, v.0 << 8, v.1 << 8);
        }
    }
    let exp = w.data.enemy(ty).map(|t| t.exp).unwrap_or(0);
    if let (Some(f), Some(pi)) = (w.field.is_some().then_some(()), ctx_player(w, i)) {
        // field: the death effect object gives the kill XP on its first update, at the start of the next frame's
        // update (story playthrough: the level-up rolls come the frame after the KO)
        let _ = f;
        let pid = w.ents[pi].id;
        if let Some(f) = w.field.as_mut() {
            f.xp_due.push((pid, exp));
        }
    } else if let Some(pi) = ctx_player(w, i) {
        player::gain_xp(w, pi, exp);
    }
    for k in 0..w.ents.len() {
        if let Body::Shot(s) = &w.ents[k].body {
            if s.owner == me && matches!(s.kind, ShotKind::GrowBeam(_)) {
                shots::beam_stop(w, k);
            }
        }
    }
    // Kid Buu's death node (`0x08032D44`): the last copy is not destroyed - its defeat handler (vfunc +0x74) runs
    // and it stays as it is (segment sb 19-3 f21479: no explosion, the object lives on into event 458)
    let last_copy = death == "special" && (ty == 0x3c || ty == 0x3d);
    if (death == "Idle" || last_copy) && w.field.is_some() {
        // field: the death node Idle (0x080716F4) changes nothing - the enemy stays as it is (its defeat scene
        // takes it over; story playthrough: Olibu stays idle at the KO punch)
        w.en(i).ko = -1;
    } else if death == "Idle" {
        set_anim(w, i, &[nm("knockdown"), nm("ko"), nm("hurt")], true, Some(false));
        w.en(i).ko = 0;
    } else {
        die(w, i, true);
        if death == "special" && w.field.is_some() {
            if let Some(k) = TANKS.iter().position(|&t| t == ty) {
                tank_crew(w, i, k);
            }
        }
    }
}
/// the tanks (death node 0x080333C4, param = def +0xC): Battle Tank variant 0x7F (0), Juggernaut 0x3A (1), 0x8C (2)
pub const TANKS: [i32; 3] = [0x7f, 0x3a, 0x8c];
/// crew (enemy type, graphics) per rand(2) and tank (tables 0x087592DC / +0x18)
const TANK_CREW: [[(i32, i32); 3]; 2] = [[(8, 0x22), (0x43, 0xab), (0x2b, 0x88)], [(0x0f, 0x46), (0x2f, 0x8d), (0x61, 0xeb)]];
/// A destroyed tank's crew (0x080333C4, field): after the defeat handler, the removal and the puff, four enemies
/// from the template 0x087592BC (ctor 0x0800FD55, no callback, standing) at the tank's position + (0, 0), (24, 0),
/// (24, 24), (0, 24), each a rand(2) pick of the tank's two crew types; battles keep the reference (no crew)
fn tank_crew(w: &mut World, i: usize, tank: usize) {
    use crate::fieldcontent::{Cond, ObjDef, ObjKind, ObjRef};
    let (x, y) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
    for (dx, dy) in [(0, 0), (24, 0), (24, 24), (0, 24)] {
        let (ty, gfx) = TANK_CREW[w.rng.rand(2) as usize & 1][tank];
        let o = ObjDef { at: ObjRef::List(0xff, ty as u16), cond: Cond::Always, kind: ObjKind::Enemy { ty, x: x + dx, y: y + dy, sprite: gfx, on_defeat: None, behaviour: vec![] }, rom: 0x0875_92bc };
        crate::field::spawn_object(w, &o);
    }
}
pub fn die(w: &mut World, i: usize, explode: bool) {
    if !w.en_ref(i).defeated {
        w.en(i).defeated = true;
        w.stats.kills += 1;
    }
    if explode {
        let (cx, cy) = w.en_ref(i).a.center();
        let rate = 0x1000 + w.rng.rand(0x1000);
        // the enemy's sprite instance is destroyed first (`0x0800EC34`): the explosion can take its tiles
        // (segments sb 20-6, side 18-2: a dying enemy's block is free before the explosion's is taken)
        crate::vram::entity_gone(w, i);
        crate::fx::explosion(w, cx, cy, rate);
    }
    // the object's destruction frees its enemy-grid cells (removed entities are not re-synced: story playthrough, a
    // fighter's path re-plan went around a dead one)
    unmark_self(w, i);
    w.ents[i].removed = true;
}

/// hold the player (VampireGrab etc.)
pub fn hold_player(w: &mut World, i: usize, pi: usize, kind: HoldKind, frames: i32) {
    let me = w.ents[i].id;
    player::hold(w, pi, me, kind, frames);
}
