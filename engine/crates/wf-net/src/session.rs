//! The P2P rollback session (GGPO model): every peer simulates every frame immediately with its local inputs
//! (delayed by `input_delay`) and predicted remote inputs (= the last known input of that slot); when a remote input
//! arrives that differs from the prediction, the session loads the saved state of that frame and re-simulates up to
//! the present. No I/O and no clock inside: the host feeds received packets ([`P2PSession::handle`]), calls
//! [`P2PSession::tick`] once per frame with the current time and collects the packets to send. This makes the
//! session identical on native and wasm and testable on a virtual network.
//!
//! * **Prediction window**: the session never runs more than `max_prediction` frames past the last frame whose
//!   inputs are all known; then it stalls (counted in `stats.stalls`) until inputs arrive.
//! * **Time sync**: each packet carries the sender's frame and its frame advantage; the peer that runs ahead skips
//!   a tick now and then (at most one per `skip_interval` ticks) so both sides predict about equally far.
//! * **Desync detection**: a state is *globally confirmed* when every connected peer has all inputs before it; each
//!   peer hashes its globally confirmed states (`Game::checksum`), keeps a hash chain and sends the newest hashes;
//!   any difference raises [`NetEvent::Desync`] (comparing the chain covers every earlier frame too).
//! * **Disconnects**: a peer silent for `disconnect_ms` is dropped; its slots get neutral input from the frame
//!   after its last input everyone has (peers reconcile to the minimum via the status in every packet).
//! * **Rejoin**: the acting host (the connected player with the lowest slot) hands a disconnected slot to a
//!   rejoining peer ([`P2PSession::grant_rejoin`]): the slot stays neutral up to a frame `from` beyond every input the
//!   host has sent, then its inputs come from the new peer. The change travels with the host's inputs (every packet,
//!   until the receiver's status shows the new seat epoch), so no peer can confirm a frame past `from` without it.
//!   The rejoiner starts from the host's newest globally confirmed state (serialized snapshot + its hash and chain
//!   value, verified on load: [`P2PSession::resume`]) and catches up with confirmed inputs at several frames per tick.
//! * **Spectators**: the acting host forwards confirmed inputs (all slots) to spectators ([`crate::spectator`]).

use std::collections::VecDeque;

use crate::game::{chain_mix, Game};
use crate::proto::{InputMsg, Msg, SeatChange, SlotStatus, SpectMsg, MAX_FRAMES_PER_PACKET};

#[derive(Clone, Debug, PartialEq)]
pub struct NetConfig {
    /// player slots in the match (1..=4)
    pub slots: usize,
    /// frames between a local button press and the frame it applies to
    pub input_delay: u32,
    /// frames the session may run ahead of the confirmed inputs before stalling
    pub max_prediction: u32,
    /// a peer that sent nothing for this long is dropped
    pub disconnect_ms: f64,
    /// simulation rate (for time sync estimates)
    pub fps: f64,
    /// match nonce (stray packets from other matches are ignored)
    pub session: u32,
    pub time_sync: bool,
    /// at most one time-sync skip every this many ticks
    pub skip_interval: u32,
    /// frames simulated per tick at most while behind the other peers (rejoin, after a long stall)
    pub catchup: u32,
    /// a rejoined slot is handed over this many frames after the host's newest input (time to load + catch up)
    pub rejoin_lead: i32,
    /// a freshly seated (rejoining) peer may stay silent this long before it is dropped again
    pub rejoin_grace_ms: f64,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            slots: 2,
            input_delay: 2,
            max_prediction: 12,
            disconnect_ms: 5000.0,
            fps: 59.7275,
            session: 0,
            time_sync: true,
            skip_interval: 12,
            catchup: 8,
            rejoin_lead: 90,
            rejoin_grace_ms: 20_000.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NetEvent {
    /// a peer's state hash differs from ours (state index = frames simulated)
    Desync { frame: i32, local: u64, remote: u64, peer: String, chain: bool },
    Disconnected { peer: String, slots: Vec<usize>, frame: i32 },
    /// a disconnected slot was handed to (rejoining) peer `peer`: its inputs count from frame `from`
    Rejoined { peer: String, slot: usize, from: i32 },
}

/// Counters for the overlay, tests and reports.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct NetStats {
    /// frames simulated for the first time
    pub frames: u64,
    pub rollbacks: u64,
    /// frames re-simulated by rollbacks
    pub resim_frames: u64,
    pub max_rollback: u32,
    /// wall time spent in rollbacks (load + re-simulate + save), µs
    pub rollback_us: f64,
    pub rollback_us_max: f64,
    /// ticks that could not advance (prediction window full)
    pub stalls: u64,
    /// ticks skipped by time sync
    pub skips: u64,
    /// frames simulated in catch-up (behind the others: rejoin)
    pub catchup_frames: u64,
    pub packets_sent: u64,
    pub packets_recv: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    /// remote hashes compared with ours
    pub hash_checks: u64,
    pub desyncs: u64,
    /// time spent in plain (first-time) frames, µs
    pub step_us: f64,
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub id: String,
    /// slots this peer controls (empty = spectator)
    pub slots: Vec<usize>,
    pub spectator: bool,
    pub connected: bool,
    last_recv: f64,
    /// not dropped for silence before this time (a rejoining peer loading the snapshot)
    grace_until: f64,
    /// how many of our local inputs (or, to a spectator, confirmed frames) the peer has
    acked: i32,
    pub rtt_ms: f64,
    last_ping: u32,
    last_ping_at: f64,
    remote_frame: i32,
    local_adv: f64,
    remote_adv: f64,
    /// the peer's view of every slot (known inputs / disconnect frame, seat epoch)
    status: Vec<SlotStatus>,
    pending_hashes: Vec<(i32, u64)>,
    pending_chain: Option<(i32, u64)>,
    desynced: bool,
    got_any: bool,
    /// it said goodbye (left on purpose)
    pub bye: bool,
    /// dropped because it went silent (not announced by a goodbye or the transport): maybe it is us who lost
    /// the connection
    pub timed_out: bool,
}

