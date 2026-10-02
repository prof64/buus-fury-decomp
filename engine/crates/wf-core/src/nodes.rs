//! The AI interpreter's execution: composites (Sequence / Selector / Parallel / the shared conditional decorator
//! 0x080342A9), the action nodes, the on-hit trees, and the AI stack operations. Node for node the reference
//! interpreter (site/src/battle/rulesets/buu/ai.ts); INFERRED nodes there are INFERRED here (see `NODE_NOTES`).

use crate::ai::{new_rt, BehTree, NodeRt, PathRt, StackEntry, K, P};
use crate::content::AnimRef;
use crate::damage::atk;
use crate::enemy::{self as en, set_anim, KnockState, ShotType};
use crate::fixed::{round_div_sqrt, HOP_ARC8};
use crate::geom::{facing_toward, overlaps, Rect, DIR_VEC};
use crate::player::{self, HoldKind};
use crate::shots;
use crate::tables::{DRIFT_COS, DRIFT_SIN, RING24, RING40};
use crate::world::{Body, HitMsg, World};
use std::sync::Arc;

/// nodes whose details are inferred in the reference (tables not decoded, cosmetic nodes)
pub const NODE_NOTES: &[(&str, &str)] = &[
    ("IfPlayerInZone", "sniper zone rectangle not decoded: true within 160 px"),
    ("IfPlayerInRoomZone", "vampire lair region not decoded: always true"),
    ("MoveToFormationA", "formation offset tables 0x0877AE80/0x0877AECC not decoded: ring of 8 slots at 40/24 px"),
    ("TankDrive", "turn helpers not decoded: axis-aligned drive toward the player at 0.5 px/frame"),
    ("EnergyBarrage", "barrage flight path not decoded: short 4 px/frame shots, 12 frames"),
    ("IfUnknown", "condition 0x0804EE25 not decoded: false"),
    ("ShockStun", "electric-shock stun length not decoded: 48 frames"),
    ("VolleyballLaunch", "launched ball flight not decoded: 4 px/frame for 30 frames, hits other enemies"),
    ("InlineScript", "bytecode not run (scripted escape / cutscene): the wf-script seam"),
    ("RunScript", "cutscene scripts are not run: the wf-script seam"),
];

/// where a node's runtime lives: enemy entity index, AI stack level and that entry's serial
#[derive(Clone)]
struct C {
    i: usize,
    lvl: usize,
    serial: u32,
    tree: Arc<BehTree>,
}

fn rt<'a>(w: &'a mut World, c: &C, n: u16) -> Option<&'a mut NodeRt> {
    match &mut w.ents[c.i].body {
        Body::Enemy(e) => e.ai.stack.get_mut(c.lvl).filter(|s| s.serial == c.serial).map(|s| &mut s.rt[n as usize]),
        _ => None,
    }
}
fn gen(w: &World, i: usize) -> u32 {
    w.en_ref(i).ai.gen
}
fn kids(c: &C, n: u16) -> &[u16] {
    &c.tree.nodes[n as usize].kids
}
fn kind(c: &C, n: u16) -> K {
    c.tree.nodes[n as usize].k
}
fn par(c: &C, n: u16, p: P, d: i64) -> i64 {
    c.tree.nodes[n as usize].p.get(p, d)
}
fn an(v: i32) -> AnimRef {
    AnimRef::N(v)
}
fn nm(s: &'static str) -> AnimRef {
    AnimRef::Name(s)
}

// ---- stack operations --------------------------------------------------------------------------------------------

fn ctx_top(w: &World, i: usize) -> Option<C> {
    let e = w.en_ref(i);
    let lvl = e.ai.stack.len().checked_sub(1)?;
    let s = &e.ai.stack[lvl];
    Some(C { i, lvl, serial: s.serial, tree: Arc::clone(&s.tree) })
}
fn build(w: &mut World, id: u16) -> StackEntry {
    let tree = w.data.states.get(&id).cloned().unwrap_or_else(|| Arc::new(crate::ai::compile_behaviour(id, &Default::default())));
    let serial = w.next_serial();
    StackEntry { state: id, serial, rt: new_rt(&tree), tree }
}
/// (the reference's history log; the state stamp obj+0x210 is only written by the Block node's exit)
fn note(_w: &mut World, _i: usize) {}
pub fn push(w: &mut World, i: usize, id: u16) {
    if w.en_ref(i).ai.stack.len() >= 8 {
        return;
    }
    w.en(i).ai.gen += 1;
    let s = build(w, id);
    w.en(i).ai.stack.push(s);
    note(w, i);
    if let Some(c) = ctx_top(w, i) {
        enter(w, &c, 0);
    }
}
pub fn replace_top(w: &mut World, i: usize, id: u16) {
    if w.en_ref(i).ai.stack.is_empty() {
        return push(w, i, id);
    }
    w.en(i).ai.gen += 1;
    if let Some(c) = ctx_top(w, i) {
        exit(w, &c, 0);
    }
    let s = build(w, id);
    if let Some(last) = w.en(i).ai.stack.last_mut() {
        *last = s;
    }
    note(w, i);
    if let Some(c) = ctx_top(w, i) {
        enter(w, &c, 0);
    }
}
pub fn pop(w: &mut World, i: usize) {
    if w.en_ref(i).ai.stack.len() <= 1 {
        return;
    }
    w.en(i).ai.gen += 1;
    if let Some(c) = ctx_top(w, i) {
        exit(w, &c, 0);
        let e = w.en(i);
        if e.ai.stack.last().is_some_and(|s| s.serial == c.serial) {
            e.ai.stack.pop();
        }
    }
    note(w, i);
}
pub fn reset(w: &mut World, i: usize, id: u16) {
    while let Some(c) = ctx_top(w, i) {
        exit(w, &c, 0);
        let e = w.en(i);
        if e.ai.stack.last().is_some_and(|s| s.serial == c.serial) {
            e.ai.stack.pop();
        } else {
            break;
        }
    }
    w.en(i).ai.stack.clear();
    push(w, i, id);
}
/// one AI frame: the top state's tree (0x0800AB0C ignores the root's result: a finished tree is not restarted, its
/// composites keep their state)
pub fn tick(w: &mut World, i: usize) {
    let Some(c) = ctx_top(w, i) else { return };
    update(w, &c, 0);
}

// ---- composites ----------------------------------------------------------------------------------------------------

fn is_cond(k: K) -> bool {
    use K::*;
    matches!(
        k,
        Chance | IfPlayerWithin | IfPlayerWithin2 | IfPlayerFarther | IfPlayerInMeleeRange | IfPlayerAttackingMe | IfInPlayerBeamLine
            | IfTimeInState | IfCooldownOver | IfCooldown2Over | IfCounterEq | IfGlobalFlag | IfBuuPacified | IfAxisAligned | IfHpBelow
            | IfPlayerInFront | IfPlayerInZone | IfPlayerInRoomZone | IfPlayerBlastIncoming | IfPlayerInLineOfFire | IfContactHit
            | IfMeleeHit | IfContactExplode | IfLandedOnRock | IfPlayerPetrified | IfPlayerNotPetrified | IfSphereDestroyed
            | IfPlayerNearAndHittable | IfLastKidBuu | IfChildCount | IfLatchOverlap | IfButtonMashed | IfPlayerOnTop
            | IfPartnerHolding | IfUnknown
    )
}

fn enter(w: &mut World, c: &C, n: u16) {
    match kind(c, n) {
        K::Sequence => {
            if let Some(r) = rt(w, c, n) {
                r.i = 0;
            }
            if let Some(&k0) = kids(c, n).first() {
                enter(w, c, k0);
            }
        }
        K::Selector => {
            for &k in kids(c, n).to_vec().iter() {
                enter(w, c, k);
            }
        }
        K::ParallelAll | K::ParallelAny => {
            if let Some(r) = rt(w, c, n) {
                r.done = 0;
            }
            for &k in kids(c, n).to_vec().iter() {
                enter(w, c, k);
            }
        }
        k if is_cond(k) => {
            if let Some(r) = rt(w, c, n) {
                r.on = false;
            }
            if k == K::IfHpBelow {
                let e = w.en_ref(c.i);
                let thr = (par(c, n, P::Frac, 0) * e.max_hp1024).div_euclid(65536);
                if let Some(r) = rt(w, c, n) {
                    // threshold kept in 64 bits across two scratch words
                    r.d[1] = (thr >> 32) as i32;
                    r.d[2] = thr as u32 as i32;
                }
            }
        }
        _ => {
            let Some(r) = rt(w, c, n) else { return };
            r.f = 0;
            r.d = [0; 8];
            let mut r = *r;
            leaf_enter(w, c, n, &mut r);
            if let Some(x) = rt(w, c, n) {
                *x = r;
            }
        }
    }
}

