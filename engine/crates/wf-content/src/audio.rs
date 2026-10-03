//! Rebuild `wf_audio::bank::SoundBank` from the `audio/` content files (docs/audio.md §5), so the runtime plays
//! (modded) content with the exact driver and never needs the ROM.

use serde_json::Value;
use wf_audio::bank::SoundBank;

use crate::{Error, Packs, Result};

pub fn load_bank(packs: &Packs) -> Result<SoundBank> {
    wf_audio::content::load_bank(&|p| packs.read(p)).map_err(Error)
}

/// Song ids of the arena music table and other curated triggers (`audio/index.json`).
pub fn arena_song(packs: &Packs, arena: &str) -> Option<u16> {
    let index: Value = packs.json("audio/index.json").ok()?;
    index["arena_music"][arena].as_u64().map(|v| v as u16)
}