impl Peer {
    fn new(id: String, slots: Vec<usize>, spectator: bool, nslots: usize, now: f64) -> Peer {
        Peer {
            id,
            slots,
            spectator,
            connected: true,
            last_recv: now,
            grace_until: now,
            acked: 0,
            rtt_ms: 0.0,
            last_ping: 0,
            last_ping_at: now,
            remote_frame: 0,
            local_adv: 0.0,
            remote_adv: 0.0,
            status: vec![SlotStatus::default(); nslots],
            pending_hashes: Vec::new(),
            pending_chain: None,
            desynced: false,
            got_any: false,
            bye: false,
            timed_out: false,
        }
    }
}

/// What one tick did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tick {
    /// frames simulated for the first time (0 or 1, more while catching up)
    pub advanced: u32,
    /// frames re-simulated
    pub resimulated: u32,
    pub stalled: bool,
    pub skipped: bool,
}

/// What a rejoining peer needs besides the state snapshot (sent as JSON by the lobby, see `lobby::GrantInfo`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RejoinGrant {
    pub slot: usize,
    /// first state index of the rejoiner (the snapshot's), its state hash and hash-chain value
    pub base: i32,
    pub hash: u64,
    pub chain: u64,
    /// per slot: seat epoch, the frame the current seat's inputs start at, disconnect frame
    pub epochs: Vec<u8>,
    pub seat_from: Vec<i32>,
    pub seat_peer: Vec<String>,
    pub disc: Vec<Option<i32>>,
    /// per slot: inputs from `base` for slots no connected peer serves (the rejoiner's own up to `from`, other
    /// dropped players'); empty for connected peers' slots (they send them)
    pub tails: Vec<Vec<u16>>,
    /// (peer id, slots) of every connected player, the host included
    pub peers: Vec<(String, Vec<usize>)>,
    pub spectators: Vec<String>,
}

pub struct P2PSession<G: Game> {
    pub cfg: NetConfig,
    pub game: G,
    pub local_slots: Vec<usize>,
    pub is_host: bool,
    /// next frame to simulate (= frames simulated = index of the current state)
    frame: i32,
    /// highest frame ever simulated + 1 (frames below were simulated before: re-simulations)
    high: i32,
    /// first state index of this session (a rejoined session starts from a snapshot); earlier hashes are not checked
    base: i32,
    /// known inputs per slot, contiguous from frame 0 (the full match log; before `base`: placeholders)
    known: Vec<Vec<u16>>,
    /// slot disconnected from this frame on (its inputs are 0 there)
    disc: Vec<Option<i32>>,
    /// seat epoch per slot and the change that started it (re-sent to peers that have not applied it)
    epoch: Vec<u8>,
    seat: Vec<Option<SeatChange>>,
    /// inputs used for each simulated frame
    used: Vec<Vec<u16>>,
    /// saved states (index, state), ascending
    states: VecDeque<(i32, G::State)>,
    /// hash / chain of every globally confirmed state, by state index
    pub hashes: Vec<u64>,
    pub chain: Vec<u64>,
    rollback_from: Option<i32>,
    pub peers: Vec<Peer>,
    pub events: Vec<NetEvent>,
    pub stats: NetStats,
    since_skip: u32,
}

fn ms32(now: f64) -> u32 {
    (now as i64) as u32
}

/// `a` is a newer seat epoch than `b` (wrapping)
fn newer(a: u8, b: u8) -> bool {
    let d = a.wrapping_sub(b);
    d != 0 && d < 128
}

