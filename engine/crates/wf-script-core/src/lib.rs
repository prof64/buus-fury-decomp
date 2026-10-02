//! `wf-script-core`: the seam between the scripting layer (`wf-script` VM, `wf-script-rhai` runtime) and the
//! deterministic simulation core (`wf-core`). Spec: `engine/docs/script-host.md`.
//!
//! * [`CoreHost`] implements [`wf_script::Host`] over `&mut World` + `&mut StoryState` (borrowed: the engine
//!   calls it inside a frame); [`SharedHost`] is the owned form the Rhai runtime needs (`Box<dyn Host>`).
//! * [`StoryState`] holds the game state the battle core does not model yet (story flags, inventory, zenie,
//!   party roster, timers, the cutscene director flag, pending pickups). It is plain data, snapshotted with
//!   the World; the doc lists which parts should move into `wf-core`.
//! * Blocking: a native that spans frames records a [`Pending`] in `StoryState` and returns `Block`; every
//!   frame the engine calls [`poll`], which says when to resume the thread and with what value.
//! * Multiplayer: actor 0 = the leader = `World.players[0]`; `player(n)` = `World.players[n-1]`.

pub mod binding;

use binding::{binding, Where};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use wf_core::input::A;
use wf_core::world::{Body, EntityId, Event, World};
use wf_script::vm::{Call, Globals, Host, NativeResult, NodeCall};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Director {
    /// `begin_parallel` is open.
    pub batch: bool,
    /// `wait_for_actor` group.
    pub group: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DropKind {
    Zenie(i32),
    Supply(i32),
    Equipment(i32),
}

/// A pickup the core should spawn at the loot position (Body::Pickup does not exist in wf-core yet).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DropRequest {
    pub kind: DropKind,
    pub x: i32,
    pub y: i32,
}

/// Something the presentation layer shows (dialog box, choice, item-get popup).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Present {
    Text { text: String, portrait: u8, args: Vec<i32> },
    Choice { text: String, portrait: u8 },
    ItemGet { item: i32, equipment: bool },
}

/// A frame-spanning action in progress (the GBA's nested main loop).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pending {
    /// `wait(frames)` and other fixed-length actions.
    Frames { left: i32, result: Option<i32> },
    /// `wait_button()`, dialog boxes: until the leader presses A.
    Button,
    /// A choice menu: the leader moves the cursor and confirms with A (result stored in `menu_result`).
    Choice { options: i32, cursor: i32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timers {
    pub countdown: i32,
    pub countdown_on: bool,
    pub stopwatch: i32,
    pub stopwatch_on: bool,
}

/// Game state the scripts need that `wf-core::World` does not model (yet). Plain data, part of the snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoryState {
    pub flags: BTreeSet<i32>,
    pub l: [i32; 4],
    pub zenie: i32,
    pub variant_byte: i32,
    /// (world, map, variant)
    pub area: (i32, i32, i32),
    pub menu_result: i32,
    pub supplies: BTreeMap<i32, i32>,
    /// owned equipment ids (one entry per copy)
    pub equipment: Vec<i32>,
    pub party: BTreeSet<i32>,
    pub char_flags: BTreeSet<i32>,
    pub char_form: BTreeMap<i32, i32>,
    pub game_bits: BTreeSet<i32>,
    pub timers: Timers,
    pub shop: Vec<[i32; 4]>,
    pub eq_accum: [i32; 16],
    pub director: Director,
    /// loot position (0x03001010), set by the dying enemy
    pub loot_pos: (i32, i32),
    pub drops: Vec<DropRequest>,
    pub present: Vec<Present>,
    pub pending: Option<Pending>,
}

/// Owned state for [`SharedHost`].
pub struct CoreState {
    pub world: World,
    pub story: StoryState,
}

/// Borrowed host: the engine builds one per frame / per script call.
pub struct CoreHost<'a> {
    pub w: &'a mut World,
    pub s: &'a mut StoryState,
}

/// Owned host for `wf_script_rhai::Runtime` (which needs `Box<dyn Host>`).
#[derive(Clone)]
pub struct SharedHost(pub Rc<RefCell<CoreState>>);

fn missing<T>(n: u8) -> Result<T, String> {
    Err(format!("native 0x{n:02x} {}: not available in wf-core yet ({})", binding::name(n), binding(n).note))
}

/// Entity index of the leader (actor 0 = `World.players[0]`).
pub fn leader(w: &World) -> Option<usize> {
    w.players.first().and_then(|&id| w.find(id))
}

