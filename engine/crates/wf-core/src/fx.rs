//! Effect objects. They never damage, but they are world objects in the game (they take object slots, update in
//! the object order and some draw from the RNG), so the sim keeps them as entities with the game's lifetimes; the
//! renderer draws them from this state (record ids / addresses, positions, ticks).

use crate::geom::{Dir, Rect, DIR_VEC};
use crate::world::{broadcast, HitMsg};
use serde::{Deserialize, Serialize};

const FLAME_TABLE: u32 = 0x0875_68b4;
use crate::input;
use crate::world::{Body, EntityId, World};

/// what keeps a skill-bound effect alive (the reference's closures)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Alive {
    /// player `p`'s current skill instance is `serial`
    Skill { p: EntityId, serial: u32 },
    /// ... and its animation tick is below `n`
    SkillTickLt { p: EntityId, serial: u32, n: i32 },
    /// ... and the skill is not charged / launched yet
    SkillUncharged { p: EntityId, serial: u32 },
    /// player `p` is in the transformation state
    Transform { p: EntityId },
    /// beam `id` present, not removed, not stopped
    Beam { id: EntityId },
    /// player `p`'s Instant Transmission pick / sequence `serial`
    It { p: EntityId, serial: u32 },
}

pub fn alive(w: &World, a: Alive) -> bool {
    let pl = |p: EntityId| {
        w.find(p)
            .filter(|&i| matches!(w.ents[i].body, Body::Player(_)))
            .map(|i| w.pl_ref(i))
    };
    match a {
        Alive::Skill { p, serial } => {
            pl(p).is_some_and(|p| p.sk.as_ref().is_some_and(|s| s.serial == serial))
        }
        Alive::SkillTickLt { p, serial, n } => pl(p)
            .is_some_and(|p| p.sk.as_ref().is_some_and(|s| s.serial == serial) && p.ap.tick < n),
        Alive::SkillUncharged { p, serial } => pl(p).is_some_and(|p| {
            p.sk.as_ref()
                .is_some_and(|s| s.serial == serial && !s.charged)
        }),
        Alive::Transform { p } => {
            pl(p).is_some_and(|p| p.state == crate::player::PState::Transform)
        }
        Alive::Beam { id } => w
            .find(id)
            .is_some_and(|i| !w.ents[i].removed && !crate::shots::beam_done(w, i)),
        Alive::It { p, serial } => pl(p).is_some_and(|p| {
            p.it_sel.as_ref().is_some_and(|s| s.serial == serial)
                || p.it.as_ref().is_some_and(|s| s.serial == serial)
        }),
    }
}

