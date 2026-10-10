//! The sound data model the driver plays: sample PCM, sample entries (instruments / SFX), songs (orders +
//! patterns of decoded cells) and the driver's lookup tables. Built from the ROM by [`SoundBank::from_rom`];
//! the same model is what `audio/*.json` + `audio/samples/*.wav` serialize, so a player can be fed from
//! (modded) content files instead.

use wf_rom::Rom;

pub const SONG_TABLE: u32 = 0x083B_B78C;
pub const N_SONGS: usize = 53;
pub const SFX_TABLE: u32 = 0x0831_92A8;
pub const N_SFX: usize = 64;
/// Shared instrument table (12-byte sample entries) referenced by every song header (+8).
pub const INSTRUMENT_TABLE: u32 = 0x0836_D18C;
/// Shared "cell flag preset" table referenced by every song header (+0xC): 14 bytes, indexed by cell lead
/// byte high nibble - 2.
pub const PRESET_TABLE: u32 = 0x0836_D540;
/// Note -> pitch (u32, 0x4000 = 1.0 = the sample's own rate; note 60 = 0x4000, 12-TET).
pub const PITCH_TABLE: u32 = 0x087B_6724;
/// Effect handlers (32 x Thumb function pointer).
pub const EFFECT_TABLE: u32 = 0x087B_6904;
/// Pitch-slide-up multipliers (u16, x / 16384); the 64 entries before it are the fine table (x / 32768).
pub const SLIDE_UP_TABLE: u32 = 0x087B_6A04;
/// Pitch-slide-down multipliers (u16, x / 65536); the 64 entries before it are the fine table.
pub const SLIDE_DOWN_TABLE: u32 = 0x087B_6C84;
/// Vibrato sine (64 x s8, amplitude 64).
pub const SINE_TABLE: u32 = 0x087B_6E84;

/// One unique block of signed 8-bit PCM in the ROM.
#[derive(Clone, Debug)]
pub struct Sample {
    pub rom: u32,
    pub pcm: Vec<i8>,
}

/// A 12-byte sample entry `{u32 pcm, u8 loop_mode, u8 pad, u16 loop_start, u16 length, u16 rate}`: what the
/// driver loads into a voice. Used both as song instruments and as SFX.
#[derive(Clone, Debug)]
pub struct Entry {
    pub rom: u32,
    /// Index into [`SoundBank::samples`].
    pub sample: usize,
    /// bit0 = loop, bit1 = ping-pong loop (bit2 is the runtime "playing backwards" flag).
    pub loop_mode: u8,
    pub loop_start: u16,
    pub length: u16,
    /// Playback rate in Hz at pitch 1.0.
    pub rate: u16,
}

/// One decoded pattern cell. `flags` is the driver's cell flag byte (see docs/audio.md). `note`,
/// `instrument`, `volume`, `effect`+`param` are meaningful only when their "present" bit (1, 2, 4, 8) is set;
/// otherwise the channel keeps the value from its previous cell (the driver's per-channel cell buffer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub channel: u8,
    pub flags: u8,
    pub note: u8,
    pub instrument: u8,
    pub volume: u8,
    pub effect: u8,
    pub param: u8,
}

pub mod flag {
    /// note present (bit0) or "repeat last note" (bit4): (re)trigger
    pub const NOTE: u8 = 0x11;
    /// keep the voice's current sample entry (no instrument reload) when triggering
    pub const KEEP_SAMPLE: u8 = 0x20;
    /// volume present (bit2) or repeated (bit6)
    pub const VOLUME: u8 = 0x44;
    /// effect + param present (bit3) or repeated (bit7)
    pub const EFFECT: u8 = 0x88;
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub rom: u32,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug)]
pub struct Song {
    pub rom: u32,
    /// "tempo": samples per tick = 40000 / tempo at the 15 978 Hz mix rate.
    pub tempo: u8,
    /// ticks per row
    pub speed: u8,
    /// pattern index per order position (the ROM list ends with 0xFF -> back to order 0)
    pub orders: Vec<u8>,
    pub patterns: Vec<Pattern>,
}

/// Driver lookup tables (ROM data the player needs).
#[derive(Clone, Debug)]
pub struct Tables {
    pub pitch: Vec<u32>,
    pub slide_up: Vec<u16>,
    pub slide_up_fine: Vec<u16>,
    pub slide_down: Vec<u16>,
    pub slide_down_fine: Vec<u16>,
    pub sine: Vec<i8>,
    /// effect id -> handler kind (see [`crate::driver::EffectKind`]) for ids 0..31
    pub effects: Vec<crate::driver::EffectKind>,
}

#[derive(Clone, Debug)]
pub struct SoundBank {
    pub samples: Vec<Sample>,
    pub instruments: Vec<Entry>,
    pub sfx: Vec<Entry>,
    pub songs: Vec<Song>,
    pub tables: Tables,
    pub warnings: Vec<String>,
}