impl<G: Game> P2PSession<G> {
    /// `peers`: (transport id, slots) of every other player; `spectators`: transport ids (served by the acting host).
    pub fn new(game: G, cfg: NetConfig, local_slots: Vec<usize>, peers: Vec<(String, Vec<usize>)>, spectators: Vec<String>, is_host: bool, now: f64) -> Self {
        let n = cfg.slots.max(1);
        let mut s = P2PSession {
            game,
            local_slots,
            is_host,
            frame: 0,
            high: 0,
            base: 0,
            known: vec![Vec::new(); n],
            disc: vec![None; n],
            epoch: vec![0; n],
            seat: vec![None; n],
            used: Vec::new(),
            states: VecDeque::new(),
            hashes: Vec::new(),
            chain: Vec::new(),
            rollback_from: None,
            peers: Vec::new(),
            events: Vec::new(),
            stats: NetStats::default(),
            since_skip: 0,
            cfg,
        };
        for (id, slots) in peers {
            s.peers.push(Peer::new(id, slots, false, n, now));
        }
        for id in spectators {
            s.peers.push(Peer::new(id, Vec::new(), true, n, now));
        }
        s.game.set_frame_index(0);
        let st = s.game.save();
        s.states.push_back((0, st));
        s
    }

    /// A rejoining peer's session: `game` already holds the snapshot state of `g.base` (see
    /// `lobby::GrantInfo`); its hash is checked against the host's. Errors when the snapshot does not verify.
    pub fn resume(mut game: G, cfg: NetConfig, g: &RejoinGrant, me: &str, now: f64) -> Result<Self, String> {
        let n = cfg.slots.max(1);
        let base = g.base.max(0);
        game.set_frame_index(base);
        let st = game.save();
        let h = game.checksum(&st);
        if h != g.hash {
            return Err(format!("the rejoin snapshot does not verify: state {base} hashes {h:016x}, the host has {:016x}", g.hash));
        }
        let mut known = vec![vec![0u16; base as usize]; n];
        for (s, t) in g.tails.iter().enumerate().take(n) {
            known[s].extend_from_slice(t);
        }
        let mut seat = vec![None; n];
        for s in 0..n {
            let e = g.epochs.get(s).copied().unwrap_or(0);
            if e != 0 {
                seat[s] = Some(SeatChange { slot: s as u8, epoch: e, from: g.seat_from.get(s).copied().unwrap_or(0), peer: g.seat_peer.get(s).cloned().unwrap_or_default() });
            }
        }
        let mut hashes = vec![0u64; base as usize];
        hashes.push(h);
        let mut chain = vec![0u64; base as usize];
        chain.push(g.chain);
        let mut s = P2PSession {
            game,
            local_slots: vec![g.slot],
            is_host: false,
            frame: base,
            high: base,
            base,
            known,
            disc: g.disc.iter().copied().chain(std::iter::repeat(None)).take(n).collect(),
            epoch: g.epochs.iter().copied().chain(std::iter::repeat(0)).take(n).collect(),
            seat,
            used: vec![vec![0u16; n]; base as usize],
            states: VecDeque::new(),
            hashes,
            chain,
            rollback_from: None,
            peers: Vec::new(),
            events: Vec::new(),
            stats: NetStats::default(),
            since_skip: 0,
            cfg,
        };
        for (id, slots) in &g.peers {
            if id != me {
                let mut p = Peer::new(id.clone(), slots.clone(), false, n, now);
                p.acked = base;
                p.grace_until = now + s.cfg.rejoin_grace_ms;
                s.peers.push(p);
            }
        }
        for id in &g.spectators {
            s.peers.push(Peer::new(id.clone(), Vec::new(), true, n, now));
        }
        s.states.push_back((base, st));
        Ok(s)
    }

    pub fn add_spectator(&mut self, id: String, now: f64) {
        if !self.peers.iter().any(|p| p.id == id) {
            self.peers.push(Peer::new(id, Vec::new(), true, self.cfg.slots, now));
        }
    }

    pub fn frame(&self) -> i32 {
        self.frame
    }
    pub fn base(&self) -> i32 {
        self.base
    }
    /// inputs known for every slot up to (excluding) this frame
    pub fn confirmed(&self) -> i32 {
        (0..self.cfg.slots).map(|s| self.conf_len(s)).min().unwrap_or(0)
    }
    /// states up to (including) this index are hashed and final on every peer
    pub fn hashed(&self) -> i32 {
        self.hashes.len() as i32 - 1
    }
    pub fn known_inputs(&self) -> &[Vec<u16>] {
        &self.known
    }
    pub fn used_inputs(&self) -> &[Vec<u16>] {
        &self.used
    }
    /// the input of `slot` at frame `f` as the match uses it (neutral after a disconnect)
    pub fn input_at(&self, slot: usize, f: i32) -> u16 {
        self.input_for(slot, f)
    }
    pub fn disconnected_slots(&self) -> Vec<usize> {
        (0..self.cfg.slots).filter(|&s| self.disc[s].is_some()).collect()
    }
    pub fn seat_epoch(&self, slot: usize) -> u8 {
        self.epoch.get(slot).copied().unwrap_or(0)
    }

