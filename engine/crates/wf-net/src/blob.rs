//! Moving one larger byte string (a state snapshot, tens of KB) over the unreliable, unordered channel: chunks of
//! [`BLOB_CHUNK`] bytes, the receiver acknowledges with a bitmap, the sender re-sends what is missing (at most
//! `per_tick` chunks per tick, a chunk again only after `resend_ms`). Used to catch a rejoining peer up.

use crate::proto::{encode, Msg, BLOB_CHUNK};

pub struct BlobSender {
    pub id: u32,
    pub to: String,
    chunks: Vec<Vec<u8>>,
    total: u32,
    acked: Vec<bool>,
    sent_at: Vec<f64>,
    pub per_tick: usize,
    pub resend_ms: f64,
    pub sent: u64,
}

impl BlobSender {
    pub fn new(id: u32, to: String, data: &[u8]) -> Self {
        let chunks: Vec<Vec<u8>> = if data.is_empty() { vec![Vec::new()] } else { data.chunks(BLOB_CHUNK).map(|c| c.to_vec()).collect() };
        let n = chunks.len();
        BlobSender { id, to, chunks, total: data.len() as u32, acked: vec![false; n], sent_at: vec![f64::NEG_INFINITY; n], per_tick: 24, resend_ms: 250.0, sent: 0 }
    }
    pub fn chunks(&self) -> usize {
        self.chunks.len()
    }
    pub fn done(&self) -> bool {
        self.acked.iter().all(|a| *a)
    }
    pub fn on_ack(&mut self, have: &[u8]) {
        for (k, a) in self.acked.iter_mut().enumerate() {
            if have.get(k / 8).is_some_and(|b| b & (1 << (k % 8)) != 0) {
                *a = true;
            }
        }
    }
    /// Datagrams to send now.
    pub fn poll(&mut self, now: f64, out: &mut Vec<(String, Vec<u8>)>) {
        let n = self.chunks.len();
        let mut budget = self.per_tick;
        for k in 0..n {
            if budget == 0 {
                break;
            }
            if self.acked[k] || now - self.sent_at[k] < self.resend_ms {
                continue;
            }
            self.sent_at[k] = now;
            budget -= 1;
            self.sent += 1;
            out.push((self.to.clone(), encode(&Msg::Blob { id: self.id, idx: k as u16, n: n as u16, total: self.total, data: self.chunks[k].clone() })));
        }
    }
}

#[derive(Default)]
pub struct BlobReceiver {
    pub id: u32,
    chunks: Vec<Option<Vec<u8>>>,
    total: u32,
    last_ack: f64,
    dirty: bool,
}

impl BlobReceiver {
    pub fn new(id: u32) -> Self {
        BlobReceiver { id, chunks: Vec::new(), total: 0, last_ack: f64::NEG_INFINITY, dirty: false }
    }
    pub fn on_chunk(&mut self, id: u32, idx: u16, n: u16, total: u32, data: &[u8]) {
        if id != self.id || n == 0 {
            return;
        }
        if self.chunks.len() != n as usize {
            self.chunks = vec![None; n as usize];
            self.total = total;
        }
        if let Some(c) = self.chunks.get_mut(idx as usize) {
            if c.is_none() {
                *c = Some(data.to_vec());
            }
            self.dirty = true;
        }
    }
    pub fn progress(&self) -> (usize, usize) {
        (self.chunks.iter().filter(|c| c.is_some()).count(), self.chunks.len())
    }
    pub fn complete(&self) -> Option<Vec<u8>> {
        if self.chunks.is_empty() || self.chunks.iter().any(|c| c.is_none()) {
            return None;
        }
        let v: Vec<u8> = self.chunks.iter().flat_map(|c| c.as_ref().unwrap().iter().copied()).collect();
        (v.len() as u32 == self.total).then_some(v)
    }
    /// The ack to send (after new chunks, at most every 30 ms, and every 200 ms anyway while incomplete).
    pub fn ack(&mut self, to: &str, now: f64, out: &mut Vec<(String, Vec<u8>)>) {
        if self.chunks.is_empty() {
            return;
        }
        let due = (self.dirty && now - self.last_ack >= 30.0) || now - self.last_ack >= 200.0;
        if !due {
            return;
        }
        self.dirty = false;
        self.last_ack = now;
        let mut have = vec![0u8; self.chunks.len().div_ceil(8)];
        for (k, c) in self.chunks.iter().enumerate() {
            if c.is_some() {
                have[k / 8] |= 1 << (k % 8);
            }
        }
        out.push((to.to_string(), encode(&Msg::BlobAck { id: self.id, have })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::decode;
    #[test]
    fn lossy_transfer() {
        let data: Vec<u8> = (0..50_000u32).map(|i| (i * 7 + i / 300) as u8).collect();
        let mut s = BlobSender::new(5, "b".into(), &data);
        let mut r = BlobReceiver::new(5);
        let mut rng = crate::transport::Rng64::new(3);
        let mut now = 0.0;
        let mut out = Vec::new();
        for _ in 0..2000 {
            s.poll(now, &mut out);
            for (_, b) in out.drain(..) {
                if rng.f64() < 0.3 {
                    continue;
                }
                if let Some(Msg::Blob { id, idx, n, total, data }) = decode(&b) {
                    r.on_chunk(id, idx, n, total, &data);
                }
            }
            r.ack("a", now, &mut out);
            for (_, b) in out.drain(..) {
                if rng.f64() < 0.3 {
                    continue;
                }
                if let Some(Msg::BlobAck { have, .. }) = decode(&b) {
                    s.on_ack(&have);
                }
            }
            if s.done() {
                break;
            }
            now += 16.7;
        }
        assert!(s.done());
        assert_eq!(r.complete().unwrap(), data);
    }
}
