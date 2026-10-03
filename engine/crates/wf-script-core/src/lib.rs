//! `wf-script-core`: the script host — runs Buu's Fury scripts (`wf-script` bytecode VM, `wf-script-rhai`
//! runtime) against the deterministic core `wf-core`. Spec: `engine/docs/script-host.md`.
//!
//! * [`CoreHost`] implements [`wf_script::Host`] over `&mut World` + `&mut HostState` (borrowed: the engine
//!   calls it inside a frame); [`SharedHost`] is the owned form the Rhai runtime needs (`Box<dyn Host>`).
//! * Persistent game state lives in the World: `World.story` (flags, words, inventory, zenie, the 9 character
//!   records; field agent), `World.scene` (command tracks, director, camera control, screen effects, dialog,
//!   menus, boss bar, ...; `wf_core::scene`), `World.field` (areas, warps, map objects, pickups). [`HostState`]
//!   keeps only what belongs to the running script: the pending block, the menu result word, loot position,
//!   timers, the equipment-effect accumulator and a presentation log.
//! * Blocking: a native that spans frames records a [`Pending`] and returns `Block`; after every frame step the
//!   engine calls [`poll`], which says when to resume the thread and with what value.
//! * Multiplayer: actor 0 = the leader = `World.players[0]`; `player(n)` = `World.players[n-1]`.

pub mod binding;
pub mod natives;
pub mod text;

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wf_core::input::A;
use wf_core::scene;
use wf_core::world::{Body, EntityId, World};
use wf_script::vm::{Call, ExtCall, Globals, Host, NativeResult, NodeCall, SceneCall};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DropKind {
    Zenie(i32),
    Supply(i32),
    Equipment(i32),
}

/// A pickup to spawn at the loot position (battle mode keeps them here; the field spawns `Body::Pickup`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DropRequest {
    pub kind: DropKind,
    pub x: i32,
    pub y: i32,
}

/// Log of what scripts presented (tools, tests; the live state is `World.scene`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Present {
    Text { text: String, portrait: u8, args: Vec<i32> },
    Choice { text: String, portrait: u8 },
    ItemGet { item: i32, equipment: bool },
}

/// one phase of a multi-step blocking native (scene loads, field return)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    Fade { kind: scene::FadeKind, out: bool, frames: i32 },
    LoadScene { world: i32, map: i32, variant: i32, x: i32, y: i32, objects: bool },
    ReturnToField,
}

/// A frame-spanning action in progress (the GBA's nested main loop).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pending {
    /// `wait(frames)`, `hide_title_text` and fixed-length actions
    Frames { left: i32, result: Option<i32> },
    /// `wait_button()`: until the leader presses A
    Button,
    /// the director's wait mode (queued commands outside a batch, `end_parallel`)
    Director,
    /// a modal screen effect (fades, flash, mosaic)
    Fade,
    /// a modal camera scroll (0x31 / 0x32)
    Scroll,
    /// a text box / choice window of an event node
    Dialog { choice: bool },
    /// a menu / mini-game screen; `push`: the native pushes (result - 1)
    Menu { native: u8, push: bool },
    /// a field transition (warp_fade)
    Transition,
    /// an immediate `warp` whose area load waits for its on-enter script: the host runs that script nested (the
    /// game runs it inside EnterArea) and the warp returns once the players arrived
    Arrival,
    /// several phases in a row
    Phases { list: Vec<Phase>, k: usize, started: bool },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timers {
    pub countdown: i32,
    pub countdown_on: bool,
    pub stopwatch: i32,
    pub stopwatch_on: bool,
}

/// the field saved by `save_field_on_next_scene` for `return_to_field`
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SavedField {
    pub area: (i32, i32, i32),
    pub x: i32,
    pub y: i32,
    pub facing: u8,
    pub cam: (i32, i32),
}

