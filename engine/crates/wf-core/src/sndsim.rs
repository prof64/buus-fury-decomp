//! The sound driver as part of the deterministic world, and the CPU-time model that hangs on it.
//!
//! The game's status page redraw (`0x080154D4`) holds the main loop for ~3.8 video frames; whether the loop makes
//! the fourth VBlank (4 frames) or misses it (5) depends on how long the sound mixer ran meanwhile, i.e. on the
//! music and the sound effects playing (`engine/docs/segments.md` §9 "Redraw timing"). So the driver's state
//! lives here, advanced every frame with the same code `wf-audio` plays with, whether or not anything is heard:
//!
//! * `pos` / `phase`: the hardware side. The FIFO DMA's completion interrupt (`0x03000AE0`) advances the
//!   driver's play position by 16 samples every 16,800 CPU cycles; a frame is 280,896 cycles.
//! * a normal frame: the main loop's update (`0x0803F296`) mixes the samples played since the last update.
//! * a long iteration: when the play position gets 928 samples ahead the interrupt itself mixes them; the main
//!   loop's update follows when the iteration ends.
//!
//! [`redraw_frames`] is a bounded-error predictor, not a cycle-exact one (ROM prefetch and DMA cycle stealing
//! are not modelled): its error against the emulator is ~0.45k cycles std, 1.5k max, on an iteration of ~1.12M
//! cycles; a decision closer than [`NEAR`] to the cutoff is counted in [`SoundSim::near`] (and logged with
//! `WF_SND_DEBUG`). Without sound content (`GameData.sound` = None) the pages keep their constant costs.

use crate::world::{Event, World};
use serde::{Deserialize, Serialize};
use wf_audio::driver::{Driver, DriverState};
use wf_view::MenuOp;

/// CPU cycles per video frame (228 lines x 1232)
pub const FRAME: i64 = 280_896;
/// cycles per DMA completion interrupt (16 samples x 1050 cycles)
pub const DMA: i64 = 16_800;
/// the interrupt mixes when the play position is this far ahead of the mixed one
pub const IRQ_MIX: u32 = 928;
/// frame time of the main loop's update in a menu frame without a redraw / in a field frame (the exact field
/// value depends on the frame's work; it only moves a chunk boundary by 16 samples)
const UPDATE_AT_MENU: i64 = 4_766;
const UPDATE_AT_FIELD: i64 = 95_000;
/// a decision within this many cycles of the cutoff is flagged
pub const NEAR: i64 = 1_500;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SoundSim {
    /// the driver (None until the first frame with sound content)
    pub drv: Option<DriverState>,
    /// play position (samples) at the frame's start, position of the last update
    pub pos: u32,
    pub last: u32,
    /// cycles from the frame's start (VBlank) to its first DMA interrupt
    pub phase: u32,
    /// frames whose main update is already accounted for (a long iteration in progress)
    pub skip: u8,
    /// events of this frame already given to the driver
    pub seen: u32,
    /// redraw decisions closer than [`NEAR`] to the cutoff so far
    pub near: u32,
    /// the state came from a game RAM image without the hardware's timer phase (an emulator state imported
    /// without `snd_phase`): the driver still runs, the pages keep their constant redraw costs
    pub uncalibrated: bool,
}

/// the redraw of a page for the time model: the cycles before it in its frame and its own without the mixer
#[derive(Clone, Copy, Debug)]
pub struct Redraw {
    pub t0: i64,
    pub base: i64,
}

pub fn debug() -> bool {
    use std::sync::OnceLock;
    static D: OnceLock<bool> = OnceLock::new();
    *D.get_or_init(|| std::env::var("WF_SND_DEBUG").is_ok())
}

/// debug: the voices before an update, as the game's RAM has them (pos, step, gain, flags; tracks then voices)
fn dump(what: &str, n: u32, d: &Driver) {
    let st = d.state();
    let v: Vec<String> = st
        .player
        .tracks
        .iter()
        .map(|t| &t.v)
        .chain(st.voices.iter())
        .map(|v| format!("{},{},{},{}", v.pos, v.step, v.gain, v.flags))
        .collect();
    eprintln!("snd {what} n {n} row {} order {} | {}", st.player.row, st.player.order, v.join(" "));
}

/// play position at frame time `t` (interrupts at phase, phase + DMA, ...)
fn pos_at(s: &SoundSim, t: i64) -> u32 {
    let p = s.phase as i64;
    if t < p {
        s.pos
    } else {
        s.pos.wrapping_add(16 * ((t - p) / DMA + 1) as u32)
    }
}