fn lead(w: &World, n: u8) -> Result<usize, String> {
    leader(w).ok_or_else(|| format!("native 0x{n:02x}: no player in the World"))
}

/// `player(n)` (1-based): entity id of player n.
pub fn player(w: &World, n: i32) -> Option<EntityId> {
    if n < 1 {
        return None;
    }
    w.players.get(n as usize - 1).copied()
}

fn char_of_form(w: &World, form: i32) -> Option<i32> {
    w.data.form(form).map(|f| f.char_)
}

/// Resolve a script actor id (`0x080001C6`) to an entity index: 0 leader; 1..8 the player whose character is
/// id-1; >= 10 the first live entity with that sprite; negative (map placements) and 9 are not modelled.
pub fn resolve(w: &World, id: i32) -> Option<usize> {
    match id {
        0 => leader(w),
        1..=8 => w.players.iter().filter_map(|&p| w.find(p)).find(|&i| {
            matches!(&w.ents[i].body, Body::Player(p) if char_of_form(w, p.form) == Some(id - 1))
        }),
        10.. => (0..w.ents.len()).find(|&i| !w.ents[i].removed && w.actor(i).and_then(|a| a.sprite) == Some(id)),
        _ => None,
    }
}

fn count_actors(w: &World, id: i32) -> i32 {
    match id {
        0..=8 => resolve(w, id).is_some() as i32,
        10.. => (0..w.ents.len()).filter(|&i| !w.ents[i].removed && w.actor(i).and_then(|a| a.sprite) == Some(id)).count() as i32,
        _ => 0,
    }
}

fn arg(c: &Call, i: usize) -> i32 {
    c.args.get(i).copied().unwrap_or(0)
}

impl<'a> CoreHost<'a> {
    fn block(&mut self, p: Pending) -> Result<NativeResult, String> {
        self.s.pending = Some(p);
        Ok(NativeResult::Block)
    }

    /// Queued cutscene commands and item-get popups block outside a director batch.
    fn queued(&mut self, frames: i32) -> Result<NativeResult, String> {
        if self.s.director.batch {
            Ok(NativeResult::Done(None))
        } else {
            self.block(Pending::Frames { left: frames, result: None })
        }
    }