    /// We are the acting host: the connected player with the lowest slot (the original host while it is there).
    /// It serves spectators and grants rejoins.
    pub fn acting_host(&self) -> bool {
        for s in 0..self.cfg.slots {
            if self.local_slots.contains(&s) {
                return true;
            }
            if self.peers.iter().any(|p| p.connected && !p.spectator && p.slots.contains(&s)) {
                return false;
            }
        }
        false
    }

    fn conf_len(&self, s: usize) -> i32 {
        if self.disc[s].is_some() {
            i32::MAX
        } else {
            self.known[s].len() as i32
        }
    }

    fn input_for(&self, s: usize, f: i32) -> u16 {
        if let Some(d) = self.disc[s] {
            if f >= d {
                return 0;
            }
        }
        let k = &self.known[s];
        if (f as usize) < k.len() {
            k[f as usize]
        } else {
            k.last().copied().unwrap_or(0)
        }
    }

    fn mark_rollback(&mut self, f: i32) {
        if f < self.frame {
            self.rollback_from = Some(self.rollback_from.map_or(f, |r| r.min(f)));
        }
    }

    /// slots' disconnect frame lowered / set: frames from there were simulated with real inputs -> roll back
    fn set_disc(&mut self, s: usize, d: i32) {
        let d = d.min(self.known[s].len() as i32).max(0);
        match self.disc[s] {
            Some(old) if old <= d => return,
            _ => {}
        }
        self.disc[s] = Some(d);
        for f in d.max(self.base)..self.frame {
            if self.used.get(f as usize).is_some_and(|u| u[s] != 0) {
                self.mark_rollback(f);
                break;
            }
        }
    }

    /// Hand slot `s` to `peer` from frame `from` on (epoch `epoch`): neutral inputs from the disconnect frame up
    /// to `from`, then the new peer's.
    fn apply_seat(&mut self, c: &SeatChange, now: f64) {
        let s = c.slot as usize;
        if s >= self.cfg.slots || !newer(c.epoch, self.epoch[s]) || self.local_slots.contains(&s) {
            return;
        }
        self.epoch[s] = c.epoch;
        self.seat[s] = Some(c.clone());
        let d = self.disc[s].unwrap_or(self.known[s].len() as i32).max(0);
        let k = &mut self.known[s];
        k.truncate(d as usize);
        while (k.len() as i32) < c.from {
            k.push(0);
        }
        self.disc[s] = None;
        // frames from the disconnect on were simulated with 0 for this slot (or real inputs, if we had not dropped
        // the slot yet): roll back to the first one that differs
        for f in d.max(self.base)..self.frame {
            if self.used.get(f as usize).is_some_and(|u| u[s] != self.input_for(s, f)) {
                self.mark_rollback(f);
                break;
            }
        }
        for p in self.peers.iter_mut() {
            if p.id != c.peer {
                p.slots.retain(|&x| x != s);
            }
        }
        match self.peers.iter_mut().find(|p| p.id == c.peer) {
            Some(p) => {
                if !p.slots.contains(&s) {
                    p.slots.push(s);
                }
                p.spectator = false;
                p.connected = true;
                p.last_recv = now;
                p.grace_until = now + self.cfg.rejoin_grace_ms;
            }
            None => {
                let mut p = Peer::new(c.peer.clone(), vec![s], false, self.cfg.slots, now);
                p.grace_until = now + self.cfg.rejoin_grace_ms;
                self.peers.push(p);
            }
        }
        self.events.push(NetEvent::Rejoined { peer: c.peer.clone(), slot: s, from: c.from });
    }

    /// Can slot `s` be handed to a rejoining peer now? (we act as host, it is disconnected and every connected
    /// player agrees on its disconnect frame and epoch)
    pub fn rejoin_ready(&self, s: usize) -> Result<(), String> {
        if s >= self.cfg.slots || self.local_slots.contains(&s) {
            return Err("not a remote slot".into());
        }
        if !self.acting_host() {
            return Err("not the acting host".into());
        }
        let Some(d) = self.disc[s] else { return Err("the slot is still connected".into()) };
        for p in self.peers.iter().filter(|p| p.connected && !p.spectator) {
            let st = p.status[s];
            if !p.got_any || !st.disc || st.known != d || st.epoch != self.epoch[s] {
                return Err(format!("waiting for {} to agree on the disconnect", crate::lobby::short(&p.id)));
            }
        }
        if self.hashes.is_empty() {
            return Err("no confirmed state yet".into());
        }
        Ok(())
    }

