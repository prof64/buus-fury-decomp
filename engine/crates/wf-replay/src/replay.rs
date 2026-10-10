//! The replay file: header + input log + state-hash checkpoints; recording and verification.

use serde::{Deserialize, Serialize};
use wf_view::{Sim, SimEvent};

use crate::{err, lz, put_blob, put_varint, seal, unseal, InputLog, Reader, Result};

const MAGIC: &[u8; 4] = b"WFRP";

/// Checkpoint spacing used by default (frames): one state hash per second of play (8 bytes / s).
pub const DEFAULT_HASH_INTERVAL: u32 = 60;

/// What a recording depends on besides its inputs. Two runs with equal `ContentId` and config replay
/// identically; any difference is reported (and the checkpoints tell whether it matters).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentId {
    /// The simulation build, e.g. `wf-core 0.1.0`.
    pub engine: String,
    /// `pack.json.generated_by` of the base pack (extractor version).
    pub extractor: String,
    /// SHA-1 of the ROM the base pack was extracted from.
    #[serde(default)]
    pub rom_sha1: String,
    /// Mod ids in load order.
    #[serde(default)]
    pub mods: Vec<String>,
    /// FNV-1a 64 over every mod's id, file paths and bytes, as hex (`0` = no mods).
    #[serde(default)]
    pub mods_hash: String,
}

impl ContentId {
    /// Human-readable differences (empty = same content).
    pub fn diff(&self, other: &ContentId) -> Vec<String> {
        let mut d = Vec::new();
        if *self == ContentId::default() || *other == ContentId::default() {
            return d; // unknown (imported from a format without it)
        }
        if self.engine != other.engine {
            d.push(format!("engine {} vs {}", self.engine, other.engine));
        }
        if self.extractor != other.extractor {
            d.push(format!("extractor {} vs {}", self.extractor, other.extractor));
        }
        if self.rom_sha1 != other.rom_sha1 {
            d.push("ROM differs".into());
        }
        if self.mods_hash != other.mods_hash || self.mods != other.mods {
            d.push(format!("mods [{}] vs [{}]", self.mods.join(", "), other.mods.join(", ")));
        }
        d
    }
}

impl ContentId {
    /// `Some(message)` when a recording / state made with these mods (`self`) cannot run on `loaded`: other mod ids
    /// or other mod files (different content = a desync from the first frame it matters). Engine / extractor
    /// differences stay warnings (`diff`): a newer build may still replay exactly, which the checkpoints decide.
    pub fn mods_mismatch(&self, loaded: &ContentId) -> Option<String> {
        if *self == ContentId::default() || *loaded == ContentId::default() {
            return None;
        }
        if self.mods_hash == loaded.mods_hash && self.mods == loaded.mods {
            return None;
        }
        let list = |m: &[String]| if m.is_empty() { "none".to_string() } else { m.join(", ") };
        Some(if self.mods == loaded.mods {
            format!("made with the same mods ({}) but other versions of their files - load the exact mod folders it was made with", list(&self.mods))
        } else {
            format!("made with mods: {}; loaded now: {} - load the same mods (Mods menu, or --mod / --no-mods) to play it", list(&self.mods), list(&loaded.mods))
        })
    }
}

/// A recording that continues another one from a frame ("take over" a replay, load a save state).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    /// Id (file name stem) of the parent recording.
    pub parent: String,
    /// Frame the new inputs start at (inputs before it are the parent's).
    pub frame: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// Which simulation the config is for, e.g. `wf-core/battle`.
    pub sim: String,
    pub content: ContentId,
    /// The simulation's own config (for wf-core: `BattleConfig`), opaque here.
    pub config: serde_json::Value,
    /// RNG seed (also inside `config`; duplicated for listings and tools).
    #[serde(default)]
    pub seed: u64,
    pub players: u8,
    /// State hash checkpoint spacing in frames (1 = every frame: exact divergence frame on verify).
    pub hash_interval: u32,
    /// Free-form title for listings (`forest vs t68`).
    #[serde(default)]
    pub title: String,
    /// Wall-clock creation time (seconds since the Unix epoch): metadata only, never fed to the sim.
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub branch: Option<Branch>,
    /// Outcome / notes written when the recording is finalised (`won`, `lost`, `running`).
    #[serde(default)]
    pub outcome: String,
    /// Frame count and final hash (redundant with the body; for listings without decoding it).
    #[serde(default)]
    pub frames: u64,
    #[serde(default)]
    pub final_hash: String,
}

