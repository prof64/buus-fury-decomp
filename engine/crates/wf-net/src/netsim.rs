//! The frontend side: [`NetLobby`] (room, hellos, start — or a rejoin) and [`NetSim`], a `wf_view::Sim` that runs
//! the match through the rollback session, so `wf_app::App` (native and browser) renders and plays it like a local
//! battle. [`SyncSim`] is the local sync-test mode as a `Sim`. Status for overlays / UIs is shared through
//! `Rc<RefCell<NetStatus>>`.
//!
//! * **Online pause**: the app keeps stepping a `NetSim` while the local menu is open (`Sim::paused_input`): the
//!   local player stands still (room rule "local", default) or holds the shared pause bit (rule "shared": the
//!   match stands still for everyone while any player is in the menu).
//! * **Rejoin**: the acting host answers `rejoin` requests (seat secret checked) with a grant + the state snapshot
//!   (`blob.rs`); a player that lost every other player reconnects by itself (a new transport from
//!   [`NetSim::reconnect`] natively, [`offer_transport`] from the browser's JS) and rejoins in place.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde::Serialize;
use wf_core::content::GameData;
use wf_view::{Buttons, RenderView, Sim, SimEvent};

use crate::blob::{BlobReceiver, BlobSender};
use crate::core_game::{CoreGame, MatchConfig, SfxMode, SfxStats, BUTTONS_MASK, PAUSE_BIT};
use crate::game::Game;
use crate::lobby::{seat_key, short, GrantInfo, Lobby, LobbyEntry, LobbyEvent, LobbySettings, RejoinReq, RejoinTicket, Role, StartInfo};
use crate::proto::{decode, Msg};
use crate::session::{NetConfig, NetEvent, NetStats, P2PSession};
use crate::spectator::SpectatorSession;
use crate::synctest::SyncTestSession;
use crate::transport::{PeerChange, Transport};

pub type Clock = Rc<dyn Fn() -> f64>;

thread_local! {
    static CURRENT: RefCell<Option<Rc<RefCell<NetStatus>>>> = const { RefCell::new(None) };
    static OFFERED: RefCell<Option<Box<dyn Transport>>> = const { RefCell::new(None) };
}

