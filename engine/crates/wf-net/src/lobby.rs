//! Lobby: room by code on the signaling server, then a tiny state protocol over the game channel itself.
//!
//! * Everyone connects to `<signal>/wf-<CODE>[?token=…&key=…]` (Matchbox full mesh: every pair of peers gets a data
//!   channel). The invite code a host shares is `CODE` or `CODE-TOKEN` (the room token the signaling server checks,
//!   see `bin/wf-signal.rs`).
//! * Each peer sends `hello` (role, name, fighter / form / level, ready, seat key, content fingerprint, protocol) to
//!   every peer every 250 ms until it has the `start`; changing fighter / form / ready in the lobby UI just changes
//!   the next hello.
//! * The host (the peer that created the room) seats players in arrival order (host = slot 0) and sends `start`:
//!   match nonce, the roster (slot -> peer id, seat key), spectators, the full [`MatchConfig`] (battle with every
//!   player's fighter, PvP flag), the pause rule and the net settings. Command-line hosts start as soon as
//!   `want_players` are in (`auto_start`); the lobby UI's host starts when it presses Start and everyone is ready.
//!   The host answers every later `hello` with the same `start` (lost packets, late spectators) or `full`.
//! * **Rejoin**: a player that lost its connection (or restarted) connects to the room again and sends `rejoin`
//!   (match nonce, slot, the seat secret whose hash is its seat key) instead of hellos. The acting host checks the
//!   secret, hands it the slot (`session::P2PSession::grant_rejoin`) and answers with `grant` (+ the state snapshot
//!   as a blob, see `blob.rs`).
//!
//! * **Room kinds** (link play, engine/docs/netcode.md "Link play: rooms"): every hello / start carries the room's
//!   `kind` - "match" (the battle rooms above, the default), "dojo", "trade" - and an opaque per-kind `extra` JSON
//!   payload (a Dojo peer's save summary and pick, the host's mode / rounds, a trade offer ...). A host answers a
//!   hello of another kind with `full` ("this room is a <kind> room") - the game's "Waiting for a friend..." only
//!   pairs two players who chose the same thing. The kind owner reads the peers' extras ([`Lobby::peer_extras`])
//!   and decides when to start (`auto_start` false + [`Lobby::request_start`]).
//!
//! All messages are idempotent JSON state, so the unreliable channel is enough.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wf_core::battle::FighterSetup;

use crate::core_game::MatchConfig;
use crate::proto::{encode, Msg};
use crate::session::RejoinGrant;

pub const PROTOCOL: u32 = 2;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Host,
    Player,
    Spectator,
}

fn yes() -> bool {
    true
}
fn local() -> String {
    "local".into()
}
fn match_kind() -> String {
    KIND_MATCH.into()
}

/// The room kinds (`LobbySettings::kind`, `Hello::kind`, `StartInfo::kind`).
pub const KIND_MATCH: &str = "match";
/// link play: the Dojo (a versus match in rounds, engine/docs/netcode.md "Link play: Dojo")
pub const KIND_DOJO: &str = "dojo";
/// link play: the Trading Post (items / zenie between two saves)
pub const KIND_TRADE: &str = "trade";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    pub role: Role,
    pub name: String,
    pub fighter: FighterSetup,
    pub content: String,
    pub proto: u32,
    /// the lobby UI's ready toggle (command-line peers are always ready)
    #[serde(default = "yes")]
    pub ready: bool,
    /// hash of this peer's seat secret (proves a later rejoin)
    #[serde(default)]
    pub key: String,
    /// the room kind this peer wants ("match", "dojo", "trade")
    #[serde(default = "match_kind")]
    pub kind: String,
    /// the kind's own payload (opaque to the lobby)
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Seat {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub key: String,
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
    /// "local": a player's menu does not stop the match (default); "shared": any player's menu pauses everyone
    #[serde(default = "local")]
    pub pause: String,
    /// the room kind
    #[serde(default = "match_kind")]
    pub kind: String,
    /// the host's payload for the kind (the Dojo: the agreed setup)
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// What a rejoining player sends (instead of hellos).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RejoinReq {
    pub session: u32,
    pub slot: usize,
    pub secret: String,
    pub name: String,
}

