//! Projectiles and attack objects (player skills + the enemy classes of the bosses' data). Speeds, boxes, attack
//! types, multipliers and lifetimes are the data's (notes §5, §8b); the reference is projectiles.ts.
//!
//! Floating point: a few *fitted* hit models of the reference (not game mechanics: beam head speeds 3.34 / 8.5 /
//! 2.14 px per frame, the Big Bang growth, the Kamikaze ghost homing) accumulate decimal fractions in doubles. They are
//! kept as f64 using only IEEE-754 exact operations (+ - * / sqrt, comparisons, the JS rounding rule) so results are
//! bit-identical on every target (native, wasm32); no transcendental functions are used anywhere in the sim.

use serde::{Deserialize, Serialize};
use crate::damage::{atk, AttackType};
use crate::fx;
use crate::geom::{centered, overlaps, Dir, Rect, DIR_VEC};
use crate::player;
use crate::world::{broadcast, Body, EntityId, HitMsg, Layer, World};
use std::hash::{Hash, Hasher};

/// an f64 that hashes by its bits (fitted reference models only, see the module docs)
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct F(pub f64);
impl Hash for F {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.to_bits().hash(h)
    }
}
/// JS Math.round
pub fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}
/// V8's Math.hypot (max scaling + Kahan summation, src/builtins/math.tq)
pub fn js_hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    let max = a.max(b);
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0f64;
    let mut comp = 0.0f64;
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - comp;
        let pre = sum + summand;
        comp = (pre - sum) - summand;
        sum = pre;
    }
    sum.sqrt() * max
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum OnHit {
    #[default]
    None,
    /// mummy bandage: wraps + 1 (max 8)
    Wrap,
    /// Dabura's spit: the player turns to stone (297 frames)
    Stone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SName {
    #[default]
    KiBlast,
    Gun,
    Rifle,
    Shell,
    Star,
    Wrap,
    WrapDiag,
    Spit,
    Barrage,
    Burning,
    Volley,
}
impl SName {
    pub fn as_str(self) -> &'static str {
        match self {
            SName::KiBlast => "kiblast",
            SName::Gun => "gun",
            SName::Rifle => "rifle",
            SName::Shell => "shell",
            SName::Star => "star",
            SName::Wrap => "wrap",
            SName::WrapDiag => "burst",
            SName::Spit => "spit",
            SName::Barrage => "barrage",
            SName::Burning => "burning",
            SName::Volley => "volley",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FxId {
    Fx0874e614,
    Fx0871e630,
}
impl FxId {
    pub fn addr(self) -> u32 {
        match self {
            FxId::Fx0874e614 => 0x0874_e614,
            FxId::Fx0871e630 => 0x0871_e630,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trail {
    pub fx: FxId,
    pub rec: i32,
    pub n: i32,
    pub rate: i32,
}

/// straight-flying shot (ki blasts, bullets, rifle shots, shells, shuriken, wraps, spit, barrage, Burning Attack,
/// Volleyball wave): removed off camera, at a wall or on a hit
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StraightOpts {
    pub vx: i32,
    pub vy: i32,
    /// box relative to the position: x0 = px - ox, y0 = py - oy, size w x h
    pub b: [i32; 4],
    pub a: AttackType,
    pub mult: i32,
    pub fragile: bool,
    pub life: i32,
    pub on_hit: OnHit,
    pub explode: bool,
    pub name: SName,
    /// spark shown in place for this many ticks at the end of the flight (ki blast / Burning Attack)
    pub fx_end: i32,
    pub trail: Option<Trail>,
    /// ticks before the shot moves / hits
    pub delay: i64,
    pub rng_spawn: u32,
    pub rng_hit: u32,
    pub sfx_spawn: i32,
    pub sfx_hit: i32,
    pub sector: i32,
    // state
    pub dead: i32,
    pub spawned: bool,
    pub x0: i32,
    pub y0: i32,
}
impl Default for StraightOpts {
    fn default() -> Self {
        StraightOpts {
            vx: 0,
            vy: 0,
            b: [0; 4],
            a: atk(1),
            mult: 1024,
            fragile: false,
            life: 0,
            on_hit: OnHit::None,
            explode: true,
            name: SName::KiBlast,
            fx_end: 0,
            trail: None,
            delay: 0,
            rng_spawn: 0,
            rng_hit: 0,
            sfx_spawn: -1,
            sfx_hit: -1,
            sector: 0,
            dead: -1,
            spawned: false,
            x0: 0,
            y0: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Muzzle {
    /// player.muzzle(16): body centre + dir * 16 (+2 y when horizontal)
    Player16,
    /// Kid Buu: body centre + (ox - 8 + dx * 12, oy + dy * 12)
    KidBuu { ox: i32, oy: i32 },
}

pub const BEAM_POS_KAME_FIRE: [(i32, i32); 4] = [(8, 16), (8, -16), (-16, -6), (32, -6)];
pub const BEAM_POS_BBK_FIRE: [(i32, i32); 4] = [(8, 48), (8, -64), (-48, -6), (64, -6)];
pub const BEAM_POS_BANSHEE_FIRE: [(i32, i32); 4] = [(4, 16), (12, -16), (-16, -6), (32, -6)];
pub const FINAL_FLASH_POS: [(i32, i32); 4] = [(8, 0), (8, -16), (-8, -6), (24, -6)];

/// growing beam (class 0x080206B8): the head advances `speed` px per frame; each frame it hits the targets in the
/// near part (1 or 2 hits alternately) and the far part (5 of 6 frames) - the fitted hit model of the reference
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct GrowBeam {
    pub len: F,
    pub done: bool,
    pub a: AttackType,
    pub mult: i32,
    pub width: i32,
    pub speed: F,
    pub max_len: F,
    pub muzzle: Muzzle,
    pub gfx: bool,
    pub vis_pos: Option<[(i32, i32); 4]>,
    pub hidden: bool,
    pub vt: i32,
    pub stopped: bool,
    pub retract: Option<(i32, i32, i32)>,
    pub next_fx: i64,
    pub game: bool,
    pub glen: i32,
    pub gstop: bool,
    /// last muzzle position (kept when the owner is gone)
    pub last: (i32, i32),
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct FullBeam {
    pub len: i32,
    pub life: i64,
    pub a: AttackType,
    pub mult: i32,
    pub width: i32,
    pub table: Option<u8>,
    pub vis_pos: Option<[(i32, i32); 4]>,
    pub scale: i32,
    pub ending: bool,
    pub vis: Option<(i32, i32)>,
    pub hit_win: Option<(i64, i64)>,
    /// the player's Final Flash (the game's update 0x0803710C)
    pub game: bool,
    pub next: i64,
    pub end_t: i32,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct AreaHit {
    pub b: Rect,
    pub life: i64,
    pub every: i64,
    pub a: AttackType,
    pub mult: i32,
    pub grow: i32,
    pub first: bool,
    pub dragon: bool,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct BigBangBall {
    pub charge256: i64,
    pub launched: bool,
    pub lv: i32,
    /// obj+0x54 tick counter and obj+0x58 frame
    pub u: i32,
    pub f: i32,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct MasenkoCursor {
    pub d: i32,
    pub step: i32,
    pub released: i64,
    pub mult: i32,
    pub lv: i32,
    pub ball: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MPhase {
    Charge,
    Fly,
    Blast,
}
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct MasenkoBall {
    pub cursor: EntityId,
    pub lv: i32,
    pub phase: MPhase,
    pub vx: i32,
    pub vy: i32,
    pub a: i32,
    pub b: i32,
    pub tb: i32,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct KameBeam {
    pub len: i32,
    pub done: bool,
    pub a: AttackType,
    pub mult: i32,
    pub gfx: bool,
    /// obj+0x18 piece-variant tick
    pub vt: i32,
    /// obj+0x11: the head was stopped (impact piece)
    pub stopped: bool,
    /// obj+0x14: frame of the next hit effect
    pub next: i64,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Ghost {
    pub vx: F,
    pub vy: F,
    pub free: bool,
    pub wait: i32,
    pub slot: i32,
    pub at: i32,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub enum ShotKind {
    Straight(Box<StraightOpts>),
    GrowBeam(Box<GrowBeam>),
    FullBeam(Box<FullBeam>),
    AreaHit(Box<AreaHit>),
    BigBang(Box<BigBangBall>),
    /// the player's beam family (KameBeam)
    Kame(Box<KameBeam>),
    /// Ultra Volleyball wave
    Volley { len: i32, done: bool, n: i32 },
    MasenkoCursor(Box<MasenkoCursor>),
    MasenkoBall(Box<MasenkoBall>),
    Ghost(Box<Ghost>),
    /// energy wave 0x08049C98: segment tail..head, grows 4/frame to 48 then moves 4 px/frame
    EnergyWave { len: i32, mode: i32 },
    /// flame 0x0804BFCC: 16x16, 20 frames, one hit
    Flame { vx: i32, vy: i32, done: bool },
    /// bomb 0x0804A2E8: lobbed (height in 1/8 px), 30-frame fuse
    Bomb { vx: i32, vy: i32, z8: i32, vz8: i32, landed: i64 },
    /// Buu stretch-arm hitbox 0x08028C04
    StretchArm { ox: i32, oy: i32, hit_done: bool },
    /// Majin Buu candy beam 0x080286D4
    CandyBeam { ox: i32, oy: i32, done: bool },
    /// Super Buu lobbed ball 0x08021280
    SuperBall { phase: i32, sx: i32, sy: i32, tx: i32, ty: i32 },
    /// Super Janemba energy wave 0x080359D4
    SjWave,
    /// Dabura's dark sphere 0x0803AEA8 (radius in 1/256 px)
    DarkSphere { r256: i32 },
    /// Bio Mech grapple arm 0x0804B380
    GrappleArm { len: i32, back: bool, grabbed: bool },
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Shot {
    pub t: i64,
    pub x: i32,
    pub y: i32,
    pub owner: EntityId,
    pub dir: Dir,
    /// 0 enemy shot (hits players), 1 player shot (hits enemies), 2 hits every actor
    pub team: u8,
    pub kind: ShotKind,
}

impl Shot {
    pub fn new(owner: EntityId, dir: Dir, team: u8, x: i32, y: i32, kind: ShotKind) -> Shot {
        let mut s = Shot { t: 0, x, y, owner, dir, team, kind };
        if let ShotKind::Straight(o) = &mut s.kind {
            o.x0 = x;
            o.y0 = y;
        }
        s
    }
    pub fn px(&self) -> i32 {
        self.x >> 8
    }
    pub fn py(&self) -> i32 {
        self.y >> 8
    }
    pub fn kind_name(&self) -> &'static str {
        match &self.kind {
            ShotKind::Straight(o) => o.name.as_str(),
            ShotKind::GrowBeam(_) => "beam",
            ShotKind::FullBeam(_) => "fullbeam",
            ShotKind::AreaHit(a) => {
                if a.dragon {
                    "dragon"
                } else {
                    "area"
                }
            }
            ShotKind::BigBang(_) => "bigbang",
            ShotKind::Kame(_) => "beam",
            ShotKind::Volley { .. } => "volley",
            ShotKind::MasenkoCursor(_) => "masenko",
            ShotKind::MasenkoBall(_) => "masenkoball",
            ShotKind::Ghost(_) => "ghost",
            ShotKind::EnergyWave { .. } => "wave",
            ShotKind::Flame { .. } => "flame",
            ShotKind::Bomb { .. } => "bomb",
            ShotKind::StretchArm { .. } => "stretch",
            ShotKind::CandyBeam { .. } => "candybeam",
            ShotKind::SuperBall { .. } => "superball",
            ShotKind::SjWave => "sjwave",
            ShotKind::DarkSphere { .. } => "sphere",
            ShotKind::GrappleArm { .. } => "grapple",
        }
    }
}

pub fn add(w: &mut World, s: Shot) -> EntityId {
    w.add(Body::Shot(Box::new(s)))
}
pub fn add_straight(w: &mut World, owner: EntityId, dir: Dir, team: u8, x: i32, y: i32, o: StraightOpts) -> EntityId {
    add(w, Shot::new(owner, dir, team, x, y, ShotKind::Straight(Box::new(o))))
}

/// player / enemy ki blast class 0x080197B4 (3 px/frame): the ctor and an accepted hit pick a sound with rand(2)
#[allow(clippy::too_many_arguments)]
pub fn ki_blast(w: &mut World, owner: EntityId, dir: Dir, team: u8, x: i32, y: i32, a: AttackType, mult: i32, spawn_now: bool) -> EntityId {
    let (dx, dy) = DIR_VEC[dir as usize];
    let o = StraightOpts { vx: dx * 0x300, vy: dy * 0x300, b: [6, 6, 12, 12], a, mult, name: SName::KiBlast, fx_end: 16, rng_spawn: 2, rng_hit: 2, sfx_spawn: 20, sfx_hit: 48, ..Default::default() };
    let id = add_straight(w, owner, dir, team, x, y, o);
    if spawn_now {
        let i = w.find(id).unwrap();
        spawn_rng(w, i);
    }
    id
}
fn spawn_rng(w: &mut World, i: usize) {
    let (n, sfx) = match &mut w.shot(i).kind {
        ShotKind::Straight(o) => {
            if o.spawned {
                return;
            }
            o.spawned = true;
            (o.rng_spawn, o.sfx_spawn)
        }
        _ => return,
    };
    let r = if n > 0 { w.rng.rand(n) } else { 0 };
    if sfx >= 0 {
        w.sfx(sfx + r, 64);
    }
}

pub fn energy_wave(owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    Shot::new(owner, dir, 0, x, y, ShotKind::EnergyWave { len: 8, mode: 0 })
}
pub fn flame(w: &mut World, owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    let (dx, dy) = DIR_VEC[dir as usize];
    let vx = dx * 512 + w.rng.rand(192) - 96;
    let vy = dy * 512 + w.rng.rand(192) - 96;
    Shot::new(owner, dir, 0, x, y, ShotKind::Flame { vx, vy, done: false })
}
pub fn bomb(owner: EntityId, dir: Dir, x: i32, y: i32, tx: i32, ty: i32) -> Shot {
    let cl = |v: i32| v.clamp(-128, 128);
    let mut ddx = cl(tx - (x >> 8));
    let mut ddy = cl(ty - (y >> 8));
    if dir >= 2 {
        ddy = 0;
    } else {
        ddx = 0;
    }
    Shot::new(owner, dir, 0, x, y, ShotKind::Bomb { vx: (ddx * 0x71d) >> 8, vy: (ddy * 0x71d) >> 8, z8: -64, vz8: 16, landed: -1 })
}
pub fn stretch_arm(owner: EntityId, dir: Dir, ox: i32, oy: i32) -> Shot {
    Shot::new(owner, dir, 0, 0, 0, ShotKind::StretchArm { ox, oy, hit_done: false })
}
pub fn candy_beam(owner: EntityId, dir: Dir, ox: i32, oy: i32) -> Shot {
    Shot::new(owner, dir, 0, 0, 0, ShotKind::CandyBeam { ox, oy, done: false })
}
pub fn super_ball(owner: EntityId) -> Shot {
    Shot::new(owner, 3, 0, 0, 0, ShotKind::SuperBall { phase: 0, sx: 0, sy: 0, tx: 0, ty: 0 })
}
pub fn sj_wave(owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    Shot::new(owner, dir, 0, x, y, ShotKind::SjWave)
}
pub fn dark_sphere(w: &mut World, owner: EntityId, x: i32, y: i32) -> Shot {
    w.g.sphere_state = 0;
    Shot::new(owner, 0, 0, x, y, ShotKind::DarkSphere { r256: 256 })
}
pub fn grapple_arm(owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    Shot::new(owner, dir, 0, x, y, ShotKind::GrappleArm { len: 8, back: false, grabbed: false })
}
#[allow(clippy::too_many_arguments)]
pub fn full_beam(owner: EntityId, dir: Dir, team: u8, x: i32, y: i32, a: AttackType, mult: i32, life: i64, table: Option<u8>, vis_pos: Option<[(i32, i32); 4]>, hits: Option<(i64, i64)>) -> Shot {
    Shot::new(owner, dir, team, x << 8, y << 8, ShotKind::FullBeam(Box::new(FullBeam { len: 0, life, a, mult, width: 24, table, vis_pos, scale: 0, ending: false, vis: None, hit_win: hits, game: false, next: 0, end_t: 0 })))
}
pub fn area_hit(owner: EntityId, dir: Dir, team: u8, b: Rect, life: i64, every: i64, a: AttackType, mult: i32, grow: i32) -> Shot {
    let (cx, cy) = b.center();
    Shot::new(owner, dir, team, cx << 8, cy << 8, ShotKind::AreaHit(Box::new(AreaHit { b, life, every, a, mult, grow, first: false, dragon: false })))
}

fn muzzle_pos(w: &World, owner: EntityId, dir: Dir, m: Muzzle) -> Option<(i32, i32)> {
    let a = w.actor_by_id(owner)?;
    let (cx, cy) = a.center();
    match m {
        Muzzle::Player16 => {
            let (dx, dy) = DIR_VEC[a.facing as usize];
            Some((cx + dx * 16, cy + dy * 16 + if dx != 0 { 2 } else { 0 }))
        }
        Muzzle::KidBuu { ox, oy } => {
            let (dx, dy) = DIR_VEC[dir as usize];
            Some((cx + ox - 8 + dx * 12, cy + oy + dy * 12))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn grow_beam(w: &World, owner: EntityId, dir: Dir, team: u8, muzzle: Muzzle, a: AttackType, mult: i32, width: i32, speed256: i32, max_len256: i32, gfx: bool, vis_pos: Option<[(i32, i32); 4]>, game: bool, start_len256: i32) -> Shot {
    grow_beam_f(w, owner, dir, team, muzzle, a, mult, width, speed256 as f64 / 256.0, max_len256 as f64 / 256.0, gfx, vis_pos, game, start_len256 as f64 / 256.0)
}
#[allow(clippy::too_many_arguments)]
pub fn grow_beam_f(w: &World, owner: EntityId, dir: Dir, team: u8, muzzle: Muzzle, a: AttackType, mult: i32, width: i32, speed: f64, max_len: f64, gfx: bool, vis_pos: Option<[(i32, i32); 4]>, game: bool, start_len: f64) -> Shot {
    let (x, y) = muzzle_pos(w, owner, dir, muzzle).unwrap_or((0, 0));
    let b = GrowBeam {
        len: F(start_len),
        done: false,
        a,
        mult,
        width,
        speed: F(speed),
        max_len: F(max_len),
        muzzle,
        gfx,
        vis_pos,
        hidden: false,
        vt: 0,
        stopped: false,
        retract: None,
        next_fx: -1,
        game: game && vis_pos.is_some(),
        glen: 8,
        gstop: false,
        last: (x, y),
    };
    Shot::new(owner, dir, team, x << 8, y << 8, ShotKind::GrowBeam(Box::new(b)))
}

pub fn beam_stop(w: &mut World, i: usize) {
    match &mut w.shot(i).kind {
        ShotKind::GrowBeam(b) => b.done = true,
        ShotKind::Kame(b) => b.done = true,
        _ => {}
    }
}
pub fn beam_done(w: &World, i: usize) -> bool {
    match &w.ents[i].body {
        Body::Shot(s) => match &s.kind {
            ShotKind::GrowBeam(b) => b.done,
            ShotKind::Kame(b) => b.done,
            _ => false,
        },
        _ => false,
    }
}

// ---- shared ----------------------------------------------------------------------------------------------------

fn owner_dead(w: &World, owner: EntityId) -> bool {
    match w.find(owner) {
        None => true,
        Some(i) => w.ents[i].removed || matches!(&w.ents[i].body, Body::Enemy(e) if e.defeated),
    }
}

/// hit message from shot `i` (attacker = the owner)
pub fn hit(w: &mut World, i: usize, rect: Rect, a: AttackType, mult: i32, first: bool) -> bool {
    let s = w.shot_ref(i);
    let team = s.team;
    if owner_dead(w, s.owner) && team != 1 {
        return false;
    }
    let (owner, dir) = (s.owner, s.dir);
    let (stats, is_player) = match w.find(owner) {
        Some(oi) => (w.actor(oi).map(|a| a.stats).unwrap_or_default(), matches!(w.ents[oi].body, Body::Player(_))),
        None => (Default::default(), false),
    };
    let mut m = HitMsg { team, rect, dir, mult, atk: a, attacker: owner, att_stats: stats, att_is_player: is_player, source: w.ents[i].id, crit: false, raw: 0, dmg: 0 };
    broadcast(w, &mut m, first)
}

pub fn rect(w: &World, i: usize) -> Rect {
    let s = w.shot_ref(i);
    let (px, py) = (s.px(), s.py());
    let (dx, dy) = DIR_VEC[s.dir as usize];
    match &s.kind {
        ShotKind::Straight(o) => Rect::new(px - o.b[0], py - o.b[1], px - o.b[0] + o.b[2], py - o.b[1] + o.b[3]),
        ShotKind::GrowBeam(b) => {
            let h = b.width >> 1;
            let l = js_round(b.len.0) as i32;
            let (x1, y1) = (px + dx * l, py + dy * l);
            if dx != 0 {
                Rect::new(px.min(x1), py - h, px.max(x1), py + h)
            } else {
                Rect::new(px - h, py.min(y1), px + h, py.max(y1))
            }
        }
        ShotKind::FullBeam(b) => {
            let h = b.width >> 1;
            let (x1, y1) = (px + dx * b.len, py + dy * b.len);
            if dx != 0 {
                Rect::new(px.min(x1), py - h, px.max(x1), py + h)
            } else {
                Rect::new(px - h, py.min(y1), px + h, py.max(y1))
            }
        }
        ShotKind::AreaHit(a) => a.b,
        ShotKind::BigBang(b) => bb_rect(w, s, b),
        ShotKind::MasenkoCursor(c) => {
            let (x, y) = masenko_pos(w, s.owner, s.dir, c.d);
            centered(x, y, 16, 16)
        }
        ShotKind::MasenkoBall(_) => masenko_flight_rect(w, i),
        ShotKind::Kame(k) => {
            let (hx, hy) = kame_head(s, k.len);
            Rect::new(px.min(hx) - 6, py.min(hy) - 6, px.max(hx) + 6, py.max(hy) + 6)
        }
        ShotKind::Volley { len, .. } => {
            let (hx, hy) = (px + dx * len, py + dy * len);
            Rect::new(px.min(hx) - 2, py.min(hy) - 2, px.max(hx) + 2, py.max(hy) + 2)
        }
        ShotKind::Ghost(_) => centered(px, py, 12, 12),
        ShotKind::EnergyWave { len, .. } | ShotKind::GrappleArm { len, .. } => {
            let (hx, hy) = (px + dx * len, py + dy * len);
            Rect::new(px.min(hx) - 4, py.min(hy) - 4, px.max(hx) + 4, py.max(hy) + 4)
        }
        ShotKind::Flame { .. } | ShotKind::Bomb { .. } => centered(px, py, 16, 16),
        ShotKind::StretchArm { ox, oy, .. } => {
            let Some(a) = w.actor_by_id(s.owner) else { return Rect::default() };
            let (x, y) = (a.px() + ox, a.py() + oy);
            let t = s.t;
            let l = if t <= 7 { 4 + 4 * t as i32 } else if t == 8 { 32 } else { 0.max(32 - 4 * (t as i32 - 8)) };
            match s.dir {
                3 => Rect::new(x, y - 8, x + l, y + 8),
                2 => Rect::new(x - l, y - 8, x, y + 8),
                0 => Rect::new(x - 8, y, x + 8, y + l),
                _ => Rect::new(x - 8, y - l, x + 8, y),
            }
        }
        ShotKind::CandyBeam { ox, oy, .. } => {
            let Some(a) = w.actor_by_id(s.owner) else { return Rect::default() };
            let (x, y) = (a.px() + ox, a.py() + oy);
            let t = s.t as i32;
            let head = 40.min(8 + 4 * t);
            let tail = 0.max((t - 9) * 4);
            let (ax, ay) = (x + dx * tail, y + dy * tail);
            let (bx, by) = (x + dx * head, y + dy * head);
            Rect::new(ax.min(bx) - 3, ay.min(by) - 3, ax.max(bx) + 3, ay.max(by) + 3)
        }
        // the ball's height offset is cosmetic: the box sits at the ground position (the reference adds a float z;
        // nothing tests this box)
        ShotKind::SuperBall { .. } => centered(px, py, 16, 16),
        ShotKind::SjWave => {
            if s.dir >= 2 {
                Rect::new(px - 32, py - 8, px, py + 56)
            } else {
                Rect::new(px - 8, py - 32, px + 56, py)
            }
        }
        ShotKind::DarkSphere { r256 } => {
            let rr = (r256 + 128) >> 8;
            centered(px, py, rr * 2, rr * 2)
        }
    }
}

fn off_camera(w: &World, i: usize) -> bool {
    !w.on_any_camera(&rect(w, i))
}
/// wall test of every shot update: any 8x8 tile under the box blocked on the projectile grid
fn in_wall(w: &World, i: usize, r: &Rect) -> bool {
    if let Some(c) = &w.col {
        return !c.test_rect(Layer::Shot, r);
    }
    let b = w.arena.bounds;
    let s = w.shot_ref(i);
    s.px() < b.x0 - 8 || s.px() > b.x1 + 8 || s.py() < b.y0 - 40 || s.py() > b.y1 + 8
}
/// px from (x, y) along dir (`step` px at a time, at most `max`) before the next point is a projectile wall
fn wall_dist(w: &World, dir: Dir, x: i32, y: i32, step: i32, max: i32) -> i32 {
    let (dx, dy) = DIR_VEC[dir as usize];
    let b = w.arena.bounds;
    let mut l = 0;
    while l < max {
        let nx = x + dx * (l + step);
        let ny = y + dy * (l + step);
        let blocked = match &w.col {
            Some(c) => !c.test(Layer::Shot, nx, ny, nx + 1, ny + 1),
            None => nx < b.x0 - 16 || nx > b.x1 + 16 || ny < b.y0 - 40 || ny > b.y1 + 16,
        };
        if blocked {
            break;
        }
        l += step;
    }
    l
}

// ---- update -------------------------------------------------------------------------------------------------------------

pub fn update(w: &mut World, i: usize) {
    let kind = std::mem::discriminant(&w.shot_ref(i).kind);
    let _ = kind;
    match &w.shot_ref(i).kind {
        ShotKind::Straight(_) => straight_update(w, i),
        ShotKind::GrowBeam(_) => grow_update(w, i),
        ShotKind::FullBeam(_) => full_update(w, i),
        ShotKind::AreaHit(_) => area_update(w, i),
        ShotKind::BigBang(_) => bigbang_update(w, i),
        ShotKind::Kame(_) => kame_update(w, i),
        ShotKind::Volley { .. } => volley_update(w, i),
        ShotKind::MasenkoCursor(_) => masenko_update(w, i),
        ShotKind::MasenkoBall(_) => masenko_ball_update(w, i),
        ShotKind::Ghost(_) => ghost_update(w, i),
        ShotKind::EnergyWave { .. } => wave_update(w, i),
        ShotKind::Flame { .. } => flame_update(w, i),
        ShotKind::Bomb { .. } => bomb_update(w, i),
        ShotKind::StretchArm { .. } => {
            w.shot(i).t += 1;
            if w.shot_ref(i).t > 15 {
                w.ents[i].removed = true;
                return;
            }
            let done = matches!(w.shot_ref(i).kind, ShotKind::StretchArm { hit_done: true, .. });
            if !done {
                let r = rect(w, i);
                if hit(w, i, r, atk(0), 1024, true) {
                    if let ShotKind::StretchArm { hit_done, .. } = &mut w.shot(i).kind {
                        *hit_done = true;
                    }
                }
            }
        }
        ShotKind::CandyBeam { .. } => {
            w.shot(i).t += 1;
            if w.shot_ref(i).t > 19 {
                w.ents[i].removed = true;
                return;
            }
            let done = matches!(w.shot_ref(i).kind, ShotKind::CandyBeam { done: true, .. });
            if !done {
                let r = rect(w, i);
                if hit(w, i, r, atk(1), 0, true) {
                    if let ShotKind::CandyBeam { done, .. } = &mut w.shot(i).kind {
                        *done = true;
                    }
                    if let Some(pi) = w.player_idx(0) {
                        if !w.pl_ref(pi).dead {
                            w.g.candy_frame = w.rng.rand(3);
                            w.g.candy = 301;
                        }
                    }
                }
            }
        }
        ShotKind::SuperBall { .. } => superball_update(w, i),
        ShotKind::SjWave => {
            let (dx, dy) = DIR_VEC[w.shot_ref(i).dir as usize];
            {
                let s = w.shot(i);
                s.t += 1;
                s.x += dx * 672;
                s.y += dy * 672;
            }
            if off_camera(w, i) {
                w.ents[i].removed = true;
                return;
            }
            let r = rect(w, i);
            if in_wall(w, i, &r) || hit(w, i, r, atk(1), 1024, true) {
                w.ents[i].removed = true;
                let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
                fx::explosion(w, px, py, 0x2000);
            }
        }
        ShotKind::DarkSphere { .. } => sphere_update(w, i),
        ShotKind::GrappleArm { .. } => {
            w.shot(i).t += 1;
            let back = matches!(w.shot_ref(i).kind, ShotKind::GrappleArm { back: true, .. });
            if back {
                if let ShotKind::GrappleArm { len, .. } = &mut w.shot(i).kind {
                    *len -= 4;
                    if *len <= 0 {
                        w.ents[i].removed = true;
                    }
                }
                return;
            }
            if let ShotKind::GrappleArm { len, back, .. } = &mut w.shot(i).kind {
                *len += 4;
                if *len >= 80 {
                    *back = true;
                }
            }
            let r = rect(w, i);
            if hit(w, i, r, atk(0), 1024, true) {
                if let ShotKind::GrappleArm { back, grabbed, .. } = &mut w.shot(i).kind {
                    *back = true;
                    *grabbed = true;
                }
                let owner = w.shot_ref(i).owner;
                if let Some(pi) = w.player_idx(0) {
                    if !w.pl_ref(pi).dead {
                        player::hold(w, pi, owner, player::HoldKind::Grapple, 20);
                    }
                }
            }
        }
    }
}

fn straight_update(w: &mut World, i: usize) {
    spawn_rng(w, i);
    let o = match &w.shot_ref(i).kind {
        ShotKind::Straight(o) => **o,
        _ => return,
    };
    let t = {
        let s = w.shot(i);
        s.t += 1;
        s.t
    };
    if o.dead >= 0 {
        let nd = o.dead + 1;
        if let ShotKind::Straight(x) = &mut w.shot(i).kind {
            x.dead = nd;
        }
        if nd >= o.fx_end {
            w.ents[i].removed = true;
        }
        return;
    }
    if o.delay != 0 && t <= o.delay {
        return;
    }
    {
        let s = w.shot(i);
        s.x += o.vx;
        s.y += o.vy;
    }
    if let Some(tr) = o.trail {
        if t & 3 == 0 {
            let s = w.shot_ref(i);
            let (x, y) = ((s.x - 8 * o.vx) >> 8, (s.y - 8 * o.vy) >> 8);
            fx::fx_anim(w, tr.fx.addr(), x, y, tr.rec, tr.n, tr.rate);
        }
    }
    if o.life != 0 && t > o.life as i64 {
        return straight_end(w, i, false);
    }
    if off_camera(w, i) {
        w.ents[i].removed = true;
        return;
    }
    let r = rect(w, i);
    if in_wall(w, i, &r) {
        return straight_end(w, i, true);
    }
    let hit_p = w.shot_ref(i).team != 1;
    if hit(w, i, r, o.a, o.mult, true) {
        let rr = if o.rng_hit > 0 { w.rng.rand(o.rng_hit) } else { 0 };
        if o.sfx_hit >= 0 {
            w.sfx(o.sfx_hit + rr, 64);
        }
        if hit_p {
            match o.on_hit {
                OnHit::None => {}
                OnHit::Wrap => {
                    if let Some(pi) = w.player_idx(0) {
                        let p = w.pl(pi);
                        if p.wraps < 8 {
                            p.wraps += 1;
                        }
                    }
                }
                OnHit::Stone => {
                    let owner = w.shot_ref(i).owner;
                    if let Some(pi) = w.player_idx(0) {
                        if !w.pl_ref(pi).dead {
                            player::hold(w, pi, owner, player::HoldKind::Stone, 297);
                        }
                    }
                }
            }
        }
        straight_end(w, i, true);
    }
}
fn straight_end(w: &mut World, i: usize, explode_fx: bool) {
    let (fx_end, explode) = match &w.shot_ref(i).kind {
        ShotKind::Straight(o) => (o.fx_end, o.explode),
        _ => return,
    };
    if let ShotKind::Straight(o) = &mut w.shot(i).kind {
        o.dead = 0;
    }
    if fx_end > 0 {
        return;
    }
    if explode_fx && explode {
        let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
        fx::explosion(w, px, py, 0x2000);
    }
    w.ents[i].removed = true;
}
/// launch a delayed shot (Burning Attack) from (x, y) 24.8
pub fn launch(w: &mut World, i: usize, x: i32, y: i32) {
    let s = w.shot(i);
    s.x = x;
    s.y = y;
    if let ShotKind::Straight(o) = &mut s.kind {
        o.delay = 0;
    }
}

fn grow_visual(w: &World, i: usize) -> (i32, i32, i32) {
    let s = w.shot_ref(i);
    let ShotKind::GrowBeam(b) = &s.kind else { return (0, 0, 0) };
    if let Some(r) = b.retract {
        return r;
    }
    let (dx, dy) = DIR_VEC[s.dir as usize];
    let (ox, oy) = match (b.vis_pos, w.actor_by_id(s.owner)) {
        (Some(v), Some(a)) => (a.px() + v[s.dir as usize].0, a.py() + v[s.dir as usize].1),
        _ => (s.px(), s.py()),
    };
    if b.game {
        return (ox, oy, b.glen);
    }
    let hx = s.px() as f64 + dx as f64 * b.len.0;
    let hy = s.py() as f64 + dy as f64 * b.len.0;
    let v = js_round((hx - ox as f64) * dx as f64 + (hy - oy as f64) * dy as f64);
    (ox, oy, (v as i32).max(0))
}

fn grow_update(w: &mut World, i: usize) {
    let (dir, owner) = (w.shot_ref(i).dir, w.shot_ref(i).owner);
    let (dx, dy) = DIR_VEC[dir as usize];
    let t = {
        let s = w.shot(i);
        s.t += 1;
        if let ShotKind::GrowBeam(b) = &mut s.kind {
            b.vt += 1;
        }
        s.t
    };
    let b0 = match &w.shot_ref(i).kind {
        ShotKind::GrowBeam(b) => (**b).clone(),
        _ => return,
    };
    if b0.done {
        if b0.gfx {
            let mut r = match b0.retract {
                Some(r) => r,
                None => grow_visual(w, i),
            };
            r.0 += dx * 3;
            r.1 += dy * 3;
            r.2 -= 3;
            if let ShotKind::GrowBeam(b) = &mut w.shot(i).kind {
                b.retract = Some(r);
            }
            if r.2 < 0 {
                w.ents[i].removed = true;
            }
            return;
        }
        let nl = b0.len.0 - b0.speed.0 * 2.0;
        if let ShotKind::GrowBeam(b) = &mut w.shot(i).kind {
            b.len = F(nl);
        }
        if nl <= 0.0 {
            w.ents[i].removed = true;
        }
        return;
    }
    if b0.game {
        if let (Some(v), Some(a)) = (b0.vis_pos, w.actor_by_id(owner)) {
            let (ox, oy) = (a.px() + v[dir as usize].0, a.py() + v[dir as usize].1);
            let slot = w.find(owner).and_then(|k| match &w.ents[k].body {
                Body::Player(p) => Some(p.slot as usize),
                _ => None,
            });
            let c = w.camera(slot.unwrap_or(0));
            let hx = ox + dx * b0.glen;
            let hy = oy + dy * b0.glen;
            let inside = if dx > 0 {
                hx < c.x1 + 8
            } else if dx < 0 {
                hx > c.x0 - 8
            } else if dy > 0 {
                hy < c.y1 + 8
            } else {
                hy > c.y0 - 8
            };
            if inside && !b0.gstop {
                let wall = wall_dist(w, dir, ox, oy, 1, b0.glen + 3);
                if let ShotKind::GrowBeam(b) = &mut w.shot(i).kind {
                    if wall < b.glen + 3 {
                        b.gstop = true;
                    }
                    b.glen = (b.glen + 3).min(wall);
                }
            }
        }
    }
    let (mx, my) = muzzle_pos(w, owner, dir, b0.muzzle).unwrap_or(b0.last);
    {
        let s = w.shot(i);
        s.x = mx << 8;
        s.y = my << 8;
        if let ShotKind::GrowBeam(b) = &mut s.kind {
            b.last = (mx, my);
        }
    }
    let want = b0.max_len.0.min(b0.len.0 + b0.speed.0);
    let wall = wall_dist(w, dir, mx, my, 1, want.ceil() as i32);
    let len = want.min(wall as f64);
    if let ShotKind::GrowBeam(b) = &mut w.shot(i).kind {
        b.len = F(len);
        if (wall as f64) < want {
            b.stopped = true;
        }
    }
    let full = rect(w, i);
    const NEAR: i32 = 44;
    let cut = |from: i32, to: i32| -> Rect {
        let (ax, ay) = (mx + dx * from, my + dy * from);
        let (bx, by) = (mx + dx * to, my + dy * to);
        if dx != 0 {
            Rect::new(ax.min(bx), full.y0, ax.max(bx), full.y1)
        } else {
            Rect::new(full.x0, ay.min(by), full.x1, ay.max(by))
        }
    };
    let l = js_round(len) as i32;
    let n = 1 + (t & 1);
    let mut any = false;
    for _ in 0..n {
        if hit(w, i, cut(0, l.min(NEAR)), b0.a, b0.mult, false) {
            any = true;
        }
    }
    if l > NEAR && t % 6 != 0 && hit(w, i, cut(NEAR, l), b0.a, b0.mult, false) {
        any = true;
    }
    if any {
        grow_hit_fx(w, i);
    }
}
/// accepted hit 0x08020996: impact piece; every 3 + rand(5) ticks a sound (rand(2)) and an explosion
fn grow_hit_fx(w: &mut World, i: usize) {
    let frame = w.frame as i64;
    let next = {
        let ShotKind::GrowBeam(b) = &mut w.shot(i).kind else { return };
        b.stopped = true;
        if b.next_fx < 0 {
            b.next_fx = frame - 1;
        }
        b.next_fx
    };
    if frame - next <= 0 {
        return;
    }
    let nf = frame + w.rng.rand(5) as i64 + 3;
    if let ShotKind::GrowBeam(b) = &mut w.shot(i).kind {
        b.next_fx = nf;
    }
    let s = 48 + w.rng.rand(2);
    w.sfx(s, 64);
    let (ox, oy, l) = grow_visual(w, i);
    let (dx, dy) = DIR_VEC[w.shot_ref(i).dir as usize];
    let ex = ox + dx * (l + 4) + w.rng.rand(32) - 16;
    let ey = oy + dy * (l + 4) + w.rng.rand(32) - 16;
    let rate = 0x1000 + w.rng.rand(0x4000);
    fx::explosion(w, ex, ey, rate);
}

/// the player's Final Flash: the beam object at player + FINAL_FLASH_POS, built now (frame `built`)
#[allow(clippy::too_many_arguments)]
pub fn final_flash(owner: EntityId, dir: Dir, x: i32, y: i32, a: AttackType, mult: i32, table: u8, built: i64) -> Shot {
    let mut s = full_beam(owner, dir, 1, x, y, a, mult, 1 << 30, Some(table), Some(FINAL_FLASH_POS), None);
    if let ShotKind::FullBeam(b) = &mut s.kind {
        b.game = true;
        b.next = built;
        b.scale = 0x20;
    }
    s
}
/// update 0x0803710C: length from the wall probe (square +- 12, 8 + 8 per free square), scale +0x20 per tick, half
/// width 8 * scale >> 8, one message per tick; accepted with now - next > 0: next = now + 3 + rand(5), sound 48 +
/// rand(2). Released: no hits, scale -0x20 per tick, removed after 8 ticks
fn final_flash_update(w: &mut World, i: usize, t: i64) {
    let (dir, owner, px, py) = (w.shot_ref(i).dir, w.shot_ref(i).owner, w.shot_ref(i).px(), w.shot_ref(i).py());
    let (dx, dy) = DIR_VEC[dir as usize];
    if t == 1 {
        let mut l = 8;
        for k in 0..256 {
            let (cx, cy) = (px + dx * 8 * k, py + dy * 8 * k);
            if in_wall(w, i, &Rect::new(cx - 12, cy - 12, cx + 12, cy + 12)) {
                break;
            }
            l += 8;
        }
        let vis = w.actor_by_id(owner).map(|a| (a.px() + FINAL_FLASH_POS[dir as usize].0, a.py() + FINAL_FLASH_POS[dir as usize].1)).unwrap_or((px, py));
        if let ShotKind::FullBeam(b) = &mut w.shot(i).kind {
            b.len = l;
            b.vis = Some(vis);
        }
    }
    let b = match &w.shot_ref(i).kind {
        ShotKind::FullBeam(b) => (**b).clone(),
        _ => return,
    };
    if b.ending || t > b.life {
        let mut rm = false;
        if let ShotKind::FullBeam(x) = &mut w.shot(i).kind {
            x.ending = true;
            x.scale = 0.max(x.scale - 0x20);
            x.end_t += 1;
            rm = x.end_t >= 8;
        }
        if rm {
            w.ents[i].removed = true;
        }
        return;
    }
    let scale = if b.scale < 0x100 { b.scale + 0x20 } else { b.scale };
    if let ShotKind::FullBeam(x) = &mut w.shot(i).kind {
        x.scale = scale;
    }
    let hw = (8 * scale) >> 8;
    let (ex, ey) = (px + dx * b.len, py + dy * b.len);
    let r = Rect::new(px.min(ex) - hw, py.min(ey) - hw, px.max(ex) + hw, py.max(ey) + hw);
    if hit(w, i, r, b.a, b.mult, false) && w.frame as i64 - b.next > 0 {
        let nf = w.frame as i64 + 3 + w.rng.rand(5) as i64;
        if let ShotKind::FullBeam(x) = &mut w.shot(i).kind {
            x.next = nf;
        }
        let sid = 48 + w.rng.rand(2);
        w.sfx(sid, 64);
    }
}

fn full_update(w: &mut World, i: usize) {
    let t = {
        let s = w.shot(i);
        s.t += 1;
        s.t
    };
    if matches!(&w.shot_ref(i).kind, ShotKind::FullBeam(b) if b.game) {
        return final_flash_update(w, i, t);
    }
    let (dir, owner, px, py) = (w.shot_ref(i).dir, w.shot_ref(i).owner, w.shot_ref(i).px(), w.shot_ref(i).py());
    if t == 1 {
        let len = wall_dist(w, dir, px, py, 8, 400);
        let vp = match &w.shot_ref(i).kind {
            ShotKind::FullBeam(b) => b.vis_pos,
            _ => None,
        };
        let vis = match (vp, w.actor_by_id(owner)) {
            (Some(v), Some(a)) => (a.px() + v[dir as usize].0, a.py() + v[dir as usize].1),
            _ => (px, py),
        };
        if let ShotKind::FullBeam(b) = &mut w.shot(i).kind {
            b.len = len;
            b.vis = Some(vis);
        }
    }
    let b = match &w.shot_ref(i).kind {
        ShotKind::FullBeam(b) => (**b).clone(),
        _ => return,
    };
    if b.ending || t > b.life {
        let mut rm = false;
        if let ShotKind::FullBeam(x) = &mut w.shot(i).kind {
            x.ending = true;
            x.scale -= 0x20;
            rm = x.table.is_none() || x.scale <= 0;
        }
        if rm {
            w.ents[i].removed = true;
        }
        return;
    }
    if let ShotKind::FullBeam(x) = &mut w.shot(i).kind {
        x.scale = 0x100.min(x.scale + 0x20);
    }
    if b.hit_win.is_none_or(|(a, z)| t >= a && t <= z) {
        let r = rect(w, i);
        hit(w, i, r, b.a, b.mult, false);
    }
}
/// end a held Final Flash: life = t
pub fn full_beam_end(w: &mut World, i: usize) {
    let t = w.shot_ref(i).t;
    if let ShotKind::FullBeam(b) = &mut w.shot(i).kind {
        b.life = t;
    }
}

fn area_update(w: &mut World, i: usize) {
    let t = {
        let s = w.shot(i);
        s.t += 1;
        s.t
    };
    let a = match &w.shot_ref(i).kind {
        ShotKind::AreaHit(a) => (**a).clone(),
        _ => return,
    };
    if t > a.life {
        w.ents[i].removed = true;
        return;
    }
    let mut b = a.b;
    if a.grow != 0 && t % 2 == 0 {
        b = Rect::new(b.x0 - a.grow, b.y0 - a.grow, b.x1 + a.grow, b.y1 + a.grow);
        if let ShotKind::AreaHit(x) = &mut w.shot(i).kind {
            x.b = b;
        }
    }
    if (t - 1) % a.every == 0 {
        hit(w, i, b, a.a, a.mult, a.first);
    }
}

const BB_DEF: [(i32, i32, u32); 3] = [(4, 4, 0x0876_4e00), (6, 6, 0x0876_4e00), (7, 10, 0x0876_aae0)];
const BB_SIZE: [i32; 10] = [4, 6, 8, 12, 16, 24, 28, 32, 32, 32];
const BB_REC: [[i32; 4]; 10] = [[8, 4, 16, 16], [8, 2, 16, 16], [8, 0, 16, 16], [16, 4, 32, 32], [16, 0, 32, 32], [32, 8, 64, 64], [32, 4, 64, 64], [32, 0, 64, 64], [32, 0, 64, 64], [32, 0, 64, 64]];
/// ball position relative to the player position while charging, per facing (0x0871DD9C)
pub const BIGBANG_POS: [(i32, i32); 4] = [(2, 2), (12, -22), (-5, -6), (21, -6)];

/// Big Bang ball (class 0x08037480, update 0x08037650, launch 0x080378A0, explode 0x0803756E, handler 0x080378F4)
pub fn big_bang(owner: EntityId, dir: Dir, x: i32, y: i32, lv: i32) -> Shot {
    Shot::new(owner, dir, 1, x, y, ShotKind::BigBang(Box::new(BigBangBall { charge256: 0, launched: false, lv: lv.clamp(0, 2), u: 0, f: 0 })))
}
fn bb_cap(b: &mut BigBangBall) {
    let (lo, hi, _) = BB_DEF[b.lv as usize];
    if b.f >= lo {
        b.f = if hi == lo { lo } else { lo + ((b.u >> 2) % (hi - lo)) };
    }
}
/// the record box of the ball's current frame (falls back to the facing-down boxes)
fn bb_rect(w: &World, s: &Shot, b: &BigBangBall) -> Rect {
    let g = w.data.record(BB_DEF[b.lv as usize].2 + s.dir as u32 * 120 + 12 * b.f as u32).unwrap_or(BB_REC[(b.f as usize).min(9)]);
    Rect::new(s.px() - g[0], s.py() - g[1], s.px() - g[0] + g[2], s.py() - g[1] + g[3])
}
fn bb_around(w: &World, i: usize, sz: i32) -> Rect {
    let s = w.shot_ref(i);
    let ShotKind::BigBang(b) = &s.kind else { return Rect::default() };
    let (cx, cy) = bb_rect(w, s, b).center();
    Rect::new(cx - sz, cy - sz, cx + sz, cy + sz)
}
fn owner_slot(w: &World, owner: EntityId) -> usize {
    w.find(owner)
        .and_then(|k| match &w.ents[k].body {
            Body::Player(p) => Some(p.slot as usize),
            _ => None,
        })
        .unwrap_or(0)
}
fn bigbang_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let (dir, owner) = (w.shot_ref(i).dir, w.shot_ref(i).owner);
    let launched = matches!(&w.shot_ref(i).kind, ShotKind::BigBang(b) if b.launched);
    if !launched {
        if let ShotKind::BigBang(b) = &mut w.shot(i).kind {
            b.u += 1;
            if b.u < 16 {
                b.f = (b.u >> 3) + ((b.u >> 1) & 1);
            } else {
                if b.u == 16 {
                    b.u = 96;
                }
                b.f = (b.u >> 5) + ((b.u >> 3) & 1);
            }
            bb_cap(b);
        }
        return;
    }
    let (dx, dy) = DIR_VEC[dir as usize];
    {
        let s = w.shot(i);
        if let ShotKind::BigBang(b) = &mut s.kind {
            bb_cap(b);
        }
        s.x += dx * 128;
        s.y += dy * 128;
        if let ShotKind::BigBang(b) = &mut s.kind {
            b.u += 1;
        }
    }
    // past the camera edge on the side it flies to (0x08019B7C)
    let r = {
        let s = w.shot_ref(i);
        let ShotKind::BigBang(b) = &s.kind else { return };
        bb_rect(w, s, b)
    };
    let c = w.camera(owner_slot(w, owner));
    let gone = match dir {
        0 => r.y0 >= c.y1,
        1 => r.y1 <= c.y0,
        2 => r.x1 <= c.x0,
        _ => r.x0 >= c.x1,
    };
    if gone {
        w.ents[i].removed = true;
        w.sfx_stop(32);
        return;
    }
    let a4 = bb_around(w, i, 4);
    if in_wall(w, i, &a4) {
        return bigbang_explode(w, i);
    }
    let (u, f, charge) = match &w.shot_ref(i).kind {
        ShotKind::BigBang(b) => (b.u, b.f, b.charge256 >> 8),
        _ => return,
    };
    if u & 3 == 0 {
        let around = bb_around(w, i, BB_SIZE[(f as usize).min(9)]);
        let mult = ((charge * 0x5762 + 0xffff) >> 16) as i32;
        if hit(w, i, around, atk(13), mult, false) {
            let s = 48 + w.rng.rand(2);
            w.sfx(s, 64);
        }
    }
}
fn bigbang_explode(w: &mut World, i: usize) {
    if w.ents[i].removed {
        return;
    }
    w.ents[i].removed = true;
    w.sfx_stop(32);
    let (f, charge) = match &w.shot_ref(i).kind {
        ShotKind::BigBang(b) => (b.f, b.charge256 >> 8),
        _ => return,
    };
    let s = (BB_SIZE[(f as usize).min(9)] * 3) >> 1;
    let around = bb_around(w, i, s);
    hit(w, i, around, atk(13), (81 * charge) as i32, false);
    w.sfx(23, 64);
    let (cx, cy) = around.center();
    fx::add(w, fx::Fx::BigBangBoom { n: 60, x0: cx - s, y0: cy - s, s });
}

/// Masenko (skill 17): the cursor (ctor 0x08021594, update 0x0802163A) at the body-box centre + dir * d and the ball
/// (class 0x08021280, update 0x0802133A) charging at MASENKO_MUZZLE from the press; the B release launches the ball
pub const MASENKO_MUZZLE: [(i32, i32); 4] = [(8, -24), (8, -24), (10, -24), (6, -24)];
const MASENKO_SPARK: [i32; 3] = [9, 9, 12];
const MASENKO_N: [i32; 3] = [4, 5, 6];

pub fn masenko(owner: EntityId, dir: Dir, mult: i32, lv: i32) -> Shot {
    Shot::new(owner, dir, 1, 0, 0, ShotKind::MasenkoCursor(Box::new(MasenkoCursor { d: 0, step: 2, released: -1, mult, lv: lv.clamp(0, 2), ball: 0 })))
}
fn masenko_pos(w: &World, owner: EntityId, dir: Dir, d: i32) -> (i32, i32) {
    let (cx, cy) = w.actor_by_id(owner).map(|a| a.center()).unwrap_or((0, 0));
    let (dx, dy) = DIR_VEC[dir as usize];
    (cx + dx * d, cy + dy * d)
}
/// the B release: the ball flies (the cursor is gone)
pub fn masenko_release(w: &mut World, i: usize) {
    let t = w.shot_ref(i).t;
    let dir = w.shot_ref(i).dir;
    let (d, ball) = match &mut w.shot(i).kind {
        ShotKind::MasenkoCursor(c) => {
            if c.released >= 0 {
                return;
            }
            c.released = t;
            (c.d, c.ball)
        }
        _ => return,
    };
    w.ents[i].removed = true;
    if let Some(bi) = w.find(ball) {
        let (dx, dy) = DIR_VEC[dir as usize];
        let s = w.shot(bi);
        if dir == 1 {
            s.y -= 8 << 8;
        }
        if let ShotKind::MasenkoBall(b) = &mut s.kind {
            b.vx = (dx * d * 0x5f4) >> 8;
            b.vy = ((dy * d * 0x5f4) >> 8) - 0x200;
            b.a = 0x2000;
            b.b = 0x200;
            b.phase = MPhase::Fly;
        }
    }
}
fn masenko_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let (owner, dir) = (w.shot_ref(i).owner, w.shot_ref(i).dir);
    let c = match &w.shot_ref(i).kind {
        ShotKind::MasenkoCursor(c) => (**c).clone(),
        _ => return,
    };
    if c.ball == 0 {
        let (mx, my) = MASENKO_MUZZLE[dir as usize];
        let (opx, opy) = w.actor_by_id(owner).map(|a| (a.px(), a.py())).unwrap_or((0, 0));
        let me = w.ents[i].id;
        let ball = Shot::new(owner, dir, 1, (opx + mx) << 8, (opy + my) << 8, ShotKind::MasenkoBall(Box::new(MasenkoBall { cursor: me, lv: c.lv, phase: MPhase::Charge, vx: 0, vy: 0, a: 0, b: 0, tb: 0 })));
        let id = add(w, ball);
        if let ShotKind::MasenkoCursor(x) = &mut w.shot(i).kind {
            x.ball = id;
        }
    }
    if c.released >= 0 {
        return;
    }
    let max = if dir < 2 { 70 } else { 100 };
    let (mut d, mut step) = (c.d, c.step);
    if c.lv < 2 {
        d += step;
        if d >= max {
            step = -2;
        } else if step < 0 && d < 16 {
            step = 2;
        }
    } else {
        use crate::input::{DOWN, LEFT, RIGHT, UP};
        let held = w.input[owner_slot(w, owner)].held;
        let toward = [DOWN, UP, LEFT, RIGHT][dir as usize];
        let away = [UP, DOWN, RIGHT, LEFT][dir as usize];
        if held & toward != 0 {
            d += 2;
        } else if held & away != 0 {
            d -= 2;
        }
        d = d.clamp(16, max);
    }
    let (x, y) = masenko_pos(w, owner, dir, d);
    let s = w.shot(i);
    if let ShotKind::MasenkoCursor(x2) = &mut s.kind {
        x2.d = d;
        x2.step = step;
    }
    s.x = x << 8;
    s.y = y << 8;
}
fn masenko_flight_rect(w: &World, i: usize) -> Rect {
    let s = w.shot_ref(i);
    let ShotKind::MasenkoBall(b) = &s.kind else { return Rect::default() };
    let rec = 0x0872_1164 + 12 * (MASENKO_SPARK[b.lv as usize] + 1 + ((s.t as i32 >> 3) & 1)) as u32;
    match w.data.record(rec) {
        Some(g) => Rect::new(s.px() - g[0], s.py() - g[1], s.px() - g[0] + g[2], s.py() - g[1] + g[3]),
        None => centered(s.px(), s.py(), 16, 16),
    }
}
fn masenko_ball_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let b = match &w.shot_ref(i).kind {
        ShotKind::MasenkoBall(b) => (**b).clone(),
        _ => return,
    };
    match b.phase {
        MPhase::Charge => {
            let gone = match w.find(b.cursor) {
                None => true,
                Some(ci) => w.ents[ci].removed && matches!(&w.shot_ref(ci).kind, ShotKind::MasenkoCursor(c) if c.released < 0),
            };
            // the cursor removed without a release (the skill ended): the charging ball goes too
            let released = w.find(b.cursor).is_some_and(|ci| matches!(&w.shot_ref(ci).kind, ShotKind::MasenkoCursor(c) if c.released >= 0));
            if gone && !released {
                w.ents[i].removed = true;
            }
        }
        MPhase::Fly => {
            let mut nb = b.clone();
            {
                let s = w.shot(i);
                s.x += nb.vx;
                s.y += nb.vy;
            }
            nb.vy += 0x20;
            nb.b -= 0x20;
            nb.a += nb.b;
            let fire = nb.a < 0;
            if fire {
                nb.phase = MPhase::Blast;
                nb.tb = 0;
            }
            if let ShotKind::MasenkoBall(x) = &mut w.shot(i).kind {
                **x = nb.clone();
            }
            if fire {
                let r = masenko_flight_rect(w, i);
                let mult = match w.find(b.cursor).map(|ci| &w.shot_ref(ci).kind) {
                    Some(ShotKind::MasenkoCursor(c)) => c.mult,
                    _ => [3687, 11060, 16589][b.lv as usize],
                };
                if hit(w, i, r, atk(7), mult, false) {
                    let s = 48 + w.rng.rand(3);
                    w.sfx(s, 64);
                }
            }
        }
        MPhase::Blast => {
            let tb = b.tb + 1;
            if let ShotKind::MasenkoBall(bb) = &mut w.shot(i).kind {
                bb.tb = tb;
            }
            if (tb >> 2) >= MASENKO_N[b.lv as usize] {
                w.ents[i].removed = true;
            }
        }
    }
}

/// the player's beam (class 0x080206B8, update 0x0802084C): Kamehameha 15 / Kamekameha 16 / Electric 6 / Banshee 1 /
/// Big Bang Kamehameha 3, at the fire position, length 8 from the construction. Each firing tick: the head box
/// (head +- 8) on a projectile wall -> stopped; else the head message: accepted -> stopped + the hit effect, not
/// accepted -> len += 3 while the head box overlaps the camera. Then always the body message (position .. head + 6).
/// Released: moves 3 px per tick, len -= 3, removed once len < 0 (destructor sounds: the owner's beam_end).
pub fn kame_beam(owner: EntityId, dir: Dir, x: i32, y: i32, built: i64, a: AttackType, mult: i32, gfx: bool) -> Shot {
    Shot::new(owner, dir, 1, x << 8, y << 8, ShotKind::Kame(Box::new(KameBeam { len: 8, done: false, a, mult, gfx, vt: 0, stopped: false, next: built })))
}
fn kame_head(s: &Shot, len: i32) -> (i32, i32) {
    let (dx, dy) = DIR_VEC[s.dir as usize];
    (s.px() + dx * len, s.py() + dy * len)
}
fn kame_update(w: &mut World, i: usize) {
    let b = match &w.shot_ref(i).kind {
        ShotKind::Kame(b) => (**b).clone(),
        _ => return,
    };
    let (dir, owner) = (w.shot_ref(i).dir, w.shot_ref(i).owner);
    let (dx, dy) = DIR_VEC[dir as usize];
    {
        let s = w.shot(i);
        s.t += 1;
        if let ShotKind::Kame(k) = &mut s.kind {
            k.vt += 1;
        }
    }
    if b.done {
        let s = w.shot(i);
        s.x += dx * 3 * 256;
        s.y += dy * 3 * 256;
        let mut gone = false;
        if let ShotKind::Kame(k) = &mut s.kind {
            k.len -= 3;
            gone = k.len < 0;
        }
        if gone {
            w.ents[i].removed = true;
        }
        return;
    }
    let (hx, hy) = kame_head(w.shot_ref(i), b.len);
    let hr = Rect::new(hx - 8, hy - 8, hx + 8, hy + 8);
    if in_wall(w, i, &hr) {
        if let ShotKind::Kame(k) = &mut w.shot(i).kind {
            k.stopped = true;
        }
    } else if hit(w, i, hr, b.a, b.mult, false) {
        if let ShotKind::Kame(k) = &mut w.shot(i).kind {
            k.stopped = true;
        }
        let frame = w.frame as i64;
        if frame - b.next > 0 {
            let nf = frame + 3 + w.rng.rand(5) as i64;
            if let ShotKind::Kame(k) = &mut w.shot(i).kind {
                k.next = nf;
            }
            let sid = 48 + w.rng.rand(2);
            w.sfx(sid, 64);
            let ex = hx + dx * 4 + w.rng.rand(32) - 16;
            let ey = hy + dy * 4 + w.rng.rand(32) - 16;
            let rate = 0x1000 + w.rng.rand(0x4000);
            fx::explosion(w, ex, ey, rate);
        }
    } else if overlaps(&hr, &w.camera(owner_slot(w, owner))) {
        if let ShotKind::Kame(k) = &mut w.shot(i).kind {
            k.len += 3;
        }
    }
    let r = rect(w, i);
    hit(w, i, r, b.a, b.mult, false);
}

/// Ultra Volleyball Attack wave (Gotenks skill 23, update 0x080384C2): box origin .. origin + dir * len expanded by 2;
/// a wall or an accepted message ends it; else len += 4 up to 48, then the origin moves 4 px per tick; 240 ticks max
pub fn volley_wave(owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    Shot::new(owner, dir, 1, x << 8, y << 8, ShotKind::Volley { len: 8, done: false, n: 0 })
}
fn volley_update(w: &mut World, i: usize) {
    let dir = w.shot_ref(i).dir;
    let (dx, dy) = DIR_VEC[dir as usize];
    w.shot(i).t += 1;
    let ShotKind::Volley { done, .. } = w.shot_ref(i).kind else { return };
    if done {
        let s = w.shot(i);
        s.x += dx * 4 * 256;
        s.y += dy * 4 * 256;
        let mut gone = false;
        if let ShotKind::Volley { len, .. } = &mut s.kind {
            *len -= 4;
            gone = *len <= 0;
        }
        if gone {
            w.ents[i].removed = true;
        }
        return;
    }
    let r = rect(w, i);
    if in_wall(w, i, &r) || hit(w, i, r, atk(16), 0x400, false) {
        if let ShotKind::Volley { done, .. } = &mut w.shot(i).kind {
            *done = true;
        }
        return;
    }
    let s = w.shot(i);
    let mut mv = false;
    if let ShotKind::Volley { len, done, n } = &mut s.kind {
        if *len < 0x30 {
            *len += 4;
        } else {
            mv = true;
        }
        *n += 1;
        if *n > 0xf0 {
            *done = true;
        }
    }
    if mv {
        s.x += dx * 4 * 256;
        s.y += dy * 4 * 256;
    }
}

pub const GHOST_SPAWN: [(i32, i32); 4] = [(6, 1), (-4, -8), (-16, -6), (16, -6)];
const GHOST_SLOTS: [[(i32, i32); 3]; 4] = [[(-26, 17), (38, 17), (6, 25)], [(-36, -24), (28, -24), (-4, -32)], [(-32, -38), (-32, 26), (-40, -6)], [(32, -38), (32, 26), (40, -6)]];
pub fn ghost(owner: EntityId, dir: Dir, x: i32, y: i32, slot: i32) -> Shot {
    Shot::new(owner, dir, 1, x, y, ShotKind::Ghost(Box::new(Ghost { vx: F(0.0), vy: F(0.0), free: false, wait: 0, slot, at: 0 })))
}
fn ghost_update(w: &mut World, i: usize) {
    let t = {
        let s = w.shot(i);
        s.t += 1;
        if let ShotKind::Ghost(g) = &mut s.kind {
            g.at += 1;
        }
        s.t
    };
    let owner = w.shot_ref(i).owner;
    let g = match &w.shot_ref(i).kind {
        ShotKind::Ghost(g) => (**g).clone(),
        _ => return,
    };
    if !g.free {
        let Some(a) = w.actor_by_id(owner) else { return };
        let o = GHOST_SLOTS[a.facing as usize][g.slot.min(2) as usize];
        let tx = (a.px() + o.0) << 8;
        let ty = (a.py() + o.1) << 8;
        let s = w.shot(i);
        s.x += (tx - s.x) >> 5;
        s.y += (ty - s.y) >> 5;
        return;
    }
    if g.wait > 0 {
        if let ShotKind::Ghost(x) = &mut w.shot(i).kind {
            x.wait -= 1;
        }
        return;
    }
    let targets: Vec<usize> = w.live_enemies().into_iter().filter(|&e| w.en_ref(e).a.hittable).collect();
    let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
    if targets.is_empty() || t > 400 {
        w.ents[i].removed = true;
        let rate = 0x1000 + w.rng.rand(0x1000);
        fx::explosion(w, px, py, rate);
        return;
    }
    let mut best = targets[0];
    let mut bd = i64::MAX;
    for &e in &targets {
        let (cx, cy) = w.en_ref(e).a.center();
        let d = ((cx - px) as i64).pow(2) + ((cy - py) as i64).pow(2);
        if d < bd {
            bd = d;
            best = e;
        }
    }
    let (cx, cy) = w.en_ref(best).a.center();
    let (dx, dy) = ((cx - px) as f64, (cy - py) as f64);
    let m = js_hypot(dx, dy).max(1.0);
    let vx = (g.vx.0 * 7.0 + (dx / m) * 768.0) / 8.0;
    let vy = (g.vy.0 * 7.0 + (dy / m) * 768.0) / 8.0;
    {
        let s = w.shot(i);
        s.x += js_round(vx) as i32;
        s.y += js_round(vy) as i32;
        if let ShotKind::Ghost(x) = &mut s.kind {
            x.vx = F(vx);
            x.vy = F(vy);
        }
    }
    let r = rect(w, i);
    if overlaps(&w.en_ref(best).a.body_box(), &r) {
        w.ents[i].removed = true;
        let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
        let rate = 0x1000 + w.rng.rand(0x1000);
        fx::explosion(w, px, py, rate);
        hit(w, i, centered(px, py, 32, 32), atk(1), 2048, false);
    }
}

fn wave_update(w: &mut World, i: usize) {
    let dir = w.shot_ref(i).dir;
    let (dx, dy) = DIR_VEC[dir as usize];
    w.shot(i).t += 1;
    let t = w.shot_ref(i).t;
    let (len, mode) = match w.shot_ref(i).kind {
        ShotKind::EnergyWave { len, mode } => (len, mode),
        _ => return,
    };
    if mode == 1 {
        let s = w.shot(i);
        s.x += dx * 4 * 256;
        s.y += dy * 4 * 256;
        if let ShotKind::EnergyWave { len, .. } = &mut s.kind {
            *len -= 4;
            if *len <= 0 {
                w.ents[i].removed = true;
            }
        }
        return;
    }
    {
        let s = w.shot(i);
        if len < 48 {
            if let ShotKind::EnergyWave { len, .. } = &mut s.kind {
                *len += 4;
            }
        } else {
            s.x += dx * 4 * 256;
            s.y += dy * 4 * 256;
        }
    }
    let len = match w.shot_ref(i).kind {
        ShotKind::EnergyWave { len, .. } => len,
        _ => 0,
    };
    let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
    let (hx, hy) = (px + dx * len, py + dy * len);
    let set_mode = |w: &mut World| {
        if let ShotKind::EnergyWave { mode, .. } = &mut w.shot(i).kind {
            *mode = 1;
        }
    };
    if t > 240 || in_wall(w, i, &Rect::new(hx - 4, hy - 4, hx + 4, hy + 4)) || off_camera(w, i) {
        set_mode(w);
        return;
    }
    let r = rect(w, i);
    if hit(w, i, r, atk(1), 1024, true) {
        fx::puff_rng(w, hx, hy);
        set_mode(w);
    }
}

fn flame_update(w: &mut World, i: usize) {
    let (vx, vy, done) = match w.shot_ref(i).kind {
        ShotKind::Flame { vx, vy, done } => (vx, vy, done),
        _ => return,
    };
    {
        let s = w.shot(i);
        s.t += 1;
        s.x += vx;
        s.y += vy;
    }
    if w.shot_ref(i).t >= 20 {
        w.ents[i].removed = true;
        return;
    }
    if !done {
        let r = rect(w, i);
        if hit(w, i, r, atk(4), 51, true) {
            if let ShotKind::Flame { done, .. } = &mut w.shot(i).kind {
                *done = true;
            }
        }
    }
}

fn bomb_update(w: &mut World, i: usize) {
    let t = {
        let s = w.shot(i);
        s.t += 1;
        s.t
    };
    let ShotKind::Bomb { vx, vy, landed, .. } = w.shot_ref(i).kind else { return };
    if landed < 0 {
        let s = w.shot(i);
        s.x += vx;
        s.y += vy;
        if let ShotKind::Bomb { z8, vz8, landed, .. } = &mut s.kind {
            *z8 -= *vz8;
            *vz8 -= 1;
            if *z8 >= 0 || t >= 40 {
                *z8 = 0;
                *landed = t;
            }
        }
        return;
    }
    if t - landed >= 30 {
        w.ents[i].removed = true;
        let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
        for k in 0..4 {
            let (dvx, dvy) = [(96, 0), (-96, 0), (0, 64), (0, -64)][k];
            fx::add(w, fx::Fx::Debris { x: (px - 4) << 8, y: (py - 4) << 8, vx: dvx, vy: dvy, t: 0 });
        }
        let rate = 0x1000 + w.rng.rand(0x1000);
        fx::explosion(w, px, py, rate);
        let r = rect(w, i);
        hit(w, i, r, atk(1), 1024, true);
    }
}

fn superball_update(w: &mut World, i: usize) {
    let t = {
        let s = w.shot(i);
        s.t += 1;
        s.t
    };
    let owner = w.shot_ref(i).owner;
    let ShotKind::SuperBall { phase, sx, sy, tx, ty } = w.shot_ref(i).kind else { return };
    if phase == 0 {
        if let Some(a) = w.actor_by_id(owner) {
            let (x, y) = ((a.px() + 8) << 8, a.py() << 8);
            let s = w.shot(i);
            s.x = x;
            s.y = y;
        }
        if t >= 60 {
            let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
            let (tx, ty) = match w.player_idx(0) {
                Some(pi) => w.pl_ref(pi).a.foot_center(),
                None => (px, py),
            };
            let s = w.shot(i);
            let (x, y) = (s.x, s.y);
            s.kind = ShotKind::SuperBall { phase: 1, sx: x, sy: y, tx: tx << 8, ty: ty << 8 };
            s.t = 0;
        }
        return;
    }
    if phase == 1 {
        let k = t.min(43);
        let x = crate::fixed::round_div(sx as i64 * 43 + (tx - sx) as i64 * k, 43) as i32;
        let y = crate::fixed::round_div(sy as i64 * 43 + (ty - sy) as i64 * k, 43) as i32;
        {
            let s = w.shot(i);
            s.x = x;
            s.y = y;
        }
        if t >= 43 {
            w.ents[i].removed = true;
            let (px, py) = (x >> 8, y >> 8);
            fx::fx_anim(w, 0x0872_a5dc, px, py, 9, 6, 0x4000);
            add(w, area_hit(owner, 0, 0, centered(px, py, 32, 32), 16, 100, atk(7), 16589, 2));
        }
    }
}

fn sphere_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let owner = w.shot_ref(i).owner;
    if owner_dead(w, owner) {
        w.ents[i].removed = true;
        return;
    }
    let ShotKind::DarkSphere { mut r256 } = w.shot_ref(i).kind else { return };
    if w.g.sphere_state == 0 {
        r256 = (64 * 256).min(r256 + 51);
        if r256 >= 64 * 256 {
            w.g.sphere_state = 2;
        }
    } else if w.g.sphere_state == 2 {
        r256 += 128;
        if r256 >= 128 * 256 {
            w.g.sphere_state = 3;
            w.ents[i].removed = true;
            w.shot(i).kind = ShotKind::DarkSphere { r256 };
            return;
        }
    }
    w.shot(i).kind = ShotKind::DarkSphere { r256 };
    if let Some(pi) = w.player_idx(0) {
        if !w.pl_ref(pi).dead {
            let (cx, cy) = w.pl_ref(pi).a.center();
            let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
            let d2 = ((cx - px) as i64).pow(2) + ((cy - py) as i64).pow(2);
            if d2 * 65536 <= (r256 as i64) * (r256 as i64) {
                let bb = w.pl_ref(pi).a.body_box();
                hit(w, i, bb, atk(2), 1024, true);
            }
        }
    }
}

// ---- receivers -----------------------------------------------------------------------------------------------------------

pub fn on_message(w: &mut World, i: usize, msg: &mut HitMsg) -> bool {
    let me = w.ents[i].id;
    match &w.shot_ref(i).kind {
        // an Energy Blast message (attack sub 4, another object) overlapping the ball's box and its centre +- size*3/4
        ShotKind::BigBang(b) => {
            if !b.launched || msg.atk.sub != 4 || msg.source == me {
                return false;
            }
            let sz = (BB_SIZE[(b.f as usize).min(9)] * 3) >> 2;
            if !overlaps(&msg.rect, &rect(w, i)) || !overlaps(&msg.rect, &bb_around(w, i, sz)) {
                return false;
            }
            bigbang_explode(w, i);
            true
        }
        ShotKind::Straight(o) => {
            if o.fragile && o.dead < 0 && msg.team == 1 && msg.atk.kind == 0 && msg.source != me && overlaps(&msg.rect, &rect(w, i)) {
                w.ents[i].removed = true;
                let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
                fx::explosion(w, px, py, 0x2000);
            }
            false
        }
        ShotKind::EnergyWave { len, mode } => {
            if msg.team != 1 || *mode != 0 {
                return false;
            }
            let s = w.shot_ref(i);
            let (dx, dy) = DIR_VEC[s.dir as usize];
            let (hx, hy) = (s.px() + dx * len, s.py() + dy * len);
            if msg.dir == (s.dir ^ 1) && overlaps(&msg.rect, &Rect::new(hx - 12, hy - 12, hx + 12, hy + 12)) {
                if let ShotKind::EnergyWave { mode, .. } = &mut w.shot(i).kind {
                    *mode = 1;
                }
                fx::puff_rng(w, hx, hy);
            }
            false
        }
        ShotKind::DarkSphere { r256 } => {
            if msg.team != 1 || msg.atk.kind != 1 || w.g.sphere_state != 0 {
                return false;
            }
            if !overlaps(&msg.rect, &rect(w, i)) {
                return false;
            }
            let nr = r256 - 102;
            w.shot(i).kind = ShotKind::DarkSphere { r256: nr };
            if nr < 256 {
                w.g.sphere_state = 1;
                w.ents[i].removed = true;
                let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
                for k in 0..4 {
                    fx::puff_rng(w, px + if k & 1 != 0 { 50 } else { -50 }, py + if k & 2 != 0 { 50 } else { -50 });
                }
            }
            false
        }
        _ => false,
    }
}
