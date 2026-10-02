//! Bit-exact re-implementation of Buu's Fury's sound driver (Webfoot's tracker + software mixer).
//!
//! Structure mirrors the game (addresses are the original Thumb/ARM routines):
//! * [`Voice`] — the 0x40-byte voice (`+0` pos 18.14, `+4` step, `+0xC` packed L|R gain, `+0x10` update flags,
//!   `+0x11` loop mode, `+0x12` volume, `+0x13` pan, `+0x30` pitch, `+0x34` master volume).
//!   Update `0x0803E0B6`, mix `0x0803E152` + ARM inner loop `0x03000868`.
//! * [`Track`] — a music channel = voice + effect memory + cell buffer (0x60 bytes); row effects through the
//!   32-entry table `0x087B6904`, per-tick handlers stored at track+0x54.
//! * [`Player`] — `0x030038A4`: speed/row/order counters, tempo -> samples per tick, tick `0x0803EFD0`.
//! * [`Driver`] — the sound object `0x03003898`: player + 16 SFX voices, the 32-bit packed mix buffer and the
//!   8-bit conversion `0x03000A4C` (L = low half >> 6, R = high half >> 6, clipped to s8).
//!
//! The output is the exact pair of signed 8-bit streams the game DMAs to FIFO A (left) / B (right) at
//! 16 777 216 / 1050 = 15 978.3 Hz.

use crate::bank::{flag, Cell, Entry, Song, SoundBank};

/// The hardware mix rate (timer 0 reload 0xFBE6).
pub const MIX_RATE: f64 = 16_777_216.0 / 1050.0;
/// Music master volume = the game's default Options setting (`0x08006B36`: music 4, SFX 6; the sound init
/// `0x0803F1B8` sets 4 / 8 before the options are applied). Range 0..8 in the Options menu.
pub const MUSIC_MASTER: u8 = 4;
/// SFX master volume, default Options setting (see [`MUSIC_MASTER`]).
pub const SFX_MASTER: u8 = 6;
pub const N_TRACKS: usize = 15;
pub const N_VOICES: usize = 16;

/// Row-effect handlers (effect id -> kind via the ROM table `0x087B6904`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    /// `0x0803E4E4`: plain note/volume trigger (effect 0 and every unused id)
    None,
    SetSpeed,
    PatternJump,
    VolumeSlide,
    PitchSlideDown,
    PitchSlideUp,
    TonePortamento,
    Vibrato,
    VibratoVolumeSlide,
    PortamentoVolumeSlide,
    SampleOffset,
    Retrigger,
    SetTempo,
    FineVibrato,
    SetPan,
    NoteDelay,
    /// a handler address this implementation does not know (treated as `None`)
    Unknown(u32),
}

impl EffectKind {
    pub fn from_handler(a: u32) -> EffectKind {
        use EffectKind::*;
        match a & !1 {
            0x0803_E4E4 => None,
            0x0803_E574 => SetSpeed,
            0x0803_E58A => PatternJump,
            0x0803_E608 => VolumeSlide,
            0x0803_E6AA => PitchSlideDown,
            0x0803_E738 => PitchSlideUp,
            0x0803_E848 => TonePortamento,
            0x0803_E904 => Vibrato,
            0x0803_E9A8 => VibratoVolumeSlide,
            0x0803_EA6C => PortamentoVolumeSlide,
            0x0803_EB6C => SampleOffset,
            0x0803_EBC4 => Retrigger,
            0x0803_EC3C => SetTempo,
            0x0803_EC5A => FineVibrato,
            0x0803_ECA6 => SetPan,
            0x0803_EC10 => NoteDelay,
            x => Unknown(x),
        }
    }
    pub fn name(self) -> &'static str {
        use EffectKind::*;
        match self {
            None | Unknown(_) => "none",
            SetSpeed => "set_speed",
            PatternJump => "pattern_jump",
            VolumeSlide => "volume_slide",
            PitchSlideDown => "pitch_slide_down",
            PitchSlideUp => "pitch_slide_up",
            TonePortamento => "tone_portamento",
            Vibrato => "vibrato",
            VibratoVolumeSlide => "vibrato_volume_slide",
            PortamentoVolumeSlide => "portamento_volume_slide",
            SampleOffset => "sample_offset",
            Retrigger => "retrigger",
            SetTempo => "set_tempo",
            FineVibrato => "fine_vibrato",
            SetPan => "set_pan",
            NoteDelay => "note_delay",
        }
    }
}

/// Per-tick handler stored at track+0x54.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickFn {
    VolUp,            // 0x0803E5DA
    VolDown,          // 0x0803E5AC
    PitchDown,        // 0x0803E68C
    PitchUp,          // 0x0803E71A
    PortaDown,        // 0x0803E7C4
    PortaUp,          // 0x0803E806
    Vibrato,          // 0x0803E8BC
    VibVolDown,       // 0x0803E954
    VibVolUp,         // 0x0803E97E
    Retrig,           // 0x0803EB98
    Delay,            // 0x0803EBF8
    PortaUpVolUp,     // 0x0803ECC6 (game bug kept: snaps to the target while below it)
    PortaDownVolUp,   // 0x0803ED2A
    PortaUpVolDown,   // 0x0803EDBC
    PortaDownVolDown, // 0x0803EE20
}

/// Which sample entry a voice plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EntryRef {
    #[default]
    None,
    Instrument(u8),
    Sfx(u8),
}

