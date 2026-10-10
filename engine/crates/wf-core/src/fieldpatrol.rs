//! Field enemies' own patrol steps: the def +0x18 step list, run by the runner at enemy+0x22C (created by the enemy
//! constructor `0x0800F68C`, never reset) only while the AI executes `RunMotionController` (`0x080299A3`: enter nop,
//! update = one runner pass, never done; exit = the current step's pause hook). Steps on enemies use the enemy layer
//! only (`vt+0x4C` mark rect, `vt+0x50` test, `vt+0x48` mark own foot; skipped / free while detached).
//! Spec: re/buu/notes.md "Field: NPC actors and behaviours" (field enemies), tools/field/npc_model.py
//! (`step_enemies`), data/field/npc_enemy_patrol*.json.

use crate::content::AnimRef;
use crate::fieldcontent::{Step, StepOp};
use crate::fieldnpc::{StepRt, Wander};
use crate::geom::Rect;
use crate::world::{EntityId, Layer, World};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Patrol {
    pub entity: EntityId,
    pub steps: Vec<Step>,
    pub idx: i32,
    pub cur: Option<StepRt>,
    /// height (obj+0x1A8, 22.10) moved by `cutscene_move`
    pub z: i32,
    /// heading (obj+0x1AC) the last `cutscene_move` set: the 3D map's draw faces the sprite by it
    #[serde(default)]
    pub h: i32,
}

/// the enemy constructor's runner (no step yet)
pub fn attach(w: &mut World, entity: EntityId, steps: &[Step]) {
    if steps.is_empty() {
        return;
    }
    // the enemy ctor 0x0800F68C puts type 141 (world-map encounters) at z = 60 px; 0-50's map event lowers the
    // ground ones to 0 (set_actor_field_1a8)
    let z = w
        .find(entity)
        .filter(|&k| matches!(&w.ents[k].body, crate::world::Body::Enemy(e) if e.ty == 141))
        .map(|_| 60 << 10)
        .unwrap_or(0);
    if let Some(f) = w.field.as_mut() {
        f.patrols.push(Patrol {
            entity,
            steps: steps.to_vec(),
            idx: -1,
            cur: None,
            z,
            h: 0,
        });
    }
}

pub fn has(w: &World, i: usize) -> bool {
    let id = w.ents[i].id;
    w.field
        .as_ref()
        .is_some_and(|f| f.patrols.iter().any(|p| p.entity == id))
}

fn take(w: &mut World, i: usize) -> Option<Patrol> {
    let id = w.ents[i].id;
    let f = w.field.as_mut()?;
    let k = f.patrols.iter().position(|p| p.entity == id)?;
    Some(f.patrols.remove(k))
}
fn put(w: &mut World, p: Patrol) {
    if let Some(f) = w.field.as_mut() {
        f.patrols.push(p);
        f.patrols.sort_by_key(|p| p.entity);
    }
}

fn foot(w: &World, i: usize) -> [i32; 4] {
    let a = &w.en_ref(i).a;
    [a.px(), a.py(), a.px() + a.foot_w, a.py() + a.foot_h]
}
fn grid(w: &World, i: usize) -> bool {
    w.col.is_some() && crate::enemy::attached(w, i)
}
/// vt+0x4C: mark a rect on the enemy layer
fn emark(w: &mut World, i: usize, r: [i32; 4], v: u8) {
    if grid(w, i) {
        w.col.as_mut().unwrap().mark_enemy(r, v);
    }
}
/// vt+0x48: mark the own footprint (keeps wf-core's mark bookkeeping in step)
fn emark_foot(w: &mut World, i: usize, v: u8) {
    if !grid(w, i) {
        return;
    }
    let f = foot(w, i);
    w.col.as_mut().unwrap().mark_enemy(f, v);
    w.en(i).marked = if v == 0 { Some(f) } else { None };
}
/// vt+0x50: the enemy layer free under a rect
fn etest(w: &World, i: usize, r: [i32; 4]) -> bool {
    !grid(w, i)
        || w.col
            .as_ref()
            .unwrap()
            .test(Layer::Enemy, r[0], r[1], r[2], r[3])
}
fn wtest(w: &World, r: [i32; 4]) -> bool {
    w.col
        .as_ref()
        .is_none_or(|c| c.test(Layer::Walk, r[0], r[1], r[2], r[3]))
}

