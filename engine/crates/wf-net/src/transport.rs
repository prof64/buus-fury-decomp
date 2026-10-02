//! Datagram transports. The sessions only see `(peer id, bytes)`; a transport delivers them unreliably (WebRTC
//! data channel with `ordered: false, maxRetransmits: 0`, or a test network).
//!
//! * [`SimNet`]: an in-process network with virtual time (tests: many peers, latency, jitter, loss, all
//!   deterministic from a seed).
//! * [`Conditioner`]: wraps any transport and adds latency / jitter / loss on the send side (artificial bad
//!   network for real runs; `--net-latency 100 --net-loss 0.05`).
//! * [`QueueTransport`]: inbox / outbox queues a host fills and drains (the browser's JS WebRTC code via wf-web).
//! * `matchbox::MatchboxTransport` (feature `webrtc`, native): WebRTC through a Matchbox signaling server.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerChange {
    Connected(String),
    Disconnected(String),
}

pub trait Transport {
    /// our id on this transport (None until the signaling server assigned one)
    fn local_id(&self) -> Option<String>;
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64);
    /// everything received since the last call
    fn recv(&mut self, now: f64) -> Vec<(String, Vec<u8>)>;
    /// peers that connected / left since the last call
    fn changes(&mut self) -> Vec<PeerChange>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn local_id(&self) -> Option<String> {
        (**self).local_id()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64) {
        (**self).send(to, data, now)
    }
    fn recv(&mut self, now: f64) -> Vec<(String, Vec<u8>)> {
        (**self).recv(now)
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        (**self).changes()
    }
}