/// The script's own state (not World state). Plain data, snapshotted with the threads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostState {
    /// `get_menu_result` (word 0x03002A60, -1 = cancelled)
    pub menu_result: i32,
    pub timers: Timers,
    /// equipment-effect accumulator (0x03002B5C) of a running equipment script
    pub eq_accum: [i32; 16],
    /// loot position (0x03001010), set by the dying enemy / breakable
    pub loot_pos: (i32, i32),
    /// drops in battle mode (no field to spawn them in)
    pub drops: Vec<DropRequest>,
    pub present: Vec<Present>,
    /// the NPC being talked to (actor id 9, 0x03001BAC)
    pub talk_target: Option<EntityId>,
    /// battle-mode area (field mode reads World.field)
    pub area: (i32, i32, i32),
    pub save_armed: bool,
    pub saved_field: Option<SavedField>,
    /// timed stat boost (GD+0x4DC..0x4F0): frames, str, pow, end, spd, special
    pub boost: [i32; 6],
    /// Switch Character's chosen leader was rebuilt at the screen's close (the world restore), not again at the end
    #[serde(default)]
    pub charsel_rebuilt: bool,
    pub pending: Option<Pending>,
    /// the thread is resuming from a command wait (`Pending::Director`) this frame: an event that ends now exits at
    /// the top of the frame in the game (before the objects update)
    #[serde(default)]
    pub from_wait: bool,
    /// the newest entity when the outermost event started: older objects update after the event's scene object
    /// (the world updates newest first), so only they see a release at the top of the frame
    #[serde(default)]
    pub event_watermark: EntityId,
    /// the host runs an area load's on-enter script nested inside the script that warped (wf-import `StoryHost`);
    /// otherwise `warp` does not wait for it (the script is queued after the running one)
    #[serde(default)]
    pub nest_area_enter: bool,
    /// the running job is a unit script (inline code): an event it starts (`run_event`) is the VM's nested loop, not
    /// a scene object — its end gives the map NPCs no extra animation tick (2-90's gate: s517 from the button
    /// trigger's code)
    #[serde(default)]
    pub in_unit: bool,
    /// the running job is an NPC's talk event (started in the player's update): Story / HUD natives between a wait's
    /// end and its exit keep the exit at the top of the frame (4-27: s193 -> s194; not for map-trigger cutscenes,
    /// s073 / s159)
    #[serde(default)]
    pub talk_event: bool,
    /// the thread resumed from a modal scroll this frame: the scroll mode returned at the top of the frame, so an
    /// exit now leaves this frame's world update to run (4-20: s184 ends on its scroll, the NPCs tick in that frame)
    #[serde(default)]
    pub from_scroll: bool,
}

/// compatibility name (older tests / docs)
pub type StoryState = HostState;

/// Owned state for [`SharedHost`].
pub struct CoreState {
    pub world: World,
    pub story: HostState,
}

/// Borrowed host: the engine builds one per frame / per script call.
pub struct CoreHost<'a> {
    pub w: &'a mut World,
    pub s: &'a mut HostState,
}

/// Owned host for `wf_script_rhai::Runtime` (which needs `Box<dyn Host>`).
#[derive(Clone)]
pub struct SharedHost(pub Rc<RefCell<CoreState>>);

/// Entity index of the leader (actor 0 = `World.players[0]`).
pub fn leader(w: &World) -> Option<usize> {
    w.players.first().and_then(|&id| w.find(id))
}

/// `player(n)` (1-based): entity id of player n.
pub fn player(w: &World, n: i32) -> Option<EntityId> {
    if n < 1 {
        return None;
    }
    w.players.get(n as usize - 1).copied()
}

pub(crate) fn char_of_form(w: &World, form: i32) -> Option<i32> {
    w.data.form(form).map(|f| f.char_)
}

/// script sprite id of character `c`'s current form (`char_sprite`)
pub fn char_sprite(w: &World, c: i32) -> Option<i32> {
    let form = current_form(w, c)?;
    w.data.form(form).map(|f| f.gfx)
}