/// SetAnim(k) (0 idle: tick = rand(255); 1 walk); `force` = even when already in it (vt+0x34 check otherwise)
fn anim(w: &mut World, i: usize, k: i32, force: bool) {
    if !force && w.en_ref(i).is_anim(k) {
        return;
    }
    crate::enemy::set_anim(w, i, &[AnimRef::N(k)], true, Some(true));
}

fn create(w: &mut World, i: usize, st: &Step, z: i32) -> StepRt {
    let wd = |k: usize| st.words.get(k).copied().unwrap_or(0);
    match st.func {
        0x0800_A26F => StepRt::Stand,
        0x0800_A223 => {
            anim(w, i, 0, false);
            w.en(i).a.facing = ((wd(0) >> 16) & 3) as u8;
            StepRt::Wait {
                count: (wd(0) & 0xFFFF) as i16 as i32,
            }
        }
        0x0800_A8E9 | 0x0800_A90F => {
            anim(w, i, if st.func == 0x0800_A8E9 { 0 } else { 1 }, true);
            StepRt::FacePlayer {
                count: wd(0) as i32,
            }
        }
        0x0800_A1D5 | 0x0800_A1AF => {
            let StepOp::WalkTo { x: tx, y: ty, .. } = st.op else {
                return StepRt::Other;
            };
            let a = &w.en_ref(i).a;
            let (dx, dy) = ((tx << 8) - a.x, (ty << 8) - a.y);
            let (d, major) = if dx.abs() >= dy.abs() {
                (if dx >= 0 { 3 } else { 2 }, dx.abs())
            } else {
                (if dy >= 0 { 0 } else { 1 }, dy.abs())
            };
            let spd = (wd(1) & 0xFFFF) as i64;
            let mut cnt = if spd == 0 {
                0
            } else {
                ((((major as i64) << 8) / spd) >> 8) as i32 & 0xFFFF
            };
            w.en(i).a.facing = d;
            let (vx, vy) = if cnt != 0 {
                (
                    (((dx as i64) << 8) / ((cnt as i64) << 8)) as i32,
                    (((dy as i64) << 8) / ((cnt as i64) << 8)) as i32,
                )
            } else {
                cnt = 1;
                (0, 0)
            };
            anim(w, i, 1, true);
            StepRt::WalkTo {
                tx,
                ty,
                dir: d,
                n: cnt,
                vx,
                vy,
            }
        }
        0x0800_A82D | 0x0800_A85F => {
            anim(w, i, 0, true);
            StepRt::Wander(wander_rt(st))
        }
        // 0x08039F09 -> 0x08039D98: +4 s16 tx, +6 ty, +8 tz, +0xA u16 speed; n = (max |d| << 8) / speed (>= 1)
        0x0803_9F09 => {
            let (tx, ty) = ((wd(0) & 0xFFFF) as i16 as i32, (wd(0) >> 16) as i16 as i32);
            let (tz, spd) = ((wd(1) & 0xFFFF) as i16 as i32, (wd(1) >> 16) as i32);
            let a = &w.en_ref(i).a;
            let (dx, dy, dz) = (tx - a.px(), ty - a.py(), tz - (z >> 10));
            let m = dx.abs().max(dy.abs()).max(dz.abs()) as i64;
            let n = if spd > 0 {
                ((m << 8) / spd as i64) as i32
            } else {
                1
            }
            .max(1);
            // 0x08039E76: atan2 in doubles, then floats: * 1/(2 pi) (0x3E22F984) * 1024.0, truncated
            let ang =
                ((dy as f64).atan2(dx as f64) as f32) * f32::from_bits(0x3E22_F984) * 1024.0f32;
            StepRt::Move3 {
                tx,
                ty,
                tz,
                n,
                vx: (dx << 8) / n,
                vy: (dy << 8) / n,
                vz: (dz << 10) / n,
                h: ang as i32,
            }
        }
        // 0x0803A053: +4 s16 centre x, +6 y, +8 s16 radius (and (radius + 0x180)^2, not read by the update)
        0x0803_A053 => StepRt::Ring {
            cx: (wd(0) & 0xFFFF) as i16 as i32,
            cy: (wd(0) >> 16) as i16 as i32,
            r: (wd(1) & 0xFFFF) as i16 as i32,
        },
        _ => StepRt::Other,
    }
}

