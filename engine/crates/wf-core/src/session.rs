//! The headless session: the one API every consumer (frontend adapter, agents, tests, netcode) drives.
//!
//! `reset(config)` builds a battle, `step(inputs)` advances one frame with every player's held buttons and returns the
//! frame's events, `observe(player)` gives a structured observation, `snapshot` / `restore` copy the whole state
//! (a `World` clone: plain data + one Arc to the static game data), `hash` is the per-frame state hash.

use crate::battle::{self, BattleConfig, Outcome};
use crate::content::GameData;
use crate::input::InputFrame;
use crate::observe::{observe, Observation};
use crate::world::{Event, World, MAX_PLAYERS};
use std::sync::Arc;

#[derive(Clone)]
pub struct Snapshot {
    pub world: World,
    pub prev: [u16; MAX_PLAYERS],
    pub outcome: Outcome,
}

pub struct Session {
    pub data: Arc<GameData>,
    pub world: World,
    pub config: BattleConfig,
    /// held buttons of the previous frame per player (edges)
    pub prev: [u16; MAX_PLAYERS],
    pub outcome: Outcome,
}

impl Session {
    pub fn new(data: Arc<GameData>, config: BattleConfig) -> Session {
        let world = battle::create(Arc::clone(&data), &config);
        Session { data, world, config, prev: [0; MAX_PLAYERS], outcome: Outcome::Running }
    }
    pub fn reset(&mut self, config: BattleConfig) {
        *self = Session::new(Arc::clone(&self.data), config);
    }
    /// one frame with every player's held buttons (missing players = 0)
    pub fn step(&mut self, held: &[u16]) -> &[Event] {
        let mut inputs = [InputFrame::default(); MAX_PLAYERS];
        for (s, inp) in inputs.iter_mut().enumerate() {
            let h = held.get(s).copied().unwrap_or(0);
            *inp = InputFrame::from_held(h, self.prev[s]);
            self.prev[s] = h;
        }
        self.step_frames(inputs)
    }
    /// one frame with explicit input frames (edges given)
    pub fn step_frames(&mut self, inputs: [InputFrame; MAX_PLAYERS]) -> &[Event] {
        let w = &mut self.world;
        w.events.clear();
        w.input = inputs;
        battle::pre_step(w);
        w.step_entities();
        battle::post_step(w);
        if self.outcome == Outcome::Running {
            self.outcome = battle::outcome(w);
        }
        &self.world.events
    }
    pub fn observe(&self, player: usize) -> Observation {
        observe(&self.world, player)
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { world: self.world.clone(), prev: self.prev, outcome: self.outcome }
    }
    pub fn restore(&mut self, s: &Snapshot) {
        self.world = s.world.clone();
        self.prev = s.prev;
        self.outcome = s.outcome;
    }
    pub fn hash(&self) -> u64 {
        self.world.state_hash()
    }
    pub fn frame(&self) -> u32 {
        self.world.frame
    }
}