/// The acting host's answer: the match, the seat and how to catch up; the state snapshot follows as blob `blob`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GrantInfo {
    pub start: StartInfo,
    pub grant: RejoinGrant,
    pub blob: u32,
    pub size: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum LobbyMsg {
    Hello(Hello),
    Start(StartInfo),
    Full { reason: String },
    Rejoin(RejoinReq),
    Grant(Box<GrantInfo>),
}

/// Everything needed to get back into a running match (saved when it starts: a file natively, localStorage in the
/// browser).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RejoinTicket {
    pub room: String,
    pub signal: String,
    #[serde(default)]
    pub token: String,
    pub session: u32,
    pub slot: usize,
    pub secret: String,
    pub name: String,
    /// unix seconds
    #[serde(default)]
    pub created: u64,
}

#[derive(Clone, Debug)]
pub struct LobbySettings {
    pub role: Role,
    pub name: String,
    pub fighter: FighterSetup,
    pub content: String,
    /// host: start when this many players (incl. the host) are in the room (`auto_start`), the most it seats
    pub want_players: usize,
    /// host: the match (its `battle.players` is replaced by the seated players' fighters)
    pub base: MatchConfig,
    pub input_delay: u32,
    pub max_prediction: u32,
    /// host: start by itself once `want_players` are in and ready (command line); else on `request_start`
    pub auto_start: bool,
    pub ready: bool,
    /// host: "local" | "shared"
    pub pause: String,
    /// our seat secret (its hash goes in the hellos)
    pub secret: String,
    /// rejoin a running match instead of joining a lobby
    pub rejoin: Option<RejoinTicket>,
    /// the room kind (`KIND_MATCH` / `KIND_DOJO` / `KIND_TRADE`)
    pub kind: String,
    /// our payload in every hello (the kind's)
    pub extra: serde_json::Value,
    /// host: the payload of the `start`
    pub start_extra: serde_json::Value,
}

impl Default for LobbySettings {
    fn default() -> Self {
        LobbySettings {
            role: Role::Player,
            name: String::new(),
            fighter: FighterSetup::default(),
            content: String::new(),
            want_players: 2,
            base: MatchConfig::coop(Default::default()),
            input_delay: 2,
            max_prediction: 12,
            auto_start: true,
            ready: true,
            pause: local(),
            secret: String::new(),
            rejoin: None,
            kind: match_kind(),
            extra: serde_json::Value::Null,
            start_extra: serde_json::Value::Null,
        }
    }
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
    /// (acting) host: a player asks to rejoin the running match (the owner checks the secret and grants it)
    RejoinRequest { from: String, req: RejoinReq },
    /// rejoiner: the host's grant arrived
    Granted { from: String },
}

/// One line of the lobby list (UI).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct LobbyEntry {
    pub id: String,
    pub name: String,
    pub role: String,
    pub fighter: String,
    pub form: i32,
    pub ready: bool,
    pub me: bool,
    /// player seat if the match started now (None = spectator / waiting)
    pub seat: Option<usize>,
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
    /// the lobby UI's host pressed Start
    start_requested: bool,
    /// we serve the lobby although we did not create the room (the host left during the match)
    pub acting_host: bool,
    /// rejoiner: the grant, and who sent it
    pub grant: Option<(String, GrantInfo)>,
}

