//! wf-core's battle `Session` as a rollback [`Game`], the match description every peer builds it from, and the
//! rollback-aware sounds.
//!
//! **Sounds under rollback** ([`SfxMode`]): a frame's sounds are known when it is simulated, but a rollback can
//! change them (a predicted hit that did not happen, a real one that was not predicted). The default
//! [`SfxMode::Reconcile`] does what rollback fighting games do: play the sounds of a frame when it is first
//! simulated and remember them per frame; when a rollback re-simulates the frame, compare: sounds that no longer
//! happen are stopped (`StopSfx`, if still recent), sounds that happen now and did not before are played late (if
//! at most `late_frames` old), identical ones are left alone. [`SfxMode::Confirmed`] only plays the sounds of
//! frames whose inputs are all known (never wrong, ~one round trip late); [`SfxMode::Predicted`] is the old
//! behaviour (first simulation only, re-simulations silent).
//!
//! **Online pause**: an input with [`PAUSE_BIT`] (not a GBA button) on any slot makes that frame a no-op for the
//! world: the match stands still for everyone, deterministically, without stalling the rollback session (inputs,
//! acks, hashes keep flowing). Used by the room's "shared pause" setting.

use std::collections::VecDeque;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_core::battle::{BattleConfig, Phase};
use wf_core::content::GameData;
use wf_core::session::{Session, Snapshot};
use wf_view::{AudioEvent, Sim, SimEvent};

use crate::game::Game;

/// Input bit (above the 10 GBA buttons) of a player holding the shared pause.
pub const PAUSE_BIT: u16 = 1 << 15;
/// The GBA buttons.
pub const BUTTONS_MASK: u16 = 0x03FF;

pub fn paused(inputs: &[u16]) -> bool {
    inputs.iter().any(|&v| v & PAUSE_BIT != 0)
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SfxMode {
    /// play first-time frames' sounds, fix them up when a rollback changes them (default)
    #[default]
    Reconcile,
    /// only sounds of frames with all inputs known (never wrong, a round trip late)
    Confirmed,
    /// first simulation only, rollbacks silent (wrong sounds stay wrong)
    Predicted,
}

impl SfxMode {
    pub fn parse(s: &str) -> Option<SfxMode> {
        match s {
            "reconcile" => Some(SfxMode::Reconcile),
            "confirmed" => Some(SfxMode::Confirmed),
            "predicted" => Some(SfxMode::Predicted),
            _ => None,
        }
    }
}

/// What the sound reconciliation did (overlay, tests).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SfxStats {
    /// sounds played on first simulation
    pub played: u64,
    /// sounds a rollback added, played late
    pub late: u64,
    /// ... too old to play
    pub dropped: u64,
    /// sounds a rollback removed, stopped
    pub cancelled: u64,
}

/// One sound decision with the frame it belongs to (tests compare these with a straight replay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfxAudit {
    Play(i32, AudioEvent),
    Stop(i32, AudioEvent),
}

/// The battle session + the presentation events of first-time frames (sounds) for the frontend.
pub struct CoreGame {
    pub s: Session,
    pub events: Vec<SimEvent>,
    /// keep presentation events (off in tests / headless runs that do not need them)
    pub collect_events: bool,
    pub sfx: SfxMode,
    /// re-simulated sounds at most this many frames old are still played (~100 ms)
    pub late_frames: i32,
    /// removed sounds at most this many frames old are stopped
    pub stop_frames: i32,
    pub sfx_stats: SfxStats,
    /// every sound decision (tests)
    pub audit: Option<Vec<SfxAudit>>,
    /// session index of the current state
    cur: i32,
    /// highest index simulated + 1
    high: i32,
    quiet: bool,
    /// sounds per frame index: what was played (Reconcile) / what the newest simulation produced (Confirmed)
    log: VecDeque<(i32, Vec<AudioEvent>)>,
    /// Confirmed: frames below this were released
    released: i32,
}

impl CoreGame {
    pub fn new(s: Session) -> Self {
        CoreGame {
            s,
            events: Vec::new(),
            collect_events: true,
            sfx: SfxMode::default(),
            late_frames: 6,
            stop_frames: 30,
            sfx_stats: SfxStats::default(),
            audit: None,
            cur: 0,
            high: 0,
            quiet: false,
            log: VecDeque::new(),
            released: 0,
        }
    }
    pub fn frame_index(&self) -> i32 {
        self.cur
    }

    fn log_get(&self, f: i32) -> Option<&Vec<AudioEvent>> {
        self.log.iter().rev().find(|(k, _)| *k == f).map(|(_, v)| v)
    }
    fn log_set(&mut self, f: i32, v: Vec<AudioEvent>) {
        if let Some(e) = self.log.iter_mut().rev().find(|(k, _)| *k == f) {
            e.1 = v;
        } else {
            // frames come in ascending order except after a rollback (then the frame is already there)
            self.log.push_back((f, v));
        }
        let keep = self.high - 64.max(self.stop_frames + 2);
        while self.log.front().is_some_and(|(k, _)| *k < keep && (self.sfx != SfxMode::Confirmed || *k < self.released)) {
            self.log.pop_front();
        }
    }
    fn emit(&mut self, f: i32, e: AudioEvent) {
        self.events.push(SimEvent::Audio(e));
        if let Some(a) = &mut self.audit {
            a.push(SfxAudit::Play(f, e));
        }
    }