    pub fn call(&mut self, c: &Call) -> Result<NativeResult, String> {
        let n = c.native;
        let done = |v: i32| Ok(NativeResult::Done(Some(v)));
        let unit = || Ok(NativeResult::Done(None));
        let s = &mut *self.s;
        match n {
            0x08..=0x0b => done(s.l[n as usize - 8]),
            0x0c | 0x0d | 0x0f | 0x0e | 0xc3 | 0xc8 => {
                let i = lead(self.w, n)?;
                let p = self.w.pl_ref(i);
                let f = self.w.data.form(p.form);
                done(match n {
                    0x0c => p.rec.hp,
                    0x0d => p.rec.max_hp,
                    0x0f => p.rec.max_ep,
                    0xc3 => p.rec.xp as i32,
                    0xc8 => p.rec.level,
                    _ => match f {
                        Some(f) if f.ss3 => p.ss3_gauge as i32,
                        Some(f) if f.ss => p.ss_gauge,
                        _ => p.rec.ep,
                    },
                })
            }
            0x10 => done(s.zenie),
            0x11 => done(s.variant_byte),
            0x12 => done(s.area.0),
            0x13 => done(s.area.1),
            0x14 => done(s.area.2),
            0x15 => {
                let i = lead(self.w, n)?;
                done(char_of_form(self.w, self.w.pl_ref(i).form).unwrap_or(0))
            }
            0x18 => done(s.menu_result),
            0x19 => done(self.w.rng.rand(arg(c, 0) as u32)),
            0x1a => done(self.w.rng.chance((arg(c, 0).wrapping_mul(0x28F5D) >> 8) as i64) as i32),
            0x1b => done(s.flags.contains(&arg(c, 0)) as i32),
            0x1c => done(!s.flags.contains(&arg(c, 0)) as i32),
            0x1d => {
                s.flags.insert(arg(c, 0));
                unit()
            }
            0x1e => {
                s.flags.remove(&arg(c, 0));
                unit()
            }
            0x1f => {
                self.w.sfx(arg(c, 0), 64);
                unit()
            }
            0x20 => {
                self.w.sfx(arg(c, 0), arg(c, 1));
                unit()
            }
            0x21 => {
                self.w.sfx_stop(arg(c, 0));
                unit()
            }
            0x23 => {
                self.w.events.push(Event::Song { id: arg(c, 0) });
                unit()
            }
            0x37 => done(count_actors(self.w, arg(c, 0))),
            0x38 => {
                s.director.batch = true;
                unit()
            }
            0x39 => {
                s.director.group.push(arg(c, 0));
                unit()
            }
            0x7f => done(*s.supplies.get(&arg(c, 0)).unwrap_or(&0)),
            0x80 => {
                *s.supplies.entry(arg(c, 0)).or_default() += 1;
                s.present.push(Present::ItemGet { item: arg(c, 0), equipment: false });
                self.queued(60)
            }
            0x81 | 0x83 => {
                let d = if n == 0x81 { arg(c, 1) } else { -arg(c, 1) };
                let e = s.supplies.entry(arg(c, 0)).or_default();
                *e = (*e + d) & 0xff; // u8 counter, no underflow check (like the game)
                unit()
            }
            0x82 => {
                let e = s.supplies.entry(arg(c, 0)).or_default();
                *e = (*e - 1) & 0xff;
                unit()
            }
            0x84..=0x87 => {
                let (kind, (x, y)) = match n {
                    0x84 => (DropKind::Zenie(arg(c, 0)), s.loot_pos),
                    0x85 => (DropKind::Supply(arg(c, 0)), s.loot_pos),
                    0x86 => (DropKind::Equipment(arg(c, 0)), s.loot_pos),
                    _ => (DropKind::Zenie(arg(c, 2)), (arg(c, 0), arg(c, 1))),
                };
                s.drops.push(DropRequest { kind, x, y });
                unit()
            }
            0x88 => self.block(Pending::Frames { left: arg(c, 0), result: None }),
            0x89 => self.block(Pending::Button),
            0x92 => done(s.equipment.iter().filter(|&&e| e == arg(c, 0)).count() as i32),
            0x93 => {
                s.equipment.push(arg(c, 0));
                s.present.push(Present::ItemGet { item: arg(c, 0), equipment: true });
                self.queued(60)
            }
            0x94 => {
                if let Some(k) = s.equipment.iter().position(|&e| e == arg(c, 0)) {
                    s.equipment.remove(k);
                }
                unit()
            }
            0x95 | 0xd0 => {
                let i = lead(self.w, n)?;
                let form = if n == 0xd0 {
                    arg(c, 0)
                } else {
                    let ch = arg(c, 0);
                    match s.char_form.get(&ch) {
                        Some(&f) => f,
                        None => self.w.data.chars.iter().find(|x| x.index == ch).map(|x| x.form).ok_or("character")?,
                    }
                };
                if self.w.data.form(form).is_none() {
                    return Err(format!("native 0x{n:02x}: unknown form {form}"));
                }
                let mut p = self.w.pl_ref(i).clone();
                p.apply_form(self.w, form);
                *self.w.pl(i) = p;
                unit()
            }
            0x96 => done(s.party.contains(&arg(c, 0)) as i32),
            0x97 => {
                s.party.insert(arg(c, 0));
                unit()
            }
            0x98 => {
                s.party.remove(&arg(c, 0));
                unit()
            }
            0x99 => {
                s.char_flags.insert(arg(c, 0));
                unit()
            }
            0x9a => {
                s.char_flags.remove(&arg(c, 0));
                unit()
            }
            0x9b => {
                let i = lead(self.w, n)?;
                let f = self.w.data.form(self.w.pl_ref(i).form);
                done(f.map(|f| f.ss || f.ss3).unwrap_or(false) as i32)
            }
            0x9c => {
                let ch = arg(c, 0);
                let form = s.char_form.get(&ch).copied().or_else(|| self.w.data.chars.iter().find(|x| x.index == ch).map(|x| x.form));
                done(form.and_then(|f| self.w.data.form(f)).map(|f| f.gfx - 9).unwrap_or(0))
            }
            0x9d => {
                s.char_form.insert(arg(c, 0), arg(c, 1));
                unit()
            }
            0xa1..=0xa3 => {
                let Some(i) = resolve(self.w, arg(c, 0)) else { return done(0) };
                let a = self.w.actor(i).ok_or("actor")?;
                done(match n {
                    0xa1 => a.px(),
                    0xa2 => a.py(),
                    _ => a.facing as i32,
                })
            }
            0xae => {
                s.timers.countdown = arg(c, 0) * 60;
                s.timers.countdown_on = true;
                unit()
            }
            0xaf => {
                s.timers.countdown_on = false;
                unit()
            }
            0xbe => {
                s.timers.stopwatch = 0;
                unit()
            }
            0xbf | 0xc0 => {
                s.timers.stopwatch_on = n == 0xbf;
                unit()
            }
            0xc1 => done(s.timers.stopwatch),
            0xb0 | 0xb1 | 0xbc | 0xbd => unit(), // HUD visibility of the timers: presentation only
            0xba => {
                let (first, cnt) = (arg(c, 0), arg(c, 1));
                done((0..cnt).fold(0, |v, k| v | ((s.flags.contains(&(first + k)) as i32) << k)))
            }
            0xbb => {
                let (first, cnt, v) = (arg(c, 0), arg(c, 1), arg(c, 2));
                for k in 0..cnt {
                    if v >> k & 1 == 1 {
                        s.flags.insert(first + k);
                    } else {
                        s.flags.remove(&(first + k));
                    }
                }
                unit()
            }
            0xc2 => {
                let i = lead(self.w, n)?;
                wf_core::player::gain_xp(self.w, i, arg(c, 0) as i64);
                unit()
            }
            0xc4 | 0xc5 => {
                if n == 0xc4 {
                    s.game_bits.insert(0x20);
                } else {
                    s.game_bits.remove(&0x20);
                }
                unit()
            }
            0xeb => {
                s.game_bits.insert(0x40);
                unit()
            }
            0xdb => {
                if arg(c, 0) != 0 {
                    s.game_bits.insert(0x100);
                } else {
                    s.game_bits.remove(&0x100);
                }
                unit()
            }
            0xca => {
                let ids = self.w.players.clone();
                for id in ids {
                    if let Some(i) = self.w.find(id) {
                        let p = self.w.pl(i);
                        p.rec.hp = p.rec.max_hp;
                        p.rec.ep = p.rec.max_ep;
                    }
                }
                unit()
            }
            0xcd => {
                s.shop.push([arg(c, 0), arg(c, 1), arg(c, 2), arg(c, 3)]);
                unit()
            }
            0xd2 => done((self.w.input[0].pressed & A != 0) as i32),
            0xd5 => {
                let i = lead(self.w, n)?;
                done(self.w.pl_ref(i).equip.contains(&arg(c, 0)) as i32)
            }
            0xdd => done(self.w.players.iter().filter_map(|&id| self.w.find(id)).map(|i| self.w.pl_ref(i).rec.xp as i32).sum()),
            0xdf..=0xe8 | 0xf5 => {
                let k = if n == 0xf5 { 15 } else { (n - 0xdf) as usize };
                s.eq_accum[k] = s.eq_accum[k].wrapping_add(c.args.first().copied().unwrap_or(1));
                unit()
            }
            0xed | 0xee => {
                let Some(i) = resolve(self.w, arg(c, 0) + 1) else { return if n == 0xed { done(0) } else { unit() } };
                if n == 0xed {
                    done(self.w.pl_ref(i).rec.points)
                } else {
                    self.w.pl(i).rec.points += arg(c, 1);
                    unit()
                }
            }
            0xf1 => {
                let Some(i) = resolve(self.w, arg(c, 0) + 1) else { return unit() };
                let slot = arg(c, 1) as usize & 3;
                let old = self.w.pl_ref(i).equip[slot];
                if old != 0 {
                    s.equipment.push(old);
                    let mut p = self.w.pl_ref(i).clone();
                    p.equip[slot] = 0;
                    p.refresh_stats(self.w);
                    *self.w.pl(i) = p;
                }
                unit()
            }
            0xf2 => {
                let i = lead(self.w, n)?;
                let mut p = self.w.pl_ref(i).clone();
                if arg(c, 0) == 0 {
                    p.rec.str_ += arg(c, 1);
                    p.rec.pow += arg(c, 2);
                    p.rec.end += arg(c, 3);
                    p.rec.spd += arg(c, 4);
                } else {
                    p.holy_left = arg(c, 0) * 60; // timed boost values themselves: MISSING in EffStats
                }
                p.refresh_stats(self.w);
                *self.w.pl(i) = p;
                unit()
            }
            0xf3 => {
                let Some(i) = resolve(self.w, arg(c, 0) + 1) else { return unit() };
                let target = arg(c, 1).min(200);
                let old = self.w.pl_ref(i).rec.level;
                if target > old {
                    let ch = char_of_form(self.w, self.w.pl_ref(i).form);
                    let g = self.w.data.chars.iter().find(|x| Some(x.index) == ch).map(|x| x.growth).ok_or("character")?;
                    let mut rec = self.w.pl_ref(i).rec.clone();
                    wf_core::progression::level_up(&mut rec, &g, old, target, &mut self.w.rng);
                    if let Some(&t) = self.w.data.xp_table.get((target - 50).max(0) as usize) {
                        rec.xp = t;
                    }
                    let mut p = self.w.pl_ref(i).clone();
                    p.rec = rec;
                    p.refresh_stats(self.w);
                    *self.w.pl(i) = p;
                }
                unit()
            }
            0xf6 => done(0),
            _ => match binding(n).at {
                Where::Vm => Err(format!("native 0x{n:02x} is handled by the VM")),
                _ => missing(n),
            },
        }
    }

