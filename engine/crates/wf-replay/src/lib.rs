//! Replays, input logs, save states and seekable timelines for a deterministic [`wf_view::Sim`].
//!
//! A simulation that is a pure function of (initial state, per-frame inputs) is fully described by its
//! configuration and its input log; this crate holds the shared formats around that fact so the app (replays,
//! rewind, save states), netcode (input exchange, desync checks) and agents (MCP recordings) use one encoding:
//!
//! * [`InputLog`]: per-frame, per-player `u16` buttons (GBA KEYINPUT bits), run-length encoded on disk.
//! * [`Replay`]: versioned file = [`Header`] (format, sim kind, [`ContentId`], config JSON, seed, players, hash
//!   interval, optional start state) + input log + state-hash checkpoints. A few KB per fight.
//! * [`Recorder`] builds one while playing; [`verify`] / [`Checker`] replay it and report the first divergent
//!   checkpoint.
//! * [`SaveState`]: a serialized snapshot + the input log that led to it (so recording continues after a load).
//! * [`Timeline`]: periodic snapshots of a running sim for seeking, rewinding and branching.
//!
//! See `engine/docs/replays.md`.

pub mod lz;
mod log;
mod replay;
mod state;
mod timeline;

pub use log::InputLog;
pub use replay::{verify, Branch, Checker, ContentId, Divergence, Header, Recorder, Replay, VerifyReport, DEFAULT_HASH_INTERVAL};
pub use state::{SaveState, StateHeader};
pub use timeline::{Timeline, TimelineConfig};

/// Version of the replay / save-state container formats (bumped on incompatible changes; readers reject newer).
pub const FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Error(m.into()))
}

/// FNV-1a 64 (content hashes, file checksums). Stable across platforms.
pub fn fnv64(data: &[u8]) -> u64 {
    fnv64_update(0xcbf2_9ce4_8422_2325, data)
}
pub fn fnv64_update(mut h: u64, data: &[u8]) -> u64 {
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------------------------- byte helpers

pub(crate) fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

pub(crate) struct Reader<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }
    pub fn varint(&mut self) -> Result<u64> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let Some(&b) = self.b.get(self.at) else { return err("truncated data") };
            self.at += 1;
            if shift >= 64 {
                return err("bad varint");
            }
            v |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
        }
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.at + n > self.b.len() {
            return err("truncated data");
        }
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    pub fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }
    pub fn blob(&mut self) -> Result<&'a [u8]> {
        let n = self.varint()? as usize;
        self.bytes(n)
    }
}

pub(crate) fn put_blob(out: &mut Vec<u8>, b: &[u8]) {
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

/// `magic | version u16 | body | fnv64(magic..body)`.
pub(crate) fn seal(magic: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 14);
    out.extend_from_slice(magic);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(body);
    let h = fnv64(&out);
    out.extend_from_slice(&h.to_le_bytes());
    out
}

pub(crate) fn unseal<'a>(magic: &[u8; 4], what: &str, b: &'a [u8]) -> Result<&'a [u8]> {
    if b.len() < 14 || &b[..4] != magic {
        return err(format!("not a {what} file"));
    }
    let v = u16::from_le_bytes([b[4], b[5]]);
    if v > FORMAT_VERSION {
        return err(format!("{what} format {v} is newer than this build ({FORMAT_VERSION})"));
    }
    let (data, sum) = b.split_at(b.len() - 8);
    if fnv64(data) != u64::from_le_bytes(sum.try_into().unwrap()) {
        return err(format!("{what} file is corrupted (checksum)"));
    }
    Ok(&data[6..])
}