fn exit(w: &mut World, c: &C, n: u16) {
    match kind(c, n) {
        K::Sequence => {
            let i = rt(w, c, n).map(|r| r.i).unwrap_or(0) as usize;
            let ks = kids(c, n).to_vec();
            if i < ks.len() {
                exit(w, c, ks[i]);
            }
            if let Some(r) = rt(w, c, n) {
                r.i = ks.len() as i32;
            }
        }
        K::Selector => {
            for &k in kids(c, n).to_vec().iter() {
                exit(w, c, k);
            }
        }
        K::ParallelAll | K::ParallelAny => {
            let done = rt(w, c, n).map(|r| r.done).unwrap_or(0);
            let ks = kids(c, n).to_vec();
            for (j, &k) in ks.iter().enumerate() {
                if done & (1 << j) == 0 {
                    exit(w, c, k);
                }
            }
            if let Some(r) = rt(w, c, n) {
                r.done = if ks.len() >= 64 { u64::MAX } else { (1u64 << ks.len()) - 1 };
            }
        }
        k if is_cond(k) => {
            let on = rt(w, c, n).map(|r| r.on).unwrap_or(false);
            if on {
                if let Some(&k0) = kids(c, n).first() {
                    exit(w, c, k0);
                }
            }
            if let Some(r) = rt(w, c, n) {
                r.on = false;
            }
        }
        _ => {
            let Some(r) = rt(w, c, n) else { return };
            let mut r = *r;
            leaf_exit(w, c, n, &mut r);
            if let Some(x) = rt(w, c, n) {
                *x = r;
            }
        }
    }
}

fn update(w: &mut World, c: &C, n: u16) -> i32 {
    let i = c.i;
    match kind(c, n) {
        K::Sequence => {
            let ks = kids(c, n).to_vec();
            let Some(cur) = rt(w, c, n).map(|r| r.i as usize) else { return 0 };
            // 0x08018F0C: after the last child the index wraps to 0 without entering the first child again
            if cur >= ks.len() {
                if let Some(x) = rt(w, c, n) {
                    x.i = 0;
                }
                return 1;
            }
            let g = gen(w, i);
            let r = update(w, c, ks[cur]);
            if g != gen(w, i) || r == 0 {
                return 0;
            }
            exit(w, c, ks[cur]);
            let Some(x) = rt(w, c, n) else { return 0 };
            x.i += 1;
            let ni = x.i as usize;
            if ni >= ks.len() {
                x.i = 0;
                return 1;
            }
            enter(w, c, ks[ni]);
            0
        }
        K::Selector => {
            let g = gen(w, i);
            for &k in kids(c, n).to_vec().iter() {
                let r = update(w, c, k);
                if g != gen(w, i) {
                    return 0;
                }
                if r != 0 {
                    return 1;
                }
            }
            0
        }
        K::ParallelAll | K::ParallelAny => {
            let any = kind(c, n) == K::ParallelAny;
            let g = gen(w, i);
            let ks = kids(c, n).to_vec();
            for (j, &k) in ks.iter().enumerate() {
                if rt(w, c, n).map(|r| r.done & (1 << j) != 0).unwrap_or(true) {
                    continue;
                }
                let r = update(w, c, k);
                if g != gen(w, i) {
                    return 0;
                }
                if r != 0 {
                    if let Some(x) = rt(w, c, n) {
                        x.done |= 1 << j;
                    }
                    exit(w, c, k);
                    if any {
                        return 1;
                    }
                }
            }
            let all = if ks.len() >= 64 { u64::MAX } else { (1u64 << ks.len()) - 1 };
            (rt(w, c, n).map(|r| r.done & all == all).unwrap_or(false)) as i32
        }
        k if is_cond(k) => {
            if cond(w, c, n) {
                let Some(&kid) = kids(c, n).first() else { return 1 };
                let on = rt(w, c, n).map(|r| r.on).unwrap_or(true);
                if !on {
                    enter(w, c, kid);
                    if let Some(x) = rt(w, c, n) {
                        x.on = true;
                    }
                }
                let g = gen(w, i);
                let r = update(w, c, kid);
                if g != gen(w, i) {
                    return 0;
                }
                if r != 0 {
                    exit(w, c, kid);
                    if let Some(x) = rt(w, c, n) {
                        x.on = false;
                    }
                }
                return r;
            }
            let on = rt(w, c, n).map(|r| r.on).unwrap_or(false);
            if on {
                if let Some(&kid) = kids(c, n).first() {
                    exit(w, c, kid);
                }
            }
            if let Some(x) = rt(w, c, n) {
                x.on = false;
            }
            0
        }
        _ => {
            let Some(r) = rt(w, c, n) else { return 0 };
            r.f += 1;
            let mut r = *r;
            let f = r.f;
            let done = leaf_update(w, c, n, f, &mut r);
            if let Some(x) = rt(w, c, n) {
                *x = r;
            }
            done as i32
        }
    }
}

// ---- conditions ------------------------------------------------------------------------------------------------------

fn cond(w: &mut World, c: &C, n: u16) -> bool {
    let i = c.i;
    let frame = w.frame as i64;
    match kind(c, n) {
        K::Chance => {
            let p = par(c, n, P::Prob, 0);
            w.rng.chance(p)
        }
        K::IfPlayerWithin | K::IfPlayerWithin2 => en::dist2(w, i) <= par(c, n, P::R2, 0),
        K::IfPlayerFarther => en::player(w, i).is_some() && en::dist2(w, i) > par(c, n, P::R2, 0),
        K::IfPlayerInMeleeRange => en::in_melee_range(w, i),
        K::IfPlayerAttackingMe => en::player_attacking_me(w, i),
        K::IfInPlayerBeamLine => en::in_player_beam_line(w, i),
        K::IfTimeInState => frame - w.en_ref(i).state_stamp > par(c, n, P::Frames, 0),
        K::IfCooldownOver => frame - w.en_ref(i).mark1 > par(c, n, P::Frames, 0),
        K::IfCooldown2Over => frame - w.en_ref(i).mark2 > par(c, n, P::Frames, 0),
        K::IfCounterEq => w.en_ref(i).counter == par(c, n, P::N, 0),
        K::IfGlobalFlag => w.g.candy > 0,
        K::IfBuuPacified => w.g.buu_pacified != 0,
        K::IfAxisAligned => {
            let Some(pi) = en::player(w, i) else { return false };
            if !w.pl_ref(pi).a.hittable {
                return false;
            }
            let (dx, dy) = en::to_player(w, i);
            dx.abs() < 16 || dy.abs() < 16
        }
        K::IfHpBelow => {
            let thr = rt(w, c, n).map(|r| ((r.d[1] as i64) << 32) | (r.d[2] as u32 as i64)).unwrap_or(0);
            w.en_ref(i).hp1024 < thr
        }
        K::IfPlayerInFront => en::player_in_front(w, i, 48),
        K::IfPlayerInZone => en::dist2(w, i) <= 160 * 160,
        K::IfPlayerInRoomZone => en::player(w, i).is_some(),
        K::IfPlayerBlastIncoming => en::blast_incoming(w, i),
        K::IfPlayerInLineOfFire => {
            let (dx, dy) = en::to_player(w, i);
            let (fx, fy) = DIR_VEC[w.en_ref(i).a.facing as usize];
            if fx != 0 {
                dx.signum() == fx && dy.abs() < 16
            } else {
                dy.signum() == fy && dx.abs() < 16
            }
        }
        K::IfContactHit | K::IfMeleeHit => en::melee_strike(w, i, 1024, false, false, atk(2)),
        K::IfContactExplode => en::contact_explode(w, i),
        K::IfLandedOnRock => en::land_on_rock(w, i),
        K::IfPlayerPetrified => en::player(w, i).is_some_and(|pi| w.pl_ref(pi).is_stone()),
        K::IfPlayerNotPetrified => en::player(w, i).is_some_and(|pi| !w.pl_ref(pi).is_stone()),
        K::IfSphereDestroyed => w.g.sphere_state == 1,
        K::IfPlayerNearAndHittable => {
            let Some(pi) = en::player(w, i) else { return false };
            if !w.pl_ref(pi).a.hittable {
                return false;
            }
            let (dx, dy) = en::to_player(w, i);
            dx.abs() <= 47 && dy.abs() <= 47
        }
        K::IfLastKidBuu => w.live_enemies().iter().filter(|&&o| matches!(w.en_ref(o).ty, 0x3c | 0x3d)).count() == 1,
        K::IfChildCount => {
            let kids = w.en_ref(i).children.clone();
            let live = kids.iter().filter(|&&k| w.enemy_by_id(k).is_some_and(|(ki, e)| !w.ents[ki].removed && !e.defeated)).count();
            live as i64 == par(c, n, P::N, 0)
        }
        K::IfLatchOverlap => {
            let Some(pi) = en::player(w, i) else { return false };
            let p = w.pl_ref(pi);
            let e = &w.en_ref(i).a;
            if p.held_by != 0 || e.facing != 0 {
                return false;
            }
            let pb = p.a.body_box();
            let eb = e.body_box();
            overlaps(&Rect::new(pb.x0 + 6, pb.y0, pb.x1 - 6, pb.y1), &Rect::new(eb.x0, eb.y1 - 8, eb.x0 + 7, eb.y1 - 1))
        }
        K::IfButtonMashed => en::player(w, i).is_some_and(|pi| w.pl_ref(pi).held_by != w.ents[i].id),
        K::IfPlayerOnTop => {
            let (dx, dy) = en::to_player(w, i);
            dx.abs() < 16 && dy.abs() < 16
        }
        K::IfPartnerHolding => {
            let held = en::ctx_player(w, i).map(|pi| w.pl_ref(pi).held_by).unwrap_or(0);
            held != 0 && w.live_enemies().iter().any(|&o| matches!(w.en_ref(o).ty, 0x6d..=0x70) && w.ents[o].id == held)
        }
        _ => false,
    }
}