fn apply_events(w: &mut World, d: &mut Driver) {
    let from = (w.sound.seen as usize).min(w.events.len());
    for e in &w.events[from..] {
        match e {
            Event::Sfx { id, vol } => {
                d.play_sfx(*id as u8, (*vol).clamp(0, 64) as u8, 0);
            }
            Event::SfxStop { id } => d.stop_sfx(*id as u8),
            Event::SfxStopMatch { id } => d.stop_matching_sfx(*id as u8),
            Event::SfxStopAll => d.stop_all_sfx(),
            Event::Song { id } => d.play_song(*id as usize),
            Event::SongStop => d.stop_song(),
            Event::MasterVolumes { music, sfx } => d.set_master_volumes(*music as u8, *sfx as u8),
            _ => {}
        }
    }
    w.sound.seen = w.events.len() as u32;
}

/// run `f` on the world's driver (None without sound content)
fn with_driver<R>(w: &mut World, f: impl FnOnce(&mut World, &mut Driver) -> R) -> Option<R> {
    let bank = w.data.sound.clone()?;
    let mut d = match w.sound.drv.take() {
        Some(st) => Driver::from_state(&bank, st),
        None => Driver::new(&bank),
    };
    let r = f(w, &mut d);
    w.sound.drv = Some(d.state());
    Some(r)
}

/// The end of a frame: the frame's sound events, the main loop's update, the hardware's frame.
pub fn end_frame(w: &mut World) {
    let menu = crate::menu::active(w);
    with_driver(w, |w, d| {
        apply_events(w, d);
        if w.sound.skip > 0 {
            w.sound.skip -= 1;
        } else {
            let at = if menu { UPDATE_AT_MENU } else { UPDATE_AT_FIELD };
            let p = pos_at(&w.sound, at);
            let n = p.wrapping_sub(w.sound.last) & 1023;
            if debug() {
                dump("upd", n, d);
            }
            if n >= 64 {
                wf_audio::cost::update(d, n);
                w.sound.last = p;
            }
        }
    });
    w.sound.seen = 0;
    hw_frame(&mut w.sound);
}

/// the hardware's frame: its DMA interrupts
fn hw_frame(s: &mut SoundSim) {
    let p = s.phase as i64;
    let k = (FRAME - p + DMA - 1) / DMA;
    s.pos = s.pos.wrapping_add(16 * k as u32);
    s.phase = (p + k * DMA - FRAME) as u32;
}

/// A video frame the main loop does not finish an iteration in (the game's own lag: loads, long scripts; a host
/// that replays a recording calls this for them): no update, the interrupt mixes when the play position runs
/// [`IRQ_MIX`] ahead.
pub fn lag_frame(w: &mut World) {
    with_driver(w, |w, d| {
        let s = &mut w.sound;
        if s.skip > 0 {
            return;
        }
        let ahead = s.pos.wrapping_sub(s.last) & 1023;
        let at_end = ahead + 16 * ((FRAME - s.phase as i64 + DMA - 1) / DMA) as u32;
        if ahead < IRQ_MIX && at_end >= IRQ_MIX {
            if debug() {
                dump("irq", IRQ_MIX, d);
            }
            wf_audio::cost::update(d, IRQ_MIX);
            s.last = s.last.wrapping_add(IRQ_MIX);
        }
    });
    if w.sound.skip > 0 {
        w.sound.skip -= 1;
    }
    hw_frame(&mut w.sound);
}

/// A main-loop iteration with a redraw starting now: the interrupt's mix inside it, the update after it; returns
/// the video frames it takes (None without sound content: the caller keeps its constant).
pub fn redraw_frames(w: &mut World, r: Redraw) -> Option<u8> {
    if w.sound.uncalibrated {
        return None;
    }
    let frame = w.frame;
    with_driver(w, |w, d| {
        apply_events(w, d);
        let s = &mut w.sound;
        let phase = s.phase as i64;
        // the DMA interrupts before the redraw delay it (~187 cycles each)
        let r = Redraw { t0: r.t0 + if phase < r.t0 { 187 * ((r.t0 - phase) / DMA + 1) } else { 0 }, ..r };
        // every interrupt after which the play position is IRQ_MIX ahead mixes (one in a redraw, several in a
        // reload); each mix lengthens the iteration by its cycles and the interrupts' entry / exit around it
        let mut t1 = r.t0 + r.base;
        let mut irq = (0i64, 0i64);
        let ahead = (s.pos.wrapping_sub(s.last) & 1023) as i64;
        for j in 1..200i64 {
            // the j-th mix: the interrupt after which the position is j x IRQ_MIX past the last update's
            let need = IRQ_MIX as i64 * j - ahead;
            if need < 0 {
                break;
            }
            let k = (need + 15) / 16;
            // k == 0: already there at the frame's start (the previous iteration ran long)
            let t = if k == 0 { 0 } else { phase + (k - 1) * DMA };
            if t >= t1 {
                break;
            }
            if debug() {
                dump("irq", IRQ_MIX, d);
            }
            let c = wf_audio::cost::update(d, IRQ_MIX) * 10_065 / 10_000;
            s.last = s.last.wrapping_add(IRQ_MIX);
            t1 += c;
            irq = (t, irq.1 + c);
        }
        let t = t1 + 37;
        let p = pos_at(s, t);
        let n = p.wrapping_sub(s.last) & 1023;
        let mut post = 0;
        if n >= 64 {
            post = wf_audio::cost::update(d, n);
            s.last = p;
        }
        let end = t + post;
        // the iteration ends before the VBlank it then waits for
        let frames = ((end - CUTOFF).div_euclid(FRAME) + 1).clamp(1, 60);
        let margin = frames * FRAME + CUTOFF - end;
        let near = margin.min(FRAME - margin) < NEAR;
        if near {
            s.near += 1;
        }
        if debug() {
            eprintln!(
                "snd redraw f{frame}: t0 {} base {} irq at {} cost {} post n {} cost {} end {} -> {} frames, margin {}{}",
                r.t0,
                r.base,
                irq.0,
                irq.1,
                n,
                post,
                end,
                frames,
                margin,
                if near { " NEAR" } else { "" }
            );
        }
        s.skip = frames as u8;
        frames as u8
    })
}

