//! Wire format (little-endian, hand-rolled: identical on native and wasm, no serde in the hot path).
//!
//! Every datagram: `magic u16 = 0x5746 ("WF") | version u8 | kind u8 | body`. All game traffic goes over one
//! unreliable, unordered channel; reliability comes from redundancy (every input packet carries all inputs the
//! receiver has not acknowledged) and from idempotent lobby state messages that are re-sent until answered.

pub const MAGIC: u16 = 0x5746;
pub const VERSION: u8 = 1;
/// at most this many frames of inputs per packet (the rest goes in the next one)
pub const MAX_FRAMES_PER_PACKET: usize = 192;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Input(InputMsg),
    /// host -> spectator: confirmed inputs of every slot
    Spect(SpectMsg),
    SpectAck { session: u32, ack: i32 },
    /// lobby state (JSON, see `lobby.rs`)
    Lobby(String),
    Bye { session: u32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputMsg {
    pub session: u32,
    /// the sender's current frame (time sync)
    pub frame: i32,
    /// the sender's frame advantage over the receiver, 1/16 frames
    pub adv: i16,
    /// sender clock (ms, wrapping) and the echo of the receiver's last ping + how long the sender held it
    pub ping: u32,
    pub pong: u32,
    pub hold: u16,
    /// how many of the receiver's inputs the sender has (contiguous from frame 0)
    pub ack: i32,
    /// per slot: inputs the sender knows (contiguous) and whether it considers the slot disconnected (then the
    /// value is the disconnect frame)
    pub status: Vec<(i32, bool)>,
    /// recent confirmed state hashes (state index, hash) and the hash chain at the newest one
    pub hashes: Vec<(i32, u64)>,
    pub chain: (i32, u64),
    /// inputs of the sender's slots: frames `start..start + n`, `slots` values per frame
    pub start: i32,
    pub slots: u8,
    pub inputs: Vec<u16>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpectMsg {
    pub session: u32,
    pub frame: i32,
    pub start: i32,
    pub slots: u8,
    pub inputs: Vec<u16>,
    pub hashes: Vec<(i32, u64)>,
    pub chain: (i32, u64),
}

struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v)
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes())
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes())
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes())
    }
    fn hashes(&mut self, h: &[(i32, u64)], chain: (i32, u64)) {
        self.u8(h.len() as u8);
        for &(k, v) in h {
            self.i32(k);
            self.u64(v);
        }
        self.i32(chain.0);
        self.u64(chain.1);
    }
    fn inputs(&mut self, start: i32, slots: u8, v: &[u16]) {
        self.i32(start);
        self.u8(slots);
        let n = if slots == 0 { 0 } else { v.len() / slots as usize };
        self.u16(n as u16);
        for &x in &v[..n * slots as usize] {
            self.u16(x);
        }
    }
}

struct R<'a>(&'a [u8], usize);
impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.0.get(self.1..self.1 + n)?;
        self.1 += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    fn hashes(&mut self) -> Option<(Vec<(i32, u64)>, (i32, u64))> {
        let n = self.u8()? as usize;
        let mut h = Vec::with_capacity(n);
        for _ in 0..n {
            h.push((self.i32()?, self.u64()?));
        }
        Some((h, (self.i32()?, self.u64()?)))
    }
    fn inputs(&mut self) -> Option<(i32, u8, Vec<u16>)> {
        let start = self.i32()?;
        let slots = self.u8()?;
        let n = self.u16()? as usize;
        let mut v = Vec::with_capacity(n * slots as usize);
        for _ in 0..n * slots as usize {
            v.push(self.u16()?);
        }
        Some((start, slots, v))
    }
}

pub fn encode(m: &Msg) -> Vec<u8> {
    let mut w = W(Vec::with_capacity(64));
    w.u16(MAGIC);
    w.u8(VERSION);
    match m {
        Msg::Input(i) => {
            w.u8(1);
            w.u32(i.session);
            w.i32(i.frame);
            w.u16(i.adv as u16);
            w.u32(i.ping);
            w.u32(i.pong);
            w.u16(i.hold);
            w.i32(i.ack);
            w.u8(i.status.len() as u8);
            for &(k, d) in &i.status {
                w.i32(k);
                w.u8(d as u8);
            }
            w.hashes(&i.hashes, i.chain);
            w.inputs(i.start, i.slots, &i.inputs);
        }
        Msg::Spect(s) => {
            w.u8(2);
            w.u32(s.session);
            w.i32(s.frame);
            w.hashes(&s.hashes, s.chain);
            w.inputs(s.start, s.slots, &s.inputs);
        }
        Msg::SpectAck { session, ack } => {
            w.u8(3);
            w.u32(*session);
            w.i32(*ack);
        }
        Msg::Lobby(s) => {
            w.u8(4);
            w.0.extend_from_slice(s.as_bytes());
        }
        Msg::Bye { session } => {
            w.u8(5);
            w.u32(*session);
        }
    }
    w.0
}

pub fn decode(b: &[u8]) -> Option<Msg> {
    let mut r = R(b, 0);
    if r.u16()? != MAGIC || r.u8()? != VERSION {
        return None;
    }
    Some(match r.u8()? {
        1 => {
            let session = r.u32()?;
            let frame = r.i32()?;
            let adv = r.u16()? as i16;
            let ping = r.u32()?;
            let pong = r.u32()?;
            let hold = r.u16()?;
            let ack = r.i32()?;
            let n = r.u8()? as usize;
            let mut status = Vec::with_capacity(n);
            for _ in 0..n {
                status.push((r.i32()?, r.u8()? != 0));
            }
            let (hashes, chain) = r.hashes()?;
            let (start, slots, inputs) = r.inputs()?;
            Msg::Input(InputMsg { session, frame, adv, ping, pong, hold, ack, status, hashes, chain, start, slots, inputs })
        }
        2 => {
            let session = r.u32()?;
            let frame = r.i32()?;
            let (hashes, chain) = r.hashes()?;
            let (start, slots, inputs) = r.inputs()?;
            Msg::Spect(SpectMsg { session, frame, start, slots, inputs, hashes, chain })
        }
        3 => Msg::SpectAck { session: r.u32()?, ack: r.i32()? },
        4 => Msg::Lobby(String::from_utf8(b[r.1..].to_vec()).ok()?),
        5 => Msg::Bye { session: r.u32()? },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let m = Msg::Input(InputMsg {
            session: 7,
            frame: 120,
            adv: -33,
            ping: 1000,
            pong: 990,
            hold: 4,
            ack: 118,
            status: vec![(121, false), (119, true)],
            hashes: vec![(100, 0xdead_beef), (101, 42)],
            chain: (101, 99),
            start: 118,
            slots: 2,
            inputs: vec![1, 2, 3, 4, 5, 6],
        });
        assert_eq!(decode(&encode(&m)), Some(m));
        for m in [Msg::Lobby("{\"t\":\"hello\"}".into()), Msg::Bye { session: 3 }, Msg::SpectAck { session: 1, ack: 5 }] {
            assert_eq!(decode(&encode(&m)), Some(m));
        }
        assert_eq!(decode(&[0, 1, 2]), None);
    }
}