/// A complete recording.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replay {
    pub header: Header,
    /// Serialized sim state the recording starts from (`None` = fresh from `config` at frame 0).
    pub start_state: Option<Vec<u8>>,
    pub inputs: InputLog,
    /// (frame, state hash after `frame` steps), ascending. The first is the start state's hash.
    pub checkpoints: Vec<(u64, u64)>,
}

impl Replay {
    pub fn start_frame(&self) -> u64 {
        self.inputs.start
    }
    pub fn end_frame(&self) -> u64 {
        self.inputs.end()
    }
    pub fn checkpoint(&self, frame: u64) -> Option<u64> {
        self.checkpoints.binary_search_by_key(&frame, |c| c.0).ok().map(|i| self.checkpoints[i].1)
    }

    /// `"WFRP" ver | blob(header JSON) | flags | [blob(lz start state)] | input log | checkpoints | fnv64`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut h = self.header.clone();
        h.frames = self.inputs.len();
        if let Some(&(_, x)) = self.checkpoints.last() {
            h.final_hash = format!("{x:016x}");
        }
        let mut b = Vec::new();
        put_blob(&mut b, &serde_json::to_vec(&h).unwrap_or_default());
        b.push(self.start_state.is_some() as u8);
        if let Some(s) = &self.start_state {
            put_blob(&mut b, &lz::compress(s));
        }
        self.inputs.encode(&mut b);
        put_varint(&mut b, self.checkpoints.len() as u64);
        let mut last = 0;
        for &(f, x) in &self.checkpoints {
            put_varint(&mut b, f - last);
            b.extend_from_slice(&x.to_le_bytes());
            last = f;
        }
        seal(MAGIC, &b)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Replay> {
        let body = unseal(MAGIC, "replay", bytes)?;
        let mut r = Reader::new(body);
        let header: Header = serde_json::from_slice(r.blob()?).map_err(|e| crate::Error(format!("replay header: {e}")))?;
        let flags = r.bytes(1)?[0];
        let start_state = if flags & 1 != 0 { Some(lz::decompress(r.blob()?)?) } else { None };
        let inputs = InputLog::decode(&mut r)?;
        let n = r.varint()? as usize;
        if n > inputs.len() as usize + 2 {
            return err("replay: too many checkpoints");
        }
        let mut checkpoints = Vec::with_capacity(n);
        let mut f = 0;
        for _ in 0..n {
            f += r.varint()?;
            checkpoints.push((f, r.u64()?));
        }
        Ok(Replay { header, start_state, inputs, checkpoints })
    }

    /// JSON interchange form `{"format": "wf-replay/1", "header", "start_state"?: lz hex, "inputs": [[[buttons per
    /// player], frames], ...], "start", "checkpoints": [[frame, "hash hex"], ...]}` (tools, agents, diffs).
    pub fn to_json(&self) -> serde_json::Value {
        let mut runs: Vec<(Vec<u16>, u64)> = Vec::new();
        for f in self.inputs.frames() {
            match runs.last_mut() {
                Some((m, c)) if m.as_slice() == f => *c += 1,
                _ => runs.push((f.to_vec(), 1)),
            }
        }
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        serde_json::json!({
            "format": "wf-replay/1",
            "header": self.header,
            "start": self.inputs.start,
            "start_state": self.start_state.as_ref().map(|s| hex(&lz::compress(s))),
            "inputs": runs,
            "checkpoints": self.checkpoints.iter().map(|(f, h)| (f, format!("{h:016x}"))).collect::<Vec<_>>(),
        })
    }