/// The pause menu's close: the world is rebuilt in one main-loop iteration - the map loader's decompression
/// (`MapDef.reload_cycles`, +0.35 % for the sound DMA's stolen cycles), ~267 k cycles of everything else, and an
/// interrupt mix every 928 samples. Returns the video frames the iteration takes (None: unknown map / no sound
/// content / uncalibrated). Measured: 2-9 22 frames (R 252 k), 3-15 26 (R 281 k); the object-dependent part of R
/// is not modelled (+-15 k cycles).
pub fn reload_frames(w: &mut World) -> Option<u8> {
    let f = w.field.as_ref()?;
    let c = w.data.field.as_ref()?.maps.get(&f.map)?.reload_cycles as i64;
    if c == 0 {
        return None;
    }
    redraw_frames(w, Redraw { t0: 0, base: RELOAD_REST + c * 10_035 / 10_000 })
}

const RELOAD_REST: i64 = 266_800;

/// the iteration makes the VBlank when it ends this many cycles before it at the latest (the wait's own entry)
const CUTOFF: i64 = 0;

/// A long iteration of `frames` video frames whose length is not modelled (other pages' redraws): the driver
/// goes through the same interrupt mix and catch-up update.
pub fn long_iteration(w: &mut World, frames: u8) {
    if frames < 2 {
        return;
    }
    with_driver(w, |w, d| {
        apply_events(w, d);
        let s = &mut w.sound;
        let end = frames as i64 * FRAME - 20_000;
        let ahead = s.pos.wrapping_sub(s.last) & 1023;
        if ahead <= IRQ_MIX {
            let k = ((IRQ_MIX - ahead + 15) / 16) as i64;
            let t = if k == 0 { 0 } else { s.phase as i64 + (k - 1) * DMA };
            if t < end {
                wf_audio::cost::update(d, IRQ_MIX);
                s.last = s.last.wrapping_add(IRQ_MIX);
            }
        }
        let p = pos_at(s, end);
        let n = p.wrapping_sub(s.last) & 1023;
        if n >= 64 {
            wf_audio::cost::update(d, n);
            s.last = p;
        }
        s.skip = frames;
    });
}

/// instructions the shadowed glyph routine `0x0803F5A4` runs for one glyph (columns: bit 0 = top row), counted
/// like the measurements this model was fitted on (pc in 0x0803F61C..0x0803F700)
pub fn glyph_instructions(cols: &[u16]) -> i64 {
    let w = cols.len();
    // setup 24, the column copy 13 per column, 27 before the loop, 6 after
    let mut n = 24 + 13 * w as i64 + 27 + 6;
    for k in 0..=w {
        let g = if k < w { cols[k] as u32 } else { 0 };
        let sh = if k > 0 { (cols[k - 1] as u32) << 1 } else { 0 };
        n += 26;
        let (mut g, mut sh) = (g, sh);
        while g | sh != 0 {
            n += if g & 1 != 0 {
                12
            } else if sh & 1 != 0 {
                13
            } else {
                11
            };
            g >>= 1;
            sh >>= 1;
        }
    }
    n
}

/// instructions of the plain glyph routine's column loop (`0x0803F500..0x0803F532`) for one glyph
pub fn plain_glyph_instructions(cols: &[u16]) -> i64 {
    let mut n = 0;
    for &c in cols {
        n += 17;
        let mut c = c as u32;
        while c != 0 {
            n += if c & 1 != 0 { 9 } else { 7 };
            c >>= 1;
        }
    }
    n + 6
}