    /// Acting host: hand disconnected slot `s` to `new_peer`. Returns what the rejoiner needs and the serialized
    /// snapshot of the newest globally confirmed state.
    pub fn grant_rejoin(&mut self, s: usize, new_peer: &str, now: f64) -> Result<(RejoinGrant, Vec<u8>), String> {
        self.rejoin_ready(s)?;
        let c = self.hashed();
        let Some(st) = self.states.iter().find(|(i, _)| *i == c).map(|(_, st)| st.clone()) else {
            return Err(format!("state {c} is not saved"));
        };
        // serialize state c (load it, serialize, back to the present)
        let cur = self.states.back().map(|(_, st)| st.clone()).unwrap();
        self.game.load(&st);
        let bytes = self.game.save_bytes().ok_or("this game cannot serialize its state")?;
        self.game.load(&cur);
        // the seat: after every input we already sent, plus time for the rejoiner to load and catch up
        let sent = self.local_slots.iter().map(|&x| self.known[x].len() as i32).max().unwrap_or(0).max(self.frame);
        let from = sent + self.cfg.rejoin_lead.max(1);
        let change = SeatChange { slot: s as u8, epoch: self.epoch[s].wrapping_add(1).max(1), from, peer: new_peer.to_string() };
        self.apply_seat(&change, now);
        let n = self.cfg.slots;
        let mut tails = vec![Vec::new(); n];
        for (q, tail) in tails.iter_mut().enumerate() {
            let served = self.local_slots.contains(&q) || self.peers.iter().any(|p| p.connected && !p.spectator && p.id != new_peer && p.slots.contains(&q));
            if q == s || !served {
                let k = &self.known[q];
                *tail = k[(c as usize).min(k.len())..].to_vec();
            }
        }
        let mut peers = vec![(String::new(), self.local_slots.clone())];
        for p in self.peers.iter().filter(|p| p.connected && !p.spectator && !p.slots.is_empty()) {
            peers.push((p.id.clone(), p.slots.clone()));
        }
        let g = RejoinGrant {
            slot: s,
            base: c,
            hash: self.hashes[c as usize],
            chain: self.chain[c as usize],
            epochs: self.epoch.clone(),
            seat_from: self.seat.iter().map(|x| x.as_ref().map_or(0, |c| c.from)).collect(),
            seat_peer: self.seat.iter().map(|x| x.as_ref().map_or(String::new(), |c| c.peer.clone())).collect(),
            disc: self.disc.clone(),
            tails,
            peers,
            spectators: self.peers.iter().filter(|p| p.spectator && p.connected).map(|p| p.id.clone()).collect(),
        };
        Ok((g, bytes))
    }

    /// A packet from transport peer `from`.
    pub fn handle(&mut self, from: &str, msg: &Msg, now: f64) {
        let Some(pi) = self.peers.iter().position(|p| p.id == from) else {
            // a spectator we do not know yet (it joined late, or the host changed): serve it
            if let Msg::SpectAck { session, .. } = msg {
                if *session == self.cfg.session {
                    self.add_spectator(from.to_string(), now);
                }
            }
            return;
        };
        match msg {
            Msg::Input(m) if m.session == self.cfg.session => self.on_input(pi, m, now),
            Msg::SpectAck { session, ack } if *session == self.cfg.session => {
                let p = &mut self.peers[pi];
                if p.spectator {
                    p.connected = true;
                    p.last_recv = now;
                    p.acked = p.acked.max(*ack);
                }
            }
            Msg::Bye { session } if *session == self.cfg.session => {
                self.peers[pi].bye = true;
                self.drop_peer(pi)
            }
            _ => {}
        }
    }