#[derive(Clone, Debug, Default)]
pub struct Voice {
    pub pos: i32,
    pub step: i32,
    pub sample: Option<usize>,
    /// `+0xC`: u16 left gain | u16 right gain << 16
    pub gain: u32,
    pub flags: u8,
    pub loop_mode: u8,
    pub volume: u8,
    pub pan: i8,
    pub rate: u16,
    pub step_mag: i32,
    pub loop_start: i32,
    pub end: i32,
    pub entry: EntryRef,
    pub pitch: u32,
    pub master: u8,
    /// `+0x2C` set for music tracks (end of a one-shot sample clears the track's "note on" bit)
    pub is_track: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Track {
    pub v: Voice,
    // +0x40..
    base_pitch: u32, // +0x40
    target: u32,     // +0x44
    vol: u8,         // +0x48 (s8 arithmetic)
    pan_mem: u8,     // +0x49
    mode: u8,        // +0x4A bits0-1 tick mode, bit2 note on, bit3 portamento target set
    slide: u8,       // +0x4B
    volslide_mem: u8, // +0x4C
    pitch_mem: u8,   // +0x4D
    retrig_mem: u8,  // +0x4E
    vib_speed: u8,   // +0x4F
    // +0x50.. cell buffer
    vib_depth: u8,   // +0x50
    vib_phase: u8,   // +0x51
    counter: u8,     // +0x52
    tick: Option<TickFn>,
    cflags: u8,      // +0x58
    note: u8,        // +0x5A
    inst: u8,        // +0x5B
    cvol: u8,        // +0x5C
    fx: u8,          // +0x5D
    param: u8,       // +0x5E
}

#[derive(Clone, Debug, Default)]
pub struct Player {
    pub ticks_left: u8,       // +0
    pub row: u8,              // +1
    pub rows: u8,             // +2
    pub speed: u8,            // +3
    pub tempo: u8,            // +4
    pub playing: bool,        // +5
    pub samples_per_tick: u16, // +6
    pub countdown: u16,       // +8
    pub song: Option<usize>,  // +0xC
    pub order: i32,           // +0x10
    pattern: usize,
    row_in_pattern: usize,
    pub tracks: Vec<Track>,
}

/// What the sequencer did at a row (for tooling / loop detection).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowEvent {
    pub sample: u64,
    pub order: i32,
    pub row: u8,
}

pub struct Driver<'a> {
    pub bank: &'a SoundBank,
    pub player: Player,
    pub voices: Vec<Voice>,
    pub(crate) mixbuf: Vec<u32>,
    /// samples output so far
    pub clock: u64,
    /// row log (only when `log_rows`)
    pub rows_log: Vec<RowEvent>,
    pub log_rows: bool,
    /// debug: tracks whose output is discarded (their state still advances)
    pub mute: u16,
    scratch: Vec<u32>,
}