// ---- actions ---------------------------------------------------------------------------------------------------------

fn alive(w: &World, id: i32) -> bool {
    id != 0 && w.alive(id as u32)
}

fn timed_enter(w: &mut World, i: usize, anim: &[AnimRef], face: bool) {
    if face {
        en::face_player(w, i);
    }
    set_anim(w, i, anim, true, Some(false));
}
fn timed_end(w: &mut World, i: usize, f: i32, len: i32) -> bool {
    if f >= len {
        set_anim(w, i, &[nm("idle")], true, Some(true));
        return true;
    }
    false
}
fn kb_of(r: &NodeRt) -> KnockState {
    KnockState { vx: r.d[0], vy: r.d[1], n: r.d[2], anim: r.d[3] }
}
fn kb_set(r: &mut NodeRt, k: &KnockState) {
    r.d[0] = k.vx;
    r.d[1] = k.vy;
    r.d[2] = k.n;
    r.d[3] = k.anim;
}

const KIDBUU_CHARGE: [(i32, i32); 4] = [(8, -2), (12, -2), (12, -2), (4, -2)];

fn leaf_enter(w: &mut World, c: &C, n: u16, r: &mut NodeRt) {
    let i = c.i;
    let frame = w.frame as i64;
    match kind(c, n) {
        K::Idle | K::RunMotionController | K::PartMirrorAnim => {
            if !w.en_ref(i).moving {
                set_anim(w, i, &[nm("idle")], false, Some(true));
            }
        }
        K::PushState => r.d[0] = 0,
        // enter 0x0804E50C: SetAnim(0) (the idle enter draws its start tick)
        K::StandStill => set_anim(w, i, &[nm("idle")], true, Some(true)),
        // enter 0x08018CE0: SetAnimRaw(1) + state 0x03002B9C, the walk cycle played backwards
        K::BackOffFacingPlayer | K::BackOffFacingPlayerThroughRocks => {
            let rev = w.data.anim.reverse;
            en::set_anim_rule(w, i, &[an(1)], true, Some(true), Some(rev));
        }
        K::Block => {
            w.en(i).a.blocking = true;
            set_anim(w, i, &[an(7)], true, Some(true));
        }
        K::ChasePlayer | K::ChasePlayerThroughRocks => {
            let Some(pi) = en::player(w, i) else { return };
            r.path.gx = w.pl_ref(pi).a.x;
            r.path.gy = w.pl_ref(pi).a.y;
            let mut p = r.path;
            crate::path::enter(w, i, &mut p);
            r.path = p;
        }
        K::WalkToPlayer => r.d[0] = 0,
        K::MoveToPoint => {
            r.path.gx = (par(c, n, P::X, 0) as i32) << 8;
            r.path.gy = (par(c, n, P::Y, 0) as i32) << 8;
            let mut p = r.path;
            crate::path::enter(w, i, &mut p);
            r.path = p;
        }
        K::WanderToRandomPoint => {
            r.path.counter = 0;
            for _ in 0..1000 {
                let gx = (0x60 + w.rng.rand(0x126)) << 8;
                let gy = (0x70 + w.rng.rand(0xf0)) << 8;
                if !en::passable(w, i, gx >> 8, gy >> 8) {
                    continue;
                }
                let dx = gx - w.en_ref(i).a.x;
                let dy = gy - w.en_ref(i).a.y;
                if ((dx.wrapping_mul(dx) >> 8) as i64) + ((dy.wrapping_mul(dy) >> 8) as i64) < 0x5000 {
                    continue;
                }
                r.path.gx = gx;
                r.path.gy = gy;
                break;
            }
        }
        K::BackStep => {
            let s = ((w.rng.rand(64) + 96) * par(c, n, P::K, 16) as i32) >> 7;
            let (dx, dy) = DIR_VEC[w.en_ref(i).a.facing as usize];
            let side = ((w.rng.rand(32768) - 16384) * s) >> 12;
            r.d[0] = -dx * 8 * s + if dx != 0 { 0 } else { side };
            r.d[1] = -dy * 8 * s + if dy != 0 { 0 } else { side };
            set_anim(w, i, &[an(1)], true, Some(true));
        }
        K::KnockbackSlide => {
            let mut kb = KnockState { n: 16, ..Default::default() };
            en::knock_start(w, i, par(c, n, P::Frames, 0) as i32, &mut kb);
            kb_set(r, &kb);
        }
        K::LungeStep => {
            en::face_player(w, i);
            set_anim(w, i, &[an(4)], true, Some(false));
        }
        K::RandomStep => r.d[0] = w.rng.rand(4),
        K::MeleeAttack => {
            // hit test (an accepted hit plays its sound), then SetAnim(4 + rand(punch animations))
            if en::melee_strike(w, i, 0x400, true, false, atk(2)) {
                en::melee_hit_sfx(w, i);
            }
            let n = w.en_ref(i).punch_anims(&w.data);
            let k = 4 + w.rng.rand(n);
            set_anim(w, i, &[an(k)], true, Some(false));
        }
        K::BioMechPunch => {
            en::face_player(w, i);
            set_anim(w, i, &[an(4)], true, Some(false));
            en::melee_strike(w, i, 0x400, true, false, atk(2));
        }
        K::HeavyPunch | K::WindupMelee => timed_enter(w, i, &[an(4), an(5)], true),
        K::HeavyStrike => {
            r.d[0] = en::player(w, i).is_some_and(|pi| w.pl_ref(pi).a.hittable) as i32;
            if r.d[0] != 0 {
                set_anim(w, i, &[an(4)], true, Some(false));
            }
        }
        K::ChargedPunch => {
            en::face_player(w, i);
            set_anim(w, i, &[an(18)], true, Some(true));
        }
        K::MiniYakonLunge => {
            r.d[0] = en::player(w, i).is_some_and(|pi| w.pl_ref(pi).a.hittable) as i32;
            if r.d[0] != 0 {
                en::face_player(w, i);
                set_anim(w, i, &[an(4)], true, Some(false));
            }
        }
        K::DriftContact => {
            let ang = w.rng.rand(256) as usize;
            let s = w.rng.rand(3) as usize;
            r.d[0] = DRIFT_COS[s][ang] as i32;
            r.d[1] = DRIFT_SIN[s][ang] as i32;
            r.d[2] = 0;
        }
        K::KiBlast => timed_enter(w, i, &[an(3)], true),
        K::GunShot | K::RifleShot | K::MummyWrapShot => timed_enter(w, i, &[an(17), an(3)], true),
        K::MummyWrapBurst => timed_enter(w, i, &[an(17), an(3)], false),
        K::NinjaStarThrow => timed_enter(w, i, &[an(18), an(4)], true),
        K::SoldierThrow | K::CyborgShot | K::TurretFire => {
            en::face_player(w, i);
            set_anim(w, i, &[an(18), an(3)], true, Some(false));
        }
        K::TurretOpen => set_anim(w, i, &[an(19)], true, Some(false)),
        K::TurretClose => set_anim(w, i, &[an(18)], true, Some(false)),
        K::BombThrow => timed_enter(w, i, &[an(18), an(4)], true),
        K::FlameBreath => {
            en::face_player(w, i);
            set_anim(w, i, &[an(17), an(4)], true, Some(true));
        }
        K::GrappleArm => timed_enter(w, i, &[an(4)], true),
        K::EnergyBarrage => {
            en::face_player(w, i);
            set_anim(w, i, &[an(3)], true, Some(true));
        }
        K::FinalFlash => timed_enter(w, i, &[an(19), an(3)], true),
        K::MouthBeamDown => set_anim(w, i, &[an(19)], true, Some(true)),
        K::KidBuuKamehameha => {
            en::face_player(w, i);
            set_anim(w, i, &[an(3)], true, Some(false));
            r.d[0] = 0;
        }
        K::StoneSpit | K::AimAtPlayer => timed_enter(w, i, &[an(19)], true),
        K::TeleportNearPlayer => {
            r.d[0] = w.rng.rand(4);
            set_anim(w, i, &[an(17)], true, Some(false));
        }
        K::TeleportChain4 => set_anim(w, i, &[an(17)], true, Some(false)),
        K::NinjaReveal => {
            r.d[0] = w.en_ref(i).a.x;
            r.d[1] = w.en_ref(i).a.y;
        }
        K::VampireGrab => {
            let ok = en::player(w, i).filter(|&pi| w.pl_ref(pi).held_by == 0);
            r.d[0] = ok.is_some() as i32;
            if let Some(pi) = ok {
                en::hold_player(w, i, pi, HoldKind::Vampire, 0);
                set_anim(w, i, &[an(17)], true, Some(true));
            }
        }
        K::LatchDrain => {
            if let Some(pi) = en::player(w, i) {
                if w.pl_ref(pi).held_by == 0 {
                    en::hold_player(w, i, pi, HoldKind::Latch, 0);
                }
            }
            set_anim(w, i, &[an(0)], true, Some(true));
        }
        K::GrabWindup => set_anim(w, i, &[an(17)], true, Some(false)),
        K::BearHug => {
            let yamu = w.live_enemies().iter().any(|&o| w.en_ref(o).ty == 0x93);
            let ok = en::player(w, i).filter(|&pi| w.pl_ref(pi).held_by == 0 && yamu);
            r.d[0] = ok.is_some() as i32;
            if let Some(pi) = ok {
                en::hold_player(w, i, pi, HoldKind::Hug, 0);
                set_anim(w, i, &[an(17)], true, Some(true));
            }
        }
        K::EnergyDrain => {
            r.d[0] = 0;
            r.d[1] = 0;
        }
        K::FreezePlayer => {
            if let Some(pi) = en::ctx_player(w, i) {
                en::hold_player(w, i, pi, HoldKind::Freeze, 0);
            }
            set_anim(w, i, &[an(17)], true, Some(false));
        }
        K::EatCandy => {
            let p = en::player(w, i);
            en::face_player(w, i);
            set_anim(w, i, &[an(19)], true, Some(true));
            r.d[0] = p.is_some() as i32;
            if let Some(pi) = p {
                en::hold_player(w, i, pi, HoldKind::Eat, 0);
            }
        }
        K::BuuStretchPunch | K::CandyBeam => {
            let ok = en::player(w, i).is_some_and(|pi| w.pl_ref(pi).hp() > 0 && w.pl_ref(pi).a.hittable);
            r.d[0] = ok as i32;
            if ok {
                en::face_player(w, i);
                if kind(c, n) == K::BuuStretchPunch {
                    set_anim(w, i, &[an(19)], true, Some(true));
                } else {
                    set_anim(w, i, &[an(18)], true, Some(false));
                }
            }
        }
        K::AbsorbStance => set_anim(w, i, &[an(20)], true, Some(false)),
        K::AbsorbPose | K::AbsorbPose2 => set_anim(w, i, &[an(20)], true, Some(false)),
        K::SuperBuuPowerUp => {
            w.en(i).powered = true;
            set_anim(w, i, &[an(11)], true, Some(false));
        }
        K::ThrowBigBall => {
            set_anim(w, i, &[an(15)], true, Some(true));
            let id = w.ents[i].id;
            shots::add(w, shots::super_ball(id));
        }
        K::GlideToWaypoint => {
            const WP: [(i32, i32); 5] = [(224, 104), (344, 128), (120, 128), (264, 112), (144, 144)];
            let step = if w.rng.rand(2) != 0 { 1 } else { 4 };
            w.g.waypoint = (w.g.waypoint + step) % 5;
            let (tx, ty) = WP[w.g.waypoint as usize];
            let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
            r.d[0] = ((tx - px) << 8) / 60;
            r.d[1] = ((ty - py) << 8) / 60;
            w.en(i).a.facing = facing_toward(r.d[0], r.d[1]);
            set_anim(w, i, &[an(14)], true, Some(true));
        }
        K::SummonMinion => timed_enter(w, i, &[an(17)], false),
        K::KidBuuSplitAnim => set_anim(w, i, &[an(11)], true, Some(false)),
        K::BabidiHover => w.g.buu_pacified = 0,
        K::BabidiGesture => set_anim(w, i, &[an(18)], true, Some(true)),
        K::JanembaHandStrikeA | K::JanembaHandStrikeB => set_anim(w, i, &[an(17)], true, Some(false)),
        K::JanembaAscend => set_anim(w, i, &[an(18)], true, Some(true)),
        K::JanembaDescendSummon => {
            w.en(i).a.z = -200;
            r.d[0] = -1;
            set_anim(w, i, &[an(18)], true, Some(true));
        }
        K::MiniHopToward | K::MiniHopFlee | K::Hop => r.d[0] = 0,
        K::MiniRollAttack => {
            set_anim(w, i, &[an(17)], true, Some(true));
            let (dx, dy) = en::to_player(w, i);
            r.d[0] = dx.signum() * 384;
            r.d[1] = dy.signum() * 384;
        }
        K::HandAmbush => {
            if let Some(pi) = en::player(w, i) {
                let left = w.rng.rand(2) != 0;
                let (ppx, py) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.y);
                let e = w.en(i);
                e.a.x = (ppx + if left { 4 } else { -20 }) << 8;
                e.a.y = py;
                e.a.facing = if left { 2 } else { 3 };
            }
            set_anim(w, i, &[an(17)], true, Some(false));
            r.d[0] = 0;
            w.en(i).a.alpha = 256;
        }
        K::SuperJanembaWave => timed_enter(w, i, &[an(18), an(3)], true),
        K::JumpSmash => set_anim(w, i, &[an(18)], true, Some(true)),
        K::FinishLandingAnim => set_anim(w, i, &[nm("idle")], true, Some(true)),
        K::StunnedAnim | K::StunnedAnim21 | K::PlayAnimHold => set_anim(w, i, &[an(20)], true, Some(true)),
        K::RecoverAnim | K::MechTransform => set_anim(w, i, &[an(19)], true, Some(false)),
        K::PlayAnim0 => set_anim(w, i, &[an(0)], true, Some(true)),
        K::PlayAnim12 | K::PlayAnim12b | K::PlayAnim12Swarm | K::PlayAnim12Yakon | K::PlayAnim12Yakon2 | K::GhostAnimA | K::GhostAnimB => set_anim(w, i, &[an(17)], true, Some(false)),
        K::PlayAnim13 | K::PlayAnim13Yakon | K::PlayAnimOnce => set_anim(w, i, &[an(18)], true, Some(false)),
        K::PlayAnimTimed => set_anim(w, i, &[an(19)], true, Some(true)),
        K::DarkSphere => {
            let (cx, cy) = w.en_ref(i).a.center();
            let id = w.ents[i].id;
            let s = shots::dark_sphere(w, id, (cx + 2) << 8, (cy - 17) << 8);
            shots::add(w, s);
            set_anim(w, i, &[an(17)], true, Some(true));
        }
        K::TauntAnim => set_anim(w, i, &[an(4)], true, Some(false)),
        K::SpawnEnzymes => {
            r.d[0] = 120;
            r.d[1] = w.en_ref(i).spawned;
        }
        K::ShockStun => {
            w.en(i).stunned = true;
            set_anim(w, i, &[an(9)], true, Some(true));
        }
        _ => {}
    }
    let _ = frame;
}

