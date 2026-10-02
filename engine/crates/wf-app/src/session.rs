//! Headless sessions: the game without a window or an audio device — for tests, CI, agents, replays, benchmarks
//! and servers. Everything a windowed run does still happens: the simulation steps, the sound driver runs (muted:
//! its PCM goes nowhere, but the commands it received and its voice usage are observable), and any frame can be
//! rendered on demand to an offscreen buffer (screenshots, video frames, a spectator viewer). Rendering is a pure
//! read of the state, so watching or capturing never changes the simulation.

use wf_audio::bank::SoundBank;
use wf_render::Fit;
use wf_view::{AudioEvent, Buttons, GBA_FPS};

use crate::audio::AudioEngine;
use crate::App;

/// Rate the muted driver is clocked at (only affects resampling work; the driver itself runs at 15 978 Hz).
const MUTED_RATE: u32 = 48_000;

pub struct Session {
    pub app: App,
    /// The sound driver, muted (None = no audio content).
    pub audio: Option<AudioEngine>,
    /// Every audio command the simulation emitted: (frame, event).
    pub audio_log: Vec<(u64, AudioEvent)>,
    sink: Vec<f32>,
    acc: f64,
}

/// An offscreen frame: `0xAARRGGBB` pixels, `w` x `h` world pixels, meant to be shown at `scale`.
pub struct Shot {
    pub px: Vec<u32>,
    pub w: u32,
    pub h: u32,
    pub scale: u32,
}

impl Shot {
    /// Binary PPM (P6), `scale`d up by `s` (nearest neighbour).
    pub fn ppm(&self, s: u32) -> Vec<u8> {
        let s = s.max(1) as usize;
        let (w, h) = (self.w as usize, self.h as usize);
        let mut out = format!("P6\n{} {}\n255\n", w * s, h * s).into_bytes();
        for y in 0..h * s {
            for x in 0..w * s {
                let p = self.px[(y / s) * w + x / s];
                out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
            }
        }
        out
    }
    /// 24-bit BMP (what a browser `<img>` shows without any encoder dependency).
    pub fn bmp(&self) -> Vec<u8> {
        let (w, h) = (self.w as usize, self.h as usize);
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut b = Vec::with_capacity(size);
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(size as u32).to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(-(h as i32)).to_le_bytes()); // top-down
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0; 24]);
        for y in 0..h {
            let start = b.len();
            for x in 0..w {
                let p = self.px[y * w + x];
                b.extend_from_slice(&[p as u8, (p >> 8) as u8, (p >> 16) as u8]);
            }
            b.resize(start + row, 0);
        }
        b
    }
}

impl Session {
    /// `bank`: the sound bank (from `wf_content::audio::load_bank`), leaked by the caller for `'static`.
    pub fn new(app: App, bank: Option<&'static SoundBank>) -> Session {
        let audio = bank.map(|b| {
            let mut e = AudioEngine::new(b, MUTED_RATE);
            e.muted = true;
            e
        });
        Session { app, audio, audio_log: Vec::new(), sink: Vec::new(), acc: 0.0 }
    }

    /// One frame with these inputs (missing players = 0).
    pub fn step(&mut self, inputs: &[Buttons]) {
        for (i, v) in self.app.inputs.iter_mut().enumerate() {
            *v = inputs.get(i).copied().unwrap_or(0);
        }
        self.app.step();
        let f = self.app.sim.frame();
        for e in self.app.take_audio() {
            self.audio_log.push((f, e));
            if let Some(a) = &mut self.audio {
                a.push(e);
            }
        }
        // clock the muted driver by one frame of output so its state (voices, song position) stays live
        if let Some(a) = &mut self.audio {
            self.acc += MUTED_RATE as f64 / GBA_FPS;
            let n = self.acc as usize;
            self.acc -= n as f64;
            self.sink.resize(n * 2, 0.0);
            a.render(&mut self.sink);
        }
    }

    pub fn frame(&self) -> u64 {
        self.app.sim.frame()
    }

    pub fn state_hash(&self) -> u64 {
        self.app.sim.state_hash()
    }

    /// Render local player `player` as a `win_w` x `win_h` window would show it (current view mode).
    pub fn screenshot(&mut self, player: usize, win_w: u32, win_h: u32) -> Shot {
        self.app.sim.view(&mut self.app.view);
        let f: Fit = self.app.render(player, win_w, win_h);
        Shot { px: self.app.renderer.fb.clone(), w: f.view_w, h: f.view_h, scale: f.scale }
    }

    /// Busy SFX voices of the (muted) driver.
    pub fn sfx_voices(&self) -> usize {
        self.audio.as_ref().map(|a| a.busy_sfx_voices()).unwrap_or(0)
    }
}

/// Whether this process should run headless + muted by default: explicit env (`WF_AGENT=1`, `WF_HEADLESS=1`,
/// `CI`) or no terminal on stdin and stdout (tests, agents, pipes). A human asks for a window with `--window`
/// or `WF_WINDOW=1` (an app bundle / desktop launcher passes `--window`).
pub fn automation_default() -> bool {
    use std::io::IsTerminal;
    let env = |k: &str| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0");
    if env("WF_WINDOW") {
        return false;
    }
    env("WF_AGENT") || env("WF_HEADLESS") || env("CI") || (!std::io::stdin().is_terminal() && !std::io::stdout().is_terminal())
}