    /// Read any replay: this crate's binary file, its JSON form, or the agent server's `wf-agent-replay/1` JSON
    /// (`{config, slots, inputs: [[[mask per slot], frames]...], frames, hash, outcome}`; its final hash becomes
    /// the one checkpoint; `sim` / `content` are left for the caller to fill).
    pub fn from_any(bytes: &[u8]) -> Result<Replay> {
        if bytes.starts_with(MAGIC) {
            return Replay::from_bytes(bytes);
        }
        let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| crate::Error(format!("replay: {e}")))?;
        let bad = |m: &str| crate::Error(format!("replay JSON: {m}"));
        let fmt = v["format"].as_str().unwrap_or("");
        let runs = v["inputs"].as_array().ok_or_else(|| bad("no inputs"))?;
        let players = match fmt {
            "wf-agent-replay/1" => v["slots"].as_u64().unwrap_or(1) as usize,
            "wf-replay/1" => v["header"]["players"].as_u64().unwrap_or(1) as usize,
            _ => return Err(bad(&format!("unknown format {fmt:?}"))),
        };
        let mut inputs = InputLog::new(players, v["start"].as_u64().unwrap_or(0));
        for r in runs {
            let mask: Vec<u16> = r[0].as_array().ok_or_else(|| bad("run"))?.iter().map(|x| x.as_u64().unwrap_or(0) as u16).collect();
            for _ in 0..r[1].as_u64().ok_or_else(|| bad("run length"))? {
                inputs.push(&mask);
            }
        }
        let hx = |s: &str| u64::from_str_radix(s, 16).map_err(|_| bad("hash"));
        if fmt == "wf-agent-replay/1" {
            let config = v["config"].clone();
            let header = Header {
                players: players as u8,
                hash_interval: u32::MAX,
                outcome: v["outcome"].as_str().unwrap_or("").into(),
                title: "agent replay".into(),
                config,
                ..Default::default()
            };
            let end = inputs.end();
            let checkpoints = v["hash"].as_str().map(|h| hx(h).map(|h| vec![(end, h)])).transpose()?.unwrap_or_default();
            return Ok(Replay { header, start_state: None, inputs, checkpoints });
        }
        let header: Header = serde_json::from_value(v["header"].clone()).map_err(|e| bad(&e.to_string()))?;
        let start_state = match v["start_state"].as_str() {
            Some(s) => {
                let b: Vec<u8> = (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0)).collect();
                Some(lz::decompress(&b)?)
            }
            None => None,
        };
        let mut checkpoints = Vec::new();
        for c in v["checkpoints"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            checkpoints.push((c[0].as_u64().ok_or_else(|| bad("checkpoint"))?, hx(c[1].as_str().unwrap_or(""))?));
        }
        Ok(Replay { header, start_state, inputs, checkpoints })
    }

    /// Only the header (cheap listing of a replay folder).
    pub fn peek_header(bytes: &[u8]) -> Result<Header> {
        if bytes.len() < 14 || &bytes[..4] != MAGIC {
            return err("not a replay file");
        }
        let mut r = Reader::new(&bytes[6..]);
        serde_json::from_slice(r.blob()?).map_err(|e| crate::Error(format!("replay header: {e}")))
    }
}

/// Builds a [`Replay`] while a session runs.
#[derive(Clone, Debug)]
pub struct Recorder {
    pub replay: Replay,
}

