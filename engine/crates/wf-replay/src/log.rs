//! Per-frame, per-player button log (the only input of a deterministic sim).

use crate::{err, put_varint, Reader, Result};

/// `inputs[frame][player]`, frames counted from `start` (the sim frame the first entry is applied at: the step
/// from frame `start + i` to `start + i + 1` uses entry `i`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputLog {
    pub players: usize,
    pub start: u64,
    data: Vec<u16>,
}

impl InputLog {
    pub fn new(players: usize, start: u64) -> InputLog {
        InputLog { players: players.max(1), start, data: Vec::new() }
    }
    /// Number of recorded frames.
    pub fn len(&self) -> u64 {
        (self.data.len() / self.players) as u64
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// Sim frame after the last recorded step.
    pub fn end(&self) -> u64 {
        self.start + self.len()
    }
    /// Append one frame (missing players = 0, extra ignored).
    pub fn push(&mut self, inputs: &[u16]) {
        for p in 0..self.players {
            self.data.push(inputs.get(p).copied().unwrap_or(0));
        }
    }
    /// The inputs of the step that starts at sim frame `frame`.
    pub fn get(&self, frame: u64) -> Option<&[u16]> {
        let i = frame.checked_sub(self.start)? as usize;
        self.data.get(i * self.players..(i + 1) * self.players)
    }
    /// Keep only the steps before sim frame `frame` (rewind / branch).
    pub fn truncate(&mut self, frame: u64) {
        let k = frame.saturating_sub(self.start) as usize * self.players;
        self.data.truncate(k);
    }
    pub fn frames(&self) -> impl Iterator<Item = &[u16]> {
        self.data.chunks_exact(self.players)
    }

    /// Run-length encoding per player: `varint(players) varint(start) varint(frames)` then per player a list of
    /// runs `varint(len) varint(buttons)` covering all frames.
    pub fn encode(&self, out: &mut Vec<u8>) {
        put_varint(out, self.players as u64);
        put_varint(out, self.start);
        let n = self.len() as usize;
        put_varint(out, n as u64);
        for p in 0..self.players {
            let mut f = 0;
            while f < n {
                let v = self.data[f * self.players + p];
                let mut k = f + 1;
                while k < n && self.data[k * self.players + p] == v {
                    k += 1;
                }
                put_varint(out, (k - f) as u64);
                put_varint(out, v as u64);
                f = k;
            }
        }
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<InputLog> {
        let players = r.varint()? as usize;
        if players == 0 || players > 64 {
            return err("input log: bad player count");
        }
        let start = r.varint()?;
        let n = r.varint()? as usize;
        if n > 1 << 28 {
            return err("input log: too long");
        }
        let mut data = vec![0u16; n * players];
        for p in 0..players {
            let mut f = 0;
            while f < n {
                let len = r.varint()? as usize;
                let v = r.varint()?;
                if len == 0 || f + len > n || v > u16::MAX as u64 {
                    return err("input log: bad run");
                }
                for k in f..f + len {
                    data[k * players + p] = v as u16;
                }
                f += len;
            }
        }
        Ok(InputLog { players, start, data })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        self.encode(&mut v);
        v
    }
    pub fn from_bytes(b: &[u8]) -> Result<InputLog> {
        InputLog::decode(&mut Reader::new(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rle_round_trip() {
        let mut l = InputLog::new(2, 5);
        for f in 0..1000u16 {
            l.push(&[if f % 100 < 30 { 0x10 } else { 0 }, f / 7]);
        }
        let b = l.to_bytes();
        assert_eq!(InputLog::from_bytes(&b).unwrap(), l);
        assert_eq!(l.get(5).unwrap(), &[0x10, 0]);
        assert_eq!(l.get(4), None);
        assert_eq!(l.end(), 1005);
        let mut t = l.clone();
        t.truncate(105);
        assert_eq!(t.len(), 100);
    }
}