/// which record rule a record object shows
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecWhat {
    /// Flame Kamehameha charge: fx_08721164 record 12 + f
    FlameCharge,
    /// Energy Punch Lv3 aura: 0x08762270 + 12 * (3 * dir + (tick >= 56))
    EnergyPunch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BeamSet {
    Kamehameha1,
    Other,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub enum Fx {
    /// generic animated effect 0x0802A81C: records start..start+n-1 of run `id`, acc += rate, advance acc >> 16
    /// `id` = ROM address of the effect run (content id `fx_%08x`)
    Anim {
        id: u32,
        x: i32,
        y: i32,
        rec: i32,
        left: i32,
        rate: i32,
        acc: i32,
        /// sorted with the actors by its y (a Kamikaze ghost's explosion: skill audit sa_f31_skill13); else in front
        /// of them (a breakable's / an enemy's: pixel gate dr3_007_10-15 f210, sb_010_20_04 f150)
        #[serde(default)]
        sorted: bool,
    },
    /// looping variant (Super Saiyan aura) until `alive` fails
    Loop {
        id: u32,
        x: i32,
        y: i32,
        first: i32,
        n: i32,
        rec: i32,
        rate: i32,
        acc: i32,
        alive: Alive,
    },
    /// debris piece 0x0800C454: 32 ticks
    Debris {
        x: i32,
        y: i32,
        vx: i32,
        vy: i32,
        t: i32,
    },
    /// record object 0x080507A0 (`what` names the record rule for the renderer)
    RecObj {
        what: RecWhat,
        owner: EntityId,
        t: i32,
        alive: Alive,
    },
    /// beam charge sparkle at owner + off[facing]
    BeamCharge {
        owner: EntityId,
        off: [(i32, i32); 4],
        set: BeamSet,
        t: i32,
    },
    /// Dragon Fist Explosion chain (6 segments, 1/256 px)
    /// Dragon Fist Explosion chain: segment 0 = root .. 5 = head (1/256 px), their boxes of the last update, the
    /// head's steering offset, ticks since the launch (-1 flying)
    DragonFist {
        owner: EntityId,
        seg: [[i32; 2]; 6],
        boxes: [Rect; 6],
        steer: (i32, i32),
        dir: Dir,
        dying: i32,
        t: i32,
        /// the player control's shift of this frame (frame, v): the chain's OAM is built before it (`view`)
        shifted: (u32, i32),
    },
    EnergySword {
        owner: EntityId,
        slash: i32,
        t: i32,
        alive: Alive,
    },
    BbkFlare {
        x: i32,
        y: i32,
        dir: Dir,
        t: i32,
        ended: bool,
        alive: Alive,
    },
    /// Flame Kamehameha links [x, y, t, shift*2]
    /// Flame Kamehameha links [x, y, t, shift*2, box x0, y0, x1, y1 of the previous tick]; hits with attack `a` x `mult`
    FlameKame {
        owner: EntityId,
        dir: Dir,
        n: i32,
        links: Vec<[i32; 8]>,
        alive: Alive,
        a: crate::damage::AttackType,
        mult: i32,
    },
    ItMarkers {
        t: i32,
        alive: Alive,
    },
    /// Big Bang explosion phase (state 2 of 0x08037650): 60 ticks, an explosion every 8th (RNG)
    BigBangBoom {
        n: i32,
        x0: i32,
        y0: i32,
        s: i32,
    },
    /// thrown rock 0x08033F28 (vtable 0x0804CE50, Goten's rock drill): 22 updates of two gravity substeps (24.8),
    /// then a 24x24 energy hit (0x086B8090, mult 0) on the player - accepted queues event s120 (header 0x08783084) -
    /// and a puff
    Rock {
        owner: EntityId,
        x: i32,
        y: i32,
        vx: i32,
        vy: i32,
        n: i32,
    },
}

impl Fx {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Fx::Anim { .. } => "fx",
            Fx::Loop { .. } => "fx",
            Fx::Debris { .. } => "debris",
            Fx::RecObj { .. } => "recobj",
            Fx::BeamCharge { .. } => "beamcharge",
            Fx::DragonFist { .. } => "dragonfx",
            Fx::EnergySword { .. } => "swordfx",
            Fx::BbkFlare { .. } => "bbkfx",
            Fx::FlameKame { .. } => "flamekamefx",
            Fx::ItMarkers { .. } => "itfx",
            Fx::BigBangBoom { .. } => "bigbangboom",
            Fx::Rock { .. } => "rockfx",
        }
    }
}

/// a bomb chip's `t` once its update has deleted it and released its graphic reference
pub const DEBRIS_RELEASED: i32 = i32::MAX;

