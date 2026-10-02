//! Lobby: room by code on the signaling server, then a tiny state protocol over the game channel itself.
//!
//! * Everyone connects to `<signal>/wf-<CODE>` (Matchbox full mesh: every pair of peers gets a data channel).
//! * Each peer sends `hello` (role, name, fighter / form / level, content fingerprint, protocol) to every peer
//!   every 250 ms until it has the `start`.
//! * The host (the peer that created the room) seats players in arrival order (host = slot 0) and, once
//!   `want_players` are present, sends `start`: match nonce, the roster (slot -> peer id), spectators, the full
//!   [`MatchConfig`] (battle with every player's fighter, PvP flag) and the net settings. It answers every later
//!   `hello` with the same `start` (lost packets, late spectators) or `full`.
//!
//! All messages are idempotent JSON state, so the unreliable channel is enough.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wf_core::battle::FighterSetup;

use crate::core_game::MatchConfig;
use crate::proto::{encode, Msg};

pub const PROTOCOL: u32 = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Host,
    Player,
    Spectator,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    pub role: Role,
    pub name: String,
    pub fighter: FighterSetup,
    pub content: String,
    pub proto: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Seat {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StartInfo {
    pub session: u32,
    pub host: String,
    /// slot order
    pub players: Vec<Seat>,
    pub spectators: Vec<String>,
    pub config: MatchConfig,
    pub input_delay: u32,
    pub max_prediction: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum LobbyMsg {
    Hello(Hello),
    Start(StartInfo),
    Full { reason: String },
}

#[derive(Clone, Debug)]
pub struct LobbySettings {
    pub role: Role,
    pub name: String,
    pub fighter: FighterSetup,
    pub content: String,
    /// host: start when this many players (incl. the host) are in the room
    pub want_players: usize,
    /// host: the match (its `battle.players` is replaced by the seated players' fighters)
    pub base: MatchConfig,
    pub input_delay: u32,
    pub max_prediction: u32,
}

#[derive(Clone, Debug, Default)]
struct PeerInfo {
    connected: bool,
    hello: Option<Hello>,
}

/// What the lobby tells its owner.
#[derive(Clone, Debug, PartialEq)]
pub enum LobbyEvent {
    /// host, after the start: a spectator arrived (add it to the session)
    SpectatorJoined(String),
    Rejected(String),
}

pub struct Lobby {
    pub settings: LobbySettings,
    peers: BTreeMap<String, PeerInfo>,
    /// player hellos in arrival order (host seating)
    arrival: Vec<String>,
    pub start: Option<StartInfo>,
    last_hello: f64,
    pub log: Vec<String>,
    pub rejected: Option<String>,
    nonce: u32,
}

impl Lobby {
    pub fn new(settings: LobbySettings, nonce: u32) -> Lobby {
        Lobby { settings, peers: BTreeMap::new(), arrival: Vec::new(), start: None, last_hello: f64::NEG_INFINITY, log: Vec::new(), rejected: None, nonce }
    }

    fn hello(&self) -> Hello {
        Hello { role: self.settings.role, name: self.settings.name.clone(), fighter: self.settings.fighter.clone(), content: self.settings.content.clone(), proto: PROTOCOL }
    }

    pub fn connected(&mut self, id: &str) {
        self.peers.entry(id.to_string()).or_default().connected = true;
        self.log.push(format!("peer connected: {}", short(id)));
    }
    pub fn disconnected(&mut self, id: &str) {
        if let Some(p) = self.peers.get_mut(id) {
            p.connected = false;
        }
        if self.start.is_none() {
            self.arrival.retain(|a| a != id);
        }
        self.log.push(format!("peer left: {}", short(id)));
    }

    /// (name, role) of everyone seen, for the UI
    pub fn roster(&self) -> Vec<(String, String, Role)> {
        self.peers.iter().filter(|(_, p)| p.connected).filter_map(|(id, p)| p.hello.as_ref().map(|h| (id.clone(), h.name.clone(), h.role))).collect()
    }
    pub fn players_present(&self) -> usize {
        1 + self.arrival.iter().filter(|id| self.peers.get(*id).is_some_and(|p| p.connected)).count()
    }

    fn send(out: &mut Vec<(String, Vec<u8>)>, to: &str, m: &LobbyMsg) {
        out.push((to.to_string(), encode(&Msg::Lobby(serde_json::to_string(m).unwrap()))));
    }

    /// A lobby packet from `from`.
    pub fn handle(&mut self, me: &str, from: &str, json: &str, out: &mut Vec<(String, Vec<u8>)>) -> Option<LobbyEvent> {
        let Ok(m) = serde_json::from_str::<LobbyMsg>(json) else { return None };
        match m {
            LobbyMsg::Hello(h) => {
                let p = self.peers.entry(from.to_string()).or_default();
                p.connected = true;
                let first = p.hello.is_none();
                p.hello = Some(h.clone());
                if first {
                    self.log.push(format!("{} joined as {:?} ({} form {})", h.name, h.role, h.fighter.fighter, h.fighter.form));
                }
                if self.settings.role != Role::Host {
                    return None;
                }
                if h.proto != PROTOCOL || h.content != self.settings.content {
                    let reason = if h.proto != PROTOCOL { "different game version".to_string() } else { "different game content (ROM / mods)".to_string() };
                    Self::send(out, from, &LobbyMsg::Full { reason });
                    return None;
                }
                match (&mut self.start, h.role) {
                    (None, Role::Player) => {
                        if !self.arrival.iter().any(|a| a == from) {
                            self.arrival.push(from.to_string());
                        }
                        None
                    }
                    (None, _) => None,
                    (Some(st), Role::Spectator) => {
                        let new = !st.spectators.iter().any(|s| s == from);
                        if new {
                            st.spectators.push(from.to_string());
                        }
                        let st = st.clone();
                        Self::send(out, from, &LobbyMsg::Start(st));
                        new.then(|| LobbyEvent::SpectatorJoined(from.to_string()))
                    }
                    (Some(st), _) => {
                        if st.players.iter().any(|s| s.id == from) {
                            let st = st.clone();
                            Self::send(out, from, &LobbyMsg::Start(st));
                        } else {
                            Self::send(out, from, &LobbyMsg::Full { reason: "match already running".into() });
                        }
                        None
                    }
                }
            }
            LobbyMsg::Start(st) => {
                if self.start.is_some() || self.settings.role == Role::Host {
                    return None;
                }
                let seated = st.players.iter().any(|s| s.id == me) || st.spectators.iter().any(|s| s == me);
                if seated {
                    self.log.push(format!("match starts: {} players, session {:08x}", st.players.len(), st.session));
                    self.start = Some(st);
                }
                None
            }
            LobbyMsg::Full { reason } => {
                if self.start.is_none() {
                    self.rejected = Some(reason.clone());
                    return Some(LobbyEvent::Rejected(reason));
                }
                None
            }
        }
    }

    /// Periodic work: hellos until started; the host starts the match when the room is full.
    pub fn poll(&mut self, me: &str, now: f64, out: &mut Vec<(String, Vec<u8>)>) {
        if self.start.is_some() {
            return;
        }
        if now - self.last_hello >= 250.0 {
            self.last_hello = now;
            let h = LobbyMsg::Hello(self.hello());
            for (id, p) in &self.peers {
                if p.connected {
                    Self::send(out, id, &h);
                }
            }
        }
        if self.settings.role == Role::Host {
            let present: Vec<String> = self.arrival.iter().filter(|id| self.peers.get(*id).is_some_and(|p| p.connected)).cloned().collect();
            if present.len() + 1 >= self.settings.want_players.max(1) {
                let mut players = vec![Seat { id: me.to_string(), name: self.settings.name.clone() }];
                let mut cfg = self.settings.base.clone();
                cfg.battle.players = vec![self.settings.fighter.clone()];
                for id in present.iter().take(self.settings.want_players.saturating_sub(1).min(3)) {
                    let h = self.peers[id].hello.clone().unwrap();
                    players.push(Seat { id: id.clone(), name: h.name.clone() });
                    cfg.battle.players.push(h.fighter.clone());
                }
                let spectators: Vec<String> = self.peers.iter().filter(|(_, p)| p.connected && p.hello.as_ref().is_some_and(|h| h.role == Role::Spectator)).map(|(id, _)| id.clone()).collect();
                let st = StartInfo { session: self.nonce | 1, host: me.to_string(), players, spectators, config: cfg, input_delay: self.settings.input_delay, max_prediction: self.settings.max_prediction };
                for id in st.players.iter().skip(1).map(|s| s.id.clone()).chain(st.spectators.iter().cloned()).collect::<Vec<_>>() {
                    Self::send(out, &id, &LobbyMsg::Start(st.clone()));
                }
                self.log.push(format!("match starts: {} players", st.players.len()));
                self.start = Some(st);
            }
        }
    }
}

pub fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

/// A room code: 5 letters without look-alikes.
pub fn room_code(seed: u64) -> String {
    const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
    let mut r = crate::transport::Rng64::new(seed);
    (0..5).map(|_| A[(r.next() % A.len() as u64) as usize] as char).collect()
}

/// The signaling room URL of a code.
pub fn room_url(signal: &str, code: &str) -> String {
    format!("{}/wf-{}", signal.trim_end_matches('/'), code.trim().to_ascii_uppercase())
}