/// a wander step's runtime as its create makes it (0x0800A82D / 0x0800A85F; no progress yet) - the importer fills in a
/// wander under way (`patrol_wander_import`)
pub fn wander_rt(st: &Step) -> Box<Wander> {
    let wd = |k: usize| st.words.get(k).copied().unwrap_or(0);
    let spd = wd(0) as i32;
    let r = ((spd << 5) + 0x7FF) & !0x7FF;
    let rect = (st.func == 0x0800_A85F).then(|| {
        Rect::new(
            (wd(1) & 0xFFFF) as i32,
            (wd(1) >> 16) as i32,
            (wd(2) & 0xFFFF) as i32,
            (wd(2) >> 16) as i32,
        )
    });
    let chance = if rect.is_some() {
        (spd as i64) << 7
    } else {
        (spd as i64) << 4
    };
    Box::new(Wander {
        spd,
        dist: r >> 8,
        frames: if spd > 0 { (r + spd - 1) / spd } else { 0 },
        left: 0,
        chance,
        talk: rect.is_none(),
        rect,
        old: [0; 4],
        new: [0; 4],
        vx: 0,
        vy: 0,
        tx: 0,
        ty: 0,
        dir: 0,
    })
}

/// the importer: a wander step under way on a patrol (the runner's step object, enemy +0x22C -> vt 0x0804B318: +8
/// frames left, +0x18 / +0x1C 24.8 velocity, +0x20 / +0x24 target px, +0x28 the footprint it left, +0x38 the one it
/// walks to) - the runner's current step, the step index the first wander step of the list
#[allow(clippy::too_many_arguments)]
pub fn import_wander(w: &mut World, i: usize, left: i32, v: (i32, i32), t: (i32, i32), old: [i32; 4], new: [i32; 4]) -> bool {
    let id = w.ents[i].id;
    let dir = w.en_ref(i).a.facing;
    let (ax, ay) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    let Some(f) = w.field.as_mut() else { return false };
    let Some(p) = f.patrols.iter_mut().find(|p| p.entity == id) else { return false };
    let Some(k) = p.steps.iter().position(|s| matches!(s.func, 0x0800_A82D | 0x0800_A85F)) else { return false };
    let mut wr = wander_rt(&p.steps[k]);
    wr.left = left;
    (wr.vx, wr.vy) = v;
    (wr.tx, wr.ty) = t;
    wr.old = old;
    wr.new = new;
    wr.dir = dir;
    p.idx = k as i32;
    // (only a walk in progress: the position on the step's line, `left` steps of `v` short of the target - a step
    // object left over while the AI chases (kais_hfil9's other monster: left 2, the chase moving it elsewhere) holds
    // nothing; the enemy marks its footprint)
    let on_line = ax + wr.left * wr.vx == wr.tx << 8 && ay + wr.left * wr.vy == wr.ty << 8;
    let held = wr.left != 0 && wr.rect.is_none() && on_line;
    p.cur = Some(StepRt::Wander(wr));
    // the walk reserves its target's footprint on the enemy grid at its start and holds it to the end (`holds_mark`;
    // HFIL tour kais_hfil9: a monster walking from y 211.5 to 224 holds tiles 45-46 row 28 all the way)
    if held && w.en_ref(i).marked.is_some() {
        w.en(i).marked = Some(new);
    }
    true
}