impl Recorder {
    /// Start recording a sim that is at `start_frame` with state hash `start_hash` (`start_state`: its snapshot,
    /// needed unless the sim was freshly built from the header's config).
    pub fn new(header: Header, start_frame: u64, start_hash: u64, start_state: Option<Vec<u8>>) -> Recorder {
        let players = header.players.max(1) as usize;
        Recorder { replay: Replay { header, start_state, inputs: InputLog::new(players, start_frame), checkpoints: vec![(start_frame, start_hash)] } }
    }
    /// Continue an existing recording (after loading a save state / taking over a replay at its end).
    pub fn resume(replay: Replay) -> Recorder {
        Recorder { replay }
    }
    /// The frame the next recorded step starts at.
    pub fn frame(&self) -> u64 {
        self.replay.inputs.end()
    }
    /// Record one step: `inputs` were applied and the sim is now at `frame_after`; `hash` is called on checkpoint
    /// frames only.
    pub fn record(&mut self, inputs: &[u16], frame_after: u64, hash: impl FnOnce() -> u64) {
        self.replay.inputs.push(inputs);
        let iv = self.replay.header.hash_interval.max(1) as u64;
        if frame_after % iv == 0 {
            self.replay.checkpoints.push((frame_after, hash()));
        }
    }
    /// Forget everything after sim frame `frame` (the sim was rewound there and continues with new inputs).
    pub fn truncate(&mut self, frame: u64) {
        self.replay.inputs.truncate(frame);
        let keep = self.replay.checkpoints.partition_point(|c| c.0 <= frame);
        self.replay.checkpoints.truncate(keep.max(1));
    }
    /// Add a final checkpoint (end of the recording) so verification covers the last frames.
    pub fn finish(&mut self, hash: u64) -> &Replay {
        let end = self.replay.inputs.end();
        if self.replay.checkpoints.last().map(|c| c.0) != Some(end) {
            self.replay.checkpoints.push((end, hash));
        }
        &self.replay
    }
}

/// A checkpoint whose hash differs: the state first differs somewhere in `(last_good, frame]` (exactly at
/// `frame` when the replay has a checkpoint every frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Divergence {
    pub frame: u64,
    pub last_good: u64,
    pub expected: u64,
    pub got: u64,
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.frame == self.last_good + 1 {
            write!(f, "diverged at frame {} (state {:016x}, expected {:016x})", self.frame, self.got, self.expected)
        } else {
            write!(f, "diverged between frames {} and {} (state {:016x}, expected {:016x})", self.last_good + 1, self.frame, self.got, self.expected)
        }
    }
}

/// Incremental checkpoint comparison for a replay being played back (any pace, any seeks).
#[derive(Clone, Debug, Default)]
pub struct Checker {
    /// first divergence seen (sticky)
    pub divergence: Option<Divergence>,
    pub checked: usize,
}

impl Checker {
    /// Compare the sim at `frame` (call after every step); `hash` is only evaluated on checkpoint frames.
    pub fn check(&mut self, replay: &Replay, frame: u64, hash: impl FnOnce() -> u64) -> Option<Divergence> {
        let i = replay.checkpoints.binary_search_by_key(&frame, |c| c.0).ok()?;
        let expected = replay.checkpoints[i].1;
        let got = hash();
        self.checked += 1;
        if got == expected {
            return None;
        }
        let d = Divergence { frame, last_good: if i > 0 { replay.checkpoints[i - 1].0 } else { frame.saturating_sub(1) }, expected, got };
        if self.divergence.is_none_or(|x| x.frame > d.frame) {
            self.divergence = Some(d);
        }
        Some(d)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct VerifyReport {
    pub frames: u64,
    pub checkpoints: usize,
    pub divergence: Option<Divergence>,
    pub final_hash: u64,
    pub ok: bool,
}

/// Replay `r` on `sim` (freshly built from `r.header.config`; a start state is restored here) and compare every
/// checkpoint. Stops at the first divergence.
pub fn verify(r: &Replay, sim: &mut dyn Sim) -> VerifyReport {
    if let Some(s) = &r.start_state {
        if !sim.restore(s) {
            return VerifyReport { frames: 0, checkpoints: 0, divergence: None, final_hash: 0, ok: false };
        }
    }
    let mut c = Checker::default();
    let mut ev: Vec<SimEvent> = Vec::new();
    let mut f = sim.frame();
    if c.check(r, f, || sim.state_hash()).is_some() {
        return VerifyReport { frames: 0, checkpoints: c.checked, divergence: c.divergence, final_hash: sim.state_hash(), ok: false };
    }
    while let Some(inp) = r.inputs.get(f) {
        ev.clear();
        sim.step(inp, &mut ev);
        f = sim.frame();
        if c.check(r, f, || sim.state_hash()).is_some() {
            break;
        }
    }
    let ok = c.divergence.is_none();
    VerifyReport { frames: f - r.start_frame(), checkpoints: c.checked, divergence: c.divergence, final_hash: sim.state_hash(), ok }
}