/// character `c`'s current form: the live fighter's when one plays it, else the story record's
pub fn current_form(w: &World, c: i32) -> Option<i32> {
    for &id in &w.players {
        if let Some(i) = w.find(id) {
            let f = w.pl_ref(i).form;
            if char_of_form(w, f) == Some(c) {
                return Some(f);
            }
        }
    }
    let cs = w.story.chars.get(c.max(0) as usize)?;
    if cs.form != 0 || w.data.form(0).is_some_and(|f| f.char_ == c) {
        return Some(cs.form);
    }
    w.data.chars.iter().find(|x| x.index == c).map(|x| x.form)
}

/// the game's object list has the newest object first (`0x080001C6` returns the first match): our entity list is
/// oldest first, so the newest matching entity wins (boss_defeat: commands after a second `spawn_actor(89)` go
/// to the new explosion)
fn first_with_sprite(w: &World, sprite: i32) -> Option<usize> {
    (0..w.ents.len()).rev().find(|&i| !w.ents[i].removed && scene::sprite_id(w, i) == Some(sprite))
}

/// Resolve a script actor id (`0x080001C6`): 0 leader; 1..8 the first object showing character id-1's current
/// sprite; 9 the talk target; >= 10 the first object with that sprite id; < 0 the map placement -(id+1).
pub fn resolve_actor(w: &World, s: &HostState, id: i32) -> Option<usize> {
    match id {
        0 => leader(w),
        1..=8 => first_with_sprite(w, char_sprite(w, id - 1)?),
        9 => s.talk_target.and_then(|t| w.find(t)).filter(|&i| !w.ents[i].removed),
        10.. => first_with_sprite(w, id),
        _ => {
            let tag = -(id + 1);
            if let Some(i) = (0..w.ents.len()).find(|&i| !w.ents[i].removed && matches!(&w.ents[i].body, Body::Npc(n) if n.tag == Some(tag))) {
                return Some(i);
            }
            wf_core::field::actor_by_tag(w, tag).filter(|&i| !w.ents[i].removed)
        }
    }
}

/// compatibility: resolve with no host state
pub fn resolve(w: &World, id: i32) -> Option<usize> {
    resolve_actor(w, &HostState::default(), id)
}

/// `count_actors`
pub fn count_actors(w: &World, s: &HostState, id: i32) -> i32 {
    let sprite = match id {
        0 => leader(w).and_then(|i| scene::sprite_id(w, i)),
        1..=8 => char_sprite(w, id - 1),
        9 => return resolve_actor(w, s, 9).is_some() as i32,
        10.. => Some(id),
        _ => return resolve_actor(w, s, id).is_some() as i32,
    };
    let Some(sprite) = sprite else { return 0 };
    (0..w.ents.len()).filter(|&i| !w.ents[i].removed && scene::sprite_id(w, i) == Some(sprite)).count() as i32
}