/// Small deterministic PRNG (xorshift64*) for loss / jitter.
#[derive(Clone, Debug)]
pub struct Rng64(u64);
impl Rng64 {
    pub fn new(seed: u64) -> Self {
        Rng64(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// uniform in [0, 1)
    pub fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Network conditions applied on send.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Conditions {
    /// one-way delay added to every packet, ms
    pub latency_ms: f64,
    /// + uniform random 0..jitter ms (packets may arrive out of order)
    pub jitter_ms: f64,
    /// probability a packet is dropped
    pub loss: f64,
}

pub struct Conditioner<T: Transport> {
    pub inner: T,
    pub cond: Conditions,
    rng: Rng64,
    queue: Vec<(f64, u64, String, Vec<u8>)>,
    seq: u64,
    pub dropped: u64,
    pub delayed: u64,
}

impl<T: Transport> Conditioner<T> {
    pub fn new(inner: T, cond: Conditions, seed: u64) -> Self {
        Conditioner { inner, cond, rng: Rng64::new(seed), queue: Vec::new(), seq: 0, dropped: 0, delayed: 0 }
    }
    fn flush(&mut self, now: f64) {
        if self.queue.is_empty() {
            return;
        }
        self.queue.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let n = self.queue.iter().take_while(|q| q.0 <= now).count();
        for (_, _, to, d) in self.queue.drain(..n).collect::<Vec<_>>() {
            self.inner.send(&to, d, now);
        }
    }
}

impl<T: Transport> Transport for Conditioner<T> {
    fn local_id(&self) -> Option<String> {
        self.inner.local_id()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64) {
        self.flush(now);
        if self.cond.loss > 0.0 && self.rng.f64() < self.cond.loss {
            self.dropped += 1;
            return;
        }
        let d = self.cond.latency_ms + if self.cond.jitter_ms > 0.0 { self.rng.f64() * self.cond.jitter_ms } else { 0.0 };
        if d <= 0.0 {
            self.inner.send(to, data, now);
            return;
        }
        self.delayed += 1;
        self.seq += 1;
        self.queue.push((now + d, self.seq, to.to_string(), data));
    }
    fn recv(&mut self, now: f64) -> Vec<(String, Vec<u8>)> {
        self.flush(now);
        self.inner.recv(now)
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        self.inner.changes()
    }
}

// ------------------------------------------------------------------------------------------- in-process network

struct NetInner {
    /// (deliver at, seq, from, to, bytes)
    flight: Vec<(f64, u64, String, String, Vec<u8>)>,
    seq: u64,
    members: Vec<String>,
    /// per receiver: pending peer changes
    changes: BTreeMap<String, Vec<PeerChange>>,
    rng: Rng64,
    cond: Conditions,
    /// pairs that cannot talk (partition tests)
    cut: Vec<(String, String)>,
    pub delivered: u64,
    pub dropped: u64,
}

/// A virtual network: every endpoint shares it; time is whatever the test passes as `now`.
#[derive(Clone)]
pub struct SimNet(Rc<RefCell<NetInner>>);

impl SimNet {
    pub fn new(cond: Conditions, seed: u64) -> Self {
        SimNet(Rc::new(RefCell::new(NetInner { flight: Vec::new(), seq: 0, members: Vec::new(), changes: BTreeMap::new(), rng: Rng64::new(seed), cond, cut: Vec::new(), delivered: 0, dropped: 0 })))
    }
    pub fn endpoint(&self, id: &str) -> SimEndpoint {
        let mut n = self.0.borrow_mut();
        for m in n.members.clone() {
            n.changes.entry(m.clone()).or_default().push(PeerChange::Connected(id.to_string()));
            n.changes.entry(id.to_string()).or_default().push(PeerChange::Connected(m));
        }
        n.members.push(id.to_string());
        SimEndpoint { net: self.clone(), id: id.to_string() }
    }
    /// remove an endpoint (everyone else sees it leave)
    pub fn leave(&self, id: &str) {
        let mut n = self.0.borrow_mut();
        n.members.retain(|m| m != id);
        for m in n.members.clone() {
            n.changes.entry(m).or_default().push(PeerChange::Disconnected(id.to_string()));
        }
    }
    pub fn cut(&self, a: &str, b: &str) {
        self.0.borrow_mut().cut.push((a.to_string(), b.to_string()));
    }
    pub fn set_conditions(&self, c: Conditions) {
        self.0.borrow_mut().cond = c;
    }
    pub fn stats(&self) -> (u64, u64) {
        let n = self.0.borrow();
        (n.delivered, n.dropped)
    }
}

pub struct SimEndpoint {
    net: SimNet,
    id: String,
}

impl Transport for SimEndpoint {
    fn local_id(&self) -> Option<String> {
        Some(self.id.clone())
    }
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64) {
        let mut n = self.net.0.borrow_mut();
        if !n.members.iter().any(|m| m == to) || n.cut.iter().any(|(a, b)| (a == &self.id && b == to) || (b == &self.id && a == to)) {
            return;
        }
        let c = n.cond;
        if c.loss > 0.0 && n.rng.f64() < c.loss {
            n.dropped += 1;
            return;
        }
        let d = c.latency_ms + if c.jitter_ms > 0.0 { n.rng.f64() * c.jitter_ms } else { 0.0 };
        n.seq += 1;
        let seq = n.seq;
        n.flight.push((now + d, seq, self.id.clone(), to.to_string(), data));
    }
    fn recv(&mut self, now: f64) -> Vec<(String, Vec<u8>)> {
        let mut n = self.net.0.borrow_mut();
        let mut out = Vec::new();
        let mut rest = Vec::new();
        let mut due: Vec<_> = Vec::new();
        for f in n.flight.drain(..) {
            if f.3 == self.id && f.0 <= now {
                due.push(f);
            } else {
                rest.push(f);
            }
        }
        n.flight = rest;
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        n.delivered += due.len() as u64;
        for (_, _, from, _, d) in due {
            out.push((from, d));
        }
        out
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        self.net.0.borrow_mut().changes.remove(&self.id).unwrap_or_default()
    }
}

// --------------------------------------------------------------------------------------------- host-fed queues

#[derive(Default)]
pub struct Queues {
    pub id: Option<String>,
    pub inbox: VecDeque<(String, Vec<u8>)>,
    pub outbox: VecDeque<(String, Vec<u8>)>,
    pub changes: Vec<PeerChange>,
}

/// Transport whose packets a host moves (the browser: RTCDataChannel messages in, `send` calls out).
#[derive(Clone, Default)]
pub struct QueueTransport(pub Rc<RefCell<Queues>>);

impl Transport for QueueTransport {
    fn local_id(&self) -> Option<String> {
        self.0.borrow().id.clone()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, _now: f64) {
        self.0.borrow_mut().outbox.push_back((to.to_string(), data));
    }
    fn recv(&mut self, _now: f64) -> Vec<(String, Vec<u8>)> {
        self.0.borrow_mut().inbox.drain(..).collect()
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        std::mem::take(&mut self.0.borrow_mut().changes)
    }
}