/// The status of the newest lobby / match / sync test on this thread (shells draw the overlay from it without
/// holding the sim).
pub fn current_status() -> Option<Rc<RefCell<NetStatus>>> {
    CURRENT.with(|c| c.borrow().clone())
}
fn set_current(s: &Rc<RefCell<NetStatus>>) {
    CURRENT.with(|c| *c.borrow_mut() = Some(s.clone()));
}
/// Forget the current status (the shell left the match / lobby).
pub fn clear_current() {
    CURRENT.with(|c| *c.borrow_mut() = None);
}
/// The browser: a fresh transport (new signaling connection) for a match that is reconnecting
/// (`NetStatus::phase == "reconnecting"` and `need_transport`).
pub fn offer_transport(t: Box<dyn Transport>) {
    OFFERED.with(|o| *o.borrow_mut() = Some(t));
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NetStatus {
    /// connecting | lobby | starting | running | spectating | reconnecting | rejected | error | synctest
    pub phase: String,
    pub room: String,
    /// what to share: `ROOM` or `ROOM-TOKEN`
    pub invite: String,
    pub me: String,
    pub role: String,
    pub local_slot: Option<usize>,
    /// (name, role) in the lobby / (name, slot) in the match
    pub roster: Vec<(String, String)>,
    /// the lobby UI's list (fighters, ready)
    pub lobby: Vec<LobbyEntry>,
    pub players_present: usize,
    pub want_players: usize,
    pub frame: i32,
    pub confirmed: i32,
    pub hashed: i32,
    pub last_hash: String,
    pub stats: NetStats,
    /// (peer, rtt ms)
    pub rtt: Vec<(String, f64)>,
    pub wait_frames: f64,
    pub desync: Option<String>,
    pub desync_report: Option<String>,
    pub report_path: Option<String>,
    pub disconnected: Vec<String>,
    /// players that came back
    pub rejoined: Vec<String>,
    pub outcome: String,
    pub winner: Option<usize>,
    pub pvp: bool,
    pub log: Vec<String>,
    pub spect_behind: i32,
    /// "local" | "shared"
    pub pause: String,
    /// slots holding the shared pause
    pub paused_by: Vec<usize>,
    /// rejoin progress / why it waits
    pub rejoin: Option<String>,
    /// the browser must open a new signaling connection and `wf_net_reconnect`
    pub need_transport: bool,
    pub sfx: SfxStats,
    pub acting_host: bool,
    pub error: Option<String>,
    /// JSON rejoin ticket of the running match (the browser keeps it in localStorage)
    pub ticket: Option<String>,
}

impl NetStatus {
    pub fn to_json(&self) -> String {
        let mut s = self.clone();
        s.desync_report = None;
        serde_json::to_string(&s).unwrap_or_default()
    }
    /// short lines for an on-screen overlay (first = alert, if any)
    pub fn overlay(&self) -> Vec<(String, bool)> {
        let mut v = Vec::new();
        if let Some(d) = &self.desync {
            v.push((format!("DESYNC {d}"), true));
        }
        for p in &self.disconnected {
            v.push((format!("{p} disconnected"), true));
        }
        for p in &self.rejoined {
            v.push((format!("{p} rejoined"), false));
        }
        if !self.paused_by.is_empty() {
            let who: Vec<String> = self.paused_by.iter().map(|s| format!("P{}", s + 1)).collect();
            v.push((format!("PAUSED BY {}", who.join(" ")), true));
        }
        if let Some(r) = &self.rejoin {
            v.push((r.clone(), true));
        }
        let rtt = self.rtt.iter().map(|(_, r)| format!("{r:.0}")).collect::<Vec<_>>().join("/");
        match self.phase.as_str() {
            "running" => v.push((
                format!(
                    "P{} rtt {}ms  rollbacks {} (max {})  stalls {}  f{}",
                    self.local_slot.map(|s| s + 1).unwrap_or(0),
                    rtt,
                    self.stats.rollbacks,
                    self.stats.max_rollback,
                    self.stats.stalls,
                    self.frame
                ),
                false,
            )),
            "spectating" => v.push((format!("SPECTATING  f{}  behind {}", self.frame, self.spect_behind), false)),
            "synctest" => v.push((format!("SYNCTEST f{}  checked {}", self.frame, self.stats.hash_checks), false)),
            "reconnecting" => v.push(("RECONNECTING...".into(), true)),
            p => v.push((format!("{} {}", p.to_uppercase(), self.room), false)),
        }
        if self.outcome != "running" && !self.outcome.is_empty() {
            v.push((
                match self.winner {
                    Some(w) => format!("PLAYER {} WINS", w + 1),
                    None => self.outcome.to_uppercase(),
                },
                true,
            ));
        }
        v
    }
}

/// Content fingerprint for the lobby: peers with different ROM / mods would desync, so they are refused.
pub fn content_id(d: &GameData) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = wf_core::world::StableHasher::default();
    format!("{:?}{:?}{:?}{:?}{:?}{:?}", d.enemies, d.chars, d.forms, d.skills, d.equipment, d.xp_table).hash(&mut h);
    for (k, m) in &d.maps {
        k.hash(&mut h);
        m.walk.hash(&mut h);
        m.shot.hash(&mut h);
    }
    d.states.len().hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A random-enough secret from whatever entropy the platform gives (wasm: the host passes `seed`).
pub fn make_secret(seed: u64) -> String {
    let mut r = crate::transport::Rng64::new(seed ^ 0xA5A5_5A5A_DEAD_BEEF);
    (0..4).map(|_| format!("{:016x}", r.next())).collect()
}

pub struct NetLobby {
    pub transport: Box<dyn Transport>,
    pub lobby: Lobby,
    pub status: Rc<RefCell<NetStatus>>,
    clock: Clock,
    out: Vec<(String, Vec<u8>)>,
    /// rejoin: the snapshot coming from the host
    blob: Option<BlobReceiver>,
    pub room: String,
    pub token: String,
    pub signal: String,
    /// local sound mode for the match
    pub sfx: SfxMode,
}

impl NetLobby {
    pub fn new(transport: Box<dyn Transport>, settings: LobbySettings, room: &str, clock: Clock) -> Self {
        let st = NetStatus { phase: "connecting".into(), room: room.to_string(), invite: room.to_string(), role: format!("{:?}", settings.role).to_lowercase(), want_players: settings.want_players, pvp: settings.base.pvp, pause: settings.pause.clone(), ..Default::default() };
        let status = Rc::new(RefCell::new(st));
        set_current(&status);
        Self::with_status(transport, settings, room, clock, status)
    }

    /// (reconnecting in place: keep showing the match's status)
    fn with_status(transport: Box<dyn Transport>, settings: LobbySettings, room: &str, clock: Clock, status: Rc<RefCell<NetStatus>>) -> Self {
        let nonce = ((clock)() as u64).wrapping_mul(0x9E37_79B9).wrapping_add(room.len() as u64 * 7919) as u32 ^ 0x5A5A_1234 ^ (crate::transport::Rng64::new(settings.secret.len() as u64 + seat_key(&settings.secret).bytes().map(|b| b as u64).sum::<u64>()).next() as u32);
        NetLobby { transport, lobby: Lobby::new(settings, nonce), status, clock, out: Vec::new(), blob: None, room: room.to_string(), token: String::new(), signal: String::new(), sfx: SfxMode::default() }
    }

    /// what the host shares, and where (for the rejoin ticket)
    pub fn set_invite(&mut self, token: &str, signal: &str) {
        self.token = token.to_string();
        self.signal = signal.to_string();
        self.status.borrow_mut().invite = crate::lobby::invite(&self.room, token);
    }

    /// Pump the transport and the lobby; true once the match starts / the rejoin snapshot is complete (or we were
    /// refused / the connection failed: see `status`).
    pub fn poll(&mut self) -> bool {
        let now = (self.clock)();
        for c in self.transport.changes() {
            match c {
                PeerChange::Connected(id) => self.lobby.connected(&id),
                PeerChange::Disconnected(id) => self.lobby.disconnected(&id),
            }
        }
        let err = self.transport.closed().then(|| self.transport.error().unwrap_or_else(|| "the signaling server closed the connection".into()));
        let Some(me) = self.transport.local_id() else {
            if let Some(e) = err {
                let mut s = self.status.borrow_mut();
                s.phase = "error".into();
                s.error = Some(e);
                return true;
            }
            return false;
        };
        for (from, b) in self.transport.recv(now) {
            match decode(&b) {
                Some(Msg::Lobby(j)) => {
                    if let Some(LobbyEvent::Granted { .. }) = self.lobby.handle(&me, &from, &j, &mut self.out) {
                        let id = self.lobby.grant.as_ref().map(|g| g.1.blob).unwrap_or(0);
                        if self.blob.as_ref().is_none_or(|b| b.id != id) {
                            self.blob = Some(BlobReceiver::new(id));
                        }
                    }
                }
                Some(Msg::Blob { id, idx, n, total, data }) => {
                    if self.blob.is_none() {
                        // chunks can overtake the grant
                        self.blob = Some(BlobReceiver::new(id));
                    }
                    if let Some(r) = &mut self.blob {
                        if r.id == id {
                            r.on_chunk(id, idx, n, total, &data);
                        }
                    }
                }
                _ => {}
            }
        }
        self.lobby.poll(&me, now, &mut self.out);
        // chunks are acknowledged to the sender the grant names
        if let (Some(r), Some((from, _))) = (&mut self.blob, &self.lobby.grant) {
            r.ack(from, now, &mut self.out);
        }
        for (to, b) in self.out.drain(..) {
            self.transport.send(&to, b, now);
        }
        let blob_done = self.lobby.grant.is_some() && self.blob.as_ref().is_some_and(|b| b.complete().is_some());
        let mut s = self.status.borrow_mut();
        s.me = me.clone();
        let rejoin = self.lobby.settings.rejoin.is_some();
        s.phase = if self.lobby.rejected.is_some() {
            "rejected".into()
        } else if rejoin {
            "reconnecting".into()
        } else if self.lobby.start.is_some() {
            "starting".into()
        } else {
            "lobby".into()
        };
        if rejoin {
            s.rejoin = Some(match (&self.lobby.grant, &self.blob) {
                (Some(_), Some(b)) => {
                    let (k, n) = b.progress();
                    format!("REJOINING: snapshot {k}/{n}")
                }
                _ => "REJOINING: waiting for the host".into(),
            });
        } else {
            s.roster = self.lobby.roster().into_iter().map(|(_, n, r)| (n, format!("{r:?}").to_lowercase())).collect();
            s.lobby = self.lobby.entries(&me);
            s.players_present = self.lobby.players_present();
        }
        s.log = self.lobby.log.iter().rev().take(8).rev().cloned().collect();
        if let Some(r) = &self.lobby.rejected {
            s.desync = None;
            s.outcome = format!("refused: {r}");
        }
        if let Some(e) = err {
            if !blob_done && self.lobby.start.is_none() {
                s.phase = "error".into();
                s.error = Some(e);
                return true;
            }
        }
        (self.lobby.start.is_some() && !rejoin) || blob_done || self.lobby.rejected.is_some()
    }

    pub fn started(&self) -> Option<&StartInfo> {
        self.lobby.start.as_ref()
    }

    /// The match, once `poll` returned true and `started()` (or a rejoin grant) is there.
    pub fn into_sim(self, data: Arc<GameData>) -> Result<NetSim, String> {
        let now = (self.clock)();
        let me = self.transport.local_id().unwrap_or_default();
        let (mode, start, start_state) = self.build(&me, data.clone(), now)?;
        let status = self.status.clone();
        let ticket = self.ticket(&start, &me);
        let mut sim = NetSim {
            transport: self.transport,
            lobby: self.lobby,
            mode,
            clock: self.clock,
            status,
            me,
            start,
            out: Vec::new(),
            report_dir: None,
            record_dir: None,
            content: Default::default(),
            created: 0,
            last_record_tick: 0,
            ticks: 0,
            reported: false,
            data,
            grants: Vec::new(),
            next_blob: 1,
            start_state,
            ticket,
            ticket_path: None,
            reconnect: None,
            rejoining: None,
            lonely_since: None,
            reconnect_tries: 0,
            next_probe: 0.0,
            room: self.room,
            token: self.token,
            signal: self.signal,
            sfx: self.sfx,
            last_rejoin_note: 0.0,
        };
        sim.init_status();
        Ok(sim)
    }

    fn ticket(&self, st: &StartInfo, me: &str) -> Option<RejoinTicket> {
        let slot = st.players.iter().position(|s| s.id == me)?;
        let secret = match &self.lobby.settings.rejoin {
            Some(t) => t.secret.clone(),
            None => self.lobby.settings.secret.clone(),
        };
        if secret.is_empty() {
            return None;
        }
        Some(RejoinTicket { room: self.room.clone(), signal: self.signal.clone(), token: self.token.clone(), session: st.session, slot, secret, name: self.lobby.settings.name.clone(), created: 0 })
    }

    /// The session (player / spectator / rejoined player), the match and, for a rejoin, the start snapshot.
    #[allow(clippy::type_complexity)]
    fn build(&self, me: &str, data: Arc<GameData>, now: f64) -> Result<(Mode, StartInfo, Option<(i32, u32, Vec<u8>)>), String> {
        if let Some((_, g)) = &self.lobby.grant {
            let bytes = self.blob.as_ref().and_then(|b| b.complete()).ok_or("the rejoin snapshot is incomplete")?;
            let mut st = g.start.clone();
            let mut game = CoreGame::new(st.config.session(data));
            game.sfx = self.sfx;
            if !game.load_bytes(&bytes) {
                return Err("the rejoin snapshot does not load (different game content?)".into());
            }
            let world_frame = game.s.world.frame;
            let ncfg = NetConfig { slots: st.players.len(), input_delay: st.input_delay, max_prediction: st.max_prediction, session: st.session, ..Default::default() };
            let p = P2PSession::resume(game, ncfg, &g.grant, me, now)?;
            if let Some(seat) = st.players.get_mut(g.grant.slot) {
                seat.id = me.to_string();
            }
            return Ok((Mode::Player(Box::new(p)), st, Some((g.grant.base, world_frame, bytes))));
        }
        let st = self.lobby.start.clone().ok_or_else(|| self.lobby.rejected.clone().unwrap_or_else(|| "not started".into()))?;
        let mut game = CoreGame::new(st.config.session(data));
        game.collect_events = true;
        game.sfx = self.sfx;
        let ncfg = NetConfig { slots: st.players.len(), input_delay: st.input_delay, max_prediction: st.max_prediction, session: st.session, ..Default::default() };
        let mode = match st.players.iter().position(|s| s.id == me) {
            Some(slot) => {
                let peers = st.players.iter().enumerate().filter(|(_, s)| s.id != me).map(|(k, s)| (s.id.clone(), vec![k])).collect();
                let host = st.host == me;
                // every player knows the spectators (whoever acts as host serves them)
                Mode::Player(Box::new(P2PSession::new(game, ncfg, vec![slot], peers, st.spectators.clone(), host, now)))
            }
            None => {
                let mut sp = SpectatorSession::new(game, st.session, st.host.clone(), st.players.len(), now);
                sp.players = st.players.iter().map(|s| s.id.clone()).collect();
                Mode::Spectator(Box::new(sp))
            }
        };
        Ok((mode, st, None))
    }
}

pub enum Mode {
    Player(Box<P2PSession<CoreGame>>),
    Spectator(Box<SpectatorSession<CoreGame>>),
}

type TransportFactory = Box<dyn FnMut() -> Result<Box<dyn Transport>, String>>;

pub struct NetSim {
    pub transport: Box<dyn Transport>,
    pub lobby: Lobby,
    pub mode: Mode,
    clock: Clock,
    pub status: Rc<RefCell<NetStatus>>,
    pub me: String,
    pub start: StartInfo,
    out: Vec<(String, Vec<u8>)>,
    /// native: write desync reports here
    pub report_dir: Option<std::path::PathBuf>,
    /// native: save the match as a .wfr replay here (every 30 s and at the end); the shell sets `<data>/replays`
    pub record_dir: Option<std::path::PathBuf>,
    /// what the replay header records about the content (the shell fills it from its packs)
    pub content: wf_replay::ContentId,
    /// unix seconds of the match start (replay metadata / file name)
    pub created: u64,
    last_record_tick: u64,
    pub ticks: u64,
    reported: bool,
    data: Arc<GameData>,
    /// rejoins we granted: (peer, slot, grant, snapshot sender)
    grants: Vec<(String, usize, GrantInfo, BlobSender)>,
    next_blob: u32,
    /// rejoined: (state index, world frame, snapshot) the match continues from (replays start there)
    start_state: Option<(i32, u32, Vec<u8>)>,
    /// how to get back into this match (saved by the shell; the browser keeps `status.ticket`)
    pub ticket: Option<RejoinTicket>,
    /// native: the ticket file (written at the start, removed when the match is decided)
    pub ticket_path: Option<std::path::PathBuf>,
    /// native: a fresh transport to the same room (automatic reconnect)
    pub reconnect: Option<TransportFactory>,
    rejoining: Option<Box<NetLobby>>,
    lonely_since: Option<f64>,
    reconnect_tries: u32,
    next_probe: f64,
    pub room: String,
    pub token: String,
    pub signal: String,
    sfx: SfxMode,
    last_rejoin_note: f64,
}

impl NetSim {
    pub fn game(&self) -> &CoreGame {
        match &self.mode {
            Mode::Player(p) => &p.game,
            Mode::Spectator(s) => &s.game,
        }
    }
    pub fn game_mut(&mut self) -> &mut CoreGame {
        match &mut self.mode {
            Mode::Player(p) => &mut p.game,
            Mode::Spectator(s) => &mut s.game,
        }
    }
    pub fn local_slot(&self) -> Option<usize> {
        self.status.borrow().local_slot
    }
    /// (hash, chain) of a globally confirmed state, once known
    pub fn hash_at(&self, k: i32) -> Option<(u64, u64)> {
        let (h, c) = match &self.mode {
            Mode::Player(p) => (&p.hashes, &p.chain),
            Mode::Spectator(s) => (&s.hashes, &s.chain),
        };
        h.get(k as usize).map(|&x| (x, c[k as usize]))
    }
    pub fn hashed(&self) -> i32 {
        match &self.mode {
            Mode::Player(p) => p.hashed(),
            Mode::Spectator(s) => s.hashes.len() as i32 - 1,
        }
    }
    pub fn rejoined(&self) -> bool {
        self.start_state.is_some()
    }

    fn init_status(&mut self) {
        let st = &self.start;
        let mut s = self.status.borrow_mut();
        s.phase = if matches!(self.mode, Mode::Player(_)) { "running".into() } else { "spectating".into() };
        s.local_slot = st.players.iter().position(|x| x.id == self.me);
        s.roster = st.players.iter().enumerate().map(|(k, p)| (p.name.clone(), format!("P{}", k + 1))).collect();
        s.pvp = st.config.pvp;
        s.pause = st.pause.clone();
        s.rejoin = None;
        s.need_transport = false;
        s.ticket = self.ticket.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default());
        drop(s);
        self.write_ticket();
    }

    fn write_ticket(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some(p), Some(t)) = (&self.ticket_path, &self.ticket) {
            let mut t = t.clone();
            t.created = self.created;
            let _ = std::fs::write(p, serde_json::to_string_pretty(&t).unwrap_or_default());
        }
    }
    /// the shell set `ticket_path` / `created` after `into_sim`
    pub fn save_ticket(&self) {
        self.write_ticket();
    }

    /// One tick: network in, rollback / advance, network out. `local` = held buttons of the local player.
    pub fn tick(&mut self, local: u16) -> crate::session::Tick {
        let now = (self.clock)();
        self.ticks += 1;
        let changes = self.transport.changes();
        for c in changes {
            match c {
                PeerChange::Connected(id) => self.lobby.connected(&id),
                PeerChange::Disconnected(id) => {
                    self.lobby.disconnected(&id);
                    if let Mode::Player(p) = &mut self.mode {
                        p.peer_left(&id);
                    }
                    if let Mode::Spectator(s) = &self.mode {
                        if s.host == id {
                            self.status.borrow_mut().disconnected.push("host".into());
                        }
                    }
                }
            }
        }
        for (from, b) in self.transport.recv(now) {
            let Some(m) = decode(&b) else { continue };
            match (&m, &mut self.mode) {
                (Msg::Lobby(j), mode) => match self.lobby.handle(&self.me, &from, j, &mut self.out) {
                    Some(LobbyEvent::SpectatorJoined(id)) => {
                        if let Mode::Player(p) = mode {
                            p.add_spectator(id, now);
                        }
                    }
                    Some(LobbyEvent::RejoinRequest { from, req }) => self.on_rejoin_request(&from, &req, now),
                    _ => {}
                },
                (Msg::BlobAck { id, have }, _) => {
                    if let Some(g) = self.grants.iter_mut().find(|g| g.3.id == *id && g.0 == from) {
                        g.3.on_ack(have);
                    }
                }
                (_, Mode::Player(p)) => {
                    p.count_recv(b.len());
                    p.handle(&from, &m, now);
                }
                (_, Mode::Spectator(s)) => s.handle(&from, &m, now),
            }
        }
        let clock = self.clock.clone();
        let t = match &mut self.mode {
            Mode::Player(p) => {
                let t = p.tick(now, &*clock, &[local], &mut self.out);
                self.lobby.acting_host = p.acting_host() && !p.is_host;
                let c = p.confirmed().min(p.frame());
                p.game.release_confirmed(c);
                t
            }
            Mode::Spectator(s) => {
                let n = s.tick(&mut self.out);
                crate::session::Tick { advanced: n, ..Default::default() }
            }
        };
        for g in self.grants.iter_mut() {
            g.3.poll(now, &mut self.out);
        }
        self.grants.retain(|g| !g.3.done());
        for (to, b) in self.out.drain(..) {
            self.transport.send(&to, b, now);
        }
        self.update_status();
        self.watch_connection(now);
        if self.rejoining.is_some() {
            self.pump_rejoin(now);
        }
        if self.record_dir.is_some() && self.ticks - self.last_record_tick >= 1800 {
            self.save_replay();
        }
        t
    }

    /// (acting) host: a player asks to come back into slot `req.slot`
    fn on_rejoin_request(&mut self, from: &str, req: &RejoinReq, now: f64) {
        let Mode::Player(p) = &mut self.mode else { return };
        if let Some(g) = self.grants.iter().find(|g| g.0 == from) {
            // a repeated request: the grant got lost
            Lobby::send_grant(&mut self.out, from, &g.2);
            return;
        }
        if !p.acting_host() || p.local_slots.contains(&req.slot) {
            // (our own reconnect probe asks everyone in the room, us included)
            return;
        }
        let Some(seat) = self.start.players.get(req.slot) else { return };
        if seat.key.is_empty() || seat_key(&req.secret) != seat.key {
            eprintln!("wf-net: rejoin request for P{} from {} with a wrong seat secret: ignored", req.slot + 1, short(from));
            return;
        }
        // the secret proves it is that player: its old connection (if we still think it is there) is stale
        let old: Vec<String> = p.peers.iter().filter(|q| q.connected && !q.spectator && q.slots.contains(&req.slot) && q.id != from).map(|q| q.id.clone()).collect();
        for id in old {
            p.drop_peer_id(&id);
        }
        match p.grant_rejoin(req.slot, from, now) {
            Ok((mut g, bytes)) => {
                for e in g.peers.iter_mut() {
                    if e.0.is_empty() {
                        e.0 = self.me.clone();
                    }
                }
                let id = self.next_blob;
                self.next_blob += 1;
                let mut start = self.start.clone();
                start.players[req.slot].id = from.to_string();
                self.start.players[req.slot].id = from.to_string();
                let info = GrantInfo { start, grant: g, blob: id, size: bytes.len() as u32 };
                eprintln!("wf-net: {} rejoins as P{} (snapshot of state {}, {} bytes)", req.name, req.slot + 1, info.grant.base, bytes.len());
                Lobby::send_grant(&mut self.out, from, &info);
                self.grants.push((from.to_string(), req.slot, info, BlobSender::new(id, from.to_string(), &bytes)));
            }
            Err(e) => {
                if now - self.last_rejoin_note > 1000.0 {
                    self.last_rejoin_note = now;
                    eprintln!("wf-net: rejoin of P{} waits: {e}", req.slot + 1);
                }
            }
        }
    }

    /// Lost every other player? If they went silent (not announced by goodbyes or the signaling server) or our
    /// signaling connection died, it may be us: probe with a new connection to the room and ask to rejoin. The
    /// session keeps running meanwhile (if it was the others, they rejoin to us), and is replaced only when a grant
    /// arrives. Natively the probe's transport comes from [`NetSim::reconnect`]; in the browser from
    /// [`offer_transport`], asked for (`need_transport`) once the signaling connection is closed.
    fn watch_connection(&mut self, now: f64) {
        let Mode::Player(p) = &self.mode else { return };
        let closed = self.transport.closed();
        let running = self.start.players.len() >= 2 && self.game().s.outcome == wf_core::battle::Outcome::Running;
        let anyone = p.peers.iter().any(|q| q.connected && !q.spectator);
        let silent = p.peers.iter().any(|q| !q.connected && !q.spectator && q.timed_out);
        if !running || (!closed && (anyone || !silent)) {
            self.lonely_since = None;
            self.reconnect_tries = 0;
            if self.rejoining.take().is_some() {
                eprintln!("wf-net: the others are back: reconnect probe dropped");
            }
            let mut s = self.status.borrow_mut();
            if s.need_transport || s.rejoin.is_some() {
                s.need_transport = false;
                s.rejoin = None;
            }
            return;
        }
        let since = *self.lonely_since.get_or_insert(now);
        if now - since < 1500.0 || self.rejoining.is_some() || now < self.next_probe {
            return;
        }
        if self.reconnect_tries >= 6 {
            self.status.borrow_mut().rejoin = Some("CONNECTION LOST (everyone left?)".into());
            return;
        }
        let Some(ticket) = self.ticket.clone() else { return };
        let t = match self.reconnect.as_mut() {
            Some(f) => match f() {
                Ok(t) => Some(t),
                Err(e) => {
                    eprintln!("wf-net: reconnect: {e}");
                    None
                }
            },
            None if closed => OFFERED.with(|o| o.borrow_mut().take()),
            None => None,
        };
        let Some(t) = t else {
            let mut s = self.status.borrow_mut();
            s.need_transport = closed && self.reconnect.is_none();
            s.rejoin = Some(if closed { "RECONNECTING..." } else { "CONNECTION LOST? waiting" }.into());
            return;
        };
        self.reconnect_tries += 1;
        self.next_probe = now + 20_000.0;
        eprintln!("wf-net: lost every other player: probing room {} with a new connection (attempt {})", self.room, self.reconnect_tries);
        let settings = LobbySettings { role: Role::Player, rejoin: Some(ticket), ..self.lobby.settings.clone() };
        // the probe has its own status (the match's stays on screen)
        let st = Rc::new(RefCell::new(NetStatus::default()));
        let mut l = NetLobby::with_status(t, settings, &self.room, self.clock.clone(), st);
        l.token = self.token.clone();
        l.signal = self.signal.clone();
        l.sfx = self.sfx;
        self.rejoining = Some(Box::new(l));
        let mut s = self.status.borrow_mut();
        s.need_transport = false;
        s.rejoin = Some("RECONNECTING...".into());
    }

    /// The reconnect probe: when the grant + snapshot are in, the match continues on the new connection.
    fn pump_rejoin(&mut self, now: f64) {
        let Some(l) = self.rejoining.as_mut() else { return };
        if !l.poll() {
            let r = l.status.borrow().rejoin.clone();
            if l.lobby.grant.is_some() {
                self.status.borrow_mut().rejoin = r;
            }
            return;
        }
        let l = self.rejoining.take().unwrap();
        if l.lobby.rejected.is_some() || l.status.borrow().phase == "error" {
            eprintln!("wf-net: reconnect probe failed: {}", l.status.borrow().error.clone().or(l.lobby.rejected.clone()).unwrap_or_default());
            return;
        }
        let me = l.transport.local_id().unwrap_or_default();
        match l.build(&me, self.data.clone(), now) {
            Ok((mode, start, ss)) => {
                let mut l = l;
                self.transport = std::mem::replace(&mut l.transport, Box::new(crate::transport::QueueTransport::default()));
                self.lobby = l.lobby;
                self.mode = mode;
                self.start = start;
                self.start_state = ss;
                self.me = me;
                self.grants.clear();
                self.lonely_since = None;
                self.reconnect_tries = 0;
                self.status.borrow_mut().disconnected.clear();
                self.init_status();
                eprintln!("wf-net: rejoined the match");
            }
            Err(e) => {
                eprintln!("wf-net: rejoin failed: {e}");
                self.status.borrow_mut().rejoin = Some(format!("REJOIN FAILED: {e}"));
            }
        }
    }

    fn update_status(&mut self) {
        let events: Vec<NetEvent> = match &mut self.mode {
            Mode::Player(p) => std::mem::take(&mut p.events),
            Mode::Spectator(s) => std::mem::take(&mut s.events),
        };
        for e in events {
            match e {
                NetEvent::Desync { frame, local, remote, peer, chain } => {
                    let txt = format!("at frame {frame} with {} ({}: {local:016x} vs {remote:016x})", short(&peer), if chain { "hash chain" } else { "state hash" });
                    eprintln!("wf-net: DESYNC {txt}");
                    let report = self.desync_report(frame, local, remote, &peer, chain);
                    let mut s = self.status.borrow_mut();
                    if s.desync.is_none() {
                        s.desync = Some(txt);
                    }
                    if !self.reported {
                        self.reported = true;
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some(dir) = &self.report_dir {
                            let _ = std::fs::create_dir_all(dir);
                            let p = dir.join(format!("desync-{:08x}-f{frame}-{}.json", self.start.session, short(&self.me)));
                            if std::fs::write(&p, &report).is_ok() {
                                eprintln!("wf-net: desync report written to {}", p.display());
                                let _ = std::fs::write(p.with_extension("wfr"), self.replay(Some(frame)).to_bytes());
                                s.report_path = Some(p.display().to_string());
                            }
                        }
                        s.desync_report = Some(report);
                    }
                }
                NetEvent::Disconnected { peer, slots, frame } => {
                    let name = self.start.players.iter().find(|s| s.id == peer).map(|s| s.name.clone()).unwrap_or_else(|| short(&peer).to_string());
                    eprintln!("wf-net: {name} disconnected (slots {slots:?} neutral from frame {frame})");
                    let mut s = self.status.borrow_mut();
                    s.disconnected.push(name.clone());
                    s.rejoined.retain(|n| *n != name);
                }
                NetEvent::Rejoined { peer, slot, from } => {
                    if let Some(seat) = self.start.players.get_mut(slot) {
                        seat.id = peer.clone();
                    }
                    let name = self.start.players.get(slot).map(|s| s.name.clone()).unwrap_or_default();
                    eprintln!("wf-net: {name} rejoined as P{} (inputs from frame {from})", slot + 1);
                    let mut s = self.status.borrow_mut();
                    s.disconnected.retain(|n| *n != name);
                    if !s.rejoined.contains(&name) {
                        s.rejoined.push(name);
                    }
                }
            }
        }
        let g = &self.game().s;
        let outcome = format!("{:?}", g.outcome).to_lowercase();
        let winner = wf_core::battle::pvp_winner(&g.world);
        let sfx = self.game().sfx_stats.clone();
        let mut s = self.status.borrow_mut();
        if s.outcome == "running" && outcome != "running" {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(p) = &self.ticket_path {
                let _ = std::fs::remove_file(p);
            }
        }
        s.outcome = outcome;
        s.winner = winner;
        s.sfx = sfx;
        match &self.mode {
            Mode::Player(p) => {
                s.frame = p.frame();
                s.confirmed = p.confirmed().min(p.frame());
                s.hashed = p.hashed();
                s.last_hash = p.hashes.last().map(|h| format!("{h:016x}")).unwrap_or_default();
                s.stats = p.stats.clone();
                s.rtt = p.peers.iter().filter(|q| !q.spectator && q.connected).map(|q| (short(&q.id).to_string(), q.rtt_ms)).collect();
                s.wait_frames = p.wait_recommendation();
                s.acting_host = p.acting_host();
                s.paused_by = p.used_inputs().last().map(|u| (0..u.len()).filter(|&k| u[k] & PAUSE_BIT != 0).collect()).unwrap_or_default();
                if s.phase == "reconnecting" && self.rejoining.is_none() && !s.need_transport {
                    s.phase = "running".into();
                }
            }
            Mode::Spectator(sp) => {
                s.frame = sp.frame();
                s.confirmed = sp.frame();
                s.hashed = sp.hashes.len() as i32 - 1;
                s.last_hash = sp.hashes.last().map(|h| format!("{h:016x}")).unwrap_or_default();
                s.spect_behind = sp.frames_behind;
                s.stats.hash_checks = sp.hash_checks;
                s.stats.packets_recv = sp.packets_recv;
                s.stats.frames = sp.frame() as u64;
                let f = sp.frame() as usize;
                s.paused_by = if f > 0 { sp.inputs().get(f - 1).map(|u| (0..u.len()).filter(|&k| u[k] & PAUSE_BIT != 0).collect()).unwrap_or_default() } else { Vec::new() };
            }
        }
    }

    /// Inputs of every slot as the match used them (neutral after a disconnect), frames `from..to`.
    fn match_inputs(&self, from: i32, to: i32) -> Vec<Vec<u16>> {
        let n = self.start.players.len();
        match &self.mode {
            Mode::Player(p) => (from..to).map(|f| (0..n).map(|k| p.input_at(k, f)).collect()).collect(),
            Mode::Spectator(s) => (from..to).map(|f| s.inputs().get(f as usize).cloned().unwrap_or_else(|| vec![0; n])).collect(),
        }
    }

    /// JSON: what happened, the match config and the full input log (a replay: re-simulate it with
    /// [`replay_hashes`] on both peers' reports to find the first divergent frame), recent hashes, observations.
    pub fn desync_report(&self, frame: i32, local: u64, remote: u64, peer: &str, chain: bool) -> String {
        let hashes = match &self.mode {
            Mode::Player(p) => &p.hashes,
            Mode::Spectator(s) => &s.hashes,
        };
        let base = self.start_state.as_ref().map_or(0, |s| s.0);
        let end = match &self.mode {
            Mode::Player(p) => p.known_inputs().iter().map(|k| k.len()).max().unwrap_or(0) as i32,
            Mode::Spectator(s) => s.inputs().len() as i32,
        };
        let rows = self.match_inputs(0, end);
        let n = self.start.players.len();
        let rle: Vec<Vec<(u16, u32)>> = (0..n).map(|k| rle(&rows.iter().map(|r| r[k]).collect::<Vec<_>>())).collect();
        let from = hashes.len().saturating_sub(600).max(base as usize);
        let recent: Vec<(usize, String)> = (from..hashes.len()).map(|k| (k, format!("{:016x}", hashes[k]))).collect();
        let w = &self.game().s.world;
        let obs: Vec<serde_json::Value> = (0..w.players.len()).map(|k| serde_json::to_value(wf_core::observe::observe(w, k)).unwrap_or_default()).collect();
        serde_json::json!({
            "kind": "wf-net desync report",
            "frame": frame,
            "local": format!("{local:016x}"),
            "remote": format!("{remote:016x}"),
            "chain": chain,
            "peer": peer,
            "me": self.me,
            "local_slot": self.local_slot(),
            "session": self.start.session,
            "start": self.start,
            "rejoined_at": self.start_state.as_ref().map(|s| s.0),
            "inputs_rle": rle,
            "hashes": recent,
            "frame_now": w.frame,
            "observations_now": obs,
        })
        .to_string()
    }

    /// The match so far as a wf-replay recording (`.wfr`, engine/docs/replays.md): config (+ `"pvp": true`, which
    /// the app's replay player applies), every slot's inputs as the match used them and the state-hash
    /// checkpoints, up to the newest state every peer confirmed (or `upto`, e.g. a desync frame). Frames of the
    /// shared pause are left out (the world stood still); a rejoined peer's recording starts from the snapshot it
    /// rejoined with. Plays and verifies like any recording.
    pub fn replay(&self, upto: Option<i32>) -> wf_replay::Replay {
        let n = self.start.players.len();
        let hashes = match &self.mode {
            Mode::Player(p) => &p.hashes,
            Mode::Spectator(s) => &s.hashes,
        };
        let (base, wf0, start_state) = match &self.start_state {
            Some((b, f, bytes)) => (*b, *f as u64, Some(bytes.clone())),
            None => (0, 0, None),
        };
        let mut end = hashes.len() as i32 - 1;
        if let Some(u) = upto {
            end = end.min(u);
        }
        let end = end.max(base);
        let rows = self.match_inputs(base, end);
        let mut log = wf_replay::InputLog::new(n, wf0);
        let iv = wf_replay::DEFAULT_HASH_INTERVAL;
        let mut checkpoints: Vec<(u64, u64)> = Vec::new();
        let mut wf = wf0;
        if let Some(&h) = hashes.get(base as usize) {
            checkpoints.push((wf, h));
        }
        for (i, row) in rows.iter().enumerate() {
            if crate::core_game::paused(row) {
                continue;
            }
            let r: Vec<u16> = row.iter().map(|v| v & BUTTONS_MASK).collect();
            log.push(&r);
            wf += 1;
            let k = base as usize + i + 1;
            if wf % iv as u64 == 0 || k == end as usize {
                if let Some(&h) = hashes.get(k) {
                    if checkpoints.last().is_none_or(|c| c.0 != wf) {
                        checkpoints.push((wf, h));
                    }
                }
            }
        }
        let mut config = serde_json::to_value(&self.start.config.battle).unwrap_or_default();
        if self.start.config.pvp {
            config["pvp"] = serde_json::Value::Bool(true);
        }
        let b = &self.start.config.battle;
        let fighters: Vec<&str> = b.players.iter().map(|f| f.fighter.as_str()).collect();
        let enemies: Vec<&str> = b.phases.iter().map(|p| p.enemy.as_str()).collect();
        let header = wf_replay::Header {
            sim: "wf-core/battle".into(),
            content: self.content.clone(),
            config,
            seed: match b.rng {
                wf_core::battle::RngInit::Seed(s) => s as u64,
                wf_core::battle::RngInit::State(a, c) => ((a as u64) << 32) | c as u64,
            },
            players: n as u8,
            hash_interval: iv,
            title: format!(
                "online{}{}: {} ({})",
                if self.start.config.pvp { " PvP" } else { "" },
                if base > 0 { " (rejoined)" } else { "" },
                b.arena,
                if enemies.is_empty() { fighters.join(" vs ") } else { format!("{} vs {}", fighters.join("+"), enemies.join(", ")) }
            ),
            created: self.created,
            outcome: format!("{:?}", self.game().s.outcome).to_lowercase(),
            ..Default::default()
        };
        wf_replay::Replay { header, start_state, inputs: log, checkpoints }
    }

    /// Save the replay into `record_dir` (native; the file name is stable per match, so it is overwritten).
    pub fn save_replay(&mut self) -> Option<std::path::PathBuf> {
        self.last_record_tick = self.ticks;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = &self.record_dir {
            let r = self.replay(None);
            if r.inputs.is_empty() {
                return None;
            }
            let _ = std::fs::create_dir_all(dir);
            let p = dir.join(format!("{:010}-net{:08x}{}.wfr", self.created, self.start.session, if self.start_state.is_some() { "-rejoin" } else { "" }));
            if std::fs::write(&p, r.to_bytes()).is_ok() {
                return Some(p);
            }
        }
        None
    }

    /// Tell the peers we leave.
    pub fn bye(&mut self) {
        if let Some(p) = self.save_replay() {
            eprintln!("wf-net: match saved as replay {}", p.display());
        }
        let now = (self.clock)();
        let mut out = Vec::new();
        if let Mode::Player(p) = &self.mode {
            p.bye(&mut out);
        }
        for (to, b) in out {
            self.transport.send(&to, b, now);
        }
        let _ = self.transport.recv(now + 1e6);
    }
}

