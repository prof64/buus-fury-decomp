//! Buu's Fury audio without an emulator: the Webfoot sound driver (tracker sequencer + software mixer)
//! re-implemented bit-exactly, plus the moddable content export (`audio/...`). Format: `engine/docs/audio.md`.
//!
//! * [`bank::SoundBank::from_rom`] — samples, instruments, SFX, songs and driver tables from the ROM.
//! * [`driver::Driver`] — the game's driver: `play_song`, `play_sfx`, `render` → exact 8-bit FIFO output
//!   (stereo, 15 978.3 Hz).
//! * [`render_song`] / [`render_sfx`] — convenience renders (8-bit hardware path or 16-bit).
//! * [`hq`] — a higher-quality float renderer (same sequencer, interpolated mixing at any rate).
//! * [`extract`] — content files; [`extract_rendered`] additionally writes WAV renders (not part of the default
//!   extraction).

pub mod bank;
pub mod driver;
pub mod export;
pub mod hq;
pub mod wav;

use bank::SoundBank;
use driver::Driver;
use wf_rom::{content::Content, Rom};

/// Samples per driver update when rendering offline (the game mixes once per 59.73 Hz frame ≈ 267.5 samples).
pub const FRAME_SAMPLES: usize = 267;

/// Render `seconds` of song `id` through the exact driver: interleaved stereo s8 at [`driver::MIX_RATE`].
pub fn render_song(bank: &SoundBank, id: usize, seconds: f64) -> Vec<(i8, i8)> {
    let mut d = Driver::new(bank);
    d.play_song(id);
    d.render((seconds * driver::MIX_RATE) as usize, FRAME_SAMPLES)
}

/// Render SFX `id` (volume 0..64, centre pan) until its voice ends (or `max_seconds` for looping effects).
pub fn render_sfx(bank: &SoundBank, id: u8, volume: u8, max_seconds: f64) -> Vec<(i8, i8)> {
    let mut d = Driver::new(bank);
    d.play_sfx(id, volume, 0);
    let max = (max_seconds * driver::MIX_RATE) as usize;
    let mut out = Vec::new();
    while out.len() < max {
        d.render_into(&mut out, FRAME_SAMPLES);
        if d.busy_voices() == 0 {
            break;
        }
    }
    out
}

/// Scale the 8-bit hardware output to 16-bit PCM (x * 256).
pub fn to_i16(s: &[(i8, i8)]) -> Vec<i16> {
    let mut v = Vec::with_capacity(s.len() * 2);
    for &(l, r) in s {
        v.push(l as i16 * 256);
        v.push(r as i16 * 256);
    }
    v
}

/// Write the moddable audio content (`audio/...`, paths relative to the content root).
pub fn extract(rom: &Rom, out: &mut Content) -> wf_rom::Result<()> {
    let bank = SoundBank::from_rom(rom);
    for w in &bank.warnings {
        out.warn(format!("audio: {w}"));
    }
    export::write(&bank, out);
    Ok(())
}

/// [`extract`] plus rendered WAVs of every song (`seconds` each, hardware 8-bit path at 15 978 Hz) and SFX
/// under `audio/rendered/`. Slow-ish (a few seconds); not part of the default extraction.
pub fn extract_rendered(rom: &Rom, out: &mut Content, seconds: f64) -> wf_rom::Result<()> {
    extract(rom, out)?;
    let bank = SoundBank::from_rom(rom);
    let rate = driver::MIX_RATE.round() as u32;
    for i in 0..bank.songs.len() {
        let pcm = render_song(&bank, i, seconds);
        out.put(format!("audio/rendered/song_{i:02}.wav"), wav::stereo_s8(&pcm, rate));
    }
    for i in 0..bank.sfx.len() {
        let pcm = render_sfx(&bank, i as u8, 64, 10.0);
        out.put(format!("audio/rendered/sfx_{i:02}.wav"), wav::stereo_s8(&pcm, rate));
    }
    Ok(())
}
