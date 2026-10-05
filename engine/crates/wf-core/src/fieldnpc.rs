//! Map NPCs of the field (`0x080111A9` plain / `0x08017DA3` talk): the scene layer's `Body::Npc` carrying a
//! `Behaviour` (the def's step list run by `0x0800A95A`, the game's idle / walk animation, the grid marks of talk
//! NPCs), the A-button interact message (talk, A pickups) and the event start. Spec and verification:
//! re/buu/notes.md "Field: NPC actors and behaviours", tools/field/npc_model.py, data/field/npc_*.json.

use crate::field::{set_busy, Cause, FieldEvent};
use crate::fieldcontent::{Activation, Callback, ObjDef, ObjKind, ObjRef, Step, StepOp};
use crate::geom::{Dir, Rect};
use crate::scene::Npc;
use crate::world::{Body, EntityId, Event, World};
use serde::{Deserialize, Serialize};

/// a map NPC's behaviour runner (obj+0x1B0) and animation (the player's idle / walk state singletons)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Behaviour {
    pub steps: Vec<Step>,
    /// a talk NPC (marks its footprint on the grids)
    pub talk: bool,
    /// current step index (-1 before the first)
    pub idx: i32,
    pub cur: Option<StepRt>,
    /// SetAnim id: 0 idle, 1 walk
    pub anim: u8,
    pub tick: i32,
    pub frame: i32,
    pub raw: i32,
    /// a script left the NPC in a pose (set_pose's state has no tick): frozen until its next SetAnim (3-3: the
    /// officer s143 posed and glided away keeps cel 0)
    #[serde(default)]
    pub pose: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepRt {
    Stand,
    Wait {
        count: i32,
    },
    FacePlayer {
        count: i32,
    },
    WalkTo {
        tx: i32,
        ty: i32,
        dir: Dir,
        n: i32,
        vx: i32,
        vy: i32,
    },
    Wander(Box<Wander>),
    /// `cutscene_move` (0x08039D98, world-map encounter objects): to (tx, ty, tz) in n frames (z 22.10)
    Move3 {
        tx: i32,
        ty: i32,
        tz: i32,
        n: i32,
        vx: i32,
        vy: i32,
        vz: i32,
        /// the heading the step's ctor stores at obj+0x1AC: (int)(atan2(dy, dx) / 2pi * 1024) in floats
        #[serde(default)]
        h: i32,
    },
    /// world-map marker (step 0x0803A053, update 0x08039F4E): kept on the circle of radius `r` around (cx, cy), on
    /// the side of the leader (0-50's airship marker, s017); never finishes
    #[serde(alias = "ring")]
    Ring {
        cx: i32,
        cy: i32,
        r: i32,
    },
    /// steps not modelled for NPCs (enemy walk_to, cutscene moves): never finish
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wander {
    pub spd: i32,
    pub dist: i32,
    pub frames: i32,
    pub left: i32,
    pub chance: i64,
    pub talk: bool,
    pub rect: Option<Rect>,
    pub old: [i32; 4],
    pub new: [i32; 4],
    pub vx: i32,
    pub vy: i32,
    pub tx: i32,
    pub ty: i32,
    pub dir: Dir,
}

/// create a map NPC: SetAnim(0) (rand(255)); talk NPCs mark their footprint blocked on the walk grid + enemy layer
pub fn spawn(w: &mut World, o: &ObjDef) -> Option<EntityId> {
    let ObjKind::Npc {
        x,
        y,
        sprite,
        talk,
        behaviour,
    } = &o.kind
    else {
        return None;
    };
    // the def stores the script sprite id (content sprite index + 9), like `spawn_actor`; 1..=8 are characters
    // 0..7 in their current form (Babidi's ship 8-27 / 8-67: the def's 1 shows Goku)
    let sid = if (1..=8).contains(sprite) {
        char_sprite(w, *sprite - 1).unwrap_or(*sprite)
    } else {
        *sprite
    };
    let mut n = Npc::new(&w.data, sid, *x, *y, 0);
    n.talk = talk
        .as_ref()
        .and_then(|t| t.trim_start_matches('s').parse().ok());
    n.tag = match o.at {
        ObjRef::List(_, k) => Some(k as i32),
        _ => None,
    };
    let tick = w.rng.rand(255);
    n.behaviour = Some(Behaviour {
        steps: behaviour.clone(),
        talk: talk.is_some(),
        idx: -1,
        cur: None,
        anim: 0,
        tick,
        frame: 0,
        raw: 1,
        pose: false,
    });
    mirror(&mut n);
    let f = foot(&n);
    let id = w.add(Body::Npc(Box::new(n)));
    let ni = w.ents.len() - 1;
    crate::vram::actor_created(w, ni);
    if talk.is_some() {
        mark(w, f, 0);
    }
    Some(id)
}

/// script sprite id of character `c`'s current form: the live player's form when one plays it, else the story
/// record's (wf-script-core `char_sprite`)
pub fn char_sprite(w: &World, c: i32) -> Option<i32> {
    let mut form = None;
    for &id in &w.players {
        if let Some(i) = w.find(id) {
            let f = w.pl_ref(i).form;
            if w.data.form(f).map(|x| x.char_) == Some(c) {
                form = Some(f);
            }
        }
    }
    let form = form.or_else(|| {
        let cs = w.story.chars.get(c.max(0) as usize)?;
        if cs.form != 0 || w.data.form(0).is_some_and(|f| f.char_ == c) {
            return Some(cs.form);
        }
        w.data.chars.iter().find(|x| x.index == c).map(|x| x.form)
    })?;
    w.data.form(form).map(|f| f.gfx)
}

fn foot(n: &Npc) -> [i32; 4] {
    let (x, y) = (n.a.px(), n.a.py());
    [x, y, x + n.a.foot_w, y + n.a.foot_h]
}

fn ov(a: [i32; 4], b: [i32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// the behaviour's animation shown through the scene animation (frozen pose = what the behaviour computed)
fn mirror(n: &mut Npc) {
    if let Some(b) = &n.behaviour {
        n.anim.raw = b.raw;
        n.anim.frame = b.frame;
        n.anim.tick = b.tick;
        n.anim.mode = crate::scene::AnimMode::Pose;
    }
}

fn npc(w: &mut World, i: usize) -> &mut Npc {
    match &mut w.ents[i].body {
        Body::Npc(n) => n,
        _ => panic!("entity {i} is not an npc"),
    }
}
fn npc_ref(w: &World, i: usize) -> &Npc {
    match &w.ents[i].body {
        Body::Npc(n) => n,
        _ => panic!("entity {i} is not an npc"),
    }
}
fn beh(w: &mut World, i: usize) -> &mut Behaviour {
    npc(w, i).behaviour.as_mut().expect("behaviour")
}

/// SetAnim(k) (`force` = even when already in it): the idle enter draws tick = rand(255), walk starts at tick 0
fn set_anim(w: &mut World, i: usize, k: u8, force: bool) {
    if !force && beh(w, i).anim == k {
        return;
    }
    let tick = if k == 0 { w.rng.rand(255) } else { 0 };
    let b = beh(w, i);
    b.pose = false;
    b.anim = k;
    b.raw = k as i32 + 1;
    b.tick = tick;
    b.frame = 0;
}

/// the state tick: idle blinks on 2-frame animations (frame 1 while (tick & 255) > 240), else frame = tick >> 3
fn anim_tick(w: &mut World, i: usize) {
    let n = npc_ref(w, i);
    let raw = n.behaviour.as_ref().map(|b| b.raw).unwrap_or(1);
    let nf =
        n.a.sprite
            .and_then(|s| w.data.sprite(s))
            .and_then(|sp| sp.anim_raw(raw))
            .map(|a| a.nframes())
            .unwrap_or(1);
    let b = beh(w, i);
    if b.pose {
        return;
    }
    b.tick = (b.tick + 1) & 0xFFFF;
    if b.anim == 0 && nf == 2 {
        b.frame = if (b.tick & 0xFF) > 0xF0 { 1 } else { 0 };
        return;
    }
    let f = (b.tick >> 3) & 0xFF;
    if f >= nf {
        b.frame = 0;
        b.tick = 0;
    } else {
        b.frame = f;
    }
}

fn mark(w: &mut World, r: [i32; 4], v: u8) {
    if let Some(c) = w.col.as_mut() {
        c.mark_both(r, v);
    }
}
fn test_walk(w: &World, r: [i32; 4]) -> bool {
    w.col
        .as_ref()
        .is_none_or(|c| c.test(crate::world::Layer::Walk, r[0], r[1], r[2], r[3]))
}
fn players_feet(w: &World) -> Vec<[i32; 4]> {
    w.players
        .iter()
        .filter_map(|&id| w.find(id))
        .map(|k| {
            let f = w.pl_ref(k).a.foot_box();
            [f.x0, f.y0, f.x1, f.y1]
        })
        .collect()
}

/// wf-import (any-frame import): put map NPC `i` on behaviour step `idx`, created as its runner would (the importer
/// then sets the step's progress and the animation from RAM)
pub fn import_step(w: &mut World, i: usize, idx: usize) {
    let Some(st) = npc_ref(w, i)
        .behaviour
        .as_ref()
        .and_then(|b| b.steps.get(idx).cloned())
    else {
        return;
    };
    let s = create(w, i, &st);
    let b = beh(w, i);
    b.idx = idx as i32;
    b.cur = Some(s);
}

fn create(w: &mut World, i: usize, st: &Step) -> StepRt {
    let wd = |k: usize| st.words.get(k).copied().unwrap_or(0);
    match st.func {
        0x0800_A26F => StepRt::Stand,
        0x0800_A223 => {
            set_anim(w, i, 0, false);
            npc(w, i).a.facing = ((wd(0) >> 16) & 3) as u8;
            StepRt::Wait {
                count: (wd(0) & 0xFFFF) as i16 as i32,
            }
        }
        0x0800_A8E9 | 0x0800_A90F => {
            set_anim(w, i, if st.func == 0x0800_A8E9 { 0 } else { 1 }, true);
            StepRt::FacePlayer {
                count: wd(0) as i32,
            }
        }
        0x0800_A1AF => {
            let StepOp::WalkTo { x: tx, y: ty, .. } = st.op else {
                return StepRt::Other;
            };
            let n = npc_ref(w, i);
            let (dx, dy) = ((tx << 8) - n.a.x, (ty << 8) - n.a.y);
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
            npc(w, i).a.facing = d;
            let (vx, vy) = if cnt != 0 {
                (
                    (((dx as i64) << 8) / ((cnt as i64) << 8)) as i32,
                    (((dy as i64) << 8) / ((cnt as i64) << 8)) as i32,
                )
            } else {
                cnt = 1;
                (0, 0)
            };
            StepRt::WalkTo {
                tx,
                ty,
                dir: d,
                n: cnt,
                vx,
                vy,
            }
        }
        0x0800_A75B | 0x0800_A7A1 | 0x0800_A7DF | 0x0800_A811 => {
            set_anim(w, i, 0, true);
            let spd = wd(0) as i32;
            let r = ((spd << 5) + 0x7FF) & !0x7FF;
            let rect = matches!(st.func, 0x0800_A7A1 | 0x0800_A811).then(|| {
                Rect::new(
                    (wd(1) & 0xFFFF) as i32,
                    (wd(1) >> 16) as i32,
                    (wd(2) & 0xFFFF) as i32,
                    (wd(2) >> 16) as i32,
                )
            });
            let mut chance = (spd as i64) << 4;
            if st.func == 0x0800_A811 {
                chance <<= 3;
            }
            StepRt::Wander(Box::new(Wander {
                spd,
                dist: r >> 8,
                frames: if spd > 0 { (r + spd - 1) / spd } else { 0 },
                left: 0,
                chance,
                talk: matches!(st.func, 0x0800_A75B | 0x0800_A7A1),
                rect,
                old: [0; 4],
                new: [0; 4],
                vx: 0,
                vy: 0,
                tx: 0,
                ty: 0,
                dir: 0,
            }))
        }
        _ => StepRt::Other,
    }
}

/// one step update; true = done
fn update_step(w: &mut World, i: usize, s: &mut StepRt) -> bool {
    match s {
        StepRt::Stand | StepRt::Other | StepRt::Move3 { .. } | StepRt::Ring { .. } => false,
        StepRt::Wait { count } => {
            *count -= 1;
            *count < 0
        }
        StepRt::FacePlayer { count } => {
            if let Some(pi) = w.player_idx(0) {
                let (px, py) = (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y);
                let n = npc(w, i);
                let (dx, dy) = (px - n.a.x, py - n.a.y);
                n.a.facing = if dx.abs() >= dy.abs() {
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
        StepRt::WalkTo {
            tx,
            ty,
            dir,
            n: cnt,
            vx,
            vy,
        } => {
            let old = foot(npc_ref(w, i));
            {
                let n = npc(w, i);
                n.a.x += *vx;
                n.a.y += *vy;
            }
            let new = foot(npc_ref(w, i));
            for p in players_feet(w) {
                let pa = [p[0] & !7, p[1] & !7, (p[2] + 7) & !7, (p[3] + 7) & !7];
                if ov(pa, old) || ov(pa, new) {
                    let n = npc(w, i);
                    n.a.x -= *vx;
                    n.a.y -= *vy;
                    return false;
                }
            }
            mark(w, old, 1);
            if !test_walk(w, new) {
                mark(w, old, 0);
                set_anim(w, i, 0, false);
                let n = npc(w, i);
                n.a.x -= *vx;
                n.a.y -= *vy;
                return false;
            }
            set_anim(w, i, 1, false);
            npc(w, i).a.facing = *dir;
            mark(w, new, 0);
            *cnt = (*cnt - 1) & 0xFFFF;
            if *cnt == 0 {
                let f = foot(npc_ref(w, i));
                mark(w, f, 1);
                let n = npc(w, i);
                n.a.x = *tx << 8;
                n.a.y = *ty << 8;
                let f = foot(npc_ref(w, i));
                mark(w, f, 0);
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

fn wander(w: &mut World, i: usize, s: &mut Wander) {
    if s.left == 0 && w.rng.chance(s.chance) {
        s.old = foot(npc_ref(w, i));
        let d = w.rng.rand(5);
        if d < 4 {
            npc(w, i).a.facing = d as u8;
        }
        let n = npc_ref(w, i);
        let (sx, sy) = (n.a.px(), n.a.py());
        let (sp, ds) = (s.spd, s.dist);
        let v = match n.a.facing {
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
        let ok = 'c: {
            if let Some(r) = s.rect {
                if r.x0 > new[0] || r.y0 > new[1] || r.x1 < new[2] || r.y1 < new[3] {
                    break 'c false;
                }
            }
            if !s.talk {
                break 'c test_walk(w, new);
            }
            if players_feet(w).iter().any(|p| ov(*p, new)) {
                break 'c false;
            }
            mark(w, o, 1);
            let ok = test_walk(w, new);
            mark(w, if ok { new } else { o }, 0);
            ok
        };
        if ok {
            s.left = s.frames;
            s.dir = npc_ref(w, i).a.facing;
            set_anim(w, i, 1, false);
        }
    }
    if s.left != 0 {
        {
            let n = npc(w, i);
            n.a.x += s.vx;
            n.a.y += s.vy;
            n.a.facing = s.dir;
        }
        s.left -= 1;
        if s.left == 0 {
            {
                let n = npc(w, i);
                n.a.x = s.tx << 8;
                n.a.y = s.ty << 8;
            }
            if s.talk {
                mark(w, s.new, 1);
                let f = foot(npc_ref(w, i));
                mark(w, f, 0);
            }
            set_anim(w, i, 0, false);
        } else {
            set_anim(w, i, 1, false);
        }
    }
}

/// the runner `0x0800A95A`: update the current step; a finished step chains into the next in the same frame
fn runner(w: &mut World, i: usize) {
    let n = beh(w, i).steps.len();
    if n == 0 {
        return;
    }
    for _ in 0..64 {
        if let Some(mut cur) = beh(w, i).cur.take() {
            let done = update_step(w, i, &mut cur);
            beh(w, i).cur = Some(cur);
            if !done {
                return;
            }
        }
        let b = beh(w, i);
        b.idx += 1;
        if b.idx as usize >= n {
            b.idx = 0;
        }
        let st = b.steps[b.idx as usize].clone();
        let s = create(w, i, &st);
        beh(w, i).cur = Some(s);
    }
}

/// a map NPC's update (vt+8 `0x08011192`): the runner, then the animation tick. While an event holds the field only
/// the animation ticks (not in the frame the event started); the frame it ends ticks once more before the update.
pub fn update(w: &mut World, i: usize) {
    // a load waiting for its on-enter script holds the map NPCs like an event (story playthrough: Grand Kai's 1-15)
    let (busy, since, ended) = w
        .field
        .as_ref()
        .map(|f| (f.busy || f.arrival.is_some(), f.busy_since, f.event_ended))
        .unwrap_or((false, 0, false));
    // the item-get presentation's loop holds the behaviours too, animations tick (4-30: the scientist's wander draws
    // nothing while Gohan holds the capsule up)
    let item_get = w.field.as_ref().is_some_and(|f| f.item_get.is_some());
    if busy {
        if since != w.frame {
            anim_tick(w, i);
        }
    } else if item_get {
        anim_tick(w, i);
    } else {
        if ended {
            anim_tick(w, i);
        }
        runner(w, i);
        anim_tick(w, i);
    }
    mirror(npc(w, i));
}

/// a map NPC's footprint on the walk grid (marked by the NPC itself): a script command moving it lifts the mark at
/// its start and sets it where the NPC stops (story playthrough: the Yemma office ogre's walk_to frees the door)
pub fn script_move_mark(w: &mut World, i: usize, set: bool) {
    // (a talk NPC only - the one whose spawn marked the grid: 3-12 s159 walks nine plain crowd NPCs to (240, 200)
    // and removes them, the game's walk grid never changes - emulator write watch on their tiles)
    if !matches!(&w.ents[i].body, Body::Npc(n) if n.behaviour.as_ref().is_some_and(|b| b.talk)) {
        return;
    }
    let f = foot(npc_ref(w, i));
    mark(w, f, if set { 0 } else { 1 });
}

/// the rest of this frame's update for an NPC whose event ended before the world update in the game but after it
/// here (an event whose last action was a command wait: the nested wait loop returns at the top of the frame, the
/// event exits, then the objects update): the runner and the second animation tick (the busy update already
/// ticked once; story playthrough, the Yemma office ogre s055)
pub fn release_update(w: &mut World, i: usize) {
    runner(w, i);
    anim_tick(w, i);
    mirror(npc(w, i));
}

/// an animation tick alone (the load's last non-logic frames)
pub fn tick_only(w: &mut World, i: usize) {
    anim_tick(w, i);
    mirror(npc(w, i));
}

/// interact box per player facing (`0x083C340C`), relative to the footprint top-left
pub const TALK_BOX: [[i32; 4]; 4] = [
    [0, 0, 16, 24],
    [0, -16, 16, 8],
    [-16, -8, 16, 8],
    [0, -8, 32, 8],
];

/// the A press in the field once the melee hit found nobody (`0x08012A24`): message 3 "interact" to the objects last
/// to first (a talk NPC or an A pickup under the box accepts), then an A trigger under the point. true = the punch
/// is suppressed (the trigger itself fires in the trigger check)
pub fn interact(w: &mut World, pi: usize) -> bool {
    if w.field.is_none() {
        return false;
    }
    let (px, py, facing) = {
        let a = &w.pl_ref(pi).a;
        (a.px(), a.py(), a.facing)
    };
    let me = w.ents[pi].id;
    let slot = w.players.iter().position(|&id| id == me).unwrap_or(0) as u8;
    let b = TALK_BOX[facing as usize & 3];
    let bx = [px + b[0], py + b[1], px + b[2], py + b[3]];
    for k in (0..w.ents.len()).rev() {
        if w.ents[k].removed {
            continue;
        }
        let talk = match &w.ents[k].body {
            Body::Npc(n)
                if n.behaviour.as_ref().is_some_and(|b| b.talk)
                    && n.talk.is_some()
                    && ov(foot(n), bx) =>
            {
                n.talk
            }
            Body::Pickup(p) if p.by_a() && ov(p.rect(), bx) => {
                crate::fieldobj::collect(w, k, slot);
                return true;
            }
            _ => None,
        };
        if let Some(ev) = talk {
            let id = w.ents[k].id;
            npc(w, k).a.facing = facing ^ 1;
            set_anim(w, k, 0, true);
            mirror(npc(w, k));
            start_event(w);
            let pk = (0..w.ents.len())
                .filter(|&j| !w.ents[j].removed)
                .position(|j| j == pi)
                .unwrap_or(0)
                + crate::field::props_object(w) as usize;
            if let Some(f) = w.field.as_mut() {
                f.talk_pass = Some((pk, f.loads));
            }
            // the talk scene's speaker anchor (`0x08017D44`): player - NPC, 24.8 (text box side / top-bottom)
            let (nx, ny) = (npc(w, k).a.x, npc(w, k).a.y);
            let pa = &w.pl_ref(pi).a;
            w.scene.talk_anchor = [pa.x - nx, pa.y - ny];
            w.events.push(Event::Field(FieldEvent::Run {
                run: Callback::Event(format!("s{ev:03}")),
                cause: Cause::Talk { npc: id },
                slot,
            }));
            return true;
        }
    }
    let f = w.field.as_ref().unwrap();
    if std::env::var_os("XRS").is_some() { for t in &f.triggers { eprintln!("XRS trig {:?} {:?} rect {:?} last {} p ({}, {})", t.obj, t.act, t.rect, t.last, px, py); } }
    let f = w.field.as_ref().unwrap();
    f.triggers.iter().any(|t| {
        matches!(
            t.act,
            Activation::Button(_) | Activation::Unknown(0x0802_266F)
        ) && t.rect.x0 <= px
            && px < t.rect.x1
            && t.rect.y0 <= py
            && py < t.rect.y1
    })
}

/// an event starts (`0x0801246E`): message 0xF to every object, last to first (walking map NPCs go idle); with a
/// script host attached the field is busy until the host clears it
pub fn start_event(w: &mut World) {
    w.scene.talk_anchor = crate::scene::NO_ANCHOR;
    for k in (0..w.ents.len()).rev() {
        match &w.ents[k].body {
            Body::Npc(n) if n.behaviour.as_ref().is_some_and(|b| b.anim == 1) => {
                set_anim(w, k, 0, true);
                mirror(npc(w, k));
            }
            // (the bored pose too: 12-22, the countdown's hook #9 starts on a player standing bored - idle, one draw)
            Body::Player(p)
                if matches!(p.gs, crate::player::Gs::Walk | crate::player::Gs::Bored) =>
            {
                crate::player::set_idle(w, k)
            }
            // (a beam skill too: its state stops, idle with one draw - segment sb 6-6 f22178, Gotenks firing when
            // Super Buu's defeat event starts)
            Body::Player(p) if matches!(p.gs, crate::player::Gs::Skill(15 | 16 | 6 | 1 | 3)) => {
                crate::player::set_idle(w, k)
            }
            // a walking enemy stops: idle state (story playthrough: a fighter walking when the level-up hook starts
            // goes idle with a fresh idle tick; a hurt one keeps its state)
            // (a defeated enemy that stays for its on-defeat event - ko < 0, a boss - is still an actor: Totenhotep
            // walking when the last punch lands goes idle as the level-up event starts, Kyodai Pyramid 14-24)
            Body::Enemy(e) if !e.defeated || e.ko < 0 => crate::enemy::stop_msg(w, k),
            _ => {}
        }
    }
    if w.field.as_ref().is_some_and(|f| f.host) {
        set_busy(w, true);
    }
}
