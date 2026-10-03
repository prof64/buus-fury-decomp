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
    /// Dabura's spit: the player's stone state (vtable 0x0804AB34)
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
    /// the sound of the shot's destroy function (wall, accepted hit, shot down), before its explosion (rifle bullet
    /// 0x0802770C: 46)
    pub sfx_end: i32,
    pub sector: i32,
    // state
    pub dead: i32,
    pub spawned: bool,
    pub x0: i32,
    pub y0: i32,
    /// the box is the current record's (the player's blast 0x0804B454: obj+0x18 advances with its frame, the hit
    /// box shrinks to 8 x 16 / 16 x 8 from tick 16; 2-10: a blast passing a criminal's edge misses)
    #[serde(default)]
    pub rec_box: bool,
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
            sfx_end: -1,
            sector: 0,
            dead: -1,
            spawned: false,
            x0: 0,
            y0: 0,
            rec_box: false,
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
    /// charging (state 0, before `kame_fire`): no update
    pub charging: bool,
    /// the object plays its own fire / end sounds (enemy use)
    pub sounds: bool,
}

/// Kamikaze ghost (skill 13): an actor (sprite 148) with its own animation state and AI (projectiles.ts Ghost)
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Ghost {
    pub free: bool,
    /// frames between the release and the AI state 0x11B running
    pub wait: i32,
    /// formation slot (0..2, table 0x0871E024)
    pub slot: i32,
    /// the actor's animation (raw / frame / tick) under its state rule
    pub ea: crate::eanim::EnemyAnim,
    pub facing: Dir,
    pub boom: bool,
}