pub fn add(w: &mut World, f: Fx) -> EntityId {
    // the generic animated effect object (`0x0802A81C`, vtable 0x0804CF3C) holds 0x400 of object tiles
    let tiles = matches!(f, Fx::Anim { .. } | Fx::Loop { .. });
    // the Instant Transmission target markers (ctor `0x080222E0`, records 0x0872BF04: 16 x 32 -> 0x200 bytes; real
    // run 20-6 r062_001 tile +11,000 f6)
    let markers = matches!(f, Fx::ItMarkers { .. });
    let id = w.add(Body::Fx(Box::new(f)));
    if tiles {
        crate::vram::acquire(w, crate::vram::Owner::Ent(id), 0x400);
    }
    if markers {
        crate::vram::acquire(w, crate::vram::Owner::Ent(id), 0x200);
    }
    id
}
pub fn fx_anim(w: &mut World, id: u32, x: i32, y: i32, start: i32, n: i32, rate: i32) -> EntityId {
    add(
        w,
        Fx::Anim {
            id,
            x,
            y,
            rec: start,
            left: n,
            rate,
            acc: 0,
            sorted: false,
        },
    )
}
/// [`fx_anim`] with the caller's tile size (the ctor `0x0802A81C` takes it as an argument: the shots' trail puffs
/// 0x40 - tank shell 0x0801CA4A, rifle bullet 0x080277F4; run 3 cut 112,165, 3-14: the game's puff takes 1 unit)
pub fn fx_anim_sized(w: &mut World, id: u32, x: i32, y: i32, start: i32, n: i32, rate: i32, bytes: i32) -> EntityId {
    let e = w.add(Body::Fx(Box::new(Fx::Anim { id, x, y, rec: start, left: n, rate, acc: 0, sorted: false })));
    crate::vram::acquire(w, crate::vram::Owner::Ent(e), bytes);
    e
}
/// smoke puff 0x086FD9D0 (4 records)
pub fn puff(w: &mut World, x: i32, y: i32, rate: i32) {
    fx_anim(w, 0x086f_d9d0, x, y, 0, 4, rate);
}
/// 0x08033F28: a rock thrown from (sx, sy) to (tx, ty) in 44 gravity substeps
pub fn rock_throw(w: &mut World, owner: EntityId, sx: i32, sy: i32, tx: i32, ty: i32) -> EntityId {
    let vx = ((tx - sx) * 0x5d3) >> 8;
    let vy = (((ty - sy) * 0x5d3 - 0xba60) >> 8) - 0x200;
    let id = add(
        w,
        Fx::Rock {
            owner,
            x: sx << 8,
            y: sy << 8,
            vx,
            vy,
            n: 0x16,
        },
    );
    // the rock is a projectile (0x08033F28 -> the base ctor 0x08019B04 with 0x40): one unit of sprite memory (run 3
    // cut 157,820, Earth 2-90: the game's (434, 1) in the throw's frame); field only (the allocator)
    crate::vram::acquire(w, crate::vram::Owner::Ent(id), 0x40);
    id
}
/// puff with the game's random rate 0x1000 + rand(0x1000)
pub fn puff_rng(w: &mut World, x: i32, y: i32) {
    let r = 0x1000 + w.rng.rand(0x1000);
    puff(w, x, y, r);
}
/// explosion 0x08712FDC (4 records)
pub fn explosion(w: &mut World, x: i32, y: i32, rate: i32) -> EntityId {
    fx_anim(w, 0x0871_2fdc, x, y, 0, 4, rate)
}
/// [`explosion`] sorted with the actors by its y (a Kamikaze ghost's)
pub fn explosion_sorted(w: &mut World, x: i32, y: i32, rate: i32) -> EntityId {
    let e = explosion(w, x, y, rate);
    set_sorted(w, e);
    e
}
/// an animation effect sorted with the actors by its y (layer 0x38) instead of in front of them
pub fn set_sorted(w: &mut World, e: EntityId) {
    if let Some(k) = w.find(e) {
        if let crate::world::Body::Fx(f) = &mut w.ents[k].body {
            if let Fx::Anim { sorted, .. } = f.as_mut() {
                *sorted = true;
            }
        }
    }
}
pub fn beam_charge(w: &mut World, owner: EntityId, off: [(i32, i32); 4], set: BeamSet) -> EntityId {
    add(
        w,
        Fx::BeamCharge {
            owner,
            off,
            set,
            t: 0,
        },
    )
}

const DF_ROOT: [(i32, i32); 4] = [(8, 16), (8, -32), (-8, -8), (24, -8)];
const DF_OFFS: [i32; 6] = [0, 1, 3, 6, 10, 15];
/// head box origin per facing (box = position - origin .. + 64)
const DF_HEAD: [(i32, i32); 4] = [(32, 32), (32, 32), (40, 32), (24, 32)];
const DF_TABLE: u32 = 0x0877_ad10;
const DF_MULT: i32 = 328;
const DF_LAUNCH_MULT: i32 = 0xa00;