    pub fn set(&mut self, var: u8, v: i32) -> Result<(), String> {
        match var {
            8..=11 => self.s.l[var as usize - 8] = v,
            12..=15 => {
                let i = lead(self.w, var)?;
                let p = self.w.pl(i);
                match var {
                    12 => p.rec.hp = v.clamp(0, p.rec.max_hp),
                    13 => {
                        p.rec.max_hp = v;
                        p.rec.hp = v;
                    }
                    14 => p.rec.ep = v.clamp(0, p.rec.max_ep),
                    _ => {
                        p.rec.max_ep = v;
                        p.rec.ep = v;
                    }
                }
            }
            16 => self.s.zenie = v.max(0),
            17 => self.s.variant_byte = v & 0xff,
            _ => return Err(format!("setter {var} out of range")),
        }
        Ok(())
    }

    pub fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        match call {
            NodeCall::Text { text, arg, args, .. } => {
                self.s.present.push(Present::Text { text, portrait: arg, args });
                self.block(Pending::Button)
            }
            NodeCall::Choice { text, arg, .. } => {
                let options = text.lines().filter(|l| l.starts_with('-')).count() as i32;
                self.s.present.push(Present::Choice { text, portrait: arg });
                self.block(Pending::Choice { options: options.max(1), cursor: 0 })
            }
        }
    }
}