/// Hex SHA-1 of a seat secret (the seat key everybody sees).
pub fn seat_key(secret: &str) -> String {
    use sha1::{Digest, Sha1};
    if secret.is_empty() {
        return String::new();
    }
    let d = Sha1::digest(secret.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

impl Lobby {
    pub fn new(settings: LobbySettings, nonce: u32) -> Lobby {
        Lobby {
            settings,
            peers: BTreeMap::new(),
            arrival: Vec::new(),
            start: None,
            last_hello: f64::NEG_INFINITY,
            log: Vec::new(),
            rejected: None,
            nonce,
            start_requested: false,
            acting_host: false,
            grant: None,
        }
    }

    fn hello(&self) -> Hello {
        Hello {
            role: self.settings.role,
            name: self.settings.name.clone(),
            fighter: self.settings.fighter.clone(),
            content: self.settings.content.clone(),
            proto: PROTOCOL,
            ready: self.settings.ready || self.settings.role == Role::Host,
            key: seat_key(&self.settings.secret),
            kind: self.settings.kind.clone(),
            extra: self.settings.extra.clone(),
        }
    }

    /// the kind's payload of every connected peer that said hello (peer id -> extra), in id order
    pub fn peer_extras(&self) -> Vec<(String, serde_json::Value)> {
        self.peers.iter().filter(|(_, p)| p.connected).filter_map(|(id, p)| p.hello.as_ref().map(|h| (id.clone(), h.extra.clone()))).collect()
    }
    /// lobby UI / kind owner: change our payload (goes out with the next hello)
    pub fn set_extra(&mut self, v: serde_json::Value) {
        self.settings.extra = v;
        self.last_hello = f64::NEG_INFINITY;
    }
    /// host: the payload the `start` will carry
    pub fn set_start_extra(&mut self, v: serde_json::Value) {
        self.settings.start_extra = v;
    }
    /// a seated peer dropped before the start ("Disconnected")
    pub fn peer_connected(&self, id: &str) -> bool {
        self.peers.get(id).is_some_and(|p| p.connected)
    }

    /// lobby UI: change what we play / our ready state (goes out with the next hello)
    pub fn set_fighter(&mut self, f: FighterSetup) {
        self.settings.fighter = f;
        self.last_hello = f64::NEG_INFINITY;
    }
    pub fn set_ready(&mut self, r: bool) {
        self.settings.ready = r;
        self.last_hello = f64::NEG_INFINITY;
    }
    /// lobby UI host: start as soon as everyone present is ready (at least one other player)
    pub fn request_start(&mut self) {
        self.start_requested = true;
    }
    /// the match is over and the players are back in the lobby (the link Dojo's character page after a fight):
    /// hellos go out again, what the others said before the match no longer counts, the next start is a new session
    pub fn reopen(&mut self) {
        self.start = None;
        self.grant = None;
        self.start_requested = false;
        self.nonce = self.nonce.wrapping_add(2);
        self.last_hello = f64::NEG_INFINITY;
        for p in self.peers.values_mut() {
            p.hello = None;
        }
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
    fn present(&self) -> Vec<String> {
        self.arrival.iter().filter(|id| self.peers.get(*id).is_some_and(|p| p.connected && p.hello.is_some())).cloned().collect()
    }
    pub fn players_present(&self) -> usize {
        1 + self.present().len()
    }
    /// Everyone in the room for the lobby UI: us first, then the players in seating order, then spectators.
    pub fn entries(&self, me: &str) -> Vec<LobbyEntry> {
        let h = self.hello();
        let mut v = vec![LobbyEntry {
            id: me.to_string(),
            name: h.name.clone(),
            role: format!("{:?}", h.role).to_lowercase(),
            fighter: h.fighter.fighter.clone(),
            form: h.fighter.form,
            ready: h.ready,
            me: true,
            seat: None,
        }];
        let cap = self.settings.want_players.clamp(1, 4);
        let mut seat = 1;
        let mut seen = std::collections::BTreeSet::new();
        for id in self.present().iter().chain(self.peers.keys()) {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(p) = self.peers.get(id).filter(|p| p.connected) else { continue };
            let Some(h) = &p.hello else { continue };
            let s = if h.role == Role::Spectator || self.settings.role != Role::Host {
                None
            } else if seat < cap {
                seat += 1;
                Some(seat - 1)
            } else {
                None
            };
            v.push(LobbyEntry { id: id.clone(), name: h.name.clone(), role: format!("{:?}", h.role).to_lowercase(), fighter: h.fighter.fighter.clone(), form: h.fighter.form, ready: h.ready, me: false, seat: s });
        }
        if self.settings.role == Role::Host {
            v[0].seat = Some(0);
        }
        v
    }

    fn send(out: &mut Vec<(String, Vec<u8>)>, to: &str, m: &LobbyMsg) {
        out.push((to.to_string(), encode(&Msg::Lobby(serde_json::to_string(m).unwrap()))));
    }
    pub fn send_grant(out: &mut Vec<(String, Vec<u8>)>, to: &str, g: &GrantInfo) {
        Self::send(out, to, &LobbyMsg::Grant(Box::new(g.clone())));
    }

    fn serving(&self) -> bool {
        self.settings.role == Role::Host || self.acting_host
    }

    /// A lobby packet from `from`.
    pub fn handle(&mut self, me: &str, from: &str, json: &str, out: &mut Vec<(String, Vec<u8>)>) -> Option<LobbyEvent> {
        let Ok(m) = serde_json::from_str::<LobbyMsg>(json) else { return None };
        match m {
            LobbyMsg::Hello(h) => {
                let p = self.peers.entry(from.to_string()).or_default();
                p.connected = true;
                let first = p.hello.is_none();
                let changed = p.hello.as_ref().is_some_and(|o| o.ready != h.ready || o.fighter != h.fighter || o.extra != h.extra);
                p.hello = Some(h.clone());
                if first {
                    self.log.push(format!("{} joined as {:?} ({} form {})", h.name, h.role, h.fighter.fighter, h.fighter.form));
                } else if changed && self.start.is_none() {
                    self.log.push(format!("{}: {} form {}{}", h.name, h.fighter.fighter, h.fighter.form, if h.ready { ", ready" } else { "" }));
                }
                if !self.serving() {
                    return None;
                }
                if h.proto != PROTOCOL || h.content != self.settings.content || h.kind != self.settings.kind {
                    let reason = if h.proto != PROTOCOL {
                        "different game version".to_string()
                    } else if h.kind != self.settings.kind {
                        format!("this room is a {} room", self.settings.kind)
                    } else {
                        "different game content (ROM / mods)".to_string()
                    };
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
                if self.start.is_some() || self.settings.role == Role::Host || self.settings.rejoin.is_some() {
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
            LobbyMsg::Rejoin(req) => {
                if self.start.as_ref().is_some_and(|st| st.session == req.session) {
                    return Some(LobbyEvent::RejoinRequest { from: from.to_string(), req });
                }
                None
            }
            LobbyMsg::Grant(g) => {
                let t = self.settings.rejoin.as_ref()?;
                if self.grant.is_some() || g.start.session != t.session || g.grant.slot != t.slot {
                    return None;
                }
                self.log.push(format!("rejoin granted by {}: slot P{} from frame {}, snapshot {} bytes", short(from), t.slot + 1, g.grant.base, g.size));
                self.grant = Some((from.to_string(), *g));
                Some(LobbyEvent::Granted { from: from.to_string() })
            }
        }
    }

    /// Periodic work: hellos (or rejoin requests) until started; the host starts the match.
    pub fn poll(&mut self, me: &str, now: f64, out: &mut Vec<(String, Vec<u8>)>) {
        if self.start.is_some() || self.grant.is_some() {
            return;
        }
        if now - self.last_hello >= 250.0 {
            self.last_hello = now;
            let h = match &self.settings.rejoin {
                Some(t) => LobbyMsg::Rejoin(RejoinReq { session: t.session, slot: t.slot, secret: t.secret.clone(), name: t.name.clone() }),
                None => LobbyMsg::Hello(self.hello()),
            };
            for (id, p) in &self.peers {
                if p.connected {
                    Self::send(out, id, &h);
                }
            }
        }
        if self.settings.role != Role::Host {
            return;
        }
        let present = self.present();
        let cap = self.settings.want_players.clamp(1, 4);
        let seated: Vec<String> = present.iter().take(cap - 1).cloned().collect();
        let all_ready = seated.iter().all(|id| self.peers[id].hello.as_ref().is_some_and(|h| h.ready));
        let go = if self.settings.auto_start { seated.len() + 1 >= cap && all_ready } else { self.start_requested && !seated.is_empty() && all_ready };
        if !go {
            return;
        }
        let mut players = vec![Seat { id: me.to_string(), name: self.settings.name.clone(), key: seat_key(&self.settings.secret) }];
        let mut cfg = self.settings.base.clone();
        cfg.battle.players = vec![self.settings.fighter.clone()];
        for id in &seated {
            let h = self.peers[id].hello.clone().unwrap();
            players.push(Seat { id: id.clone(), name: h.name.clone(), key: h.key.clone() });
            cfg.battle.players.push(h.fighter.clone());
        }
        let spectators: Vec<String> = self.peers.iter().filter(|(_, p)| p.connected && p.hello.as_ref().is_some_and(|h| h.role == Role::Spectator)).map(|(id, _)| id.clone()).collect();
        let st = StartInfo {
            session: self.nonce | 1,
            host: me.to_string(),
            players,
            spectators,
            config: cfg,
            input_delay: self.settings.input_delay,
            max_prediction: self.settings.max_prediction,
            pause: self.settings.pause.clone(),
            kind: self.settings.kind.clone(),
            extra: self.settings.start_extra.clone(),
        };
        for id in st.players.iter().skip(1).map(|s| s.id.clone()).chain(st.spectators.iter().cloned()).collect::<Vec<_>>() {
            Self::send(out, &id, &LobbyMsg::Start(st.clone()));
        }
        // the ones that did not get a seat
        for id in present.iter().skip(cap - 1) {
            Self::send(out, id, &LobbyMsg::Full { reason: "the room is full".into() });
        }
        self.log.push(format!("match starts: {} players", st.players.len()));
        self.start = Some(st);
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

/// A room token: 6 characters (letters without look-alikes + digits 2-9), ~31 bits.
pub fn room_token(seed: u64) -> String {
    const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut r = crate::transport::Rng64::new(seed ^ 0x7F4A_7C15_9E37_79B9);
    (0..6).map(|_| A[(r.next() % A.len() as u64) as usize] as char).collect()
}

/// An invite code `ROOM` or `ROOM-TOKEN` -> (room, token).
pub fn parse_invite(s: &str) -> (String, String) {
    let s = s.trim().to_ascii_uppercase();
    match s.split_once('-') {
        Some((r, t)) => (r.trim().to_string(), t.trim().to_string()),
        None => (s, String::new()),
    }
}

pub fn invite(room: &str, token: &str) -> String {
    if token.is_empty() {
        room.to_string()
    } else {
        format!("{room}-{token}")
    }
}

/// The signaling room URL of a code (+ the room token and the server access key as query parameters).
pub fn room_url(signal: &str, code: &str) -> String {
    room_url_auth(signal, code, "", "")
}

pub fn room_url_auth(signal: &str, code: &str, token: &str, key: &str) -> String {
    let mut u = format!("{}/wf-{}", signal.trim_end_matches('/'), code.trim().to_ascii_uppercase());
    let mut q = Vec::new();
    if !token.is_empty() {
        q.push(format!("token={}", url_escape(token)));
    }
    if !key.is_empty() {
        q.push(format!("key={}", url_escape(key)));
    }
    if !q.is_empty() {
        u.push('?');
        u.push_str(&q.join("&"));
    }
    u
}

pub fn url_escape(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invites_and_urls() {
        assert_eq!(parse_invite(" kqzta-7f3k2q "), ("KQZTA".into(), "7F3K2Q".into()));
        assert_eq!(parse_invite("KQZTA"), ("KQZTA".into(), String::new()));
        assert_eq!(room_url_auth("wss://x.example/", "kqzta", "7F3K", "a b"), "wss://x.example/wf-KQZTA?token=7F3K&key=a%20b");
        assert_eq!(room_url("ws://127.0.0.1:3536", "AB"), "ws://127.0.0.1:3536/wf-AB");
        assert_eq!(seat_key("s").len(), 40);
        assert_eq!(room_token(1).len(), 6);
    }
}