    fn on_input(&mut self, pi: usize, m: &InputMsg, now: f64) {
        let fms = 1000.0 / self.cfg.fps;
        {
            let p = &mut self.peers[pi];
            if !p.connected {
                return;
            }
            p.got_any = true;
            p.last_recv = now;
            p.acked = p.acked.max(m.ack);
            if m.pong != 0 {
                let rtt = (ms32(now).wrapping_sub(m.pong) as i32 - m.hold as i32).max(0) as f64;
                if rtt < 10_000.0 {
                    p.rtt_ms = if p.rtt_ms == 0.0 { rtt } else { p.rtt_ms * 0.9 + rtt * 0.1 };
                }
            }
            p.last_ping = m.ping;
            p.last_ping_at = now;
            p.remote_frame = m.frame;
        }
        // time sync: where the peer is now (its frame + half a round trip) vs us
        let est = m.frame as f64 + self.peers[pi].rtt_ms / 2.0 / fms;
        let local_adv = self.frame as f64 - est;
        {
            let p = &mut self.peers[pi];
            p.local_adv = p.local_adv * 0.9 + local_adv * 0.1;
            p.remote_adv = p.remote_adv * 0.9 + (m.adv as f64 / 16.0) * 0.1;
        }
        // seat changes first: the inputs in this packet may depend on them
        for c in &m.seats {
            self.apply_seat(c, now);
        }
        // the peer's view of every slot; adopt its disconnects (of the same seat)
        for (s, &st) in m.status.iter().enumerate().take(self.cfg.slots) {
            self.peers[pi].status[s] = st;
            if st.disc && st.epoch == self.epoch[s] && !self.local_slots.contains(&s) {
                self.set_disc(s, st.known);
            }
        }
        // its inputs
        let slots = self.peers[pi].slots.clone();
        if m.slots as usize == slots.len() && !slots.is_empty() {
            let n = m.inputs.len() / slots.len();
            for i in 0..n {
                let f = m.start + i as i32;
                for (j, &s) in slots.iter().enumerate() {
                    if (f as usize) != self.known[s].len() {
                        continue;
                    }
                    if self.disc[s].is_some_and(|d| f >= d) {
                        continue;
                    }
                    let v = m.inputs[i * slots.len() + j];
                    self.known[s].push(v);
                    if f < self.frame && f >= self.base && self.used[f as usize][s] != v {
                        self.mark_rollback(f);
                    }
                }
            }
        }
        // its hashes
        let p = &mut self.peers[pi];
        for &h in &m.hashes {
            if h.0 >= self.base && !p.pending_hashes.contains(&h) {
                p.pending_hashes.push(h);
            }
        }
        if p.pending_hashes.len() > 64 {
            let cut = p.pending_hashes.len() - 64;
            p.pending_hashes.drain(..cut);
        }
        if m.chain.0 > 0 && m.chain.0 >= self.base {
            p.pending_chain = Some(m.chain);
        }
    }

    /// the transport reports a peer gone (WebRTC channel closed)
    pub fn peer_left(&mut self, id: &str) {
        if let Some(pi) = self.peers.iter().position(|p| p.id == id) {
            if self.peers[pi].spectator {
                self.peers[pi].connected = false;
            } else {
                self.drop_peer(pi);
            }
        }
    }

    /// drop a player peer now (a rejoin request proved that its old connection is stale)
    pub fn drop_peer_id(&mut self, id: &str) {
        if let Some(pi) = self.peers.iter().position(|p| p.id == id && !p.spectator) {
            self.drop_peer(pi);
        }
    }

    fn drop_peer(&mut self, pi: usize) {
        if !self.peers[pi].connected {
            return;
        }
        self.peers[pi].connected = false;
        let slots = self.peers[pi].slots.clone();
        for &s in &slots {
            let d = self.known[s].len() as i32;
            self.set_disc(s, d);
        }
        let f = slots.iter().filter_map(|&s| self.disc[s]).min().unwrap_or(self.frame);
        self.events.push(NetEvent::Disconnected { peer: self.peers[pi].id.clone(), slots, frame: f });
    }

    fn simulate(&mut self, f: i32) {
        let inputs: Vec<u16> = (0..self.cfg.slots).map(|s| self.input_for(s, f)).collect();
        let first = f >= self.high;
        self.game.advance(&inputs, first);
        if (f as usize) < self.used.len() {
            self.used[f as usize] = inputs;
        } else {
            self.used.push(inputs);
        }
        self.frame = f + 1;
        self.high = self.high.max(self.frame);
        let st = self.game.save();
        self.states.push_back((self.frame, st));
    }

    fn rollback(&mut self, rb: i32, clock: &dyn Fn() -> f64) -> u32 {
        let t0 = clock();
        let target = self.frame;
        let rb = rb.max(self.base);
        let Some(k) = self.states.iter().position(|(i, _)| *i == rb) else {
            // cannot happen: states are kept down to the oldest unconfirmed frame
            debug_assert!(false, "no saved state for frame {rb}");
            return 0;
        };
        self.states.truncate(k + 1);
        let st = self.states[k].1.clone();
        self.game.load(&st);
        self.frame = rb;
        for f in rb..target {
            self.simulate(f);
        }
        let n = (target - rb) as u32;
        let us = (clock() - t0) * 1000.0;
        self.stats.rollbacks += 1;
        self.stats.resim_frames += n as u64;
        self.stats.max_rollback = self.stats.max_rollback.max(n);
        self.stats.rollback_us += us;
        self.stats.rollback_us_max = self.stats.rollback_us_max.max(us);
        n
    }

    /// state index up to which every connected peer has all inputs (and so has the final state)
    fn global_confirmed(&self) -> i32 {
        let mut g = self.confirmed().min(self.frame);
        for p in self.peers.iter().filter(|p| p.connected && !p.spectator) {
            if !p.got_any {
                return self.hashed().max(0);
            }
            for s in 0..self.cfg.slots {
                if self.disc[s].is_some() {
                    continue;
                }
                let st = p.status[s];
                if st.epoch != self.epoch[s] {
                    // it has not taken over the slot's new seat yet: it has the (neutral) inputs up to the handover
                    g = g.min(self.seat[s].as_ref().map_or(0, |c| c.from));
                } else if !st.disc {
                    g = g.min(st.known);
                }
            }
        }
        g
    }