/// What a page's draw list costs beyond the page's constant part: glyph-routine instructions of the shadowed
/// text, the 16 x 16 pictures (the cursor, the prompts), and the cycles of the plain text lines, fills and pixels
/// (the tabs: one per party member). Per-unit costs from emulator profiles (`work/earth/timing/prof.py`): a plain
/// text line 990 cycles + 315 per glyph + 3.227 per column-loop instruction, a fill row 54 + 4.25 per pixel, a
/// pixel 45.
pub fn ops_cost(t: &crate::menu::text::TextData, ops: &[MenuOp]) -> (i64, i64, i64) {
    let mut gn = 0;
    let mut small = 0;
    let mut extra = 0;
    for op in ops {
        match op {
            MenuOp::Text { font, shadow, text, .. } => {
                let fm = t.font(*font);
                if shadow.is_none() {
                    extra += 990;
                }
                let mut k = 0;
                while k < text.len() {
                    let c = text[k];
                    if c == 8 && k + 1 < text.len() {
                        k += 2;
                        continue;
                    }
                    k += 1;
                    if c < 0xFF && fm.has.get(c as usize).copied().unwrap_or(false) {
                        let cols = fm.cols.get(c as usize).map(|v| v.as_slice()).unwrap_or(&[]);
                        if shadow.is_some() {
                            gn += glyph_instructions(cols);
                        } else {
                            extra += 315 + plain_glyph_instructions(cols) * 3_227 / 1000;
                        }
                    }
                }
            }
            MenuOp::Image { fw: 16, fh: 16, .. } => small += 1,
            MenuOp::Fill { x0, y0, x1, y1, .. } => extra += (y1 - y0).max(0) as i64 * (54 + (x1 - x0).max(0) as i64 * 425 / 100),
            MenuOp::Pixel { .. } => extra += 45,
            _ => {}
        }
    }
    (gn, small, extra)
}

/// Cycles of a draw list for the pages whose constant part is measured per page (inventory, journal, ...): the
/// shadowed glyphs (3.5664 per glyph-routine instruction), the pictures (16 x 16: 7,284, the keyed cursor 6,596,
/// others 28.45 per pixel: the blit `0x0803F922`), and the plain text / fills / pixels of [`ops_cost`]. The
/// backgrounds, panels and the surface copies are part of the page's constant.
pub fn layer_cycles(t: &crate::menu::text::TextData, ops: &[MenuOp]) -> i64 {
    let (gn, small, extra) = ops_cost(t, ops);
    let mut c = gn * 35_664 / 10_000 + small * 7_284 + extra;
    for op in ops {
        if let MenuOp::Image { path, fw, fh, .. } = op {
            if (*fw, *fh) != (16, 16) {
                c += *fw as i64 * *fh as i64 * 2_845 / 100;
            } else if path.ends_with("cursor.png") {
                c -= 688;
            }
        }
    }
    c
}

/// debug: a draw list's operations other than shadowed text, compact
pub fn ops_summary(ops: &[MenuOp]) -> String {
    let mut out = Vec::new();
    let mut px = 0;
    for op in ops {
        match op {
            MenuOp::Text { shadow: Some(_), .. } => {}
            MenuOp::Pixel { .. } => px += 1,
            MenuOp::Text { font, text, .. } => out.push(format!("text[{}]{}", font.id(), String::from_utf16_lossy(text))),
            MenuOp::TextOutline { font, text, .. } => out.push(format!("outline[{}]{}", font.id(), String::from_utf16_lossy(text))),
            MenuOp::Fill { x0, y0, x1, y1, .. } => out.push(format!("fill{}x{}", x1 - x0, y1 - y0)),
            MenuOp::Panel { x0, y0, x1, y1 } => out.push(format!("panel{}x{}", x1 - x0, y1 - y0)),
            MenuOp::Background { w, rows, .. } => out.push(format!("bg{w}x{rows}")),
            MenuOp::Image { path, fw, fh, .. } => out.push(format!("img{fw}x{fh}:{}", path.rsplit('/').next().unwrap_or(""))),
            _ => out.push("other".into()),
        }
    }
    format!("{} px{px}", out.join(" "))
}

/// The driver from the game's RAM: `ram` = player header + tracks + voices (`0x030038A4`, 0x18 + 15 x 0x60 +
/// 16 x 0x40), the sound object's counters (`0x03003898` +4 / +8) and the cycles to the next DMA interrupt.
pub fn import(w: &mut World, ram: &[u8], pos: u32, last: u32, phase: Option<u32>) {
    let Some(bank) = w.data.sound.clone() else { return };
    let mut d = Driver::new(&bank);
    d.seed_ram(ram);
    w.sound = SoundSim { drv: Some(d.state()), pos, last, phase: phase.unwrap_or(0), uncalibrated: phase.is_none(), ..Default::default() };
}