/// mummy bandage (builder 0x0802D79C, update 0x0802D940, messages 0x0802DB10)
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct MummyWrap {
    pub vx: i32,
    pub vy: i32,
    pub diag: bool,
    pub tick: i32,
    pub stuck: bool,
    pub delay: i32,
    pub off_x: i32,
    pub off_y: i32,
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
    /// mummy bandage 0x0802D79C
    Wrap(Box<MummyWrap>),
    /// energy wave 0x08049C98: segment tail..head, grows 4/frame to 48 then moves 4 px/frame (`life` +0x1C)
    EnergyWave { len: i32, mode: i32, life: i32 },
    /// flame 0x0804BFCC: 16x16, 20 frames, one hit
    Flame { vx: i32, vy: i32, done: bool },
    /// bomb 0x0804A2E8: thrown in a screen-space arc (40 moves), explodes at counter 71
    Bomb { vx: i32, vy: i32, landed: i64 },
    /// Buu stretch arm 0x08028C04 (origin = the position; phase 0 extend, 1 retract)
    StretchArm { len: i32, phase: i32 },
    /// Majin Buu candy beam 0x080286D4 (origin = the position; phase 0 grow / slide, 1 retract)
    CandyBeam { len: i32, phase: i32, cnt: i32 },
    /// Super Buu lobbed ball 0x08021280
    SuperBall { phase: i32, sx: i32, sy: i32, tx: i32, ty: i32 },
    /// Super Janemba energy wave 0x080359D4
    SjWave,
    /// Dabura's dark sphere 0x0803AEA8 (radius in 1/256 px)
    DarkSphere { r256: i32 },
    /// Bio Mech grapple arm 0x0804B380 (phase 0 extend, 1 retract, 2 reel the player in; (gx, gy) the grip, px)
    GrappleArm { len: i32, phase: i32, gx: i32, gy: i32 },
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
            ShotKind::Wrap(_) => "wrap",
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
    // the projectile base ctor `0x08019B04` takes the class's object tiles (vram.rs): the ki blast 0x080197B4
    // 0x100, the lobbed ball 0x08021280 0x1000, the flame / energy shot 0x080246B4 0x40 (VERIFIED sizes; other
    // classes not measured yet)
    let tiles = match &s.kind {
        ShotKind::Straight(o) if o.name == SName::KiBlast => 0x100,
        ShotKind::SuperBall { .. } | ShotKind::MasenkoBall(_) => 0x1000,
        ShotKind::Flame { .. } => 0x40,
        // the beam class `0x080206B8` (segment sb 19-3: Kid Buu's beam)
        ShotKind::Kame(_) => 0x300,
        _ => 0,
    };
    let id = w.add(Body::Shot(Box::new(s)));
    crate::vram::acquire(w, crate::vram::Owner::Ent(id), tiles);
    id
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
/// the blast's box (vtable 0x0804B454: the record at obj+0x18, 0x0871370C + 12 * (facing * 4 + frame), frame t >> 3 =
/// 0, 1, 2, 3, then 2, 3): 16 x 16 for frames 0-1, then 8 x 16 (down / up) or 16 x 8 (left / right)
pub fn ki_blast_box(dir: i32, t: i32) -> [i32; 4] {
    if t >> 3 < 2 {
        [8, 8, 16, 16]
    } else if dir < 2 {
        [4, 8, 8, 16]
    } else {
        [8, 4, 16, 8]
    }
}
pub fn spawn_rng(w: &mut World, i: usize) {
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

/// enemy energy wave 0x0802A2EC (Cyborg / turret / soldier): the ctor plays sound 35
pub fn energy_wave(w: &mut World, owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    w.sfx(35, 64);
    Shot::new(owner, dir, 0, x, y, ShotKind::EnergyWave { len: 8, mode: 0, life: 0 })
}
/// bandage velocities (24.8 per frame) by facing, straight and diagonal (0x08756B74)
const WRAP_VEL: [[i32; 4]; 4] = [[0, 448, -316, 316], [0, -448, 316, -316], [-448, 0, -316, -316], [448, 0, 316, 316]];
/// a stuck bandage's offset above the player's position by slot (0x08756BB4)
const WRAP_SLOT_Y: [i32; 8] = [-2, -10, 2, 18, 6, 10, -6, 14];
pub fn mummy_wrap(owner: EntityId, dir: Dir, x: i32, y: i32, diag: bool) -> Shot {
    let v = WRAP_VEL[dir as usize];
    let (vx, vy) = if diag { (v[2], v[3]) } else { (v[0], v[1]) };
    Shot::new(owner, dir, 0, x, y, ShotKind::Wrap(Box::new(MummyWrap { vx, vy, diag, tick: 0, stuck: false, delay: 0, off_x: 0, off_y: 0 })))
}
pub fn flame(w: &mut World, owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    let (dx, dy) = DIR_VEC[dir as usize];
    let vx = dx * 512 + w.rng.rand(192) - 96;
    let vy = dy * 512 + w.rng.rand(192) - 96;
    Shot::new(owner, dir, 0, x, y, ShotKind::Flame { vx, vy, done: false })
}
/// (ddx, ddy): the thrower's aim offset (px, clamped, one axis; BombThrow)
pub fn bomb(owner: EntityId, dir: Dir, x: i32, y: i32, ddx: i32, ddy: i32) -> Shot {
    Shot::new(owner, dir, 0, x, y, ShotKind::Bomb { vx: (ddx * 0x71d) >> 8, vy: ((ddy * 0x71d) >> 8) - 0x200, landed: -1 })
}
/// Buu stretch arm (class 0x0804CDD4, builder 0x08028C04) at the integer position (x, y)
pub fn stretch_arm(owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    Shot::new(owner, dir, 0, x << 8, y << 8, ShotKind::StretchArm { len: 8, phase: 0 })
}
/// Majin Buu candy beam (class 0x0804B3AC, builder 0x080286D4) at the integer position (x, y)
pub fn candy_beam(owner: EntityId, dir: Dir, x: i32, y: i32) -> Shot {
    Shot::new(owner, dir, 0, x << 8, y << 8, ShotKind::CandyBeam { len: 8, phase: 0, cnt: 0 })
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
    Shot::new(owner, dir, 0, x, y, ShotKind::GrappleArm { len: 8, phase: 0, gx: 0, gy: 0 })
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
    let mut gone = None;
    match &mut w.shot(i).kind {
        ShotKind::GrowBeam(b) => b.done = true,
        // 0x0802082A: still charging -> removed (destructor: stop 3); firing -> retracts
        ShotKind::Kame(b) => {
            if b.charging {
                gone = Some(b.sounds);
            } else {
                b.done = true;
            }
        }
        _ => {}
    }
    if let Some(sounds) = gone {
        w.ents[i].removed = true;
        if sounds {
            w.sfx_stop(3);
        }
    }
}
/// 0x080207D2: an enemy beam fires (stop 3, sound 32)
pub fn kame_fire(w: &mut World, i: usize) {
    let fire = match &mut w.shot(i).kind {
        ShotKind::Kame(b) if b.charging => {
            b.charging = false;
            true
        }
        _ => false,
    };
    if fire {
        w.sfx_stop(3);
        w.sfx(32, 64);
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
    let mut m = HitMsg { team, rect, dir, mult, atk: a, attacker: owner, att_stats: stats, att_is_player: is_player, source: w.ents[i].id, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
    broadcast(w, &mut m, first)
}

pub fn rect(w: &World, i: usize) -> Rect {
    let s = w.shot_ref(i);
    let (px, py) = (s.px(), s.py());
    let (dx, dy) = DIR_VEC[s.dir as usize];
    match &s.kind {
        ShotKind::Straight(o) if o.rec_box => {
            let b = ki_blast_box(s.dir as i32, s.t as i32);
            Rect::new(px - b[0], py - b[1], px - b[0] + b[2], py - b[1] + b[3])
        }
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
        ShotKind::Ghost(g) => {
            let b = w.data.sprite(GHOST_SPRITE).and_then(|sp| sp.frame_box(g.ea.pos.raw, g.facing, g.ea.pos.frame));
            match b {
                Some(b) => Rect::new(px + b[0], py + b[1], px + b[0] + b[2], py + b[1] + b[3]),
                None => centered(px, py, 12, 12),
            }
        }
        // the record's box: straight vertical 16 x 8, horizontal 8 x 16, diagonal 16 x 16; stuck: 8 x 16
        ShotKind::Wrap(m) => {
            let [ox, oy, ww, hh] = if m.diag {
                [8, 8, 16, 16]
            } else if m.stuck || s.dir >= 2 {
                [4, 8, 8, 16]
            } else {
                [8, 4, 16, 8]
            };
            Rect::new(px - ox, py - oy, px - ox + ww, py - oy + hh)
        }
        ShotKind::EnergyWave { len, .. } => {
            let (hx, hy) = (px + dx * len, py + dy * len);
            Rect::new(px.min(hx) - 4, py.min(hy) - 4, px.max(hx) + 4, py.max(hy) + 4)
        }
        ShotKind::GrappleArm { len, .. } => {
            let (hx, hy) = (px + dx * len, py + dy * len);
            Rect::new(hx, hy - 12, hx + 32, hy + 12)
        }
        ShotKind::Flame { .. } | ShotKind::Bomb { .. } => centered(px, py, 16, 16),
        ShotKind::StretchArm { len, .. } | ShotKind::CandyBeam { len, .. } => {
            let (hx, hy) = (px + dx * len, py + dy * len);
            Rect::new(hx - 8, hy - 8, hx + 8, hy + 8)
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
        ShotKind::StretchArm { .. } => stretch_update(w, i),
        ShotKind::CandyBeam { .. } => candy_update(w, i),
        ShotKind::Wrap(_) => wrap_update(w, i),
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
        ShotKind::GrappleArm { .. } => grapple_update(w, i),
    }
}

fn set_kind(w: &mut World, i: usize, k: ShotKind) {
    w.shot(i).kind = k;
}
fn shot_head(w: &World, i: usize, len: i32) -> (i32, i32) {
    let s = w.shot_ref(i);
    let (dx, dy) = DIR_VEC[s.dir as usize];
    (s.px() + dx * len, s.py() + dy * len)
}

/// Buu stretch arm (update 0x08028CF5): phase 0: box (head +- 8); a projectile wall under it or an accepted kind-0
/// message (melee 0x086B8088, x1.0) -> 1; else len < 80 -> + 4, else 1. Phase 1: len - 4, <= 0 -> removed
fn stretch_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let ShotKind::StretchArm { len, phase } = w.shot_ref(i).kind else { return };
    if phase == 1 {
        let l = len - 4;
        set_kind(w, i, ShotKind::StretchArm { len: l, phase });
        if l <= 0 {
            w.ents[i].removed = true;
        }
        return;
    }
    let r = rect(w, i);
    if in_wall(w, i, &r) || hit(w, i, r, atk(0), 1024, false) {
        set_kind(w, i, ShotKind::StretchArm { len, phase: 1 });
        return;
    }
    set_kind(w, i, if len < 80 { ShotKind::StretchArm { len: len + 4, phase } } else { ShotKind::StretchArm { len, phase: 1 } });
}

/// Majin Buu candy beam (update 0x080287AD): phase 0: box (head +- 8); a projectile wall -> 1; an accepted kind-0
/// message (energy 0x086B8090, x0) -> the player turns into candy and 1; else len < 64 -> + 4, else the origin moves
/// 4 px along; counter + 1 > 240 -> 1. Phase 1: the origin moves 4 px along, len - 4; <= 0 -> removed
fn candy_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let ShotKind::CandyBeam { len, phase, cnt } = w.shot_ref(i).kind else { return };
    let (dx, dy) = DIR_VEC[w.shot_ref(i).dir as usize];
    let advance = |w: &mut World| {
        let s = w.shot(i);
        s.x += (dx * 4) << 8;
        s.y += (dy * 4) << 8;
    };
    if phase == 1 {
        advance(w);
        let l = len - 4;
        set_kind(w, i, ShotKind::CandyBeam { len: l, phase, cnt });
        if l <= 0 {
            w.ents[i].removed = true;
        }
        return;
    }
    let r = rect(w, i);
    if in_wall(w, i, &r) {
        set_kind(w, i, ShotKind::CandyBeam { len, phase: 1, cnt });
        return;
    }
    let owner = w.shot_ref(i).owner;
    if !owner_dead(w, owner) && hit(w, i, r, atk(1), 0, false) {
        if let Some(pi) = target_player(w, owner) {
            player::candy_hit(w, pi);
        }
        set_kind(w, i, ShotKind::CandyBeam { len, phase: 1, cnt });
        return;
    }
    let mut l = len;
    if len < 64 {
        l += 4;
    } else {
        advance(w);
    }
    let c = cnt + 1;
    set_kind(w, i, ShotKind::CandyBeam { len: l, phase: if c > 240 { 1 } else { 0 }, cnt: c });
}

/// the player an enemy-owned object plays against (its owner's target; else slot 0)
fn target_player(w: &World, owner: EntityId) -> Option<usize> {
    match w.find(owner) {
        Some(oi) if matches!(w.ents[oi].body, Body::Enemy(_)) => crate::enemy::ctx_player(w, oi),
        _ => w.player_idx(0),
    }
}

/// Bio Mech grapple arm (update 0x08037FB4): phase 0 extend: box (hx, hy - 12)..(hx + 32, hy + 12); a projectile wall
/// -> 1; a kind-0 message (melee x1.0): not accepted -> len + 4 (>= 80 -> 1); accepted -> HP <= 0 -> 1, else the grip
/// = head + 0x0876CD08[dir]; a front block -> 1; the grip fits -> the player held there (hold state, frame kept),
/// phase 2; else 1. Phase 2 reel: grip -= dir * 4: fits -> the player there, else the player SetAnim(0) and 1; then
/// like 1. Phase 1 retract: len - 4; <= 0 -> (phase 2: the player SetAnim(0)) removed
fn grapple_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let ShotKind::GrappleArm { len, phase, gx, gy } = w.shot_ref(i).kind else { return };
    let (dir, owner) = (w.shot_ref(i).dir, w.shot_ref(i).owner);
    let (dx, dy) = DIR_VEC[dir as usize];
    let pi = target_player(w, owner);
    if phase == 0 {
        let r = rect(w, i);
        if in_wall(w, i, &r) {
            set_kind(w, i, ShotKind::GrappleArm { len, phase: 1, gx, gy });
            return;
        }
        if !hit(w, i, r, atk(0), 1024, false) {
            set_kind(w, i, if len >= 80 { ShotKind::GrappleArm { len, phase: 1, gx, gy } } else { ShotKind::GrappleArm { len: len + 4, phase, gx, gy } });
            return;
        }
        let Some(pi) = pi.filter(|&pi| w.pl_ref(pi).hp() > 0) else {
            set_kind(w, i, ShotKind::GrappleArm { len, phase: 1, gx, gy });
            return;
        };
        let (hx, hy) = shot_head(w, i, len);
        const G: [(i32, i32); 4] = [(8, 20), (8, 0), (-8, 16), (0, 16)];
        let (ngx, ngy) = (hx + G[dir as usize].0, hy + G[dir as usize].1);
        // a front block (blocking flag and facing the arm) stops the grab
        let p = w.pl_ref(pi);
        if p.a.blocking && p.a.facing == (dir ^ 1) {
            set_kind(w, i, ShotKind::GrappleArm { len, phase: 1, gx: ngx, gy: ngy });
            return;
        }
        if player::fits(w, pi, ngx, ngy) {
            player::enter_hold(w, pi);
            let p = w.pl(pi);
            p.a.x = ngx << 8;
            p.a.y = ngy << 8;
            set_kind(w, i, ShotKind::GrappleArm { len, phase: 2, gx: ngx, gy: ngy });
        } else {
            set_kind(w, i, ShotKind::GrappleArm { len, phase: 1, gx: ngx, gy: ngy });
        }
        return;
    }
    let (mut ph, mut gx, mut gy) = (phase, gx, gy);
    if ph == 2 {
        if let Some(pi) = pi {
            gx -= dx * 4;
            gy -= dy * 4;
            if player::fits(w, pi, gx, gy) {
                let p = w.pl(pi);
                p.a.x = gx << 8;
                p.a.y = gy << 8;
            } else {
                player::to_idle(w, pi);
                ph = 1;
            }
        }
    }
    let l = len - 4;
    set_kind(w, i, ShotKind::GrappleArm { len: l, phase: ph, gx, gy });
    if l > 0 {
        return;
    }
    if ph == 2 {
        if let Some(pi) = pi {
            player::to_idle(w, pi);
        }
    }
    w.ents[i].removed = true;
}

/// the bandage (update 0x0802D940): every update first reapplies the player's stats (0x0802D8E6, when hittable:
/// resists 128, walk / dash -48 per stuck bandage). Flying: pos += v, a 4-frame stick delay counting down; off camera
/// or in a projectile wall -> removed; else a hit message (energy sub 1, x1.0): accepted -> overlapping the player's
/// body: sound 48 + rand(2) and with < 8 stuck the delay, else it bursts. Stuck: follows the player; 8 d-pad presses
/// within 60 frames (one now) -> removed
fn wrap_update(w: &mut World, i: usize) {
    let owner = w.shot_ref(i).owner;
    let pi = target_player(w, owner);
    wrap_reapply(w, pi);
    let ShotKind::Wrap(m) = &w.shot_ref(i).kind else { return };
    let mut m = (**m).clone();
    if m.stuck {
        if let Some(pi) = pi {
            let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
            let s = w.shot(i);
            s.x = px - m.off_x;
            s.y = py - m.off_y;
        }
        let slot = pi.map(|pi| w.pl_ref(pi).slot as usize).unwrap_or(0);
        if w.g.press_run(slot, 0xf0, 8, 60, w.frame) {
            w.ents[i].removed = true;
            wrap_reapply(w, pi);
            return;
        }
    } else {
        let s = w.shot(i);
        s.x += m.vx;
        s.y += m.vy;
        m.tick += 1;
        if m.tick == 24 {
            m.tick = 0;
        }
    }
    if m.delay != 0 {
        m.delay -= 1;
        if m.delay == 0 {
            // 0x0802D826: at the player's position + (4, -slotY[slot]), slot = 0x030030BC (then + 1 & 7)
            if let Some(pi) = pi {
                let slot = w.g.wrap_slot;
                w.g.wrap_slot = (slot + 1) & 7;
                let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
                let (x, y) = (px + 0x400, py - (WRAP_SLOT_Y[slot as usize & 7] << 8));
                let s = w.shot(i);
                s.x = x;
                s.y = y;
                m.stuck = true;
                m.off_x = px - x;
                m.off_y = py - y;
            }
        }
    }
    let (stuck, delay) = (m.stuck, m.delay);
    w.shot(i).kind = ShotKind::Wrap(Box::new(m));
    if stuck || delay > 0 {
        return;
    }
    let r = rect(w, i);
    if off_camera(w, i) || in_wall(w, i, &r) {
        w.ents[i].removed = true;
        return;
    }
    if !hit(w, i, r, atk(1), 1024, true) {
        return;
    }
    let on_body = pi.is_some_and(|pi| overlaps(&rect(w, i), &w.pl_ref(pi).a.body_box()));
    if on_body {
        let s = 48 + w.rng.rand(2);
        w.sfx(s, 64);
        if wrap_stuck_count(w) < 8 {
            if let ShotKind::Wrap(m) = &mut w.shot(i).kind {
                m.delay = 4;
            }
        } else {
            wrap_burst(w, i);
        }
    } else {
        wrap_burst(w, i);
    }
}
fn wrap_stuck_count(w: &World) -> i32 {
    w.ents.iter().filter(|e| !e.removed && matches!(&e.body, Body::Shot(s) if matches!(&s.kind, ShotKind::Wrap(m) if m.stuck))).count() as i32
}
/// 0x0802D8E6: the form controller's stat reset (resists 128) and walk / dash -48 per stuck bandage (while hittable)
fn wrap_reapply(w: &mut World, pi: Option<usize>) {
    let Some(pi) = pi else { return };
    if !w.pl_ref(pi).a.hittable {
        return;
    }
    let n = wrap_stuck_count(w);
    player::wrap_stats(w, pi, n);
}
/// 0x0802D89A: removed + explosion 0x08712FDC rate 0x2000
fn wrap_burst(w: &mut World, i: usize) {
    w.ents[i].removed = true;
    let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
    fx::explosion(w, px, py, 0x2000);
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
    // field: while an event holds the field an enemy's shot flies on (its trail too) but sends no hit message
    // (Thieves Den 9-4: a Hessian's rocket crosses the player during the level-up popup; the player's handler
    // 0x08010DDE gets no kind-0 message in those frames)
    if hit_p && w.field.as_ref().is_some_and(|f| f.busy) {
        return;
    }
    if hit(w, i, r, o.a, o.mult, true) {
        let rr = if o.rng_hit > 0 { w.rng.rand(o.rng_hit) } else { 0 };
        if o.sfx_hit >= 0 {
            w.sfx(o.sfx_hit + rr, 64);
        }
        if hit_p {
            match o.on_hit {
                OnHit::None => {}
                OnHit::Stone => {
                    let owner = w.shot_ref(i).owner;
                    if let Some(pi) = target_player(w, owner) {
                        player::petrify(w, pi);
                    }
                }
            }
        }
        straight_end(w, i, true);
    }
}
fn straight_end(w: &mut World, i: usize, explode_fx: bool) {
    let (fx_end, explode, sfx_end) = match &w.shot_ref(i).kind {
        ShotKind::Straight(o) => (o.fx_end, o.explode, o.sfx_end),
        _ => return,
    };
    if let ShotKind::Straight(o) = &mut w.shot(i).kind {
        o.dead = 0;
    }
    if fx_end > 0 {
        return;
    }
    if explode_fx && sfx_end >= 0 {
        w.sfx(sfx_end, 64);
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
    Shot::new(owner, dir, 1, x << 8, y << 8, ShotKind::Kame(Box::new(KameBeam { len: 8, done: false, a, mult, gfx, vt: 0, stopped: false, next: built, charging: false, sounds: false })))
}
/// an enemy's beam of the player's class (Kid Buu's Kamehameha): team 0, charging until `kame_fire`, its own sounds
pub fn kame_beam_enemy(owner: EntityId, dir: Dir, x: i32, y: i32, built: i64, a: AttackType, mult: i32) -> Shot {
    Shot::new(owner, dir, 0, x << 8, y << 8, ShotKind::Kame(Box::new(KameBeam { len: 8, done: false, a, mult, gfx: true, vt: 0, stopped: false, next: built, charging: true, sounds: true })))
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
            // destructor 0x08020768: stop 32, sound 33
            if b.sounds {
                w.sfx_stop(32);
                w.sfx(33, 64);
            }
        }
        return;
    }
    if b.charging {
        return;
    }
    let (hx, hy) = kame_head(w.shot_ref(i), b.len);
    let hr = Rect::new(hx - 8, hy - 8, hx + 8, hy + 8);
    // the body box uses the head of this tick before any growth
    let body = rect(w, i);
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
    hit(w, i, body, b.a, b.mult, false);
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
pub const GHOST_SPRITE: i32 = 148;
/// the ghost's animation host: sprite 148, the game RNG
struct GHost<'a> {
    d: &'a crate::content::GameData,
    facing: Dir,
    rng: &'a mut crate::rng::Rng,
}
impl crate::eanim::Host for GHost<'_> {
    fn nframes(&mut self, raw: i32) -> i32 {
        self.d.sprite(GHOST_SPRITE).and_then(|s| s.anim_raw(raw)).map(|a| a.steps(self.facing).len() as i32).unwrap_or(0)
    }
    fn rand(&mut self, n: u32) -> i32 {
        self.rng.rand(n)
    }
    fn sfx(&mut self, _id: i32, _vol: i32) {}
}
fn ghost_enter(w: &mut World, i: usize, raw: i32, rule: u32) {
    let d = std::sync::Arc::clone(&w.data);
    let mut rng = w.rng;
    if let ShotKind::Ghost(g) = &mut w.shot(i).kind {
        let mut h = GHost { d: &d, facing: g.facing, rng: &mut rng };
        g.ea.enter(&d.anim.rules, raw, rule, &mut h);
    }
    w.rng = rng;
}
/// Kamikaze ghost (skill 13), created at the skill's tick 16: the constructor's SetAnim(0) (the idle enter draws
/// rand(255)), then the appear animation raw 19 (once, shift 3, its end SetAnim(0) again)
pub fn ghost(w: &mut World, owner: EntityId, dir: Dir, x: i32, y: i32, slot: i32) -> EntityId {
    let facing = w.actor_by_id(owner).map(|a| a.facing).unwrap_or(dir);
    let id = add(w, Shot::new(owner, dir, 1, x, y, ShotKind::Ghost(Box::new(Ghost { free: false, wait: 0, slot, ea: Default::default(), facing, boom: false }))));
    let i = w.find(id).unwrap();
    let (idle, once) = (w.data.anim.setanim[&1], w.data.anim.setanim[&4]);
    ghost_enter(w, i, 1, idle);
    ghost_enter(w, i, 19, once);
    id
}
/// PvP rule set (`World::opts.pvp`, multiplayer only): the other players a player's homing shot may chase - alive,
/// hittable, not being eaten (`player::pvp_targetable`), in slot order. Empty without PvP or for an enemy's shot.
pub fn pvp_homing_targets(w: &World, owner: EntityId) -> Vec<usize> {
    if !w.opts.pvp || !w.find(owner).is_some_and(|k| matches!(w.ents[k].body, Body::Player(_))) {
        return Vec::new();
    }
    w.players
        .iter()
        .filter(|&&id| id != owner)
        .filter_map(|&id| w.find(id))
        .filter(|&k| !w.ents[k].removed && matches!(w.ents[k].body, Body::Player(_)) && player::pvp_targetable(w, k))
        .collect()
}