fn leaf_exit(w: &mut World, c: &C, n: u16, r: &mut NodeRt) {
    let i = c.i;
    let me = w.ents[i].id;
    let release_if_held = |w: &mut World| {
        if let Some(pi) = en::player(w, i) {
            if w.pl_ref(pi).held_by == me {
                player::release(w, pi);
            }
        }
    };
    match kind(c, n) {
        K::Block => {
            w.en(i).a.blocking = false;
            let f = w.frame as i64;
            w.en(i).state_stamp = f;
        }
        K::KnockbackSlide => {
            let kb = kb_of(r);
            if kb.anim != 0 && w.en_ref(i).a.anim.map(|a| a as i32 + 1) == Some(kb.anim) {
                set_anim(w, i, &[nm("idle")], true, Some(true));
            }
        }
        K::ChargedPunch => w.en(i).a.hittable = true,
        K::KidBuuKamehameha => {
            if let Some(k) = w.find(r.d[1] as u32) {
                w.ents[k].removed = true;
            }
            stop_enemy_beam(w, i);
        }
        K::VampireGrab | K::LatchDrain | K::BearHug | K::FreezePlayer => release_if_held(w),
        K::EatCandy => {
            if let Some(pi) = en::player(w, i) {
                if w.pl_ref(pi).held_by == me {
                    w.g.candy = 0;
                    player::release(w, pi);
                }
            }
        }
        K::DarkSphere => w.en(i).a.hittable = true,
        K::ShockStun => w.en(i).stunned = false,
        _ => {}
    }
}

fn stop_enemy_beam(w: &mut World, i: usize) {
    let b = w.en_ref(i).beam;
    if let Some(k) = w.find(b) {
        shots::beam_stop(w, k);
    }
    w.en(i).beam = 0;
}