fn df_box(seg: &[[i32; 2]; 6], k: usize, dir: Dir) -> Rect {
    let (x, y) = (seg[k][0] >> 8, seg[k][1] >> 8);
    if k < 5 {
        return Rect::new(x - 16, y - 16, x + 16, y + 16);
    }
    let (ox, oy) = DF_HEAD[dir as usize];
    Rect::new(x - ox, y - oy, x - ox + 64, y - oy + 64)
}
/// Dragon Fist Explosion (chain builder 0x08038AD8, segment update 0x08038C32): 6 segments from the root at DF_ROOT
pub fn dragon_fist(w: &World, owner_px: i32, owner_py: i32, dir: Dir, owner: EntityId) -> Fx {
    let _ = w;
    let (dx, dy) = DIR_VEC[dir as usize];
    let (rx, ry) = DF_ROOT[dir as usize];
    let (x, y) = ((owner_px + rx) << 8, (owner_py + ry) << 8);
    let mut seg = [[0; 2]; 6];
    for k in 0..6 {
        seg[k] = [x + dx * DF_OFFS[k] * 256, y + dy * DF_OFFS[k] * 256];
    }
    let boxes = std::array::from_fn(|k| df_box(&seg, k, dir));
    Fx::DragonFist {
        owner,
        seg,
        boxes,
        steer: (0, 0),
        dir,
        dying: -1,
        t: 0,
        shifted: (u32::MAX, 0),
    }
}
fn df_msg(w: &World, owner: EntityId, src: EntityId, rect: Rect, dir: Dir, mult: i32) -> HitMsg {
    let stats = w.actor_by_id(owner).map(|a| a.stats).unwrap_or_default();
    HitMsg {
        team: 1,
        rect,
        dir,
        mult,
        atk: crate::damage::atk(18),
        attacker: owner,
        att_stats: stats,
        att_is_player: true,
        source: src,
        crit: false,
        raw: 0,
        dmg: 0,
        no_react: false,
        foot: false,
    }
}
/// 0x08038E8A (the player's control, tick < 90): root by v, each child 0x20 more along the facing
pub fn dragon_shift(w: &mut World, i: usize, v: i32) {
    let frame = w.frame;
    if let Fx::DragonFist { seg, dir, shifted, .. } = fx_mut(w, i) {
        *shifted = (frame, v);
        let (dx, dy) = DIR_VEC[*dir as usize];
        for (k, s) in seg.iter_mut().enumerate() {
            s[0] += dx * (v + 0x20 * k as i32);
            s[1] += dy * (v + 0x20 * k as i32);
        }
    }
}
/// 0x08038E42: one x2.5 message with the head's box of this tick; the segments stop sending
pub fn dragon_launch(w: &mut World, i: usize) {
    let Fx::DragonFist {
        owner,
        boxes,
        dir,
        dying,
        ..
    } = fx_mut(w, i).clone()
    else {
        return;
    };
    if dying >= 0 {
        return;
    }
    if let Fx::DragonFist { dying, .. } = fx_mut(w, i) {
        *dying = 0;
    }
    let mut m = df_msg(w, owner, w.ents[i].id, boxes[5], dir, DF_LAUNCH_MULT);
    broadcast(w, &mut m, false);
}
/// every segment explodes (rand(0x1000) each): root first at the end, head first on a cancel
pub fn dragon_explode(w: &mut World, i: usize, cancel: bool) {
    let Fx::DragonFist { seg, dir, .. } = fx_mut(w, i).clone() else {
        return;
    };
    for n in 0..6 {
        let k = if cancel { 5 - n } else { n };
        let rec = DF_TABLE + 12 * (dir as u32 * 4 + if k == 5 { 3 } else { 0 });
        let (x, y) = (seg[k][0] >> 8, seg[k][1] >> 8);
        let (cx, cy) = match w.data.record(rec) {
            Some(g) => (x - g[0] + (g[2] >> 1), y - g[1] + (g[3] >> 1)),
            None => (x, y),
        };
        let rate = 0x1000 + w.rng.rand(0x1000);
        explosion(w, cx, cy, rate);
    }
    w.ents[i].removed = true;
}

fn fx_mut(w: &mut World, i: usize) -> &mut Fx {
    match &mut w.ents[i].body {
        Body::Fx(f) => f,
        _ => panic!("not fx"),
    }
}