impl Host for CoreHost<'_> {
    fn native(&mut self, call: Call<'_>) -> Result<NativeResult, String> {
        // any native between a wait's end and the event's exit: the exit no longer happens at the top of the frame
        // (story playthrough: s073 end_parallel -> camera_follow -> exit, no NPC catch-up)
        let keeps = self.s.talk_event && (matches!(binding::binding(call.native).at, binding::Where::Story | binding::Where::Vm) || matches!(call.native, 0x2f | 0x30));
        if !keeps {
            self.s.from_wait = false;
        }
        self.s.from_scroll = false;
        let r = natives::call(self, &call);
        // the VM's globals are World state too (they persist across scripts and snapshot with the world)
        self.w.story.g = call.globals.g;
        r
    }
    fn set_var(&mut self, var: u8, value: i32, _p: &mut [i32; 4], g: &mut Globals) -> Result<(), String> {
        let r = natives::set(self, var, value);
        self.w.story.g = g.g;
        r
    }
    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        natives::node(self, call)
    }
    fn ext(&mut self, call: ExtCall<'_>) -> Result<NativeResult, String> {
        natives::ext(self, &call)
    }
    fn scene(&mut self, call: SceneCall) -> Result<NativeResult, String> {
        match call {
            // run_scene: the scene object's first step runs a frame after its creation; players lose their input
            SceneCall::Enter { .. } => {
                if self.w.scene.depth == 0 {
                    self.s.event_watermark = self.w.ents.iter().map(|e| e.id).max().unwrap_or(0);
                }
                begin_event(self.w);
                self.s.pending = Some(Pending::Frames { left: 1, result: None });
                Ok(NativeResult::Block)
            }
            SceneCall::Exit { .. } => {
                if std::mem::take(&mut self.s.from_scroll) && self.w.scene.depth == 1 {
                    if let Some(f) = self.w.field.as_mut() {
                        f.held_event = true;
                    }
                }
                end_event(self.w);
                if self.s.in_unit && self.w.scene.depth == 0 {
                    // (not for an event run before the objects, a queued one / a broken pod's callback: the enemies
                    // run their update in its exit frame, segment sb 21-11)
                    if let Some(f) = self.w.field.as_mut().filter(|f| !f.queued_event) {
                        f.event_ended = false;
                    }
                }
                // the event ended straight out of a command wait: in the game the wait loop returned at the top of
                // this frame, so the field's NPCs already ran this frame's update unfrozen
                if self.s.from_wait && self.w.scene.depth == 0 && !self.s.in_unit {
                    release_field_npcs(self.w, self.s.event_watermark);
                }
                Ok(NativeResult::Done(None))
            }
        }
    }
}

impl Host for SharedHost {
    fn native(&mut self, call: Call<'_>) -> Result<NativeResult, String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.native(call)
    }
    fn set_var(&mut self, var: u8, value: i32, p: &mut [i32; 4], g: &mut Globals) -> Result<(), String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.set_var(var, value, p, g)
    }
    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.node(call)
    }
    fn ext(&mut self, call: ExtCall<'_>) -> Result<NativeResult, String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.ext(call)
    }
    fn scene(&mut self, call: SceneCall) -> Result<NativeResult, String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.scene(call)
    }
}

/// An event graph starts (`Host::scene` Enter, called by the VM / Rhai runtime for top-level and nested graphs):
/// players lose their input until [`end_event`] (the game's scene nesting count 0x03002A5C / world message 0x0F).
pub fn begin_event(w: &mut World) {
    // an event a script starts (run_event from a trigger's inline code: West City's world map circle -> s515) stops
    // the field like an event trigger / talk does at its fire (walking map NPCs go idle, the player stops)
    if w.scene.depth == 0 && w.field.as_ref().is_some_and(|f| f.host && !f.busy) {
        wf_core::fieldnpc::start_event(w);
    }
    w.scene.depth += 1;
    // the field holds still while an event runs: no triggers / talk, map NPCs freeze (their animations tick)
    wf_core::field::set_busy(w, true);
}

/// The engine's event thread finished: scene nesting -1; at 0 the scripted puppets are released.
pub fn end_event(w: &mut World) {
    w.scene.depth = w.scene.depth.saturating_sub(1);
    if w.scene.depth == 0 {
        scene::release_puppets(w);
        wf_core::field::set_busy(w, false);
    }
}

/// the field NPCs' catch-up update after an event released the field at the top of the frame (see `from_wait`):
/// those older than the event (updated after its scene object in the game)
fn release_field_npcs(w: &mut World, watermark: EntityId) {
    if w.field.is_none() {
        return;
    }
    for i in (0..w.ents.len()).rev() {
        let id = w.ents[i].id;
        if !w.ents[i].removed && matches!(&w.ents[i].body, wf_core::world::Body::Npc(n) if n.behaviour.is_some()) && !scene::is_puppet(w, id) && id <= watermark {
            wf_core::fieldnpc::release_update(w, i);
        }
    }
}

