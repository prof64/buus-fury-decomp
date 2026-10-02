//! The frontend side: [`NetLobby`] (room, hellos, start) and [`NetSim`], a `wf_view::Sim` that runs the match
//! through the rollback session, so `wf_app::App` (native and browser) renders and plays it like a local battle.
//! [`SyncSim`] is the local sync-test mode as a `Sim`. Status for overlays / UIs is shared through
//! `Rc<RefCell<NetStatus>>`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde::Serialize;
use wf_core::content::GameData;
use wf_view::{RenderView, Sim, SimEvent};

use crate::core_game::{CoreGame, MatchConfig};
use crate::game::Game;
use crate::lobby::{short, Lobby, LobbyEvent, LobbySettings, StartInfo};
use crate::proto::{decode, Msg};
use crate::session::{NetConfig, NetEvent, NetStats, P2PSession};
use crate::spectator::SpectatorSession;
use crate::synctest::SyncTestSession;
use crate::transport::{PeerChange, Transport};

pub type Clock = Rc<dyn Fn() -> f64>;

thread_local! {
    static CURRENT: RefCell<Option<Rc<RefCell<NetStatus>>>> = const { RefCell::new(None) };
}

/// The status of the newest lobby / match / sync test on this thread (shells draw the overlay from it without
/// holding the sim).
pub fn current_status() -> Option<Rc<RefCell<NetStatus>>> {
    CURRENT.with(|c| c.borrow().clone())
}
fn set_current(s: &Rc<RefCell<NetStatus>>) {
    CURRENT.with(|c| *c.borrow_mut() = Some(s.clone()));
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NetStatus {
    /// connecting | lobby | running | spectating | rejected | ended | synctest
    pub phase: String,
    pub room: String,
    pub me: String,
    pub role: String,
    pub local_slot: Option<usize>,
    /// (name, role) in the lobby / (name, slot) in the match
    pub roster: Vec<(String, String)>,
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
    pub outcome: String,
    pub winner: Option<usize>,
    pub pvp: bool,
    pub log: Vec<String>,
    pub spect_behind: i32,
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

pub struct NetLobby {
    pub transport: Box<dyn Transport>,
    pub lobby: Lobby,
    pub status: Rc<RefCell<NetStatus>>,
    clock: Clock,
    out: Vec<(String, Vec<u8>)>,
}

impl NetLobby {
    pub fn new(transport: Box<dyn Transport>, settings: LobbySettings, room: &str, clock: Clock) -> Self {
        let nonce = ((clock)() as u64).wrapping_mul(0x9E37_79B9).wrapping_add(room.len() as u64 * 7919) as u32 ^ 0x5A5A_1234;
        let st = NetStatus { phase: "connecting".into(), room: room.to_string(), role: format!("{:?}", settings.role).to_lowercase(), want_players: settings.want_players, pvp: settings.base.pvp, ..Default::default() };
        let status = Rc::new(RefCell::new(st));
        set_current(&status);
        NetLobby { transport, lobby: Lobby::new(settings, nonce), status, clock, out: Vec::new() }
    }

    /// Pump the transport and the lobby; true once the match starts (or we were refused: see `status`).
    pub fn poll(&mut self) -> bool {
        let now = (self.clock)();
        for c in self.transport.changes() {
            match c {
                PeerChange::Connected(id) => self.lobby.connected(&id),
                PeerChange::Disconnected(id) => self.lobby.disconnected(&id),
            }
        }
        let Some(me) = self.transport.local_id() else { return false };
        for (from, b) in self.transport.recv(now) {
            if let Some(Msg::Lobby(j)) = decode(&b) {
                self.lobby.handle(&me, &from, &j, &mut self.out);
            }
        }
        self.lobby.poll(&me, now, &mut self.out);
        for (to, b) in self.out.drain(..) {
            self.transport.send(&to, b, now);
        }
        let mut s = self.status.borrow_mut();
        s.me = me;
        s.phase = if self.lobby.rejected.is_some() { "rejected".into() } else if self.lobby.start.is_some() { "starting".into() } else { "lobby".into() };
        s.roster = self.lobby.roster().into_iter().map(|(_, n, r)| (n, format!("{r:?}").to_lowercase())).collect();
        s.players_present = self.lobby.players_present();
        s.log = self.lobby.log.iter().rev().take(8).rev().cloned().collect();
        if let Some(r) = &self.lobby.rejected {
            s.desync = None;
            s.outcome = format!("refused: {r}");
        }
        self.lobby.start.is_some() || self.lobby.rejected.is_some()
    }

    pub fn started(&self) -> Option<&StartInfo> {
        self.lobby.start.as_ref()
    }

    /// The match, once `poll` returned true and `started()` is Some.
    pub fn into_sim(self, data: Arc<GameData>) -> Result<NetSim, String> {
        let st = self.lobby.start.clone().ok_or_else(|| self.lobby.rejected.clone().unwrap_or_else(|| "not started".into()))?;
        let me = self.transport.local_id().unwrap_or_default();
        let now = (self.clock)();
        let mut game = CoreGame::new(st.config.session(data));
        game.collect_events = true;
        let ncfg = NetConfig { slots: st.players.len(), input_delay: st.input_delay, max_prediction: st.max_prediction, session: st.session, ..Default::default() };
        let mode = match st.players.iter().position(|s| s.id == me) {
            Some(slot) => {
                let peers = st.players.iter().enumerate().filter(|(_, s)| s.id != me).map(|(k, s)| (s.id.clone(), vec![k])).collect();
                let host = st.host == me;
                let specs = if host { st.spectators.clone() } else { Vec::new() };
                Mode::Player(Box::new(P2PSession::new(game, ncfg, vec![slot], peers, specs, host, now)))
            }
            None => Mode::Spectator(Box::new(SpectatorSession::new(game, st.session, st.host.clone(), st.players.len(), now))),
        };
        {
            let mut s = self.status.borrow_mut();
            s.phase = if matches!(mode, Mode::Player(_)) { "running".into() } else { "spectating".into() };
            s.local_slot = st.players.iter().position(|s| s.id == me);
            s.roster = st.players.iter().enumerate().map(|(k, p)| (p.name.clone(), format!("P{}", k + 1))).collect();
            s.pvp = st.config.pvp;
        }
        Ok(NetSim { transport: self.transport, lobby: self.lobby, mode, clock: self.clock, status: self.status, me, start: st, out: Vec::new(), report_dir: None, record_dir: None, content: Default::default(), created: 0, last_record_tick: 0, ticks: 0, reported: false })
    }
}

pub enum Mode {
    Player(Box<P2PSession<CoreGame>>),
    Spectator(Box<SpectatorSession<CoreGame>>),
}

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
                (Msg::Lobby(j), mode) => {
                    if let Some(LobbyEvent::SpectatorJoined(id)) = self.lobby.handle(&self.me, &from, j, &mut self.out) {
                        if let Mode::Player(p) = mode {
                            p.add_spectator(id, now);
                        }
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
            Mode::Player(p) => p.tick(now, &*clock, &[local], &mut self.out),
            Mode::Spectator(s) => {
                let n = s.tick(&mut self.out);
                crate::session::Tick { advanced: n, ..Default::default() }
            }
        };
        for (to, b) in self.out.drain(..) {
            self.transport.send(&to, b, now);
        }
        self.update_status();
        if self.record_dir.is_some() && self.ticks - self.last_record_tick >= 1800 {
            self.save_replay();
        }
        t
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
                    self.status.borrow_mut().disconnected.push(name);
                }
            }
        }
        let g = &self.game().s;
        let outcome = format!("{:?}", g.outcome).to_lowercase();
        let winner = wf_core::battle::pvp_winner(&g.world);
        let mut s = self.status.borrow_mut();
        s.outcome = outcome;
        s.winner = winner;
        match &self.mode {
            Mode::Player(p) => {
                s.frame = p.frame();
                s.confirmed = p.confirmed().min(p.frame());
                s.hashed = p.hashed();
                s.last_hash = p.hashes.last().map(|h| format!("{h:016x}")).unwrap_or_default();
                s.stats = p.stats.clone();
                s.rtt = p.peers.iter().filter(|q| !q.spectator).map(|q| (short(&q.id).to_string(), q.rtt_ms)).collect();
                s.wait_frames = p.wait_recommendation();
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
            }
        }
    }

    /// JSON: what happened, the match config and the full input log (a replay: re-simulate it with
    /// [`replay_hashes`] on both peers' reports to find the first divergent frame), recent hashes, observations.
    pub fn desync_report(&self, frame: i32, local: u64, remote: u64, peer: &str, chain: bool) -> String {
        let (inputs, hashes): (Vec<Vec<u16>>, &Vec<u64>) = match &self.mode {
            Mode::Player(p) => (p.known_inputs().to_vec(), &p.hashes),
            Mode::Spectator(s) => {
                let n = self.start.players.len();
                ((0..n).map(|k| s.inputs().iter().map(|f| f[k]).collect()).collect(), &s.hashes)
            }
        };
        let rle: Vec<Vec<(u16, u32)>> = inputs.iter().map(|v| rle(v)).collect();
        let from = hashes.len().saturating_sub(600);
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
            "inputs_rle": rle,
            "hashes": recent,
            "frame_now": w.frame,
            "observations_now": obs,
        })
        .to_string()
    }

    /// The match so far as a wf-replay recording (`.wfr`, engine/docs/replays.md): config (+ `"pvp": true`, which
    /// the app's replay player applies), every slot's confirmed inputs and the state-hash checkpoints, up to the
    /// newest state every peer confirmed (or `upto`, e.g. a desync frame). Plays and verifies like any recording.
    pub fn replay(&self, upto: Option<i32>) -> wf_replay::Replay {
        let n = self.start.players.len();
        let (inputs, hashes): (Vec<Vec<u16>>, &Vec<u64>) = match &self.mode {
            Mode::Player(p) => (p.known_inputs().to_vec(), &p.hashes),
            Mode::Spectator(s) => ((0..n).map(|k| s.inputs().iter().map(|f| f[k]).collect()).collect(), &s.hashes),
        };
        let mut end = hashes.len() as i32 - 1;
        if let Some(u) = upto {
            end = end.min(u);
        }
        let end = end.max(0) as usize;
        let mut log = wf_replay::InputLog::new(n, 0);
        for f in 0..end {
            let row: Vec<u16> = (0..n).map(|k| inputs[k].get(f).copied().unwrap_or(0)).collect();
            log.push(&row);
        }
        let iv = wf_replay::DEFAULT_HASH_INTERVAL as usize;
        let mut checkpoints: Vec<(u64, u64)> = (0..=end).filter(|k| k % iv == 0 || *k == end).filter_map(|k| hashes.get(k).map(|&h| (k as u64, h))).collect();
        checkpoints.dedup();
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
            hash_interval: wf_replay::DEFAULT_HASH_INTERVAL,
            title: format!("online{}: {} ({})", if self.start.config.pvp { " PvP" } else { "" }, b.arena, if enemies.is_empty() { fighters.join(" vs ") } else { format!("{} vs {}", fighters.join("+"), enemies.join(", ")) }),
            created: self.created,
            outcome: format!("{:?}", self.game().s.outcome).to_lowercase(),
            ..Default::default()
        };
        wf_replay::Replay { header, start_state: None, inputs: log, checkpoints }
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
            let p = dir.join(format!("{:010}-net{:08x}.wfr", self.created, self.start.session));
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