fn leaf_update(w: &mut World, c: &C, n: u16, f: i32, r: &mut NodeRt) -> bool {
    let i = c.i;
    let me = w.ents[i].id;
    let frame = w.frame as i64;
    let speed = par(c, n, P::Speed, 0) as i32;
    match kind(c, n) {
        K::Never => false,
        K::GotoState => {
            let st = par(c, n, P::State, 0) as u16;
            replace_top(w, i, st);
            true
        }
        K::PushState => {
            if r.d[0] != 0 {
                return true;
            }
            r.d[0] = 1;
            let st = par(c, n, P::State, 0) as u16;
            push(w, i, st);
            false
        }
        K::PopState => {
            w.en(i).stunned = false;
            pop(w, i);
            true
        }
        K::Succeed | K::SelfMessage11 | K::RunScript | K::InlineScript => true,
        K::StaggerMinions => {
            let rr = w.rng.rand(8);
            let kids = w.en_ref(i).children.clone();
            for (k, id) in kids.iter().enumerate() {
                if let Some((ki, _)) = w.enemy_by_id(*id) {
                    let s = (rr + k as i32 + 1) & 7;
                    w.en(ki).slot = if s == 0 { 8 } else { s };
                }
            }
            true
        }
        K::Idle | K::RunMotionController | K::PartMirrorAnim => false,
        K::StandStill => true,
        K::FacePlayer => {
            en::face_player(w, i);
            true
        }
        K::Wait => f as i64 >= par(c, n, P::Frames, 0),
        K::MarkTime => {
            w.en(i).mark1 = frame;
            true
        }
        K::MarkTime2 => {
            w.en(i).mark2 = frame;
            true
        }
        K::SetCounter => {
            w.en(i).counter = par(c, n, P::N, 0);
            true
        }
        K::IncCounter => {
            w.en(i).counter += 1;
            true
        }
        K::SpawnPuff | K::VampireSummon | K::VampireSummon2 => {
            let (cx, cy) = w.en_ref(i).a.center();
            crate::fx::puff(w, cx, cy, 0x1800);
            true
        }
        K::SelfDestruct => {
            en::die(w, i, true);
            true
        }
        K::TriggerSelf => {
            en::die(w, i, false);
            true
        }
        K::Block => false,
        // ---- movement
        K::ChasePlayer | K::ChasePlayerThroughRocks => {
            let Some(pi) = en::player(w, i) else { return false };
            r.path.gx = w.pl_ref(pi).a.x;
            r.path.gy = w.pl_ref(pi).a.y;
            let dx = r.path.gx.wrapping_sub(w.en_ref(i).a.x);
            let dy = r.path.gy.wrapping_sub(w.en_ref(i).a.y);
            if ((dx.wrapping_mul(dx) >> 8) as i64) + ((dy.wrapping_mul(dy) >> 8) as i64) <= 0x400 {
                return false;
            }
            let mut p = r.path;
            crate::path::update(w, i, &mut p);
            r.path = p;
            false
        }
        K::BackOffFacingPlayer | K::BackOffFacingPlayerThroughRocks => en::step_from_player(w, i, speed, true),
        K::StepAwayFromPlayer => en::step_from_player(w, i, speed, false),
        K::WalkToPlayer => {
            let af = w.en_ref(i).anim_frame();
            let mut st = r.d[0];
            let res = en::walk_to_player(w, i, speed, &mut st, af);
            r.d[0] = st;
            res
        }
        K::FlyToPlayer => en::move_toward(w, i, speed, 8, false, true),
        K::AlignWithPlayer | K::LineUpForShot => en::align(w, i, if speed != 0 { speed } else { 0x73 }),
        K::Wander => {
            en::wander_step(w, i, speed, par(c, n, P::TurnP, 0));
            false
        }
        K::MoveToPoint | K::WanderToRandomPoint => {
            let mut p: PathRt = r.path;
            crate::path::update(w, i, &mut p);
            r.path = p;
            (p.gx - w.en_ref(i).a.x).abs() < 0x800 && (p.gy - w.en_ref(i).a.y).abs() < 0x800
        }
        K::CirclePlayer => {
            en::face_player(w, i);
            let (dx, dy) = DIR_VEC[w.en_ref(i).a.facing as usize];
            en::walk_by(w, i, -dy * speed, dx * speed, (dy != 0 || dx != 0) && speed != 0);
            false
        }
        K::HoverDrift => {
            let (dx, dy) = en::to_player(w, i);
            {
                let e = w.en(i);
                e.vx = (e.vx + dx.signum() * 38).clamp(-128, 128);
                e.vy = (e.vy + dy.signum() * 38).clamp(-128, 128);
            }
            en::face_player(w, i);
            let (vx, vy) = (w.en_ref(i).vx, w.en_ref(i).vy);
            en::walk_by(w, i, vx, vy, vx != 0 || vy != 0);
            false
        }
        K::BackStep => {
            en::move_by(w, i, r.d[0], r.d[1]);
            f >= 48
        }
        K::KnockbackSlide => {
            let mut kb = kb_of(r);
            let done = en::knock_step(w, i, &mut kb);
            kb_set(r, &kb);
            done
        }
        K::LungeStep => {
            let (dx, dy) = DIR_VEC[w.en_ref(i).a.facing as usize];
            en::move_by(w, i, dx * 1024, dy * 1024);
            f >= 16
        }
        K::FollowMajinBuu => {
            let buu = w.live_enemies().into_iter().find(|&o| matches!(w.en_ref(o).ty, 0x1a | 0x1b | 0x1c));
            let Some(b) = buu else { return false };
            let (bx, by) = w.en_ref(b).a.center();
            let (x, y) = w.en_ref(i).a.center();
            let (dx, dy) = (bx - x, by - y);
            if dx.abs().max(dy.abs()) as i64 > par(c, n, P::Dist, 50) {
                en::walk_by(w, i, dx.signum() * 256, dy.signum() * 256, dx != 0 || dy != 0);
                w.en(i).a.facing = facing_toward(dx, dy);
            }
            false
        }
        K::TankDrive => {
            let (dx, dy) = en::to_player(w, i);
            let fdir = facing_toward(dx, dy);
            w.en(i).a.facing = fdir;
            let (fx, fy) = DIR_VEC[fdir as usize];
            if dx.abs() + dy.abs() > 24 {
                en::walk_by(w, i, fx * 128, fy * 128, true);
            }
            false
        }
        K::RandomStep => {
            let (dx, dy) = DIR_VEC[r.d[0] as usize & 3];
            en::walk_by(w, i, dx * 256, dy * 256, true);
            f >= 16
        }
        // ---- melee
        K::MeleeAttack | K::BioMechPunch => f >= 16,
        K::HeavyPunch => {
            if f == 48 {
                en::melee_strike(w, i, 0x600, true, false, atk(2));
            }
            timed_end(w, i, f, 64)
        }
        K::WindupMelee => {
            if f == 32 {
                en::melee_strike(w, i, 0x400, true, false, atk(2));
            }
            timed_end(w, i, f, 48)
        }
        K::HeavyStrike => {
            if r.d[0] == 0 {
                return true;
            }
            if f == 58 {
                en::heavy_strike(w, i);
            }
            f >= 76
        }
        K::ChargedPunch => {
            if f == 140 {
                set_anim(w, i, &[an(4)], true, Some(false));
                w.en(i).a.hittable = false;
                en::melee_strike(w, i, 0x600, false, true, atk(2));
            }
            f >= 149
        }
        K::MiniYakonLunge => {
            if r.d[0] == 0 {
                return true;
            }
            let (dx, dy) = DIR_VEC[w.en_ref(i).a.facing as usize];
            en::move_by(w, i, dx * 384, dy * 384);
            if r.d[1] == 0 && en::contact_hit(w, i) {
                r.d[1] = 1;
            }
            f >= 24
        }
        K::DriftContact => {
            let k = if r.d[2] > 0 { -1 } else { 1 };
            if r.d[2] > 0 {
                r.d[2] -= 1;
            }
            if !en::move_by(w, i, k * r.d[0], k * r.d[1]) {
                r.d[0] = -r.d[0];
                r.d[1] = -r.d[1];
            }
            if r.d[2] <= 0 && en::contact_hit(w, i) {
                r.d[2] = 16;
            }
            false
        }
        // ---- ranged
        K::KiBlast => {
            if f == 16 {
                let (mx, my) = en::muzzle(w, i, 10);
                let dir = w.en_ref(i).a.facing;
                shots::ki_blast(w, me, dir, 0, mx << 8, my << 8, atk(4), 1024, true);
            }
            timed_end(w, i, f, 32)
        }
        K::GunShot => {
            if f == 4 {
                en::shot(w, i, ShotType::Gun);
            }
            timed_end(w, i, f, 16)
        }
        K::RifleShot => {
            if f == 8 {
                en::shot(w, i, ShotType::Rifle);
            }
            timed_end(w, i, f, 32)
        }
        K::NinjaStarThrow => {
            if f == 8 {
                en::shot(w, i, ShotType::Star);
            }
            timed_end(w, i, f, 32)
        }
        K::MummyWrapShot | K::MummyWrapBurst => {
            if f == 8 {
                en::shot(w, i, if kind(c, n) == K::MummyWrapShot { ShotType::Wrap } else { ShotType::Burst });
            }
            timed_end(w, i, f, 24)
        }
        K::TankShoot => {
            en::shot(w, i, ShotType::Shell);
            true
        }
        K::SoldierThrow | K::CyborgShot | K::TurretFire => {
            let k = kind(c, n);
            let fire = if k == K::CyborgShot { 1 } else { 8 };
            let len = if k == K::TurretFire { 72 } else if k == K::SoldierThrow { 32 } else { 16 };
            if f == fire {
                let (x, y) = en::muzzle_fix(w, i, k == K::TurretFire);
                let dir = w.en_ref(i).a.facing;
                r.d[0] = shots::add(w, shots::energy_wave(me, dir, x, y)) as i32;
            }
            f >= len && !alive(w, r.d[0])
        }
        K::TurretOpen | K::TurretClose => {
            let d = Arc::clone(&w.data);
            f >= 12 && w.en_ref(i).anim_done(&d)
        }
        K::BombThrow => {
            if f == 16 {
                if let Some(pi) = en::player(w, i) {
                    let (mx, my) = en::muzzle(w, i, 8);
                    let (tx, ty) = w.pl_ref(pi).a.foot_center();
                    let dir = w.en_ref(i).a.facing;
                    shots::add(w, shots::bomb(me, dir, mx << 8, my << 8, tx, ty));
                }
            }
            timed_end(w, i, f, 32)
        }
        K::FlameBreath => {
            if (4..128).contains(&f) {
                const NOZ: [(i32, i32); 4] = [(46, 10), (16, -48), (-6, -12), (74, -12)];
                let a = &w.en_ref(i).a;
                let (ox, oy) = NOZ[a.facing as usize];
                let (dir, x, y) = (a.facing, (a.px() + ox) << 8, (a.py() + oy) << 8);
                let s = shots::flame(w, me, dir, x, y);
                shots::add(w, s);
            }
            f >= 128
        }
        K::GrappleArm => {
            if f == 28 {
                let (mx, my) = en::muzzle(w, i, 8);
                let dir = w.en_ref(i).a.facing;
                shots::add(w, shots::grapple_arm(me, dir, mx << 8, my << 8));
            }
            timed_end(w, i, f, 60)
        }
        K::EnergyBarrage => {
            if f <= 48 {
                let (mx, my) = en::muzzle(w, i, 10);
                let dir = w.en_ref(i).a.facing;
                let (dx, dy) = DIR_VEC[dir as usize];
                let o = shots::StraightOpts { vx: dx * 1024, vy: dy * 1024, b: [4, 4, 8, 8], a: atk(4), mult: 51, life: 12, explode: false, name: shots::SName::Barrage, ..Default::default() };
                shots::add_straight(w, me, dir, 0, mx << 8, my << 8, o);
            }
            f >= 48
        }
        K::FinalFlash => {
            if f == 24 {
                let (mx, my) = en::muzzle(w, i, 14);
                let dir = w.en_ref(i).a.facing;
                shots::add(w, shots::full_beam(me, dir, 0, mx, my, atk(11), 328, 156, Some(2), None, None));
            }
            timed_end(w, i, f, 180)
        }
        K::MouthBeamDown => {
            if f == 32 {
                let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
                shots::add(w, shots::full_beam(me, 0, 0, px + 32, py - 16, atk(11), 328, 253, Some(2), None, None));
            }
            f >= 301
        }
        K::KidBuuKamehameha => {
            if f == 1 {
                r.d[1] = crate::fx::beam_charge(w, me, KIDBUU_CHARGE, crate::fx::BeamSet::Kamehameha1) as i32;
            }
            if f == 8 {
                if let Some(k) = w.find(r.d[1] as u32) {
                    w.ents[k].removed = true;
                }
                let dir = w.en_ref(i).a.facing;
                const OFF: [(i32, i32); 4] = [(8, -2), (12, -2), (12, -2), (4, -2)];
                let (ox, oy) = OFF[dir as usize];
                let b = shots::grow_beam(w, me, dir, 0, shots::Muzzle::KidBuu { ox, oy }, atk(5), 41, 20, 3 << 8, 80 << 8, true, Some(shots::BEAM_POS_KAME_FIRE), false, 0);
                let id = shots::add(w, b);
                r.d[0] = id as i32;
                w.en(i).beam = id;
            }
            if f >= 90 {
                stop_enemy_beam(w, i);
                return true;
            }
            false
        }
        K::StoneSpit => {
            if f == 1 {
                en::shot(w, i, ShotType::Spit);
            }
            timed_end(w, i, f, 16)
        }
        K::AimAtPlayer => timed_end(w, i, f, 8),
        // ---- teleports
        K::NinjaTeleport => {
            en::ninja_teleport(w, i);
            true
        }
        K::TeleportNearPlayer => {
            if f == 36 {
                en::teleport_beside(w, i, r.d[0]);
            }
            f >= 54
        }
        K::TeleportChain4 => {
            if f % 72 == 36 {
                let side = w.g.tele_chain & 3;
                en::teleport_beside(w, i, side);
                w.g.tele_chain = (w.g.tele_chain + 1) % 12;
                if w.g.tele_chain % 4 == 0 {
                    return false;
                }
            }
            f % 72 == 0 && w.g.tele_chain % 4 == 0
        }
        K::NinjaDisguise => {
            set_anim(w, i, &[an(17)], true, Some(true));
            true
        }
        K::NinjaReveal => {
            if f % 2 == 0 {
                let jx = w.rng.rand(3) - 1;
                let jy = w.rng.rand(3) - 1;
                let e = w.en(i);
                e.a.x = r.d[0] + jx * 256;
                e.a.y = r.d[1] + jy * 256;
            }
            if f >= 64 {
                {
                    let e = w.en(i);
                    e.a.x = r.d[0];
                    e.a.y = r.d[1];
                }
                let (cx, cy) = w.en_ref(i).a.center();
                crate::fx::puff(w, cx, cy, 0x1800);
                return true;
            }
            false
        }
        // ---- grabs
        K::VampireGrab => {
            let p = en::player(w, i);
            let Some(pi) = p.filter(|&pi| r.d[0] != 0 && w.pl_ref(pi).held_by == me) else { return true };
            if w.frame & 15 == 0 {
                let pow = w.en_ref(i).a.stats.pow;
                let d = player::lose_hp(w, pi, pow, 0);
                en::heal(w, i, d as i64 * 1024);
            }
            false
        }
        K::LatchDrain => {
            let Some(pi) = en::player(w, i).filter(|&pi| w.pl_ref(pi).held_by == me) else { return true };
            if w.frame & 63 == 0 {
                player::lose_hp(w, pi, 1, 0);
            }
            false
        }
        K::GrabWindup => {
            let d = Arc::clone(&w.data);
            f >= 12 && w.en_ref(i).anim_done(&d)
        }
        K::BearHug => {
            if r.d[0] == 0 {
                return f >= 2;
            }
            let p = en::player(w, i);
            if r.d[1] == 0 && (p.is_none() || p.is_some_and(|pi| w.pl_ref(pi).held_by != me) || w.en_ref(i).drain_done) {
                r.d[1] = f;
                if let Some(pi) = p {
                    if w.pl_ref(pi).held_by == me {
                        player::release(w, pi);
                    }
                }
                w.en(i).drain_done = false;
            }
            r.d[1] != 0 && f - r.d[1] >= 60
        }
        K::EnergyDrain => {
            let Some(pi) = en::player(w, i) else { return true };
            match r.d[0] {
                0 => {
                    if en::move_toward(w, i, 0x100, 16, true, false) {
                        r.d[0] = 1;
                    }
                    w.pl_ref(pi).held_by == 0
                }
                1 => {
                    let held = w.pl_ref(pi).held_by;
                    if held == 0 {
                        return true;
                    }
                    let (ep, max_ep) = (w.pl_ref(pi).ep() as i64, w.pl_ref(pi).max_ep() as i64);
                    let lost = r.d[1] as i64;
                    if f % 8 == 0 && 4 * ep > max_ep && 4 * lost < max_ep {
                        player::lose_ep(w, pi, 1);
                        r.d[1] += 1;
                    } else if 4 * lost >= max_ep || 4 * ep <= max_ep {
                        if let Some(spo) = w.live_enemies().into_iter().find(|&o| w.ents[o].id == held) {
                            w.en(spo).drain_done = true;
                        }
                        r.d[0] = 2;
                        r.d[2] = f;
                    }
                    false
                }
                _ => f - r.d[2] >= 60,
            }
        }
        K::FreezePlayer => f >= 48,
        K::EatCandy => {
            let p = en::player(w, i);
            let Some(pi) = p.filter(|_| r.d[0] != 0) else { return true };
            if (24..=111).contains(&f) && f % 8 == 0 {
                let d = (w.en_ref(i).a.stats.pow >> 2).min(w.pl_ref(pi).hp());
                let lost = player::lose_hp(w, pi, d, 1);
                en::heal(w, i, lost as i64 * 1024);
            }
            if f >= 120 {
                w.g.candy = 0;
                player::release(w, pi);
                return true;
            }
            false
        }
        // ---- Buu
        K::BuuStretchPunch => {
            if r.d[0] == 0 {
                return true;
            }
            if f == 64 {
                const OFF: [(i32, i32); 4] = [(9, 16), (15, -56), (-24, -16), (48, -16)];
                let a = &w.en_ref(i).a;
                let (ox, oy) = OFF[a.facing as usize];
                let gfx = w.data.enemy(w.en_ref(i).ty).and_then(|t| t.gfx);
                let dir = a.facing;
                r.d[1] = shots::add(w, shots::stretch_arm(me, dir, ox, oy + if gfx == Some(0x2b) { -10 } else { 0 })) as i32;
            }
            if f >= 72 {
                return !alive(w, r.d[1]);
            }
            false
        }
        K::CandyBeam => {
            if r.d[0] == 0 {
                return true;
            }
            if f == 16 {
                const OFF: [(i32, i32); 4] = [(16, 8), (16, -50), (-8, -26), (40, -26)];
                let dir = w.en_ref(i).a.facing;
                let (ox, oy) = OFF[dir as usize];
                r.d[1] = shots::add(w, shots::candy_beam(me, dir, ox, oy)) as i32;
            }
            if f >= 32 {
                return !alive(w, r.d[1]);
            }
            false
        }
        K::AbsorbStance => f >= 8,
        K::AbsorbPose | K::AbsorbPose2 => {
            let d = Arc::clone(&w.data);
            f >= 8 && w.en_ref(i).anim_done(&d)
        }
        K::SuperBuuPowerUp => {
            let d = Arc::clone(&w.data);
            f >= 32 && w.en_ref(i).anim_done(&d)
        }
        K::ThrowBigBall => f >= 92,
        K::GlideToWaypoint => {
            {
                let e = w.en(i);
                e.a.x += r.d[0];
                e.a.y += r.d[1];
            }
            en::clamp_inside(w, i);
            f >= 60
        }
        K::SummonMinion => {
            if f == 40 {
                const OFF: [(i32, i32); 4] = [(8, 12), (8, -12), (-12, 8), (12, 8)];
                let a = &w.en_ref(i).a;
                let (ox, oy) = OFF[a.facing as usize];
                let (x, y) = (a.px() + ox, a.py() + oy);
                crate::battle::spawn_enemy(w, 0x0d, x, y, me, None);
            }
            timed_end(w, i, f, 72)
        }
        K::KidBuuSplitAnim => f >= 55,
        K::KidBuuSplit => {
            en::kid_buu_split(w, i);
            true
        }
        K::KidBuuMerge => {
            en::kid_buu_merge(w, i);
            true
        }
        K::BabidiHover => {
            let z = (w.en_ref(i).a.z - 1).max(-50);
            w.en(i).a.z = z;
            set_anim(w, i, &[an(if f & 8 != 0 { 1 } else { 0 })], false, Some(true));
            false
        }
        K::BabidiGesture => false,
        // ---- Janemba
        K::JanembaSpawnParts => {
            en::janemba_parts(w, i);
            true
        }
        K::JanembaHandStrikeA | K::JanembaHandStrikeB => {
            if f == 16 {
                en::hand_strike(w, i, kind(c, n) == K::JanembaHandStrikeA);
            }
            f >= 57
        }
        K::JanembaAscend => {
            if f >= 8 {
                w.en(i).a.z = (24 - 3 * f).max(-200);
            }
            f > 128
        }
        K::JanembaDescendSummon => {
            if w.en_ref(i).a.z < 0 {
                let z = (w.en_ref(i).a.z + 3).min(0);
                w.en(i).a.z = z;
                if z == 0 {
                    r.d[0] = 16;
                }
                return false;
            }
            if r.d[0] > 0 {
                r.d[0] -= 1;
                if r.d[0] == 0 {
                    en::top_up_minis(w, i);
                }
            }
            r.d[0] == 0 && f > 70
        }
        K::WaitMinisDefeated => !w.live_enemies().iter().any(|&o| w.en_ref(o).ty == 0x38),
        K::MiniHopToward | K::MiniHopFlee | K::Hop => {
            r.d[0] = (r.d[0] + 8) & 0xff;
            let ph = r.d[0];
            let air = ph >= 0x80;
            w.en(i).a.z = if air { -HOP_ARC8[((ph - 0x80) >> 3) as usize] } else { 0 };
            if kind(c, n) == K::Hop {
                return false;
            }
            if ph == 0 {
                if kind(c, n) == K::MiniHopToward {
                    if w.rng.chance(0x4000) {
                        let (dx, dy) = en::to_player(w, i);
                        r.d[1] = if dx >= 0 {
                            if dy >= 0 {
                                0
                            } else {
                                3
                            }
                        } else if dy >= 0 {
                            2
                        } else {
                            1
                        };
                    }
                } else {
                    en::face_player(w, i);
                }
            }
            if air {
                if kind(c, n) == K::MiniHopToward {
                    const V: [(i32, i32); 4] = [(128, 128), (-128, -128), (-128, 128), (128, -128)];
                    let (vx, vy) = V[r.d[1] as usize & 3];
                    en::move_by(w, i, vx, vy);
                } else {
                    let (dx, dy) = en::to_player(w, i);
                    let s = dx as i64 * dx as i64 + dy as i64 * dy as i64;
                    let (vx, vy) = if s == 0 { (0, 0) } else { (round_div_sqrt(dx as i64 * 128, s) as i32, round_div_sqrt(dy as i64 * 128, s) as i32) };
                    en::move_by(w, i, -vx, -vy);
                }
            }
            false
        }
        K::MiniRollAttack => {
            if f > 16 && f < 32 {
                en::move_by(w, i, r.d[0], r.d[1]);
            }
            if f == 32 {
                let bb = w.en_ref(i).a.body_box();
                en::area_hit(w, i, bb, atk(0), 0x400);
                return true;
            }
            false
        }
        K::HandAmbush => {
            if (80..=103).contains(&f) && r.d[0] == 0 {
                let bb = w.en_ref(i).a.body_box();
                if en::area_hit(w, i, bb, atk(2), 0x400) {
                    r.d[0] = 1;
                }
            }
            f >= 106
        }
        K::HandSink => {
            w.en(i).a.alpha = (256 - f * 256 / 107).max(51);
            f >= 107
        }
        K::SuperJanembaWave => {
            if f == 24 {
                const OFF: [(i32, i32); 4] = [(4, 4), (13, -24), (-8, -8), (24, -8)];
                let a = &w.en_ref(i).a;
                let (ox, oy) = OFF[a.facing as usize];
                let b = a.body_box();
                let dir = a.facing;
                shots::add(w, shots::sj_wave(me, dir, (b.x0 + ox) << 8, (b.y0 + oy + 24) << 8));
            }
            timed_end(w, i, f, 40)
        }
        // ---- others
        K::BrolySpawnRocks => {
            if w.ents.iter().any(|e| matches!(e.body, Body::Rock { .. })) {
                return true;
            }
            for (x, y) in [(200, 136), (112, 272), (296, 240)] {
                let b = w.arena.bounds;
                if x + 16 >= b.x0 && x + 48 <= b.x1 && y + 16 >= b.y0 && y + 48 <= b.y1 {
                    w.add(Body::Rock { x, y, hp: 2 });
                }
            }
            true
        }
        K::JumpSmash => {
            if (8..56).contains(&f) {
                w.en(i).a.z -= 8;
            } else if (56..200).contains(&f) {
                let (dx, dy) = en::to_player(w, i);
                {
                    let e = w.en(i);
                    e.a.x += dx.signum() * 256;
                    e.a.y += dy.signum() * 256;
                }
                en::clamp_inside(w, i);
            } else if (200..248).contains(&f) {
                let z = (w.en_ref(i).a.z + 8).min(0);
                w.en(i).a.z = z;
            }
            if f == 248 {
                w.en(i).a.z = 0;
                for k in -1..=1 {
                    let (cx, _) = w.en_ref(i).a.center();
                    let py = w.en_ref(i).a.py();
                    crate::fx::puff(w, cx + k * 12, py + 4, 0x1800);
                }
                return true;
            }
            false
        }
        K::FinishLandingAnim => f >= 11,
        K::StunnedAnim | K::StunnedAnim21 | K::PlayAnimHold => false,
        K::RecoverAnim => {
            let d = Arc::clone(&w.data);
            f >= 8 && w.en_ref(i).anim_done(&d)
        }
        K::PlayAnim0 => f >= 80,
        K::PlayAnim12 | K::PlayAnim12b | K::PlayAnim12Swarm | K::PlayAnim12Yakon | K::PlayAnim12Yakon2 | K::GhostAnimA | K::GhostAnimB | K::PlayAnim13 | K::PlayAnim13Yakon | K::PlayAnimOnce => {
            let d = Arc::clone(&w.data);
            f >= 16 && w.en_ref(i).anim_done(&d)
        }
        K::PlayAnimTimed => f >= 32,
        K::DarkSphere => w.g.sphere_state == 1 || w.g.sphere_state == 3,
        K::TauntAnim | K::MechTransform => {
            let d = Arc::clone(&w.data);
            f >= 12 && w.en_ref(i).anim_done(&d)
        }
        K::SummonMiniYakons => {
            for k in 0..5 {
                crate::battle::spawn_enemy(w, 0x92, 192 + 20 * k, 96, me, None);
            }
            true
        }
        K::WaitSummonsDead => !w.live_enemies().iter().any(|&o| w.en_ref(o).ty == 0x92),
        K::SpawnEnzymes => {
            if w.en_ref(i).spawned >= 8 {
                return false;
            }
            w.en(i).enz_timer -= 1;
            if w.en_ref(i).enz_timer <= 0 {
                let off = if w.rng.rand(2) != 0 { (16, -8) } else { (0, 0) };
                let (px, py) = (w.en_ref(i).a.px(), w.en_ref(i).a.py());
                if crate::battle::spawn_enemy(w, 0x18, px + off.0, py + off.1, me, None).is_some() {
                    let e = w.en(i);
                    e.spawned += 1;
                    e.enz_timer = 120;
                }
            }
            false
        }
        K::RaiseShield => {
            w.en(i).shield = 1;
            true
        }
        K::LowerShield => {
            w.en(i).shield = 0;
            true
        }
        K::ShieldBreak => {
            w.en(i).shield = -1;
            let (cx, cy) = w.en_ref(i).a.center();
            crate::fx::puff_rng(w, cx, cy);
            true
        }
        K::WaitFormationSlot => w.en_ref(i).slot != 0,
        K::MoveToFormationA | K::MoveToFormationB => {
            let Some(pi) = en::player(w, i) else { return true };
            let slot = w.en_ref(i).slot;
            let k = ((if slot != 0 { slot } else { 1 }) - 1).clamp(0, 7) as usize;
            let (ox, oy) = if kind(c, n) == K::MoveToFormationA { RING40[k] } else { RING24[k] };
            let (px, py) = w.pl_ref(pi).a.foot_center();
            let (fw, fh) = (w.en_ref(i).a.foot_w, w.en_ref(i).a.foot_h);
            en::move_to_point(w, i, px + ox - (fw >> 1), py + oy - (fh >> 1), 0x100)
        }
        K::WormSpawnBody => {
            en::worm_spawn_body(w, i);
            true
        }
        K::WormHeadMove => {
            en::worm_head_step(w, i);
            false
        }
        K::WormBodyFollow => {
            en::worm_body_step(w, i);
            false
        }
        // ---- reaction states
        K::StunHold => {
            w.en(i).stunned = true;
            set_anim(w, i, &[an(9)], true, Some(true));
            true
        }
        K::DeathBlink => {
            w.en(i).a.alpha = if f & 2 != 0 { 77 } else { 256 };
            f as i64 >= par(c, n, P::Frames, 32)
        }
        K::ShockStun => f >= 48,
        K::VolleyballTransform | K::VolleyballRestore => {
            w.en(i).volleyball = kind(c, n) == K::VolleyballTransform;
            let (cx, cy) = w.en_ref(i).a.center();
            let rate = 0x1000 + w.rng.rand(0x1000);
            crate::fx::explosion(w, cx, cy, rate);
            true
        }
        K::VolleyballLaunch => {
            let (dx, dy) = DIR_VEC[w.en_ref(i).last_hit_dir as usize];
            if !en::move_by(w, i, dx * 1024, dy * 1024) {
                return true;
            }
            if let Some(pi) = en::ctx_player(w, i) {
                let bb = w.en_ref(i).a.body_box();
                en::area_hit_enemies(w, i, bb, pi);
            }
            f >= 30
        }
        _ => false,
    }
}