    /// Confirmed mode: release the sounds of frames below `upto` (all inputs known: they will not change).
    pub fn release_confirmed(&mut self, upto: i32) {
        if self.sfx != SfxMode::Confirmed {
            return;
        }
        let from = self.released;
        for f in from..upto.min(self.high) {
            let v = self.log_get(f).cloned().unwrap_or_default();
            for e in v {
                self.sfx_stats.played += 1;
                self.emit(f, e);
            }
        }
        self.released = self.released.max(upto.min(self.high));
    }

    /// The frame's sounds after a re-simulation: fix up what was played (Reconcile).
    fn reconcile(&mut self, f: i32, new: Vec<AudioEvent>) {
        let old = self.log_get(f).cloned().unwrap_or_default();
        let age = self.high - 1 - f;
        let mut unmatched_old: Vec<Option<AudioEvent>> = old.iter().copied().map(Some).collect();
        let mut added = Vec::new();
        for e in &new {
            match unmatched_old.iter_mut().find(|o| o.as_ref() == Some(e)) {
                Some(o) => *o = None,
                None => added.push(*e),
            }
        }
        for e in added {
            if matches!(e, AudioEvent::StopSfx(_) | AudioEvent::StopAllSfx | AudioEvent::SetMasterVolumes { .. } | AudioEvent::PlaySong(_) | AudioEvent::StopSong) || age <= self.late_frames {
                self.sfx_stats.late += 1;
                self.emit(f, e);
            } else {
                self.sfx_stats.dropped += 1;
            }
        }
        for e in unmatched_old.into_iter().flatten() {
            if let AudioEvent::PlaySfx { id, .. } = e {
                if age <= self.stop_frames {
                    // keep it if a later frame still plays the same sound (it is the same voice to the listener)
                    let again = self.log.iter().any(|(k, v)| *k > f && v.iter().any(|x| matches!(x, AudioEvent::PlaySfx { id: i, .. } if *i == id)));
                    if !again {
                        self.sfx_stats.cancelled += 1;
                        self.events.push(SimEvent::Audio(AudioEvent::StopSfx(id)));
                        if let Some(a) = &mut self.audit {
                            a.push(SfxAudit::Stop(f, e));
                        }
                    }
                }
            }
        }
        self.log_set(f, new);
    }
}

impl Game for CoreGame {
    type State = (i32, Snapshot);
    fn save(&self) -> (i32, Snapshot) {
        (self.cur, self.s.snapshot())
    }
    fn load(&mut self, st: &(i32, Snapshot)) {
        self.cur = st.0;
        self.s.restore(&st.1);
    }
    fn advance(&mut self, inputs: &[u16], first: bool) {
        let f = self.cur;
        self.cur += 1;
        if paused(inputs) {
            // the shared pause: this frame changes nothing (its sounds: none)
            if self.collect_events && self.sfx != SfxMode::Predicted {
                if first {
                    self.high = self.high.max(self.cur);
                    self.log_set(f, Vec::new());
                } else {
                    self.reconcile(f, Vec::new());
                }
            }
            self.high = self.high.max(self.cur);
            return;
        }
        if !self.collect_events || (self.sfx == SfxMode::Predicted && !first) {
            self.s.step(inputs);
            self.high = self.high.max(self.cur);
            return;
        }
        // through the Sim adapter: also emits the arena song on frame 0
        let mut ev = Vec::new();
        Sim::step(&mut self.s, inputs, &mut ev);
        let audio: Vec<AudioEvent> = ev.iter().filter_map(|e| if let SimEvent::Audio(a) = e { Some(*a) } else { None }).collect();
        if first {
            self.high = self.high.max(self.cur);
        }
        match (self.sfx, first) {
            (SfxMode::Predicted, true) => {
                if !self.quiet {
                    for e in audio {
                        self.sfx_stats.played += 1;
                        self.emit(f, e);
                    }
                }
            }
            (SfxMode::Predicted, false) => {}
            (SfxMode::Confirmed, _) => {
                self.log_set(f, audio);
                if self.quiet {
                    self.released = self.released.max(self.cur);
                }
            }
            (SfxMode::Reconcile, true) => {
                if !self.quiet {
                    for &e in &audio {
                        self.sfx_stats.played += 1;
                        self.emit(f, e);
                    }
                }
                self.log_set(f, audio);
            }
            (SfxMode::Reconcile, false) => self.reconcile(f, audio),
        }
    }
    fn checksum(&self, st: &(i32, Snapshot)) -> u64 {
        st.1.world.state_hash()
    }
    fn save_bytes(&self) -> Option<Vec<u8>> {
        Some(self.s.snapshot_bytes())
    }
    fn load_bytes(&mut self, b: &[u8]) -> bool {
        self.s.restore_bytes(b)
    }
    fn set_frame_index(&mut self, f: i32) {
        self.cur = f;
        self.high = self.high.max(f);
        self.released = self.released.max(f);
    }
    fn set_quiet(&mut self, on: bool) {
        self.quiet = on;
    }
}
