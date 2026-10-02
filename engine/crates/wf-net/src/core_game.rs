//! wf-core's battle `Session` as a rollback [`Game`], and the match description every peer builds it from.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_core::battle::{BattleConfig, Phase};
use wf_core::content::GameData;
use wf_core::session::{Session, Snapshot};
use wf_view::{Sim, SimEvent};

use crate::game::Game;

/// Everything that defines a match (identical on every peer; the host sends it in the lobby's `start`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MatchConfig {
    pub battle: BattleConfig,
    /// players can hit each other (`wf_core::battle::set_pvp`)
    #[serde(default)]
    pub pvp: bool,
}

impl MatchConfig {
    /// co-op: the battle as given (2–4 players vs the phases' enemies; each enemy targets a player slot)
    pub fn coop(battle: BattleConfig) -> Self {
        MatchConfig { battle, pvp: false }
    }
    /// PvP: no enemies unless `with_enemies` (then everyone fights everyone)
    pub fn pvp(mut battle: BattleConfig, with_enemies: bool) -> Self {
        if !with_enemies {
            battle.phases = Vec::<Phase>::new();
        }
        MatchConfig { battle, pvp: true }
    }
    pub fn session(&self, data: Arc<GameData>) -> Session {
        let mut s = Session::new(data, self.battle.clone());
        if self.pvp {
            wf_core::battle::set_pvp(&mut s.world, true);
        }
        s
    }
}

/// The battle session + the presentation events of first-time frames (sounds) for the frontend.
pub struct CoreGame {
    pub s: Session,
    pub events: Vec<SimEvent>,
    /// keep presentation events (off in tests / headless runs that do not need them)
    pub collect_events: bool,
}

impl CoreGame {
    pub fn new(s: Session) -> Self {
        CoreGame { s, events: Vec::new(), collect_events: true }
    }
}

impl Game for CoreGame {
    type State = Snapshot;
    fn save(&self) -> Snapshot {
        self.s.snapshot()
    }
    fn load(&mut self, st: &Snapshot) {
        self.s.restore(st);
    }
    fn advance(&mut self, inputs: &[u16], first: bool) {
        if first && self.collect_events {
            // through the Sim adapter: also emits the arena song on frame 0
            Sim::step(&mut self.s, inputs, &mut self.events);
        } else {
            self.s.step(inputs);
        }
    }
    fn checksum(&self, st: &Snapshot) -> u64 {
        st.world.state_hash()
    }
    fn save_bytes(&self) -> Option<Vec<u8>> {
        Some(self.s.snapshot_bytes())
    }
    fn load_bytes(&mut self, b: &[u8]) -> bool {
        self.s.restore_bytes(b)
    }
}