/// Per-frame poll of the pending action, after the World stepped. `Some(result)` = resume the thread now with
/// `result`; `None` = still waiting. Also ticks the script timers (call every frame).
pub fn poll(w: &mut World, s: &mut HostState) -> Option<Option<i32>> {
    if s.timers.countdown_on && s.timers.countdown > 0 {
        s.timers.countdown -= 1;
    }
    if s.timers.stopwatch_on {
        s.timers.stopwatch += 1;
    }
    if s.boost[0] > 0 {
        s.boost[0] -= 1;
    }
    s.from_wait = false;
    if matches!(s.pending, Some(Pending::Phases { .. })) {
        let r = natives::poll_phases(w, s);
        if r.is_some() {
            s.pending = None;
        }
        return r;
    }
    let r = match s.pending.as_mut()? {
        Pending::Frames { left, result } => {
            *left -= 1;
            (*left <= 0).then_some(*result)
        }
        Pending::Button => (w.scene.keys[0].pressed & A != 0).then_some(None),
        Pending::Director => {
            let done = scene::wait_done(w);
            s.from_wait = done;
            done.then_some(None)
        }
        Pending::Fade => scene::fade_done(w).then_some(None),
        Pending::Scroll => {
            let done = scene::scroll_done(w);
            s.from_scroll = done;
            done.then_some(None)
        }
        Pending::Dialog { choice } => {
            let choice = *choice;
            match scene::dialog_result(w) {
                Some(r) => {
                    if choice {
                        s.menu_result = r;
                    }
                    // a talk event's text box closes at the top of the frame too (4-27: Bulma walks in s193's exit
                    // frame)
                    s.from_wait = s.talk_event;
                    Some(None)
                }
                None => None,
            }
        }
        Pending::Menu { native, push } => {
            let (native, push) = (*native, *push);
            // Switch Character chose (2): the object world restored when the screen closes (its second leave
            // frame) rebuilds the leader for it, idle entered once (segment sb 21-14: one rand(255) then, none at
            // the fade-in's end)
            if native == 0x8d && !s.charsel_rebuilt && matches!(w.menu.phase, wf_core::menu::Phase::Leave { t } if t >= 1) && w.menu.result.as_ref().is_some_and(|r| r.code == 2) {
                s.charsel_rebuilt = true;
                s.boost[0] = 0;
                let c = w.story.current;
                let _ = natives::switch_character(w, c);
            }
            // the real screens (wf_core::menu): their effects are applied by the screen; the result after the fade-in
            if let Some(r) = wf_core::menu::take_result(w) {
                // Switch Character chose (2): its handler set the current character and cleared the timed buff
                // (GD+0x4DC); `0x080027DE` rebuilds the leader for it
                if native == 0x8d && r == 2 && !std::mem::take(&mut s.charsel_rebuilt) {
                    s.boost[0] = 0;
                    let c = w.story.current;
                    let _ = natives::switch_character(w, c);
                }
                return {
                    s.pending = None;
                    Some(push.then_some(r - 1))
                };
            }
            match scene::menu_result(w) {
                Some((r, log)) => {
                    natives::menu_closed(w, s, native, r, &log);
                    Some(push.then_some(r - 1))
                }
                None => None,
            }
        }
        Pending::Transition => w.field.as_ref().is_none_or(|f| f.mode == wf_core::field::Mode::Normal).then_some(None),
        Pending::Arrival => w.field.as_ref().is_none_or(|f| f.arrival.is_none()).then_some(None),
        Pending::Phases { .. } => None,
    };
    if r.is_some() {
        s.pending = None;
    }
    r
}

/// the frame's raw input of the leader (script queries; players' own input may be masked by a scene)
pub fn leader_pressed(w: &World) -> u16 {
    w.scene.keys[0].pressed
}
