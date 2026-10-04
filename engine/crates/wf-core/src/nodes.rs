//! The AI interpreter's execution: composites (Sequence / Selector / Parallel / the shared conditional decorator
//! 0x080342A9), the action nodes, the on-hit trees, and the AI stack operations. Node for node the reference
//! interpreter (site/src/battle/rulesets/buu/ai.ts); INFERRED nodes there are INFERRED here (see `NODE_NOTES`).

use crate::ai::{new_rt, BehTree, NodeRt, PathRt, StackEntry, K, P};
use crate::content::AnimRef;
use crate::damage::atk;
use crate::enemy::{self as en, set_anim, KnockState, ShotType};
use crate::fixed::{round_div_sqrt, HOP_ARC8};
use crate::eanim::RuleKind;
use crate::geom::{facing_toward, overlaps, Rect, DIR_VEC};
use crate::player::{self, HoldKind};
use crate::shots;
use crate::tables::{RING24, RING40, SIN256};
use crate::world::{Body, HitMsg, World};
use std::sync::Arc;

/// nodes whose details are inferred in the reference (tables not decoded, cosmetic nodes)
pub const NODE_NOTES: &[(&str, &str)] = &[
    ("IfPlayerInZone", "sniper zone rectangle not decoded: true within 160 px"),
    ("IfPlayerInRoomZone", "vampire lair region not decoded: always true"),
    ("MoveToFormationA", "formation offset tables 0x0877AE80/0x0877AECC not decoded: ring of 8 slots at 40/24 px"),
    ("TankDrive", "turn helpers not decoded: axis-aligned drive toward the player at 0.5 px/frame"),
    ("EnergyBarrage", "barrage flight path not decoded: short 4 px/frame shots, 12 frames"),
    ("IfUnknown", "condition 0x0804EE25: Majin Buu is not pacified (1 - [0x0300302C])"),
    ("ShockStun", "electric-shock stun length not decoded: 48 frames"),
    ("VolleyballLaunch", "launched ball flight not decoded: 4 px/frame for 30 frames, hits other enemies"),
    ("InlineScript", "queued once as a script run (the game re-runs the inline bytecode each update until it returns 1)"),
    ("RunScript", "queues the event as a script run for the host (FieldEvent::Run, cause Ai)"),
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
        Body::Enemy(e) => {
            let ai = &mut e.ai;
            if ai.stack.get(c.lvl).is_some_and(|s| s.serial == c.serial) {
                return Some(&mut ai.stack[c.lvl].rt[n as usize]);
            }
            // replaced / popped during this evaluation: the tree finishes its update on its own runtime
            ai.orphans.iter_mut().find(|s| s.serial == c.serial).map(|s| &mut s.rt[n as usize])
        }
        _ => None,
    }
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
/// the Skeleton's collapse node (the first of state 0x213's two PlayAnimOnce defs)
const SKELETON_COLLAPSE: u32 = 0x0875_9370;
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
/// a state's tree is built (the reference's `create` of every node, in tree order): the nodes that do work at
/// creation (DriftContact's velocity draws, IfButtonMashed's counter state)
fn build(w: &mut World, i: usize, id: u16) -> StackEntry {
    let tree = w.data.states.get(&id).cloned().unwrap_or_else(|| Arc::new(crate::ai::compile_behaviour(id, &Default::default())));
    let serial = w.next_serial();
    let mut rt = new_rt(&tree);
    for (k, n) in tree.nodes.iter().enumerate() {
        match n.k {
            // node create 0x0802C69C: s = rand(3) + 1, a = rand(256); v = (cos a, sin a) * s >> 10 (1/256 px)
            K::DriftContact => {
                let s3 = w.rng.rand(3) + 1;
                let ang = w.rng.rand(256) as usize;
                rt[k].d[0] = (SIN256[(ang + 0x40) & 0xff] * s3) >> 10;
                rt[k].d[1] = (SIN256[ang] * s3) >> 10;
                rt[k].d[2] = 0;
            }
            // create 0x080363FA: count +0x10 = 0, stamp +0xC = now - gap - 1
            K::IfButtonMashed => {
                let gap = n.p.g(P::Gap) as i32;
                rt[k].d[3] = 0;
                rt[k].d[4] = w.g.clock(w.frame) as i32 - gap - 1;
            }
            _ => {}
        }
    }
    let _ = i;
    StackEntry { state: id, serial, rt, tree }
}
/// (the reference's history log; the state stamp obj+0x210 is only written by the Block node's exit)
fn note(_w: &mut World, _i: usize) {}
pub fn push(w: &mut World, i: usize, id: u16) {
    if w.en_ref(i).ai.stack.len() >= 8 {
        return;
    }
    w.en(i).ai.gen += 1;
    let s = build(w, i, id);
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
    let s = build(w, i, id);
    let ai = &mut w.en(i).ai;
    if let Some(last) = ai.stack.last_mut() {
        let old = std::mem::replace(last, s);
        ai.orphans.push(old);
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
            let old = e.ai.stack.pop().unwrap();
            e.ai.orphans.push(old);
        }
    }
    note(w, i);
}
pub fn reset(w: &mut World, i: usize, id: u16) {
    while let Some(c) = ctx_top(w, i) {
        exit(w, &c, 0);
        let e = w.en(i);
        if e.ai.stack.last().is_some_and(|s| s.serial == c.serial) {
            let old = e.ai.stack.pop().unwrap();
            e.ai.orphans.push(old);
        } else {
            break;
        }
    }
    let e = w.en(i);
    let rest: Vec<_> = e.ai.stack.drain(..).collect();
    e.ai.orphans.extend(rest);
    push(w, i, id);
}
/// one AI frame: the top state's tree (0x0800AB0C ignores the root's result: a finished tree is not restarted, its
/// composites keep their state)
pub fn tick(w: &mut World, i: usize) {
    let Some(c) = ctx_top(w, i) else { return };
    update(w, &c, 0);
    // the replaced trees' runtimes are only addressable while their evaluation runs
    if let Body::Enemy(e) = &mut w.ents[i].body {
        e.ai.orphans.clear();
    }
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
            | IfPartnerHolding | IfUnknown | ModCondition
    )
}

fn enter(w: &mut World, c: &C, n: u16) {
    match kind(c, n) {
        // 0x08018EF6: enters the child at the current index (the index is not reset: a re-entered sequence resumes)
        K::Sequence => {
            let cur = rt(w, c, n).map(|r| r.i).unwrap_or(0) as usize;
            if let Some(&k) = kids(c, n).get(cur) {
                enter(w, c, k);
            }
        }
        K::Selector | K::ParallelAll | K::ParallelAny => {
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
                // 0x0804E6A0: (frac * whole max HP) >> 16 whole HP, tested as << 10 (0x0804EDD8)
                let thr = ((par(c, n, P::Frac, 0) * (e.max_hp1024 >> 10)) >> 16) << 10;
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
            // DriftContact keeps what its create drew (and its back-off count) across enters
            if kind(c, n) != K::DriftContact {
                r.d = [0; 8];
            }
            let mut r = *r;
            if is_ctl(w, kind(c, n)) || (w.field.is_some() && matches!(kind(c, n), K::MiniHopToward | K::MiniHopFlee)) {
                w.en(c.i).ctl = (c.serial, n);
            }
            leaf_enter(w, c, n, &mut r);
            if let Some(x) = rt(w, c, n) {
                *x = r;
            }
        }
    }
}