impl SoundBank {
    pub fn from_rom(rom: &Rom) -> SoundBank {
        let mut samples: Vec<Sample> = Vec::new();
        let mut warnings = Vec::new();
        let entry = |a: u32, samples: &mut Vec<Sample>| -> Entry {
            let ptr = rom.u32(a);
            let length = rom.u16(a + 8);
            let idx = match samples.iter().position(|s| s.rom == ptr) {
                Some(i) => i,
                None => {
                    samples.push(Sample { rom: ptr, pcm: Vec::new() });
                    samples.len() - 1
                }
            };
            let s = &mut samples[idx];
            if s.pcm.len() < length as usize {
                s.pcm = rom.bytes(ptr, length as usize).iter().map(|&b| b as i8).collect();
            }
            Entry { rom: a, sample: idx, loop_mode: rom.u8(a + 4), loop_start: rom.u16(a + 6), length, rate: rom.u16(a + 10) }
        };
        // instruments: the table runs up to the preset table that follows it
        let n_inst = ((PRESET_TABLE - INSTRUMENT_TABLE) / 12) as usize;
        let instruments: Vec<Entry> = (0..n_inst).map(|i| entry(INSTRUMENT_TABLE + 12 * i as u32, &mut samples)).collect();
        let sfx: Vec<Entry> = (0..N_SFX).map(|i| entry(SFX_TABLE + 12 * i as u32, &mut samples)).collect();

        let presets: Vec<u8> = rom.bytes(PRESET_TABLE, 14).to_vec();
        let mut songs = Vec::new();
        for i in 0..N_SONGS {
            let h = SONG_TABLE + 20 * i as u32;
            let pat_tbl = rom.u32(h);
            let ord = rom.u32(h + 4);
            if rom.u32(h + 8) != INSTRUMENT_TABLE || rom.u32(h + 12) != PRESET_TABLE {
                warnings.push(format!("song {i}: non-shared instrument/preset table"));
            }
            let mut orders = Vec::new();
            while orders.len() < 256 {
                let o = rom.u8(ord + orders.len() as u32);
                if o == 0xFF {
                    break;
                }
                orders.push(o);
            }
            let npat = orders.iter().map(|&o| o as usize + 1).max().unwrap_or(0);
            let patterns = (0..npat)
                .map(|p| {
                    let pa = rom.u32(pat_tbl + 4 * p as u32);
                    decode_pattern(rom, pa, &presets, &mut warnings, i)
                })
                .collect();
            songs.push(Song { rom: h, tempo: rom.u8(h + 16), speed: rom.u8(h + 17), orders, patterns });
        }
        let r16 = |a: u32, n: usize| (0..n).map(|k| rom.u16(a + 2 * k as u32)).collect::<Vec<u16>>();
        let tables = Tables {
            pitch: (0..256).map(|k| rom.u32(PITCH_TABLE + 4 * k)).collect(),
            slide_up: r16(SLIDE_UP_TABLE, 256),
            slide_up_fine: r16(SLIDE_UP_TABLE - 0x80, 64),
            slide_down: r16(SLIDE_DOWN_TABLE, 256),
            slide_down_fine: r16(SLIDE_DOWN_TABLE - 0x80, 64),
            sine: rom.bytes(SINE_TABLE, 64).iter().map(|&b| b as i8).collect(),
            effects: (0..32).map(|k| crate::driver::EffectKind::from_handler(rom.u32(EFFECT_TABLE + 4 * k))).collect(),
        };
        SoundBank { samples, instruments, sfx, songs, tables, warnings }
    }
}

/// Decode one pattern exactly like the row reader (`0x0803EEE4`), tracking each channel's cell buffer so every
/// decoded cell carries the resolved values.
fn decode_pattern(rom: &Rom, pa: u32, presets: &[u8], warnings: &mut Vec<String>, song: usize) -> Pattern {
    let nrows = rom.u8(pa) as usize;
    let mut p = pa + 1;
    // "repeat the channel's last explicit flag byte" (lead byte high nibble 1) reads the driver's per-channel
    // memory, which persists across patterns; it is resolved here within the pattern and flagged if a pattern
    // would depend on an earlier one (never happens in the ROM; a test checks there are no such warnings).
    let mut last_flags: [Option<u8>; 16] = [None; 16];
    let mut rows = Vec::with_capacity(nrows);
    for _ in 0..nrows {
        let mut cells = Vec::new();
        loop {
            let b = rom.u8(p);
            p += 1;
            if b == 0 {
                break;
            }
            let ch = (b & 0xF) as i32 - 1;
            if ch < 0 {
                warnings.push(format!("song {song}: cell for channel -1 at 0x{:08X}", p - 1));
            }
            let chu = (ch.max(0) as usize).min(15);
            let hi = b >> 4;
            let flags = match hi {
                0 => {
                    let f = rom.u8(p);
                    p += 1;
                    last_flags[chu] = Some(f);
                    f
                }
                1 => last_flags[chu].unwrap_or_else(|| {
                    warnings.push(format!("song {song}: pattern 0x{pa:08X} repeats flags from an earlier pattern"));
                    0
                }),
                _ => presets[(hi - 2) as usize],
            };
            let mut cell = Cell { channel: chu as u8, flags, note: 0, instrument: 0, volume: 0, effect: 0, param: 0 };
            if flags & 1 != 0 {
                cell.note = rom.u8(p);
                p += 1;
            }
            if flags & 2 != 0 {
                cell.instrument = rom.u8(p);
                p += 1;
            }
            if flags & 4 != 0 {
                cell.volume = rom.u8(p);
                p += 1;
            }
            if flags & 8 != 0 {
                cell.effect = rom.u8(p);
                cell.param = rom.u8(p + 1);
                p += 2;
            }
            cells.push(cell);
        }
        rows.push(cells);
    }
    Pattern { rom: pa, rows }
}