    fn hash_confirmed(&mut self) {
        let g = self.global_confirmed();
        while (self.hashes.len() as i32) <= g {
            let k = self.hashes.len() as i32;
            let Some((_, st)) = self.states.iter().find(|(i, _)| *i == k) else { break };
            let h = self.game.checksum(st);
            let c = chain_mix(self.chain.last().copied().unwrap_or(0), h);
            self.hashes.push(h);
            self.chain.push(c);
        }
        // keep states from the oldest one a rollback or the hashing may still need
        // (the newest hashed state too: a peer's disconnect frame can be exactly that state; a rejoin snapshot)
        let keep = (self.hashes.len() as i32 - 1).max(self.base).min(self.confirmed()).min(self.frame);
        while self.states.len() > 1 && self.states[0].0 < keep {
            self.states.pop_front();
        }
        // compare remote hashes
        let n = self.hashes.len() as i32;
        let base = self.base;
        let mut found = Vec::new();
        for p in self.peers.iter_mut() {
            let mut rest = Vec::new();
            for &(k, h) in &p.pending_hashes {
                if k < base {
                    continue;
                }
                if k < n {
                    self.stats.hash_checks += 1;
                    if self.hashes[k as usize] != h && !p.desynced {
                        p.desynced = true;
                        found.push(NetEvent::Desync { frame: k, local: self.hashes[k as usize], remote: h, peer: p.id.clone(), chain: false });
                    }
                } else {
                    rest.push((k, h));
                }
            }
            p.pending_hashes = rest;
            if let Some((k, c)) = p.pending_chain {
                if k < base {
                    p.pending_chain = None;
                } else if k < n {
                    p.pending_chain = None;
                    self.stats.hash_checks += 1;
                    if self.chain[k as usize] != c && !p.desynced {
                        p.desynced = true;
                        found.push(NetEvent::Desync { frame: k, local: self.chain[k as usize], remote: c, peer: p.id.clone(), chain: true });
                    }
                }
            }
        }
        self.stats.desyncs += found.len() as u64;
        self.events.extend(found);
    }

    /// frames to wait recommended by time sync (max over peers)
    pub fn wait_recommendation(&self) -> f64 {
        self.peers.iter().filter(|p| p.connected && !p.spectator && p.got_any).map(|p| (p.local_adv - p.remote_adv) / 2.0).fold(0.0, f64::max)
    }

    /// how far the most advanced connected player is ahead of us (estimated now), frames
    pub fn frames_behind(&self) -> f64 {
        let fms = 1000.0 / self.cfg.fps;
        self.peers.iter().filter(|p| p.connected && !p.spectator && p.got_any).map(|p| p.remote_frame as f64 + p.rtt_ms / 2.0 / fms - self.frame as f64).fold(0.0, f64::max)
    }

    fn record_local(&mut self, local: &[u16]) {
        let target = self.frame + self.cfg.input_delay as i32;
        for (i, &s) in self.local_slots.clone().iter().enumerate() {
            let v = local.get(i).copied().unwrap_or(0);
            while (self.known[s].len() as i32) <= target {
                let at_target = self.known[s].len() as i32 == target;
                self.known[s].push(if at_target { v } else { 0 });
            }
        }
    }

    /// One frame of wall time: roll back if needed, maybe advance one frame with `local` (held buttons of the
    /// local slots, in `local_slots` order), and append the packets to send to `out` as (peer id, bytes).
    pub fn tick(&mut self, now: f64, clock: &dyn Fn() -> f64, local: &[u16], out: &mut Vec<(String, Vec<u8>)>) -> Tick {
        let mut t = Tick::default();
        for pi in 0..self.peers.len() {
            let p = &self.peers[pi];
            if p.connected && now - p.last_recv > self.cfg.disconnect_ms && now > p.grace_until {
                if p.spectator {
                    self.peers[pi].connected = false;
                } else {
                    self.peers[pi].timed_out = true;
                    self.drop_peer(pi);
                }
            }
        }
        if let Some(rb) = self.rollback_from.take() {
            t.resimulated = self.rollback(rb, clock);
        }
        self.hash_confirmed();
        // behind the others (a rejoin, a long stall): simulate frames whose inputs are all known, quietly
        if self.frames_behind() > 6.0 {
            self.game.set_quiet(true);
            let mut n = 0;
            while n < self.cfg.catchup && self.frame + 1 < self.confirmed() && self.frames_behind() > 3.0 {
                self.record_local(local);
                let f = self.frame;
                self.simulate(f);
                self.stats.catchup_frames += 1;
                self.stats.frames += 1;
                t.advanced += 1;
                n += 1;
            }
            self.game.set_quiet(false);
            if n > 0 {
                self.hash_confirmed();
            }
        }
        let ahead = self.frame - self.confirmed();
        self.since_skip += 1;
        if ahead >= self.cfg.max_prediction as i32 {
            t.stalled = true;
            self.stats.stalls += 1;
        } else if self.cfg.time_sync && self.since_skip >= self.cfg.skip_interval && self.wait_recommendation() >= 1.0 {
            t.skipped = true;
            self.since_skip = 0;
            self.stats.skips += 1;
        } else {
            self.record_local(local);
            let t0 = clock();
            let f = self.frame;
            self.simulate(f);
            self.stats.step_us += (clock() - t0) * 1000.0;
            self.stats.frames += 1;
            t.advanced += 1;
            self.hash_confirmed();
        }
        self.send(now, out);
        t
    }