fn exit(w: &mut World, c: &C, n: u16) {
    match kind(c, n) {
        // 0x08018F74: exits the current child (the index is kept)
        K::Sequence => {
            let i = rt(w, c, n).map(|r| r.i).unwrap_or(0) as usize;
            let ks = kids(c, n).to_vec();
            if i < ks.len() {
                exit(w, c, ks[i]);
            }
        }
        K::Selector | K::ParallelAll | K::ParallelAny => {
            for &k in kids(c, n).to_vec().iter() {
                exit(w, c, k);
            }
        }
        // 0x08034307: exits the running child (the flag is reset by the next enter)
        k if is_cond(k) => {
            let on = rt(w, c, n).map(|r| r.on).unwrap_or(false);
            if on {
                if let Some(&k0) = kids(c, n).first() {
                    exit(w, c, k0);
                }
            }
        }
        _ => {
            if is_ctl(w, kind(c, n)) && w.en_ref(c.i).ctl == (c.serial, n) {
                w.en(c.i).ctl = (0, 0);
            }
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
    match kind(c, n) {
        // 0x08018F0C: a finished child exits and the next one enters; after the last the index wraps to 0 without
        // entering the first child again. A state change inside does not stop the evaluation
        K::Sequence => {
            let ks = kids(c, n).to_vec();
            let Some(cur) = rt(w, c, n).map(|r| r.i as usize) else { return 0 };
            if cur >= ks.len() {
                if let Some(x) = rt(w, c, n) {
                    x.i = 0;
                }
                return 1;
            }
            if update(w, c, ks[cur]) == 0 {
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
        // 0x08019192: the first non-zero child result
        K::Selector => {
            for &k in kids(c, n).to_vec().iter() {
                if update(w, c, k) != 0 {
                    return 1;
                }
            }
            0
        }
        // ParallelAll 0x08019108 / ParallelAny 0x08019162: every child on every update, AND / OR of the results
        K::ParallelAll | K::ParallelAny => {
            let any = kind(c, n) == K::ParallelAny;
            let mut r = if any { 0 } else { 1 };
            for &k in kids(c, n).to_vec().iter() {
                let v = (update(w, c, k) != 0) as i32;
                r = if any { r | v } else { r & v };
            }
            r
        }
        // 0x080342AF: the child's result is returned; a finished child is not exited (it runs on while the
        // condition holds)
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
                return update(w, c, kid);
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
            let ctl = is_ctl(w, kind(c, n));
            let Some(r) = rt(w, c, n) else { return 0 };
            // a motion controller that already ran out while the field was held: the node only reports it
            if ctl && r.d[7] != 0 {
                return 1;
            }
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

/// the nodes whose work runs in a motion controller (a state object ticked with the animation in the actor update,
/// before the AI): FlameBreath 0x03002FA8, SoldierThrow 0x03002FCC, TurretFire 0x0300305C, CyborgShot 0x03003108,
/// GrappleArm 0x03003294, BombThrow 0x0203DD04
fn is_controller(k: K) -> bool {
    matches!(k, K::FlameBreath | K::SoldierThrow | K::TurretFire | K::CyborgShot | K::GrappleArm | K::BombThrow)
}
/// `is_controller` + the field's other shot states, which are the same kind of actor state object (GunShot
/// 0x03002FC4, RifleShot 0x03002FD8, KiBlast 0x03003068, NinjaStarThrow 0x03003074, MummyWrapShot 0x030030A8 / Burst
/// 0x030030B0: the actor's own update ticks them, so a shot leaves at its tick while an event holds the field -
/// Thieves Den 9-4: a Hessian's rocket during the level-up popup; WindupMelee 0x03003214: a Skeleton's swing turns
/// to the player at tick 32 inside a dart trap's event, Kyodai Pyramid 14-4); battles keep the reference's set
/// (HandSink, the Janemba Hand's second strike: its one-shot controller 0x0804DBA8 sinks on through an event - the real
/// route's 18-10, the switch scene: the hand is in its walk state when the scene ends)
fn is_ctl(w: &World, k: K) -> bool {
    is_controller(k) || (w.field.is_some() && matches!(k, K::GunShot | K::RifleShot | K::KiBlast | K::NinjaStarThrow | K::MummyWrapShot | K::MummyWrapBurst | K::WindupMelee | K::HandSink))
}

/// an event holds the field: the AI does not think, but the running motion controller keeps ticking (a turret's fire
/// ends on time, a Destroyer keeps breathing flames; Babidi's ship 8-53 / 8-59 door scripts). Once it has run out,
/// the node finishes on the AI's next update without running again
pub fn busy_controller(w: &mut World, i: usize) {
    let (serial, n) = w.en_ref(i).ctl;
    if serial == 0 {
        return;
    }
    let Some(c) = ctx_top(w, i) else { return };
    // field: a mini's hop counter is its controller's (0x0300322C): it counts on through an event, the hop's moves
    // are the AI's (sc_janemba: after the defeat scene the minis resume mid-cycle)
    if c.serial == serial && w.field.is_some() && matches!(kind(&c, n), K::MiniHopToward | K::MiniHopFlee) {
        if let Some(r) = rt(w, &c, n) {
            r.d[0] = (r.d[0] + 8) & 0xff;
            let ph = r.d[0];
            w.en(i).a.z = if ph >= 0x80 { -HOP_ARC8[((ph - 0x80) >> 3) as usize] } else { 0 };
        }
        return;
    }
    if c.serial != serial || !is_ctl(w, kind(&c, n)) {
        return;
    }
    let Some(r) = rt(w, &c, n) else { return };
    if r.d[7] != 0 {
        return;
    }
    r.f += 1;
    let mut r = *r;
    let f = r.f;
    if leaf_update(w, &c, n, f, &mut r) {
        r.d[7] = 1;
    }
    if let Some(x) = rt(w, &c, n) {
        *x = r;
    }
}

/// the game's face-the-player routine 0x0801599C: exact in the field (24.8 difference, ties horizontal); battles keep
/// the footprint-pixel rule the TypeScript cross-check pins (tests/cross_ts.rs) until it is re-verified there
fn face_player_routine(w: &mut World, i: usize) {
    if w.field.is_some() {
        en::face_player_exact(w, i);
    } else {
        en::face_player(w, i);
    }
}

/// the ThroughRocks wrapper (update 0x080369D0) broadcasts message 0x22 before and after its child's update; a
/// rock's handler 0x08036C4E answers it by writing its 32 x 32 rect (x + 16, y + 16) into the walk and the enemy
/// grid: free, both times, for an intact or cracked rock (state index < 2). The rocks mark nothing when they
/// appear, so in the field they block nobody, and each chase update frees the tiles under them - the part of
/// Broly's own mark inside a rock's rect included, until his next re-plan marks it again (the real route, Volcano
/// Summit 15-32: the player walks through a rock's place; Broly's mark shrinks to the tiles outside it)
fn rock_msg(w: &mut World) {
    let rects: Vec<[i32; 4]> = w.ents.iter().filter(|e| !e.removed).filter_map(|e| match e.body {
        Body::Rock { x, y, .. } => Some([x + 16, y + 16, x + 48, y + 48]),
        _ => None,
    }).collect();
    if let Some(c) = w.col.as_mut() {
        for r in rects {
            c.mark_both(r, 1);
        }
    }
}

// ---- conditions ------------------------------------------------------------------------------------------------------

fn cond(w: &mut World, c: &C, n: u16) -> bool {
    let i = c.i;
    let frame = w.g.clock(w.frame);
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
        // 0x0804EE24: 1 - [0x0300302C] - Majin Buu is not pacified (his hurt states under Babidi's spell pop at once)
        K::IfUnknown => w.g.buu_pacified != 1,
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
        // 0x080331EC: only while IsAnim(0) (3-2: a tank that just started driving does not roll its Chance); the
        // footprint centres more than 16 px apart along the facing, less than 16 across
        K::IfPlayerInLineOfFire => {
            if !w.en_ref(i).is_anim(0) {
                return false;
            }
            let (dx, dy) = en::to_player(w, i);
            match w.en_ref(i).a.facing {
                0 => dx.abs() < 16 && dy > 16,
                1 => dx.abs() < 16 && dy < -16,
                2 => dy.abs() < 16 && dx < -16,
                _ => dy.abs() < 16 && dx > 16,
            }
        }
        // the field: the game's shared condition 0x080361B8 (IfContactHit restores the facing, IfMeleeHit keeps the
        // turn); battles keep the rule cross_ts pins
        K::IfContactHit if w.field.is_some() => en::contact_strike(w, i, 1024, true),
        K::IfMeleeHit if w.field.is_some() => en::contact_strike(w, i, 1024, false),
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
            let (mut dx, mut dy) = en::to_player(w, i);
            // field (sc_janemba): 0x08034574 compares the centres of the current-facing body boxes (0x0800FE78, at the
            // ground position: a hopping mini's height does not count);
            // battle keeps the reference's
            if w.field.is_some() {
                let c = |b: crate::geom::Rect| ((b.x0 + b.x1) >> 1, (b.y0 + b.y1) >> 1);
                let (px, py) = c(w.pl_ref(pi).a.body_box_ground());
                let (mx, my) = c(w.en_ref(i).a.body_box_ground());
                (dx, dy) = (px - mx, py - my);
            }
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
            // 0x08037B96 (vtable 0x0804B0F8, condition 0x08037B48): a pure overlap test (no hold on the player: several
            // enzymes can latch) - the player's box (0x0800FE78) narrowed 6 px a side against the enemy's whole box, any
            // facing (real run 21-6 r064_004 f254: a 1-px overlap at the box's bottom row latches)
            let pb = p.a.body_box();
            let eb = e.body_box();
            overlaps(&Rect::new(pb.x0 + 6, pb.y0, pb.x1 - 6, pb.y1), &eb)
        }
        // condition 0x080363C4: a new A/B/d-pad press this frame -> stamp = now, count + 1, true once count > presses;
        // no press and now - stamp > gap -> count = 0 (state built with the tree: d[3] count, d[4] stamp)
        K::IfButtonMashed => {
            let presses = par(c, n, P::Presses, 0) as i32;
            let gap = par(c, n, P::Gap, 0) as i32;
            let now = w.g.clock(w.frame) as i32;
            let slot = w.en_ref(i).target as usize;
            let pressed = w.g.pressed(slot) & 0xf3 != 0;
            let Some(r) = rt(w, c, n) else { return false };
            if pressed {
                r.d[4] = now;
                r.d[3] += 1;
                return r.d[3] > presses;
            }
            if now - r.d[4] > gap {
                r.d[3] = 0;
            }
            false
        }
        // cond 0x08036EA4 (world-map encounters): px distances <= 16 and |player z - own z| < 16 px (obj+0x1A8, 22.10)
        K::IfPlayerOnTop => {
            let Some(pi) = w.player_idx(0) else { return false };
            let (pa, ea) = (&w.pl_ref(pi).a, &w.en_ref(i).a);
            let (dx, dy) = (pa.px() - ea.px(), pa.py() - ea.py());
            if dx.abs() > 16 || dy.abs() > 16 {
                return false;
            }
            let pz = w.field.as_ref().and_then(|f| f.flight.as_ref()).map(|f| f.z).unwrap_or(0);
            let dz = (pz - crate::fieldpatrol::z_of(w, i)) >> 10;
            dz.abs() < 16
        }
        // 0x0802F945: the first Spopovich is in 0x03001B30 (the end of GrabWindup) showing anim id 0x12's animation
        K::IfPartnerHolding => {
            let g = w.data.anim.generic["0x03001B30"];
            w.live_enemies().into_iter().find(|&o| matches!(w.en_ref(o).ty, 0x6d..=0x70)).is_some_and(|o| w.en_ref(o).ea.rule == g && is_anim_id(w, o, 0x12))
        }
        K::ModCondition => {
            let node = &c.tree.nodes[n as usize];
            crate::modkit::ai_hook(w, i, &node.call, &node.args, 0) != 0
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
        one_idle(w, i);
        return true;
    }
    false
}
/// one SetAnim(0) at a controller's end: unless the state's own end (the recorded sequence, this frame's anim tick)
/// already did it
fn one_idle(w: &mut World, i: usize) {
    let d = Arc::clone(&w.data);
    let e = w.en_ref(i);
    if e.a.anim_name(&d) != Some("idle") || !e.anim_ended {
        set_anim(w, i, &[nm("idle")], true, Some(true));
    }
}
/// The enemy's pose setter (vt+0x30, 0x0800ED3E, and the nodes that do the same by hand): the raw only (the tick
/// stays), the given frame, under the hold state 0x03001B00 (nothing ticks)
fn hold_pose(w: &mut World, i: usize, raw: i32, frame: i32) {
    let d = Arc::clone(&w.data);
    let e = w.en(i);
    e.set_raw(&d, raw);
    e.ea.pos.frame = frame;
    e.ea.rule = d.anim.hold;
    e.ea.done = false;
    e.own_state = true;
    e.anim_ended = false;
    e.anim_seq = e.anim_seq.wrapping_add(1);
}
/// enemy SetAnim id -> raw (0x08072670 + 1)
const SETANIM_RAW: [i32; 23] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24];
/// 0x0800EEB0(obj, id): the actor shows SetAnim(id)'s animation (the shown raw)
fn is_anim_id(w: &World, i: usize, id: usize) -> bool {
    let raw = w.en_ref(i).a.anim_raw(&w.data);
    raw.is_some() && raw == SETANIM_RAW.get(id).copied()
}
fn set_anim_r(w: &mut World, i: usize, refs: &[AnimRef], looped: bool, rule: u32) {
    en::set_anim_rule(w, i, refs, true, Some(looped), Some(rule));
}
fn melee_sfx(w: &mut World) {
    let s = 48 + w.rng.rand(2);
    w.sfx(s, 64);
}
fn alive_ent(w: &World, id: i32) -> bool {
    id != 0 && w.find(id as u32).is_some_and(|k| !w.ents[k].removed)
}
/// the straight-line mover 0x0800A1D4 (base 0x08009D24; SetAnim(1) on creation): n = max(|dx|, |dy|) / speed ticks,
/// velocity d / n, facing on the dominant axis; kept in the node's path record (wx / wy target px, speed / budget
/// velocity, valid n, counter dir)
fn line_mover(w: &mut World, i: usize, tx: i32, ty: i32, speed: i32) -> PathRt {
    let (ex, ey) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    let dx = (tx << 8) - ex;
    let dy = (ty << 8) - ey;
    let (ax, ay) = (dx.abs(), dy.abs());
    let dir = if ax >= ay {
        if dx >= 0 {
            3
        } else {
            2
        }
    } else if dy >= 0 {
        0
    } else {
        1
    };
    let mut n = (((if ax >= ay { ax } else { ay }).wrapping_shl(8)) / speed) >> 8;
    let (mut vx, mut vy) = (0, 0);
    if n != 0 {
        vx = dx.wrapping_shl(8) / n.wrapping_shl(8);
        vy = dy.wrapping_shl(8) / n.wrapping_shl(8);
    } else {
        n = 1;
    }
    w.en(i).a.facing = dir;
    set_anim(w, i, &[nm("walk")], true, Some(true));
    PathRt { speed: vx, budget: vy, wx: tx, wy: ty, valid: n, counter: dir as i32, gx: 0, gy: 0 }
}
/// step 0x0800A01D: blocked -> SetAnim(0) unless already; else move, SetAnim(1) unless walking, face, n - 1; 0 ->
/// exactly at the target, arrived
fn line_mover_step(w: &mut World, i: usize, m: &mut PathRt) -> bool {
    en::unmark_self(w, i);
    let (x, y) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    if !en::fits(w, i, (x + m.speed) >> 8, (y + m.budget) >> 8) {
        en::mark_self(w, i);
        if !is_anim_id(w, i, 0) {
            set_anim(w, i, &[nm("idle")], true, Some(true));
        }
        return false;
    }
    {
        let a = &mut w.en(i).a;
        a.x += m.speed;
        a.y += m.budget;
    }
    if !is_anim_id(w, i, 1) {
        set_anim(w, i, &[nm("walk")], true, Some(true));
    }
    w.en(i).a.facing = m.counter as u8;
    en::mark_self(w, i);
    m.valid -= 1;
    if m.valid != 0 {
        return false;
    }
    en::unmark_self(w, i);
    {
        let a = &mut w.en(i).a;
        a.x = m.wx << 8;
        a.y = m.wy << 8;
    }
    en::mark_self(w, i);
    true
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


/// BrolySpawnRocks (0x08035F81): three rock obstacles (ctor 0x08036A44) at map px (200,136), (112,272), (296,240)
/// unless rocks exist; in the field each rock object holds its own 0x1000-byte tile block
fn spawn_rocks(w: &mut World) {
    if w.ents.iter().any(|e| matches!(e.body, Body::Rock { .. })) {
        return;
    }
    for (x, y) in [(200, 136), (112, 272), (296, 240)] {
        let b = w.arena.bounds;
        if x + 16 >= b.x0 && x + 48 <= b.x1 && y + 16 >= b.y0 && y + 48 <= b.y1 {
            let id = w.add(Body::Rock { x, y, hp: 2 });
            crate::vram::acquire(w, crate::vram::Owner::Ent(id), 0x1000);
        }
    }
}

fn leaf_enter(w: &mut World, c: &C, n: u16, r: &mut NodeRt) {
    let i = c.i;
    let frame = w.frame as i64;
    match kind(c, n) {
        // node 0x080716F4: enter no-op, update 0 - no animation change
        K::Idle => {}
        // BrolySpawnRocks spawns on enter (Broly's initial state 0x33: the rocks are made with Broly - the game's
        // 15-32 allocator at the area entry holds their three blocks right below Broly's own, before the player's
        // and the HUD's; spawned later, the portrait text box that follows purges differently and Broly's place
        // in the update list changes - app soak of the real route, 15-32). Field only: the battle keeps the
        // update-time spawn the TypeScript reference has.
        K::BrolySpawnRocks if w.field.is_some() => spawn_rocks(w),
        // shield soldiers: the graphics swap runs on enter (seen on the GotoState frame)
        K::RaiseShield => {
            w.en(i).shield = 1;
            shield_gfx(w, i, &[(0xB0, 0xAF), (0x102, 0xB2), (0x56, 0xAC)]);
        }
        K::LowerShield => {
            w.en(i).shield = 0;
            shield_gfx(w, i, &[(0xAF, 0xB0), (0xB2, 0x102), (0xAC, 0x56)]);
        }
        K::ShieldBreak => {
            w.en(i).shield = -1;
            shield_gfx(w, i, &[(0xB0, 0xB1), (0xAF, 0xB1), (0x102, 0x100), (0xB2, 0x100), (0x56, 0x55), (0xAC, 0x55)]);
            let (cx, cy) = w.en_ref(i).a.center();
            crate::fx::puff_rng(w, cx, cy);
        }
        // RunMotionController with a field patrol runner (0x080299A3): enter is a nop (fieldpatrol.rs)
        K::RunMotionController if crate::fieldpatrol::has(w, i) => {}
        K::RunMotionController | K::PartMirrorAnim => {
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
        // HoverDrift's velocity lives in the node: a re-entered state starts from 0
        K::HoverDrift => {
            let e = w.en(i);
            e.vx = 0;
            e.vy = 0;
        }
        // enter 0x08031388: first a bump hit (0x0803125C: face, standard melee x1.0, accepted -> 48 + rand(2)), then
        // the hop velocity
        K::BackStep => {
            if en::melee_strike(w, i, 0x400, true, false, atk(2)) {
                melee_sfx(w);
            }
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
        // enter 0x08037A42 = SetAnimHold(4, 0): raw 5 under the hold state, frame 0 (no swing, no rand)
        K::LungeStep => {
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(4)], false, h);
        }
        // RandomStep (create 0x08037C06, vtable 0x0804B1C0; the Skeleton's bones rattle before they rise): enter
        // 0x0804E824 keeps the position
        K::RandomStep => {
            r.d[0] = w.en_ref(i).a.x;
            r.d[1] = w.en_ref(i).a.y;
        }
        K::MeleeAttack => {
            // hit test (an accepted hit plays its sound), then SetAnim(4 + rand(punch animations))
            if en::melee_strike(w, i, 0x400, true, false, atk(2)) {
                en::melee_hit_sfx(w, i);
            }
            let n = w.en_ref(i).punch_anims(&w.data);
            let k = 4 + w.rng.rand(n);
            set_anim(w, i, &[an(k)], true, Some(false));
        }
        K::BioMechPunch => timed_enter(w, i, &[an(4)], true),
        // enter 0x0803CB82: face, SetAnimRaw(4) + state 0x030032F4; ends at 48 + 8 * frames
        K::HeavyPunch => {
            timed_enter(w, i, &[an(4), an(5)], true);
            let d = Arc::clone(&w.data);
            let e = w.en_ref(i);
            let nf = e.a.anim_raw(&d).and_then(|raw| e.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.anim_raw(raw))).map(|a| a.steps(e.a.facing).len() as i32);
            r.d[0] = 48 + 8 * nf.unwrap_or(2);
        }
        // singleton 0x03003210: enter 0x0804E808 = SetAnimRaw(4) + SetState(0x03003214) (no facing change); finished as
        // soon as the actor left that controller; the end at 64 + 8 * (frames - 2)
        K::WindupMelee => {
            set_anim(w, i, &[an(4), an(5)], true, Some(false));
            let d = Arc::clone(&w.data);
            let e = w.en_ref(i);
            let nf = e.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.anim_raw(e.ea.pos.raw)).map(|a| a.steps(e.a.facing).len() as i32).unwrap_or(0);
            r.d[0] = e.anim_seq as i32;
            r.d[1] = 64 + ((nf.max(2) - 2) << 3);
        }
        // enter 0x0802F74C: player active -> the wind-up pose (no SetAnim), armed, timer 60
        K::HeavyStrike => {
            r.d[0] = en::player(w, i).is_some_and(|pi| w.pl_ref(pi).a.hittable) as i32;
            if r.d[0] != 0 {
                // field: the enter 0x0802F74C only poses - vt+0x30(4, 0): SetAnim id 4's raw, frame 0, held - and
                // starts the 60-frame count; the facing is what IfPlayerInMeleeRange's turn left (Northern Wilderness
                // 15-12: a Knight Destroyer winds up facing left). Battles keep the reference (a turn, no pose)
                if w.field.is_some() {
                    hold_pose(w, i, SETANIM_RAW[4], 0);
                } else {
                    en::face_player(w, i);
                }
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
        // enter 0x0804E850 (node vtable 0x0804B640 entry 0): the Blood Cell detaches from the enemy grid
        // (`0x0800F924`: obj+0x291 = 1, its tiles freed; it never marks again - VERIFIED real route 21-4: the two
        // drifting cells are detached in RAM and leave no mark where the Blister's Enzyme spawns)
        K::DriftContact => {
            en::unmark_self(w, i);
            w.en(i).detached = true;
        }
        K::KiBlast => {
            timed_enter(w, i, &[an(3)], true);
            r.d[6] = w.en_ref(i).anim_seq as i32;
        }
        // (r.d[6] = the animation state the shot runs under: see `shot_state_gone`)
        K::GunShot | K::RifleShot | K::MummyWrapShot => {
            timed_enter(w, i, &[an(17), an(3)], true);
            r.d[6] = w.en_ref(i).anim_seq as i32;
        }
        K::MummyWrapBurst => {
            timed_enter(w, i, &[an(17), an(3)], false);
            r.d[6] = w.en_ref(i).anim_seq as i32;
        }
        K::NinjaStarThrow => {
            timed_enter(w, i, &[an(18), an(4)], true);
            r.d[6] = w.en_ref(i).anim_seq as i32;
        }
        // SoldierThrow 0x080266D8 (SetAnimRaw(0x13)) / TurretFire 0x0802B684 (SetAnimRaw(0x14)): face the player
        // with the routine 0x0801599C (field: the 24.8 rule, ties horizontal, Babidi's ship 8-22 turret; battles keep
        // the footprint rule the TypeScript cross-check pins, as FacePlayer)
        K::SoldierThrow | K::TurretFire => {
            face_player_routine(w, i);
            let r0 = w.data.anim.once_stop3;
            let a = if kind(c, n) == K::SoldierThrow { 19 } else { 20 };
            set_anim_r(w, i, &[an(a), an(3)], false, r0);
            r.d[2] = w.en_ref(i).ea.pos.raw;
        }
        // BeamDodge 0x08029778 (state 0x15A, pushed by IfInPlayerBeamLine): away from the player's line - the player
        // facing down / up: left when the player's x (24.8) is greater, else right; facing left / right: up when
        // the player's y is greater, else down; SetAnim(0x12) under the controller vt 0x0804A758 (real run 14-2)
        K::BeamDodge => {
            if let Some(pi) = en::player(w, i) {
                let pa = &w.pl_ref(pi).a;
                let ea = &w.en_ref(i).a;
                r.d[0] = if pa.facing <= 1 {
                    if pa.x > ea.x { 2 } else { 3 }
                } else if pa.y > ea.y {
                    1
                } else {
                    0
                };
            }
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(17)], true, h);
            r.d[6] = w.en_ref(i).anim_seq as i32;
        }
        // controller state 0x03003108 (enter 0x0802F6A8: face, SetAnimRaw(0x12))
        K::CyborgShot => {
            face_player_routine(w, i);
            let r0 = w.data.anim.once_hold3;
            set_anim_r(w, i, &[an(18), an(3)], false, r0);
        }
        // 0x0802B701 / 0x0802B8B5: SetAnimRaw(0x13 / 0x12) under the one-shot state 0x03001B30
        // TurretOpen draws the gun into the wall: not hittable (obj+0x1CC = 0) until TurretClose brings it out
        // (real route 8-36: a punch beside a retracted turret only hits the fighter)
        K::TurretOpen | K::TurretClose => {
            let r0 = w.data.anim.once_stop3;
            let open = kind(c, n) == K::TurretOpen;
            set_anim_r(w, i, &[an(if open { 19 } else { 18 })], false, r0);
            // field: TurretClose makes it hittable only at its end (Flying Mega-Fortress XJ-6 12-20: a punch during
            // the 24 frames of the gun coming out passes through; obj+0x1CC set at the node's finish)
            if open || w.field.is_none() {
                w.en(i).a.hittable = !open;
            }
        }
        // the shell controller 0x030030EC: SetAnimRaw(0x13) played once (8 ticks per frame), then SetAnim(0)
        K::TankShoot => {
            let once = w.data.anim.setanim[&4];
            set_anim_r(w, i, &[an(18)], true, once);
            r.d[1] = w.en_ref(i).ea.pos.raw;
        }
        // enter 0x08026A26 (after facing): d = the player's position - own, the component across the facing zeroed,
        // each clamped to +-128
        K::BombThrow => {
            timed_enter(w, i, &[an(18), an(4)], true);
            r.d[2] = w.en_ref(i).ea.pos.raw;
            if let Some(pi) = en::player(w, i) {
                let (ppx, ppy) = (w.pl_ref(pi).a.px(), w.pl_ref(pi).a.py());
                let a = &w.en_ref(i).a;
                r.d[0] = if a.facing >= 2 { (ppx - a.px()).clamp(-128, 128) } else { 0 };
                r.d[1] = if a.facing < 2 { (ppy - a.py()).clamp(-128, 128) } else { 0 };
            }
        }
        // enter 0x08023FA8: SetAnimRaw(0x12) under the flame controller 0x03002FA8 (no swing), sound 63
        K::FlameBreath => {
            en::face_player(w, i);
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(18)], true, h);
            w.sfx(63, 64);
        }
        // enter 0x08037E2D: face, SetAnimRaw(0x12) under controller 0x03003294
        K::GrappleArm => {
            en::face_player(w, i);
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(18)], true, h);
        }
        K::EnergyBarrage => {
            en::face_player(w, i);
            set_anim(w, i, &[an(3)], true, Some(true));
        }
        K::FinalFlash => timed_enter(w, i, &[an(19), an(3)], true),
        K::MouthBeamDown => set_anim(w, i, &[an(19)], true, Some(true)),
        // the player's beam class 0x080206B8 (Kamehameha Lv1: x0.04, sub 5; sound 3) at the position + 0x0875928C[dir]
        K::KidBuuKamehameha => {
            en::face_player(w, i);
            set_anim(w, i, &[an(3)], true, Some(false));
            if w.field.is_some() {
                let h = w.data.anim.hold;
                w.en(i).ea.rule = h;
            }
            const OFF: [(i32, i32); 4] = [(16, 20), (16, -4), (-12, 2), (44, 2)];
            let a = &w.en_ref(i).a;
            let (ox, oy) = OFF[a.facing as usize];
            let (dir, x, y) = (a.facing, a.px() + ox, a.py() + oy);
            let me = w.ents[i].id;
            let built = w.frame as i64;
            let id = shots::add(w, shots::kame_beam_enemy(me, dir, x, y, built, atk(5), 41));
            r.d[0] = id as i32;
            w.en(i).beam = id;
            w.sfx(3, 64);
        }
        // StoneSpit = node 0x030030CC: SetAnimRaw(0x14) + controller 0x030030C8 (it spits on its first tick, enemy.rs)
        K::StoneSpit => {
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(20)], true, h);
            w.en(i).spit_ctl = 0;
        }
        // AimAtPlayer = node 0x030030D4: SetAnimRaw(0x14) + controller 0x030030D0 (head pose only), done at once
        K::AimAtPlayer => {
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(20)], true, h);
        }
        // field (HFIL's Nosferatu, sc_hfil 18-8): the puff appears in the frame the state is entered, its rate drawn
        // (0x1000 + rand(0x1000), 0x0804E733); battle keeps the reference's fixed-rate puff on the node's update
        K::VampireSummon | K::VampireSummon2 if w.field.is_some() => {
            let (cx, cy) = w.en_ref(i).a.center();
            crate::fx::puff_rng(w, cx, cy);
        }
        K::TeleportNearPlayer => {
            r.d[0] = w.rng.rand(4);
            set_anim(w, i, &[an(17)], true, Some(false));
            // field (sc_janemba): the vanish takes hits off until the node ends (a punch at it computes nothing);
            // battle keeps the reference's
            if w.field.is_some() {
                w.en(i).a.hittable = false;
            }
        }
        K::TeleportChain4 => {
            set_anim(w, i, &[an(17)], true, Some(false));
            // field: the vanish takes hits off (as TeleportNearPlayer's); its raw is what the node's check knows
            if w.field.is_some() {
                w.en(i).a.hittable = false;
                r.d[2] = w.en_ref(i).ea.pos.raw;
            }
        }
        // field: the enter 0x0802C1B0 - SetAnimRaw(0x12) (the raw only: the tick stays), frame = rand(the animation's
        // frame count) (which prop the ninja poses as), SetState(0x03001B00: a state that holds); it runs when the
        // state is entered (Northern Wilderness 15-7: in the frame of the GotoState). Battles keep the reference's
        // rule (no draw, the animation set by the update)
        K::NinjaDisguise if w.field.is_some() => {
            let d = Arc::clone(&w.data);
            let cnt = {
                let e = w.en_ref(i);
                e.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.anim_raw(19)).map(|a| a.steps(e.a.facing).len() as u32).unwrap_or(0)
            };
            let fr = w.rng.rand(cnt);
            hold_pose(w, i, 19, fr);
        }
        K::NinjaReveal => {
            r.d[0] = w.en_ref(i).a.x;
            r.d[1] = w.en_ref(i).a.y;
        }
        // enter 0x0803399E: the player shows raw 10 frame 0, faces the vampire's facing ^ 1, held; the VAMPIRE moves next
        // to the player (pos = player pos - 0x08759334[facing]); vampire SetAnimRaw(0x12) + its own loop state
        K::VampireGrab => {
            r.d[0] = 0;
            r.d[1] = 0;
            let Some(pi) = en::player(w, i) else { return };
            en::hold_player(w, i, pi, HoldKind::Vampire, 0);
            const OFF: [(i32, i32); 4] = [(0, 16), (0, -16), (-16, 0), (16, 0)];
            let f = w.en_ref(i).a.facing;
            w.pl(pi).a.facing = f ^ 1;
            let (ox, oy) = OFF[f as usize];
            let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
            en::unmark_self(w, i);
            {
                let a = &mut w.en(i).a;
                a.x = px - (ox << 8);
                a.y = py - (oy << 8);
            }
            en::mark_self(w, i);
            let l = w.data.anim.loop3;
            set_anim_r(w, i, &[an(18)], true, l);
        }
        // enter 0x080298D8: SetAnimRaw(1) + a controller that glues the enzyme to the player (not held)
        K::LatchDrain => en::latch_enter(w, i),
        // enter 0x0802C465: face, SetAnimRaw(0x12) under the play-once controller 0x0300308C (frame t >> 3)
        K::GrabWindup => {
            en::face_player(w, i);
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(18)], true, h);
            let d = Arc::clone(&w.data);
            let e = w.en_ref(i);
            let nf = e.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.anim_raw(19)).filter(|_| e.a.anim_raw(&d) == Some(19)).map(|a| a.nframes());
            r.d[0] = nf.unwrap_or(0);
        }
        // enter 0x0802C4C9: keep the player's position, SetAnimRaw(0x14) frame 0 under 0x03001B00 at Spopovich + (8, -16);
        // partner = the first Yamu
        K::BearHug => {
            let yamu = w.live_enemies().into_iter().find(|&o| matches!(w.en_ref(o).ty, 0x93 | 0x94));
            r.d[3] = yamu.map(|o| w.ents[o].id as i32).unwrap_or(0);
            let Some(pi) = en::player(w, i).filter(|&pi| !w.pl_ref(pi).dead) else { return };
            r.d[4] = w.pl_ref(pi).a.x;
            r.d[5] = w.pl_ref(pi).a.y;
            player::hold_pose(w, pi, 21, 0);
            let (ex, ey) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
            let p = w.pl(pi);
            p.a.x = ex + (8 << 8);
            p.a.y = ey - (16 << 8);
        }
        // Yamu's drain: the counter is uninitialised heap memory (reads 0x6D)
        K::EnergyDrain => r.d[2] = 0x6d,
        // enter 0x080305AA: player frame 0, raw 1, held; the fragment's freeze state
        K::FreezePlayer => {
            if let Some(pi) = en::player(w, i) {
                let me = w.ents[i].id;
                player::hold_raw(w, pi, me, HoldKind::Freeze, 0, 1);
            }
            en::freeze_enter(w, i);
        }
        // enter 0x0802B19D: face, SetAnimRaw(0x14) under controller 0x0300303C
        K::EatCandy => {
            en::face_player(w, i);
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(20)], true, h);
        }
        // enter 0x0802ACB3 / 0x0802B333: the player HP > 0 and hittable -> face, SetAnimRaw(0x13 / 0x12) under the
        // node's controller
        K::BuuStretchPunch | K::CandyBeam => {
            let ok = en::player(w, i).is_some_and(|pi| w.pl_ref(pi).hp() > 0 && w.pl_ref(pi).a.hittable);
            r.d[0] = ok as i32;
            if ok {
                en::face_player(w, i);
                let h = w.data.anim.hold;
                set_anim_r(w, i, &[an(if kind(c, n) == K::BuuStretchPunch { 19 } else { 18 })], true, h);
            }
            r.d[6] = w.en_ref(i).anim_seq as i32;
        }
        K::AbsorbStance => set_anim(w, i, &[an(20)], true, Some(false)),
        K::AbsorbPose | K::AbsorbPose2 => set_anim(w, i, &[an(20)], true, Some(false)),
        K::SuperBuuPowerUp => {
            w.en(i).powered = true;
            // anim 0x0C under the idle controller 0x03001B30 (once, 8 ticks a cel; segment sb 6-6 on 6-1: raw 13
            // to its 7th cel at tick 56)
            let g = w.data.anim.generic["0x03001B30"];
            set_anim_r(w, i, &[an(12)], false, g);
        }
        K::ThrowBigBall => {
            set_anim(w, i, &[an(15)], true, Some(true));
            let id = w.ents[i].id;
            shots::add(w, shots::super_ball(id));
            // the builder (0x08021280) starts the charge sound (0x080212F2: sound 3)
            w.sfx(3, 64);
        }
        K::GlideToWaypoint => {
            // the waypoint table 0x0877B1BC (24.8 pairs; real run 20-6 r062_001 tile +7,000: index 2 is (304, 256))
            const WP: [(i32, i32); 5] = [(224, 104), (344, 128), (304, 256), (120, 264), (112, 144)];
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
        // Babidi (real route 8-71, Vegeta vs Majin Buu). Hover enter 0x0802AD88: SetAnimRaw(0x13) under the
        // controller 0x03003014 (no idle draw), Buu is no longer pacified, not hittable (obj+0x1CC = 0)
        K::BabidiHover => {
            w.g.buu_pacified = 0;
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(19)], true, h);
            w.en(i).a.hittable = false;
        }
        // the landing, enter 0x0802AE76: SetAnimRaw(0x13) under the controller 0x03003024
        K::BabidiGesture => {
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(19)], true, h);
        }
        // field (sc_janemba): the arm part plays the strike (raw 19), the body keeps its idle; battle keeps the
        // reference's body animation (cross_ts)
        // (the arm's animation starts with its controller, on the node's first update)
        K::JanembaHandStrikeA | K::JanembaHandStrikeB if w.field.is_some() => {}
        K::JanembaHandStrikeA | K::JanembaHandStrikeB => set_anim(w, i, &[an(17)], true, Some(false)),
        // field (sc_janemba): anim 0x13 = raw 20, one pass at 8 frames a cel, held on its last cel through the rise and
        // the descent; battle keeps the reference's
        K::JanembaAscend if w.field.is_some() => {
            let rule = w.data.anim.fallback_once;
            en::set_anim_rule(w, i, &[an(19)], true, Some(false), Some(rule));
            // the controller (0x030032B8) counts from its first update: cel 0 shows 9 frames
            w.en(i).ea.pos.tick = 0xffff;
        }
        K::JanembaAscend => set_anim(w, i, &[an(18)], true, Some(true)),
        // field: the same animation goes on (no restart)
        K::JanembaDescendSummon if w.field.is_some() => {
            w.en(i).a.z = -200;
            r.d[0] = -1;
            // its controller (0x0804D3C8) rises from its second frame (minis 229 frames after the ascent began)
            r.d[1] = 1;
        }
        K::JanembaDescendSummon => {
            w.en(i).a.z = -200;
            r.d[0] = -1;
            set_anim(w, i, &[an(18)], true, Some(true));
        }
        // node 0x03003230 (enter 0x0803467A: SetAnimRaw(0x12), the hop controller, aim)
        K::MiniHopToward => {
            r.d[0] = 0;
            set_anim(w, i, &[an(18)], true, Some(true));
            mini_aim(w, i);
        }
        K::MiniHopFlee | K::Hop => r.d[0] = 0,
        // controller 0x03003248 enter 0x08034CF8: the diagonal toward the player, velocity 0x0875947C[dir] (+-1.5 px)
        K::MiniRollAttack => {
            set_anim(w, i, &[an(17)], true, Some(true));
            let (dx, dy) = en::to_player(w, i);
            let f: u8 = if dx > 0 {
                if dy > 0 {
                    0
                } else {
                    3
                }
            } else if dy > 0 {
                2
            } else {
                1
            };
            w.en(i).a.facing = f;
            const V: [(i32, i32); 4] = [(384, 384), (-384, -384), (-384, 384), (384, -384)];
            r.d[0] = V[f as usize].0;
            r.d[1] = V[f as usize].1;
        }
        // field (HFIL's Janemba Hands, segments sc_hfil): the node's first update does the enter's work (the side
        // draw, the jump to the player's position of that frame, the controller 0x0804D714)
        K::HandAmbush if w.field.is_some() => {
            r.d[0] = 0;
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
        // field: the jump is SetAnim id 0x13 = raw 20 under its own controller 0x0804D310@0x0300325C (the type's
        // recorded sequence, 249 ticks: Volcano Summit 15-32 - Broly's animation runs to the landing; raw 19 is the
        // ChargedPunch's, whose one-shot ends at tick 148 with an idle draw). Battles keep the reference
        K::JumpSmash if w.field.is_some() => set_anim(w, i, &[an(19)], true, Some(true)),
        K::JumpSmash => set_anim(w, i, &[an(18)], true, Some(true)),
        // (field: the update plays the landing's cels first)
        K::FinishLandingAnim if w.field.is_some() => {}
        K::FinishLandingAnim => set_anim(w, i, &[nm("idle")], true, Some(true)),
        // node 0x0874CA2C (Ronin / Samurai / Shogun guard): the enemy's SetAnim(0x11) (vfunc +0x28 -> 0x0800FAB6):
        // raw 19 under the one-shot state 0x03001B18 (16 ticks, then SetAnim(0)); restarted by every enter
        // (13-51: the Samurai's stance restarts every 3 frames while its state loops, and runs out when it does not).
        // Field only: battles keep the rule the TypeScript cross-check pins
        K::SelfMessage11 if w.field.is_some() => set_anim(w, i, &[an(18)], true, Some(false)),
        // Babidi's state 0xCA, enter 0x0802ADF0: SetAnimRaw(0x15) under the controller 0x0300301C
        K::PlayAnimHold if w.en_ref(i).ty == 0x1d => {
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(21)], true, h);
        }
        K::StunnedAnim | K::StunnedAnim21 | K::PlayAnimHold => set_anim(w, i, &[an(20)], true, Some(true)),
        K::RecoverAnim => set_anim(w, i, &[an(19)], true, Some(false)),
        // create 0x080315A4: SetAnimRaw(0x15) under the one-shot state 0x03001B30
        K::MechTransform => set_anim(w, i, &[an(20)], true, Some(false)),
        // enter 0x0804E5E8 installs the fragment state 0x03003158
        K::PlayAnim0 => en::frag_enter(w, i),
        K::PlayAnim12 | K::PlayAnim12b | K::PlayAnim12Swarm | K::PlayAnim12Yakon | K::PlayAnim12Yakon2 => set_anim(w, i, &[an(17)], true, Some(false)),
        // GhostAnimA / B: enters 0x0804EBC8 / 0x0804EB38 = SetAnimRaw(0x12) under the one-shot state 0x03001B30
        // the Skeleton's collapse (state 0x213's first node, def 0x08759370, singleton 0x0300320C): enter 0x0804EAA0 =
        // not hittable (obj+0x1CC = 0) + SetAnimRaw(0x11) under the one-shot state 0x03001B38; its second
        // PlayAnimOnce (def 0x0875938C, the rise) is SetAnimRaw(0x12), and its exit 0x0804F6CC makes it hittable again
        // (0x03001B38 = {vtable 0x0804E0AC, shift 2}: 4 ticks a cel, the last one stays)
        K::PlayAnimOnce => {
            let collapse = c.tree.nodes[n as usize].addr == SKELETON_COLLAPSE;
            if collapse {
                w.en(i).a.hittable = false;
            }
            let rule = w.data.anim.once_stop2;
            set_anim_r(w, i, &[an(if collapse { 17 } else { 18 })], false, rule);
        }
        // field: the Yakon pose's SetAnim runs in the node's first update (below)
        K::PlayAnim13Yakon if w.field.is_some() => {}
        K::PlayAnim13 | K::PlayAnim13Yakon | K::GhostAnimA | K::GhostAnimB => set_anim(w, i, &[an(18)], true, Some(false)),
        // field (sc_janemba, Super Janemba's state 0x2DD: a hit cuts the teleport): the landing animation (0x13)
        // played backwards from its end, then the vanish (0x11) backwards - 71 frames, then a SetAnim(0)
        K::PlayAnimTimed if w.field.is_some() && w.en_ref(i).ty != 0x1d => {
            let hold = w.data.anim.hold;
            en::set_anim_rule(w, i, &[an(19)], true, Some(false), Some(hold));
            // no hits until its end
            w.en(i).a.hittable = false;
        }
        // Babidi's state 0xC9 (hit on the ground), enter 0x0802AEDA: SetAnimRaw(0x14) under the controller
        // 0x03003030 (4 cels of 8 ticks, looping) and Majin Buu is pacified (0x0300302C = 1)
        K::PlayAnimTimed if w.en_ref(i).ty == 0x1d => {
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(20)], true, h);
            w.g.buu_pacified = 1;
        }
        K::PlayAnimTimed => set_anim(w, i, &[an(19)], true, Some(true)),
        K::DarkSphere => {
            let (cx, cy) = w.en_ref(i).a.center();
            let id = w.ents[i].id;
            let s = shots::dark_sphere(w, id, (cx + 2) << 8, (cy - 17) << 8);
            shots::add(w, s);
            set_anim(w, i, &[an(17)], true, Some(true));
        }
        // controller 0x0804E030 (enter 0x08033D84): anim 4, frame = tick >> 3, no swing sound; tick 0x20 -> SetAnim(0)
        // (the idle enter's rand(255))
        K::TauntAnim => {
            let once = w.data.anim.setanim[&4];
            en::set_anim_rule(w, i, &[an(4)], true, Some(false), Some(once))
        }
        K::SpawnEnzymes => {
            r.d[0] = 120;
            r.d[1] = w.en_ref(i).spawned;
        }
        K::ShockStun => {
            w.en(i).stunned = true;
            set_anim(w, i, &[an(9)], true, Some(true));
        }
        // the segments are created by the node's enter 0x0802CBD4 (inside the GotoState)
        K::WormSpawnBody => en::worm_spawn_body(w, i),
        _ => {}
    }
    let _ = frame;
}
/// MiniHopToward's aim: the diagonal toward the player from the footprint centres (a zero keeps the facing)
fn mini_aim(w: &mut World, i: usize) {
    let (mut dx, mut dy) = en::to_player(w, i);
    // 0x080346FC aims with the whole-pixel footprint centres (0x0800FDFA + 0x0804F900)
    if w.field.is_some() {
        if let Some(pi) = en::player(w, i) {
            let (px, py) = w.pl_ref(pi).a.fc();
            let (mx, my) = w.en_ref(i).a.fc();
            (dx, dy) = (px - mx, py - my);
        }
    }
    let f = if dx > 0 && dy > 0 {
        0
    } else if dx > 0 && dy < 0 {
        3
    } else if dx < 0 && dy > 0 {
        2
    } else if dx < 0 && dy < 0 {
        1
    } else {
        return;
    };
    w.en(i).a.facing = f;
}

