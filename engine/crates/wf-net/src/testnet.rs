//! Test harness: several peers' sessions on one [`SimNet`] with virtual time (deterministic, faster than real
//! time). Used by the crate's tests and the benchmarks; also handy to reproduce a network scenario.

use crate::cli::RandomInput;
use crate::game::Game;
use crate::proto::decode;
use crate::session::{NetConfig, P2PSession};
use crate::spectator::SpectatorSession;
use crate::transport::{PeerChange, SimEndpoint, SimNet, Transport};

pub struct PeerRun<G: Game> {
    pub id: String,
    pub sess: P2PSession<G>,
    pub ep: SimEndpoint,
    pub input: RandomInput,
    /// virtual ticks before this peer starts (start skew)
    pub start_tick: u64,
    /// leave the network at this tick (disconnect test)
    pub leave_tick: Option<u64>,
    pub left: bool,
    /// tick period of this peer in ms (clock drift test); default 1000 / fps
    pub period_ms: f64,
    next_ms: f64,
}

pub struct SpecRun<G: Game> {
    pub sess: SpectatorSession<G>,
    pub ep: SimEndpoint,
    pub start_tick: u64,
    next_ms: f64,
}

/// `games[i]` = peer i's game (slot i). Peer 0 is the host (serves spectators).
pub fn peers<G: Game>(net: &SimNet, games: Vec<G>, cfg: &NetConfig, spectators: usize, seed: u64) -> Vec<PeerRun<G>> {
    let n = games.len();
    let ids: Vec<String> = (0..n).map(|i| format!("peer{i}")).collect();
    let mut out = Vec::new();
    for (i, g) in games.into_iter().enumerate() {
        let others = (0..n).filter(|&k| k != i).map(|k| (ids[k].clone(), vec![k])).collect();
        // every player knows the spectators; the acting host (lowest connected slot) serves them
        let specs = (0..spectators).map(|k| format!("spec{k}")).collect();
        let ep = net.endpoint(&ids[i]);
        let sess = P2PSession::new(g, NetConfig { slots: n, ..cfg.clone() }, vec![i], others, specs, i == 0, 0.0);
        out.push(PeerRun { id: ids[i].clone(), sess, ep, input: RandomInput::new(seed * 131 + i as u64), start_tick: 0, leave_tick: None, left: false, period_ms: 1000.0 / cfg.fps, next_ms: 0.0 });
    }
    out
}

pub fn spectator<G: Game>(net: &SimNet, game: G, cfg: &NetConfig, slots: usize, k: usize) -> SpecRun<G> {
    let ep = net.endpoint(&format!("spec{k}"));
    let mut sess = SpectatorSession::new(game, cfg.session, "peer0".into(), slots, 0.0);
    sess.players = (0..slots).map(|k| format!("peer{k}")).collect();
    SpecRun { sess, ep, start_tick: 0, next_ms: 0.0 }
}

#[derive(Debug, Default)]
pub struct RunResult {
    pub ms: f64,
    /// every peer reached a hashed state >= frames
    pub done: bool,
}

/// Run until every remaining peer (and spectator) has state `frames` hashed, or `max_ms` of virtual time.
/// Time advances in 1 ms steps; each peer ticks on its own period.
pub fn run<G: Game>(net: &SimNet, ps: &mut Vec<PeerRun<G>>, specs: &mut [SpecRun<G>], frames: i32, max_ms: f64) -> RunResult {
    run_with(net, ps, specs, frames, max_ms, &mut |_, _| None)
}