fn rock_land(w: &mut World, i: usize, owner: EntityId, x: i32, y: i32) {
    let pi = w
        .ents
        .iter()
        .position(|e| !e.removed && matches!(e.body, Body::Player(_)));
    if let Some(pi) = pi {
        let stats = w
            .find(owner)
            .and_then(|oi| w.actor(oi).map(|a| a.stats))
            .unwrap_or_default();
        let id = w.ents[i].id;
        let mut m = HitMsg {
            team: 0,
            rect: Rect::new(x - 12, y - 12, x + 12, y + 12),
            dir: 0,
            mult: 0,
            atk: crate::damage::atk(1),
            attacker: owner,
            att_stats: stats,
            att_is_player: false,
            source: id,
            crit: false,
            raw: 0,
            dmg: 0,
            no_react: false,
            foot: true,
        };
        if crate::player::on_message(w, pi, &mut m) {
            // (the event starts with the next frame's world update, before the objects: the hit leader does not take
            // its step - run 2 row 159,704, Goten's tag game)
            let at = w.frame + 1;
            if let Some(f) = w.field.as_mut() {
                f.deferred.push(crate::fieldobj::Deferred::Run {
                    at,
                    run: crate::fieldcontent::Callback::Event("header_0x08783084".into()),
                    cause: crate::field::Cause::Ai { entity: owner },
                    nested: true,
                });
            }
        }
    }
    puff_rng(w, x, y);
    // (its sprite memory goes with it, in the landing update: run 3 cut 158,217)
    let id = w.ents[i].id;
    crate::vram::release(w, crate::vram::Owner::Ent(id));
    w.ents[i].removed = true;
}

pub fn update(w: &mut World, i: usize) {
    // a broken breakable's timer object (the list's newest when it fires) starts its callback's event inside its
    // update, and the event takes the rest of the frame: the older objects are not updated (VERIFIED segment sb
    // 21-5 f552: the break's puff `0x0802A8AA` is not called that frame and so lives one frame longer; the player's
    // skip is player.rs's)
    if w.field
        .as_ref()
        .is_some_and(|f| f.busy && f.busy_pre_pass == w.frame && f.busy_since + 1 == w.frame)
    {
        return;
    }
    // ... and in the callback's last frame they are updated twice (field.rs `break_owed`)
    if w.field.as_ref().is_some_and(|f| f.break_owed == w.frame)
        && matches!(&w.ents[i].body, Body::Fx(f) if matches!(**f, Fx::Anim { .. }))
    {
        update_one(w, i);
        if w.ents[i].removed {
            return;
        }
    }
    update_one(w, i);
}

