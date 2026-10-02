//! Effect objects. They never damage, but they are world objects in the game (they take object slots, update in
//! the object order and some draw from the RNG), so the sim keeps them as entities with the game's lifetimes; the
//! renderer draws them from this state (record ids / addresses, positions, ticks).

use serde::{Deserialize, Serialize};
use crate::geom::{Dir, DIR_VEC};
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
    let pl = |p: EntityId| w.find(p).filter(|&i| matches!(w.ents[i].body, Body::Player(_))).map(|i| w.pl_ref(i));
    match a {
        Alive::Skill { p, serial } => pl(p).is_some_and(|p| p.sk.as_ref().is_some_and(|s| s.serial == serial)),
        Alive::SkillTickLt { p, serial, n } => pl(p).is_some_and(|p| p.sk.as_ref().is_some_and(|s| s.serial == serial) && p.ap.tick < n),
        Alive::SkillUncharged { p, serial } => pl(p).is_some_and(|p| p.sk.as_ref().is_some_and(|s| s.serial == serial && !s.charged)),
        Alive::Transform { p } => pl(p).is_some_and(|p| p.state == crate::player::PState::Transform),
        Alive::Beam { id } => w.find(id).is_some_and(|i| !w.ents[i].removed && !crate::shots::beam_done(w, i)),
        Alive::It { p, serial } => pl(p).is_some_and(|p| p.it_sel.as_ref().is_some_and(|s| s.serial == serial) || p.it.as_ref().is_some_and(|s| s.serial == serial)),
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
    Anim { id: u32, x: i32, y: i32, rec: i32, left: i32, rate: i32, acc: i32 },
    /// looping variant (Super Saiyan aura) until `alive` fails
    Loop { id: u32, x: i32, y: i32, first: i32, n: i32, rec: i32, rate: i32, acc: i32, alive: Alive },
    /// debris piece 0x0800C454: 32 ticks
    Debris { x: i32, y: i32, vx: i32, vy: i32, t: i32 },
    /// record object 0x080507A0 (`what` names the record rule for the renderer)
    RecObj { what: RecWhat, owner: EntityId, t: i32, alive: Alive },
    /// beam charge sparkle at owner + off[facing]
    BeamCharge { owner: EntityId, off: [(i32, i32); 4], set: BeamSet, t: i32 },
    /// Dragon Fist Explosion chain (6 segments, 1/256 px)
    DragonFist { seg: [[i32; 2]; 6], steer: (i32, i32), dir: Dir, dying: i32, t: i32, alive: Alive },
    EnergySword { owner: EntityId, slash: i32, t: i32, alive: Alive },
    BbkFlare { x: i32, y: i32, dir: Dir, t: i32, ended: bool, alive: Alive },
    /// Flame Kamehameha links [x, y, t, shift*2]
    FlameKame { owner: EntityId, dir: Dir, n: i32, links: Vec<[i32; 4]>, alive: Alive },
    ItMarkers { t: i32, alive: Alive },
    /// Big Bang explosion phase (state 2 of 0x08037650): 60 ticks, an explosion every 8th (RNG)
    BigBangBoom { n: i32, x0: i32, y0: i32, s: i32 },
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
        }
    }
}

pub fn add(w: &mut World, f: Fx) -> EntityId {
    w.add(Body::Fx(Box::new(f)))
}
pub fn fx_anim(w: &mut World, id: u32, x: i32, y: i32, start: i32, n: i32, rate: i32) -> EntityId {
    add(w, Fx::Anim { id, x, y, rec: start, left: n, rate, acc: 0 })
}
/// smoke puff 0x086FD9D0 (4 records)
pub fn puff(w: &mut World, x: i32, y: i32, rate: i32) {
    fx_anim(w, 0x086f_d9d0, x, y, 0, 4, rate);
}
/// puff with the game's random rate 0x1000 + rand(0x1000)
pub fn puff_rng(w: &mut World, x: i32, y: i32) {
    let r = 0x1000 + w.rng.rand(0x1000);
    puff(w, x, y, r);
}
/// explosion 0x08712FDC (4 records)
pub fn explosion(w: &mut World, x: i32, y: i32, rate: i32) {
    fx_anim(w, 0x0871_2fdc, x, y, 0, 4, rate);
}
pub fn beam_charge(w: &mut World, owner: EntityId, off: [(i32, i32); 4], set: BeamSet) -> EntityId {
    add(w, Fx::BeamCharge { owner, off, set, t: 0 })
}

const DF_ROOT: [(i32, i32); 4] = [(8, 16), (8, -32), (-8, -8), (24, -8)];
const DF_TABLE: u32 = 0x0877_ad10;
pub fn dragon_fist(owner_px: i32, owner_py: i32, dir: Dir, alive: Alive) -> Fx {
    let (dx, dy) = DIR_VEC[dir as usize];
    let (rx, ry) = DF_ROOT[dir as usize];
    let mut x = (owner_px + rx) << 8;
    let mut y = (owner_py + ry) << 8;
    let mut seg = [[0; 2]; 6];
    seg[5] = [x, y];
    for k in (0..=4).rev() {
        x += dx * (6 - k as i32) * 256;
        y += dy * (6 - k as i32) * 256;
        seg[k] = [x, y];
    }
    Fx::DragonFist { seg, steer: (0, 0), dir, dying: -1, t: 0, alive }
}