impl Host for CoreHost<'_> {
    fn native(&mut self, call: Call<'_>) -> Result<NativeResult, String> {
        self.call(&call)
    }
    fn set_var(&mut self, var: u8, value: i32, _p: &mut [i32; 4], _g: &mut Globals) -> Result<(), String> {
        self.set(var, value)
    }
    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        CoreHost::node(self, call)
    }
}

impl Host for SharedHost {
    fn native(&mut self, call: Call<'_>) -> Result<NativeResult, String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.call(&call)
    }
    fn set_var(&mut self, var: u8, value: i32, _p: &mut [i32; 4], _g: &mut Globals) -> Result<(), String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.set(var, value)
    }
    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        let mut st = self.0.borrow_mut();
        let CoreState { world, story } = &mut *st;
        CoreHost { w: world, s: story }.node(call)
    }
}

/// Per-frame poll of the pending action (call after `World` stepped). `Some(result)` = resume the thread now
/// with `result`; `None` = still waiting. Also ticks the script timers.
pub fn poll(w: &World, s: &mut StoryState) -> Option<Option<i32>> {
    if s.timers.countdown_on && s.timers.countdown > 0 {
        s.timers.countdown -= 1;
    }
    if s.timers.stopwatch_on {
        s.timers.stopwatch += 1;
    }
    let a = w.input[0].pressed & A != 0;
    let r = match s.pending.as_mut()? {
        Pending::Frames { left, result } => {
            *left -= 1;
            if *left <= 0 {
                Some(*result)
            } else {
                None
            }
        }
        Pending::Button => a.then_some(None),
        Pending::Choice { options, cursor } => {
            let p = w.input[0].pressed;
            if p & wf_core::input::DOWN != 0 {
                *cursor = (*cursor + 1) % *options;
            }
            if p & wf_core::input::UP != 0 {
                *cursor = (*cursor + *options - 1) % *options;
            }
            if a {
                s.menu_result = *cursor;
                Some(None)
            } else {
                None
            }
        }
    };
    if r.is_some() {
        s.pending = None;
    }
    r
}
