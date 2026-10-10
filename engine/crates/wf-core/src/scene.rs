//! Cutscene systems: what Buu's Fury scripts drive (spec `re/buu/SCRIPT.md` part B, host `docs/script-host.md`),
//! as plain World data (`World.scene`, `Body::Npc`), stepped by the frame and snapshotted / hashed with the World.
//!
//! * **Scripted actors** (`Body::Npc`, the game's NPC class `0x0801111C` used by `spawn_actor` / `change_sprite`)
//!   and **puppets** (players and enemies a script commands): the actor animation state machine of the game (state
//!   id → raw anim `state + 1`, per-state timing from the state objects `0x0807273C`, ends in idle / hold / loop).
//! * **Command tracks** (`actor+0x18C`, B3.2): one queue per actor plus the camera track; commands start, update
//!   once per frame and report done; a finished head is popped and the next one starts in the same tick; a track
//!   that popped its last command still reports busy for that tick (the game's tick returns 2), so a lone instant
//!   command keeps a blocking native busy for 2 frames. A head the tick itself starts (not chained) is not updated
//!   in that tick, and a refresh command popped at its start (set_pos) leaves the next for the next tick (story
//!   playthrough s101). Tracks only run while a director wait mode runs.
//! * **Director** (`0x03002BAC`, B3.3): batch flag, `wait_for_actor` group, the wait mode (all tracks / the group +
//!   camera) whose completion resumes the script.
//! * **Camera control**: follow another actor, modal scrolls, queued pans / sets (the default stays the player
//!   camera `battle::camera`).
//! * **Screen**: fades (black / white / mosaic / flash), palette flashes, BG layer visibility / priority / blend
//!   targets / alpha, HUD visibility, boss bar, title text, timer HUD, destination label, markers, radar.
//! * **Dialog** (text boxes with pages, choices with the result kept for `get_menu_result`, box positions) and
//!   **menus** (shop buy / sell, save, character switch, mini-games, credits) as deterministic input-driven state.
//! * **Requests** the session / frontend acts on (return to title, credits, sleep, scene loads in battle mode).
//!
//! Frame order (`Session::step_frames`): [`pre_step`] (raw input kept for script queries; players' input masked
//! while an event graph runs) → entities (Npc / puppet animation ticks) → [`tick`] (screen effects, dialog, menus,
//! command tracks) → cameras ([`camera`] before the player camera) → the script host polls and resumes.
//! Every timing here is checked against emulator recordings (`data/script_lockstep`, test
//! `wf-script-core/tests/lockstep.rs`).

use crate::actor::Actor;
use crate::content::GameData;
use crate::geom::{facing_toward, Dir, Rect};
use crate::input::{InputFrame, A, B, DOWN, UP};
use crate::world::{Body, EntityId, World, GBA_H, GBA_W, MAX_PLAYERS};
use serde::{Deserialize, Serialize};

/// script sprite id = content sprite index + 9 (the game's gfx id; `0x08000158`)
pub const SPRITE_BASE: i32 = 9;

// ---------------------------------------------------------------------------------------------------------------
// actor animation state machine (state objects 0x0807273C; script anim id -> state id table 0x08072670)
//
// The same generic state objects drive every actor (player.rs ports them for the player: `Gs`, `anim_tick`);
// scripted actors use this copy: the tick counts up from the state's start, the cel follows from it.

/// script anim id -> actor state id (`0x08072670`); raw anim = state + 1
pub fn anim_state(anim: i32) -> i32 {
    const T: [i32; 25] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
        0x17, 0x0a, 0x0b,
    ];
    T.get(anim.max(0) as usize).copied().unwrap_or(anim)
}

/// how a state ticks (the state objects' vtable +4, see player.rs `anim_tick`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickKind {
    /// idle: 2 cels blink (cel 1 while tick & 0xFF > 0xF0), else 8 ticks per cel
    Idle,
    /// loops, 1 << 3 ticks per cel (walk, dash, ...); `sound`: dash's SFX 25 every 16 ticks
    Loop { sound: bool },
    /// once, 1 << shift ticks per cel, then idle (`hold`: stays on the last cel)
    Once { shift: i32, hold: bool },
    /// the bored state (32 ticks per cel, loops from cel 2)
    Bored,
    /// frozen
    Hold,
}

