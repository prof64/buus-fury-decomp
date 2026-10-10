//! Higher-quality renderer: the exact sequencer and voice logic of [`Driver`] (same notes, ticks, effects,
//! loop points and volumes), but each voice is resampled with linear interpolation directly to the output rate
//! and mixed in floating point (no 8-bit packing, no `>> 6` truncation, no clipping before the final output).
//! The result is a cleaner version of the same music, not a bit-exact one (use [`Driver::render`] for that).

use crate::bank::SoundBank;
use crate::driver::{Driver, Voice, MIX_RATE};

/// Voice state at the start of a segment.
#[derive(Clone)]
struct Snap {
    pos: f64,
    step: f64,
    loop_mode: u8,
    ls: f64,
    end: f64,
    gl: f32,
    gr: f32,
    sample: usize,
}

fn snap(v: &Voice) -> Option<Snap> {
    if v.flags & 1 == 0 || v.gain == 0 {
        return None;
    }
    Some(Snap {
        pos: v.pos as f64,
        step: v.step as f64,
        loop_mode: v.loop_mode,
        ls: v.loop_start as f64,
        end: v.end as f64,
        gl: (v.gain & 0xFFFF) as u16 as i16 as f32,
        gr: (v.gain >> 16) as u16 as i16 as f32,
        sample: v.sample?,
    })
}

/// Sample value of a voice `dt` native samples after its snapshot (None once a one-shot has ended).
fn eval(s: &Snap, pcm: &[i8], dt: f64, linear: bool) -> Option<f32> {
    let mut p = s.pos + s.step * dt;
    let (ls, end) = (s.ls, s.end);
    if s.loop_mode & 1 == 0 {
        if p >= end || p < 0.0 {
            return None;
        }
    } else if end > ls {
        let len = end - ls;
        if s.loop_mode & 2 == 0 {
            if p >= end {
                p = ls + (p - ls).rem_euclid(len);
            }
        } else if p >= end || p < ls {
            let u = (p - ls).rem_euclid(2.0 * len);
            p = ls + if u > len { 2.0 * len - u } else { u };
        }
    }
    let x = p / 16384.0;
    let i = x.floor();
    let f = (x - i) as f32;
    let i = i as isize;
    let at = |k: isize| -> f32 {
        if k < 0 {
            return 0.0;
        }
        let mut k = k as usize;
        if s.loop_mode & 1 != 0 && k as f64 >= end / 16384.0 {
            k = (ls / 16384.0) as usize; // next sample after the loop end is the loop start
        }
        pcm.get(k).copied().unwrap_or(0) as f32
    };
    Some(if linear { at(i) * (1.0 - f) + at(i + 1) * f } else { at(i) })
}

/// Render `n_out` frames at `rate` Hz, interleaved stereo f32 in -1..1, advancing the driver exactly like
/// [`Driver::render`] (driver updates of `chunk` native samples). `linear`: linear interpolation between PCM
/// samples (smoother); false = the game's drop-sample reads.
pub fn render(d: &mut Driver, rate: u32, n_out: usize, chunk: usize, linear: bool) -> Vec<f32> {
    let bank: &SoundBank = d.bank;
    let ratio = MIX_RATE / rate as f64; // native samples per output sample
    let mut out = vec![0f32; n_out * 2];
    let mut native: u64 = 0; // native clock at the current chunk start
    let total_native = (n_out as f64 * ratio).ceil() as u64 + 1;
    let add = |snaps: &[Snap], t0: f64, t1: f64, out: &mut Vec<f32>| {
        // output frames whose time falls in [t0, t1)
        let j0 = (t0 / ratio).ceil() as usize;
        let j1 = ((t1 / ratio).ceil() as usize).min(n_out);
        for s in snaps {
            let pcm = &bank.samples[s.sample].pcm;
            for j in j0..j1 {
                let dt = j as f64 * ratio - t0;
                if let Some(v) = eval(s, pcm, dt, linear) {
                    out[2 * j] += v * s.gl;
                    out[2 * j + 1] += v * s.gr;
                }
            }
        }
    };
    while native < total_native {
        let n = chunk.max(1);
        // SFX: frame-start update, one segment for the whole chunk
        for v in d.voices.iter_mut() {
            v.update(bank);
        }
        let sfx: Vec<Snap> = d.voices.iter().filter_map(snap).collect();
        add(&sfx, native as f64, (native + n as u64) as f64, &mut out);
        d.mixbuf.clear();
        d.mixbuf.resize(n, 0);
        // music: segments between ticks
        if d.player.playing {
            let mut off = 0usize;
            let mut left = n;
            loop {
                let cd = d.player.countdown as usize;
                let seg = cd.min(left);
                if seg > 0 {
                    let snaps: Vec<Snap> = d.player.tracks.iter().filter_map(|t| snap(&t.v)).collect();
                    let t0 = (native + off as u64) as f64;
                    add(&snaps, t0, t0 + seg as f64, &mut out);
                    d.mix_tracks(off, seg);
                }
                if cd >= left {
                    d.player.countdown -= left as u16;
                    break;
                }
                off += cd;
                left -= cd;
                d.tick();
                d.player.countdown = d.player.samples_per_tick;
            }
        }
        for v in d.voices.iter_mut() {
            v.mix(bank, &mut d.mixbuf, n);
        }
        native += n as u64;
        d.clock = native;
    }
    // scale: pcm (s8) * gain / 64 is the 8-bit output scale; / 128 -> -1..1
    for x in out.iter_mut() {
        *x = (*x / (64.0 * 128.0)).clamp(-1.0, 1.0);
    }
    out
}

/// Convenience: song `id` for `seconds` at `rate` Hz (f32 stereo interleaved).
pub fn render_song(bank: &SoundBank, id: usize, seconds: f64, rate: u32, linear: bool) -> Vec<f32> {
    let mut d = Driver::new(bank);
    d.play_song(id);
    render(&mut d, rate, (seconds * rate as f64) as usize, crate::FRAME_SAMPLES, linear)
}

/// f32 -1..1 -> i16.
pub fn to_i16(x: &[f32]) -> Vec<i16> {
    x.iter().map(|&v| (v * 32767.0).round() as i16).collect()
}