// ---- on-hit trees -------------------------------------------------------------------------------------------------------

/// the top state's aux tree; 0 = not handled, 1 = handled, 2 = stop
pub fn on_hit(w: &mut World, i: usize, msg: &HitMsg) -> i32 {
    let Some(st) = w.en_ref(i).ai.top_state() else { return 0 };
    let Some(aux) = w.data.aux.get(&st).cloned() else { return 0 };
    hit_node(w, i, &aux, 0, msg)
}

fn hit_node(w: &mut World, i: usize, t: &Arc<BehTree>, n: u16, msg: &HitMsg) -> i32 {
    let node = &t.nodes[n as usize];
    let kids = node.kids.clone();
    let p = node.p.clone();
    let kid = |w: &mut World, k: usize| -> i32 {
        match kids.get(k) {
            Some(&c) => hit_node(w, i, t, c, msg),
            None => 0,
        }
    };
    let frame = w.frame as i64;
    match node.k {
        K::Sequence => {
            let mut res = 1;
            for &k in &kids {
                let r = hit_node(w, i, t, k, msg);
                if r == 2 {
                    return 2;
                }
                res &= r;
            }
            res
        }
        K::Selector => {
            for &k in &kids {
                let r = hit_node(w, i, t, k, msg);
                if r != 0 {
                    return r;
                }
            }
            0
        }
        K::IfAttack => {
            let a = p.g(P::Atk) as u8;
            if msg.atk.sub == a || msg.atk.kind == a {
                kid(w, 0)
            } else {
                0
            }
        }
        K::ApplyDamage => {
            if msg.mult == 0 {
                return 0;
            }
            if en::take_hit(w, i, msg) {
                2
            } else {
                1
            }
        }
        K::GotoState => {
            replace_top(w, i, p.g(P::State) as u16);
            1
        }
        K::PushState => {
            push(w, i, p.g(P::State) as u16);
            1
        }
        K::PopState => {
            pop(w, i);
            2
        }
        K::SubState => match w.data.aux.get(&(p.g(P::State) as u16)).cloned() {
            Some(aux) => hit_node(w, i, &aux, 0, msg),
            None => 0,
        },
        K::Chance => {
            if w.rng.chance(p.g(P::Prob)) {
                kid(w, 0)
            } else {
                0
            }
        }
        K::IfTimeInState => {
            if frame - w.en_ref(i).state_stamp > p.g(P::Frames) {
                kid(w, 0)
            } else {
                0
            }
        }
        K::IfCounterEq => {
            if w.en_ref(i).counter == p.g(P::N) {
                kid(w, 0)
            } else {
                0
            }
        }
        K::IfCooldownOver => {
            if frame - w.en_ref(i).mark1 > p.g(P::Frames) {
                kid(w, 0)
            } else {
                0
            }
        }
        K::IfHpBelow => {
            let e = w.en_ref(i);
            if e.hp1024 < (p.g(P::Frac) * e.max_hp1024).div_euclid(65536) {
                kid(w, 0)
            } else {
                0
            }
        }
        K::IfHitFromFront => {
            if (msg.dir ^ 1) == w.en_ref(i).a.facing {
                kid(w, 0)
            } else {
                0
            }
        }
        K::Succeed => 2,
        K::MarkTime => {
            w.en(i).mark1 = frame;
            0
        }
        K::IncCounter => {
            w.en(i).counter += 1;
            0
        }
        K::SetCounter => {
            w.en(i).counter = p.g(P::N);
            0
        }
        K::Block => {
            w.en(i).a.blocking = true;
            set_anim(w, i, &[an(7)], false, Some(true));
            1
        }
        K::StunHold => {
            w.en(i).stunned = true;
            1
        }
        K::HoverDrift => {
            let (dx, dy) = DIR_VEC[msg.dir as usize];
            let e = w.en(i);
            e.vx += dx * 0x400;
            e.vy += dy * 0x400;
            1
        }
        K::SetHitDir => {
            w.en(i).last_hit_dir = msg.dir;
            1
        }
        K::HealFraction => {
            if msg.mult == 0 {
                return 1;
            }
            // 0x0803A79C: (s32)(dmg * frac) >> 16, a 32-bit product (Yakon's 100% absorb wraps negative for damage
            // with bit 15 set: re/buu/YAKON.md)
            en::heal(w, i, ((msg.dmg as i32).wrapping_mul(p.g(P::Frac) as i32) >> 16) as i64);
            1
        }
        K::GlideToWaypoint => {
            // accepted only while the player swings (bats the ball back); the third hit ends the fight (cutscene)
            let swinging = en::ctx_player(w, i).is_some_and(|pi| w.pl_ref(pi).swinging());
            if !swinging {
                return 2;
            }
            w.en(i).script_hits += 1;
            if w.en_ref(i).script_hits >= 3 {
                en::scripted_defeat(w, i);
            }
            1
        }
        K::AbsorbPose | K::AbsorbPose2 | K::FreezeHelperHit | K::ShieldHit => 1,
        _ => 0,
    }
}
