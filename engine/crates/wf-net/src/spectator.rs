//! Spectator: receives the confirmed inputs of every slot from the host and simulates them (never predicts, never
//! rolls back). It runs behind the players by about one round trip; when further behind (late join, a stall) it
//! catches up by simulating up to `catchup` frames per tick. It checks the host's state hashes like a player.

use crate::game::{chain_mix, Game};
use crate::proto::Msg;
use crate::session::NetEvent;

pub struct SpectatorSession<G: Game> {
    pub game: G,
    pub session: u32,
    pub host: String,
    pub slots: usize,
    /// frames simulated
    frame: i32,
    /// inputs per frame (all slots), contiguous from 0
    inputs: Vec<Vec<u16>>,
    pub hashes: Vec<u64>,
    pub chain: Vec<u64>,
    pending: Vec<(i32, u64)>,
    pending_chain: Option<(i32, u64)>,
    pub events: Vec<NetEvent>,
    pub catchup: u32,
    pub host_frame: i32,
    pub last_recv: f64,
    pub frames_behind: i32,
    desynced: bool,
    pub packets_recv: u64,
    pub hash_checks: u64,
}

impl<G: Game> SpectatorSession<G> {
    pub fn new(game: G, session: u32, host: String, slots: usize, now: f64) -> Self {
        let mut s = SpectatorSession {
            game,
            session,
            host,
            slots,
            frame: 0,
            inputs: Vec::new(),
            hashes: Vec::new(),
            chain: Vec::new(),
            pending: Vec::new(),
            pending_chain: None,
            events: Vec::new(),
            catchup: 8,
            host_frame: 0,
            last_recv: now,
            frames_behind: 0,
            desynced: false,
            packets_recv: 0,
            hash_checks: 0,
        };
        s.hash_current();
        s
    }

    pub fn frame(&self) -> i32 {
        self.frame
    }

    fn hash_current(&mut self) {
        let st = self.game.save();
        let h = self.game.checksum(&st);
        let c = chain_mix(self.chain.last().copied().unwrap_or(0), h);
        self.hashes.push(h);
        self.chain.push(c);
    }

    pub fn handle(&mut self, from: &str, msg: &Msg, now: f64) {
        if from != self.host {
            return;
        }
        let Msg::Spect(m) = msg else { return };
        if m.session != self.session || m.slots as usize != self.slots || self.slots == 0 {
            return;
        }
        self.packets_recv += 1;
        self.last_recv = now;
        self.host_frame = m.frame;
        let n = m.inputs.len() / self.slots;
        for i in 0..n {
            let f = m.start as usize + i;
            if f == self.inputs.len() {
                self.inputs.push(m.inputs[i * self.slots..(i + 1) * self.slots].to_vec());
            }
        }
        for &h in &m.hashes {
            if !self.pending.contains(&h) {
                self.pending.push(h);
            }
        }
        if m.chain.0 > 0 {
            self.pending_chain = Some(m.chain);
        }
    }

    /// Simulate what is available (1 frame, or up to `catchup` when behind); returns frames simulated. Appends
    /// the ack for the host to `out`.
    pub fn tick(&mut self, out: &mut Vec<(String, Vec<u8>)>) -> u32 {
        let avail = self.inputs.len() as i32 - self.frame;
        self.frames_behind = avail;
        let n = if avail > 6 { avail.min(self.catchup as i32) } else { avail.min(1) };
        for _ in 0..n {
            let inp = self.inputs[self.frame as usize].clone();
            self.game.advance(&inp, true);
            self.frame += 1;
            self.hash_current();
        }
        let len = self.hashes.len() as i32;
        let mut rest = Vec::new();
        for &(k, h) in &self.pending {
            if k < len {
                self.hash_checks += 1;
                if self.hashes[k as usize] != h && !self.desynced {
                    self.desynced = true;
                    self.events.push(NetEvent::Desync { frame: k, local: self.hashes[k as usize], remote: h, peer: self.host.clone(), chain: false });
                }
            } else {
                rest.push((k, h));
            }
        }
        self.pending = rest;
        if let Some((k, c)) = self.pending_chain {
            if k < len {
                self.pending_chain = None;
                self.hash_checks += 1;
                if self.chain[k as usize] != c && !self.desynced {
                    self.desynced = true;
                    self.events.push(NetEvent::Desync { frame: k, local: self.chain[k as usize], remote: c, peer: self.host.clone(), chain: true });
                }
            }
        }
        out.push((self.host.clone(), crate::proto::encode(&Msg::SpectAck { session: self.session, ack: self.inputs.len() as i32 })));
        n.max(0) as u32
    }

    pub fn inputs(&self) -> &[Vec<u16>] {
        &self.inputs
    }
}
