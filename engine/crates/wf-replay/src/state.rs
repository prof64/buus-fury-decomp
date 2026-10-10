//! Save states: one sim snapshot plus the recording that led to it.

use serde::{Deserialize, Serialize};

use crate::{lz, put_blob, seal, unseal, ContentId, Reader, Replay, Result};

const MAGIC: &[u8; 4] = b"WFSS";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StateHeader {
    pub sim: String,
    pub content: ContentId,
    pub config: serde_json::Value,
    pub frame: u64,
    /// state hash at `frame` (checked after loading)
    pub hash: String,
    /// wall clock seconds, metadata only
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub slot: u32,
    #[serde(default)]
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveState {
    pub header: StateHeader,
    /// `Sim::snapshot()` bytes
    pub snapshot: Vec<u8>,
    /// The inputs from the recording's start up to `frame` (so recording continues across a load).
    pub replay: Option<Replay>,
}

impl SaveState {
    /// `"WFSS" ver | blob(header JSON) | blob(lz snapshot) | blob(replay file or empty) | fnv64`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        put_blob(&mut b, &serde_json::to_vec(&self.header).unwrap_or_default());
        put_blob(&mut b, &lz::compress(&self.snapshot));
        put_blob(&mut b, &self.replay.as_ref().map(|r| r.to_bytes()).unwrap_or_default());
        seal(MAGIC, &b)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<SaveState> {
        let body = unseal(MAGIC, "save state", bytes)?;
        let mut r = Reader::new(body);
        let header: StateHeader = serde_json::from_slice(r.blob()?).map_err(|e| crate::Error(format!("save state header: {e}")))?;
        let snapshot = lz::decompress(r.blob()?)?;
        let rb = r.blob()?;
        let replay = if rb.is_empty() { None } else { Some(Replay::from_bytes(rb)?) };
        Ok(SaveState { header, snapshot, replay })
    }
    pub fn peek_header(bytes: &[u8]) -> Result<StateHeader> {
        if bytes.len() < 14 || &bytes[..4] != MAGIC {
            return crate::err("not a save state file");
        }
        let mut r = Reader::new(&bytes[6..]);
        serde_json::from_slice(r.blob()?).map_err(|e| crate::Error(format!("save state header: {e}")))
    }
}