    fn status(&self) -> Vec<SlotStatus> {
        (0..self.cfg.slots)
            .map(|s| match self.disc[s] {
                Some(d) => SlotStatus { known: d, disc: true, epoch: self.epoch[s] },
                None => SlotStatus { known: self.known[s].len() as i32, disc: false, epoch: self.epoch[s] },
            })
            .collect()
    }

    fn recent_hashes(&self) -> (Vec<(i32, u64)>, (i32, u64)) {
        let n = self.hashes.len();
        let from = n.saturating_sub(4).max(self.base as usize);
        let h = (from..n).map(|k| (k as i32, self.hashes[k])).collect();
        let c = if n > 0 { ((n - 1) as i32, self.chain[n - 1]) } else { (0, 0) };
        (h, c)
    }

    fn send(&mut self, now: f64, out: &mut Vec<(String, Vec<u8>)>) {
        let status = self.status();
        let (hashes, chain) = self.recent_hashes();
        let local_len = self.local_slots.iter().map(|&s| self.known[s].len()).min().unwrap_or(0) as i32;
        let confirmed = self.confirmed().min(self.frame);
        let serve_spectators = self.acting_host();
        for pi in 0..self.peers.len() {
            let p = &self.peers[pi];
            if !p.connected {
                continue;
            }
            let msg = if p.spectator {
                if !serve_spectators {
                    continue;
                }
                let start = p.acked.clamp(self.base, confirmed.max(self.base));
                let end = confirmed.min(start + MAX_FRAMES_PER_PACKET as i32);
                let mut inputs = Vec::with_capacity((end - start).max(0) as usize * self.cfg.slots);
                for f in start..end {
                    for s in 0..self.cfg.slots {
                        inputs.push(self.input_for(s, f));
                    }
                }
                Msg::Spect(SpectMsg { session: self.cfg.session, frame: self.frame, start, slots: self.cfg.slots as u8, inputs, hashes: hashes.clone(), chain })
            } else {
                let start = p.acked.clamp(0, local_len);
                let end = local_len.min(start + MAX_FRAMES_PER_PACKET as i32);
                let mut inputs = Vec::with_capacity((end - start) as usize * self.local_slots.len());
                for f in start..end {
                    for &s in &self.local_slots {
                        inputs.push(self.known[s][f as usize]);
                    }
                }
                let ack = p.slots.iter().map(|&s| self.known[s].len() as i32).min().unwrap_or(0);
                let fms = 1000.0 / self.cfg.fps;
                let est = p.remote_frame as f64 + p.rtt_ms / 2.0 / fms;
                let adv = ((self.frame as f64 - est) * 16.0).clamp(-32000.0, 32000.0) as i16;
                // seat changes the peer has not applied (its status shows an older epoch)
                let seats: Vec<SeatChange> = (0..self.cfg.slots).filter_map(|s| self.seat[s].as_ref().filter(|c| newer(c.epoch, p.status[s].epoch) && c.peer != p.id).cloned()).collect();
                Msg::Input(InputMsg {
                    session: self.cfg.session,
                    frame: self.frame,
                    adv,
                    ping: ms32(now).max(1),
                    pong: p.last_ping,
                    hold: (now - p.last_ping_at).clamp(0.0, 65535.0) as u16,
                    ack,
                    status: status.clone(),
                    seats,
                    hashes: hashes.clone(),
                    chain,
                    start,
                    slots: self.local_slots.len() as u8,
                    inputs,
                })
            };
            let b = crate::proto::encode(&msg);
            self.stats.packets_sent += 1;
            self.stats.bytes_sent += b.len() as u64;
            out.push((p.id.clone(), b));
        }
    }

    /// count a received packet (the driver decodes and calls `handle`)
    pub fn count_recv(&mut self, bytes: usize) {
        self.stats.packets_recv += 1;
        self.stats.bytes_recv += bytes as u64;
    }

    /// Tell everyone we leave (best effort).
    pub fn bye(&self, out: &mut Vec<(String, Vec<u8>)>) {
        for p in self.peers.iter().filter(|p| p.connected) {
            out.push((p.id.clone(), crate::proto::encode(&Msg::Bye { session: self.cfg.session })));
        }
    }
}