fn fx_mut(w: &mut World, i: usize) -> &mut Fx {
    match &mut w.ents[i].body {
        Body::Fx(f) => f,
        _ => panic!("not fx"),
    }
}

pub fn update(w: &mut World, i: usize) {
    let f = match &w.ents[i].body {
        Body::Fx(f) => (**f).clone(),
        _ => return,
    };
    match f {
        Fx::Anim { .. } => {
            if let Fx::Anim { rec, left, rate, acc, .. } = fx_mut(w, i) {
                *acc += *rate;
                let adv = *acc >> 16;
                if adv != 0 {
                    *rec += adv;
                    *left -= adv;
                    *acc &= 0xffff;
                    if *left <= 0 {
                        w.ents[i].removed = true;
                    }
                }
            }
        }
        Fx::Loop { alive: al, .. } => {
            if !alive(w, al) {
                w.ents[i].removed = true;
                return;
            }
            if let Fx::Loop { first, n, rec, rate, acc, .. } = fx_mut(w, i) {
                *acc += *rate;
                let adv = *acc >> 16;
                if adv != 0 {
                    *acc &= 0xffff;
                    *rec = *first + (*rec - *first + adv) % *n;
                }
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
                    w.ents[i].removed = true;
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
        Fx::DragonFist { mut seg, mut steer, dir, mut dying, mut t, alive: al } => {
            t += 1;
            if dying < 0 {
                if !alive(w, al) {
                    dying = 1;
                } else {
                    let (dx, dy) = DIR_VEC[dir as usize];
                    for (k, s) in seg.iter_mut().enumerate() {
                        let v = 0x60 + 0x20 * (5 - k as i32);
                        s[0] += dx * v;
                        s[1] += dy * v;
                    }
                    for k in 1..5 {
                        let mx = (seg[k - 1][0] + seg[k + 1][0]) >> 1;
                        let my = (seg[k - 1][1] + seg[k + 1][1]) >> 1;
                        seg[k][0] = (3 * seg[k][0] + mx) >> 2;
                        seg[k][1] = (3 * seg[k][1] + my) >> 2;
                    }
                    let held = w.input[0].held;
                    let sx = if held & input::LEFT != 0 { -3 } else if held & input::RIGHT != 0 { 3 } else { 0 };
                    let sy = if held & input::UP != 0 { -3 } else if held & input::DOWN != 0 { 3 } else { 0 };
                    let nx = (steer.0 + sx).clamp(-56, 56);
                    let ny = (steer.1 + sy).clamp(-32, 32);
                    seg[0][0] += (nx - steer.0) << 8;
                    seg[0][1] += (ny - steer.1) << 8;
                    steer = (nx, ny);
                }
            } else {
                dying += 1;
                if dying >= 24 {
                    for k in (0..6).rev() {
                        let rec = DF_TABLE + 12 * (dir as u32 * 4 + if k == 0 { 3 } else { 0 });
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
            }
            if let Fx::DragonFist { seg: s0, steer: s1, dying: d0, t: t0, .. } = fx_mut(w, i) {
                *s0 = seg;
                *s1 = steer;
                *d0 = dying;
                *t0 = t;
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
        Fx::BbkFlare { alive: al, ended, .. } => {
            let gone = !ended && !alive(w, al);
            if let Fx::BbkFlare { t, ended, .. } = fx_mut(w, i) {
                *t += 1;
                if gone {
                    *ended = true;
                }
                if *ended && ((*t >> 2) & 3) == 0 && (*t & 3) == 0 {
                    w.ents[i].removed = true;
                }
            }
        }
        Fx::FlameKame { alive: al, .. } => {
            if !alive(w, al) {
                w.ents[i].removed = true;
                return;
            }
            let held = w.input[0].held;
            if let Fx::FlameKame { dir, n, links, .. } = fx_mut(w, i) {
                let (dx, dy) = DIR_VEC[*dir as usize];
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
                let len = links.len();
                for k in 0..len {
                    let m = ((k + 1) * (k + 1)) as i32;
                    let l = &mut links[k];
                    l[2] += 1;
                    // shift kept doubled (the reference adds m / 2)
                    l[3] = (-2 * m).max((2 * m).min(l[3] + perp * m));
                    if l[2] == 8 && k == len - 1 && (len as i32) < *n {
                        let nl = [l[0] + dx * 16, l[1] + dy * 16, 0, 0];
                        links.push(nl);
                    }
                }
            }
        }
        Fx::ItMarkers { alive: al, .. } => {
            if let Fx::ItMarkers { t, .. } = fx_mut(w, i) {
                *t += 1;
            }
            if !alive(w, al) {
                w.ents[i].removed = true;
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