fn update_one(w: &mut World, i: usize) {
    let f = match &w.ents[i].body {
        Body::Fx(f) => (**f).clone(),
        _ => return,
    };
    match f {
        Fx::Anim { .. } => {
            let mut ended = false;
            if let Fx::Anim {
                rec,
                left,
                rate,
                acc,
                ..
            } = fx_mut(w, i)
            {
                *acc += *rate;
                let adv = *acc >> 16;
                if adv != 0 {
                    *rec += adv;
                    *left -= adv;
                    *acc &= 0xffff;
                    ended = *left <= 0;
                }
            }
            if ended {
                // the object deletes itself in its update (`0x0802A8AA`): its tiles are free for the objects
                // updated after it this frame (segment sb 19-3: a beam's new explosion takes the block)
                w.ents[i].removed = true;
                crate::vram::entity_gone(w, i);
            }
        }
        Fx::Loop { alive: al, .. } => {
            if !alive(w, al) {
                w.ents[i].removed = true;
                return;
            }
            if let Fx::Loop {
                first,
                n,
                rec,
                rate,
                acc,
                ..
            } = fx_mut(w, i)
            {
                *acc += *rate;
                let adv = *acc >> 16;
                if adv != 0 {
                    *acc &= 0xffff;
                    *rec = *first + (*rec - *first + adv) % *n;
                }
            }
        }
        Fx::Rock { owner, .. } => {
            let mut end = None;
            if let Fx::Rock {
                x, y, vx, vy, n, ..
            } = fx_mut(w, i)
            {
                for _ in 0..2 {
                    *vy += 0x20;
                    *x += *vx;
                    *y += *vy;
                }
                *n -= 1;
                if *n == 0 {
                    end = Some((*x >> 8, *y >> 8));
                }
            }
            if let Some((x, y)) = end {
                rock_land(w, i, owner, x, y);
            }
        }
        Fx::Debris { .. } => {
            if let Fx::Debris { x, y, vx, vy, t } = fx_mut(w, i) {
                *vx += 1;
                *vy += 2;
                *x += *vx;
                *y += *vy;
                *t += 1;
                if *t >= 32 {
                    // the chip object is deleted in its update: its reference of the shared graphic goes now, before
                    // the older objects' updates allocate (tour_earth_circus f1381: a rifle trail's puff after a
                    // bomb's four chips end); `DEBRIS_RELEASED` keeps the reap from releasing it again
                    *t = DEBRIS_RELEASED;
                    w.ents[i].removed = true;
                    crate::vram::keyed_release(w, crate::vram::KEY_DEBRIS);
                }
            }
        }
        Fx::RecObj { alive: al, .. } => {
            if let Fx::RecObj { t, .. } = fx_mut(w, i) {
                *t += 1;
            }
            if !alive(w, al) {
                w.ents[i].removed = true;
            }
        }
        Fx::BeamCharge { .. } => {
            if let Fx::BeamCharge { t, .. } = fx_mut(w, i) {
                *t += 1;
            }
        }
        Fx::DragonFist {
            owner,
            mut seg,
            mut boxes,
            mut steer,
            dir,
            dying,
            t,
            ..
        } => {
            if dying >= 0 {
                let d = dying + 1;
                // the body keeps easing between its neighbours after the launch (no steering, no messages): skill
                // audit sa_f41_skill5_steer f127.., the game's segments 1 .. 4 still moving toward the head and root
                for k in 1..5 {
                    let (p0, c0) = (seg[k - 1], seg[k + 1]);
                    seg[k][0] = (3 * seg[k][0] + ((p0[0] + c0[0]) >> 1)) >> 2;
                    seg[k][1] = (3 * seg[k][1] + ((p0[1] + c0[1]) >> 1)) >> 2;
                }
                if let Fx::DragonFist {
                    seg: s0,
                    dying: d0,
                    t: t0,
                    ..
                } = fx_mut(w, i)
                {
                    *s0 = seg;
                    *d0 = d;
                    *t0 = t + 1;
                }
                if d >= 24 {
                    dragon_explode(w, i, false);
                }
                return;
            }
            let held = w.find(owner).and_then(|k| match &w.ents[k].body {
                Body::Player(p) => Some(w.input[p.slot as usize].held),
                _ => None,
            });
            let held = held.unwrap_or(0);
            let me = w.ents[i].id;
            for k in 0..6 {
                if k > 0 && k < 5 {
                    let (p0, c0) = (seg[k - 1], seg[k + 1]);
                    seg[k][0] = (3 * seg[k][0] + ((p0[0] + c0[0]) >> 1)) >> 2;
                    seg[k][1] = (3 * seg[k][1] + ((p0[1] + c0[1]) >> 1)) >> 2;
                } else if k == 5 {
                    let mut sx = 0;
                    let mut sy = 0;
                    if held & input::LEFT != 0 {
                        if steer.0 > -56 {
                            sx = -3;
                        }
                    } else if held & input::RIGHT != 0 && steer.0 < 56 {
                        sx = 3;
                    }
                    if held & input::UP != 0 {
                        if steer.1 > -32 {
                            sy = -3;
                        }
                    } else if held & input::DOWN != 0 && steer.1 < 32 {
                        sy = 3;
                    }
                    steer.0 += sx;
                    steer.1 += sy;
                    seg[5][0] += sx << 8;
                    seg[5][1] += sy << 8;
                }
                boxes[k] = df_box(&seg, k, dir);
                // store before the message (a receiver may look at the chain)
                if let Fx::DragonFist {
                    seg: s0,
                    boxes: b0,
                    steer: st0,
                    t: t0,
                    ..
                } = fx_mut(w, i)
                {
                    *s0 = seg;
                    *b0 = boxes;
                    *st0 = steer;
                    *t0 = t + 1;
                }
                let mut m = df_msg(w, owner, me, boxes[k], dir, DF_MULT);
                broadcast(w, &mut m, false);
            }
        }
        Fx::EnergySword { alive: al, .. } => {
            if !alive(w, al) {
                w.ents[i].removed = true;
                return;
            }
            if let Fx::EnergySword { slash, t, .. } = fx_mut(w, i) {
                if *slash >= 0 {
                    *slash += 1;
                } else {
                    *t += 1;
                }
            }
        }
        Fx::BbkFlare {
            alive: al, ended, ..
        } => {
            let gone = !ended && !alive(w, al);
            if let Fx::BbkFlare { t, ended, .. } = fx_mut(w, i) {
                *t += 1;
                if gone {
                    *ended = true;
                }
                // (removed at the flare's cycle end three ticks on: skill audit sa_f41_skill3, the game's flare to
                // f246 - its own tick is ours + 1, `view`)
                if *ended && (*t & 15) == 3 {
                    w.ents[i].removed = true;
                }
            }
        }
        Fx::FlameKame {
            owner,
            dir,
            n,
            mut links,
            alive: al,
            a,
            mult,
        } => {
            if !alive(w, al) {
                w.ents[i].removed = true;
                return;
            }
            let held = w.find(owner).and_then(|k| match &w.ents[k].body {
                Body::Player(p) => Some(w.input[p.slot as usize].held),
                _ => None,
            });
            let held = held.unwrap_or(0);
            let (dx, dy) = DIR_VEC[dir as usize];
            let perp = if dx != 0 {
                if held & input::UP != 0 {
                    -1
                } else if held & input::DOWN != 0 {
                    1
                } else {
                    0
                }
            } else if held & input::LEFT != 0 {
                -1
            } else if held & input::RIGHT != 0 {
                1
            } else {
                0
            };
            let me = w.ents[i].id;
            let n0 = links.len();
            // newest link first; a link spawned now updates from the next tick
            for k in (0..n0).rev() {
                let m = ((k + 1) * (k + 1)) as i32;
                links[k][2] += 1;
                links[k][3] = (-2 * m).max((2 * m).min(links[k][3] + perp * m));
                if links[k][2] == 8 && k == n0 - 1 && (links.len() as i32) < n {
                    let (x, y) = (links[k][0] + dx * 16, links[k][1] + dy * 16);
                    let b = crate::player::flame_link_box(w, FLAME_TABLE, x, y);
                    links.push([x, y, 0, 0, b.x0, b.y0, b.x1, b.y1]);
                }
                let l = links[k];
                let rect = Rect::new(l[4], l[5], l[6], l[7]);
                if let Fx::FlameKame { links: l0, .. } = fx_mut(w, i) {
                    *l0 = links.clone();
                }
                let stats = w.actor_by_id(owner).map(|x| x.stats).unwrap_or_default();
                let mut msg = HitMsg {
                    team: 1,
                    rect,
                    dir,
                    mult,
                    atk: a,
                    attacker: owner,
                    att_stats: stats,
                    att_is_player: true,
                    source: me,
                    crit: false,
                    raw: 0,
                    dmg: 0,
                    no_react: false,
                    foot: false,
                };
                if broadcast(w, &mut msg, true) {
                    let s = 48 + w.rng.rand(2);
                    w.sfx(s, 64);
                }
                // the box for the next tick: this tick's record at the shifted position
                let mut f = links[k][2] >> 2;
                if k as i32 == n - 1 {
                    if f >= 7 {
                        f = (f & 3) + 4;
                        if f == 7 {
                            f = 5;
                        }
                    }
                } else if f >= 4 {
                    f &= 3;
                    if f == 0 {
                        f = 2;
                    }
                }
                let sh = links[k][3] / 2;
                let (x, y) = (
                    links[k][0] + if dx != 0 { 0 } else { sh },
                    links[k][1] + if dx != 0 { sh } else { 0 },
                );
                let b = crate::player::flame_link_box(
                    w,
                    FLAME_TABLE + 0x54 * dir as u32 + 12 * f as u32,
                    x,
                    y,
                );
                links[k][4] = b.x0;
                links[k][5] = b.y0;
                links[k][6] = b.x1;
                links[k][7] = b.y1;
            }
            if let Fx::FlameKame { links: l0, .. } = fx_mut(w, i) {
                *l0 = links;
            }
        }
        Fx::ItMarkers { alive: al, .. } => {
            if let Fx::ItMarkers { t, .. } = fx_mut(w, i) {
                *t += 1;
            }
            if !alive(w, al) {
                w.ents[i].removed = true;
                crate::vram::release(w, crate::vram::Owner::Ent(w.ents[i].id));
            }
        }
        Fx::BigBangBoom { .. } => {
            let (nn, x0, y0, s) = match fx_mut(w, i) {
                Fx::BigBangBoom { n, x0, y0, s } => {
                    *n -= 1;
                    (*n, *x0, *y0, *s)
                }
                _ => unreachable!(),
            };
            if nn <= 0 {
                w.ents[i].removed = true;
                return;
            }
            if nn & 7 != 0 {
                return;
            }
            let x = x0 + w.rng.rand((s * 2) as u32);
            let y = y0 + w.rng.rand((s * 2) as u32);
            let rate = 0x1000 + w.rng.rand(0x1000);
            explosion(w, x, y, rate);
        }
    }
}