/// the state kind of a script anim id (player.rs ANIM_STATE)
pub fn tick_kind(id: i32) -> TickKind {
    match id {
        0 => TickKind::Idle,
        2 => TickKind::Loop { sound: true },
        1 | 13 | 14 | 15 => TickKind::Loop { sound: false },
        // (an NPC's 10 / 11 are their own state `0x03001B30` in its table `0x080727C8`: played once, 8 ticks a
        // cel, then it stays on the last one (no idle enter); segment sb 19-3: s457's play_anim(5, 10) ends after
        // 11 cels without the idle's draw)
        10 | 11 => TickKind::Once {
            shift: 3,
            hold: true,
        },
        3 | 9 => TickKind::Once {
            shift: 3,
            hold: false,
        },
        4..=6 => TickKind::Once {
            shift: 2,
            hold: false,
        },
        7 => TickKind::Hold,
        8 => TickKind::Bored,
        16 => TickKind::Once {
            shift: 5,
            hold: true,
        },
        _ => TickKind::Once {
            shift: 4,
            hold: false,
        },
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnimMode {
    /// the state's own tick
    #[default]
    Table,
    /// `set_pose` / held cels of a command: frozen on a cel
    Pose,
    /// `play_anim_frames`: cels 0..last, each `each` frames, restart at `loop_start`, `loops` passes (0 = forever)
    Frames {
        each: i32,
        loops: i32,
        loop_start: i32,
        last: i32,
        pass: i32,
    },
    /// an enemy's SetAnim: the enemy state table's rule for the raw (eanim.rs `setanim_table`; 2-90: play_anim(123,
    /// 10) on Goten plays raw 11 once and stops on its last cel, then the script goes on)
    Enemy,
}

/// an actor's animation as the game keeps it (obj+0x50 anim, +0x11 cel, +0x12 tick)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActAnim {
    /// raw anim index (state + 1; 1 = idle)
    pub raw: i32,
    pub frame: i32,
    pub tick: i32,
    /// script anim id of the state
    pub id: i32,
    pub mode: AnimMode,
    /// a one-shot animation reached its end (`play_anim` done)
    pub ended: bool,
    /// a frame-range controller created this frame by a track's pop-and-start: no tick yet
    pub fresh: bool,
}

/// what an animation tick asks the world to do
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimOut {
    None,
    /// the once-state ended: SetAnim(0) (idle enter: tick = rand(255))
    ToIdle,
    /// SFX (id, volume)
    Sfx(i32, i32),
}

impl ActAnim {
    pub fn idle() -> ActAnim {
        ActAnim {
            raw: 1,
            ..Default::default()
        }
    }
    /// SetAnim(id) without the enter side effects (see [`set_state`])
    pub fn of(id: i32) -> ActAnim {
        ActAnim {
            raw: anim_state(id) + 1,
            id,
            ..Default::default()
        }
    }
    fn cels(&self, d: &GameData, sprite: Option<i32>, dir: Dir) -> i32 {
        sprite
            .and_then(|s| d.sprite(s))
            .and_then(|sp| sp.anim_raw(self.raw))
            .map(|a| a.steps(dir).len() as i32)
            .unwrap_or(0)
    }
    /// one frame of the state's tick
    pub fn tick(&mut self, d: &GameData, sprite: Option<i32>, dir: Dir) -> AnimOut {
        let n = self.cels(d, sprite, dir).max(1);
        match self.mode.clone() {
            AnimMode::Pose => AnimOut::None,
            AnimMode::Enemy => {
                use crate::eanim::{End, RuleKind};
                let Some(r) = d
                    .anim
                    .setanim
                    .get(&self.raw)
                    .map(|&k| &d.anim.rules[k as usize])
                else {
                    return AnimOut::None;
                };
                self.tick = (self.tick + 1) & 0xffff;
                match r.kind {
                    RuleKind::Idle => {
                        if n == 2 {
                            self.frame = ((self.tick & 0xff) > 0xf0) as i32;
                        } else {
                            self.frame = (self.tick >> 3) & 0xff;
                            if self.frame >= n {
                                self.frame = 0;
                                self.tick = 0;
                            }
                        }
                    }
                    RuleKind::Loop => {
                        let mut f = self.tick >> r.shift;
                        if f >= n {
                            self.tick = 0;
                            f = 0;
                        }
                        self.frame = f;
                    }
                    RuleKind::Bored => {
                        let f = self.tick >> 5;
                        if f >= n {
                            self.tick = 0x40;
                            self.frame = 2;
                        } else {
                            self.frame = f;
                        }
                    }
                    RuleKind::Once | RuleKind::OnceStop if !self.ended => {
                        let f = self.tick >> r.shift;
                        if f >= n {
                            self.ended = true;
                            if r.end == Some(End::Idle) {
                                return AnimOut::ToIdle;
                            }
                        } else {
                            self.frame = f;
                        }
                    }
                    _ => {}
                }
                AnimOut::None
            }
            AnimMode::Frames {
                each,
                loops,
                loop_start,
                last,
                pass,
            } => {
                // the frame-range controller (0x0804DE58): cels 0..last, `each` ticks per cel, later passes from
                // `loop_start` (its tick restarts at loop_start * each); a controller created by a track's
                // pop-and-start waits a frame before its first tick (a new world object)
                if self.ended {
                    return AnimOut::None;
                }
                if self.fresh {
                    self.fresh = false;
                    return AnimOut::None;
                }
                let each = each.max(1);
                let last = last.max(1);
                let ls = loop_start.clamp(0, last - 1);
                let mut pass = pass;
                self.tick += 1;
                let mut c = self.tick / each;
                if c >= last {
                    pass += 1;
                    if loops > 0 && pass >= loops {
                        self.ended = true;
                        c = last - 1;
                    } else {
                        self.tick = ls * each;
                        c = ls;
                    }
                }
                self.frame = c;
                self.mode = AnimMode::Frames {
                    each,
                    loops,
                    loop_start,
                    last,
                    pass,
                };
                AnimOut::None
            }
            AnimMode::Table => {
                self.tick = (self.tick + 1) & 0xffff;
                match tick_kind(self.id) {
                    TickKind::Idle => {
                        if n == 2 {
                            self.frame = ((self.tick & 0xff) > 0xf0) as i32;
                        } else {
                            self.frame = (self.tick >> 3) & 0xff;
                            if self.frame >= n {
                                self.frame = 0;
                                self.tick = 0;
                            }
                        }
                        AnimOut::None
                    }
                    TickKind::Loop { sound } => {
                        let mut f = self.tick >> 3;
                        if f >= n {
                            self.tick = 0;
                            f = 0;
                        }
                        self.frame = f;
                        if sound && self.tick & 15 == 0 {
                            AnimOut::Sfx(25, 32)
                        } else {
                            AnimOut::None
                        }
                    }
                    TickKind::Once { shift, hold } => {
                        if self.ended {
                            return AnimOut::None;
                        }
                        let f = self.tick >> shift;
                        if f >= n {
                            self.ended = true;
                            if hold {
                                self.mode = AnimMode::Pose;
                                return AnimOut::None;
                            }
                            return AnimOut::ToIdle;
                        }
                        self.frame = f;
                        AnimOut::None
                    }
                    TickKind::Bored => {
                        let f = self.tick >> 5;
                        if f >= n {
                            self.tick = 0x40;
                            self.frame = 2;
                        } else {
                            self.frame = f;
                        }
                        AnimOut::None
                    }
                    TickKind::Hold => AnimOut::None,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// scripted actors

/// A scripted actor (the game's NPC class, vtable 0x0804CEF4 for `spawn_actor`; map NPCs use the same object
/// layout). Field NPC behaviour (wander, talk) is the field layer's (`docs/core.md` §9).
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Npc {
    pub a: Actor,
    /// script sprite id (>= 10; content sprite index + 9)
    pub sprite_id: i32,
    /// map placement tag (negative actor ids address it as -(tag + 1)); None = spawned by a script
    pub tag: Option<i32>,
    pub anim: ActAnim,
    /// `tint` preset (index into the 20 presets of 0x08752EEC, `None` = own palette)
    pub tint: Option<u8>,
    /// `set_attachment` record (kind, variant) and offset
    pub attach: Option<[i32; 4]>,
    /// `set_actor_speed` (actor+0x180 = v << 10) and `set_actor_byte8`
    pub speed: i32,
    pub byte8: i32,
    /// item held up by `show_item_get` (item id; negative = equipment)
    pub hold_item: Option<i32>,
    /// talk event (`sNNN` number) of a map NPC
    pub talk: Option<i32>,
    /// field NPC behaviour (field.rs: map placement steps, the game's idle / walk animation, grid marks); None =
    /// only scripts move it
    #[serde(default)]
    pub behaviour: Option<crate::field::Behaviour>,
    /// a map NPC's talk resume node (object +0x1BC, u16; the talk start `0x08017D66` passes it to the scene start,
    /// whose end node writes the exit value back - see `TriggerRt::resume`). The constructor zeroes it (`0x08017C56`)
    #[serde(default)]
    pub resume: u16,
}

impl Npc {
    pub fn new(d: &GameData, sprite_id: i32, x: i32, y: i32, dir: Dir) -> Npc {
        let mut a = Actor::default();
        a.set_sprite(d, Some(sprite_id - SPRITE_BASE));
        a.x = x << 8;
        a.y = y << 8;
        a.facing = dir;
        Npc {
            a,
            sprite_id,
            tag: None,
            anim: ActAnim::idle(),
            tint: None,
            attach: None,
            speed: 0,
            byte8: 0x38,
            hold_item: None,
            talk: None,
            behaviour: None,
            resume: 0,
        }
    }
}

/// spawn a scripted actor (`spawn_actor`: facing down, idle)
pub fn spawn_npc(w: &mut World, sprite_id: i32, x: i32, y: i32, dir: Dir) -> EntityId {
    let mut n = Npc::new(&w.data, sprite_id, x, y, dir);
    // the actor's constructor enters the idle state: a random start tick (one RNG draw)
    n.anim.tick = w.rng.rand(255);
    let id = w.add(Body::Npc(Box::new(n)));
    let ni = w.ents.len() - 1;
    crate::vram::actor_created(w, ni);
    id
}

pub fn npc_update(w: &mut World, i: usize) {
    if matches!(&w.ents[i].body, Body::Npc(n) if n.behaviour.is_some())
        && !is_puppet(w, w.ents[i].id)
    {
        return crate::field::npc_update(w, i);
    }
    let d = w.data.clone();
    let out = if let Body::Npc(n) = &mut w.ents[i].body {
        let (s, f) = (n.a.sprite, n.a.facing);
        let o = n.anim.tick(&d, s, f);
        n.a.tick_anim();
        o
    } else {
        AnimOut::None
    };
    anim_out(w, i, out);
}

fn anim_out(w: &mut World, i: usize, out: AnimOut) {
    match out {
        AnimOut::None => {}
        AnimOut::ToIdle => set_state(w, i, 0),
        // the dash state's step sound is the player's (scripted actors run silently: buu_form)
        AnimOut::Sfx(..) => {}
    }
}

/// A player / enemy under script control. Players keep their own state machine (player.rs: the states the
/// commands set tick, the controller is off); enemies play `anim` instead of their own update.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Puppet {
    pub id: EntityId,
    pub anim: ActAnim,
}

pub fn is_puppet(w: &World, id: EntityId) -> bool {
    w.scene.puppets.iter().any(|p| p.id == id)
}

fn make_puppet(w: &mut World, i: usize) {
    let id = w.ents[i].id;
    if !is_puppet(w, id) {
        w.scene.puppets.push(Puppet {
            id,
            anim: ActAnim::idle(),
        });
    }
}

/// called at the top of the enemy update: true = the enemy is a puppet this frame (its update is done)
pub fn puppet_update(w: &mut World, i: usize) -> bool {
    let id = w.ents[i].id;
    if !matches!(w.ents[i].body, Body::Enemy(_)) {
        return false;
    }
    let Some(k) = w.scene.puppets.iter().position(|p| p.id == id) else {
        return false;
    };
    let (s, f) = match w.actor(i) {
        Some(a) => (a.sprite, a.facing),
        None => return false,
    };
    let d = w.data.clone();
    let out = w.scene.puppets[k].anim.tick(&d, s, f);
    if let Body::Enemy(e) = &mut w.ents[i].body {
        e.a.tick_anim();
    }
    anim_out(w, i, out);
    true
}

/// the animation the game shows for entity `i` (raw, cel): players their state's, Npcs / enemy puppets the scene's
pub fn shown_anim(w: &World, i: usize) -> Option<(i32, i32)> {
    match &w.ents[i].body {
        Body::Npc(n) => Some((n.anim.raw, n.anim.frame)),
        Body::Player(p) => Some((p.ap.raw, p.ap.frame)),
        _ => w
            .scene
            .puppets
            .iter()
            .find(|p| p.id == w.ents[i].id)
            .map(|p| (p.anim.raw, p.anim.frame)),
    }
}

/// the box of the cel an actor shows (frame record +4..+7: ox, oy, w, h relative to its integer position)
pub fn cel_box(w: &World, i: usize) -> Option<[i32; 4]> {
    let a = w.actor(i)?;
    let sp = w.data.sprite(a.sprite?)?;
    let (raw, frame) = match shown_anim(w, i) {
        Some(x) => x,
        None => (
            a.anim
                .and_then(|k| sp.anims.get(k as usize))
                .map(|an| an.raw)
                .unwrap_or(1),
            0,
        ),
    };
    let an = sp.anim_raw(raw)?;
    let st = an
        .steps(a.facing)
        .get(frame.max(0) as usize)
        .or_else(|| an.steps(a.facing).first())?;
    st.fbox
}

/// decompression cycles of the cel an actor shows (`AnimStep.dec`; 0 = unknown / no sprite)
pub fn cel_dec(w: &World, i: usize) -> u32 {
    let Some(a) = w.actor(i) else { return 0 };
    let Some(sp) = a.sprite.and_then(|s| w.data.sprite(s)) else {
        return 0;
    };
    let (raw, frame) = match shown_anim(w, i) {
        Some(x) => x,
        None => (
            a.anim
                .and_then(|k| sp.anims.get(k as usize))
                .map(|an| an.raw)
                .unwrap_or(1),
            0,
        ),
    };
    let Some(an) = sp.anim_raw(raw) else { return 0 };
    an.steps(a.facing)
        .get(frame.max(0) as usize)
        .or_else(|| an.steps(a.facing).first())
        .map(|s| s.dec)
        .unwrap_or(0)
}

/// the scene animation of an Npc / enemy puppet
pub fn scene_anim(w: &World, i: usize) -> Option<&ActAnim> {
    match &w.ents[i].body {
        Body::Npc(n) => Some(&n.anim),
        _ => w
            .scene
            .puppets
            .iter()
            .find(|p| p.id == w.ents[i].id)
            .map(|p| &p.anim),
    }
}

fn anim_mut(w: &mut World, i: usize) -> Option<&mut ActAnim> {
    let id = w.ents[i].id;
    let npc = matches!(w.ents[i].body, Body::Npc(_));
    // a map NPC with a field behaviour comes under script control (its behaviour pauses until the event ends;
    // its animation stays in `Npc.anim`)
    if !npc || matches!(&w.ents[i].body, Body::Npc(n) if n.behaviour.is_some()) {
        make_puppet(w, i);
    }
    // (an enemy's script animation replaces its controller: a charging ball goes - shots::drop_charging_ball - and a
    // charging Kamehameha, enemy::drop_charging_kame)
    if matches!(w.ents[i].body, Body::Enemy(_)) {
        crate::shots::drop_charging_ball(w, id);
        if w.field.is_some() {
            crate::enemy::drop_charging_kame(w, i);
        }
    }
    if npc {
        match &mut w.ents[i].body {
            Body::Npc(n) => Some(&mut n.anim),
            _ => None,
        }
    } else {
        w.scene
            .puppets
            .iter_mut()
            .find(|p| p.id == id)
            .map(|p| &mut p.anim)
    }
}

fn actor_mut(w: &mut World, i: usize) -> Option<&mut Actor> {
    match &mut w.ents[i].body {
        Body::Player(p) => Some(&mut p.a),
        Body::Enemy(e) => Some(&mut e.a),
        Body::Npc(n) => Some(&mut n.a),
        Body::Hand(h) => Some(&mut h.a),
        _ => None,
    }
}

/// SetAnim(id) (actor vtable +0x28) with a script anim id, including the state's enter (idle: a random tick, one
/// RNG draw; punches: SFX 50 + rand(2))
pub fn set_state(w: &mut World, i: usize, id: i32) {
    if matches!(w.ents[i].body, Body::Player(_)) {
        make_puppet(w, i);
        // a new state replaces a frame-range animation the player was playing (s464: play_anim_frames(0, 17, ..)
        // under the power struggle, then play_anim(0, 17) runs its 6 cels)
        let pid = w.ents[i].id;
        if let Some(pp) = w.scene.puppets.iter_mut().find(|p| p.id == pid) {
            if matches!(pp.anim.mode, AnimMode::Frames { .. }) {
                pp.anim = ActAnim::of(id);
            }
        }
        crate::player::script_set_anim(w, i, id.clamp(0, 24) as usize);
        return;
    }
    let mut a = ActAnim::of(id);
    match id {
        0 => a.tick = w.rng.rand(255),
        4..=6 => {
            let s = 50 + w.rng.rand(2);
            w.sfx(s, 64);
        }
        _ => {}
    }
    if matches!(w.ents[i].body, Body::Enemy(_)) {
        a.mode = AnimMode::Enemy;
    }
    if let Some(m) = anim_mut(w, i) {
        *m = a;
    }
}

/// SetPose (vtable +0x30): state `id`'s animation frozen on `frame`
pub fn set_pose(w: &mut World, i: usize, id: i32, frame: i32) {
    if matches!(w.ents[i].body, Body::Player(_)) {
        make_puppet(w, i);
        crate::player::script_pose(w, i, id.clamp(0, 24) as usize, frame);
        return;
    }
    let a = ActAnim {
        frame,
        mode: AnimMode::Pose,
        ..ActAnim::of(id)
    };
    if let Some(m) = anim_mut(w, i) {
        *m = a;
    }
}

fn set_frames(w: &mut World, i: usize, id: i32, mode: AnimMode) {
    let player = matches!(w.ents[i].body, Body::Player(_));
    let a = ActAnim {
        mode,
        fresh: w.scene.chaining,
        ..ActAnim::of(id)
    };
    if let Some(m) = anim_mut(w, i) {
        *m = a;
    }
    if player {
        // the player has no frame-range controller state of its own: it runs on the scene copy, shown as a pose
        crate::player::script_pose(w, i, id.clamp(0, 24) as usize, 0);
    }
}

/// the state's animation finished (vtable +0x34)
pub fn anim_done(w: &World, i: usize) -> bool {
    match &w.ents[i].body {
        Body::Player(_) => crate::player::script_anim_done(w, i),
        _ => scene_anim(w, i).is_none_or(|a| a.ended || a.id == 0),
    }
}

/// the scene released its actors: players / enemies resume their own update. A player left in a frame-range
/// animation keeps playing it until its player presses a button (`pre_step`), like the game's controller object.
pub fn release_puppets(w: &mut World) {
    let keep: Vec<bool> = w
        .scene
        .puppets
        .iter()
        .map(|p| {
            matches!(p.anim.mode, AnimMode::Frames { .. })
                && w.find(p.id)
                    .is_some_and(|i| matches!(w.ents[i].body, Body::Player(_)))
        })
        .collect();
    // map NPCs go back to their field behaviour with the animation the script left (one animation state in the
    // game: obj+0x50 / +0x11 / +0x12)
    for (p, &k) in w.scene.puppets.iter().zip(keep.iter()) {
        if k {
            continue;
        }
        let Some(i) = w.ents.iter().position(|e| e.id == p.id) else {
            continue;
        };
        if let Body::Npc(n) = &mut w.ents[i].body {
            let (raw, tick, frame) = (n.anim.raw, n.anim.tick, n.anim.frame);
            let pose = n.anim.mode == AnimMode::Pose;
            if let Some(b) = n.behaviour.as_mut() {
                b.anim = (raw - 1).clamp(0, 255) as u8;
                b.raw = raw;
                b.tick = tick;
                b.frame = frame;
                b.pose = pose;
            }
        }
        // an enemy likewise keeps the position the script's animation reached in the state they share (db5 12-27
        // db5_020 f1361: the Majin fighter's idle runs on at the scene's tick 753, not its own from before the scene)
        if let Body::Enemy(e) = &mut w.ents[i].body {
            if e.ea.pos.raw == p.anim.raw && !matches!(p.anim.mode, AnimMode::Pose) {
                e.ea.pos.frame = p.anim.frame;
                e.ea.pos.tick = p.anim.tick;
            }
        }
    }
    let mut it = keep.into_iter();
    w.scene.puppets.retain(|_| it.next().unwrap_or(false));
}

/// the actor's script sprite id (players / enemies: their sprite index + 9)
pub fn sprite_id(w: &World, i: usize) -> Option<i32> {
    match &w.ents[i].body {
        Body::Npc(n) => Some(n.sprite_id),
        // the player object's sprite def is its form's (gfx id), even where the content lacks that sheet
        // (Majin Buu's candy: SetDef(0x36), the sweets)
        Body::Player(p) if p.candy_def => Some(0x36),
        Body::Player(p) => w.data.form(p.form).map(|f| f.gfx),
        _ => w.actor(i).and_then(|a| a.sprite).map(|s| s + SPRITE_BASE),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// commands and tracks

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Owner {
    Entity(EntityId),
    Camera,
    /// a map prop (`prop_*` natives; the field layer's props)
    Prop(i32),
}

/// One queued command (B3.5; class names after the natives that create them).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cmd {
    /// 0x4A actor_wait, 0x68 prop_wait
    Wait {
        left: i32,
    },
    /// 0x3C play_anim (anim 0: instant)
    PlayAnim {
        anim: i32,
    },
    /// 0x3D set_anim
    SetAnim {
        anim: i32,
    },
    /// 0x51 set_pose
    SetPose {
        anim: i32,
        frame: i32,
    },
    /// 0x59 play_anim_frames
    PlayAnimFrames {
        anim: i32,
        each: i32,
        loops: i32,
        wait: bool,
        loop_start: i32,
        last: i32,
    },
    /// 0x53 face
    Face {
        dir: i32,
    },
    /// 0x47 face_actor (holds: stays the track's head)
    FaceActor {
        target: EntityId,
    },
    /// 0x44 walk_to (state 0xF), 0x50 move_to (state 2), 0x60 / 0x61 walk (state 1), 0x5F walk_to_actor;
    /// straight line at speed/100 px per frame (step math 0x080192CE), snapped at the end
    MoveTo {
        x: i32,
        y: i32,
        speed: i32,
        state: i32,
        target: Option<(EntityId, i32, i32)>,
        n: i32,
        t: i32,
        sx: i32,
        sy: i32,
    },
    /// 0x49 glide_to: as MoveTo without animation or facing change
    GlideTo {
        x: i32,
        y: i32,
        speed: i32,
        n: i32,
        t: i32,
        sx: i32,
        sy: i32,
    },
    /// 0x45 jump_to: 8 crouch frames, `air` frames of arc, 8 landing frames
    JumpTo {
        x: i32,
        y: i32,
        air: i32,
        t: i32,
        sx: i32,
        vy: i32,
    },
    /// 0x5D set_pos
    SetPos {
        x: i32,
        y: i32,
    },
    /// 0x3F puff / 0x40 puff_offset (box centre + offset), 0x48 puff_at / 0x65 prop_effect_at (absolute)
    Puff {
        x: i32,
        y: i32,
        absolute: bool,
    },
    /// 0x43 hit_flash
    HitFlash,
    /// 0x52 tint
    Tint {
        r: i32,
        g: i32,
        b: i32,
        a: i32,
    },
    /// 0x4D remove_actor
    Remove,
    /// 0x4E change_sprite
    ChangeSprite {
        sprite: i32,
    },
    /// 0x4F become_map_object
    BecomeMapObject {
        index: i32,
    },
    /// 0x4B queue_sfx
    Sfx {
        id: i32,
    },
    /// 0x4C screen_flash
    ScreenFlash {
        r: i32,
        g: i32,
        b: i32,
        frames: i32,
        t: i32,
    },
    /// 0x57 shake_screen
    Shake {
        intensity: i32,
        frames: i32,
        t: i32,
    },
    /// 0x58 show_item_get / 0x80 give_item / 0x93 give_equipment (60 frames)
    ItemGet {
        item: i32,
        t: i32,
    },
    /// 0x5A emote_wait / 0x5B emote
    Emote {
        emote: i32,
        wait: bool,
        t: i32,
    },
    /// 0x3B fire_wave_beam, 0x46 fire_beam, 0x5C fire_beam_alt, 0x5E fire_blast
    Beam {
        kind: BeamKind,
        dir: i32,
        frames: i32,
        t: i32,
    },
    /// 0x3E fire_ball_at, 0x41 shoot_at_actor, 0x42 shoot_at
    Shoot {
        kind: ShootKind,
        x: i32,
        y: i32,
        speed: i32,
        t: i32,
        n: i32,
    },
    /// 0x54 set_actor_speed, 0x56 set_actor_byte8, 0x55 set_attachment
    SetSpeed {
        v: i32,
    },
    SetByte8 {
        v: i32,
    },
    SetAttachment {
        kind: i32,
        variant: i32,
        dx: i32,
        dy: i32,
    },
    /// props: 0x62 poof, 0x63/0x64 hide/show, 0x66 move, 0x67 jump, 0x69 set pos
    PropPoof {
        vanish: bool,
        t: i32,
    },
    PropShow {
        on: bool,
    },
    PropMoveTo {
        x: i32,
        y: i32,
        speed: i32,
        n: i32,
        t: i32,
        sx: i32,
        sy: i32,
    },
    PropJumpTo {
        x: i32,
        y: i32,
        t: i32,
        vx: i32,
        vy: i32,
    },
    /// `solo`: enqueued outside a batch (done at its start: s464 n12); inside a batch it takes its tick (s464 n36:
    /// 80 positions of the Katchin block, one per frame plus the batch's start)
    PropSetPos {
        x: i32,
        y: i32,
        #[serde(default)]
        solo: bool,
    },
    /// camera track: 0x6A follow + wait, 0x6B pan, 0x6C release + wait, 0x6D / 0x6E params, 0x6F / 0x70 set
    CamFollowWait {
        target: EntityId,
        left: i32,
    },
    CamPanTo {
        x: i32,
        y: i32,
        speed: i32,
        n: i32,
        t: i32,
        sx: i32,
        sy: i32,
    },
    CamReleaseWait {
        left: i32,
    },
    CamParam {
        which: i32,
        v: i32,
    },
    CamSet {
        x: i32,
        y: i32,
        z: Option<i32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BeamKind {
    Wave,
    Beam,
    BeamAlt,
    Blast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShootKind {
    /// 0x3E: pink energy ball thrown toward a point
    Ball,
    /// 0x41 / 0x42: ki blast
    Blast,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Track {
    pub owner: Owner,
    pub queue: Vec<Cmd>,
    /// the head has been started (msg 0x0D)
    pub started: bool,
    /// the head finished and holds (FaceActor)
    pub holding: bool,
    /// the held FaceActor still turns toward its target until the wait mode it finished in ends (s160: the class
    /// follows the Red Shark announcer's walk in the same batch; s084: not in a later one)
    #[serde(default)]
    pub live: bool,
    /// the frame's tick already ran in the actor's own update (Some(idle)): `track_after_update`
    #[serde(default)]
    pub ticked: Option<bool>,
}

/// the director's wait mode (run_mode on WaitAll 0x0804C0C4 or the group 0x0804DEEC)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WaitMode {
    All,
    Group(Vec<Owner>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Director {
    /// `begin_parallel` open
    pub batch: bool,
    /// `wait_for_actor` group of the open batch
    pub group: Vec<Owner>,
    /// the wait mode running (the script is blocked on it)
    pub wait: Option<WaitMode>,
    /// the wait finished this frame (the host resumes the script)
    pub done: bool,
    /// the group mode runs only its members' updates (`World.modal` before it)
    pub prev_modal: Option<Option<Vec<EntityId>>>,
    /// the finished wait was a group mode (`done`): its loop ran before the world's pass
    #[serde(default)]
    pub done_group: bool,
}

// ---------------------------------------------------------------------------------------------------------------
// camera control

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CamMode {
    /// the player camera (battle::camera)
    #[default]
    Player,
    /// a follow object on another actor (0x33 camera_follow / 0x6A)
    Follow(EntityId),
    /// released (0x34 / 0x6C / any set or pan): the camera stays where it is
    Fixed,
}

/// modal scroll (0x31 camera_center_on / 0x32 camera_scroll_to): mover 0x08015B68
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scroll {
    pub tx: i32,
    pub ty: i32,
    pub sx: i32,
    pub sy: i32,
    pub n: i32,
    pub t: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CamCtl {
    pub mode: CamMode,
    pub scroll: Option<Scroll>,
    /// camera+8 / +0xC (0x6D / 0x6E; meaning unknown, kept)
    pub param8: i32,
    /// skip this frame's camera update (an area load snapped the camera within the frame)
    pub hold: bool,
    pub param12: i32,
    /// the follow object's target position of its last step (px): it keeps easing there when the followed actor
    /// is removed (story playthrough: Olibu walks off and is removed while the camera follows him)
    #[serde(default)]
    pub follow_last: Option<(i32, i32)>,
    /// this frame's camera update already ran (set by `camera`, cleared at the frame's start): a scroll started
    /// after it (a trigger's script) takes its first step at once
    #[serde(default)]
    pub stepped: bool,
    /// the camera rect before this frame's pan step: the BGs show it (the camera object copies its scroll before the
    /// director's tracks step the pan - rfat_010 11-17 row 540: BG1 HOFS 45, the camera 46.97 after its 2 px step)
    #[serde(default)]
    pub pan_pre: Option<Rect>,
}

// ---------------------------------------------------------------------------------------------------------------
// screen

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FadeKind {
    /// 0x78 / 0x79 (and scene loads, warps)
    Black,
    /// 0x7A / 0x7B
    White,
    /// 0x7C / 0x7D
    Mosaic,
    /// 0x7E flash_white (to white and back)
    Flash,
}

/// a running modal screen effect
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fade {
    pub kind: FadeKind,
    /// true = toward the effect (out), false = back to normal (in)
    pub out: bool,
    pub t: i32,
    pub len: i32,
}

/// palette flash of `screen_flash` (0x4C): toward (r, g, b) and back over `len` frames
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PalFlash {
    pub rgb: (u8, u8, u8),
    pub t: i32,
    pub len: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Layers {
    /// BG0..3 visible (DISPCNT bits 8..11)
    pub visible: [bool; 4],
    /// BGxCNT priority set by scripts (None = the map's)
    pub priority: [Option<u8>; 4],
    /// BLDCNT target bits set by scripts (bit0 first target, bit1 second target; None = the map's)
    pub blend: [Option<u8>; 4],
    /// BLDALPHA (eva, evb) in 1/16 (None = the map's)
    pub alpha: Option<(u8, u8)>,
    /// `set_bg_offset`: added to a BG layer's scroll (px)
    pub offset: [(i32, i32); 4],
}

impl Default for Layers {
    fn default() -> Self {
        Layers {
            visible: [true; 4],
            priority: [None; 4],
            blend: [None; 4],
            alpha: None,
            offset: [(0, 0); 4],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Screen {
    pub fade: Option<Fade>,
    /// the level an effect left behind: 0 normal .. 16 full (black / white / mosaic)
    pub black: i32,
    pub white: i32,
    pub mosaic: i32,
    /// the black fade in 1/256 (the palette scale: colour * (256 - dim) >> 8)
    pub dim: i32,
    pub flash: Option<PalFlash>,
    pub layers: Layers,
}

impl Default for Screen {
    fn default() -> Self {
        Screen {
            fade: None,
            black: 0,
            white: 0,
            mosaic: 0,
            dim: 0,
            flash: None,
            layers: Layers::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BossBar {
    pub entity: EntityId,
    pub a: i32,
    pub b: i32,
    /// the frame it was shown (`0x080195A8`: y -8, velocity +1 per update up to 8: `0x08019628`)
    #[serde(default)]
    pub at: u32,
}

/// a hidden boss bar sliding out (`hide_boss_bar`: message 0x13, velocity -1, the value frozen, the entity link
/// gone; the object deletes itself below y -8)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BossBarOut {
    /// the frozen fill (0..53 steps)
    pub fill: i32,
    /// y at frame `at`
    pub y: i32,
    pub at: u32,
}

impl BossBar {
    /// its y on frame `frame` (slides in from -8 to 8)
    pub fn y(&self, frame: u32) -> i32 {
        (-8 + frame.wrapping_sub(self.at) as i32).min(8)
    }

    /// its fill in steps of 53 (draw `0x08019658`: `a * 53 / 100` + the entity's vfunc +0x64 with
    /// `b * 53 / 100 - a * 53 / 100`; an enemy's (`0x0804F874`): `(hp >> 10) * arg / max hp`)
    pub fn fill(&self, w: &World) -> i32 {
        let lo = self.a * 53 / 100;
        let span = self.b * 53 / 100 - lo;
        let hp = w.find(self.entity).and_then(|k| match &w.ents[k].body {
            Body::Enemy(e) if e.max_hp1024 >> 10 != 0 => {
                Some((e.hp1024 >> 10) * span as i64 / (e.max_hp1024 >> 10))
            }
            _ => None,
        });
        lo + hp.unwrap_or(0) as i32
    }
}

impl BossBarOut {
    /// its y on frame `frame`; None once deleted
    pub fn y_at(&self, frame: u32) -> Option<i32> {
        let y = self.y - frame.wrapping_sub(self.at) as i32;
        (y >= -8).then_some(y)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TitleText {
    pub string: i32,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimerHud {
    Countdown,
    Stopwatch,
}

/// presentation objects scripts create (markers 0x9E/0x9F, box 0xA0, map markers 0xEC, energy sphere 0xF4,
/// radar blips 0xAC, emote balloons, item icons, beams, balls)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Visual {
    pub kind: VisualKind,
    pub x: i32,
    pub y: i32,
    pub owner: EntityId,
    pub t: i32,
    /// frames left (-1 = until removed)
    pub life: i32,
    /// the object's tile owner key (vram.rs; 0 = none)
    #[serde(default)]
    pub vid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VisualKind {
    MarkerA,
    MarkerB,
    Box { w: i32, h: i32 },
    MapMarker { highlight: bool },
    EnergySphere,
    Emote(i32),
    ItemIcon(i32),
    Beam { kind: BeamKind, dir: i32 },
    Ball,
    KiBlast,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Radar {
    pub targets: Vec<(i32, i32)>,
}

// ---------------------------------------------------------------------------------------------------------------
// dialog and menus

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoxPos {
    Top,
    Middle,
    #[default]
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DialogPhase {
    Opening,
    Shown,
    Closing,
    Closed,
}

/// one text box (event node kinds 5 / 6 / 1 / 2) or choice window (kind 3)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dialog {
    /// text without the placement prefix (`\n`, `\u{b}` page breaks, `\u{8}`+n colours)
    pub text: String,
    /// bank text reference (0 = inline / formatted text)
    pub text_ref: u32,
    pub portrait: u8,
    /// node kind (5 / 6 text, 3 choice)
    pub kind: u8,
    pub pos: BoxPos,
    pub page: u16,
    pub pages: u16,
    /// typewriter frames per page (host-computed with the dialogue font) and the characters typed on this page
    pub typing: Vec<u16>,
    pub typed: i32,
    /// choice: number of options (0 = plain text), cursor
    pub options: i32,
    pub cursor: i32,
    pub phase: DialogPhase,
    pub t: i32,
    /// set when the box closed: the choice result (-1 cancelled) or 0
    pub result: Option<i32>,
    /// the portrait sits left of the box (`0x08012012`, see [`speaker_layout`])
    #[serde(default)]
    pub portrait_left: bool,
    /// a message window (kind 3) that fades in / out ([`window_fades`], decided when it opens)
    #[serde(default)]
    pub fades: bool,

    /// `t` at the current page's first typed character (a choice page's cursor object is made then)
    #[serde(default)]
    pub page_t: i32,
    /// the slow text speeds' frame counter (text box +0x58: 0 at the box's creation, +1 each update without an A / B
    /// press while the speed is below 4; see [`type_steps`])
    #[serde(default)]
    pub type_c: i32,
}

/// The typewriter's steps this frame (text box update `0x0800B0A8`), by the options' text speed `s` (0..8,
/// `0x03001914`): from 4 up, `s - 3` steps every frame (4 = one character a frame); below 4, one step on a frame with
/// A or B pressed, else one when the counter (+0x58, +1 first) is a multiple of `5 - s` (speed 3: every 2nd frame,
/// 0: every 5th). A step (`0x0800AE74`) types the page's next character (a colour pair / a skipped new line ride
/// along), or on a complete page with A / B pressed this frame turns the page / closes the box (emulator, every
/// speed: shipr_227 7-2 rows 172-186, `work/earth/timing/speedprobe.py`).
pub fn type_steps(speed: u8, pressed: bool, counter: &mut i32) -> i32 {
    let s = speed.min(8) as i32;
    if s >= 4 {
        s - 3
    } else if pressed {
        1
    } else {
        *counter += 1;
        (*counter % (5 - s) == 0) as i32
    }
}

/// the first character appears this many frames after the box is created; the event continues this many frames
/// after the closing A (dialog object 0x0800AD00; measured on events 43 and 74)
pub const DIALOG_OPEN: i32 = 7;
pub const DIALOG_CLOSE: i32 = 9;
/// a menu / message window (node kind 3) continues the event this many frames after the A / B (event 34)
pub const WINDOW_CLOSE: i32 = 2;
/// ... and this many when it fades (see [`window_fades`]): it fades out over 16 frames (BLDALPHA 0x10 -> 0x0F01,
/// one step a frame from the frame after the press), is gone on the 16th, and the next node runs the frame after
/// (real game, West City Z-Mart s030 at 0-92: six windows, all 17)
pub const WINDOW_FADE_CLOSE: i32 = 17;
/// ... and its fade-in: no input for this many updates (BLDALPHA 0 -> 0x10, one step a frame)
pub const WINDOW_FADE_IN: i32 = 16;

/// A message window fades in and out (`0x08030ED2`: BLDCNT := 0x3E41, BLDALPHA ramped by the callback 0x08030BF5)
/// only when the current BLDCNT has no first-target layer (bits 0..5 clear); otherwise it shows and goes at once.
/// BLDCNT here = the map's (`bldcnt` property: 0-91 / 0-92 0x1040 fade; 1-1, 4-1 0x1041 not) with the scripts'
/// layer overrides.
pub fn window_fades(w: &World) -> bool {
    let Some(f) = w.field.as_ref() else {
        return false;
    };
    let Some(m) = w
        .data
        .field
        .as_ref()
        .and_then(|d| d.maps.get(&f.map).map(|m| m.bldcnt))
    else {
        return false;
    };
    let mut first = m & 0x3F;
    for (i, b) in w.scene.screen.layers.blend.iter().enumerate() {
        if let Some(b) = b {
            if b & 1 != 0 {
                first |= 1 << i;
            } else {
                first &= !(1 << i);
            }
        }
    }
    first == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuKind {
    /// 0xCE open_shop_buy
    ShopBuy,
    /// 0xCF open_shop_sell(percent)
    ShopSell { percent: i32 },
    /// 0x8C save_menu
    Save,
    /// 0x8D character_menu
    Character,
    /// 0xA7 watch_this_game(level)
    WatchThis { level: i32 },
    /// 0xC6 run_power_struggle
    PowerStruggle,
    /// 0xC7 run_punch_minigame(mode)
    Punch { mode: i32 },
    /// 0xD1 run_credits
    Credits,
}

/// a modal menu / mini-game screen (deterministic: driven only by the leader's input)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Menu {
    pub kind: MenuKind,
    pub cursor: i32,
    pub t: i32,
    /// mini-game gauge / progress
    pub gauge: i32,
    /// fade in / out frames of the screen change
    pub fade: i32,
    pub closing: bool,
    pub result: Option<i32>,
    /// items bought / sold (shop), for the host to apply to the inventory
    pub log: Vec<(i32, i32)>,
    /// a purchase confirmed this frame (applied the next)
    pub pending: Option<i32>,
}

/// shop entry (`shop_add_item`): category 0 supply / 1 equipment, item, price, flag
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShopItem {
    pub category: i32,
    pub item: i32,
    pub price: i32,
    pub flag: i32,
}

/// things for the session / frontend (not World state changes the core can do itself)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Request {
    ReturnToTitle,
    Credits,
    Sleep,
    /// battle mode has no field: scene loads / warps are reported (field mode handles them in field.rs)
    LoadScene {
        world: i32,
        map: i32,
        variant: i32,
        x: i32,
        y: i32,
        objects: bool,
    },
    Warp {
        world: i32,
        map: i32,
        variant: i32,
        x: i32,
        y: i32,
    },
    SaveField,
    ReturnToField,
    /// engine broadcasts with unknown receivers (0xC9, 0xDC, 0xF0)
    Broadcast {
        msg: i32,
        a: i32,
        b: i32,
    },
}

/// a collision override (`open_area` / `block_area`): re-applied when a byte snapshot is restored
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AreaOp {
    pub rect: [i32; 4],
    /// true: walk + shot grids; false: the third grid (+0xAC)
    pub main: bool,
    pub open: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Prop {
    pub id: i32,
    /// position in 1/256 px (prop +0x24 / +0x28); the area's prop rect moves with it (+0x2C)
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub frame: i32,
    /// prop_jump_to's vertical speed (1/256 px per frame)
    pub vy: i32,
    /// the area load this prop belongs to (`Field.loads`)
    pub load: u32,
}

// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timers {
    pub countdown: i32,
    pub countdown_on: bool,
    pub stopwatch: i32,
    pub stopwatch_on: bool,
    /// the countdown was started in the last script phase (see `tick_timers`)
    #[serde(default)]
    pub fresh: bool,
}

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct Scene {
    pub tracks: Vec<Track>,
    pub director: Director,
    pub puppets: Vec<Puppet>,
    pub cam: CamCtl,
    pub screen: Screen,
    pub hud_hidden: bool,
    /// the HUD object's position (+0x28) and step (+0x14), its update 0x0800B8FC: pos += step; below -16 it is gone,
    /// past 8 it stops at 8. `show_hud` (0x080046F4) makes it at -16 moving +1, `hide_hud` (0x08004764, message
    /// 0x15) sets -1 (real route rclose_002 4-1 f1,973 .. 1,981: OAM y 0 .. 8)
    #[serde(default = "hud_y_rest")]
    pub hud_y: i32,
    #[serde(default)]
    pub hud_vel: i32,
    pub boss_bar: Option<BossBar>,
    /// a hidden boss bar still sliding out
    #[serde(default)]
    pub boss_bar_out: Option<BossBarOut>,
    /// the title texts shown (`show_title_text` 0xB2: each call makes its own object, "Chapter 12" + "A New
    /// Beginning"; 0xEF deletes them all)
    #[serde(default)]
    pub titles: Vec<TitleText>,
    pub timer_hud: Option<TimerHud>,
    /// the script countdown / stopwatch in frames (GameData +0x4D4 / +0x4D8, running = game bits 2 / 4): set and
    /// ticked by the script natives / `wf_script_core::tick_timers`, read by the display (`RenderView.timer`)
    #[serde(default)]
    pub timers: Timers,
    /// destination label set (0xD9) and its 32 entries (0xDA)
    pub dest_label: Option<(i32, Vec<i32>)>,
    /// the travel menu's selection (label entity vt 0x0804BAF4 +0x10): an index into `Field.triggers` (= the label
    /// index); see `field::dest_menu`
    #[serde(default)]
    pub dest_sel: i32,
    pub radar: Option<Radar>,
    pub visuals: Vec<Visual>,
    /// the BG sphere's rim jitter of this frame's redraw (rand(4) per row 0 .. radius - 3, `0x0803AA58`)
    #[serde(default)]
    pub sphere_jitter: Vec<u8>,
    pub dialog: Option<Dialog>,
    pub menu: Option<Menu>,
    pub shop: Vec<ShopItem>,
    pub requests: Vec<Request>,
    pub area_ops: Vec<AreaOp>,
    /// an emote object's tile key and the next entity id at its creation (its place in the object list)
    #[serde(default)]
    pub emote_stamps: Vec<(u32, u32)>,
    pub props: Vec<Prop>,
    /// event graphs running (run_event nesting, 0x03002A5C): players get no input while > 0
    pub depth: u32,
    /// a modal mode that does not run the world update is up (camera scroll, mosaic)
    pub freeze: bool,
    /// the song playing (-1 none): PlaySong 0x0803F110 does not restart the song already playing
    pub song: i32,
    /// a track is starting its next command after a pop (inside the frame's track tick)
    pub chaining: bool,
    /// the frame's raw input (script queries read it even while the players' input is masked)
    pub keys: [InputFrame; MAX_PLAYERS],
    /// the player objects while a cutscene scene is loaded (load_scene 0x72-0x74 builds the map with no player
    /// object, 0x08009030); `return_to_field` / a warp brings them back
    pub offstage: Vec<crate::world::Entity>,
    /// the field `save_field_on_next_scene` kept for `return_to_field` (field.rs `save_world`)
    #[serde(default)]
    pub saved_world: Option<Box<crate::field::SavedWorld>>,
    /// enemies whose AI InlineScript node waits for its script (the host calls `ai_script_done`)
    pub ai_wait: Vec<EntityId>,
    /// the area-name banner (object 0x0800BF34): its text and frames since its first draw (0..=238)
    pub banner: Option<(String, u32)>,
    /// the last area name id shown (RAM 0x03001984, kept across loads)
    pub banner_last: i64,
    /// the frame a load made the banner in (its age starts after that frame's end)
    #[serde(default)]
    pub banner_made: u32,
    /// the running event's speaker anchor (scene object +0xC / +0x10, ctor `0x0801246E`): a map NPC's talk event
    /// (`0x08017D44`) passes player - NPC (24.8), every other event (-1, -1). Places text boxes ([`speaker_layout`])
    #[serde(default = "no_anchor")]
    pub talk_anchor: [i32; 2],
}

fn no_anchor() -> [i32; 2] {
    NO_ANCHOR
}

/// the anchor of events not started by a talk (`0x080707C0` / `0x080728A4`)
pub const NO_ANCHOR: [i32; 2] = [-1, -1];

impl Default for Scene {
    fn default() -> Self {
        Scene {
            tracks: Vec::new(),
            director: Director::default(),
            puppets: Vec::new(),
            cam: CamCtl::default(),
            screen: Screen::default(),
            hud_hidden: false,
            hud_y: 8,
            hud_vel: 0,
            boss_bar: None,
            boss_bar_out: None,
            titles: Vec::new(),
            timer_hud: None,
            timers: Timers::default(),
            dest_label: None,
            dest_sel: 0,
            radar: None,
            visuals: Vec::new(),
            sphere_jitter: Vec::new(),
            dialog: None,
            menu: None,
            shop: Vec::new(),
            requests: Vec::new(),
            area_ops: Vec::new(),
            emote_stamps: Vec::new(),
            props: Vec::new(),
            depth: 0,
            freeze: false,
            song: -1,
            chaining: false,
            offstage: Vec::new(),
            saved_world: None,
            ai_wait: Vec::new(),
            banner: None,
            banner_last: 0,
            banner_made: u32::MAX,
            keys: [InputFrame::default(); MAX_PLAYERS],
            talk_anchor: NO_ANCHOR,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// enqueue / director

fn track_index(w: &mut World, owner: Owner) -> usize {
    if let Some(k) = w.scene.tracks.iter().position(|t| t.owner == owner) {
        return k;
    }
    w.scene.tracks.push(Track {
        owner,
        queue: Vec::new(),
        started: false,
        holding: false,
        live: false,
        ticked: None,
    });
    w.scene.tracks.len() - 1
}

/// Enqueue a command on an owner's track (`0x0801936A`). Outside a batch the director starts the wait-all mode
/// at once (the caller blocks until [`wait_done`]); inside a batch the command waits for `end_parallel`.
/// Returns true when the caller must block.
pub fn enqueue(w: &mut World, owner: Owner, cmd: Cmd) -> bool {
    let k = track_index(w, owner);
    // a finished head that only holds (face_actor) gives way to the new command
    if w.scene.tracks[k].holding {
        w.scene.tracks[k].queue.clear();
        w.scene.tracks[k].holding = false;
        w.scene.tracks[k].started = false;
    }
    w.scene.tracks[k].queue.push(cmd);
    if w.scene.director.batch {
        return false;
    }
    begin_wait(w, WaitMode::All);
    sync_props(w);
    true
}

/// a command whose actor resolved to nothing (deleted by the game): outside a batch the wait mode still runs
pub fn enqueue_none(w: &mut World) {
    begin_wait(w, WaitMode::All);
}

/// `begin_parallel`
pub fn begin_batch(w: &mut World) {
    w.scene.director.batch = true;
    w.scene.director.group.clear();
}

/// `wait_for_actor`
pub fn batch_group(w: &mut World, owner: Owner) {
    if !w.scene.director.group.contains(&owner) {
        w.scene.director.group.push(owner);
    }
}

/// `end_parallel`: run the batch (group or all) and block until it is idle
pub fn commit(w: &mut World) {
    let group = std::mem::take(&mut w.scene.director.group);
    w.scene.director.batch = false;
    let mode = if group.is_empty() {
        WaitMode::All
    } else {
        WaitMode::Group(group)
    };
    begin_wait(w, mode);
}

fn in_mode(mode: &WaitMode, o: Owner) -> bool {
    match mode {
        WaitMode::All => true,
        WaitMode::Group(g) => o == Owner::Camera || g.contains(&o),
    }
}

/// enter a wait mode: start every head the mode covers (msg 0x0D)
fn begin_wait(w: &mut World, mode: WaitMode) {
    // heads start in track order; the SetAnim heads of posed actors among them (play_anim / set_anim after a
    // set_pose: the idle enter draws) take their turns in the world list's order, last object first, like the ticks
    // (s019 on 0-9: two play_anim(.., 0) in one batch - Trunks, spawned second, draws before Goten; the other heads
    // keep the track order: sb 21-25). INFERRED from those two recordings.
    // (only actors a `set_pose` holds: their SetAnim leaves the pose in the object's own turn)
    let posed = |w: &World, o: Owner| match o {
        Owner::Entity(id) => w.find(id).is_some_and(
            |i| matches!(&w.ents[i].body, Body::Npc(n) if matches!(n.anim.mode, AnimMode::Pose)),
        ),
        _ => false,
    };
    let is_anim = |t: &Track| {
        !t.started
            && matches!(
                t.queue.first(),
                Some(Cmd::PlayAnim { .. } | Cmd::SetAnim { .. })
            )
            && posed(w, t.owner)
    };
    let key = |w: &World, o: Owner| -> i64 {
        match o {
            Owner::Entity(id) => -(w.find(id).map(|i| i as i64).unwrap_or(-1)),
            _ => i64::MIN,
        }
    };
    let mut owners: Vec<Owner> = w.scene.tracks.iter().map(|t| t.owner).collect();
    let slots: Vec<usize> = (0..owners.len())
        .filter(|&k| in_mode(&mode, owners[k]) && is_anim(&w.scene.tracks[k]))
        .collect();
    let mut anim: Vec<Owner> = slots.iter().map(|&k| owners[k]).collect();
    anim.sort_by_key(|&o| key(w, o));
    for (j, &k) in slots.iter().enumerate() {
        owners[k] = anim[j];
    }
    for o in owners {
        let Some(k) = w.scene.tracks.iter().position(|t| t.owner == o) else {
            continue;
        };
        if !in_mode(&mode, w.scene.tracks[k].owner) {
            continue;
        }
        if !w.scene.tracks[k].started && !w.scene.tracks[k].queue.is_empty() {
            start_head(w, k);
        }
    }
    // the group mode (0x0804DEEC) updates only the group's actors (msg 0x0E), not the whole world
    if let WaitMode::Group(g) = &mode {
        let ids: Vec<EntityId> = g
            .iter()
            .filter_map(|o| {
                if let Owner::Entity(id) = o {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        if w.scene.director.prev_modal.is_none() {
            w.scene.director.prev_modal = Some(w.modal.take());
        }
        w.modal = Some(ids);
    }
    w.scene.director.wait = Some(mode);
    w.scene.director.done = false;
}

/// the wait mode finished this frame: the host resumes the script (clears the flag)
pub fn wait_done(w: &mut World) -> bool {
    if w.scene.director.done {
        w.scene.director.done = false;
        true
    } else {
        false
    }
}

/// is anything still queued for this owner
pub fn track_busy(w: &World, owner: Owner) -> bool {
    w.scene
        .tracks
        .iter()
        .any(|t| t.owner == owner && !t.queue.is_empty() && !t.holding)
}

// ---------------------------------------------------------------------------------------------------------------
// command semantics

fn owner_index(w: &World, o: Owner) -> Option<usize> {
    match o {
        Owner::Entity(id) => w.find(id).filter(|&i| !w.ents[i].removed),
        _ => None,
    }
}

/// step math 0x080192CE: frames = (100 * isqrt((dx² + dy²) << 16) / speed >> 8) + 1, step = (d << shift) / frames
pub fn step_math(dx: i32, dy: i32, speed: i32, shift: u32) -> (i32, i32, i32) {
    if dx == 0 && dy == 0 {
        return (1, 0, 0);
    }
    let d2 = (dx as i64 * dx as i64 + dy as i64 * dy as i64) << 16;
    let s = crate::fixed::isqrt(d2);
    let n = ((100 * s) / speed.max(1) as i64 >> 8) as i32 + 1;
    let n = n.max(1);
    (
        n,
        (((dx as i64) << shift) / n as i64) as i32,
        (((dy as i64) << shift) / n as i64) as i32,
    )
}

fn actor_pos(w: &World, i: usize) -> (i32, i32) {
    w.actor(i).map(|a| (a.px(), a.py())).unwrap_or((0, 0))
}

/// the actor's body-box centre (shot targets, puffs)
pub fn box_center(w: &World, i: usize) -> (i32, i32) {
    w.actor(i).map(|a| a.center()).unwrap_or((0, 0))
}

/// a shot's target point on an actor (ShootAtActor's start 0x08027BE6): the centre of the shown frame record's box
/// (0x0800FE34; Babidi's ship 8-16 s319: Dabura's spit at the Supreme Kai flies to (232, 136)); the facing-down
/// body box when the record is unknown
pub fn target_center(w: &World, i: usize) -> (i32, i32) {
    let Some(a) = w.actor(i) else { return (0, 0) };
    if let Some(b) = cel_box(w, i) {
        let (x0, y0) = (a.px() + b[0], a.py() + b[1]);
        return ((x0 + x0 + b[2]) >> 1, (y0 + y0 + b[3]) >> 1);
    }
    let b = a.body[0];
    (
        (a.px() * 2 + b[0] + b[2]) >> 1,
        (a.py() * 2 + b[1] + b[3]) >> 1,
    )
}

/// where a scripted ki blast leaves the actor (0x0800FF44, from the facing's body box 0x0800FE78): its leading edge
/// (left / right) at the middle height, its centre (up), or the centre x at the actor's own y (down: 8-16 s319,
/// Dabura's box is not the standard one)
pub fn muzzle(w: &World, i: usize) -> (i32, i32) {
    let Some(a) = w.actor(i) else { return (0, 0) };
    let b = a.body_box();
    let (cx, cy) = ((b.x0 + b.x1) >> 1, (b.y0 + b.y1) >> 1);
    match a.facing {
        0 => (cx, a.py()),
        1 => (cx, cy),
        2 => (b.x0, cy),
        _ => (b.x1, cy),
    }
}

/// commands whose start refreshes the actor / camera (vtable +0x3C / +0x40, 0x0802ED98): the refresh ticks the
/// track, so they are done and popped within the start (set_pos and the camera sets take 1 frame, not 2)
fn done_at_start(cmd: &Cmd) -> bool {
    // remove_actor deletes the actor and its track with it (1 frame, trace boss_janemba_handover)
    // prop_set_pos too (s464: the boulder placed between Goku's walk and his pose: 1 frame)
    matches!(
        cmd,
        Cmd::SetPos { .. }
            | Cmd::CamSet { .. }
            | Cmd::CamParam { .. }
            | Cmd::Remove
            | Cmd::PropSetPos { solo: true, .. }
    )
}

/// wf-import (an event imported mid-node): start track `k`'s head as its tick would (the importer then sets the
/// command's progress from RAM)
pub fn import_start_head(w: &mut World, k: usize) {
    start_head(w, k);
}

fn start_head(w: &mut World, k: usize) {
    loop {
        let Some(t) = w.scene.tracks.get_mut(k) else {
            return;
        };
        t.started = true;
        t.holding = false;
        let owner = t.owner;
        let Some(mut cmd) = t.queue.first().cloned() else {
            return;
        };
        start_cmd(w, owner, &mut cmd);
        // an instant command reached by a track's pop-and-start keeps the track for this tick; the next command
        // starts in the next one (story playthrough: s073 walk_to -> set_pos -> face(206, 2))
        let chained = w.scene.chaining;
        let Some(t) = w.scene.tracks.get_mut(k) else {
            return;
        };
        if done_at_start(&cmd) && !chained {
            if !t.queue.is_empty() {
                t.queue.remove(0);
            }
            t.started = false;
            if t.queue.is_empty() {
                return;
            }
            // fresh (not chained): the next command starts in the next tick (story playthrough s101: set_pos ->
            // face(60, 0) -> walk_to_speed: the face lands a frame after the block starts, the walk a frame later)
            return;
        }
        if let Some(c) = t.queue.first_mut() {
            *c = cmd;
        }
        return;
    }
}

/// the player-0 camera update (field or battle; a script's camera control first): the frame's camera step
pub fn camera_update(w: &mut World) {
    if w.field.is_some() {
        crate::field::camera(w, 0);
    } else {
        crate::battle::camera(w, 0);
    }
}

fn start_cmd(w: &mut World, owner: Owner, cmd: &mut Cmd) {
    let i = owner_index(w, owner);
    match cmd {
        Cmd::PlayAnim { anim } => {
            // the command's start is SetAnim(anim) (0x08015944 -> vt+0x28), anim 0 included: the NPC class always
            // re-enters the state (vt+0x2C 0x08011162 -> SetState), so idle draws its rand(255) even on an idle
            // actor (s025, s464); a player always re-enters idle too (story playthrough s101)
            if let Some(i) = i {
                set_state(w, i, *anim);
            }
        }
        Cmd::SetAnim { anim } => {
            if let Some(i) = i {
                set_state(w, i, *anim);
            }
        }
        Cmd::SetPose { anim, frame } => {
            if let Some(i) = i {
                set_pose(w, i, *anim, *frame);
            }
        }
        Cmd::PlayAnimFrames {
            anim,
            each,
            loops,
            loop_start,
            last,
            ..
        } => {
            if let Some(i) = i {
                set_frames(
                    w,
                    i,
                    *anim,
                    AnimMode::Frames {
                        each: *each,
                        loops: *loops,
                        loop_start: *loop_start,
                        last: *last,
                        pass: 0,
                    },
                );
            }
        }
        Cmd::Face { dir } => {
            if let Some(a) = i.and_then(|i| actor_mut(w, i)) {
                a.facing = (*dir & 3) as Dir;
            }
        }
        // FaceActor's start does nothing: its update turns the actor (0x0801599D)
        Cmd::FaceActor { .. } => {}
        Cmd::MoveTo {
            x,
            y,
            speed,
            state,
            target,
            n,
            t,
            sx,
            sy,
        } => {
            if let Some(i) = i {
                if let Some((tid, dx, dy)) = *target {
                    if let Some(ti) = w.find(tid) {
                        let (tx, ty) = actor_pos(w, ti);
                        *x = tx + dx;
                        *y = ty + dy;
                    }
                }
                let (px, py) = actor_pos(w, i);
                let (nn, ssx, ssy) = step_math(*x - px, *y - py, *speed, 8);
                (*n, *sx, *sy, *t) = (nn, ssx, ssy, 0);
                crate::fieldnpc::script_move_mark(w, i, false);
                if let Some(a) = actor_mut(w, i) {
                    // the facing from the whole-pixel difference; none (a sub-pixel walk) faces down (story
                    // playthrough: Yemma's s048 walk_to(0, 232, 360) from y 360.625)
                    a.facing = if *x == px && *y == py {
                        0
                    } else {
                        facing_toward(*x - px, *y - py)
                    };
                }
                set_state(w, i, *state);
            }
        }
        Cmd::GlideTo {
            x,
            y,
            speed,
            n,
            t,
            sx,
            sy,
        } => {
            if let Some(i) = i {
                let (px, py) = actor_pos(w, i);
                let (nn, ssx, ssy) = step_math(*x - px, *y - py, *speed, 8);
                (*n, *sx, *sy, *t) = (nn, ssx, ssy, 0);
                // a glide lifts the NPC's grid mark like a walk and sets none where it stops (story playthrough
                // s101: the fighters knocked back from (328, 384) leave the floor free for Goku's walk)
                crate::fieldnpc::script_move_mark(w, i, false);
            }
        }
        Cmd::JumpTo {
            x,
            y,
            air,
            t,
            sx,
            vy,
        } => {
            if let Some(i) = i {
                let (px, py) = actor_pos(w, i);
                let a = (*air).max(1);
                *sx = ((*x - px) << 8) / a;
                // y velocity (1/256 px): gravity 8 is added before each step
                *vy = ((*y - py) << 8) / a - (8 * a) / 2;
                *t = 0;
                if let Some(ac) = actor_mut(w, i) {
                    if *x != px || *y != py {
                        ac.facing = facing_toward(*x - px, *y - py);
                    }
                }
                // crouch: state 14's first cel (msg 0xE, 0)
                set_pose(w, i, 14, 0);
            }
        }
        Cmd::SetPos { x, y } => {
            if let Some(i) = i {
                // a map NPC's walk mark moves with it (Nataday Village 15-1: the on-enter script puts the elder
                // beside the gate, the spot he was placed on is free)
                crate::fieldnpc::script_move_mark(w, i, false);
                if let Some(a) = actor_mut(w, i) {
                    a.x = *x << 8;
                    a.y = *y << 8;
                }
                crate::fieldnpc::script_move_mark(w, i, true);
            }
        }
        Cmd::Puff { x, y, absolute } => {
            if let Some(i) = i {
                let (cx, cy) = if *absolute {
                    (*x, *y)
                } else {
                    let (bx, by) = box_center(w, i);
                    (bx + *x, by + *y)
                };
                // the cloud is the explosion 0x08712FDC at rate 0x1000 + rand(0x1000) (0x0801952C: its run at
                // 0x08019564; story playthrough: the gate's puffs; sb_007 6-9 f12392: s232's five clouds are the tan
                // explosion, not the grey smoke puff 0x086FD9D0)
                let rate = 0x1000 + w.rng.rand(0x1000);
                crate::fx::explosion(w, cx, cy, rate);
            } else if *absolute && matches!(owner, Owner::Prop(_)) {
                let rate = 0x1000 + w.rng.rand(0x1000);
                crate::fx::explosion(w, *x, *y, rate);
            }
        }
        Cmd::HitFlash => {
            if let Some(a) = i.and_then(|i| actor_mut(w, i)) {
                a.flash = 12;
            }
        }
        Cmd::Tint { r, g, b, a } => {
            let preset = tint_preset(*r, *g, *b, *a);
            if let Some(i) = i {
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.tint = preset;
                }
            }
        }
        Cmd::Remove => {
            if let Some(i) = i {
                if matches!(w.ents[i].body, Body::Enemy(_)) {
                    crate::nodes::exit_on_remove(w, i);
                }
                crate::fieldnpc::script_move_mark(w, i, false);
                // the object is destroyed now: its tiles go (segment sb 7-5)
                crate::vram::entity_gone(w, i);
                w.ents[i].removed = true;
                if matches!(w.ents[i].body, Body::Player(_)) {
                    if let Some(f) = w.field.as_mut() {
                        f.player_removed = true;
                    }
                }
                if w.scene
                    .boss_bar
                    .as_ref()
                    .is_some_and(|b| b.entity == w.ents[i].id)
                {
                    w.scene.boss_bar = None;
                    crate::vram::release(w, crate::vram::Owner::BossBar);
                }
            }
        }
        Cmd::ChangeSprite { sprite } => {
            if let Some(i) = i {
                let (px, py) = actor_pos(w, i);
                let dir = w.actor(i).map(|a| a.facing).unwrap_or(0);
                // the old actor's object is destroyed first: its tiles are free for the new one (segment sb 20-8);
                // its destructor lifts its walk-grid mark (segment sb 21-18: Buu 123 -> 126 left (152, 200) blocked)
                crate::fieldnpc::script_move_mark(w, i, false);
                crate::vram::entity_gone(w, i);
                // same place and facing (0x080192BA moves the queue)
                let id = spawn_npc(w, *sprite, px, py, dir);
                let old = w.ents[i].id;
                w.ents[i].removed = true;
                // the remaining queued commands move to the new actor (0x080192BA)
                if let Some(t) = w
                    .scene
                    .tracks
                    .iter_mut()
                    .find(|t| t.owner == Owner::Entity(old))
                {
                    t.owner = Owner::Entity(id);
                }
            }
        }
        Cmd::BecomeMapObject { index } => {
            if let Some(i) = i {
                let (px, py) = actor_pos(w, i);
                let dir = w.actor(i).map(|a| a.facing).unwrap_or(0);
                let old = w.ents[i].id;
                // the NPC's destructor lifts its grid mark (story playthrough s101: Pikkon's spot at (376, 384)
                // stays walkable once he is the enemy)
                crate::fieldnpc::script_move_mark(w, i, false);
                // (its tiles go before the map object takes its own: segment sb 21-19, the same block)
                crate::vram::entity_gone(w, i);
                w.ents[i].removed = true;
                // a placed enemy replaced this way is not defeated: no on-defeat callback (7-6, the tournament ring:
                // Yamu's defeat s309 turns Spopovich #8 into #11 - the game runs no s308; app soak of the ship route)
                if let Some(f) = w.field.as_mut() {
                    f.placed.retain(|p| p.0 != old);
                    f.placed_loot.retain(|l| l.0 != old);
                }
                // the remaining queued commands move to the map object (0x080192BA), as for change_sprite
                if let Some(id) = become_map_object(w, *index, px, py, dir) {
                    if let Some(t) = w
                        .scene
                        .tracks
                        .iter_mut()
                        .find(|t| t.owner == Owner::Entity(old))
                    {
                        t.owner = Owner::Entity(id);
                    }
                }
            }
        }
        Cmd::Sfx { id } => w.sfx(*id, 64),
        Cmd::ScreenFlash { r, g, b, frames, t } => {
            let len = (*frames).max(2);
            w.scene.screen.flash = Some(PalFlash {
                rgb: (*r as u8, *g as u8, *b as u8),
                t: 0,
                len,
            });
            *t = 0;
        }
        Cmd::Shake {
            intensity,
            frames,
            t,
        } => {
            // the shake object (0x0804A720) is updated in the frame it is created: its first tick (and RNG
            // draws) happen now; later ticks in the world update (story playthrough, Yemma's s048)
            // field: the shake it suspends did not tick in this frame (the game makes the script's shake before the
            // task list runs: the real route's arena, s450 - the killing punch's crit shake resumes after the script's
            // 60 frames with its count as it was 4 frames in, not 5). A tick that drew stays (never seen).
            if w.field.is_some() && w.g.shakes_ticked == w.frame {
                if let Some(s) = w.g.shakes.last_mut().filter(|s| s.cnt != s.period) {
                    s.left += 1;
                    s.cnt += 1;
                }
            }
            let mut others = std::mem::take(&mut w.g.shakes);
            w.shake(*frames, *intensity);
            w.tick_shakes();
            others.append(&mut w.g.shakes);
            w.g.shakes = others;
            *t = 0;
        }
        Cmd::ItemGet { item, t } => {
            *t = 0;
            w.sfx(45, 64);
            if let Some(i) = i {
                // the hold-up pose: state 13's second cel
                set_pose(w, i, 13, 1);
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.hold_item = Some(*item);
                }
                let (x, y) = actor_pos(w, i);
                let owner = w.ents[i].id;
                let vid = w.next_serial();
                w.scene.visuals.push(Visual {
                    kind: VisualKind::ItemIcon(*item),
                    x,
                    y: y - 32,
                    owner,
                    t: 0,
                    life: 60,
                    vid,
                });
                // the icon object's tiles (`0x0802BD4C`, vtable 0x0804B358 over the sprite object `0x0800D960`:
                // width x height bytes of the item's icon def - run 2 cut 62,000 f7: 0x100, freed 60 updates later)
                crate::vram::acquire(w, crate::vram::Owner::Visual(vid), item_icon_bytes(*item));
            }
        }
        Cmd::Emote { emote, t, .. } => {
            *t = 0;
            if let Some(i) = i {
                let (x, y) = actor_pos(w, i);
                let owner = w.ents[i].id;
                let vid = w.next_serial();
                w.scene.visuals.push(Visual {
                    kind: VisualKind::Emote(*emote),
                    x,
                    y: y - 32,
                    owner,
                    t: 0,
                    life: emote_len(*emote),
                    vid,
                });
                // the emote object's tiles (`0x08016304`, vtable 0x0804A298: 0x40)
                crate::vram::acquire(w, crate::vram::Owner::Visual(vid), 0x40);
                let stamp = w.next_id;
                w.scene.emote_stamps.push((vid, stamp));
            }
        }
        Cmd::Beam { kind, dir, t, .. } => {
            *t = 0;
            if let Some(i) = i {
                if let Some(a) = actor_mut(w, i) {
                    a.facing = (*dir & 3) as Dir;
                }
                set_pose(w, i, beam_anim(*kind), 0);
                if matches!(kind, BeamKind::Beam | BeamKind::BeamAlt | BeamKind::Blast) {
                    // the beam object 0x080206B8 charging: the charge effect (table 0x0877B200[dir]: charge
                    // position, record 55/56) and sound 3
                    w.sfx(3, 64);
                    let owner = w.ents[i].id;
                    // the beam object is made here (start `0x0803C1E0` -> `0x080206B8`: its 0x300 tile bytes at once;
                    // segment sb 6-6 f5770, fire_beam_alt); the shot at t 24 is its firing (shots.rs takes the block)
                    crate::vram::release(w, crate::vram::Owner::BeamObj(owner));
                    crate::vram::acquire(w, crate::vram::Owner::BeamObj(owner), 0x300);
                    crate::fx::beam_charge(w, owner, BEAM_CHARGE, crate::fx::BeamSet::Other);
                }
            }
        }
        Cmd::Shoot {
            kind,
            x,
            y,
            speed,
            t,
            n,
        } => {
            *t = 0;
            if let Some(i) = i {
                let (cx, cy) = muzzle(w, i);
                match kind {
                    ShootKind::Blast => {
                        // the shoot state (raw 4) runs by itself; the blast flies to the point
                        set_state(w, i, 3);
                        // the ki blast object (0x08027CBC): its sound is 20 + rand(2) (story segment sb_01_03, s025)
                        let snd = 20 + w.rng.rand(2);
                        w.sfx(snd, 64);
                        let (nn, _, _) = step_math(*x - cx, *y - cy, *speed, 8);
                        *n = nn;
                        // its object tiles (base ctor `0x08019B04`: 0x100) while it flies
                        let id = w.ents[i].id;
                        crate::vram::acquire(w, crate::vram::Owner::Blast(id), 0x100);
                    }
                    ShootKind::Ball => {
                        // start 0x0803C4DC: SetAnimRaw(0x14); the throw direction k (5-way, from the footprint
                        // centre to the point) is kept in `speed`, the cel is 2k + 1 with the tick at 0; `n` = the
                        // ball's frames left (none yet)
                        set_state(w, i, 19);
                        let (fx, fy) = w.actor(i).map(|a| a.foot_center()).unwrap_or((0, 0));
                        let (dx, dy) = (*x - fx, *y - fy);
                        let k = if 2 * dx < -5 * dy {
                            0
                        } else if 12 * dx < -5 * dy {
                            1
                        } else if 2 * dx > 5 * dy {
                            4
                        } else if 12 * dx > 5 * dy {
                            3
                        } else {
                            2
                        };
                        *speed = k;
                        *n = 0;
                        ball_cel(w, i, 2 * k + 1, 0);
                    }
                }
            }
        }
        Cmd::SetSpeed { v } => {
            if let Some(i) = i {
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.speed = *v << 10;
                }
            }
        }
        Cmd::SetByte8 { v } => {
            if let Some(i) = i {
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.byte8 = *v;
                }
            }
        }
        Cmd::SetAttachment {
            kind,
            variant,
            dx,
            dy,
        } => {
            if let Some(i) = i {
                crate::vram::shadow_set(w, i, (*kind, *variant));
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.attach = if *kind == 0 && *variant == 0 {
                        None
                    } else {
                        Some([*kind, *variant, *dx, *dy])
                    };
                }
            }
        }
        Cmd::PropShow { on } => {
            if let Owner::Prop(p) = owner {
                prop_mut(w, p).visible = *on;
            }
        }
        Cmd::PropSetPos { x, y, .. } => {
            if let Owner::Prop(p) = owner {
                let pr = prop_mut(w, p);
                pr.x = *x << 8;
                pr.y = *y << 8;
            }
        }
        Cmd::PropMoveTo {
            x,
            y,
            speed,
            n,
            t,
            sx,
            sy,
        } => {
            if let Owner::Prop(p) = owner {
                let pr = prop_mut(w, p).clone();
                let (nn, ssx, ssy) = step_math(*x - (pr.x >> 8), *y - (pr.y >> 8), *speed, 8);
                (*n, *sx, *sy, *t) = (nn, ssx, ssy, 0);
            }
        }
        Cmd::PropJumpTo { x, y, t, vx, vy } => {
            // 0x0804DD08: vx = dx * 0x5D3 >> 8, vy0 = (dy * 0x5D3 - 0xBA60 >> 8) - 0x200 (1/256 px), gravity 0x20
            if let Owner::Prop(p) = owner {
                let pr = prop_mut(w, p).clone();
                *vx = ((*x - (pr.x >> 8)) * 0x5D3) >> 8;
                *vy = (((*y - (pr.y >> 8)) * 0x5D3 - 0xBA60) >> 8) - 0x200;
                *t = 0;
                prop_mut(w, p).vy = *vy;
            }
        }
        Cmd::PropPoof { t, .. } => *t = 0,
        Cmd::CamFollowWait { target, .. } => follow(w, *target),
        Cmd::CamReleaseWait { .. } => {
            w.scene.cam.mode = CamMode::Fixed;
        }
        Cmd::CamPanTo {
            x,
            y,
            speed,
            n,
            t,
            sx,
            sy,
        } => {
            camera_mode_set(w);
            w.scene.cam.mode = CamMode::Fixed;
            let c = &w.cameras[0];
            let (cx, cy) = (c.x >> 10, c.y >> 10);
            // the step count and the step come from the target as given; the camera's limits hold the position on
            // the way and at the end (Diablo Desert Express 11-17: camera_pan_to(280, 32) from (0, 88) steps (501,
            // -100) / 256 - x stops at the map's 240 while y goes on to 32)
            let _ = clamp_cam;
            let (tx, ty) = (*x, *y);
            let (nn, ssx, ssy) = step_math(tx - cx, ty - cy, *speed, 8);
            (*n, *sx, *sy, *t) = (nn, ssx << 2, ssy << 2, 0);
            *x = tx;
            *y = ty;
            // the pan starts from the whole-pixel position (story playthrough s103: 323.98 -> 314.0 -> 305 at 9 px)
            cam_put(w, cx << 10, cy << 10);
            // the pan's first step is applied at once
            cam_step(w, *sx, *sy);
            *t = 1;
        }
        Cmd::CamParam { which, v } => {
            w.scene.cam.mode = CamMode::Fixed;
            if *which == 8 {
                w.scene.cam.param8 = *v;
            } else {
                w.scene.cam.param12 = *v;
            }
        }
        Cmd::CamSet { x, y, z } => {
            w.scene.cam.mode = CamMode::Fixed;
            if let Some(z) = z {
                w.scene.cam.param8 = *z;
            }
            crate::battle::set_camera(w, 0, *x << 10, *y << 10);
        }
        Cmd::Wait { .. } => {}
    }
}

/// the actor state a beam command holds: 17 (raw 19) for the Kamehameha / wave beam, 3 (raw 4) for the others
/// the beam object's charge position per facing (0x0877B200)
const BEAM_CHARGE: [(i32, i32); 4] = [(8, -2), (12, -2), (12, -2), (4, -2)];

/// remove the actor's beam charge effect; true when there was one
fn remove_charge(w: &mut World, owner: crate::world::EntityId) -> bool {
    let mut any = false;
    for e in w.ents.iter_mut() {
        if !e.removed
            && matches!(&e.body, Body::Fx(f) if matches!(**f, crate::fx::Fx::BeamCharge { owner: o, .. } if o == owner))
        {
            e.removed = true;
            any = true;
        }
    }
    any
}

fn beam_anim(kind: BeamKind) -> i32 {
    match kind {
        BeamKind::Wave | BeamKind::Beam => 17,
        BeamKind::BeamAlt | BeamKind::Blast => 3,
    }
}

/// one update of the head; true = done
/// fire_ball_at drives the thrower's cel and tick itself (actor +0x11 / +0x12): a frozen cel of the throw state
fn ball_cel(w: &mut World, i: usize, cel: i32, tick: i32) {
    if matches!(w.ents[i].body, Body::Player(_)) {
        return;
    }
    if let Some(m) = anim_mut(w, i) {
        m.mode = AnimMode::Pose;
        m.frame = cel;
        m.tick = tick;
    }
}

fn update_cmd(w: &mut World, owner: Owner, cmd: &mut Cmd) -> bool {
    let i = owner_index(w, owner);
    match cmd {
        Cmd::Wait { left } | Cmd::CamFollowWait { left, .. } | Cmd::CamReleaseWait { left } => {
            *left -= 1;
            *left == 0
        }
        Cmd::PlayAnim { anim } => {
            if *anim == 0 {
                return true;
            }
            match i {
                Some(i) => anim_done(w, i),
                None => true,
            }
        }
        Cmd::PlayAnimFrames { wait, .. } => {
            if !*wait {
                return true;
            }
            match i {
                Some(i) => scene_anim(w, i).is_none_or(|a| a.ended),
                None => true,
            }
        }
        Cmd::FaceActor { target } => {
            face_toward(w, i, *target);
            true
        }
        Cmd::MoveTo {
            x, y, n, t, sx, sy, ..
        } => {
            let Some(i) = i else { return true };
            *t += 1;
            if let Some(a) = actor_mut(w, i) {
                if *t >= *n {
                    a.x = *x << 8;
                    a.y = *y << 8;
                } else {
                    a.x += *sx;
                    a.y += *sy;
                }
            }
            if *t >= *n {
                set_state(w, i, 0);
                crate::fieldnpc::script_move_mark(w, i, true);
                return true;
            }
            false
        }
        Cmd::GlideTo {
            x, y, n, t, sx, sy, ..
        } => {
            let Some(i) = i else { return true };
            *t += 1;
            if let Some(a) = actor_mut(w, i) {
                if *t >= *n {
                    a.x = *x << 8;
                    a.y = *y << 8;
                } else {
                    a.x += *sx;
                    a.y += *sy;
                }
            }
            *t >= *n
        }
        Cmd::JumpTo { air, t, sx, vy, .. } => {
            let Some(i) = i else { return true };
            *t += 1;
            let a = (*air).max(1);
            if *t >= 8 && *t < 8 + a {
                *vy += 8;
                let v = *vy;
                if let Some(ac) = actor_mut(w, i) {
                    if *t == 8 {
                        // the flight runs from the whole-pixel start (the fraction is dropped)
                        ac.x = (ac.x >> 8) << 8;
                        ac.y = (ac.y >> 8) << 8;
                    }
                    ac.x += *sx;
                    ac.y += v;
                }
                set_pose(w, i, 14, if *t - 8 < a / 2 { 1 } else { 2 });
            }
            if *t == 8 + a {
                set_pose(w, i, 14, 0);
            }
            if *t >= a + 16 {
                set_state(w, i, 0);
                return true;
            }
            false
        }
        Cmd::ScreenFlash { t, frames, .. } => {
            *t += 1;
            *t >= (*frames).max(2)
        }
        Cmd::Shake { t, frames, .. } => {
            *t += 1;
            *t >= *frames
        }
        Cmd::ItemGet { t, .. } => {
            *t += 1;
            if *t >= 60 {
                if let Some(i) = i {
                    // a player in Instant Transmission's field run goes back to the run's state (s099)
                    let it =
                        matches!(w.ents[i].body, Body::Player(_)) && crate::player::resume_it(w, i);
                    if !it {
                        set_state(w, i, 0);
                    }
                    if let Body::Npc(n) = &mut w.ents[i].body {
                        n.hold_item = None;
                    }
                }
                return true;
            }
            false
        }
        Cmd::Emote { wait, t, emote } => {
            if !*wait {
                return true;
            }
            *t += 1;
            *t >= emote_len(*emote)
        }
        Cmd::Beam {
            frames, t, kind, ..
        } => {
            *t += 1;
            let kind = *kind;
            if kind == BeamKind::Wave {
                if let Some(i) = i {
                    if *t < *frames {
                        // 8 ticks per cel, held on cel 3 (or the sprite's last; s025: sprites 43 / 42)
                        let (sp, dir) = w
                            .actor(i)
                            .map(|a| (a.sprite, a.facing))
                            .unwrap_or((None, 0));
                        let n = ActAnim::of(17).cels(&w.data, sp, dir).max(1);
                        set_pose(w, i, 17, (*t >> 3).min(3).min(n - 1));
                    }
                    if *t == 16 {
                        let (x, y) = actor_pos(w, i);
                        let owner = w.ents[i].id;
                        let dir = w.actor(i).map(|a| a.facing as i32).unwrap_or(0);
                        w.scene.visuals.push(Visual {
                            kind: VisualKind::Beam { kind, dir },
                            x,
                            y,
                            owner,
                            t: 0,
                            life: (*frames - 16).max(1),
                            vid: 0,
                        });
                        // the beam object (`0x080286D4` from `0x0803C494`, 0xC0 tile bytes) lives the command's
                        // frame count (segment sb 1-3 f6210 / f6246: s025's two fire_wave_beam(.., 35))
                        crate::vram::acquire(w, crate::vram::Owner::Blast(owner), 0xC0);
                        crate::vram::release_later(
                            w,
                            crate::vram::Owner::Blast(owner),
                            (*frames).max(1) as u32,
                        );
                    }
                }
            } else if *t == 24 {
                // 0x080207D2: the beam fires at the fire position (table +0x30: (8,16) (8,-16) (-16,-6) (32,-6)); it
                // grows 3 px per tick while its head is on screen and retracts 3 px per tick after the command
                // (destructor: stop 32, sound 33) - the player's beam class (shots::KameBeam)
                if let Some(i) = i {
                    set_pose(
                        w,
                        i,
                        beam_anim(kind),
                        if kind == BeamKind::Beam { 2 } else { 1 },
                    );
                    let owner = w.ents[i].id;
                    remove_charge(w, owner);
                    let a = w.actor(i).expect("actor");
                    let dir = a.facing;
                    let (fx0, fy0) = crate::shots::BEAM_POS_KAME_FIRE[dir as usize];
                    let (x, y) = (a.px() + fx0, a.py() + fy0);
                    let mut shot = crate::shots::kame_beam(
                        owner,
                        dir,
                        x,
                        y,
                        w.frame as i64 - 23,
                        crate::damage::atk(5),
                        328,
                        true,
                        "beam_kamehameha_1",
                    );
                    // the beam's hit message takes its sender class from the owner's +0x1D0: a map NPC's object has
                    // no such field there (Piccolo's reads 0x030032EC), and the enemies take classes 1 and 2 only -
                    // an NPC's scripted beam hurts nobody (VERIFIED segment sb 6-6 f5795: s228's fire_beam_alt)
                    if matches!(w.ents[i].body, Body::Npc(_)) {
                        shot.team = crate::shots::TEAM_NONE;
                    }
                    if let crate::shots::ShotKind::Kame(k) = &mut shot.kind {
                        k.sounds = true;
                    }
                    crate::shots::add(w, shot);
                }
                w.sfx_stop(3);
                w.sfx(32, 64);
            } else if *t > 24 && *t < *frames && kind == BeamKind::Beam {
                // firing: the pose alternates cels 2 / 1 every 8 ticks
                if let Some(i) = i {
                    set_pose(w, i, beam_anim(kind), 2 - (((*t - 24) >> 3) & 1));
                }
            }
            if *t >= *frames {
                if let Some(i) = i {
                    // 0x0802082A: still charging -> the charge goes (stop 3); firing -> the beam retracts
                    let owner = w.ents[i].id;
                    if remove_charge(w, owner) {
                        w.sfx_stop(3);
                    }
                    let shots: Vec<usize> = (0..w.ents.len())
                        .filter(|&k| !w.ents[k].removed && matches!(&w.ents[k].body, Body::Shot(s) if s.owner == owner && matches!(s.kind, crate::shots::ShotKind::Kame(_))))
                        .collect();
                    for k in shots {
                        crate::shots::beam_stop(w, k);
                    }
                    set_state(w, i, 0);
                }
                return true;
            }
            false
        }
        // fire_ball_at's tick 0x0803C576 / done 0x0803C65C: the ball (0x0802E444) leaves at tick 8 from the position
        // + 0x0877C454[k] and lives (max(|dx|, |dy|) >> 1) + 1 ticks; cel 2k + 2 from tick 8, 2k + 1 again from 16;
        // done from tick 24 on once the ball is gone (8-16 s319: Dabura's spits, 57 and 41 ticks)
        Cmd::Shoot {
            kind: ShootKind::Ball,
            x,
            y,
            speed,
            t,
            n,
        } => {
            const OFF: [(i32, i32); 5] = [(0, -32), (0, -32), (8, -32), (16, -32), (16, -32)];
            *t += 1;
            let k = (*speed).clamp(0, 4);
            if *n > 0 {
                *n -= 1;
                // the ball's tiles go with it
                if *n == 0 {
                    if let Some(i) = i {
                        let id = w.ents[i].id;
                        crate::vram::release(w, crate::vram::Owner::Blast(id));
                    }
                }
            }
            if let Some(i) = i {
                if *t == 8 {
                    let (px, py) = actor_pos(w, i);
                    let (sx, sy) = (px + OFF[k as usize].0, py + OFF[k as usize].1);
                    *n = (((*x - sx).abs().max((*y - sy).abs())) >> 1) + 1;
                    // the ball object's tiles (projectile base ctor `0x08019B04`: 0x100; emulator, tour_castle_forest_16
                    // f5234: Dabura's spit)
                    let id = w.ents[i].id;
                    crate::vram::acquire(w, crate::vram::Owner::Blast(id), 0x100);
                }
                let cel = if (8..16).contains(t) {
                    2 * k + 2
                } else {
                    2 * k + 1
                };
                ball_cel(w, i, cel, *t);
            }
            *t >= 24 && *n == 0
        }
        Cmd::Shoot { t, n, .. } => {
            *t += 1;
            let done = *t >= *n;
            if done {
                if let Owner::Entity(id) = owner {
                    // the blast object's end effect plays 16 more frames
                    crate::vram::release_later(w, crate::vram::Owner::Blast(id), 16);
                }
            }
            done
        }
        Cmd::PropMoveTo {
            x, y, n, t, sx, sy, ..
        } => {
            if let Owner::Prop(p) = owner {
                *t += 1;
                let pr = prop_mut(w, p);
                if *t >= *n {
                    pr.x = *x << 8;
                    pr.y = *y << 8;
                    return true;
                }
                pr.x += *sx;
                pr.y += *sy;
                return false;
            }
            true
        }
        Cmd::PropJumpTo { x, y, t, vx, .. } => {
            *t += 1;
            if let Owner::Prop(p) = owner {
                let pr = prop_mut(w, p);
                if *t >= 44 {
                    pr.x = *x << 8;
                    pr.y = *y << 8;
                    return true;
                }
                pr.x += *vx;
                pr.y += pr.vy;
                pr.vy += 0x20;
                return false;
            }
            *t >= 44
        }
        Cmd::PropPoof { vanish, t } => {
            *t += 1;
            let steps = if *vanish { 8 } else { 5 };
            if *t >= steps * 8 {
                if *vanish {
                    if let Owner::Prop(p) = owner {
                        prop_mut(w, p).visible = false;
                    }
                }
                return true;
            }
            false
        }
        Cmd::CamPanTo {
            x, y, n, t, sx, sy, ..
        } => {
            if *t >= *n {
                return true;
            }
            w.scene.cam.pan_pre = Some(w.cameras[0].rect);
            *t += 1;
            if *t >= *n {
                cam_put(w, *x << 10, *y << 10);
            } else {
                cam_step(w, *sx, *sy);
            }
            false
        }
        _ => true,
    }
}

/// the command's vt+0x10 (0x080193AE: the last command of a track that finished asks it): 1 = the track is idle at
/// once, the command stays as its head. face_actor, and the instant set_pos / prop_set_pos / camera_set* /
/// camera_set_param* (vt+0x10 = `mov r0, #1`: 0x0804ED2D / 35 / 39 / 3D / 31 / 41; sc_janemba: Gogeta's split
/// face -> set_pos ends the wait in set_pos's tick); the others return 0 (pop, idle on the next tick)
fn hold_cmd(cmd: &Cmd) -> bool {
    matches!(
        cmd,
        Cmd::FaceActor { .. }
            | Cmd::SetPos { .. }
            | Cmd::CamSet { .. }
            | Cmd::CamParam { .. }
            | Cmd::PropSetPos { .. }
    )
}

/// FaceActor's update (0x0801599D): the dominant axis of the 24.8 position difference, ties horizontal
fn face_toward(w: &mut World, i: Option<usize>, target: EntityId) {
    let (Some(i), Some(ti)) = (i, w.find(target)) else {
        return;
    };
    let (Some(a), Some(b)) = (w.actor(i), w.actor(ti)) else {
        return;
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dir = if dx.abs() >= dy.abs() {
        if dx > 0 {
            Some(3)
        } else if dx < 0 {
            Some(2)
        } else {
            None
        }
    } else if dy > 0 {
        Some(0)
    } else {
        Some(1)
    };
    if let (Some(d), Some(a)) = (dir, actor_mut(w, i)) {
        a.facing = d;
    }
}

/// emote balloon length (0x08016304; `emote_wait` measured 24 + 1 frames for emote 2)
pub const EMOTE_LEN: i32 = 24;

/// the balloon's length per emote: its graphics size (table 0x086B8128: u32 count, u32 bytes) / 8 frames (emote 2:
/// 0xC0 -> 24, measured; emote 4: 0x180 -> 48, measured in the story playthrough, Yemma's s048)
pub fn emote_len(emote: i32) -> i32 {
    const BYTES: [i32; 5] = [0x140, 0x100, 0xC0, 0x100, 0x180];
    BYTES
        .get(emote as usize)
        .map(|b| b / 8)
        .unwrap_or(EMOTE_LEN)
}

/// a player playing a frame range (`play_anim_frames`): the scene copy ticks in the player's update and is shown
/// as a pose of the player's own state (called by player.rs after its state tick)
pub fn player_frames_tick(w: &mut World, i: usize) {
    let id = w.ents[i].id;
    let Some(k) = w.scene.puppets.iter().position(|p| p.id == id) else {
        return;
    };
    let p = w.scene.puppets[k].clone();
    if !matches!(p.anim.mode, AnimMode::Frames { .. }) {
        return;
    }
    let (s, f) = w
        .actor(i)
        .map(|a| (a.sprite, a.facing))
        .unwrap_or((None, 0));
    let d = w.data.clone();
    let mut a = p.anim.clone();
    let out = a.tick(&d, s, f);
    w.scene.puppets[k].anim = a.clone();
    if out == AnimOut::ToIdle {
        w.scene.puppets[k].anim = ActAnim::idle();
        crate::player::script_set_anim(w, i, 0);
    } else {
        crate::player::script_pose(w, i, a.id.clamp(0, 24) as usize, a.frame);
    }
}

/// nearest of the 20 tint presets (table 0x08752EEC: RGBA keys); (0,0,0,0)-like keys restore
pub fn tint_preset(r: i32, g: i32, b: i32, a: i32) -> Option<u8> {
    // table 0x08752EEC: 20 entries {RGBA key, palette}; the first nearest key (squared distance) wins; a key
    // without a palette (alpha 0 rows) restores the sprite's own colours. The index is the entry (the view's
    // palette `tint_<index>`).
    const KEYS: [(i32, i32, i32, i32, bool); 20] = [
        (0, 0, 0, 0, false),
        (0, 0, 0, 64, true),
        (0, 0, 0, 128, true),
        (0, 0, 0, 192, true),
        (255, 0, 0, 0, false),
        (255, 0, 0, 64, true),
        (255, 0, 0, 128, true),
        (255, 0, 0, 192, true),
        (255, 255, 255, 0, false),
        (255, 255, 255, 32, true),
        (255, 255, 255, 64, true),
        (255, 255, 255, 96, true),
        (255, 255, 255, 128, true),
        (255, 255, 255, 192, true),
        (255, 255, 255, 255, true),
        (128, 255, 128, 0, false),
        (128, 255, 128, 32, true),
        (128, 255, 128, 64, true),
        (128, 255, 128, 96, true),
        (128, 255, 128, 128, true),
    ];
    let (best, k) = KEYS
        .iter()
        .enumerate()
        .min_by_key(|(_, k)| {
            let d = |p: i32, q: i32| (p - q) * (p - q);
            d(r, k.0) + d(g, k.1) + d(b, k.2) + d(a, k.3)
        })
        .expect("table");
    k.4.then_some(best as u8)
}

/// the script's handle on map prop `id` of the current area (seeded from `Field.props`: position = the rect's top-left)
fn prop_mut(w: &mut World, id: i32) -> &mut Prop {
    let load = w.field.as_ref().map(|f| f.loads).unwrap_or(0);
    w.scene.props.retain(|p| p.load == load);
    if let Some(k) = w.scene.props.iter().position(|p| p.id == id) {
        return &mut w.scene.props[k];
    }
    let (x, y, visible) = w
        .field
        .as_ref()
        .and_then(|f| f.props.get(id.max(0) as usize))
        .map(|p| (p.rect.x0 << 8, p.rect.y0 << 8, p.visible))
        .unwrap_or((0, 0, true));
    w.scene.props.push(Prop {
        id,
        x,
        y,
        visible,
        frame: 0,
        vy: 0,
        load,
    });
    w.scene.props.last_mut().unwrap()
}

/// the script props onto the area's map props (drawn and solid from `Field.props`): visibility, and the rect moved
/// by the position change
pub fn sync_props(w: &mut World) {
    let Some(f) = w.field.as_mut() else { return };
    for p in &w.scene.props {
        if p.load != f.loads {
            continue;
        }
        let Some(r) = f.props.get_mut(p.id.max(0) as usize) else {
            continue;
        };
        r.visible = p.visible;
        let (dx, dy) = ((p.x >> 8) - r.rect.x0, (p.y >> 8) - r.rect.y0);
        r.rect = Rect::new(
            r.rect.x0 + dx,
            r.rect.y0 + dy,
            r.rect.x1 + dx,
            r.rect.y1 + dy,
        );
    }
}

/// `become_map_object`: the field layer spawns area object `index` at the stand-in's place
fn become_map_object(w: &mut World, index: i32, x: i32, y: i32, dir: Dir) -> Option<EntityId> {
    w.field.as_ref()?;
    // area +0x14: the actor list (list 1)
    let at = crate::fieldcontent::ObjRef::List(1, index as u16);
    let id = crate::field::spawn_map_object(w, at, true)?;
    if let Some(i) = w.find(id) {
        if let Some(a) = actor_mut(w, i) {
            a.x = x << 8;
            a.y = y << 8;
            a.facing = dir;
        }
    }
    Some(id)
}

// ---------------------------------------------------------------------------------------------------------------
// per frame

/// the frame's music events (any source: area loads, scripts) keep `Scene.song` current; called at the end of the
/// frame (`Session::step_post`) and after a session is created
pub fn note_events(w: &mut World) {
    banner_step(w);
    for e in &w.events {
        match e {
            crate::world::Event::Song { id } => w.scene.song = *id,
            crate::world::Event::SongStop => w.scene.song = -1,
            _ => {}
        }
    }
}

/// the script an enemy's AI node started (`Cause::Ai`) is over: its node completes, and the actors it commanded
/// go back to their own updates (no event ends for an AI script, so nothing else releases them)
pub fn ai_script_done(w: &mut World, entity: EntityId) {
    w.scene.ai_wait.retain(|&e| e != entity);
    if w.scene.depth == 0 {
        release_puppets(w);
    }
}

/// a cutscene scene load: the player objects leave the world (kept here unchanged)
pub fn send_offstage(w: &mut World) {
    let players = w.players.clone();
    let mut k = 0;
    while k < w.ents.len() {
        if players.contains(&w.ents[k].id) && matches!(w.ents[k].body, Body::Player(_)) {
            let e = w.ents.remove(k);
            w.scene.offstage.push(e);
        } else {
            k += 1;
        }
    }
}

/// the players come back (before the field area they return to is loaded)
pub fn bring_back(w: &mut World) {
    let back = std::mem::take(&mut w.scene.offstage);
    w.ents.extend(back);
}

/// the area banner after a frame (docs/frontend.md §7): the shown one ages and goes after t = 238; a load deletes
/// it, and starts one when the new area's name id differs from the last one shown and is not 0 (the 3D world map
/// only records the id). Called with the music bookkeeping at the end of the frame / after a session is created.
pub fn banner_step(w: &mut World) {
    // the banner a load of this frame made shows from now (t = 0)
    // (once: a frame that does not advance the world's frame count still ages it; segment sb 19-3)
    if w.scene.banner.is_some() && w.scene.banner_made == w.frame {
        w.scene.banner_made = u32::MAX;
        return;
    }
    if let Some((_, t)) = w.scene.banner.as_mut() {
        *t += 1;
        if *t > 238 {
            w.scene.banner = None;
            crate::vram::banner(w, false);
        }
    }
}

/// a load's banner (the arrival, after the HUD object: segment sb 4-21): the old one went with the old area's
/// objects; the id is recorded when it differs from the last one (also 0), and a new banner starts when it is not 0 (the 3D world map only records the id; a cutscene's backdrop load has no banner
/// object: segment sb 4-24)
pub fn banner_load(w: &mut World, a: &crate::fieldcontent::AreaDef, three_d: bool, scene: bool) {
    if scene {
        return;
    }
    w.scene.banner = None;
    if three_d {
        w.scene.banner_last = a.name_id;
    } else if a.name_id != w.scene.banner_last {
        // (`0x08008E4E`: a different id is stored even when it is 0 - an unnamed map between two visits of the same
        // area brings its banner back; tour_kais_pad191 f156)
        w.scene.banner_last = a.name_id;
        if a.name_id == 0 {
            return;
        }
        w.scene.banner = Some((a.banner.clone(), 0));
        w.scene.banner_made = w.frame;
        crate::vram::banner(w, true);
    }
}

/// `play_music`: start a song unless it is already playing
pub fn play_song(w: &mut World, id: i32) {
    let playing = w.scene.song == id
        || w.events.iter().rev().find_map(|e| match e {
            crate::world::Event::Song { id: s } => Some(*s == id),
            crate::world::Event::SongStop => Some(false),
            _ => None,
        }) == Some(true);
    if !playing {
        w.events.push(crate::world::Event::Song { id });
        w.scene.song = id;
    }
}

/// the BG sphere effect's draws of a frame (also under the power gauge minigame, whose loop runs the world update:
/// Kid Buu's arena 21-25, the Spirit Bomb's push under s-script's `power_minigame`)
pub fn sphere_jitter(w: &mut World) {
    let jitter: i32 = w
        .scene
        .visuals
        .iter()
        .filter(|v| v.kind == VisualKind::EnergySphere)
        .map(|v| (v.owner as i32 - 3).max(0))
        .sum();
    w.scene.sphere_jitter.clear();
    for _ in 0..jitter {
        let r = w.rng.rand(4) as u8;
        w.scene.sphere_jitter.push(r);
    }
}

/// start of the frame: keep the raw input for the scripts; players get none while an event graph runs, a modal
/// fade / dialog / menu is up, or the director waits
pub fn pre_step(w: &mut World) {
    // the BG sphere effect (`0x0803AA58`) redraws every frame with jittered rows, before the objects update: rand(4)
    // for each row past the third (radius - 3 draws; VERIFIED segment sb 20-8, Kid Buu's planet burst)
    sphere_jitter(w);
    w.scene.keys = w.input;
    w.scene.cam.stepped = false;
    w.scene.cam.pan_pre = None;
    if w.scene.depth == 0 && !w.scene.puppets.is_empty() && w.scene.director.wait.is_none() {
        // outside a scene a player's input ends its leftover frame-range animation
        let held: Vec<(EntityId, bool)> = w
            .players
            .iter()
            .enumerate()
            .map(|(s, &id)| (id, w.input[s.min(MAX_PLAYERS - 1)].held != 0))
            .collect();
        w.scene
            .puppets
            .retain(|p| !held.iter().any(|&(id, h)| id == p.id && h));
    }
    if w.scene.depth > 0 || w.scene.menu.is_some() {
        // the world map's flyer reads the keys itself during an event (s515's arrival fade-in)
        let flyer = w.scene.menu.is_none() && crate::fieldworld::flying(w);
        // a warp_fade's arrival too, through its fade-in (`field::free_arrival`: 0-21, s026)
        let free = w.scene.menu.is_none() && crate::field::free_arrival(w);
        for s in 0..w.players.len().min(MAX_PLAYERS) {
            if !(flyer && s == 0) && !free {
                w.input[s] = InputFrame::default();
            }
        }
    }
}

/// after the entities: screen effects, dialog, menus, visuals, then the command tracks of the running wait mode
pub fn tick(w: &mut World) {
    tick_screen(w);
    tick_dialog(w);
    tick_menu(w);
    tick_visuals(w);
    let blocked = w.scene.director.wait.is_some();
    tick_tracks(w);
    for t in w.scene.tracks.iter_mut() {
        t.ticked = None;
    }
    // An AI InlineScript's parallel block ended: the game runs the script's nested frame loop inside that enemy's
    // update, and when the block ends the update goes on in the same frame - the node returns 1 and the rest of the
    // enemy's AI runs (Earth 4-25 f6646: Vegeta's 0x0870EA28 ends with face_actor / his own jump_to, then
    // GotoState(0x310) and StandStill's idle draw in that frame). The host ends the script after the objects.
    if blocked && w.scene.director.wait.is_none() && w.field.is_some() && w.scene.depth == 0 {
        for id in w.scene.ai_wait.clone() {
            let Some(i) = w
                .find(id)
                .filter(|&i| !w.ents[i].removed && matches!(w.ents[i].body, Body::Enemy(_)))
            else {
                continue;
            };
            ai_script_done(w, id);
            w.scene.puppets.retain(|p| p.id != id);
            crate::enemy::release_update(w, i);
        }
    }
    sync_props(w);
    // dropped owners
    let gone: Vec<usize> = (0..w.scene.tracks.len())
        .filter(|&k| {
            matches!(w.scene.tracks[k].owner, Owner::Entity(id) if !w.alive(id))
                && w.scene.tracks[k].queue.is_empty()
        })
        .collect();
    for k in gone.into_iter().rev() {
        w.scene.tracks.remove(k);
    }
    let live: Vec<bool> = w
        .scene
        .puppets
        .iter()
        .map(|p| w.find(p.id).is_some())
        .collect();
    let mut it = live.into_iter();
    w.scene.puppets.retain(|_| it.next().unwrap_or(false));
}

/// The wait-all mode's actor tick (message 0x0C, 0x08006C18 -> 0x08011C0C to each object of the world list, last
/// object first): an actor's handler updates its animation and then ticks its own command list (0x0800F5C8: the
/// anim components' vt+4, then 0x080193AE), so a command that ends with the animation advances and starts the next
/// one before the next object's update. Run 2 19-3 row 1,877,946: Goku and Kid Buu end their punches in one frame -
/// the game draws Kid Buu's idle rand(255), his next punch's 50 + rand(2), then Goku's idle and punch; ours had
/// both idles in the object pass and both punches in the track tick after it (same draws, the punch sounds swapped).
/// Called after an actor's update in the object pass; the other commands keep the track tick after the objects.
pub fn track_after_update(w: &mut World, id: EntityId) {
    if !matches!(w.scene.director.wait, Some(WaitMode::All)) {
        return;
    }
    let owner = Owner::Entity(id);
    let Some(k) = w.scene.tracks.iter().position(|t| t.owner == owner) else {
        return;
    };
    let t = &w.scene.tracks[k];
    if !t.started || t.holding || t.ticked.is_some() {
        return;
    }
    let Some(mut cmd) = t.queue.first().cloned() else {
        return;
    };
    // only an animation command next (its start touches the actor alone: a state, its idle draw / swing sound); a
    // next command that makes an object (an emote's bubble, an effect) keeps the tick after the objects - ours
    // updates those objects in passes of their own (sb_005 1-5 f17209: four actors' emotes)
    if !matches!(
        t.queue.get(1),
        Some(Cmd::PlayAnim { .. } | Cmd::SetAnim { .. } | Cmd::SetPose { .. } | Cmd::PlayAnimFrames { .. })
    ) {
        return;
    }
    if !matches!(cmd, Cmd::PlayAnim { .. } | Cmd::PlayAnimFrames { .. })
        || !update_cmd(w, owner, &mut cmd)
    {
        return;
    }
    let idle = tick_track(w, k);
    if let Some(t) = w.scene.tracks.iter_mut().find(|t| t.owner == owner) {
        t.ticked = Some(idle);
    }
}

fn tick_tracks(w: &mut World) {
    let Some(mode) = w.scene.director.wait.clone() else {
        return;
    };
    let mut all_idle = true;
    // the actors' tracks tick in the world list's update order (last object first), the camera track first (story
    // playthrough s103: three NPCs ending their walks in one frame draw their idle ticks newest object first).
    // A group wait (`wait_for_actor`) ticks its members from the director's table (`0x08006CE0`: msg 0x0C to each
    // member in the world list's order, first object first - dbx_004_6-1 f18,270: the player, then Goten)
    let key = |w: &World, o: Owner| -> i64 {
        match o {
            Owner::Entity(id) if matches!(&mode, WaitMode::Group(g) if g.contains(&o)) => {
                w.find(id).map(|i| i as i64).unwrap_or(-1)
            }
            Owner::Entity(id) => -(w.find(id).map(|i| i as i64).unwrap_or(-1)),
            _ => i64::MIN,
        }
    };
    let mut owners: Vec<Owner> = w.scene.tracks.iter().map(|t| t.owner).collect();
    owners.sort_by_key(|&o| key(w, o));
    for o in owners {
        let Some(k) = w.scene.tracks.iter().position(|t| t.owner == o) else {
            continue;
        };
        if !in_mode(&mode, o) {
            continue;
        }
        if let Some(idle) = w.scene.tracks[k].ticked.take() {
            if !idle {
                all_idle = false;
            }
            continue;
        }
        if !tick_track(w, k) {
            all_idle = false;
        }
    }
    if all_idle {
        for t in w.scene.tracks.iter_mut() {
            t.live = false;
        }
        w.scene.director.done_group = matches!(mode, WaitMode::Group(_));
        w.scene.director.wait = None;
        w.scene.director.done = true;
        if let Some(prev) = w.scene.director.prev_modal.take() {
            w.modal = prev;
        }
    }
}

/// the actor message 0x0C tick (0x080193AE): true = idle
fn tick_track(w: &mut World, k: usize) -> bool {
    if w.scene.tracks[k].queue.is_empty() {
        return true;
    }
    let owner = w.scene.tracks[k].owner;
    // commands of an actor that no longer exists are dropped (the game deletes them)
    if let Owner::Entity(id) = owner {
        if !w.alive(id) {
            w.scene.tracks[k].queue.clear();
            return true;
        }
    }
    if !w.scene.tracks[k].started {
        start_head(w, k);
        // a head started by the tick is not updated in it (s101: the face of a block's second command)
        return w.scene.tracks.get(k).is_none_or(|t| t.queue.is_empty());
    }
    // a held command (face_actor left at the head) no longer updates: the actor keeps the facing it got
    // (story playthrough: s084, Goku does not follow Pikkon's later walk)
    if w.scene.tracks[k].holding {
        if w.scene.tracks[k].live {
            if let Some(Cmd::FaceActor { target }) = w.scene.tracks[k].queue.first().cloned() {
                let i = match owner {
                    Owner::Entity(id) => w.find(id),
                    _ => None,
                };
                face_toward(w, i, target);
            }
        }
        return true;
    }
    let mut cmd = w.scene.tracks[k].queue[0].clone();
    let done = update_cmd(w, owner, &mut cmd);
    // the command may have moved its track (change_sprite moves the queue to the new actor)
    let owner = if w.scene.tracks.get(k).is_some_and(|t| t.owner == owner) {
        owner
    } else {
        w.scene.tracks.get(k).map(|t| t.owner).unwrap_or(owner)
    };
    let Some(k) = w.scene.tracks.iter().position(|t| t.owner == owner) else {
        return true;
    };
    if let Some(c) = w.scene.tracks[k].queue.first_mut() {
        *c = cmd.clone();
    }
    if !done {
        return false;
    }
    if w.scene.tracks[k].queue.len() == 1 && hold_cmd(&cmd) {
        w.scene.tracks[k].holding = true;
        w.scene.tracks[k].live = true;
        return true;
    }
    w.scene.tracks[k].queue.remove(0);
    w.scene.tracks[k].started = false;
    w.scene.tracks[k].holding = false;
    if !w.scene.tracks[k].queue.is_empty() {
        w.scene.chaining = true;
        start_head(w, k);
        w.scene.chaining = false;
    }
    false
}

/// the item-get icon's tile bytes: width x height of its def (items: 0x083BD2F8 + 0x34 * id + 0xC; equipment, a
/// negative id: 0x086F82EC + 0x2C * -id + 4 - every equipment icon 16 x 16)
pub fn item_icon_bytes(item: i32) -> i32 {
    match item {
        9 => 0x200,
        34 | 43..=82 => 0x400,
        35 | 36 => 0x80,
        _ => 0x100,
    }
}

/// an emote object that ends this frame frees its tiles in its own update, at its place in the object pass: before
/// the update of the first older entity (`id`) (emulator, tour_castle_forest_16 f3500: the emote's free from
/// 0x080163B5 comes before an older effect's from 0x08051157; ours freed it in the scene's tick after the pass)
pub fn emotes_end_before(w: &mut World, id: u32) {
    if w.scene.emote_stamps.is_empty() {
        return;
    }
    let ending: Vec<u32> = w
        .scene
        .emote_stamps
        .iter()
        .filter(|s| s.1 > id && w.scene.visuals.iter().any(|v| v.vid == s.0 && v.life == 1))
        .map(|s| s.0)
        .collect();
    if ending.is_empty() {
        return;
    }
    w.scene.emote_stamps.retain(|s| !ending.contains(&s.0));
    for o in ending.into_iter().rev() {
        crate::vram::release(w, crate::vram::Owner::Visual(o));
    }
}

fn tick_visuals(w: &mut World) {
    for v in w.scene.visuals.iter_mut() {
        v.t += 1;
        if v.life > 0 {
            v.life -= 1;
        }
    }
    // a fired beam's effect object ends with its sound changing (32 -> 33)
    let ended: Vec<u32> = w
        .scene
        .visuals
        .iter()
        .filter(|v| v.life == 0 && v.vid != 0)
        .map(|v| v.vid)
        .collect();
    w.scene.visuals.retain(|v| v.life != 0);
    w.scene.emote_stamps.retain(|s| ended.iter().all(|&o| o != s.0));
    // (the objects update from the list's end: the later one goes first)
    for o in ended.into_iter().rev() {
        crate::vram::release(w, crate::vram::Owner::Visual(o));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// camera

/// the camera clamp in 22.10: field areas use their camera_max (0..=max px), battle arenas the view rect
fn cam_clamp(w: &World, x: i32, y: i32) -> (i32, i32) {
    if let Some(f) = w.field.as_ref() {
        let c = |v: i32, max: i32| {
            if v < 0 {
                0
            } else if (v >> 10) > max {
                max << 10
            } else {
                v
            }
        };
        return (c(x, f.camera_max.0), c(y, f.camera_max.1));
    }
    match w.arena.view {
        Some(v) => (
            (v.x0 << 10).max(x.min(v.x0.max(v.x1 - GBA_W) << 10)),
            (v.y0 << 10).max(y.min(v.y0.max(v.y1 - GBA_H) << 10)),
        ),
        None => (x, y),
    }
}

/// pixel clamp of a camera target (pans)
fn clamp_cam(w: &World, x: i32, y: i32) -> (i32, i32) {
    let (cx, cy) = cam_clamp(w, x << 10, y << 10);
    (cx >> 10, cy >> 10)
}

/// a scene load's camera (`load_scene*` x, y px): placed inside the new map's camera limits (7-2 s273:
/// load_scene(7, 4, 1, 1, 1) shows (1, 0))
pub fn set_scene_camera(w: &mut World, x: i32, y: i32) {
    cam_put(w, x << 10, y << 10);
    let c = &mut w.cameras[0];
    c.shown = c.rect;
    c.shown_fine = (c.x, c.y);
}

fn cam_put(w: &mut World, x: i32, y: i32) {
    let (x, y) = cam_clamp(w, x, y);
    let c = &mut w.cameras[0];
    c.x = x;
    c.y = y;
    c.init = true;
    let (cx, cy) = (x >> 10, y >> 10);
    c.rect = Rect::new(cx, cy, cx + GBA_W, cy + GBA_H);
}

fn cam_step(w: &mut World, sx: i32, sy: i32) {
    let c = &w.cameras[0];
    let (x, y) = (c.x + sx, c.y + sy);
    cam_put(w, x, y);
}

/// `camera_center_on` (0x31) / `camera_scroll_to` (0x32): the camera mover (0x08015B68) steps the top-left toward
/// (x, y) px, `speed` px per frame on the dominant axis (n = dominant / speed + 1 steps of (d << 10) / n, clamped
/// every step, no final snap); the first step happens in the frame it starts. The world is frozen meanwhile (the
/// scroll mode does not run the world update). The host blocks until [`scroll_done`].
pub fn start_scroll(w: &mut World, x: i32, y: i32, speed: i32) {
    // the mover is a camera mode (`0x08015B68` -> `0x08021094`): a running shake ends (segment sb 6-6 f11326)
    camera_mode_set(w);
    // an A-button trigger's script runs before the frame's cameras in the game: its scroll starts from the camera
    // before this frame's follow ease (field.rs `cam_before`)
    if w.scene.cam.stepped {
        if let Some((bx, by)) = w.field.as_mut().and_then(|f| f.cam_before.take()) {
            w.cameras[0].x = bx;
            w.cameras[0].y = by;
        }
    }
    let c = &w.cameras[0];
    // the mover works on the 22.10 difference; the step count comes from its whole pixels
    let (dx, dy) = ((x << 10) - c.x, (y << 10) - c.y);
    let n = (dx.abs().max(dy.abs()) >> 10) / speed.max(1) + 1;
    let (sx, sy) = (dx / n, dy / n);
    w.scene.cam.mode = CamMode::Fixed;
    w.scene.cam.scroll = Some(Scroll {
        tx: x,
        ty: y,
        sx,
        sy,
        n,
        t: 0,
    });
    w.scene.freeze = true;
    // the mover's first step belongs to the frame it starts in: a script the trigger check started (after the
    // frame's camera update) steps here (8-16's ship entrance: the camera is on the player the frame A is pressed)
    if w.scene.cam.stepped {
        scroll_step(w);
    }
}

fn scroll_step(w: &mut World) {
    let Some(s) = w.scene.cam.scroll.clone() else {
        return;
    };
    if s.t >= s.n {
        // arrived: the host's poll ends the mode (and the freeze)
        return;
    }
    cam_step(w, s.sx, s.sy);
    w.scene.cam.scroll = Some(Scroll { t: s.t + 1, ..s });
}

/// the modal scroll arrived (clears it)
pub fn scroll_done(w: &mut World) -> bool {
    match &w.scene.cam.scroll {
        Some(s) if s.t >= s.n => {
            w.scene.cam.scroll = None;
            w.scene.freeze = false;
            true
        }
        None => true,
        _ => false,
    }
}

/// the camera target of `camera_center_on`: the actor's box centre (0x0800FDFA: the footprint centre) - (120, 80)
pub fn center_on(w: &World, i: usize) -> (i32, i32) {
    let (cx, cy) = w.actor(i).map(|a| a.fc()).unwrap_or((0, 0));
    (cx - 120, cy - 80)
}

/// one step of the follow object (0x0804D300): the player camera's easing (3/64, 9/128) toward another actor
fn follow_step(w: &mut World, id: EntityId) {
    let (tx, ty) = match w.find(id).filter(|&i| !w.ents[i].removed) {
        Some(i) => {
            let a = w.actor(i).cloned().unwrap_or_default();
            let t = (
                ((a.px() * 2 + a.foot_w) >> 1) - 120,
                ((a.py() * 2 + a.foot_h) >> 1) - 84,
            );
            w.scene.cam.follow_last = Some(t);
            t
        }
        None => match w.scene.cam.follow_last {
            Some(t) => t,
            None => return,
        },
    };
    let c = &w.cameras[0];
    let x = c.x + ((((tx << 10) - c.x) * 3) >> 6);
    let y = c.y + ((((ty << 10) - c.y) * 9) >> 7);
    cam_put(w, x, y);
}

/// `camera_follow` / `camera_follow_wait`: the follow object takes the camera (it steps with the frame's camera)
/// a script camera command sets the camera object's mode (`0x08021094`: the old mode's exit, the new one's enter).
/// A screen shake is a mode pushed on that camera (`0x0800797C` -> `0x080210D6`): replaced, it ends - no more
/// offset draws (VERIFIED segment sb 6-6 f4694: Gotenks' crit shake stops at s228's camera_pan_to)
pub fn camera_mode_set(w: &mut World) {
    if !w.g.shakes.is_empty() {
        w.g.shakes.clear();
        // (its end routine, which zeroes the layer offsets, does not run: the BGs stay drawn at the last offset
        // until the next shake - sb 6-6 f4,694 on, the arena (3, -3) off)
    }
}

pub fn follow(w: &mut World, id: EntityId) {
    camera_mode_set(w);
    w.scene.cam.follow_last = None;
    w.scene.cam.mode = CamMode::Follow(id);
    w.scene.cam.scroll = None;
}

/// the follow camera's step of a frame whose target was out of the world during the camera step (an area load waiting
/// for its on-enter script: field::finish_arrival)
pub fn follow_owed(w: &mut World) {
    if let (CamMode::Follow(id), None) = (w.scene.cam.mode.clone(), w.scene.cam.scroll.as_ref()) {
        if !w.camera_held_by_shake() {
            follow_step(w, id);
        }
    }
}

/// the script owns player 0's camera (no step: `camera` steps it)
pub fn camera_owned(w: &World) -> bool {
    w.scene.cam.scroll.is_some() || !matches!(w.scene.cam.mode, CamMode::Player)
}

/// player-0 camera under script control. Returns true when the script owns the camera this frame (the caller
/// skips the player camera).
pub fn camera(w: &mut World) -> bool {
    w.scene.cam.stepped = true;
    if w.scene.cam.hold {
        w.scene.cam.hold = false;
        let c = &mut w.cameras[0];
        c.shown = c.rect;
        c.shown_fine = (c.x, c.y);
        return true;
    }
    if w.scene.cam.scroll.is_some() {
        let c = &mut w.cameras[0];
        c.shown = c.rect;
        c.shown_fine = (c.x, c.y);
        scroll_step(w);
        return true;
    }
    match w.scene.cam.mode.clone() {
        CamMode::Player => false,
        CamMode::Fixed => {
            let pre = w.scene.cam.pan_pre.take();
            let c = &mut w.cameras[0];
            c.shown = pre.unwrap_or(c.rect);
            true
        }
        CamMode::Follow(id) => {
            let c = &mut w.cameras[0];
            c.shown = c.rect;
            c.shown_fine = (c.x, c.y);
            // a screen shake takes the mode's camera slot while it lasts (field.rs camera)
            if !w.camera_held_by_shake() {
                follow_step(w, id);
            }
            true
        }
    }
}

/// the world update is suspended (modal camera scroll, mosaic): only the camera / screen run
pub fn frozen(w: &World) -> bool {
    // field: frames that pass while a batch is open (begin_parallel .. end_parallel: a fade inside it) do not run
    // the world update (sc_janemba: Gogeta's split, flash_white(10) + fade_to_white(5) between 0x38 and 0x3A - every
    // object holds its animation)
    w.scene.freeze || (w.field.is_some() && w.scene.director.batch)
}

// ---------------------------------------------------------------------------------------------------------------
// screen effects

/// start a modal screen effect; the host blocks until [`fade_done`]
pub fn start_fade(w: &mut World, kind: FadeKind, out: bool, frames: i32) {
    let len = match kind {
        FadeKind::Flash => 2 * frames.max(2),
        _ => frames.max(1),
    };
    w.scene.screen.fade = Some(Fade {
        kind,
        out,
        t: 0,
        len,
    });
    // the mosaic modes do not run the world update; mosaic_in restores the palette at once
    if kind == FadeKind::Mosaic {
        w.scene.freeze = true;
        if !out {
            w.scene.screen.black = 0;
        }
    }
}

pub fn fade_done(w: &World) -> bool {
    w.scene.screen.fade.is_none()
}

fn tick_screen(w: &mut World) {
    if let Some(f) = w.scene.screen.flash.as_mut() {
        f.t += 1;
        if f.t >= f.len {
            w.scene.screen.flash = None;
        }
    }
    let Some(mut f) = w.scene.screen.fade.clone() else {
        return;
    };
    f.t += 1;
    let lvl = (16 * f.t.min(f.len) / f.len.max(1)).clamp(0, 16);
    let s = &mut w.scene.screen;
    let fine = (256 * f.t.min(f.len) / f.len.max(1)).clamp(0, 256);
    match (f.kind, f.out) {
        (FadeKind::Black, true) => {
            // a fade-out darkens from the current palette: an already black screen stays black (return_to_field
            // after a scene that ended faded out: s057 -> 1-3)
            s.black = s.black.max(lvl);
            s.dim = s.dim.max(fine);
        }
        (FadeKind::Black, false) => {
            s.black = 16 - lvl;
            s.dim = 256 - fine;
            // the fade from black writes the palette from the stored colours: a white left by an earlier
            // fade_out_white is gone (earth_2_2_2 f5,023: fade_out_white, fade_out, the load, fade_in)
            s.white = 0;
        }
        (FadeKind::White, true) => s.white = lvl,
        (FadeKind::White, false) => {
            // the fade from white writes the palette from white toward the target: a screen left black before it
            // (a load's black) shows from its first step (real route 8-66 f18226: Yakon's room after the load)
            s.white = 16 - lvl;
            s.black = 0;
            s.dim = 0;
        }
        (FadeKind::Mosaic, true) => s.mosaic = lvl,
        (FadeKind::Mosaic, false) => s.mosaic = 16 - lvl,
        (FadeKind::Flash, _) => {
            let half = f.len / 2;
            s.white = if f.t <= half {
                16 * f.t / half.max(1)
            } else {
                16 * (f.len - f.t).max(0) / half.max(1)
            };
        }
    }
    if f.t >= f.len {
        if f.kind == FadeKind::Flash {
            s.white = 0;
        }
        s.fade = None;
        if f.kind == FadeKind::Mosaic {
            w.scene.freeze = false;
            if f.out {
                // mosaic_out ends with the palette RAM cleared
                w.scene.screen.black = 16;
            }
        }
    } else {
        s.fade = Some(f);
    }
}

/// the palette is all black: the brightest 5-bit colour scales to 0 (or the screen is fully faded)
pub fn palette_black(w: &World) -> bool {
    let s = &w.scene.screen;
    // a black fade in progress scales the palette by its fine level; otherwise the coarse level (menus, mosaic)
    let dim = if s.fade.as_ref().is_some_and(|f| f.kind == FadeKind::Black) {
        s.dim
    } else {
        s.dim.max(s.black * 16)
    };
    (31 * (256 - dim.min(256))) >> 8 == 0
}

/// brightness toward black 0..16 shown this frame (`RenderView::fade_black`)
pub fn fade_black(w: &World) -> u8 {
    w.scene.screen.black.clamp(0, 16) as u8
}

// ---------------------------------------------------------------------------------------------------------------
// dialog

/// open a text box (event text node). `text` keeps its placement prefix (`!` top, `@` middle, `#` bottom);
/// `typing` = per page, the frames the typewriter needs (one visible character per frame, the line fitter's
/// skipped spaces / new lines / option markers ride along; computed by the host with the dialogue font). A page
/// whose lines start with '-' is a choice (options = those lines).
pub fn open_text(
    w: &mut World,
    text: &str,
    text_ref: u32,
    portrait: u8,
    kind: u8,
    typing: Vec<u16>,
) {
    let (mut pos, body) = split_pos(text);
    let mut portrait_left = false;
    // text boxes place themselves by speaker; the menu / message window (kind 3) is another object
    if kind != 3 {
        let prefix = (body.len() != text.len()).then_some(pos);
        // nested scenes (run_scene natives, depth > 1) are built with (-1, -1)
        let anchor = if w.scene.depth <= 1 {
            w.scene.talk_anchor
        } else {
            NO_ANCHOR
        };
        (pos, portrait_left) = speaker_layout(prefix, portrait, anchor);
    }
    let last = body.split('\u{b}').last().unwrap_or("");
    let options = last
        .lines()
        .filter(|l| l.trim_start().starts_with('-'))
        .count() as i32;
    // the box object's tiles (`0x08012030`; a portrait unless the box kind is 0 / 7)
    if kind == 3 {
        // a menu / message window (vtable 0x0804ABD8) holds no tiles of its own; its options' cursor (vtable
        // 0x0804C974, 0x100) does - not the dialogue box's blocks (tour_earth_zmart_thieves f425: Z-Mart 0-93's
        // welcome window, the game's (375, 4); freed as A / B picks, f630)
        if options > 0 {
            crate::vram::acquire(w, crate::vram::Owner::Cursor, 0x100);
        }
    } else {
        crate::vram::text_open(w, portrait != 0 && portrait != 7);
    }
    let pages = (typing.len().max(1)) as u16;
    w.scene.dialog = Some(Dialog {
        text: body.to_string(),
        text_ref,
        portrait,
        kind,
        pos,
        page: 0,
        pages,
        typing,
        typed: 0,
        options,
        cursor: 0,
        phase: DialogPhase::Opening,
        t: 0,
        result: None,
        portrait_left,
        fades: false,
        page_t: 0,
        type_c: 0,
    });
    if kind == 3 {
        let f = window_fades(w);
        if let Some(d) = w.scene.dialog.as_mut() {
            d.fades = f;
        }
    }
    // opened in a menu's last fade-in iteration (the event resumed from the shop native, the object pass still to
    // come in the game's iteration): the box's first update reads this iteration's B, which closes it at once
    // (level 1; run 3 row 891,908 - Papaya Island's food stall s258, B on the "Goodbye!" box's first frame, the
    // event on at 891,917)
    if kind != 3 && w.menu.faded_in_iter && w.input[0].pressed & B != 0 {
        if let Some(d) = w.scene.dialog.as_mut() {
            d.result = Some(if d.options > 0 { -1 } else { 0 });
            d.phase = DialogPhase::Closing;
            d.t = DIALOG_CLOSE - 1;
        }
        crate::vram::release(w, crate::vram::Owner::Cursor);
        crate::vram::text_close(w);
    }
    // the box's creation is a long main-loop iteration (`Options::sim_lag`): `sndsim::dialog_frames`
    if w.opts.sim_lag && kind != 3 {
        let chars = w
            .scene
            .dialog
            .as_ref()
            .map(|d| d.typing.first().copied().unwrap_or(0) as i64)
            .unwrap_or(0);
        if let Some(n) = crate::sndsim::dialog_frames(w, portrait, chars) {
            w.g.lag = (n as u16).saturating_sub(1);
            w.g.lag_cam = u16::MAX;
        }
    }
}

/// A text box's placement and portrait side from its prefix, the node `arg` and the event's speaker anchor
/// (`0x08012012`, VERIFIED on Yemma's Office talks from each side and the intro): a prefix (`!` / `@` / `#`) wins;
/// else `arg` 0 → bottom; `arg` 1 → bottom when anchor y >= 0 (else top), portrait left when anchor x < 0; any
/// other `arg` → bottom when anchor y < 0 (else top), portrait left when anchor x >= 0. So a talk from below the
/// NPC opens at the top, and the portrait is on the NPC's side; cutscenes ((-1, -1)) open at the bottom with the
/// portrait right.
pub fn speaker_layout(prefix: Option<BoxPos>, arg: u8, anchor: [i32; 2]) -> (BoxPos, bool) {
    let (mut pos, mut left) = (BoxPos::Top, false);
    match arg {
        0 => pos = BoxPos::Bottom,
        1 => {
            if anchor[1] >= 0 {
                pos = BoxPos::Bottom;
            }
            left = anchor[0] < 0;
        }
        _ => {
            if anchor[1] < 0 {
                pos = BoxPos::Bottom;
            }
            left = anchor[0] >= 0;
        }
    }
    (prefix.unwrap_or(pos), left)
}

/// open a choice window (event node kind 3: lines starting with '-' are the options)
pub fn open_choice(w: &mut World, text: &str, portrait: u8, typing: Vec<u16>) {
    open_text(w, text, 0, portrait, 3, typing);
    if let Some(d) = w.scene.dialog.as_mut() {
        d.options = d.options.max(1);
    }
}

fn split_pos(text: &str) -> (BoxPos, &str) {
    match text.chars().next() {
        Some('!') => (BoxPos::Top, &text[1..]),
        Some('@') => (BoxPos::Middle, &text[1..]),
        Some('#') => (BoxPos::Bottom, &text[1..]),
        _ => (BoxPos::Bottom, text),
    }
}

/// the box closed: its result (choice index, 0 for plain text) — clears it
pub fn dialog_result(w: &mut World) -> Option<i32> {
    match &w.scene.dialog {
        Some(d) if d.phase == DialogPhase::Closed => {
            let r = d.result;
            w.scene.dialog = None;
            r.or(Some(0))
        }
        _ => None,
    }
}

/// The dialogue box (module 0x0800AD00), measured on NPC talk (events 43 / 74): the first character appears
/// [`DIALOG_OPEN`] frames after the box is created, then one per frame; A is taken once the page is complete (A
/// while typing does nothing); on a page break the next page types from the next frame; after the last page the box
/// closes and the event continues [`DIALOG_CLOSE`] frames after the A. A choice cursor moves with UP / DOWN.
fn tick_dialog(w: &mut World) {
    let k = w.scene.keys[0].pressed;
    let speed = w.menu.settings.text_speed;
    let Some(d) = w.scene.dialog.as_mut() else {
        return;
    };
    d.t += 1;
    // a fading message window reads no keys while it fades in: its callback ramps BLDALPHA for 16 updates
    // (0x08030BF5), then the input phase (0x08030B6D: UP / DOWN, A, B) takes over (run 2 row 1,470,522, West City
    // Z-Mart s030 at 0-92: B 12 frames after the welcome window opened is ignored, the next one 42 after picks)
    if d.kind == 3 && d.fades && d.t <= WINDOW_FADE_IN && matches!(d.phase, DialogPhase::Opening | DialogPhase::Shown)
    {
        d.phase = DialogPhase::Shown;
        return;
    }
    // (a text box's choice: the cursor object exists from its last page's text on - UP / DOWN before that do
    // nothing: tour_kami_grandkai11 f624..636, DOWN on the first page of a 2-page "Yes / No")
    let cursor_up = d.kind == 3
        || d.page + 1 >= d.pages
            && d.phase == DialogPhase::Shown
            && (d.typed > 0 || d.typing.get(d.page as usize).copied().unwrap_or(0) == 0);
    if d.options > 0 && cursor_up && matches!(d.phase, DialogPhase::Opening | DialogPhase::Shown) {
        // (the cursor stops at the first / last option, no wrap: run 3 row 883,540, Papaya Island 7-2's trinket stall
        // s259 - DOWN twice on "-Sure / -No thanks" stays on No thanks)
        if k & DOWN != 0 {
            d.cursor = (d.cursor + 1).min(d.options - 1);
        }
        if k & UP != 0 {
            d.cursor = (d.cursor - 1).max(0);
        }
    }
    if d.kind == 3 {
        // a menu / message window (node kind 3): shown at once, A picks, B cancels (-1), closes 2 frames later
        match d.phase {
            DialogPhase::Opening | DialogPhase::Shown => {
                d.phase = DialogPhase::Shown;
                if k & A != 0 {
                    d.result = Some(d.cursor);
                    d.phase = DialogPhase::Closing;
                    d.t = 0;
                    crate::vram::release(w, crate::vram::Owner::Cursor);
                } else if k & B != 0 {
                    d.result = Some(-1);
                    d.phase = DialogPhase::Closing;
                    d.t = 0;
                    crate::vram::release(w, crate::vram::Owner::Cursor);
                }
            }
            DialogPhase::Closing => {
                if d.t
                    >= if d.fades {
                        WINDOW_FADE_CLOSE
                    } else {
                        WINDOW_CLOSE
                    }
                {
                    d.phase = DialogPhase::Closed;
                    crate::vram::text_close(w);
                }
            }
            DialogPhase::Closed => {}
        }
        return;
    }
    // B (a fresh press) closes the box at once, opening or shown, every page left unread (the box's dispatcher
    // `0x0800B32C`: the result -1, the close routine `0x0800AE64` from this frame): its open level (+0x50: t + 1 while
    // opening, 8 open) counts down a step a frame, the box goes at 0 and the event continues the frame after - B +
    // level (run 2 row 140,804: B at level 6, the box gone at 140,809, the event on at 140,810; an A close starts the
    // frame after the press: A + 9, [`DIALOG_CLOSE`])
    if k & B != 0 && matches!(d.phase, DialogPhase::Opening | DialogPhase::Shown) {
        let level = (d.t + 1).min(8);
        d.result = Some(if d.options > 0 { -1 } else { 0 });
        d.phase = DialogPhase::Closing;
        d.t = DIALOG_CLOSE - level;
        let gone = d.t == DIALOG_CLOSE - 1;
        // (a choice's cursor goes with the box: tour_castle_ship_66 f358, B on the save point's "-Save /
        // -Switch Characters" - the cursor's block stays until the box is gone)
        if gone {
            crate::vram::text_close(w);
            crate::vram::release(w, crate::vram::Owner::Cursor);
        }
        return;
    }
    let mut first = false;
    if d.phase == DialogPhase::Opening && d.t >= DIALOG_OPEN {
        // the box's update runs from this frame (`0x0800B156` -> `0x0800B0A8`)
        d.phase = DialogPhase::Shown;
        d.typed = 0;
        first = true;
    }
    match d.phase {
        DialogPhase::Opening => {}
        DialogPhase::Shown => {
            // A pressed this frame (+0x30; B never gets here): a complete page turns / the box closes
            let mut press = k & A != 0;
            let n = type_steps(speed, press, &mut d.type_c);
            let mut cursor = false;
            for _ in 0..n {
                let need = d.typing.get(d.page as usize).copied().unwrap_or(0) as i32;
                if d.typed < need {
                    if d.typed == 0 {
                        d.page_t = d.t;
                        // a choice box's cursor object (`0x080178C8`, 0x100 tiles) appears with the text of its last
                        // page (the options' page: tour_kami_grandkai11 f646, the 2nd page of a 2-page choice)
                        cursor |= d.page + 1 >= d.pages && d.options > 0;
                    }
                    d.typed += 1;
                } else if press {
                    press = false;
                    if d.page + 1 < d.pages {
                        d.page += 1;
                        d.typed = 0;
                        d.page_t = d.t + 1;
                    } else {
                        d.result = Some(if d.options > 0 { d.cursor } else { 0 });
                        d.phase = DialogPhase::Closing;
                        d.t = 0;
                        break;
                    }
                }
            }
            // (a page with nothing to type shows at once)
            if first && d.typing.get(d.page as usize).copied().unwrap_or(0) == 0 {
                d.page_t = d.t;
                cursor |= d.options > 0;
            }
            if cursor {
                crate::vram::acquire(w, crate::vram::Owner::Cursor, 0x100);
            }
            if w.scene.dialog.as_ref().is_some_and(|d| d.phase == DialogPhase::Closing) {
                crate::vram::release(w, crate::vram::Owner::Cursor);
            }
        }
        DialogPhase::Closing => {
            // the box object goes a frame before the event continues
            let gone = d.t == DIALOG_CLOSE - 1;
            if d.t >= DIALOG_CLOSE {
                d.phase = DialogPhase::Closed;
            }
            if gone {
                crate::vram::text_close(w);
                crate::vram::release(w, crate::vram::Owner::Cursor);
            }
        }
        DialogPhase::Closed => {}
    }
}

// ---------------------------------------------------------------------------------------------------------------
// menus and mini-games (deterministic models; the host applies purchases / sales / switches it reads from `log`)

/// screen fade of the menu changes (quick fade 0x0805058C / 0x08050568)
pub const MENU_FADE: i32 = 16;

pub fn open_menu(w: &mut World, kind: MenuKind) {
    w.scene.menu = Some(Menu {
        kind,
        cursor: 0,
        t: 0,
        gauge: 0,
        fade: MENU_FADE,
        closing: false,
        result: None,
        log: Vec::new(),
        pending: None,
    });
    // the world is saved and hidden while the menu screen runs (0x08011DB4 / 0x08011E0E)
    w.scene.freeze = true;
}

/// the menu closed: (result, log) — clears it
pub fn menu_result(w: &mut World) -> Option<(i32, Vec<(i32, i32)>)> {
    match &w.scene.menu {
        Some(m) if m.result.is_some() && m.closing && m.fade >= MENU_FADE + 2 => {
            let r = (m.result.unwrap_or(-1), m.log.clone());
            w.scene.menu = None;
            w.scene.freeze = false;
            w.scene.screen.black = 0;
            Some(r)
        }
        _ => None,
    }
}

fn tick_menu(w: &mut World) {
    let k = w.scene.keys[0].pressed;
    let held = w.scene.keys[0].held;
    let shop: Vec<ShopItem> = w.scene.shop.clone();
    let zenie = w.story.zenie as i64;
    // the character menu's slots: the game's nine records, more when mods add story characters
    let nchars = w.story.chars.len().max(crate::story::CHARS) as i32;
    let mut black: Option<i32> = None;
    let mut sound: Option<i32> = None;
    if let Some(m) = w.scene.menu.as_mut() {
        menu_step(m, k, held, &shop, zenie, nchars, &mut black, &mut sound);
    }
    if let Some(b) = black {
        w.scene.screen.black = b;
    }
    if let Some(s) = sound {
        w.sfx(s, 64);
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn menu_step(
    m: &mut Menu,
    k: u16,
    held: u16,
    shop: &[ShopItem],
    zenie: i64,
    nchars: i32,
    black: &mut Option<i32>,
    sound: &mut Option<i32>,
) {
    m.t += 1;
    // the screen change: MENU_FADE frames to black, the menu screen; on leaving, the world is restored (black for
    // one frame, two frames after the closing press) and fades in over MENU_FADE frames (measured on the shop)
    if m.closing {
        m.fade += 1;
        let f = m.fade;
        if f == 2 {
            *black = Some(16);
        } else if f > 2 {
            *black = Some((16 - 16 * (f - 2) / MENU_FADE).clamp(0, 16));
        }
        return;
    }
    if m.fade > 0 {
        m.fade -= 1;
        return;
    }
    // a purchase confirmed last frame: the money changes hands with the shop's sound
    if let Some(p) = m.pending.take() {
        if let Some(it) = shop.get(p as usize) {
            if zenie >= it.price as i64 {
                m.log.push((p, 1));
                *sound = Some(41);
            }
        }
        return;
    }
    let close = |m: &mut Menu, r: i32| {
        m.result = Some(r);
        m.closing = true;
        m.fade = 0;
    };
    match m.kind {
        MenuKind::ShopBuy | MenuKind::ShopSell { .. } | MenuKind::Character => {
            let n = match m.kind {
                MenuKind::ShopBuy => (shop.len() as i32).max(1),
                MenuKind::Character => nchars,
                _ => 64,
            };
            if k & DOWN != 0 {
                m.cursor = (m.cursor + 1) % n;
            }
            if k & UP != 0 {
                m.cursor = (m.cursor + n - 1) % n;
            }
            if k & A != 0 {
                match m.kind {
                    MenuKind::Character => close(m, m.cursor),
                    MenuKind::ShopBuy => {
                        // the purchase happens in the frame of the press (its redraw makes that frame long)
                        if let Some(it) = shop.get(m.cursor as usize) {
                            if zenie >= it.price as i64 {
                                m.log.push((m.cursor, 1));
                                *sound = Some(41);
                            }
                        }
                    }
                    _ => m.log.push((m.cursor, -1)),
                }
            } else if k & B != 0 {
                close(m, -1);
            }
        }
        MenuKind::Save => {
            if k & A != 0 {
                close(m, 1);
            } else if k & B != 0 {
                close(m, 0);
            }
        }
        MenuKind::WatchThis { level } => {
            // a fixed-length button sequence: each press advances the gauge (model, not the game's prompts)
            if k != 0 {
                m.gauge += 1;
            }
            if m.gauge >= 4 + level.max(0) {
                close(m, 1);
            } else if m.t > 600 {
                close(m, 0);
            }
        }
        MenuKind::PowerStruggle | MenuKind::Punch { .. } => {
            if k & A != 0 {
                m.gauge += 4;
            }
            if m.t % 8 == 0 {
                m.gauge -= 1;
            }
            if m.gauge >= 100 {
                close(m, 1);
            } else if m.gauge < -100 || m.t > 1800 {
                close(m, 0);
            }
        }
        MenuKind::Credits => {
            if m.t >= CREDITS_LEN || (held & A != 0 && k & B != 0) {
                close(m, 0);
            }
        }
    }
}

/// length of the staff credits mode (0x080335BC)
pub const CREDITS_LEN: i32 = 3600;

// ---------------------------------------------------------------------------------------------------------------
// collision overrides

/// `open_area` / `block_area` (+ `_alt`): set / clear the 8x8 tiles of [x0>>3, (x1+7)>>3) x [y0>>3, (y1+7)>>3)
pub fn set_area(w: &mut World, rect: [i32; 4], main: bool, open: bool) {
    w.scene.area_ops.push(AreaOp { rect, main, open });
    apply_area(w, rect, main, open);
}

fn apply_area(w: &mut World, r: [i32; 4], main: bool, open: bool) {
    // `open_area` / `block_area` (0x2B / 0x2C, main) set the walk grid and the enemy layer; the `_alt` forms (0x2D /
    // 0x2E) set the shot grid only (re/buu/notes.md "Field: live collision grids", all 458 area loads verified)
    let Some(col) = w.col.as_mut() else { return };
    let g = crate::content::GRID as i32;
    let (tx0, ty0, tx1, ty1) = (
        (r[0] >> 3).max(0),
        (r[1] >> 3).max(0),
        ((r[2] + 7) >> 3).min(g),
        ((r[3] + 7) >> 3).min(g),
    );
    let v = open as u8;
    let m = std::sync::Arc::make_mut(&mut col.map);
    for ty in ty0..ty1 {
        for tx in tx0..tx1 {
            let k = (ty * g + tx) as usize;
            if main {
                m.walk[k] = v;
                // the field's live walk grid (talk NPC / prop marks, `Collision.walk`) shadows the map's
                if let Some(wk) = col.walk.as_mut() {
                    wk[k] = v;
                }
                col.enemy[k] = v;
            } else {
                m.shot[k] = v;
            }
        }
    }
}

/// re-apply the collision overrides after the static grids were re-attached (byte snapshot restore): to the map's
/// copy only - the live walk / enemy layers come from the snapshot as they stood, with every mark made after the
/// override (re-applying them there undid an actor's mark on an opened tile: the whole run's save-state check, Earth
/// 2-x at step 23,151, a restored world one byte row apart in the enemy layer)
pub fn reapply_areas(w: &mut World) {
    let ops = w.scene.area_ops.clone();
    let Some(col) = w.col.as_mut() else { return };
    let g = crate::content::GRID as i32;
    let m = std::sync::Arc::make_mut(&mut col.map);
    for op in ops {
        let r = op.rect;
        let (tx0, ty0, tx1, ty1) = (
            (r[0] >> 3).max(0),
            (r[1] >> 3).max(0),
            ((r[2] + 7) >> 3).min(g),
            ((r[3] + 7) >> 3).min(g),
        );
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let k = (ty * g + tx) as usize;
                if op.main {
                    m.walk[k] = op.open as u8;
                } else {
                    m.shot[k] = op.open as u8;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// queries

/// screen rect queries 0xCB / 0xCC: a w x h rect centred on the actor's box centre, clamped to the camera / map
pub fn screen_rect_at(w: &World, i: usize, rw: i32, rh: i32) -> (i32, i32) {
    // field: centred on the footprint centre (sc_janemba: Gogeta's split, rect 48 x 8 at (144, 200) with a 16 x 8
    // footprint -> (128, 200): the walk_to_speed target the script builds from it)
    if w.field.is_some() {
        // ... and moved by 0x08009816 to the first spot where the rect fits the walk layer (the split's ending: Vegeta
        // by the arena wall walks to where both fit)
        let (cx, cy) = w.actor(i).map(|a| a.foot_center()).unwrap_or((0, 0));
        return crate::enemy::free_rect(w, cx - rw / 2, cy - rh / 2, rw, rh);
    }
    let (cx, cy) = box_center(w, i);
    let c = w.cameras[0].rect;
    let x = (cx - rw / 2).clamp(c.x0, (c.x1 - rw).max(c.x0));
    let y = (cy - rh / 2).clamp(c.y0, (c.y1 - rh).max(c.y0));
    (x, y)
}

/// the game's sine table 0x087B6EEC (256 entries per turn, 16.16; truncated, so not `sin()` rounded)
pub const SIN_TABLE: [i32; 256] = [
    0, 1608, 3215, 4821, 6423, 8022, 9616, 11204, 12785, 14359, 15923, 17479, 19024, 20557, 22078,
    23586, 25079, 26557, 28020, 29465, 30893, 32302, 33692, 35061, 36409, 37736, 39039, 40319,
    41575, 42806, 44011, 45189, 46340, 47464, 48558, 49624, 50660, 51665, 52639, 53581, 54491,
    55368, 56212, 57022, 57797, 58538, 59243, 59913, 60547, 61144, 61705, 62228, 62714, 63162,
    63571, 63943, 64276, 64571, 64826, 65043, 65220, 65358, 65457, 65516, 65536, 65516, 65457,
    65358, 65220, 65043, 64826, 64571, 64276, 63943, 63571, 63162, 62714, 62228, 61705, 61144,
    60547, 59913, 59243, 58538, 57797, 57022, 56212, 55368, 54491, 53581, 52639, 51665, 50660,
    49624, 48558, 47464, 46340, 45189, 44011, 42806, 41575, 40319, 39039, 37736, 36409, 35061,
    33692, 32302, 30893, 29465, 28020, 26557, 25079, 23586, 22078, 20557, 19024, 17479, 15923,
    14359, 12785, 11204, 9616, 8022, 6423, 4821, 3215, 1608, 0, -1608, -3215, -4821, -6423, -8022,
    -9616, -11204, -12785, -14359, -15923, -17479, -19024, -20557, -22078, -23586, -25079, -26557,
    -28020, -29465, -30893, -32302, -33692, -35061, -36409, -37736, -39039, -40319, -41575, -42806,
    -44011, -45189, -46340, -47464, -48558, -49624, -50660, -51665, -52639, -53581, -54491, -55368,
    -56212, -57022, -57797, -58538, -59243, -59913, -60547, -61144, -61705, -62228, -62714, -63162,
    -63571, -63943, -64276, -64571, -64826, -65043, -65220, -65358, -65457, -65516, -65536, -65516,
    -65457, -65358, -65220, -65043, -64826, -64571, -64276, -63943, -63571, -63162, -62714, -62228,
    -61705, -61144, -60547, -59913, -59243, -58538, -57797, -57022, -56212, -55368, -54491, -53581,
    -52639, -51665, -50660, -49624, -48558, -47464, -46340, -45189, -44011, -42806, -41575, -40319,
    -39039, -37736, -36409, -35061, -33692, -32302, -30893, -29465, -28020, -26557, -25079, -23586,
    -22078, -20557, -19024, -17479, -15923, -14359, -12785, -11204, -9616, -8022, -6423, -4821,
    -3215, -1608,
];

/// `sin_scale(radius, angle)` = radius * sin_table[angle & 0xFF] >> 16 (`cos_scale`: angle + 64)
pub fn sin_scale(radius: i32, angle: i32) -> i32 {
    (radius.wrapping_mul(SIN_TABLE[(angle & 0xff) as usize])) >> 16
}

fn hud_y_rest() -> i32 {
    8
}