fn leaf_exit(w: &mut World, c: &C, n: u16, _r: &mut NodeRt) {
    let i = c.i;
    let me = w.ents[i].id;
    let release_if_held = |w: &mut World| {
        if let Some(pi) = en::player(w, i) {
            if w.pl_ref(pi).held_by == me {
                player::release(w, pi);
            }
        }
    };
    if kind(c, n) == K::RunMotionController {
        crate::fieldpatrol::pause(w, i);
    }
    if kind(c, n) == K::RandomStep {
        let e = w.en(i);
        e.a.x = _r.d[0];
        e.a.y = _r.d[1];
        e.jitter = false;
    }
    match kind(c, n) {
        K::Block => {
            w.en(i).a.blocking = false;
            let f = w.g.clock(w.frame);
            w.en(i).state_stamp = f;
        }
        K::ChargedPunch => w.en(i).a.hittable = true,
        // Babidi can be hit again once he leaves the hover (obj+0x1CC back to 1 with the state change to 0xC6)
        K::BabidiHover => w.en(i).a.hittable = true,
        // (field: the node's own exit is a no-op (vtable 0x0804A6C0 +8 = bx lr); the beam belongs to the actor state
        // its enter set (0x0804C690), which goes when the next state sets the actor's - a hit's GotoState into the
        // teleport: the beam stays active (and hits) through Kid Buu's next update and stops in the one after, its
        // retraction starting the frame after that - the real route, 19-3: hit f12345, beam hits f12346 / f12347,
        // retracts from f12348)
        K::KidBuuKamehameha if w.field.is_some() => w.en(i).beam_stop_due = 2,
        K::KidBuuKamehameha => stop_enemy_beam(w, i),
        // exit 0x08033AF6: crit = 0, hit dir = facing ^ 1 (the KnockbackSlide that follows slides it backwards), SetAnim(9)
        K::VampireGrab => {
            release_if_held(w);
            let e = w.en(i);
            e.crit_hit = false;
            e.last_hit_dir = e.a.facing ^ 1;
            set_anim(w, i, &[an(9)], true, Some(false));
        }
        // exit 0x080305E2: player SetAnim(0) (after an escape: a second time)
        K::FreezePlayer => {
            if let Some(pi) = en::player(w, i).filter(|&pi| !w.pl_ref(pi).dead) {
                if w.pl_ref(pi).held_by == me {
                    player::release(w, pi);
                } else {
                    player::set_anim_by_id(w, pi, 0);
                }
            }
        }
        K::DarkSphere => w.en(i).a.hittable = true,
        K::ShockStun => w.en(i).stunned = false,
        // the throw interrupted while the ball charges (a hit's state change): the ball goes, its destructor stops
        // the charge sound (real run 20-6 r062_001 tile +21,000 f418: stop 3 with the punch's sounds)
        K::ThrowBigBall => {
            let me = w.ents[i].id;
            let balls: Vec<usize> = (0..w.ents.len()).filter(|&k| !w.ents[k].removed && matches!(&w.ents[k].body, Body::Shot(s) if s.owner == me && matches!(s.kind, shots::ShotKind::SuperBall { phase: 0, .. }))).collect();
            for k in balls {
                w.ents[k].removed = true;
                crate::vram::entity_gone(w, k);
                w.sfx_stop(3);
            }
        }
        // exit 0x0803BD06 (vt 0x0804ADC0 +8): SetAnim(0) - the idle enter's rand(255) (real run 20-6 r062_001 tile
        // +5,000 f45: Vegeta reaches his waypoint)
        // then the actor showing sprite def 0x34 (`0x08000158(0x34)`: Super Buu's graphics) answers message 5 and the
        // wanderer turns toward it (0x0801599C: 24.8 difference, ties horizontal; real run 20-6 tiles: Vegeta faces
        // Super Buu at each waypoint)
        K::WanderToRandomPoint if w.field.is_some() && std::env::var("NOWEXIT").is_err() => {
            set_anim(w, i, &[nm("idle")], true, Some(true));
            let me = w.ents[i].id;
            let target = (0..w.ents.len()).find(|&k| !w.ents[k].removed && w.ents[k].id != me && crate::scene::sprite_id(w, k) == Some(0x34)).filter(|_| std::env::var("NOFACE").is_err());
            if std::env::var("DBGW").is_ok() { eprintln!("DBGW f{} exit en{} target {:?}", w.frame, i, target); }
            if let Some(k) = target {
                let (tx, ty) = { let a = w.actor(k).unwrap(); (a.x, a.y) };
                let (dx, dy) = (tx - w.en_ref(i).a.x, ty - w.en_ref(i).a.y);
                w.en(i).a.facing = if dx.abs() >= dy.abs() {
                    if dx > 0 { 3 } else { 2 }
                } else if dy > 0 {
                    0
                } else {
                    1
                };
            }
        }
        _ => {}
    }
}

