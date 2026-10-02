//! Engine-side model shared by every runtime prototype: a plain-data `World` (Clone = snapshot), plain-data
//! `Action` timelines (frame-spanning work), the game's PRNG, and a trace for differential checks.
//!
//! Script handlers run to completion. Anything that spans frames is an `Action` value handed to the engine
//! with `run(action)`; the engine steps timelines in `World::tick`. A rollback snapshot is `World::clone()`.

use std::cell::RefCell;
use std::collections::BTreeMap;

/// Frame-spanning engine work, as plain data. Integer-only.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Say { speaker: i64, text: i64 },
    Wait(i64),
    WalkTo { actor: i64, x: i64, y: i64 },
    Face { actor: i64, dir: i64 },
    Transform { actor: i64, form: i64 },
    PlaySfx(i64),
    StartBattle(i64),
    SetFlag(i64),
    /// An original native not yet named (raw id + args); the engine maps it.
    Op { id: i64, args: Vec<i64> },
    Sequence(Vec<Action>),
    Parallel(Vec<Action>),
    /// Continuation: run script handler `name` with `arg` when the timeline reaches this point.
    Call { name: String, arg: i64 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Actor {
    pub x: i64,
    pub y: i64,
    pub dir: i64,
    pub form: i64,
    pub state: i64,
}

/// One running timeline: a stack of (sequence, cursor) frames plus the active leaf's remaining frames.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Timeline {
    pub stack: Vec<(Vec<Action>, usize)>,
    pub leaf_left: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct World {
    pub frame: u64,
    pub rng: (u32, u32),
    pub flags: Vec<u64>,
    pub g: [i64; 4],
    pub p: [i64; 4],
    pub stats: [i64; 32],
    pub equipped: Vec<i64>,
    pub inventory: BTreeMap<i64, i64>,
    pub actors: Vec<Actor>,
    pub players: Vec<i64>,
    pub timelines: Vec<Timeline>,
    /// handler continuations requested by timelines this frame (name, arg)
    pub pending_calls: Vec<(String, i64)>,
    pub trace: Vec<String>,
}

impl Default for World {
    fn default() -> Self {
        World::new(50, 2)
    }
}

impl World {
    pub fn new(n_actors: usize, n_players: usize) -> World {
        let actors = (0..n_actors)
            .map(|i| Actor { x: (i as i64 * 37) % 240, y: (i as i64 * 53) % 160, dir: 0, form: 0, state: 0 })
            .collect();
        World {
            frame: 0,
            rng: (0x1234_5678, 0x9ABC_DEF0),
            flags: vec![0; 32],
            g: [0; 4],
            p: [0; 4],
            stats: [0; 32],
            equipped: vec![121, 144, 28],
            inventory: BTreeMap::new(),
            actors,
            players: (0..n_players as i64).collect(),
            timelines: Vec::new(),
            pending_calls: Vec::new(),
            trace: Vec::new(),
        }
    }

    /// Buu's Fury PRNG (0x0803FAAC), VERIFIED in re/buu/notes.md.
    pub fn next_rand(&mut self) -> u32 {
        let (s0, s1) = self.rng;
        let a = s0.wrapping_add(0x02CB_BC71) ^ s1;
        let b = s1.rotate_left(3).wrapping_add(0x0632_D80F) ^ a;
        self.rng = (a, b);
        a.wrapping_sub(b)
    }
    pub fn rand(&mut self, n: i64) -> i64 {
        (((self.next_rand() & 0xFFFF) as i64) * n) >> 16
    }
    pub fn chance(&mut self, pct: i64) -> bool {
        ((self.next_rand() & 0xFFFF) as i64) < ((pct * 0x28F5D) >> 8)
    }
    pub fn flag(&self, i: i64) -> bool {
        let i = i as usize;
        self.flags.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }
    pub fn set_flag(&mut self, i: i64, v: bool) {
        let i = i as usize;
        if i / 64 >= self.flags.len() {
            self.flags.resize(i / 64 + 1, 0);
        }
        if v {
            self.flags[i / 64] |= 1 << (i % 64);
        } else {
            self.flags[i / 64] &= !(1 << (i % 64));
        }
    }
    pub fn log(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.trace.push(format!("{} {}", self.frame, s));
    }

    pub fn run(&mut self, a: Action) {
        self.log(format!("run {a:?}"));
        self.timelines.push(Timeline { stack: vec![(vec![a], 0)], leaf_left: -1 });
    }

    /// Advance every timeline by one frame (deterministic order: creation order).
    pub fn tick(&mut self) {
        self.frame += 1;
        let mut tls = std::mem::take(&mut self.timelines);
        for tl in tls.iter_mut() {
            self.step_timeline(tl);
        }
        tls.retain(|t| !t.stack.is_empty());
        let mut newer = std::mem::take(&mut self.timelines);
        tls.append(&mut newer);
        self.timelines = tls;
    }

    fn step_timeline(&mut self, tl: &mut Timeline) {
        // budget prevents infinite zero-length loops
        for _ in 0..64 {
            let Some((seq, cur)) = tl.stack.last_mut() else { return };
            if *cur >= seq.len() {
                tl.stack.pop();
                if let Some((_, c)) = tl.stack.last_mut() {
                    *c += 1;
                }
                continue;
            }
            let a = seq[*cur].clone();
            match a {
                Action::Sequence(v) => {
                    tl.stack.push((v, 0));
                    continue;
                }
                Action::Parallel(v) => {
                    // simplification for the bench: spawn each branch as its own timeline, continue
                    for b in v {
                        self.timelines.push(Timeline { stack: vec![(vec![b], 0)], leaf_left: -1 });
                    }
                    *cur += 1;
                    continue;
                }
                _ => {}
            }
            if tl.leaf_left < 0 {
                tl.leaf_left = self.start_leaf(&a);
            }
            if tl.leaf_left > 0 {
                self.update_leaf(&a);
                tl.leaf_left -= 1;
                if tl.leaf_left > 0 {
                    return;
                }
            }
            tl.leaf_left = -1;
            if let Some((_, c)) = tl.stack.last_mut() {
                *c += 1;
            }
            return;
        }
    }

    /// Start a leaf action, return its duration in frames (0 = instant).
    fn start_leaf(&mut self, a: &Action) -> i64 {
        self.log(format!("start {a:?}"));
        match a {
            Action::Say { .. } => 30,
            Action::Wait(n) => *n,
            Action::WalkTo { actor, x, y } => {
                let ac = &self.actors[*actor as usize % self.actors.len()];
                (ac.x - x).abs().max((ac.y - y).abs())
            }
            Action::Face { actor, dir } => {
                let n = self.actors.len();
                self.actors[*actor as usize % n].dir = *dir;
                0
            }
            Action::Transform { actor, form } => {
                let n = self.actors.len();
                self.actors[*actor as usize % n].form = *form;
                20
            }
            Action::SetFlag(f) => {
                self.set_flag(*f, true);
                0
            }
            Action::Call { name, arg } => {
                self.pending_calls.push((name.clone(), *arg));
                0
            }
            Action::Op { args, .. } => args.len() as i64,
            _ => 0,
        }
    }

    fn update_leaf(&mut self, a: &Action) {
        if let Action::WalkTo { actor, x, y } = a {
            let n = self.actors.len();
            let ac = &mut self.actors[*actor as usize % n];
            ac.x += (x - ac.x).signum();
            ac.y += (y - ac.y).signum();
        }
    }

    /// FNV-1a hash of the trace plus the observable world state.
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |b: &[u8]| {
            for x in b {
                h ^= *x as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        };
        for l in &self.trace {
            eat(l.as_bytes());
            eat(b"\n");
        }
        eat(format!("{:?}{:?}{:?}{:?}{:?}{:?}", self.actors, self.flags, self.g, self.stats, self.inventory, self.rng).as_bytes());
        h
    }
}

thread_local! {
    static WORLD: RefCell<World> = RefCell::new(World::default());
}

/// Access the world from host functions (single-threaded; scripts run on the sim thread).
pub fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> R {
    WORLD.with(|w| f(&mut w.borrow_mut()))
}
pub fn set_world(w: World) {
    WORLD.with(|x| *x.borrow_mut() = w);
}
pub fn take_world() -> World {
    WORLD.with(|x| x.borrow().clone())
}

/// Immediate (same-frame) host functions shared by all bindings.
pub mod api {
    use super::*;
    pub fn get_flag(i: i64) -> bool {
        with_world(|w| w.flag(i))
    }
    pub fn set_flag(i: i64) {
        with_world(|w| {
            w.log(format!("set_flag {i}"));
            w.set_flag(i, true)
        })
    }
    pub fn rand(n: i64) -> i64 {
        with_world(|w| w.rand(n))
    }
    pub fn chance(p: i64) -> bool {
        with_world(|w| w.chance(p))
    }
    pub fn player(n: i64) -> i64 {
        with_world(|w| w.players.get((n - 1) as usize).copied().unwrap_or(-1))
    }
    pub fn actor_x(a: i64) -> i64 {
        with_world(|w| w.actors.get(a as usize).map_or(0, |x| x.x))
    }
    pub fn actor_y(a: i64) -> i64 {
        with_world(|w| w.actors.get(a as usize).map_or(0, |x| x.y))
    }
    pub fn set_state(a: i64, s: i64) {
        with_world(|w| {
            if let Some(x) = w.actors.get_mut(a as usize) {
                x.state = s
            }
        })
    }
    pub fn give_item(item: i64, n: i64) {
        with_world(|w| {
            w.log(format!("give_item {item} {n}"));
            *w.inventory.entry(item).or_insert(0) += n
        })
    }
    pub fn is_equipped(item: i64) -> bool {
        with_world(|w| w.equipped.contains(&item))
    }
    pub fn stat(i: i64) -> i64 {
        with_world(|w| w.stats[i as usize & 31])
    }
    pub fn set_stat(i: i64, v: i64) {
        with_world(|w| {
            w.log(format!("set_stat {i} {v}"));
            w.stats[i as usize & 31] = v
        })
    }
    pub fn var_g(i: i64) -> i64 {
        with_world(|w| w.g[i as usize & 3])
    }
    pub fn set_g(i: i64, v: i64) {
        with_world(|w| w.g[i as usize & 3] = v)
    }
    pub fn var_p(i: i64) -> i64 {
        with_world(|w| w.p[i as usize & 3])
    }
    pub fn set_p(i: i64, v: i64) {
        with_world(|w| w.p[i as usize & 3] = v)
    }
    /// immediate original native without a frame-spanning part (logged)
    pub fn op_now(id: i64, args: &[i64]) {
        with_world(|w| w.log(format!("op_now {id:#x} {args:?}")))
    }
    pub fn run(a: Action) {
        with_world(|w| w.run(a))
    }
    /// Loot sub-script stand-in (#500 zenie, #496 equipment, #499 supply): host-simulated.
    pub fn run_script(idx: i64, p0: i64) {
        with_world(|w| {
            let r = w.rand(4);
            w.log(format!("run_script {idx} p0={p0} r={r}"));
            if idx == 500 {
                w.g[1] = 1;
                w.g[2] = 1;
            }
        })
    }
}

/// The benchmark scenario, driven identically for each runtime through this trait.
pub trait Runtime {
    fn name(&self) -> &'static str;
    /// call a handler with one integer argument; returns its integer result
    fn call(&mut self, handler: &str, arg: i64) -> i64;
}

/// Deterministic scenario: item use, loot rolls, the Janemba handover (with a continuation), and N frames of
/// per-actor updates. Returns the final world digest.
pub fn scenario(rt: &mut dyn Runtime, frames: u64) -> u64 {
    scenario_setup(rt);
    for _ in 0..frames {
        frame(rt);
    }
    with_world(|w| w.digest())
}

/// One-shot events: item use, three loot rolls, the Janemba handover.
pub fn scenario_setup(rt: &mut dyn Runtime) {
    set_world(World::new(50, 2));
    rt.call("item_use", 0);
    for lvl in [3, 20, 60] {
        with_world(|w| w.p = [lvl, -1, 0, 0]);
        rt.call("loot", lvl);
    }
    let r = rt.call("janemba_handover", 0);
    with_world(|w| w.log(format!("handover returned {r}")));
}

/// One simulation frame: per-actor handlers, timeline step, timeline continuations.
pub fn frame(rt: &mut dyn Runtime) {
    let n = with_world(|w| w.actors.len()) as i64;
    let mut acc = 0;
    for a in 0..n {
        acc += rt.call("on_update", a);
    }
    let calls = with_world(|w| {
        w.tick();
        std::mem::take(&mut w.pending_calls)
    });
    for (name, arg) in calls {
        rt.call(&name, arg);
    }
    with_world(|w| {
        if w.frame % 16 == 0 {
            w.log(format!("acc {acc}"));
        }
    });
}

/// Same Rust logic as the scripts' on_update (baseline for overhead numbers).
pub struct NativeRt;
impl Runtime for NativeRt {
    fn name(&self) -> &'static str {
        "native-rust"
    }
    fn call(&mut self, handler: &str, a: i64) -> i64 {
        if handler != "on_update" {
            return 0;
        }
        let pl = api::player(1);
        let d = (api::actor_x(a) - api::actor_x(pl)).abs() + (api::actor_y(a) - api::actor_y(pl)).abs();
        if d < 64 {
            api::set_state(a, 1);
            if api::chance(10) {
                api::run(Action::Face { actor: a, dir: 3 });
            }
        } else {
            api::set_state(a, 0);
        }
        d
    }
}

/// Only per-frame updates (comparable across all runtimes, incl. the partial mini VM).
pub fn updates_only(rt: &mut dyn Runtime, frames: u64) -> u64 {
    set_world(World::new(50, 2));
    for _ in 0..frames {
        let n = with_world(|w| w.actors.len()) as i64;
        let mut acc = 0;
        for a in 0..n {
            acc += rt.call("on_update", a);
        }
        with_world(|w| {
            w.tick();
            w.log(format!("acc {acc}"));
        });
    }
    with_world(|w| w.digest())
}