/// [`run`] with a hook called every virtual ms that may return a peer to add (a rejoining player).
pub fn run_with<G: Game>(net: &SimNet, ps: &mut Vec<PeerRun<G>>, specs: &mut [SpecRun<G>], frames: i32, max_ms: f64, hook: &mut dyn FnMut(f64, &mut [PeerRun<G>]) -> Option<PeerRun<G>>) -> RunResult {
    let t0 = std::time::Instant::now();
    let clock = move || t0.elapsed().as_secs_f64() * 1000.0;
    let mut now = 0.0;
    let base = 1000.0 / 59.7275;
    let mut out = Vec::new();
    while now < max_ms {
        if let Some(p) = hook(now, ps) {
            ps.push(p);
        }
        for p in ps.iter_mut() {
            if p.left || now < p.start_tick as f64 * base || now < p.next_ms {
                continue;
            }
            p.next_ms = if p.next_ms < now - 100.0 { now } else { p.next_ms } + p.period_ms;
            if p.leave_tick.is_some_and(|t| now >= t as f64 * base) {
                p.left = true;
                net.leave(&p.id);
                continue;
            }
            for c in p.ep.changes() {
                if let PeerChange::Disconnected(id) = c {
                    p.sess.peer_left(&id);
                }
            }
            for (from, b) in p.ep.recv(now) {
                if let Some(m) = decode(&b) {
                    p.sess.count_recv(b.len());
                    p.sess.handle(&from, &m, now);
                }
            }
            let v = p.input.next();
            p.sess.tick(now, &clock, &[v], &mut out);
            for (to, b) in out.drain(..) {
                p.ep.send(&to, b, now);
            }
        }
        for s in specs.iter_mut() {
            if now < s.start_tick as f64 * base || now < s.next_ms {
                continue;
            }
            s.next_ms = if s.next_ms < now - 100.0 { now } else { s.next_ms } + base;
            for (from, b) in s.ep.recv(now) {
                if let Some(m) = decode(&b) {
                    s.sess.handle(&from, &m, now);
                }
            }
            s.sess.tick(&mut out);
            for (to, b) in out.drain(..) {
                s.ep.send(&to, b, now);
            }
        }
        let peers_done = ps.iter().filter(|p| !p.left).all(|p| p.sess.hashed() >= frames);
        let specs_done = specs.iter().all(|s| s.sess.hashes.len() as i32 > frames);
        if peers_done && specs_done {
            return RunResult { ms: now, done: true };
        }
        now += 1.0;
    }
    RunResult { ms: now, done: false }
}

/// A toy deterministic game (tests of the session logic without content).
#[derive(Clone, Default)]
pub struct ToyGame {
    pub acc: u64,
    pub frame: u32,
    /// from this frame on, peers with `bad = true` compute differently (desync test)
    pub bad_from: Option<u32>,
    pub first_frames: u32,
}

impl Game for ToyGame {
    type State = (u64, u32);
    fn save(&self) -> (u64, u32) {
        (self.acc, self.frame)
    }
    fn load(&mut self, s: &(u64, u32)) {
        self.acc = s.0;
        self.frame = s.1;
    }
    fn advance(&mut self, inputs: &[u16], first: bool) {
        for (k, &v) in inputs.iter().enumerate() {
            self.acc = crate::game::chain_mix(self.acc, ((k as u64) << 16) | v as u64);
        }
        if self.bad_from.is_some_and(|f| self.frame >= f) {
            self.acc ^= 1;
        }
        self.frame += 1;
        if first {
            self.first_frames += 1;
        }
    }
    fn checksum(&self, s: &(u64, u32)) -> u64 {
        s.0 ^ ((s.1 as u64) << 40)
    }
    fn save_bytes(&self) -> Option<Vec<u8>> {
        let mut v = self.acc.to_le_bytes().to_vec();
        v.extend_from_slice(&self.frame.to_le_bytes());
        Some(v)
    }
    fn load_bytes(&mut self, b: &[u8]) -> bool {
        if b.len() != 12 {
            return false;
        }
        self.acc = u64::from_le_bytes(b[..8].try_into().unwrap());
        self.frame = u32::from_le_bytes(b[8..].try_into().unwrap());
        true
    }
}

/// A peer that rejoins slot `slot` as `id`: the acting host (`ps[host]`) grants it (once every player agrees the
/// slot is gone) and the new session starts from the snapshot, exactly like the lobby does it (`netsim.rs`).
/// None while the host is not ready yet.
pub fn try_rejoin<G: Game>(net: &SimNet, ps: &mut [PeerRun<G>], host: usize, slot: usize, id: &str, mut game: G, cfg: &NetConfig, seed: u64, now: f64) -> Option<PeerRun<G>> {
    ps[host].sess.rejoin_ready(slot).ok()?;
    let (g, bytes) = ps[host].sess.grant_rejoin(slot, id, now).expect("grant");
    let mut g = g;
    for e in g.peers.iter_mut() {
        if e.0.is_empty() {
            e.0 = ps[host].id.clone();
        }
    }
    assert!(game.load_bytes(&bytes), "snapshot loads");
    let ep = net.endpoint(id);
    let sess = P2PSession::resume(game, NetConfig { slots: cfg.slots.max(g.epochs.len()), ..cfg.clone() }, &g, id, now).expect("snapshot verifies");
    let period = ps[host].period_ms;
    Some(PeerRun { id: id.to_string(), sess, ep, input: RandomInput::new(seed), start_tick: 0, leave_tick: None, left: false, period_ms: period, next_ms: now })
}