/// in formation it eases to its slot (pos += (slot - pos) >> 5) once the appear animation has ended. Released (AI
/// state 0x11B): Selector [IfContactExplode (0x080390A0: a message with its frame box, energy sub 1 x4.0; accepted ->
/// sound 48 + rand(2), state 0x11C), home in (0x08038F2C, speed 0x300: the nearest enemy by frame-box centres; within
/// 4 px or none -> 0x11C; else SetAnim(1) and a step d * 0x300 / max(|dx|, |dy|)), 0x11C]. State 0x11C: an explosion
/// (rate 0x1000 + rand(0x1000)) and gone
fn ghost_update(w: &mut World, i: usize) {
    w.shot(i).t += 1;
    let d = std::sync::Arc::clone(&w.data);
    let tick = {
        let mut rng = w.rng;
        let r = if let ShotKind::Ghost(g) = &mut w.shot(i).kind {
            let mut h = GHost { d: &d, facing: g.facing, rng: &mut rng };
            matches!(g.ea.tick(&d.anim.rules, &mut h), crate::eanim::Tick::Idle)
        } else {
            false
        };
        w.rng = rng;
        r
    };
    if tick {
        ghost_enter(w, i, 1, d.anim.setanim[&1]);
    }
    let owner = w.shot_ref(i).owner;
    let ShotKind::Ghost(g) = &w.shot_ref(i).kind else { return };
    let g = (**g).clone();
    if !g.free {
        let Some(a) = w.actor_by_id(owner) else { return };
        let (of, opx, opy) = (a.facing, a.px(), a.py());
        if let ShotKind::Ghost(x) = &mut w.shot(i).kind {
            x.facing = of;
        }
        if g.ea.pos.raw == 19 {
            return;
        }
        let o = GHOST_SLOTS[of as usize][g.slot.min(2) as usize];
        let tx = (opx + o.0) << 8;
        let ty = (opy + o.1) << 8;
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
    let set_boom = |w: &mut World| {
        if let ShotKind::Ghost(x) = &mut w.shot(i).kind {
            x.boom = true;
        }
    };
    if g.boom {
        w.ents[i].removed = true;
        let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
        let rate = 0x1000 + w.rng.rand(0x1000);
        fx::explosion(w, px, py, rate);
        return;
    }
    let r = rect(w, i);
    if hit(w, i, r, atk(1), 0x1000, false) {
        let sid = 48 + w.rng.rand(2);
        w.sfx(sid, 64);
        set_boom(w);
        return;
    }
    // the nearest enemy (enemy list order, frame-box centres, strict minimum)
    let (gx, gy) = ((r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1);
    let mut best: Option<(i32, i32)> = None;
    let mut bd = i64::from(i32::MAX);
    for &id in &w.enemies {
        let Some(k) = w.find(id) else { continue };
        if w.ents[k].removed || w.en_ref(k).defeated {
            continue;
        }
        let b = crate::enemy::frame_box(w, k);
        let dx = ((b.x0 + b.x1) >> 1) - gx;
        let dy = ((b.y0 + b.y1) >> 1) - gy;
        let d2 = dx as i64 * dx as i64 + dy as i64 * dy as i64;
        if d2 < bd {
            bd = d2;
            best = Some((dx, dy));
        }
    }
    // PvP rule set (not in the original game): the opposing players count as targets too, after the enemies
    if w.opts.pvp {
        for k in pvp_homing_targets(w, owner) {
            let b = w.pl_ref(k).a.body_box();
            let dx = ((b.x0 + b.x1) >> 1) - gx;
            let dy = ((b.y0 + b.y1) >> 1) - gy;
            let d2 = dx as i64 * dx as i64 + dy as i64 * dy as i64;
            if d2 < bd {
                bd = d2;
                best = Some((dx, dy));
            }
        }
    }
    let Some((dx, dy)) = best else { return set_boom(w) };
    let (ax, ay) = (dx.abs(), dy.abs());
    let facing: Dir = if ax > ay {
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
    if let ShotKind::Ghost(x) = &mut w.shot(i).kind {
        x.facing = facing;
    }
    let m = ax.max(ay);
    if m < 4 {
        return set_boom(w);
    }
    if g.ea.pos.raw != 2 {
        ghost_enter(w, i, 2, d.anim.loop3);
    }
    let s = w.shot(i);
    s.x += (dx * 0x300) / m;
    s.y += (dy * 0x300) / m;
}

/// energy wave update 0x0802A3D2 (mode 0): the segment box with the CURRENT length: a projectile wall -> mode 1; else
/// the hit (energy x1.0) accepted -> puff at the head, mode 1; else grow 4 (to 48) or move 4, the counter past 240 ->
/// mode 1. No camera test. Mode 1: moves 4, len - 4, gone at 0
fn wave_update(w: &mut World, i: usize) {
    let dir = w.shot_ref(i).dir;
    let (dx, dy) = DIR_VEC[dir as usize];
    w.shot(i).t += 1;
    let ShotKind::EnergyWave { len, mode, life } = w.shot_ref(i).kind else { return };
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
    let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
    let (hx, hy) = (px + dx * len, py + dy * len);
    let r = rect(w, i);
    if in_wall(w, i, &r) {
        set_kind(w, i, ShotKind::EnergyWave { len, mode: 1, life });
        return;
    }
    if hit(w, i, r, atk(1), 1024, true) {
        fx::puff_rng(w, hx, hy);
        set_kind(w, i, ShotKind::EnergyWave { len, mode: 1, life });
        return;
    }
    let mut l = len;
    if len < 48 {
        l += 4;
    } else {
        let s = w.shot(i);
        s.x += dx * 4 * 256;
        s.y += dy * 4 * 256;
    }
    let life = life + 1;
    set_kind(w, i, ShotKind::EnergyWave { len: l, mode: if life > 240 { 1 } else { 0 }, life });
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
    // 0x080247C0: tests the player only (16 x 16 box, x0.05, sub 4), damage without a reaction, once per flame
    let owner = w.shot_ref(i).owner;
    if !done && !owner_dead(w, owner) {
        if let Some(pi) = target_player(w, owner) {
            let r = rect(w, i);
            let stats = w.actor_by_id(owner).map(|a| a.stats).unwrap_or_default();
            let mut m = HitMsg { team: 0, rect: r, dir: w.shot_ref(i).dir, mult: 51, atk: atk(4), attacker: owner, att_stats: stats, att_is_player: false, source: w.ents[i].id, crit: false, raw: 0, dmg: 0, no_react: true, foot: false };
            if player::on_message(w, pi, &mut m) {
                if let ShotKind::Flame { done, .. } = &mut w.shot(i).kind {
                    *done = true;
                }
            }
        }
    }
}

/// bomb update (class 0x0804A2E8, 0x0801C80A): counter + 1; 1..40 move: pos += v, vy += 0x20; at 71 it explodes:
/// one message (16 x 16, energy x1.0), 4 debris, the explosion (rate 0x1000 + rand(0x1000)) and sound 24
fn bomb_update(w: &mut World, i: usize) {
    let t = {
        let s = w.shot(i);
        s.t += 1;
        s.t
    };
    let ShotKind::Bomb { vx, vy, .. } = w.shot_ref(i).kind else { return };
    if t <= 40 {
        let s = w.shot(i);
        s.x += vx;
        s.y += vy;
        s.kind = ShotKind::Bomb { vx, vy: vy + 0x20, landed: if t == 40 { t } else { -1 } };
        return;
    }
    if t >= 71 {
        w.ents[i].removed = true;
        let r = rect(w, i);
        if w.field.is_some() {
            // field: the blast's message is of sender class 2, which the enemies' handler (0x0800FBE6: class 1 or 2)
            // takes as well as the player's - a Mad Bomber's bomb hurts another bandit (Diablo Desert 11-14: 49 HP and
            // the knockback). Battles keep the reference (the player only)
            let s = w.shot_ref(i);
            let (owner, dir) = (s.owner, s.dir);
            let stats = w.find(owner).and_then(|oi| w.actor(oi).map(|a| a.stats)).unwrap_or_default();
            let mut m = HitMsg { team: 2, rect: r, dir, mult: 1024, atk: atk(1), attacker: owner, att_stats: stats, att_is_player: false, source: w.ents[i].id, crit: false, raw: 0, dmg: 0, no_react: false, foot: false };
            broadcast(w, &mut m, true);
        } else {
            hit(w, i, r, atk(1), 1024, true);
        }
        let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
        for k in 0..4 {
            let (dvx, dvy) = [(96, 0), (-96, 0), (0, 64), (0, -64)][k];
            fx::add(w, fx::Fx::Debris { x: (px - 4) << 8, y: (py - 4) << 8, vx: dvx, vy: dvy, t: 0 });
        }
        let rate = 0x1000 + w.rng.rand(0x1000);
        fx::explosion(w, px, py, rate);
        w.sfx(24, 64);
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
        // the tank shell's handler 0x0801CB38: any message of kind 0 but its own overlapping its box destroys it
        // (0x0801C978: explosion, rate 0x2000) and is accepted (3-14: Gohan's ki blast shoots a shell down, its hit
        // sound plays)
        ShotKind::Straight(o) if o.name == SName::Shell => {
            if o.dead < 0 && !msg.foot && msg.source != me && !w.ents[i].removed && overlaps(&msg.rect, &rect(w, i)) {
                w.ents[i].removed = true;
                let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
                fx::explosion(w, px, py, 0x2000);
                return true;
            }
            false
        }
        ShotKind::Straight(o) => {
            if o.fragile && o.dead < 0 && msg.team == 1 && msg.atk.kind == 0 && msg.source != me && overlaps(&msg.rect, &rect(w, i)) {
                let sfx_end = o.sfx_end;
                w.ents[i].removed = true;
                if sfx_end >= 0 {
                    w.sfx(sfx_end, 64);
                }
                let (px, py) = (w.shot_ref(i).px(), w.shot_ref(i).py());
                fx::explosion(w, px, py, 0x2000);
            }
            false
        }
        // a flying bandage is destroyed by any other object's hit message overlapping its box (message 0)
        ShotKind::Wrap(m) => {
            if msg.source == me || m.stuck || w.ents[i].removed || !overlaps(&msg.rect, &rect(w, i)) {
                return false;
            }
            wrap_burst(w, i);
            true
        }
        // another object's message head-on (the player's punch or energy attack; another wave: two laser turrets
        // facing each other) turns it back (mode 1) without a puff of its own: the sender's wave puffs on the
        // accepted hit (Babidi's ship 8-36: one rand(0x1000) for two colliding waves; 8-51: a punch, no draw)
        ShotKind::EnergyWave { len, mode, .. } => {
            if msg.source == me || *mode != 0 {
                return false;
            }
            let s = w.shot_ref(i);
            let (dx, dy) = DIR_VEC[s.dir as usize];
            let (hx, hy) = (s.px() + dx * len, s.py() + dy * len);
            if msg.dir == (s.dir ^ 1) && overlaps(&msg.rect, &Rect::new(hx - 12, hy - 12, hx + 12, hy + 12)) {
                if let ShotKind::EnergyWave { mode, .. } = &mut w.shot(i).kind {
                    *mode = 1;
                }
                return true;
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