fn rle(v: &[u16]) -> Vec<(u16, u32)> {
    let mut out: Vec<(u16, u32)> = Vec::new();
    for &x in v {
        match out.last_mut() {
            Some((y, n)) if *y == x => *n += 1,
            _ => out.push((x, 1)),
        }
    }
    out
}

/// Re-simulate a match from its config and input log; the hash of every state 0..=frames.
pub fn replay_hashes(data: Arc<GameData>, cfg: &MatchConfig, inputs: &[Vec<u16>], frames: usize) -> Vec<u64> {
    let mut g = CoreGame::new(cfg.session(data));
    g.collect_events = false;
    let mut out = vec![g.s.hash()];
    for f in 0..frames {
        let inp: Vec<u16> = inputs.iter().map(|v| v.get(f).copied().unwrap_or(0)).collect();
        g.advance(&inp, false);
        out.push(g.s.hash());
    }
    out
}

impl Sim for NetSim {
    fn step(&mut self, inputs: &[u16], events: &mut Vec<SimEvent>) {
        self.tick(inputs.first().copied().unwrap_or(0));
        events.append(&mut self.game_mut().events);
    }
    fn view(&self, out: &mut RenderView) {
        self.game().s.view(out)
    }
    fn frame(&self) -> u64 {
        self.game().s.world.frame as u64
    }
    fn state_hash(&self) -> u64 {
        self.game().s.hash()
    }
    fn players(&self) -> usize {
        1
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    /// the match cannot wait for one player's menu: keep running, standing still (or holding the shared pause)
    fn paused_input(&self) -> Option<Buttons> {
        Some(if self.start.pause == "shared" && matches!(self.mode, Mode::Player(_)) { PAUSE_BIT } else { 0 })
    }
}

/// Local sync-test mode as a `Sim`: the battle runs normally (one local player), and every frame is rolled back
/// `check_distance` frames and re-simulated; a mismatch is reported in the status (and on stderr).
pub struct SyncSim {
    pub st: SyncTestSession<CoreGame>,
    pub status: Rc<RefCell<NetStatus>>,
    pub players: usize,
    pub error: Option<String>,
}

impl SyncSim {
    pub fn new(cfg: &MatchConfig, data: Arc<GameData>, check_distance: u32, bytes: bool) -> Self {
        let mut st = SyncTestSession::new(CoreGame::new(cfg.session(data)), check_distance);
        st.bytes = bytes;
        let status = NetStatus { phase: "synctest".into(), local_slot: Some(0), pvp: cfg.pvp, ..Default::default() };
        let status = Rc::new(RefCell::new(status));
        set_current(&status);
        SyncSim { st, status, players: cfg.battle.players.len(), error: None }
    }
}

impl Sim for SyncSim {
    fn step(&mut self, inputs: &[u16], events: &mut Vec<SimEvent>) {
        let mut v = inputs.to_vec();
        v.resize(self.players.max(1), 0);
        if let Err(e) = self.st.advance(&v) {
            if self.error.is_none() {
                eprintln!("wf-net: {e}");
                self.error = Some(e.to_string());
                self.status.borrow_mut().desync = Some(e.to_string());
            }
        }
        events.append(&mut self.st.game.events);
        let mut s = self.status.borrow_mut();
        s.frame = self.st.frame();
        s.stats.hash_checks = self.st.frames_checked;
        s.stats.resim_frames = self.st.resim_frames;
        s.outcome = format!("{:?}", self.st.game.s.outcome).to_lowercase();
        s.sfx = self.st.game.sfx_stats.clone();
    }
    fn view(&self, out: &mut RenderView) {
        self.st.game.s.view(out)
    }
    fn frame(&self) -> u64 {
        self.st.game.s.world.frame as u64
    }
    fn state_hash(&self) -> u64 {
        self.st.game.s.hash()
    }
    fn players(&self) -> usize {
        self.players.max(1)
    }
}

impl Drop for NetSim {
    /// a closed window / page: save the replay, tell the peers
    fn drop(&mut self) {
        self.bye();
    }
}
