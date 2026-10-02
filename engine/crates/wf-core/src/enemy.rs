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
            script_hits: 0,
            marked: None,
            stepped: false,
            target: 0,
            ea: Default::default(),
            anim_ended: false,
        };
        // the constructor's SetAnim(0) runs before the object is in the world: no RNG draw
        e.set_anim_with(d, &[AnimRef::Name("idle")], true, Some(true), None, None, &mut Vec::new());
        e
    }
    /// SetAnim / SetAnimRaw + SetState: the state rule for the raw comes from the type's recorded rules, else the
    /// enemy SetAnim table, else a generic loop / one-shot; `rule` forces one. Re-entering restarts the state.
    #[allow(clippy::too_many_arguments)]
    pub fn set_anim_with(&mut self, d: &crate::content::GameData, refs: &[AnimRef], restart: bool, looped: Option<bool>, rule: Option<u32>, rng: Option<&mut crate::rng::Rng>, events: &mut Vec<Event>) {
        let prev = self.a.anim;
        self.a.set_anim(d, refs, restart, looped);
        let Some(raw) = self.a.anim_raw(d) else { return };
        if self.a.anim == prev && !restart && rule.is_none() {
            return;
        }
        let ar = &d.anim;
        let r = rule
            .or_else(|| ar.types.get(&self.ty).and_then(|m| m.get(&raw.to_string())).copied())
            .or_else(|| ar.setanim.get(&raw).copied())
            .unwrap_or(if looped == Some(false) { ar.fallback_once } else { ar.fallback_loop });
        self.anim_ended = false;
        let mut h = EHost { d, sprite: self.a.sprite, facing: self.a.facing, rng, events };
        self.ea.enter(&ar.rules, raw, r, &mut h);
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

/// the target player: present, not removed and not dead
pub fn player(w: &World, i: usize) -> Option<usize> {
    let slot = w.en_ref(i).target as usize;
    let pi = w.player_idx(slot).or_else(|| w.player_idx(0))?;
    if w.ents[pi].removed || w.pl_ref(pi).dead {
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
    tick_anim(w, i);
    {
        let e = w.en(i);
        if e.contact_cd > 0 {
            e.contact_cd -= 1;
        }
        if e.defeated {
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
    crate::nodes::tick(w, i);
    let d = Arc::clone(&w.data);
    let e = w.en_ref(i);
    let name = e.a.anim_name(&d).map(String::from);
    let moving = e.moving;
    if e.a.anim.is_none() || matches!(name.as_deref(), Some("idle") | Some("walk")) {
        if moving && name.as_deref() != Some("walk") {
            set_anim(w, i, &[nm("walk")], false, Some(true));
        } else if !moving && name.as_deref() == Some("walk") {
            set_anim(w, i, &[nm("idle")], false, Some(true));
        }
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
    let (dx, dy) = to_player(w, i);
    w.en(i).a.facing = facing_toward(dx, dy);
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
pub fn in_melee_range(w: &mut World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    let p = &w.pl_ref(pi).a;
    if !p.hittable || !overlaps(&p.body_box(), &w.en_ref(i).a.body_box().inflate(4)) {
        return false;
    }
    face_player(w, i);
    true
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
    !e.part && e.prev == 0 && !e.defeated && !w.ents[i].removed && e.a.z == 0 && !held
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
    w.en(i).moving = true;
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
    let mut m = HitMsg { team: 1, rect, dir: w.en_ref(i).last_hit_dir, mult: 409, atk: atk(3), attacker: id, att_stats: w.en_ref(i).a.stats, att_is_player: false, source: id, crit: false, raw: 0, dmg: 0 };
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
    w.en(i).moving = true;
    let vx = -((dx as i64 * speed as i64) / m as i64) as i32;
    let vy = -((dy as i64 * speed as i64) / m as i64) as i32;
    try_move(w, i, vx, vy, if dx >= 0 { -speed } else { speed }, if dy >= 0 { -speed } else { speed });
    !w.en_ref(i).stepped
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
    w.en(i).moving = true;
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
    w.en(i).moving = true;
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
        w.en(i).moving = true;
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
pub fn align(w: &mut World, i: usize, speed: i32) -> bool {
    if player(w, i).is_none() {
        return false;
    }
    let (dx, dy) = to_player(w, i);
    if dx.abs() > dy.abs() {
        if dy.abs() < 3 {
            w.en(i).a.facing = if dx > 0 { 3 } else { 2 };
            return true;
        }
        w.en(i).a.facing = if dy > 0 { 0 } else { 1 };
        if !walk_by(w, i, 0, dy.signum() * speed, dy != 0 && speed != 0) {
            w.en(i).a.facing = if dx > 0 { 3 } else { 2 };
            return true;
        }
    } else {
        if dx.abs() < 3 {
            w.en(i).a.facing = if dy > 0 { 0 } else { 1 };
            return true;
        }
        w.en(i).a.facing = if dx > 0 { 3 } else { 2 };
        if !walk_by(w, i, dx.signum() * speed, 0, dx != 0 && speed != 0) {
            w.en(i).a.facing = if dy > 0 { 0 } else { 1 };
            return true;
        }
    }
    false
}

// ---- attacks -------------------------------------------------------------------------------------------------------

fn msg_from(w: &World, i: usize, team: u8, rect: Rect, dir: Dir, mult: i32, a: AttackType) -> HitMsg {
    let id = w.ents[i].id;
    HitMsg { team, rect, dir, mult, atk: a, attacker: id, att_stats: w.actor(i).map(|x| x.stats).unwrap_or_default(), att_is_player: false, source: id, crit: false, raw: 0, dmg: 0 }
}

/// 0x0802A0C4: face the player on the dominant axis, the half-box on that side
pub fn strike_rect(w: &mut World, i: usize, face: bool) -> Option<Rect> {
    player(w, i)?;
    let mut f = w.en_ref(i).a.facing;
    if face {
        let (dx, dy) = to_player(w, i);
        f = facing_toward(dx, dy);
        w.en(i).a.facing = f;
    }
    let a = &w.en_ref(i).a;
    let bb = a.body_box();
    let ff = a.foot_box();
    let (cx, cy) = a.center();
    Some(match f {
        3 => Rect::new(cx, cy, bb.x1 + 3, bb.y1 + 3),
        2 => Rect::new(bb.x0 - 4, cy, cx, bb.y1 + 3),
        0 => Rect::new(ff.x0, ff.y0, ff.x1, ff.y1 - 6),
        _ => Rect::new(bb.x0, bb.y0, bb.x1, cy),
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
/// HeavyStrike 0x0802F77C
pub fn heavy_strike(w: &mut World, i: usize) {
    let Some(pi) = player(w, i) else { return };
    if !w.pl_ref(pi).a.hittable {
        return;
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
    player::on_message(w, pi, &mut m);
}
pub fn contact_hit(w: &mut World, i: usize) -> bool {
    let Some(pi) = player(w, i) else { return false };
    if !w.pl_ref(pi).a.hittable || w.en_ref(i).contact_cd > 0 {
        return false;
    }
    let bb = w.en_ref(i).a.body_box();
    if !overlaps(&bb, &w.pl_ref(pi).a.body_box()) {
        return false;
    }
    let mut m = msg_from(w, i, 0, bb, w.en_ref(i).a.facing, 0x400, atk(2));
    let ok = player::on_message(w, pi, &mut m);
    if ok {
        w.en(i).contact_cd = 20;
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
    let mut m = HitMsg { team: 1, rect, dir: w.en_ref(i).last_hit_dir, mult: 409, atk: atk(3), attacker: pid, att_stats: w.pl_ref(pi).a.stats, att_is_player: true, source: w.ents[i].id, crit: false, raw: 0, dmg: 0 };
    broadcast(w, &mut m, false);
}

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
const DIAG: [(i32, i32); 4] = [(-316, 316), (316, -316), (-316, -316), (316, 316)];

pub fn shot(w: &mut World, i: usize, kind: ShotType) {
    let a = w.en_ref(i).a.clone();
    let f = a.facing;
    let (mx, my) = GUN_MUZZLE[f as usize];
    let at = ((a.px() + mx) << 8, (a.py() + my) << 8);
    let (cx, cy) = a.center();
    let (dx, dy) = DIR_VEC[f as usize];
    let owner = w.ents[i].id;
    let mk = |w: &mut World, dir: Dir, x: i32, y: i32, vx: i32, vy: i32, b: [i32; 4], o: StraightOpts| {
        let s = Shot::new(owner, dir, 0, x, y, ShotKind::Straight(Box::new(StraightOpts { vx, vy, b, ..o })));
        w.add(Body::Shot(Box::new(s)));
    };
    let base = StraightOpts { a: atk(1), mult: 1024, ..Default::default() };
    match kind {
        ShotType::Gun => mk(w, f, at.0, at.1, dx * 672, dy * 672, GUN_BOX[f as usize], StraightOpts { fragile: true, name: shots::SName::Gun, ..base }),
        ShotType::Rifle => mk(w, f, at.0, at.1, dx * 448, dy * 448, [8, 8, 16, 16], StraightOpts { fragile: true, name: shots::SName::Rifle, trail: Some(shots::Trail { fx: shots::FxId::Fx0874e614, rec: 4, n: 3, rate: 0x2aaa }), ..base }),
        ShotType::Shell => mk(w, f, (cx + dx * 12) << 8, (cy + dy * 12) << 8, dx * 448, dy * 448, [8, 8, 16, 16], StraightOpts { fragile: true, name: shots::SName::Shell, trail: Some(shots::Trail { fx: shots::FxId::Fx0871e630, rec: 4, n: 3, rate: 0x2aaa }), ..base }),
        ShotType::Star => mk(w, f, (cx + dx * 8) << 8, (cy + dy * 8) << 8, dx * 448, dy * 448, [4, 4, 8, 8], StraightOpts { fragile: true, explode: false, name: shots::SName::Star, ..base }),
        ShotType::Wrap | ShotType::Burst => {
            let dirs: Vec<Dir> = if kind == ShotType::Wrap { vec![f] } else { vec![0, 1, 2, 3] };
            for d in dirs {
                let (ddx, ddy) = DIR_VEC[d as usize];
                let b = if d >= 2 { [8, 4, 16, 8] } else { [4, 8, 8, 16] };
                let nm = if kind == ShotType::Burst { shots::SName::WrapDiag } else { shots::SName::Wrap };
                mk(w, d, (cx + ddx * 8) << 8, (cy + ddy * 8) << 8, ddx * 448, ddy * 448, b, StraightOpts { on_hit: OnHit::Wrap, explode: false, name: nm, ..base });
                if kind == ShotType::Burst {
                    let (vx, vy) = DIAG[d as usize];
                    mk(w, d, cx << 8, cy << 8, vx, vy, [8, 8, 16, 16], StraightOpts { on_hit: OnHit::Wrap, explode: false, name: shots::SName::WrapDiag, ..base });
                }
            }
        }
        ShotType::Spit => {
            let Some(pi) = player(w, i) else { return };
            let (tx, ty) = w.pl_ref(pi).a.center();
            let ddx = tx - cx;
            let ddy = ty - cy;
            let m = 1.max(ddx.abs()).max(ddy.abs());
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
            let vx = crate::fixed::round_div(ddx as i64 * 512, m as i64) as i32;
            let vy = crate::fixed::round_div(ddy as i64 * 512, m as i64) as i32;
            mk(w, f, cx << 8, (cy - 4) << 8, vx, vy, [4, 4, 8, 8], StraightOpts { explode: false, on_hit: OnHit::Stone, name: shots::SName::Spit, sector, ..base });
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
    {
        let a = &mut w.en(i).a;
        a.x = (pfx + ox - (a.foot_w >> 1)) << 8;
        a.y = (pfy + oy - (a.foot_h >> 1)) << 8;
    }
    clamp_inside(w, i);
    face_player(w, i);
    set_anim(w, i, &[n(19)], true, Some(false));
    let (cx, cy) = w.en_ref(i).a.center();
    crate::fx::puff(w, cx, cy, 0x1800);
}
pub fn kid_buu_split(w: &mut World, i: usize) {
    for _ in 0..16 {
        let dx = w.rng.rand(192) - 96;
        let dy = w.rng.rand(192) - 96;
        let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
        if !fits(w, i, px + dx, py + dy) || !fits(w, i, px - dx, py - dy) {
            continue;
        }
        let (cx, cy) = w.en_ref(i).a.center();
        crate::fx::puff(w, cx, cy, 0x1800);
        let (x0, y0) = (px, py);
        let hp;
        {
            let e = w.en(i);
            e.a.x = (x0 + dx) << 8;
            e.a.y = (y0 + dy) << 8;
            e.hp1024 = (e.hp1024 + 1024) >> 1;
            hp = e.hp1024;
        }
        if let Some(ci) = crate::battle::spawn_enemy(w, 0x3d, x0 - dx, y0 - dy, 0, Some(0x12a)) {
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
    let mx = (w.en_ref(i).a.px() + w.en_ref(o).a.px()) >> 1;
    let my = (w.en_ref(i).a.py() + w.en_ref(o).a.py()) >> 1;
    if fits(w, i, mx, my) {
        let a = &mut w.en(i).a;
        a.x = mx << 8;
        a.y = my << 8;
    }
    let (cx, cy) = w.en_ref(i).a.center();
    crate::fx::puff(w, cx, cy, 0x1800);
    let (ox, oy) = w.en_ref(o).a.center();
    crate::fx::puff(w, ox, oy, 0x1800);
    w.ents[o].removed = true;
    let oe = w.en(o);
    oe.defeated = true;
    oe.primary = false;
}
pub fn janemba_parts(w: &mut World, i: usize) {
    let me = w.ents[i].id;
    for (t, ox, oy) in [(0x34, -12, -22), (0x35, 44, -22), (0x37, 16, -41)] {
        let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
        if let Some(pi) = crate::battle::spawn_enemy(w, t, px + ox, py + oy, me, Some(-1)) {
            let p = w.en(pi);
            p.part = true;
            p.part_offset = (ox, oy);
            p.a.hittable = false;
        }
    }
}
pub fn hand_strike(w: &mut World, i: usize, a_side: bool) {
    let Some(pi) = player(w, i) else { return };
    let Some(t) = w.data.enemy(0x36).cloned() else { return };
    let (ppx, ppy) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
    let mut a = Actor { x: (ppx + if a_side { 4 } else { -20 }) << 8, y: ppy << 8, facing: if a_side { 2 } else { 3 }, ..Default::default() };
    a.stats = crate::damage::CombatStats { str_: t.str_, pow: t.pow, end: t.end, end2: t.end, res_m: 128, res_e: 128, crit_bonus: 0, block_factor: 0x1000 };
    let d = Arc::clone(&w.data);
    a.set_sprite(&d, t.sprite);
    a.set_anim(&d, &[n(18)], true, Some(false));
    a.hittable = false;
    w.add(Body::Hand(Box::new(Hand { a, t: 0, hit: false })));
}
/// Janemba hand (type 0x36): one hit during its frames 16..55, gone after 57
pub fn hand_update(w: &mut World, i: usize) {
    let (t, hit) = {
        let Body::Hand(h) = &mut w.ents[i].body else { return };
        h.a.tick_anim();
        h.t += 1;
        (h.t, h.hit)
    };
    let pi = w.player_idx(0);
    if let Some(pi) = pi {
        if !hit && !w.pl_ref(pi).dead && (16..=55).contains(&t) {
            let Body::Hand(h) = &w.ents[i].body else { return };
            let id = w.ents[i].id;
            let mut m = HitMsg { team: 0, rect: h.a.body_box(), dir: h.a.facing, mult: 0x400, atk: atk(2), attacker: id, att_stats: h.a.stats, att_is_player: false, source: id, crit: false, raw: 0, dmg: 0 };
            if player::on_message(w, pi, &mut m) {
                if let Body::Hand(h) = &mut w.ents[i].body {
                    h.hit = true;
                }
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
pub fn top_up_minis(w: &mut World, i: usize) {
    const PTS: [(i32, i32); 5] = [(104, 112), (120, 136), (152, 144), (184, 136), (200, 112)];
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
pub fn worm_head_step(w: &mut World, i: usize) {
    if w.rng.rand(200) == 0 {
        let mut d = w.en_ref(i).worm_dir ^ 2;
        if w.rng.rand(2) != 0 {
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
            e.moving = true;
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
    contact_hit(w, i);
}
pub fn worm_body_step(w: &mut World, i: usize) {
    if w.en_ref(i).cmds.len() >= 12 {
        let d = w.en(i).cmds.remove(0);
        let (dx, dy) = DIR_VEC[d as usize];
        {
            let e = w.en(i);
            e.a.x += dx << 8;
            e.a.y += dy << 8;
            e.a.facing = d;
            e.moving = true;
        }
        let next = w.en_ref(i).next;
        if let Some((ni, _)) = w.enemy_by_id(next) {
            w.en(ni).cmds.push(d);
        }
    }
    contact_hit(w, i);
}

// ---- damage --------------------------------------------------------------------------------------------------------

/// enemy message handler (0x0800FAFA kind 0): ComputeDamage on every accepted hit, the player's melee hooks, then
/// the top state's on-hit tree (whose ApplyDamage takes the HP)
pub fn on_message(w: &mut World, i: usize, msg: &mut HitMsg) -> bool {
    {
        let e = w.en_ref(i);
        if msg.team == 0 || e.part || e.defeated || w.ents[i].removed || !e.a.hittable {
            return false;
        }
        if !overlaps(&msg.rect, &e.a.body_box()) {
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
        e.last_hit_dir = msg.dir;
        e.crit_hit = res.crit;
        e.splat_hit = msg.atk.sub == 3;
    }
    crate::nodes::on_hit(w, i, msg);
    true
}

/// ApplyDamage -> vfunc +0x58 (0x0800FA2A): true when this hit killed the enemy
pub fn take_hit(w: &mut World, i: usize, msg: &HitMsg) -> bool {
    // 0x0800FA2A: only HP == 0 counts as already dead; a negative HP (see heal) still takes the hit and dies
    if w.en_ref(i).hp1024 == 0 {
        return true;
    }
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
            let (cx, cy) = w.en_ref(i).a.center();
            crate::fx::puff(w, cx, cy, 0x1800);
            w.en(o).primary = true;
            let st = w.data.enemy(0x3c).map(|t| t.ai).unwrap_or(0);
            crate::nodes::reset(w, o, st);
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
    let exp = w.data.enemy(ty).map(|t| t.exp).unwrap_or(0);
    if let Some(pi) = ctx_player(w, i) {
        player::gain_xp(w, pi, exp);
    }
    for k in 0..w.ents.len() {
        if let Body::Shot(s) = &w.ents[k].body {
            if s.owner == me && matches!(s.kind, ShotKind::GrowBeam(_)) {
                shots::beam_stop(w, k);
            }
        }
    }
    if death == "Idle" {
        set_anim(w, i, &[nm("knockdown"), nm("ko"), nm("hurt")], true, Some(false));
        w.en(i).ko = 0;
    } else {
        die(w, i, true);
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
        crate::fx::explosion(w, cx, cy, rate);
    }
    w.ents[i].removed = true;
}

/// hold the player (VampireGrab etc.)
pub fn hold_player(w: &mut World, i: usize, pi: usize, kind: HoldKind, frames: i32) {
    let me = w.ents[i].id;
    player::hold(w, pi, me, kind, frames);
}