fn update_step(w: &mut World, i: usize, s: &mut StepRt, z: &mut i32) -> bool {
    match s {
        StepRt::Stand | StepRt::Other => false,
        // update 0x08039F4E: nothing without a leader; height = the leader's (obj+0x1A8); d = leader px - centre,
        // position = centre + radius * d / isqrt(|d|^2) (signed integer divisions), whole pixels; never done.
        // (The heading obj+0x1AC by d's octant is set by the caller, `run`.)
        StepRt::Ring { cx, cy, r } => {
            let Some(pi) = w.player_idx(0) else {
                return false;
            };
            let (px, py) = (w.pl_ref(pi).a.x >> 8, w.pl_ref(pi).a.y >> 8);
            if let Some(fz) = w
                .field
                .as_ref()
                .and_then(|f| f.flight.as_ref())
                .map(|fl| fl.z)
            {
                *z = fz;
            }
            let (dx, dy) = (px - *cx, py - *cy);
            let d =
                crate::fixed::isqrt((dx as i64) * (dx as i64) + (dy as i64) * (dy as i64)) as i32;
            if d != 0 {
                let a = &mut w.en(i).a;
                a.x = (*cx + *r * dx / d) << 8;
                a.y = (*cy + *r * dy / d) << 8;
            }
            false
        }
        StepRt::Move3 {
            tx,
            ty,
            tz,
            n,
            vx,
            vy,
            vz,
            ..
        } => {
            *n -= 1;
            let a = &mut w.en(i).a;
            if *n <= 0 {
                a.x = *tx << 8;
                a.y = *ty << 8;
                *z = *tz << 10;
                return true;
            }
            a.x += *vx;
            a.y += *vy;
            *z += *vz;
            false
        }
        StepRt::Wait { count } => {
            *count -= 1;
            *count < 0
        }
        StepRt::FacePlayer { count } => {
            if let Some(pi) = crate::enemy::player(w, i) {
                let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
                let a = &mut w.en(i).a;
                let (dx, dy) = (px - a.x, py - a.y);
                a.facing = if dx.abs() >= dy.abs() {
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
            }
            *count -= 1;
            *count < 0
        }
        // 0x0800A01C
        StepRt::WalkTo {
            tx,
            ty,
            dir,
            n: cnt,
            vx,
            vy,
        } => {
            let old = foot(w, i);
            {
                let a = &mut w.en(i).a;
                a.x += *vx;
                a.y += *vy;
            }
            let new = foot(w, i);
            emark(w, i, old, 1);
            if !etest(w, i, new) {
                emark(w, i, old, 0);
                anim(w, i, 0, false);
                let a = &mut w.en(i).a;
                a.x -= *vx;
                a.y -= *vy;
                return false;
            }
            anim(w, i, 1, false);
            w.en(i).a.facing = *dir;
            emark(w, i, new, 0);
            if grid(w, i) {
                w.en(i).marked = Some(new);
            }
            *cnt = (*cnt - 1) & 0xFFFF;
            if *cnt == 0 {
                emark_foot(w, i, 1);
                {
                    let a = &mut w.en(i).a;
                    a.x = *tx << 8;
                    a.y = *ty << 8;
                }
                emark_foot(w, i, 0);
                return true;
            }
            false
        }
        StepRt::Wander(s) => {
            wander(w, i, s);
            false
        }
    }
}

/// the wander base: enemy variant (enemy-layer marks, `talk` = true here) or the plain rect variant (walk test)
fn wander(w: &mut World, i: usize, s: &mut Wander) {
    if s.left == 0 && w.rng.chance(s.chance) {
        s.old = foot(w, i);
        let d = w.rng.rand(5);
        if d < 4 {
            w.en(i).a.facing = d as u8;
        }
        let a = &w.en_ref(i).a;
        let (sx, sy) = (a.px(), a.py());
        let (sp, ds) = (s.spd, s.dist);
        let v = match a.facing {
            0 => (0, sp, 0, ds),
            1 => (0, -sp, 0, -ds),
            2 => (-sp, 0, -ds, 0),
            _ => (sp, 0, ds, 0),
        };
        s.vx = v.0;
        s.vy = v.1;
        s.tx = sx + v.2;
        s.ty = sy + v.3;
        let o = s.old;
        s.new = [o[0] + v.2, o[1] + v.3, o[2] + v.2, o[3] + v.3];
        let new = s.new;
        let ok = match s.rect {
            Some(r) => {
                !(r.x0 > new[0] || r.y0 > new[1] || r.x1 < new[2] || r.y1 < new[3]) && wtest(w, new)
            }
            None => {
                emark(w, i, o, 1);
                let ok = etest(w, i, new);
                emark(w, i, if ok { new } else { o }, 0);
                // the step's mark is its destination until it ends (`holds_mark`: no per-frame foot mark on the
                // way - Kyodai Pyramid 14-10, the game's enemy grid during a Mummy's step)
                if ok && grid(w, i) {
                    w.en(i).marked = Some(new);
                }
                ok
            }
        };
        if ok {
            s.left = s.frames;
            s.dir = w.en_ref(i).a.facing;
            anim(w, i, 1, false);
        }
    }
    if s.left != 0 {
        {
            let a = &mut w.en(i).a;
            a.x += s.vx;
            a.y += s.vy;
            a.facing = s.dir;
        }
        s.left -= 1;
        if s.left == 0 {
            {
                let a = &mut w.en(i).a;
                a.x = s.tx << 8;
                a.y = s.ty << 8;
            }
            wander_end(w, i, s);
            anim(w, i, 0, false);
        } else {
            anim(w, i, 1, false);
        }
    }
}

fn wander_end(w: &mut World, i: usize, s: &Wander) {
    if s.rect.is_none() {
        emark(w, i, s.new, 1);
        emark_foot(w, i, 0);
    }
}

/// a wander step under way whose destination rect is the enemy's mark (set at the step's start, replaced by the foot
/// mark at its end / pause): the per-frame mark sync leaves it alone
pub fn holds_mark(w: &World, i: usize) -> bool {
    let id = w.ents[i].id;
    let Some(p) = w
        .field
        .as_ref()
        .and_then(|f| f.patrols.iter().find(|p| p.entity == id))
    else {
        return false;
    };
    matches!(&p.cur, Some(StepRt::Wander(s)) if s.left != 0 && s.rect.is_none() && w.en_ref(i).marked == Some(s.new))
}

/// the enemy where a rect wander (the plain variant, object vt 0x0804D848: the walk test only) put it - a move that
/// writes the position alone, the enemy grid untouched
pub fn pos_only(w: &World, i: usize) -> bool {
    let id = w.ents[i].id;
    let Some(p) = w.field.as_ref().and_then(|f| f.patrols.iter().find(|p| p.entity == id)) else {
        return false;
    };
    let a = &w.en_ref(i).a;
    matches!(&p.cur, Some(StepRt::Wander(s)) if s.rect.is_some() && a.x + s.left * s.vx == s.tx << 8 && a.y + s.left * s.vy == s.ty << 8)
}

/// RunMotionController update: one runner pass (`0x0800A95A`)
pub fn run(w: &mut World, i: usize) {
    let Some(mut p) = take(w, i) else { return };
    let n = p.steps.len();
    for _ in 0..64 {
        if let Some(mut cur) = p.cur.take() {
            let done = update_step(w, i, &mut cur, &mut p.z);
            if let StepRt::Ring { cx, cy, .. } = cur {
                // the marker's heading (0x08039FEC ..): by the octant of leader - centre
                if let Some(pi) = w.player_idx(0) {
                    let (dx, dy) = ((w.pl_ref(pi).a.x >> 8) - cx, (w.pl_ref(pi).a.y >> 8) - cy);
                    p.h = if dx.abs() > dy.abs() {
                        if dx > 0 {
                            if dy > 0 {
                                0x50
                            } else {
                                0x30
                            }
                        } else if dy > 0 {
                            0xB0
                        } else {
                            0xD0
                        }
                    } else if dy > 0 {
                        if dx > 0 {
                            0x70
                        } else {
                            0x90
                        }
                    } else if dx > 0 {
                        0x10
                    } else {
                        0xF0
                    };
                }
            }
            p.cur = Some(cur);
            if !done {
                break;
            }
        }
        p.idx += 1;
        if p.idx as usize >= n {
            p.idx = 0;
        }
        let st = p.steps[p.idx as usize].clone();
        let c = create(w, i, &st, p.z);
        if let StepRt::Move3 { h, .. } = c {
            p.h = h;
        }
        p.cur = Some(c);
    }
    put(w, p);
}

/// RunMotionController exit (`0x08029988`): the step's pause hook (wander: the end marks if moving; left kept)
pub fn pause(w: &mut World, i: usize) {
    let Some(p) = take(w, i) else { return };
    if let Some(StepRt::Wander(s)) = &p.cur {
        if s.left != 0 {
            let s = s.clone();
            wander_end(w, i, &s);
        }
    }
    put(w, p);
}

/// runner index (the recordings' `runner_index`: 255 before the first step)
pub fn index(w: &World, i: usize) -> Option<i32> {
    let id = w.ents[i].id;
    w.field
        .as_ref()?
        .patrols
        .iter()
        .find(|p| p.entity == id)
        .map(|p| p.idx)
}

/// an enemy's height obj+0x1A8 (22.10; the runner keeps it, 0 without one)
pub fn z_of(w: &World, i: usize) -> i32 {
    let id = w.ents[i].id;
    w.field
        .as_ref()
        .and_then(|f| f.patrols.iter().find(|p| p.entity == id))
        .map(|p| p.z)
        .unwrap_or(0)
}

/// `set_actor_field_1a8(actor, v)` (native 0xF0): obj+0x1A8 = v << 10 (positive = up)
pub fn set_z(w: &mut World, i: usize, z: i32) {
    let id = w.ents[i].id;
    if let Some(p) = w
        .field
        .as_mut()
        .and_then(|f| f.patrols.iter_mut().find(|p| p.entity == id))
    {
        p.z = z;
    }
}

/// the heading (obj+0x1AC) of a patrolling enemy
pub fn heading(w: &World, i: usize) -> i32 {
    let id = w.ents[i].id;
    w.field
        .as_ref()
        .and_then(|f| f.patrols.iter().find(|p| p.entity == id))
        .map(|p| p.h)
        .unwrap_or(0)
}