#[inline]
fn div(a: i32, b: i32) -> i32 {
    // BIOS Div (SWI 6): signed, truncating
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

impl Voice {
    /// `0x0803E0B6`: apply pending changes (flag bits 4 volume, 3 load entry, 2 restart, 1 pitch).
    pub(crate) fn update(&mut self, bank: &SoundBank) {
        let f = self.flags;
        if f & 1 == 0 {
            return;
        }
        if f & 0x10 != 0 {
            let vol = self.master as i32 * self.volume as i32;
            let p = self.pan as i32;
            let l = ((vol * (64 - p)) >> 10) as u16;
            let r = ((vol * (64 + p)) >> 10) as u16;
            self.gain = l as u32 | (r as u32) << 16;
        }
        if f & 8 != 0 {
            let e: Option<&Entry> = match self.entry {
                EntryRef::Instrument(i) => bank.instruments.get(i as usize),
                EntryRef::Sfx(i) => bank.sfx.get(i as usize),
                EntryRef::None => None,
            };
            if let Some(e) = e {
                self.sample = Some(e.sample);
                self.loop_start = (e.loop_start as i32) << 14;
                self.end = (e.length as i32) << 14;
                self.rate = e.rate;
                self.loop_mode = e.loop_mode;
            } else {
                self.sample = None;
                self.loop_start = 0;
                self.end = 0;
            }
        }
        if f & 4 != 0 {
            self.pos = 0;
            self.loop_mode &= !4;
        }
        if f & 2 != 0 {
            let s = (self.pitch.wrapping_mul(self.rate as u32) >> 14).wrapping_mul(0x1063).wrapping_add(0x7FF) >> 12;
            self.step_mag = s as i32;
            self.step = if self.loop_mode & 4 != 0 { -(s as i32) } else { s as i32 };
        }
        self.flags = 1;
    }

    /// ARM inner loop `0x03000868`: buf[i] += gain * pcm[(pos + i*step) >> 14]. (Silent voices skip the call.)
    #[inline]
    fn mix_run(&self, bank: &SoundBank, buf: &mut [u32], k: usize) {
        if self.gain == 0 {
            return;
        }
        let pcm: &[i8] = match self.sample {
            Some(s) => &bank.samples[s].pcm,
            None => &[],
        };
        let mut pos = self.pos as i64;
        let step = self.step as i64;
        for w in buf.iter_mut().take(k) {
            let idx = pos >> 14;
            let s = if idx >= 0 { pcm.get(idx as usize).copied().unwrap_or(0) } else { 0 } as i32;
            *w = w.wrapping_add(self.gain.wrapping_mul(s as u32));
            pos += step;
        }
    }

    /// `0x0803E152`: mix `n` samples handling loop / ping-pong / end. Returns true when a one-shot ended.
    pub(crate) fn mix(&mut self, bank: &SoundBank, buf: &mut [u32], n: usize) -> bool {
        if self.flags & 1 == 0 {
            return false;
        }
        let mut n = n as i32;
        let mut off = 0usize;
        if self.loop_mode & 1 == 0 {
            // one-shot
            let end = self.pos.wrapping_add(self.step.wrapping_mul(n));
            if self.end > end {
                self.mix_run(bank, &mut buf[off..], n as usize);
                self.pos = end;
                return false;
            }
            let k = div(self.end + self.step_mag - 1 - self.pos, self.step_mag);
            self.mix_run(bank, &mut buf[off..], k.max(0) as usize);
            self.flags = 0;
            return true;
        }
        if self.loop_mode & 2 == 0 {
            // forward loop
            loop {
                let end = self.pos.wrapping_add(self.step.wrapping_mul(n));
                if self.end > end {
                    self.mix_run(bank, &mut buf[off..], n as usize);
                    self.pos = end;
                    return false;
                }
                let k = div(self.end + self.step_mag - 1 - self.pos, self.step_mag);
                let np = self.pos + self.step_mag * k - (self.end - self.loop_start);
                self.mix_run(bank, &mut buf[off..], k.max(0) as usize);
                off += k.max(0) as usize;
                n -= k;
                self.pos = np;
                if n <= 0 {
                    return false;
                }
            }
        }
        // ping-pong
        loop {
            let end = self.pos.wrapping_add(self.step.wrapping_mul(n));
            if self.loop_mode & 4 != 0 {
                if self.loop_start <= end {
                    self.mix_run(bank, &mut buf[off..], n as usize);
                    self.pos = end;
                    return false;
                }
                let k = div(self.pos + self.step_mag - 1 - self.loop_start, self.step_mag);
                let np = 2 * self.loop_start - self.pos + self.step_mag * k;
                self.mix_run(bank, &mut buf[off..], k.max(0) as usize);
                off += k.max(0) as usize;
                n -= k;
                self.pos = np;
                self.loop_mode &= !4;
                self.step = self.step_mag;
            } else {
                if self.end > end {
                    self.mix_run(bank, &mut buf[off..], n as usize);
                    self.pos = end;
                    return false;
                }
                let k = div(self.end + self.step_mag - 1 - self.pos, self.step_mag);
                let np = 2 * self.end - self.pos - self.step_mag * k - 1;
                self.mix_run(bank, &mut buf[off..], k.max(0) as usize);
                off += k.max(0) as usize;
                n -= k;
                self.pos = np;
                self.loop_mode |= 4;
                self.step = -self.step_mag;
            }
            if n <= 0 {
                return false;
            }
        }
    }
}

impl Track {
    fn s8add(a: u8, b: u8) -> i8 {
        a.wrapping_add(b) as i8
    }
    // 0x0803E5DA
    fn vol_up(&mut self) {
        let v = Self::s8add(self.vol, self.slide);
        self.vol = v as u8;
        if v >= 0x40 {
            self.vol = 0x40;
            self.mode &= !3;
        }
        self.v.volume = self.vol;
        self.v.flags |= 0x10;
    }
    // 0x0803E5AC
    fn vol_down(&mut self) {
        let v = self.vol.wrapping_sub(self.slide) as i8;
        self.vol = v as u8;
        if v <= 0 {
            self.vol = 0;
            self.mode &= !3;
        }
        self.v.volume = self.vol;
        self.v.flags |= 0x10;
    }
    fn set_pitch(&mut self, p: u32) {
        self.base_pitch = p;
        self.v.pitch = p;
        self.v.flags |= 2;
    }
    // 0x0803E8BC
    fn vibrato(&mut self, t: &crate::bank::Tables) {
        self.vib_phase = (self.vib_phase.wrapping_add(self.vib_speed)) & 63;
        let s = t.sine[self.vib_phase as usize] as i32;
        let d = (self.vib_depth as i32 * s) >> 6;
        let p = if d >= 0 {
            let m = t.slide_up_fine.get(d as usize).copied().unwrap_or(0) as u32;
            self.base_pitch.wrapping_mul(m) >> 15
        } else {
            let m = t.slide_down_fine.get((-d) as usize).copied().unwrap_or(0) as u32;
            self.base_pitch.wrapping_mul(m) >> 16
        };
        self.v.pitch = p;
        self.v.flags |= 2;
    }
    fn mul_up(&self, t: &crate::bank::Tables) -> u32 {
        self.base_pitch.wrapping_mul(t.slide_up[self.pitch_mem as usize] as u32) >> 14
    }
    fn mul_down(&self, t: &crate::bank::Tables) -> u32 {
        self.base_pitch.wrapping_mul(t.slide_down[self.pitch_mem as usize] as u32) >> 16
    }

    fn run_tick(&mut self, f: TickFn, bank: &SoundBank) {
        let t = &bank.tables;
        match f {
            TickFn::VolUp => self.vol_up(),
            TickFn::VolDown => self.vol_down(),
            TickFn::PitchDown => {
                let p = self.mul_down(t);
                self.set_pitch(p)
            }
            TickFn::PitchUp => {
                let p = self.mul_up(t);
                self.set_pitch(p)
            }
            TickFn::PortaDown => {
                let p = self.mul_down(t);
                self.set_pitch(p);
                if p < self.target {
                    let tg = self.target;
                    self.set_pitch(tg);
                    self.mode &= !8;
                    self.mode &= !3;
                }
            }
            TickFn::PortaUp => {
                let p = self.mul_up(t);
                self.set_pitch(p);
                if p > self.target {
                    let tg = self.target;
                    self.set_pitch(tg);
                    self.mode &= !8;
                    self.mode &= !3;
                }
            }
            TickFn::Vibrato => self.vibrato(t),
            TickFn::VibVolDown => {
                let v = self.vol.wrapping_sub(self.slide) as i8;
                self.vol = v as u8;
                if v <= 0 {
                    self.vol = 0;
                    self.tick = Some(TickFn::Vibrato);
                }
                self.v.volume = self.vol;
                self.v.flags |= 0x10;
                self.vibrato(t);
            }
            TickFn::VibVolUp => {
                let v = Self::s8add(self.vol, self.slide);
                self.vol = v as u8;
                if v >= 0x40 {
                    self.vol = 0x40;
                    self.tick = Some(TickFn::Vibrato);
                }
                self.v.volume = self.vol;
                self.v.flags |= 0x10;
                self.vibrato(t);
            }
            TickFn::Retrig => {
                self.counter = self.counter.wrapping_sub(1);
                if self.counter == 0 {
                    self.v.flags |= 5;
                    self.counter = self.retrig_mem & 0xF;
                    self.mode |= 4;
                }
            }
            TickFn::Delay => {
                self.counter = self.counter.wrapping_sub(1);
                if self.counter == 0 {
                    self.trigger(bank);
                }
            }
            TickFn::PortaUpVolUp | TickFn::PortaDownVolUp | TickFn::PortaUpVolDown | TickFn::PortaDownVolDown => {
                let up = matches!(f, TickFn::PortaUpVolUp | TickFn::PortaUpVolDown);
                let p = if up { self.mul_up(t) } else { self.mul_down(t) };
                self.set_pitch(p);
                // the comparison each combined handler makes (0x0803ECE8 / ED4C / EDDE / EE42)
                let cont = match f {
                    TickFn::PortaUpVolUp => p >= self.target, // game bug: inverted
                    TickFn::PortaDownVolUp => p >= self.target,
                    TickFn::PortaUpVolDown => p <= self.target,
                    _ => p >= self.target,
                };
                let vol_up = matches!(f, TickFn::PortaUpVolUp | TickFn::PortaDownVolUp);
                if !cont {
                    let tg = self.target;
                    self.set_pitch(tg);
                    self.mode &= !8;
                    if vol_up {
                        self.tick = Some(TickFn::VolUp);
                        self.vol_up();
                    } else {
                        self.tick = Some(TickFn::VolDown);
                        self.vol_down();
                    }
                } else {
                    if vol_up {
                        let v = Self::s8add(self.vol, self.slide);
                        self.vol = v as u8;
                        if v >= 0x40 {
                            self.vol = 0x40;
                            self.tick = Some(TickFn::PortaDown);
                        }
                    } else {
                        let v = self.vol.wrapping_sub(self.slide) as i8;
                        self.vol = v as u8;
                        if v <= 0 {
                            self.vol = 0;
                            self.tick = Some(TickFn::PortaDown);
                        }
                    }
                    self.v.volume = self.vol;
                    self.v.flags |= 0x10;
                }
            }
        }
    }

    /// `0x0803E4E4`: start the cell's note (or stop on note 0xFF), or just set the volume.
    fn trigger(&mut self, bank: &SoundBank) {
        let f = self.cflags;
        if f & flag::NOTE != 0 {
            if self.note == 0xFF {
                self.mode &= !4;
                self.v.flags &= !1;
                return;
            }
            self.vol = self.cvol;
            let p = bank.tables.pitch[self.note as usize];
            self.base_pitch = p;
            self.mode = 4;
            if f & flag::KEEP_SAMPLE != 0 {
                self.v.pitch = p;
                self.v.volume = self.vol;
                self.v.flags |= 0x17;
            } else {
                self.v.entry = EntryRef::Instrument(self.inst);
                self.v.pitch = p;
                self.v.volume = self.vol;
                self.v.flags |= 0x1F;
            }
        } else if f & flag::VOLUME != 0 {
            self.vol = self.cvol;
            self.v.volume = self.cvol;
            self.v.flags |= 0x10;
        }
    }

    /// volume-slide parameter decoding shared by 0x0803E608 / E9A8 / EA6C. Returns which continuous slide
    /// was selected (1 up, 2 down) or 0 (fine slide applied now / none) and whether it matched any form.
    fn parse_volslide(&mut self) -> (u8, bool) {
        let v = self.volslide_mem;
        if v & 0xF == 0 {
            self.slide = v >> 4;
            if self.slide == 0xF {
                self.vol_up();
            }
            (1, true)
        } else if v >> 4 == 0 {
            self.slide = v;
            if self.slide == 0xF {
                self.vol_down();
            }
            (2, true)
        } else if v & 0xF == 0xF {
            self.slide = v >> 4;
            self.vol_up();
            (0, true)
        } else if v >> 4 == 0xF {
            self.slide = v & 0xF;
            self.vol_down();
            (0, true)
        } else {
            (0, false)
        }
    }
}

impl<'a> Driver<'a> {
    pub fn new(bank: &'a SoundBank) -> Driver<'a> {
        let mut tracks = vec![Track::default(); N_TRACKS];
        for t in tracks.iter_mut() {
            t.v.master = MUSIC_MASTER;
            t.v.is_track = true;
        }
        let voices = (0..N_VOICES).map(|_| Voice { master: SFX_MASTER, ..Default::default() }).collect();
        Driver {
            bank,
            player: Player { tracks, ..Default::default() },
            voices,
            mixbuf: Vec::new(),
            clock: 0,
            rows_log: Vec::new(),
            log_rows: false,
            mute: 0,
            scratch: Vec::new(),
        }
    }

    /// Validation helper: load a music track's persistent state from a dump of the game's 0x60-byte track RAM.
    /// The driver never clears effect memories, vibrato phase, pans or the cell buffer between songs, so the
    /// exact output of a song in the game depends on what played before; a fresh driver starts from zeros.
    pub fn seed_track_ram(&mut self, ti: usize, ram: &[u8]) {
        let bank = self.bank;
        let u32_ = |o: usize| u32::from_le_bytes([ram[o], ram[o + 1], ram[o + 2], ram[o + 3]]);
        let t = &mut self.player.tracks[ti];
        let v = &mut t.v;
        v.pos = u32_(0) as i32;
        v.step = u32_(4) as i32;
        v.gain = u32_(0xC);
        v.loop_mode = ram[0x11];
        v.volume = ram[0x12];
        v.pan = ram[0x13] as i8;
        v.rate = u16::from_le_bytes([ram[0x14], ram[0x15]]);
        v.step_mag = u32_(0x1C) as i32;
        v.loop_start = u32_(0x20) as i32;
        v.end = u32_(0x24) as i32;
        let e = u32_(0x28);
        v.entry = bank
            .instruments
            .iter()
            .position(|x| x.rom == e)
            .map(|i| EntryRef::Instrument(i as u8))
            .or_else(|| bank.sfx.iter().position(|x| x.rom == e).map(|i| EntryRef::Sfx(i as u8)))
            .unwrap_or(EntryRef::None);
        if let EntryRef::Instrument(i) = v.entry {
            v.sample = Some(bank.instruments[i as usize].sample);
        }
        v.pitch = u32_(0x30);
        v.master = ram[0x34];
        t.base_pitch = u32_(0x40);
        t.target = u32_(0x44);
        t.vol = ram[0x48];
        t.pan_mem = ram[0x49];
        t.mode = ram[0x4A];
        t.slide = ram[0x4B];
        t.volslide_mem = ram[0x4C];
        t.pitch_mem = ram[0x4D];
        t.retrig_mem = ram[0x4E];
        t.vib_speed = ram[0x4F];
        t.vib_depth = ram[0x50];
        t.vib_phase = ram[0x51];
        t.counter = ram[0x52];
        t.tick = match u32_(0x54) & !1 {
            0x0803_E5DA => Some(TickFn::VolUp),
            0x0803_E5AC => Some(TickFn::VolDown),
            0x0803_E68C => Some(TickFn::PitchDown),
            0x0803_E71A => Some(TickFn::PitchUp),
            0x0803_E7C4 => Some(TickFn::PortaDown),
            0x0803_E806 => Some(TickFn::PortaUp),
            0x0803_E8BC => Some(TickFn::Vibrato),
            0x0803_E954 => Some(TickFn::VibVolDown),
            0x0803_E97E => Some(TickFn::VibVolUp),
            0x0803_EB98 => Some(TickFn::Retrig),
            0x0803_EBF8 => Some(TickFn::Delay),
            0x0803_ECC6 => Some(TickFn::PortaUpVolUp),
            0x0803_ED2A => Some(TickFn::PortaDownVolUp),
            0x0803_EDBC => Some(TickFn::PortaUpVolDown),
            0x0803_EE20 => Some(TickFn::PortaDownVolDown),
            _ => None,
        };
        t.cflags = ram[0x58];
        t.note = ram[0x5A];
        t.inst = ram[0x5B];
        t.cvol = ram[0x5C];
        t.fx = ram[0x5D];
        t.param = ram[0x5E];
    }

    /// Options-menu volumes (`0x0803F18C` / `0x0803E42A`: master byte of every track / SFX voice; flags a
    /// volume update like the game).
    pub fn set_master_volumes(&mut self, music: u8, sfx: u8) {
        for t in self.player.tracks.iter_mut() {
            t.v.master = music;
            t.v.flags |= 0x10;
        }
        for v in self.voices.iter_mut() {
            v.master = sfx;
            v.flags |= 0x10;
        }
    }

    /// Set the initial pan of every music track (the driver never resets pans between songs; after a fresh
    /// boot they are 0).
    pub fn set_track_pans(&mut self, pans: &[i8]) {
        for (t, &p) in self.player.tracks.iter_mut().zip(pans) {
            t.v.pan = p;
            t.pan_mem = p as u8;
        }
    }

    /// `0x0803F110` PlaySong (also silences the tracks like `0x0803F0E6`).
    pub fn play_song(&mut self, id: usize) {
        let p = &mut self.player;
        p.rows = 1;
        p.ticks_left = 1;
        p.row = 0;
        p.song = Some(id);
        p.speed = self.bank.songs[id].speed;
        p.order = -1;
        for t in p.tracks.iter_mut() {
            t.mode = 0;
            t.cflags = 0;
            t.v.flags = 0;
        }
        p.tempo = self.bank.songs[id].tempo;
        p.samples_per_tick = if p.tempo != 0 { (40000 / p.tempo as u32) as u16 } else { 0 };
        p.playing = true;
    }

    pub fn stop_song(&mut self) {
        self.player.playing = false;
        for t in self.player.tracks.iter_mut() {
            t.mode = 0;
            t.cflags = 0;
            t.v.flags = 0;
        }
    }

    /// `0x0803E378` PlaySfx: first free voice, else dropped (returns false).
    pub fn play_sfx(&mut self, id: u8, volume: u8, pan: i8) -> bool {
        for v in self.voices.iter_mut() {
            if v.flags & 1 == 0 {
                v.entry = EntryRef::Sfx(id);
                v.pitch = 0x4000;
                v.volume = volume;
                v.pan = pan;
                v.flags = 0x1F;
                return true;
            }
        }
        false
    }

    /// `0x0803E302` StopSfx: the highest voice playing `id`.
    pub fn stop_sfx(&mut self, id: u8) {
        if let Some(v) = self.voices.iter_mut().rev().find(|v| v.flags & 1 != 0 && v.entry == EntryRef::Sfx(id)) {
            v.flags &= !1;
        }
    }

    pub fn stop_all_sfx(&mut self) {
        for v in self.voices.iter_mut() {
            v.flags &= !1;
        }
    }

    pub fn busy_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.flags & 1 != 0).count()
    }

    /// `0x0803EEE4`: advance to the next row (and order when the pattern ends), load the row's cells.
    fn next_row(&mut self) {
        let bank = self.bank;
        let p = &mut self.player;
        let song: &Song = &bank.songs[p.song.unwrap()];
        p.row = p.row.wrapping_add(1);
        if p.row >= p.rows {
            p.order += 1;
            if p.order < 0 || p.order as usize >= song.orders.len() {
                p.order = 0;
            }
            p.pattern = song.orders[p.order as usize] as usize;
            p.row = 0;
            p.row_in_pattern = 0;
            p.rows = song.patterns[p.pattern].rows.len() as u8;
        }
        for t in p.tracks.iter_mut() {
            t.cflags = 0;
        }
        let pat = &song.patterns[p.pattern];
        let cells: &[Cell] = pat.rows.get(p.row_in_pattern).map(|r| r.as_slice()).unwrap_or(&[]);
        p.row_in_pattern += 1;
        for c in cells {
            let Some(t) = p.tracks.get_mut(c.channel as usize) else { continue };
            t.cflags = c.flags;
            if c.flags & 1 != 0 {
                t.note = c.note;
            }
            if c.flags & 2 != 0 {
                t.inst = c.instrument;
            }
            if c.flags & 4 != 0 {
                t.cvol = c.volume;
            }
            if c.flags & 8 != 0 {
                t.fx = c.effect;
                t.param = c.param;
            }
        }
        if self.log_rows {
            self.rows_log.push(RowEvent { sample: self.clock, order: p.order, row: p.row });
        }
    }

    /// `0x0803EE84` + the row effect handlers.
    fn row_effects(&mut self, ti: usize) {
        let bank = self.bank;
        let t = &mut self.player.tracks[ti];
        t.mode &= !3;
        if t.mode & 4 != 0 && t.base_pitch != t.v.pitch {
            t.v.pitch = t.base_pitch;
            t.v.flags |= 2;
        }
        if t.cflags == 0 {
            return;
        }
        let id = if t.cflags & flag::EFFECT != 0 { t.fx } else { 0 };
        let kind = bank.tables.effects.get(id as usize).copied().unwrap_or(EffectKind::None);
        let tb = &bank.tables;
        use EffectKind as E;
        match kind {
            E::None | E::Unknown(_) => t.trigger(bank),
            E::SetSpeed | E::PatternJump | E::SetTempo => {
                t.trigger(bank);
                let prm = t.param;
                let p = &mut self.player;
                match kind {
                    E::SetSpeed => p.speed = prm,
                    E::PatternJump => {
                        p.row = p.rows.wrapping_sub(1);
                        p.order = prm as i32 - 1;
                    }
                    _ => {
                        p.tempo = prm;
                        p.samples_per_tick = if prm != 0 { (40000 / prm as u32) as u16 } else { 0 };
                    }
                }
            }
            E::VolumeSlide => {
                t.trigger(bank);
                if t.param != 0 {
                    t.volslide_mem = t.param;
                }
                let v = t.volslide_mem;
                if v & 0xF == 0 {
                    t.slide = v >> 4;
                    t.tick = Some(TickFn::VolUp);
                    t.mode |= 1;
                    if t.slide == 0xF {
                        t.vol_up();
                    }
                } else if (v >> 4) == 0 {
                    t.slide = v;
                    t.tick = Some(TickFn::VolDown);
                    t.mode |= 1;
                    if t.slide == 0xF {
                        t.vol_down();
                    }
                } else if v & 0xF == 0xF {
                    t.slide = v >> 4;
                    t.vol_up();
                } else if v >> 4 == 0xF {
                    t.slide = v & 0xF;
                    t.vol_down();
                }
            }
            E::PitchSlideDown | E::PitchSlideUp => {
                t.trigger(bank);
                if t.param != 0 {
                    t.pitch_mem = t.param;
                }
                if t.mode & 4 == 0 {
                    return;
                }
                let v = t.pitch_mem;
                let down = kind == E::PitchSlideDown;
                match v >> 4 {
                    0xF => {
                        let p = if down {
                            t.base_pitch.wrapping_mul(tb.slide_down[(v & 0xF) as usize] as u32) >> 16
                        } else {
                            t.base_pitch.wrapping_mul(tb.slide_up[(v & 0xF) as usize] as u32) >> 14
                        };
                        t.set_pitch(p);
                    }
                    0xE => {
                        let p = if down {
                            t.base_pitch.wrapping_mul(tb.slide_down_fine[(v & 0xF) as usize] as u32) >> 16
                        } else {
                            t.base_pitch.wrapping_mul(tb.slide_up_fine[(v & 0xF) as usize] as u32) >> 15
                        };
                        t.set_pitch(p);
                    }
                    _ => {
                        t.tick = Some(if down { TickFn::PitchDown } else { TickFn::PitchUp });
                        t.mode |= 1;
                    }
                }
            }
            E::TonePortamento => {
                if t.param != 0 {
                    t.pitch_mem = t.param;
                }
                if t.mode & 4 == 0 {
                    return;
                }
                let f = t.cflags;
                let mut set_fn = false;
                if f & flag::NOTE != 0 {
                    if t.note != 0xFF {
                        t.target = tb.pitch[t.note as usize];
                        t.mode |= 8;
                        set_fn = true;
                    }
                } else if t.mode & 8 != 0 {
                    set_fn = true;
                }
                if set_fn {
                    t.tick = Some(if t.target > t.base_pitch { TickFn::PortaUp } else { TickFn::PortaDown });
                    t.mode |= 1;
                }
                if f & flag::VOLUME != 0 {
                    t.vol = t.cvol;
                    t.v.volume = t.cvol;
                    t.v.flags |= 0x10;
                }
            }
            E::Vibrato | E::FineVibrato => {
                t.trigger(bank);
                if kind == E::Vibrato && t.mode & 4 == 0 {
                    return;
                }
                if t.cflags & flag::NOTE != 0 {
                    t.vib_phase = 0;
                }
                let v = t.param;
                if v & 0xF != 0 {
                    t.vib_depth = if kind == E::Vibrato { (v & 0xF) << 2 } else { v & 0xF };
                }
                if v >> 4 != 0 {
                    t.vib_speed = v >> 4;
                }
                t.tick = Some(TickFn::Vibrato);
                t.mode |= 1;
                t.vibrato(tb);
            }
            E::VibratoVolumeSlide => {
                t.trigger(bank);
                if t.param != 0 {
                    t.volslide_mem = t.param;
                }
                if t.mode & 4 == 0 {
                    return;
                }
                if t.cflags & flag::NOTE != 0 {
                    t.vib_phase = 0;
                }
                let v = t.volslide_mem;
                if v & 0xF == 0 {
                    t.slide = v >> 4;
                    t.tick = Some(TickFn::VibVolUp);
                    if t.slide == 0xF {
                        t.vol_up();
                    }
                } else if v >> 4 == 0 {
                    t.slide = v;
                    t.tick = Some(TickFn::VibVolDown);
                    if t.slide == 0xF {
                        t.vol_down();
                    }
                } else if v & 0xF == 0xF {
                    t.slide = v >> 4;
                    t.vol_up();
                    t.tick = Some(TickFn::Vibrato);
                } else if v >> 4 == 0xF {
                    t.slide = v & 0xF;
                    t.vol_down();
                    t.tick = Some(TickFn::Vibrato);
                }
                t.mode |= 1;
                t.vibrato(tb);
            }
            E::PortamentoVolumeSlide => {
                if t.param != 0 {
                    t.volslide_mem = t.param;
                }
                if t.mode & 4 == 0 {
                    return;
                }
                if t.cflags & flag::VOLUME != 0 {
                    t.vol = t.cvol;
                    t.v.volume = t.cvol;
                    t.v.flags |= 0x10;
                }
                let (r6, _) = t.parse_volslide();
                if t.cflags & flag::NOTE != 0 {
                    if t.note == 0xFF {
                        return;
                    }
                    t.target = tb.pitch[t.note as usize];
                    t.mode |= 8;
                } else if t.mode & 8 == 0 && r6 != 0 {
                    t.tick = Some(if r6 == 1 { TickFn::VolUp } else { TickFn::VolDown });
                    t.mode |= 1;
                }
                let up = t.target > t.base_pitch;
                t.tick = Some(match (up, r6) {
                    (true, 0) => TickFn::PortaUp,
                    (true, 1) => TickFn::PortaUpVolUp,
                    (true, _) => TickFn::PortaUpVolDown,
                    (false, 0) => TickFn::PortaDown,
                    (false, 1) => TickFn::PortaDownVolUp,
                    (false, _) => TickFn::PortaDownVolDown,
                });
                t.mode |= 1;
            }
            E::SampleOffset => {
                t.trigger(bank);
                if t.cflags & flag::NOTE != 0 {
                    t.v.pos = (t.param as i32) << 22;
                    t.v.flags &= !4;
                    t.v.loop_mode &= !4;
                }
            }
            E::Retrigger => {
                t.trigger(bank);
                if t.param != 0 {
                    t.retrig_mem = t.param;
                }
                t.tick = Some(TickFn::Retrig);
                t.counter = t.retrig_mem & 0xF;
                t.mode = (t.mode & !3) + 2;
            }
            E::SetPan => {
                t.trigger(bank);
                t.pan_mem = t.param;
                t.v.pan = t.param as i8;
                t.v.flags |= 0x10;
            }
            E::NoteDelay => {
                t.counter = t.param;
                t.tick = Some(TickFn::Delay);
                t.mode = (t.mode & !3) + 2;
            }
        }
    }

    /// `0x0803EFD0`: one sequencer tick.
    pub(crate) fn tick(&mut self) {
        let bank = self.bank;
        self.player.ticks_left = self.player.ticks_left.wrapping_sub(1);
        if self.player.ticks_left == 0 {
            self.next_row();
            for ti in 0..N_TRACKS {
                self.row_effects(ti);
            }
            self.player.ticks_left = self.player.speed;
        } else {
            for t in self.player.tracks.iter_mut() {
                let m = t.mode & 3;
                if m == 0 || (m == 1 && t.mode & 4 == 0) {
                    continue;
                }
                if let Some(f) = t.tick {
                    t.run_tick(f, bank);
                }
            }
        }
        for t in self.player.tracks.iter_mut() {
            t.v.update(bank);
        }
    }

    pub(crate) fn mix_tracks(&mut self, off: usize, n: usize) {
        let bank = self.bank;
        if self.scratch.len() < self.mixbuf.len() {
            self.scratch.resize(self.mixbuf.len(), 0);
        }
        for (i, t) in self.player.tracks.iter_mut().enumerate() {
            let buf = if self.mute >> i & 1 != 0 { &mut self.scratch[off..] } else { &mut self.mixbuf[off..] };
            if t.v.mix(bank, buf, n) && t.v.is_track {
                t.mode &= !4;
            }
        }
    }

    /// Walk the song's row sequence without mixing (sequencer only) and find where it starts repeating.
    /// The state at a row start that determines all following rows is (order, row, speed, samples per tick);
    /// returns the row log and, if found within `max_rows`, the index into it of the loop start and of its
    /// repetition.
    pub fn song_structure(bank: &SoundBank, id: usize, max_rows: usize) -> (Vec<RowEvent>, Option<(usize, usize)>) {
        let mut d = Driver::new(bank);
        d.log_rows = true;
        d.play_song(id);
        let mut seen: std::collections::HashMap<(i32, u8, u8, u16), usize> = std::collections::HashMap::new();
        while d.rows_log.len() < max_rows {
            let before = d.rows_log.len();
            // speed / tempo in effect when the row starts (before its own effects change them)
            let (sp, spt) = (d.player.speed, d.player.samples_per_tick);
            d.tick();
            if d.rows_log.len() > before {
                let e = d.rows_log[before];
                let key = (e.order, e.row, sp, spt);
                if let Some(&j) = seen.get(&key) {
                    return (d.rows_log, Some((j, before)));
                }
                seen.insert(key, before);
            }
            d.clock += d.player.samples_per_tick as u64;
        }
        (d.rows_log, None)
    }

    /// `0x0803F07C`: music for `n` samples of the mix buffer, ticking at the exact sample positions.
    fn mix_music(&mut self, n: usize) {
        if !self.player.playing {
            return;
        }
        let start = self.clock;
        let mut off = 0usize;
        let mut left = n;
        loop {
            let cd = self.player.countdown as usize;
            if cd >= left {
                self.mix_tracks(off, left);
                self.player.countdown -= left as u16;
                break;
            }
            if cd != 0 {
                self.mix_tracks(off, cd);
                off += cd;
                left -= cd;
            }
            self.clock = start + off as u64; // for the row log
            self.tick();
            self.player.countdown = self.player.samples_per_tick;
        }
        self.clock = start;
    }

    /// Mix `n` samples (one driver update). SFX voice state changes (`0x0803E3F6`) are applied first, like the
    /// per-frame update `0x0803F296`; music ticks happen at their exact sample positions.
    pub fn render_into(&mut self, out: &mut Vec<(i8, i8)>, n: usize) {
        let bank = self.bank;
        for v in self.voices.iter_mut() {
            v.update(bank);
        }
        self.mixbuf.clear();
        self.mixbuf.resize(n, 0);
        self.mix_music(n);
        for v in self.voices.iter_mut() {
            v.mix(bank, &mut self.mixbuf, n);
        }
        for &w in &self.mixbuf {
            let l = ((w as u16 as i16) >> 6).clamp(-128, 127) as i8;
            let r = ((w as i32) >> 22).clamp(-128, 127) as i8;
            out.push((l, r));
        }
        self.clock += n as u64;
    }


    /// Render `n` samples of the 8-bit hardware output in driver updates of `chunk` samples (the game mixes
    /// once per frame, ~267 samples; music is chunk-independent, SFX start on chunk boundaries).
    pub fn render(&mut self, n: usize, chunk: usize) -> Vec<(i8, i8)> {
        let mut out = Vec::with_capacity(n);
        let mut left = n;
        while left > 0 {
            let k = left.min(chunk.max(1));
            self.render_into(&mut out, k);
            left -= k;
        }
        out
    }
}