pub fn stop_beam_of(w: &mut World, i: usize) {
    stop_enemy_beam(w, i);
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
    let frame = w.g.clock(w.frame);
    let speed = par(c, n, P::Speed, 0) as i32;
    match kind(c, n) {
        K::Never => false,
        // a mod action: the hook returns 0 while running, non-zero when done
        K::ModAction => {
            let node = &c.tree.nodes[n as usize];
            crate::modkit::ai_hook(w, i, &node.call, &node.args, f) != 0
        }
        K::GotoState => {
            let st = par(c, n, P::State, 0) as u16;
            replace_top(w, i, st);
            true
        }
        K::PushState => {
            // field: the update 0x08018860 pushes on every call and returns 0 - a condition holding it pushes again
            // the next time its tree runs (real run 14-2 r069_059 f334: the beam dodge pushed again after the
            // first was cut short and popped); battles keep the reference's once-per-entry push
            if r.d[0] != 0 && w.field.is_none() {
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
        K::Succeed | K::SelfMessage11 => true,
        // the script seam: the host runs it (`FieldEvent::Run`, cause `Ai`). RunScript queues an event and returns 1
        // at once. InlineScript runs its bytecode inside the update (0x08004038): its blocking natives run nested
        // frames, so the node returns 1 only once the script is over - with a script host attached the node waits
        // until the host reports the end (`scene::ai_script_done`)
        K::RunScript | K::InlineScript => {
            let sc = c.tree.nodes[n as usize].script;
            let inline = matches!(c.tree.nodes[n as usize].k, K::InlineScript);
            if r.d[0] == 1 {
                return !w.scene.ai_wait.contains(&me);
            }
            if sc != 0 && r.d[0] == 0 {
                r.d[0] = 1;
                if inline && w.field.as_ref().is_some_and(|f| f.host) {
                    w.scene.ai_wait.push(me);
                }
                // RunScript (0x0803388C) appends an event object to the world's list: it runs next frame
                if !inline {
                    let run = crate::fieldcontent::Callback::Event(format!("header_0x{sc:08X}"));
                    crate::field::queue_run(w, run, crate::field::Cause::Ai { entity: me });
                } else {
                    w.events.push(crate::world::Event::Field(crate::field::FieldEvent::Run {
                        run: crate::fieldcontent::Callback::Script(format!("inline_0x{sc:08X}")),
                        cause: crate::field::Cause::Ai { entity: me },
                        slot: 0,
                    }));
                }
                return !w.scene.ai_wait.contains(&me);
            }
            true
        }
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
        K::RunMotionController if crate::fieldpatrol::has(w, i) => {
            crate::fieldpatrol::run(w, i);
            false
        }
        K::Idle | K::RunMotionController | K::PartMirrorAnim => false,
        // update 0x08019240: unless IsAnim(0) SetAnim(0) again, then succeed
        K::StandStill => {
            if !w.en_ref(i).is_anim(0) {
                set_anim(w, i, &[nm("idle")], true, Some(true));
            }
            true
        }
        K::FacePlayer => {
            // field: the game's routine (0x0801599C, 24.8 difference, ties horizontal); battles keep the footprint-
            // pixel rule the TypeScript cross-check pins (tests/cross_ts.rs) until it is re-verified there
            face_player_routine(w, i);
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
            // the node enemy_ai.json calls SpawnPuff (create 0x0802F21C, singleton 0x030030FC, vtable 0x08049A5C):
            // its enter 0x0804E53C detaches the enemy from the enemy grid (0x0800F924: obj+0x291 = 1, tiles freed)
            if matches!(kind(c, n), K::SpawnPuff) {
                en::unmark_self(w, i);
                w.en(i).detached = true;
                // field: that is all it does - no effect object, no tiles (HFIL's Janemba Hands: the object list
                // and the tile allocator at the area's start; segments sc_hfil). Battle keeps the reference's puff
                if w.field.is_some() {
                    return true;
                }
            }
            // field: the puff came with the node's enter; the form's sprite changes here, on the update (the
            // Nosferatu, type 0x52: gfx 203 the bat, 202 the vampire - 0x0800EE64 writes the object's gfx def)
            if w.field.is_some() {
                // ... and the bat is off the enemy grid (obj+0x291 = 1: it flies over walls), the vampire back on it
                if matches!(kind(c, n), K::VampireSummon) {
                    en::swap_gfx(w, i);
                    en::unmark_self(w, i);
                    w.en(i).detached = true;
                } else if matches!(kind(c, n), K::VampireSummon2) {
                    en::swap_gfx(w, i);
                    w.en(i).detached = false;
                    en::mark_self(w, i);
                }
                return true;
            }
            let (cx, cy) = w.en_ref(i).a.center();
            crate::fx::puff(w, cx, cy, 0x1800);
            true
        }
        K::SelfDestruct => {
            en::die(w, i, true);
            true
        }
        // the Enzyme's death state 0x91 (singleton 0x03003114, update 0x0802F724): the parent (+0x1F4) count +0x204
        // minus 1 (not modelled), finished at once
        K::LatchRelease => true,
        K::TriggerSelf => {
            // field (world-map encounters): the defeated handler without loot / removal runs the on-defeat script
            if w.field.is_some() {
                crate::field::trigger_self(w, i);
            } else {
                en::die(w, i, false);
            }
            true
        }
        K::Block => false,
        // ---- movement
        K::ChasePlayer | K::ChasePlayerThroughRocks => {
            let Some(pi) = en::player(w, i) else { return false };
            let rocks = kind(c, n) == K::ChasePlayerThroughRocks && w.field.is_some();
            if rocks {
                rock_msg(w);
            }
            r.path.gx = w.pl_ref(pi).a.x;
            r.path.gy = w.pl_ref(pi).a.y;
            let dx = r.path.gx.wrapping_sub(w.en_ref(i).a.x);
            let dy = r.path.gy.wrapping_sub(w.en_ref(i).a.y);
            if ((dx.wrapping_mul(dx) >> 8) as i64) + ((dy.wrapping_mul(dy) >> 8) as i64) <= 0x400 {
                // 0x0802A086: within range SetAnim(0) on every update (the idle enter draws rand(255) each frame)
                set_anim(w, i, &[nm("idle")], true, Some(true));
                if rocks {
                    rock_msg(w);
                }
                return false;
            }
            let mut p = r.path;
            crate::path::update(w, i, &mut p);
            r.path = p;
            if rocks {
                rock_msg(w);
            }
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
        K::FlyToPlayer => en::fly_to_player(w, i, speed),
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
        K::CirclePlayer => en::circle_player(w, i, speed),
        // update 0x08031004: a pending hit impulse is clamped to +-128 on x, else on y; per axis d > 0 ? (v < 128: v +=
        // 38) : (v > -128: v -= 38) (|v| reaches 152); facing by the larger component (ties: y); pos + v
        K::HoverDrift => {
            let (dx, dy) = en::to_player(w, i);
            let (vx, vy) = {
                let e = w.en(i);
                if e.vx.abs() > 0x200 || e.vy.abs() > 0x200 {
                    if e.vx > 128 {
                        e.vx = 128;
                    } else if e.vx < -128 {
                        e.vx = -128;
                    } else {
                        e.vy = e.vy.clamp(-128, 128);
                    }
                }
                if dx > 0 {
                    if e.vx < 128 {
                        e.vx += 38;
                    }
                } else if e.vx > -128 {
                    e.vx -= 38;
                }
                if dy > 0 {
                    if e.vy < 128 {
                        e.vy += 38;
                    }
                } else if e.vy > -128 {
                    e.vy -= 38;
                }
                e.a.facing = if e.vx.abs() > e.vy.abs() {
                    if e.vx > 0 {
                        3
                    } else {
                        2
                    }
                } else if e.vy > 0 {
                    0
                } else {
                    1
                };
                (e.vx, e.vy)
            };
            en::walk_by(w, i, vx, vy, vx != 0 || vy != 0);
            false
        }
        // update 0x080313EA: the counter 48 first, 0 -> finished without moving (47 moves); a step = both
        // components at once, taken back whole when the footprint does not fit (no sliding along a wall)
        K::BackStep => {
            if f >= 48 {
                return true;
            }
            en::try_move(w, i, r.d[0], r.d[1], 0, 0);
            false
        }
        K::KnockbackSlide => {
            let mut kb = kb_of(r);
            let done = en::knock_step(w, i, &mut kb);
            kb_set(r, &kb);
            done
        }
        // update 0x08037A56: no player -> done; else face the player (0x0801599C, the face_actor routine: the
        // dominant axis of the 24.8 difference) every update, then while anim 4 shows step 4 px in that facing;
        // never finishes (real route 13-47: a ninja lunges left at the player, not on in the facing it had)
        K::LungeStep => {
            if en::player(w, i).is_none() {
                return true;
            }
            if w.field.is_some() {
                en::face_player_exact(w, i);
            }
            if w.en_ref(i).ea.pos.raw != 5 {
                return false;
            }
            let (dx, dy) = DIR_VEC[w.en_ref(i).a.facing as usize];
            en::move_by(w, i, dx * 1024, dy * 1024);
            false
        }
        K::FollowMajinBuu => {
            let buu = w.live_enemies().into_iter().find(|&o| matches!(w.en_ref(o).ty, 0x1a | 0x1b | 0x1c));
            let Some(b) = buu else { return false };
            // update 0x0802B3EA: d = Buu's footprint centre - own (0x0800FDFA); always faces him (horizontal only
            // when |dx| > |dy|); from `dist` px on the dominant axis on: one step of d * 256 / max(|dx|, |dy|), taken
            // only when the whole step is free (real route 8-71: Babidi turns before he follows)
            let (bx, by) = w.en_ref(b).a.fc();
            let (x, y) = w.en_ref(i).a.fc();
            let (dx, dy) = (bx - x, by - y);
            w.en(i).a.facing = if dx.abs() > dy.abs() {
                if dx > 0 { 3 } else { 2 }
            } else if dy > 0 {
                0
            } else {
                1
            };
            let m = dx.abs().max(dy.abs());
            if m as i64 >= par(c, n, P::Dist, 50) && m > 0 {
                en::mark_moving(w, i);
                en::try_move(w, i, (dx << 8) / m, (dy << 8) / m, 0, 0);
            }
            false
        }
        // update 0x08032A64 (never finishes; d[0] the turn timer): a = the minor axis' direction toward the player, b
        // the major one; unless aligned (|dx| or |dy| <= 1): probe(a) -> drive(a), else probe(b) -> drive(b), else turn(b)
        K::TankDrive => {
            if r.d[0] != 0 {
                r.d[0] -= 1;
            }
            if en::player(w, i).is_none() {
                return false;
            }
            let (dx, dy) = en::to_player(w, i);
            let mut aligned = dx.abs() <= 1 || dy.abs() <= 1;
            let (a, b): (u8, u8) = if dx.abs() < dy.abs() {
                (if dx >= 0 { 3 } else { 2 }, if dy >= 0 { 0 } else { 1 })
            } else {
                (if dy >= 0 { 0 } else { 1 }, if dx >= 0 { 3 } else { 2 })
            };
            if !aligned {
                if tank_probe(w, i, a) {
                    tank_drive(w, i, a, r);
                } else if tank_probe(w, i, b) {
                    tank_drive(w, i, b, r);
                } else {
                    aligned = true;
                }
            }
            if aligned {
                tank_turn(w, i, b, r);
            }
            false
        }
        // update 0x08037BBC: the kept position + (rand(3) - 1, rand(3) - 1) px, written straight to the actor (no
        // grid test), never finished; exit 0x0804F638 puts the kept position back
        K::RandomStep => {
            let dx = (w.rng.rand(3) - 1) << 8;
            let dy = (w.rng.rand(3) - 1) << 8;
            let e = w.en(i);
            e.a.x = r.d[0] + dx;
            e.a.y = r.d[1] + dy;
            e.jitter = true;
            false
        }
        // ---- melee
        // update 0x0802A276 = IsAnim(0): done once back in the idle state (the punch state's end SetAnim(0))
        K::MeleeAttack => {
            let d = Arc::clone(&w.data);
            let e = w.en_ref(i);
            e.a.anim_name(&d) == Some("idle") && d.anim.rules[e.ea.rule as usize].kind == RuleKind::Idle
        }
        // controller 0x0804C6B4: at counter 16 the hit (standard melee x1.0; accepted -> 48 + rand(2)); ends at 40
        K::BioMechPunch => {
            if f == 16 && en::melee_strike(w, i, 0x400, false, false, atk(2)) {
                melee_sfx(w);
            }
            timed_end(w, i, f, 40)
        }
        // state tick 0x0803CB50: the strike (x1.5) at 48, an accepted hit plays 48 + rand(2); SetAnim(0) at 48 + 8 * frames
        K::HeavyPunch => {
            if f == 48 && en::melee_strike(w, i, 0x600, true, false, atk(2)) {
                melee_sfx(w);
            }
            if f < r.d[0] {
                return false;
            }
            one_idle(w, i);
            true
        }
        // finished as soon as another SetAnim replaced the controller; the swing at 32 (faces the player, x1.0; an
        // accepted hit plays 62); the end SetAnim(0) at 64 + 8 * (frames - 2)
        K::WindupMelee => {
            if w.en_ref(i).anim_seq as i32 != r.d[0] {
                return true;
            }
            // tick 0x08033CF0: not hittable (obj+0x1CC = 0) from the swing at 32 until the last cel starts at
            // 32 + 8 * (frames - 2) (Kyodai Pyramid 14-20: a punch passes through a swinging Skeleton)
            w.en(i).a.hittable = !(f >= 32 && f < r.d[1] - 32);
            if f == 32 && en::melee_strike(w, i, 0x400, true, false, atk(2)) {
                w.sfx(62, 64);
            }
            if f >= r.d[1] {
                set_anim(w, i, &[nm("idle")], true, Some(true));
                return true;
            }
            false
        }
        // update 0x0802F8DA: at 60 the strike 0x0802F77C then SetAnim(4) (the punch state); finished once back in idle
        K::HeavyStrike => {
            if r.d[0] == 0 {
                return true;
            }
            if f == 60 {
                en::heavy_strike(w, i);
                set_anim(w, i, &[an(4)], true, Some(false));
                return false;
            }
            let d = Arc::clone(&w.data);
            f > 60 && w.en_ref(i).a.anim_name(&d) == Some("idle")
        }
        // controller 0x03003278: at tick 0x8C the MeleeAttack hit (x1.5; accepted -> 48 + rand(2)); its end at 148
        // (one SetAnim(0))
        K::ChargedPunch => {
            if f == 140 {
                w.en(i).a.hittable = false;
                if en::melee_strike(w, i, 0x600, true, false, atk(2)) {
                    melee_sfx(w);
                }
            }
            if f >= 148 {
                one_idle(w, i);
                return true;
            }
            false
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
        // update 0x0802C6EC: after a contact 16 frames back along -v; else pos += v (no collision test) and a kind-1
        // message (the player's footprint) with the body box, melee x1.0: accepted -> 16 frames back
        K::DriftContact => {
            if r.d[2] > 0 {
                r.d[2] -= 1;
                let a = &mut w.en(i).a;
                a.x -= r.d[0];
                a.y -= r.d[1];
                return false;
            }
            {
                let a = &mut w.en(i).a;
                a.x += r.d[0];
                a.y += r.d[1];
            }
            if let Some(pi) = en::player(w, i) {
                let a = &w.en_ref(i).a;
                let mut m = HitMsg { team: 0, rect: a.body_box(), dir: a.facing, mult: 0x400, atk: atk(2), attacker: me, att_stats: a.stats, att_is_player: false, source: me, crit: false, raw: 0, dmg: 0, no_react: false, foot: true };
                if player::on_message(w, pi, &mut m) {
                    r.d[2] = 16;
                }
            }
            false
        }
        // ---- ranged
        // the player's spawner 0x08019858: origin 0x0800FF44 on the body box; the blast tests its record's box (shots::ki_blast_box)
        // (the ki blast is its controller's work too: a hit's hurt state replaced it - after the pop the node is over;
        // segment sb 6-6 f18914)
        K::KiBlast if w.field.is_some() && r.d[6] != w.en_ref(i).anim_seq as i32 => true,
        K::KiBlast => {
            if f == 16 {
                let (mx, my) = en::hand_point(w, i);
                let dir = w.en_ref(i).a.facing;
                let id = shots::ki_blast(w, me, dir, 0, mx << 8, my << 8, atk(4), 1024, true);
                if let Some(k) = w.find(id) {
                    if let shots::ShotKind::Straight(o) = &mut w.shot(k).kind {
                        o.b = [8, 8, 16, 16];
                        o.rec_box = true;
                    }
                }
            }
            timed_end(w, i, f, 32)
        }
        // field: the shot is the actor's animation state's (controller) work - a state that was replaced meanwhile
        // (a hurt animation: Volcano 15-25, a Cursed One punched mid-throw) fires nothing and the node is over
        K::GunShot | K::RifleShot | K::NinjaStarThrow | K::MummyWrapShot | K::MummyWrapBurst if w.field.is_some() && r.d[6] != w.en_ref(i).anim_seq as i32 => true,
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
        // controller 0x030030EC (tick 0x0802E278): the shell when the animation reaches frame 2 (tick 16); the node is
        // finished once the controller is gone (the once-state's end: SetAnim(0) at tick 32) (3-14: the shell sounds
        // 16 frames after the GotoState)
        K::TankShoot => {
            if r.d[0] == 0 && w.en_ref(i).ea.pos.raw == r.d[1] && w.en_ref(i).ea.pos.frame == 2 {
                r.d[0] = 1;
                en::shot(w, i, ShotType::Shell);
            }
            f > 1 && w.en_ref(i).ea.pos.raw != r.d[1]
        }
        // the wave (0x0802A2EC) at tick 8 from the per-facing muzzle; the soldier's ends at tick >= 32 once its wave is
        // gone, the turret's at 72: SetAnim(0)
        K::SoldierThrow | K::TurretFire => {
            let soldier = kind(c, n) == K::SoldierThrow;
            // the node's update (0x08026726) is finished as soon as the controller is no longer the actor's state
            // object: a hit's reaction replaced it (real route 8-31: a soldier hurt mid-throw moves on once it recovers)
            if w.en_ref(i).ea.pos.raw != r.d[2] {
                return true;
            }
            if f == 8 {
                let (x, y) = en::muzzle_fix(w, i, !soldier);
                let dir = w.en_ref(i).a.facing;
                let s = shots::energy_wave(w, me, dir, x, y);
                r.d[0] = shots::add(w, s) as i32;
                if soldier {
                    w.g.soldier_wave = r.d[0];
                }
            }
            if if soldier { f >= 32 && !alive(w, w.g.soldier_wave) } else { f >= 72 } {
                set_anim(w, i, &[nm("idle")], true, Some(true));
                return true;
            }
            false
        }
        // tick 0x0802F5F8: counter 1 spawns the wave; from 16 on, once the wave is gone: SetAnim(0), finished
        K::CyborgShot => {
            if f == 1 {
                let (x, y) = en::muzzle_fix(w, i, false);
                let dir = w.en_ref(i).a.facing;
                let s = shots::energy_wave(w, me, dir, x, y);
                r.d[0] = shots::add(w, s) as i32;
            }
            if f < 16 || alive(w, r.d[0]) {
                return false;
            }
            set_anim(w, i, &[an(0)], true, Some(true));
            true
        }
        // finished on the update that sees the anim frame at 3
        K::TurretOpen | K::TurretClose => {
            let done = w.en_ref(i).ea.pos.frame >= 3;
            if done && w.field.is_some() && kind(c, n) == K::TurretClose {
                w.en(i).a.hittable = true;
            }
            done
        }
        // controller 0x0203DD04: the bomb at tick 16 from the position + 0x0874CAD4[dir], SetAnim(0) at 24
        K::BombThrow => {
            // a hit's reaction replaced the controller (2-10: a bomber hurt mid-throw throws nothing once it recovers)
            if w.en_ref(i).ea.pos.raw != r.d[2] {
                return true;
            }
            if f == 16 {
                const BOMB_MUZZLE: [(i32, i32); 4] = [(12, -8), (12, -8), (16, -16), (0, -16)];
                let a = &w.en_ref(i).a;
                let (ox, oy) = BOMB_MUZZLE[a.facing as usize];
                let (dir, x, y) = (a.facing, (a.px() + ox) << 8, (a.py() + oy) << 8);
                shots::add(w, shots::bomb(me, dir, x, y, r.d[0], r.d[1]));
            }
            timed_end(w, i, f, 24)
        }
        // the controller 0x03002FA8 spawns a flame every tick 4..127; at 128: SetAnim(0) and the sound 63 stopped
        K::FlameBreath => {
            if (4..128).contains(&f) {
                const NOZ: [(i32, i32); 4] = [(46, 10), (16, -48), (-6, -12), (74, -12)];
                let a = &w.en_ref(i).a;
                let (ox, oy) = NOZ[a.facing as usize];
                let (dir, x, y) = (a.facing, (a.px() + ox) << 8, (a.py() + oy) << 8);
                let s = shots::flame(w, me, dir, x, y);
                shots::add(w, s);
            }
            if f == 128 {
                set_anim(w, i, &[nm("idle")], true, Some(true));
                w.events.push(crate::world::Event::SfxStopMatch { id: 63 });
            }
            f >= 128
        }
        // controller tick 0x08037D64: t < 24 frame (t >> 2) & 1, < 28 frame 2, < 32 frame 3; t 28 the arm at the
        // position + 0x0876CCB4[dir]; from 32 SetAnim(0) once the arm is gone
        K::GrappleArm => {
            if f < 32 {
                w.en(i).ea.pos.frame = if f < 24 { (f >> 2) & 1 } else if f < 28 { 2 } else { 3 };
                if f == 28 {
                    const O: [(i32, i32); 4] = [(7, 16), (-12, -56), (-24, -20), (48, -20)];
                    let a = &w.en_ref(i).a;
                    let (ox, oy) = O[a.facing as usize];
                    let (dir, x, y) = (a.facing, (a.px() + ox) << 8, (a.py() + oy) << 8);
                    r.d[0] = shots::add(w, shots::grapple_arm(me, dir, x, y)) as i32;
                }
                return false;
            }
            if alive_ent(w, r.d[0]) {
                return false;
            }
            set_anim(w, i, &[nm("idle")], true, Some(true));
            true
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
        // the beam fires at tick 8 (stop 3, sound 32); the state ends once tick > 90 (SetAnim(0); the exit stops the
        // beam: it retracts)
        K::KidBuuKamehameha => {
            // (field: the pose is the actor state 0x0804C690's - cels 0, 1, 2 every 8 ticks, then 1 / 2 alternating
            // until the state ends, never the one-shot's end and its idle: the real route 19-3, a copy's beam)
            if w.field.is_some() {
                let c = if f <= 8 { 0 } else { (f - 1) >> 3 };
                let e = w.en(i);
                e.ea.pos.frame = if c < 3 { c } else { 1 + ((c - 1) & 1) };
                e.ea.pos.tick = f;
                e.ea.done = false;
                e.anim_ended = false;
            }
            if f == 9 {
                if let Some(k) = w.find(w.en_ref(i).beam) {
                    shots::kame_fire(w, k);
                }
            }
            if f >= 91 {
                set_anim(w, i, &[nm("idle")], true, Some(true));
                stop_enemy_beam(w, i);
                return true;
            }
            false
        }
        // update 0x080500A4: done once the controller tick is > 8 (the 9th update)
        K::StoneSpit => f >= 9,
        K::AimAtPlayer => true,
        // ---- teleports
        K::NinjaTeleport => {
            en::ninja_teleport(w, i);
            true
        }
        K::TeleportNearPlayer => {
            if f == 36 {
                en::teleport_beside(w, i, r.d[0]);
            }
            if w.field.is_some() {
                // field (sc_janemba): hittable again 18 frames after the landing; the landing animation (0x14) plays
                // 36 frames and its end SetAnim(0)s the boss - the node's end, MeleeAttack follows
                if f >= 54 {
                    w.en(i).a.hittable = true;
                }
                if f >= 72 {
                    en::set_anim(w, i, &[an(0)], true, Some(true));
                    return true;
                }
                return false;
            }
            f >= 54
        }
        K::TeleportChain4 => {
            // field (sc_janemba): the jump 36 frames after the vanish shows, to side = counter & 3, hittable again 18
            // later. Battle keeps the reference's
            if w.field.is_some() && f % 72 == 54 {
                w.en(i).a.hittable = true;
            }
            if w.field.is_some() {
                // the node's check (0x08034502): out of the vanish / landing animations (a hurt that cut the
                // vanish, state 0x2DD's reverse, the landing's end below) -> the persistent side counter
                // (0x03003228) steps, a round one ends the chain, else the teleport restarts from this frame
                // (sc_janemba: 0 -> 1 at 33,981, 1 -> 2 at 37,297, 10 -> 11 at 38,045)
                let raw = w.en_ref(i).ea.pos.raw;
                if f >= 1 && raw != r.d[2] && raw != r.d[3] {
                    w.g.tele_chain = (w.g.tele_chain + 1) % 12;
                    if w.g.tele_chain % 4 == 0 {
                        return true;
                    }
                    set_anim(w, i, &[an(17)], true, Some(false));
                    w.en(i).a.hittable = false;
                    r.d[2] = w.en_ref(i).ea.pos.raw;
                    r.f = 0;
                    return false;
                }
                if f % 72 == 36 {
                    en::teleport_beside(w, i, w.g.tele_chain & 3);
                    r.d[3] = w.en_ref(i).ea.pos.raw;
                }
                // the landing animation's end (36 frames) SetAnim(0)s the boss; the node steps the counter: a round
                // counter ends the chain, else the next vanish (anim 0x11, hits off)
                if f > 0 && f % 72 == 0 {
                    en::set_anim(w, i, &[an(0)], true, Some(true));
                    w.g.tele_chain = (w.g.tele_chain + 1) % 12;
                    if w.g.tele_chain % 4 == 0 {
                        return true;
                    }
                    set_anim(w, i, &[an(17)], true, Some(false));
                    w.en(i).a.hittable = false;
                    r.d[2] = w.en_ref(i).ea.pos.raw;
                    r.f = 0;
                }
                return false;
            }
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
            // field: the node's enter did it (leaf_enter)
            if w.field.is_some() {
                return true;
            }
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
                // field (13-48): the controller 0x03003080 (update 0x0802C36A) ends at tick 64 with SetAnim(0) - the
                // position stays where the last jitter put it; the node (0x0802C3D2) then puffs at the box centre
                // with the rate 0x1000 + rand(0x1000). Battles keep the rule the TypeScript cross-check pins
                if w.field.is_some() {
                    set_anim(w, i, &[an(0)], true, Some(true));
                    let (cx, cy) = w.en_ref(i).a.center();
                    crate::fx::puff_rng(w, cx, cy);
                    return true;
                }
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
        // update 0x08033A2A: done when the player's HP <= 0 or the player left the hold; keys A/B/d-pad pressed this
        // frame (input manager +4, the press edge: held keys do not count - real route, Pilaf's Castle) -> stamp,
        // count + 1, > 8 -> player SetAnim(0), done; no press for > 10 frames -> count 0; the frame counter & 15 == 0 ->
        // the player loses POW HP (no formula) and the vampire heals the same
        K::VampireGrab => {
            let Some(pi) = en::player(w, i).filter(|&pi| w.pl_ref(pi).hp() > 0 && w.pl_ref(pi).held_by == me) else { return true };
            // update 0x08033A2A: the player's state object (player +0x14) is no longer the held one -> finished
            // (field: an exit that fired under the held player started the autowalk; HFIL 18-8, sc_hfil)
            if w.field.is_some() && matches!(w.pl_ref(pi).gs, player::Gs::AutoWalk) {
                return true;
            }
            let now = w.g.clock(w.frame) as i32;
            let slot = w.pl_ref(pi).slot as usize;
            if w.input[slot].pressed & 0xf3 != 0 {
                r.d[1] = now;
                r.d[0] += 1;
                if r.d[0] > 8 {
                    player::release(w, pi);
                    return true;
                }
            } else if now - r.d[1] > 10 {
                r.d[0] = 0;
            }
            if now & 15 == 0 {
                let pow = w.en_ref(i).a.stats.pow;
                player::lose_hp(w, pi, pow, 0);
                en::heal(w, i, pow as i64 * 1024);
            }
            w.pl_ref(pi).hp() <= 0
        }
        // update 0x08029902: no player / KO -> finished; the frame counter & 63 == 0 -> 1 HP (no formula)
        K::LatchDrain => {
            let Some(pi) = en::player(w, i).filter(|&pi| !w.pl_ref(pi).dead && w.pl_ref(pi).hp() > 0) else { return true };
            if w.g.clock(w.frame) & 63 == 0 {
                // through the player's apply-damage (vfunc -> `0x08010BF2`): its damage number too (real route 21-6
                // f176; field only - the number is a tile user)
                crate::vram::damage_number(w, w.ents[pi].id);
                player::lose_hp(w, pi, 1, 0);
            }
            false
        }
        // frame t >> 3 while below the frame count; its end: SetAnimRaw(0x13) under 0x03001B30, finished
        K::GrabWindup => {
            if (f >> 3) < r.d[0] {
                w.en(i).ea.pos.frame = f >> 3;
                return false;
            }
            let g = w.data.anim.generic["0x03001B30"];
            set_anim_r(w, i, &[an(19)], false, g);
            true
        }
        // update 0x0802C645: no partner -> finished. Phase 0: the partner showing SetAnim(9)'s animation -> release;
        // showing id 0x11 (draining) -> the presses are ignored; else a d-pad press toggles the player's frame and each
        // 0x08040284(d-pad, 20, 90) success counts, the 5th releases. Release: the player back at the kept position,
        // SetAnim(0), Spopovich SetAnim(9), phase 1: 60 more ticks
        K::BearHug => {
            let Some(yamu) = w.live_enemies().into_iter().find(|&o| w.ents[o].id as i32 == r.d[3] && r.d[3] != 0) else { return true };
            if r.d[0] == 1 {
                let c0 = r.d[1];
                r.d[1] -= 1;
                return c0 == 0;
            }
            let p = en::player(w, i);
            let release = |w: &mut World, r: &mut NodeRt| {
                if let Some(pi) = p {
                    let pl = w.pl(pi);
                    pl.a.x = r.d[4];
                    pl.a.y = r.d[5];
                    player::to_idle(w, pi);
                }
                set_anim(w, i, &[an(9)], true, Some(false));
                r.d[0] = 1;
                r.d[1] = 60;
            };
            if is_anim_id(w, yamu, 9) {
                release(w, r);
                return false;
            }
            let Some(pi) = p else { return false };
            if is_anim_id(w, yamu, 0x11) {
                return false;
            }
            let slot = w.pl_ref(pi).slot as usize;
            if w.g.pressed(slot) & 0xf0 != 0 {
                w.pl(pi).ap.frame ^= 1;
            }
            if w.g.press_run(slot, 0xf0, 20, 90, w.frame) {
                r.d[2] += 1;
                if r.d[2] == 5 {
                    release(w, r);
                }
            }
            false
        }
        // Yamu's drain (update 0x0802FC5D; d: 0 phase, 1 spot found, 2 counter, 3 partner, 4 target EP, 5 mover). Phase
        // 0: find the spot Spopovich + (40, 1) when right of it, else / if blocked + (-40, 1), walk there (straight-line
        // mover, speed 0x100); Spopovich showing SetAnim(9)'s animation -> finished; arrived -> face Spopovich's side,
        // phase 1 (SetAnimRaw(0x12)), target EP = EP - maxEP / 4. Phase 1: EP == target -> SetAnim(9), phase 2 (60
        // ticks); else counter + 1: every 8th EP - 1, every 32nd sound 19
        K::EnergyDrain => {
            let p = en::player(w, i);
            if r.d[0] == 2 {
                r.d[2] = (r.d[2] - 1) & 0xff;
                return r.d[2] == 0xff;
            }
            if r.d[0] == 1 {
                let Some(pi) = p else { return false };
                if w.pl_ref(pi).ep() == r.d[4] {
                    set_anim(w, i, &[an(9)], true, Some(false));
                    r.d[0] = 2;
                    r.d[2] = 60;
                    return false;
                }
                r.d[2] = (r.d[2] + 1) & 0xff;
                if r.d[2] & 7 == 0 {
                    w.pl(pi).rec.ep -= 1;
                }
                if r.d[2] & 31 == 0 {
                    w.sfx(19, 64);
                }
                return false;
            }
            if r.d[1] == 0 {
                let spo = w.live_enemies().into_iter().find(|&o| matches!(w.en_ref(o).ty, 0x6d..=0x70));
                r.d[3] = spo.map(|o| w.ents[o].id as i32).unwrap_or(0);
                let Some(spo) = spo else { return true };
                let (sx, sy) = (w.en_ref(spo).a.x, w.en_ref(spo).a.y);
                en::unmark_self(w, i);
                let mut tx = 0;
                let mut ty = sy + 0x100;
                if w.en_ref(i).a.x > sx && en::fits(w, i, (sx + (40 << 8)) >> 8, ty >> 8) {
                    tx = sx + (40 << 8);
                } else if en::fits(w, i, (sx - (40 << 8)) >> 8, ty >> 8) {
                    tx = sx - (40 << 8);
                } else {
                    ty = -1;
                }
                en::mark_self(w, i);
                if ty >= 0 {
                    r.d[1] = 1;
                    r.path = line_mover(w, i, tx >> 8, ty >> 8, 0x100);
                    r.d[5] = 1;
                }
            }
            let spo = w.live_enemies().into_iter().find(|&o| r.d[3] != 0 && w.ents[o].id as i32 == r.d[3]);
            if spo.is_some_and(|s| is_anim_id(w, s, 9)) {
                return true;
            }
            if r.d[1] == 0 || r.d[5] == 0 {
                return false;
            }
            let mut m = r.path;
            let arrived = line_mover_step(w, i, &mut m);
            r.path = m;
            if !arrived {
                return false;
            }
            let right = spo.is_some_and(|s| w.en_ref(i).a.x > w.en_ref(s).a.x);
            w.en(i).a.facing = if right { 2 } else { 3 };
            r.d[0] = 1;
            r.d[5] = 0;
            let h = w.data.anim.hold;
            set_anim_r(w, i, &[an(18)], true, h);
            r.d[4] = match p {
                Some(pi) if w.pl_ref(pi).ep() > w.pl_ref(pi).max_ep() >> 2 => w.pl_ref(pi).ep() - (w.pl_ref(pi).max_ep() >> 2),
                _ => 0,
            };
            false
        }
        // update 0x0804FF94: done once the fragment left its freeze state (the escape SetAnim(0)s it)
        K::FreezePlayer => w.en_ref(i).hook != en::Hook::Freeze,
        // controller tick 0x0802AF11: t 0 the player becomes the candy-eat state at the enemy + 0x08752DF8[dir]; t 8 it
        // leaves the world; t 24..111 every 8th min(POW >> 2, HP) eaten (0 -> 1 and the KO flag), every 32nd sound 11;
        // t 120 a new player object at the kept position; t 160 SetAnim(0)
        K::EatCandy => {
            let p = en::player(w, i);
            if f == 1 {
                if let Some(pi) = p {
                    r.d[0] = w.pl_ref(pi).a.x;
                    r.d[1] = w.pl_ref(pi).a.y;
                    r.d[2] = 0;
                    const O: [(i32, i32); 4] = [(-8, -20), (-8, -20), (-8, -20), (48, -20)];
                    let a = &w.en_ref(i).a;
                    let (ox, oy) = O[a.facing as usize];
                    let (x, y) = ((a.px() + ox) << 8, (a.py() + oy) << 8);
                    player::candy_eat(w, pi, x, y);
                }
            }
            if f < 24 {
                if f == 8 {
                    if let Some(pi) = p {
                        player::candy_gone(w, pi);
                    }
                    // 0x0802AFC0: a follow object (0x0804D300) on the eater takes the camera before the player object
                    // goes (real route 8-71: the camera eases to Buu while Vegeta is eaten); the new player object
                    // takes it back. Field only: battles keep the reference's camera
                    if w.field.is_some() {
                        crate::scene::follow(w, me);
                        // the player object leaves the world list: created before the eater, the list compacts
                        // and the eater is updated again in this pass (its tick goes 7 -> 9 in that frame)
                        if p.is_some_and(|pi| pi < i) {
                            if let Some(fl) = w.field.as_mut() {
                                fl.list_redo = true;
                            }
                        }
                    }
                }
                w.en(i).ea.pos.frame = f >> 3;
            } else if f < 112 {
                w.en(i).ea.pos.frame = 3 + (((f + 8) >> 3) & 1);
                if f & 7 == 0 {
                    if let Some(pi) = p {
                        let mut d = w.en_ref(i).a.stats.pow >> 2;
                        let hp = w.pl_ref(pi).rec.hp;
                        if hp <= d {
                            d = hp;
                        }
                        w.pl(pi).rec.hp -= d;
                        if w.pl_ref(pi).rec.hp == 0 {
                            w.pl(pi).rec.hp = 1;
                            r.d[2] = 1;
                        }
                        en::heal(w, i, d as i64 * 1024);
                    }
                }
                if f & 31 == 0 {
                    w.sfx(11, 64);
                }
            } else if f < 128 {
                w.en(i).ea.pos.frame = 5 + ((f - 112) >> 3);
                if f == 120 {
                    if let Some(pi) = p {
                        player::candy_respawn(w, pi, (r.d[0] >> 8) << 8, (r.d[1] >> 8) << 8, r.d[2] != 0);
                        if w.field.is_some() && w.scene.cam.mode == crate::scene::CamMode::Follow(me) {
                            w.scene.cam.mode = crate::scene::CamMode::Player;
                        }
                        // the new player object is the list's newest: not updated in this pass; its hit-damage
                        // remainder starts at 0 like any rebuilt player's (8-71: 311 -> 307 after the candy)
                        if w.field.is_some() {
                            if let Some(np) = w.player_idx(0) {
                                w.pl(np).hp_acc = Default::default();
                            }
                        }
                        if let Some(fl) = w.field.as_mut() {
                            fl.player_new = true;
                        }
                    }
                }
            } else if f >= 160 {
                set_anim(w, i, &[nm("idle")], true, Some(true));
                return true;
            }
            false
        }
        // ---- Buu
        // controller tick 0x0802AB1D: frames 0 / 1-2 / 3-4; t 64 the arm at the position + 0x08752D6C[dir] (def 0x2B) or
        // 0x08752D8C (10 px higher sideways), sound 8; from 72 SetAnim(0) once the arm is gone
        K::BuuStretchPunch => {
            if r.d[0] == 0 {
                return true;
            }
            // field: the arm is the work of the animation's controller; a hit's hurt state replaced it - the node is
            // over after the pop (segment sb 6-6 f19062, as CandyBeam / KiBlast)
            if w.field.is_some() && f < 72 && r.d[6] != w.en_ref(i).anim_seq as i32 {
                return true;
            }
            if f < 72 {
                w.en(i).ea.pos.frame = if f < 8 {
                    0
                } else if f < 56 {
                    1 + (((f + 8) >> 3) & 1)
                } else {
                    3 + ((f - 56) >> 3)
                };
                if f == 64 {
                    let gfx = w.data.enemy(w.en_ref(i).ty).and_then(|t| t.gfx);
                    let o: [(i32, i32); 4] = if gfx == Some(0x2b) { [(9, 16), (15, -56), (-24, -16), (48, -16)] } else { [(9, 16), (15, -56), (-24, -26), (48, -26)] };
                    let a = &w.en_ref(i).a;
                    let (ox, oy) = o[a.facing as usize];
                    let (dir, x, y) = (a.facing, a.px() + ox, a.py() + oy);
                    r.d[1] = shots::add(w, shots::stretch_arm(me, dir, x, y)) as i32;
                    w.sfx(8, 64);
                }
                return false;
            }
            if alive_ent(w, r.d[1]) {
                return false;
            }
            set_anim(w, i, &[nm("idle")], true, Some(true));
            true
        }
        // controller tick 0x0802B221: frame t >> 3 below 32, t 16 the beam at the position + 0x08752E28[dir] (def 0x2B)
        // or 0x08752E48; from 32 SetAnim(0) once it is gone
        K::CandyBeam => {
            if r.d[0] == 0 {
                return true;
            }
            // field: the beam is the work of the animation's controller (0x0300304C); a hit's hurt state replaced it
            // meanwhile - the node is over (segment sb 6-6 f18331, like the shot nodes)
            if w.field.is_some() && f < 32 && r.d[6] != w.en_ref(i).anim_seq as i32 {
                return true;
            }
            if f < 32 {
                w.en(i).ea.pos.frame = f >> 3;
                if f == 16 {
                    const OFF: [(i32, i32); 4] = [(12, -8), (14, -38), (-16, -34), (40, -34)];
                    const OFF_2B: [(i32, i32); 4] = [(12, -8), (14, -38), (-16, -24), (40, -24)];
                    let gfx = w.data.enemy(w.en_ref(i).ty).and_then(|t| t.gfx);
                    let a = &w.en_ref(i).a;
                    let (ox, oy) = if gfx == Some(0x2b) { OFF_2B } else { OFF }[a.facing as usize];
                    let (dir, x, y) = (a.facing, a.px() + ox, a.py() + oy);
                    r.d[1] = shots::add(w, shots::candy_beam(me, dir, x, y)) as i32;
                }
                return false;
            }
            if alive_ent(w, r.d[1]) {
                return false;
            }
            set_anim(w, i, &[nm("idle")], true, Some(true));
            true
        }
        K::AbsorbStance => f >= 8,
        K::AbsorbPose | K::AbsorbPose2 => {
            let d = Arc::clone(&w.data);
            f >= 8 && w.en_ref(i).anim_done(&d)
        }
        // update 0x0804FEC4: finished once the object's controller is no longer the one-shot 0x03001B30 - also when
        // another node replaced it (real run 6-9 r052_006 f15,334: state 0x251's sub-state jumps to 0x259 and the old
        // tree's Sequence still enters its KiBlast that frame - controller 0x03003068 - so the power-up ends next frame)
        K::SuperBuuPowerUp => {
            let d = Arc::clone(&w.data);
            let g = w.data.anim.generic["0x03001B30"];
            w.en_ref(i).ea.rule != g || (f >= 32 && w.en_ref(i).anim_done(&d))
        }
        K::ThrowBigBall => {
            // the throw's controller (0x0804D6CC) ends 32 frames after the throw with SetAnim(0) - the idle enter's
            // rand(255) - and the node finishes in the same update (real run 20-6 r062_001 tile +5,000 f52)
            if f >= 92 && std::env::var("NOTBI").is_err() {
                set_anim(w, i, &[nm("idle")], true, Some(true));
            }
            f >= 92
        }
        K::GlideToWaypoint => {
            // (field: after its 60 moves a 61st update ends it - the exit 0x0803C064 puts it on the nearest whole
            // pixel and SetAnim(0)s it (an idle draw); the state change after it comes the next frame (real run
            // 20-6/20-7 r062_001, Super Buu inside Buu: 303.98 -> 304, 127.91 -> 128, 112.03 -> 112)
            if w.field.is_some() && f > 60 {
                {
                    let e = w.en(i);
                    e.a.x = (e.a.x + 128) & !0xFF;
                    e.a.y = (e.a.y + 128) & !0xFF;
                }
                set_anim(w, i, &[nm("idle")], true, Some(true));
                return true;
            }
            {
                let e = w.en(i);
                e.a.x += r.d[0];
                e.a.y += r.d[1];
            }
            en::clamp_inside(w, i);
            w.field.is_none() && f >= 60
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
        // SetAnimRaw(0xC) under 0x03001B30; finishes on the 56th update
        K::KidBuuSplitAnim => f >= 56,
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
            // controller tick 0x0802AD58: the tick runs 0..23, then 8..23 again; cel = tick >> 3 (no draw)
            let t = if f < 24 { f } else { 8 + (f - 8) % 16 };
            let e = w.en(i);
            e.ea.pos.tick = t;
            e.ea.pos.frame = t >> 3;
            false
        }
        // controller tick 0x0802AE3A: down 1 px a frame; on the ground 8 ticks, then SetAnim(0) (the idle's
        // rand(255)) - the node (0x0802AE98) is finished once the controller is gone
        K::BabidiGesture => {
            if w.en_ref(i).a.z < 0 {
                w.en(i).a.z += 1;
                return false;
            }
            r.d[0] += 1;
            if r.d[0] >= 8 {
                set_anim(w, i, &[an(0)], true, Some(true));
                return true;
            }
            w.en(i).ea.pos.frame = r.d[0] >> 3;
            false
        }
        // ---- Janemba
        K::JanembaSpawnParts => {
            en::janemba_parts(w, i);
            true
        }
        // the part's controller is set by the node's first update; the hand on its 16th tick; its end SetAnim(0)s the
        // arm part (rand(255))
        K::JanembaHandStrikeA | K::JanembaHandStrikeB if w.field.is_some() => {
            // field: the node's first update gives the arm part the strike controller (it makes the hand and ends
            // the arm's animation itself: enemy.rs janemba_part_field); the node waits it out
            let arm = en::janemba_arm(w, i, kind(c, n) == K::JanembaHandStrikeA);
            // (not once Janemba is down: the killing punch that cut a strike starts no next one - the real route's
            // arena: all three parts end their hurt animation 27 frames into the defeat scene, three draws)
            if f == 1 && w.en_ref(i).hp1024 > 0 {
                if let Some(k) = arm {
                    // SetAnimRaw(0x12) + the strike controller (no state rule, no start draw): 8 frames a cel
                    let rule = w.data.anim.fallback_once;
                    en::set_anim_rule(w, k, &[an(18)], true, Some(false), Some(rule));
                    w.en(k).strike = (1, kind(c, n) == K::JanembaHandStrikeA);
                }
            }
            // done when the arm's strike ends - also when Janemba's hurt flash cut it (sc_janemba: ten punches, ten
            // strikes of state 0xff, then the ascent)
            f >= 57 || (f > 1 && arm.is_none_or(|k| w.en_ref(k).strike.0 == 0))
        }
        K::JanembaHandStrikeA | K::JanembaHandStrikeB => {
            if f == 17 {
                let _ = en::hand_strike(w, i, kind(c, n) == K::JanembaHandStrikeA);
            }
            if f >= 57 {
                // the arm's SetAnim(0) (its idle start draw)
                w.rng.rand(255);
                return true;
            }
            false
        }
        K::JanembaAscend => {
            if f >= 8 {
                w.en(i).a.z = (24 - 3 * f).max(-200);
            }
            f > 128
        }
        // field (sc_janemba): landed (67 frames), the animation plays back (cel 1 for 8 frames, then cel 0); 16 frames
        // later the screen shake (0x0800797C(0x46, 0x1E)); 16 more: the minis are topped up to 5 and the body goes back
        // to idle (SetAnim(0)) - the node's end (minis 229 frames after the ascent began)
        K::JanembaDescendSummon if w.field.is_some() => {
            if r.d[1] == 1 {
                r.d[1] = 0;
                return false;
            }
            if w.en_ref(i).a.z < 0 {
                let z = (w.en_ref(i).a.z + 3).min(0);
                w.en(i).a.z = z;
                if z == 0 {
                    r.d[0] = 32;
                    w.en(i).ea.pos.frame = 1;
                }
                return false;
            }
            if r.d[0] > 0 {
                r.d[0] -= 1;
                let k = 32 - r.d[0];
                w.en(i).ea.pos.frame = if k < 8 { 1 } else { 0 };
                if r.d[0] == 16 {
                    // 0x08039A90: the shake, then a puff (rate 0x1000 + rand(0x1000)) at each missing mini's spot; the
                    // minis come out 16 frames later
                    w.shake(70, 30);
                    r.d[2] = en::mini_puffs(w) as i32;
                }
                if r.d[0] == 0 {
                    en::mini_create(w, i, r.d[2] as usize);
                    en::set_anim(w, i, &[an(0)], true, Some(true));
                    return true;
                }
            }
            false
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
        // update 0x080346FC: tick + 8 (wraps at 0x100); at 0 re-aim with chance(0x4000); > 0x80: the whole step
        // 0x087593F8[dir] (+-0.5 px per axis), no sliding
        K::MiniHopToward => {
            r.d[0] = (r.d[0] + 8) & 0xff;
            let ph = r.d[0];
            w.en(i).a.z = if ph >= 0x80 { -HOP_ARC8[((ph - 0x80) >> 3) as usize] } else { 0 };
            if ph == 0 && w.rng.chance(0x4000) {
                mini_aim(w, i);
            }
            if ph > 0x80 {
                const V: [(i32, i32); 4] = [(128, 128), (-128, -128), (-128, 128), (128, -128)];
                let (vx, vy) = V[w.en_ref(i).a.facing as usize];
                en::try_move(w, i, vx, vy, 0, 0);
            }
            false
        }
        K::MiniHopFlee | K::Hop => {
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
                // field (sc_janemba): the dash is a collision-checked step without sliding (a mini blocked by
                // another stays put); battle keeps the reference's free move
                if w.field.is_some() {
                    en::try_move(w, i, r.d[0], r.d[1], 0, 0);
                } else {
                    en::move_by(w, i, r.d[0], r.d[1]);
                }
            }
            if f == 32 {
                // field: the hit box is 0x0800FE78's, at the ground position (sc_janemba: a mini mid-hop hits)
                let a = &w.en_ref(i).a;
                let bb = if w.field.is_some() { a.body_box_ground() } else { a.body_box() };
                en::area_hit(w, i, bb, atk(0), 0x400);
                return true;
            }
            false
        }
        K::HandAmbush if w.field.is_some() => {
            if f == 1 {
                if let Some(pi) = en::player(w, i) {
                    let left = w.rng.rand(2) != 0;
                    // (the 24.8 position, sub-pixels kept)
                    let (ppx, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
                    let e = w.en(i);
                    let off: i32 = if left { 4 } else { -20 };
                    e.a.x = ppx + (off << 8);
                    e.a.y = py;
                    e.a.facing = if left { 2 } else { 3 };
                }
                set_anim(w, i, &[an(17)], true, Some(false));
                w.en(i).a.alpha = 256;
            }
            let f = f - 1;
            if (80..=103).contains(&f) && r.d[0] == 0 {
                let bb = w.en_ref(i).a.body_box();
                if en::area_hit(w, i, bb, atk(2), 0x400) {
                    r.d[0] = 1;
                    // 0x0803B3D8: the strike that landed plays sound 48 + rand(2) (HFIL 18-12, sc_hfil)
                    let s = 48 + w.rng.rand(2);
                    w.sfx(s, 64);
                }
            }
            // (the strike's animation ends at tick 0x68, 0x0803B3FC: done in that update, so the next state (0x117,
            // HandSink) is the one the game shows in the following frame - real route 18-1 r038_045: a punch in that
            // frame meets the sinking hand, not a still-striking one; HandSink runs one frame longer to keep the total)
            f >= 104
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
            // update 0x0803B30E: unless the hand still shows its ambush strike (the object's current animation =
            // its def's animation table entry 0x54 / 4) the node is done at once - no sink. Real route 18-1
            // (r037_084): a hand hit by a ki blast while it rose stays in the hit's hold state (looping, stunned);
            // its state 0x117 lasts the Wait alone (182 frames, not 287). Ours: the strike is the one-shot
            if w.field.is_some() && f == 1 && w.en_ref(i).a.anim_loop != Some(false) {
                return true;
            }
            w.en(i).a.alpha = (256 - f * 256 / 107).max(51);
            // field: the one-shot controller's end (0x0800EE78) leaves the hand in the walk state - an event that
            // starts during the Wait after it idles it (message 0xF: SetAnim(0), one draw; HFIL 18-12, sc_hfil)
            if w.field.is_some() {
                if f >= 106 {
                    en::set_walk_state(w, i);
                }
                return f >= 106;
            }
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
            spawn_rocks(w);
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
            // field: the three dust clouds draw their rate (rand(0x1000) each, 0x08035D04 ..) on tick 248, where the
            // node is over, and on the controller's next two ticks (`Enemy.smash_puffs`; 15-32)
            if w.field.is_some() && f == 248 {
                w.en(i).a.z = 0;
                en::smash_dust(w, i);
                w.en(i).smash_puffs = 2;
                return true;
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
        // field: controller 0x0300326C plays the jump's cels 6 and 7 (4 ticks each, its own tick from 0), then
        // SetAnim(0) (15-32: the idle draw comes 8 frames after the landing hit)
        K::FinishLandingAnim if w.field.is_some() => {
            if (0..=7).contains(&f) {
                hold_pose(w, i, 20, 6 + (f >> 2));
                w.en(i).ea.pos.tick = f;
            } else if f == 8 {
                // (the node is over with it: StandStill's idle enter follows in the same frame)
                set_anim(w, i, &[an(0)], true, Some(true));
            }
            f >= 8
        }
        K::FinishLandingAnim => f >= 11,
        // Babidi's controller tick 0x0802ADC8: cel = (tick >> 3) & 3; at tick 0x60: SetAnim(0) (idle rand(255)), and
        // the node (0x0802AE06) is finished
        K::PlayAnimHold if w.en_ref(i).ty == 0x1d => {
            if f >= 0x60 {
                set_anim(w, i, &[an(0)], true, Some(true));
                return true;
            }
            let e = w.en(i);
            e.ea.pos.tick = f;
            e.ea.pos.frame = (f >> 3) & 3;
            false
        }
        K::StunnedAnim | K::StunnedAnim21 | K::PlayAnimHold => false,
        K::RecoverAnim => {
            let d = Arc::clone(&w.data);
            f >= 8 && w.en_ref(i).anim_done(&d)
        }
        // update 0x0804FF0C: done once the actor left the fragment state
        K::PlayAnim0 => w.en_ref(i).hook != en::Hook::Fragment,
        // GhostAnimA / B (updates 0x0802DFD8 / 0x0802E040): once the one-shot state is over, the paired graphics set
        // through 0x0800EDF8 - A: the shown set -> the unseen one (0x60 -> 0x5E, 0xDC -> 0xDB, 0xE5 -> 0xE4), B back
        K::GhostAnimA | K::GhostAnimB => {
            let d = Arc::clone(&w.data);
            if !(f >= 16 && w.en_ref(i).anim_done(&d)) {
                return false;
            }
            if matches!(kind(c, n), K::GhostAnimA) {
                ghost_gfx(w, i, &[(0x60, 0x5e), (0xdc, 0xdb), (0xe5, 0xe4)]);
            } else {
                ghost_gfx(w, i, &[(0x5e, 0x60), (0xdb, 0xdc), (0xe4, 0xe5)]);
            }
            true
        }
        // 0x0803A556 (field): the first update SetAnim(0x13)s and enters controller 0x030032CC (0x0803A538: tick + 1,
        // cel = tick >> 3, at tick 32 the walk state 0x0800EE78 without an animation change - the last cel stays);
        // the node finishes when the enemy is no longer in that state: the controller ticks before the AI, so in
        // the update after its 32nd tick (real route 8-38: the pose after the combo, then Wait 60)
        K::PlayAnim13Yakon if w.field.is_some() => {
            if f == 1 {
                set_anim(w, i, &[an(18)], true, Some(false));
            }
            f >= 33
        }
        K::PlayAnim12 | K::PlayAnim12b | K::PlayAnim12Swarm | K::PlayAnim12Yakon | K::PlayAnim12Yakon2 | K::PlayAnim13 | K::PlayAnim13Yakon | K::PlayAnimOnce => {
            let d = Arc::clone(&w.data);
            let done = f >= 16 && w.en_ref(i).anim_done(&d);
            if done && matches!(kind(c, n), K::PlayAnimOnce) && c.tree.nodes[n as usize].addr != SKELETON_COLLAPSE {
                // the rise's exit 0x0804F6CC: SetAnim(0) (the idle enter's rand(255)), hittable again
                set_anim(w, i, &[an(0)], true, Some(true));
                w.en(i).a.hittable = true;
            }
            done
        }
        K::PlayAnimTimed if w.field.is_some() && w.en_ref(i).ty != 0x1d => {
            if f == 36 {
                let hold = w.data.anim.hold;
                en::set_anim_rule(w, i, &[an(17)], true, Some(false), Some(hold));
            }
            if f == 71 {
                en::set_anim(w, i, &[an(0)], true, Some(true));
                w.en(i).a.hittable = true;
            }
            // (the sequence's PopState runs in the next frame)
            f >= 71
        }
        // controller 0x080296C0 (ticks before the AI): tick + 1; at 24 SetAnim(0) - the node finishes as the enemy
        // left its state; from tick 8 cel 1 and a 2 px step in the direction (table 0x0875021C) through the
        // footprint test (vfunc +0x54), a blocked step ends it the same way
        K::BeamDodge => {
            // the node finishes once the enemy left the controller's state: another node's SetAnim replaced it (real
            // run 14-2 r069_059 f331: the pushing selector runs on to StandStill, whose SetAnim(0) ends the dodge
            // before it moved - popped two frames later)
            if w.en_ref(i).anim_seq as i32 != r.d[6] {
                return true;
            }
            if f >= 24 {
                set_anim(w, i, &[an(0)], true, Some(true));
                return true;
            }
            {
                let e = w.en(i);
                e.ea.pos.tick = f;
                e.ea.pos.frame = (f >> 3).min(1);
            }
            if f >= 8 {
                const STEP: [(i32, i32); 4] = [(0, 512), (0, -512), (-512, 0), (512, 0)];
                let (dx, dy) = STEP[(r.d[0] & 3) as usize];
                if !en::try_move(w, i, dx, dy, 0, 0) {
                    set_anim(w, i, &[an(0)], true, Some(true));
                    return true;
                }
            }
            false
        }
        // Babidi's controller tick 0x0802AEC0: the tick wraps at 0x20, cel = tick >> 3; the node never finishes
        K::PlayAnimTimed if w.en_ref(i).ty == 0x1d => {
            let e = w.en(i);
            e.ea.pos.tick = f & 0x1f;
            e.ea.pos.frame = (f & 0x1f) >> 3;
            false
        }
        K::PlayAnimTimed => f >= 32,
        K::DarkSphere => w.g.sphere_state == 1 || w.g.sphere_state == 3,
        // controller 0x0804E030 (update 0x08033D9A): tick 8 throws a rock at the player's footprint centre
        // + (rand(64) - 32, rand(64) - 32) from the facing offset 0x0875939C (rock object 0x08033F28)
        K::TauntAnim => {
            if f == 8 {
                if let Some(pi) = en::player(w, i) {
                    let dx = w.rng.rand(64) - 32;
                    let dy = w.rng.rand(64) - 32;
                    const OFF: [(i32, i32); 4] = [(4, 4), (13, -24), (-8, -8), (24, -8)];
                    let a = &w.en_ref(i).a;
                    let (ox, oy) = OFF[a.facing as usize & 3];
                    let (sx, sy) = (a.px() + ox, a.py() + oy);
                    let fb = w.pl_ref(pi).a.foot_box();
                    let (tx, ty) = (((fb.x0 + fb.x1) >> 1) + dx, ((fb.y0 + fb.y1) >> 1) + dy);
                    crate::fx::rock_throw(w, me, sx, sy, tx, ty);
                }
            }
            f > 1 && w.en_ref(i).is_anim(0)
        }
        // update 0x080314DC: running while in the one-shot state, then the paired graphics set, SetAnim(0), finished
        K::MechTransform => {
            let d = Arc::clone(&w.data);
            if !w.en_ref(i).anim_done(&d) {
                return false;
            }
            en::swap_gfx(w, i);
            set_anim(w, i, &[an(0)], true, Some(true));
            true
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
                let (x, y) = (px + off.0, py + off.1);
                // 0x0802F458: the Enzyme's footprint (sprite def 0x57) must be free on the enemy grid, the Blister
                // detached (vfunc +0x48); else no spawn and a retry next frame (VERIFIED segment sb 21-7)
                let foot = w.data.sprite(0x57 - crate::content::GFX_OFFSET).map(|s| s.foot);
                let free = match (foot, w.col.is_some()) {
                    (Some((fw, fh)), true) => {
                        let marked = w.en_ref(i).marked;
                        let c = w.col.as_mut().unwrap();
                        let saved = marked.map(|m| (m, c.save_enemy(m)));
                        if let Some((m, _)) = saved {
                            c.mark_enemy(m, 1);
                        }
                        let ok = c.test(crate::world::Layer::Enemy, x, y, x + fw, y + fh);
                        if let Some((m, sv)) = saved {
                            c.load_enemy(m, &sv);
                        }
                        ok
                    }
                    _ => true,
                };
                if free && crate::battle::spawn_enemy(w, 0x18, x, y, me, None).is_some() {
                    let e = w.en(i);
                    e.spawned += 1;
                    e.enz_timer = 120;
                }
            }
            false
        }
        // the swap happens when the node is entered (the state's GotoState frame; VERIFIED 8-32 segment), the update
        // only finishes
        K::RaiseShield | K::LowerShield | K::ShieldBreak => true,
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
        K::WormSpawnBody => true,
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
        // field (sc_janemba): the stun lasts the hurt animation (0x08039BFD: done when the state's animation ended);
        // battle keeps the reference's 48 frames
        K::ShockStun if w.field.is_some() => f > 1 && w.en_ref(i).ea.pos.raw != 10,
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
    let n0 = w.en_ref(i).ai.orphans.len();
    let r = hit_node(w, i, &aux, 0, msg);
    if let Body::Enemy(e) = &mut w.ents[i].body {
        e.ai.orphans.truncate(n0);
    }
    r
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
    let frame = w.g.clock(w.frame);
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
        // on-hit 0x08032F84 (the tanks' armour: Juggernaut ...): as ApplyDamage, with the damage / 4 (0x08046E58, the
        // signed divide) when the hit's direction is the tank's facing
        K::TankDamage => {
            if msg.mult == 0 {
                return 0;
            }
            let mut m = msg.clone();
            if w.en_ref(i).a.facing == msg.dir {
                m.dmg /= 4;
            }
            if en::take_hit(w, i, &m) {
                2
            } else {
                1
            }
        }
        // on-hit 0x0803999D (field, sc_janemba): the body parts play the hurt animation from the hit's frame; battle
        // keeps the reference (no part flash)
        K::JanembaPartFlash => {
            if w.field.is_some() {
                en::janemba_parts_hurt(w, i);
            }
            1
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
            // 0x080299FE: ((frac * whole max HP) >> 16) << 10
            if e.hp1024 < ((p.g(P::Frac) * (e.max_hp1024 >> 10)) >> 16) << 10 {
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
        // FreezeHelperHit (hit handler 0x0803044C, field): unless a Buu fragment already answers message 5, and when
        // a fragment's footprint 64 px below is on walkable tiles (0x08009904), the enemy ctor 0x0800F68C builds a
        // fragment (type 14 "freezes the player", def 0x087113FC, sprite 45) at this enemy's pixel position (real
        // run 20-5 r061_004 f6,130: Super Buu, Gotenks absorbed, hit by a punch)
        K::FreezeHelperHit if w.field.is_some() => {
            if w.live_enemies().into_iter().any(|o| w.en_ref(o).ty == 14) {
                return 1;
            }
            let (x, y) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
            let (fw, fh) = w.data.sprite(45 - crate::scene::SPRITE_BASE).map(|s| s.foot).unwrap_or((16, 8));
            let (px, py) = (x >> 8, (y + 0x4000) >> 8);
            if w.col.as_ref().is_some_and(|c| !c.test(crate::world::Layer::Walk, px, py, px + fw, py + fh)) {
                return 1;
            }
            // (a run-time placed enemy like a tank's crew: its death runs the type's drop script, 0x08710F99)
            use crate::fieldcontent::{Cond, ObjDef, ObjKind, ObjRef};
            let o = ObjDef { at: ObjRef::List(0xff, 14), cond: Cond::Always, kind: ObjKind::Enemy { ty: 14, x: x >> 8, y: y >> 8, sprite: 45, on_defeat: None, behaviour: vec![] }, rom: 0x0875_819c };
            crate::field::spawn_object(w, &o);
            1
        }
        K::AbsorbPose | K::AbsorbPose2 | K::FreezeHelperHit | K::ShieldHit => 1,
        _ => 0,
    }
}


// ---- TankDrive helpers (node 0x08049E20; r.d[0] = the turn timer +4) ---------------------------------------------------

fn tank_is(w: &World, i: usize, walk: bool) -> bool {
    w.en_ref(i).a.anim_name(&w.data) == Some(if walk { "walk" } else { "idle" })
}
/// turn 0x0803282A: facing == dir -> ok; timer running -> no; else SetAnim(0) unless idle, timer = 60, and the
/// opposite direction is refused (a tank never turns around directly)
fn tank_turn(w: &mut World, i: usize, d: u8, r: &mut NodeRt) -> bool {
    let f = w.en_ref(i).a.facing;
    if f == d {
        return true;
    }
    if r.d[0] != 0 {
        return false;
    }
    if !tank_is(w, i, false) {
        set_anim(w, i, &[nm("idle")], true, Some(true));
    }
    r.d[0] = 60;
    if f ^ 1 == d {
        return false;
    }
    w.en(i).a.facing = d;
    true
}
/// probe 0x08032948: n = (|player pos - own pos| >> 8) / 8 on the axis (top-left positions; direction 1 measures x, a
/// game quirk), n steps of 8 px along dir must all fit
fn tank_probe(w: &mut World, i: usize, d: u8) -> bool {
    let Some(pi) = en::player(w, i) else { return false };
    let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
    let (ex, ey) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    let diff = if d == 0 { py - ey } else { px - ex };
    let steps = (diff.abs() >> 8) / 8;
    let (sx, sy) = DIR_VEC[d as usize];
    en::unmark_self(w, i);
    let (mut x, mut y) = (ex, ey);
    let mut ok = true;
    for _ in 0..steps {
        x += sx * 0x800;
        y += sy * 0x800;
        if !en::fits(w, i, x >> 8, y >> 8) {
            ok = false;
            break;
        }
    }
    en::mark_self(w, i);
    ok
}
/// drive 0x0803287E: turn ok -> 0.25 px in the facing, the footprint tested: moved -> SetAnim(1) unless walking,
/// blocked -> SetAnim(0) unless idle
fn tank_drive(w: &mut World, i: usize, d: u8, r: &mut NodeRt) {
    if !tank_turn(w, i, d, r) {
        return;
    }
    let (sx, sy) = DIR_VEC[w.en_ref(i).a.facing as usize];
    en::unmark_self(w, i);
    let (x, y) = (w.en_ref(i).a.x, w.en_ref(i).a.y);
    if en::fits(w, i, (x + sx * 0x40) >> 8, (y + sy * 0x40) >> 8) {
        {
            let a = &mut w.en(i).a;
            a.x += sx * 0x40;
            a.y += sy * 0x40;
        }
        if !tank_is(w, i, true) {
            set_anim(w, i, &[nm("walk")], true, Some(true));
        }
    } else if !tank_is(w, i, false) {
        set_anim(w, i, &[nm("idle")], true, Some(true));
    }
    en::mark_self(w, i);
}

/// Ghost / Phantom vanish and appear (GhostAnimA / GhostAnimB): the graphics set changes like the shield soldiers',
/// with the object tile bookkeeping of SetSprite
fn ghost_gfx(w: &mut World, i: usize, map: &[(i32, i32)]) {
    let old = w.en_ref(i).a.sprite;
    shield_gfx(w, i, map);
    if w.en_ref(i).a.sprite != old {
        crate::vram::sprite_changed(w, i, old);
        // 0x0800EDF8: the new set's first animation from its start (obj+0x34 = the def's table, +0x38 / +0x30 = 0):
        // the idle animation with no rand(255) draw (the one-shot state had already handed back to the normal one)
        let d = Arc::clone(&w.data);
        w.en(i).set_anim_with(&d, &[nm("idle")], true, Some(true), None, None, &mut Vec::new());
    }
}

/// shield soldiers' graphics swaps (RaiseShield 0x0802CD7D / LowerShield 0x0802CDE1 / ShieldBreak 0x0802D49F): the
/// enemy's graphics set changes (gfx ids: shield down / up / broken); the animation carries on
fn shield_gfx(w: &mut World, i: usize, map: &[(i32, i32)]) {
    let d = w.data.clone();
    let g = if w.en_ref(i).gfx_now >= 0 { Some(w.en_ref(i).gfx_now) } else { d.enemy(w.en_ref(i).ty).and_then(|t| t.gfx) };
    let Some(g) = g else { return };
    let Some(&(_, to)) = map.iter().find(|(from, _)| *from == g) else { return };
    let idx = to - crate::content::GFX_OFFSET;
    if d.sprite(idx).is_none() {
        return;
    }
    let e = w.en(i);
    e.gfx_now = to;
    e.a.set_sprite(&d, Some(idx));
}
