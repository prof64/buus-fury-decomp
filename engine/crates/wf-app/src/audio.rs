//! The game's sound driver as an output device: commands in (from [`wf_view::AudioEvent`]s), float PCM out at the
//! device rate. Runs on the audio thread (cpal callback natively, an AudioWorklet on the web).
//!
//! Like the game, the driver is updated once per video frame (256 or 272 samples, 267.5 on average, at 15 978.3 Hz) and commands take
//! effect at the start of the next update (SFX voices start on update boundaries; a request when all 16 SFX voices
//! are busy is dropped by the driver itself). The exact 8-bit FIFO stream is then resampled to the device rate.

use std::collections::VecDeque;

use wf_audio::bank::SoundBank;
use wf_audio::driver::{Driver, MIX_RATE};
use wf_view::{AudioEvent, GBA_FPS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resample {
    /// Sample-and-hold (what the GBA DAC does before analogue filtering; harsh).
    Hold,
    /// Linear interpolation between FIFO samples (default).
    Linear,
}

/// The samples of one driver update: the whole 16-sample blocks of the time accumulated (`acc`, in samples; the rest
/// stays in it) - the game counts the played samples 16 at a time.
pub fn update_samples(acc: &mut f64) -> usize {
    let n = (*acc as usize) & !15;
    *acc -= n as f64;
    n
}

/// Game frames of requests that may wait for the driver before two are taken per update (`push_frame`).
pub const MAX_BACKLOG: usize = 6;

pub struct AudioEngine {
    driver: Driver<'static>,
    step: f64,
    t: f64,
    src: VecDeque<(i8, i8)>,
    chunk: Vec<(i8, i8)>,
    frame_acc: f64,
    pending: VecDeque<AudioEvent>,
    /// game frames' requests not yet played, one group per frame (`push_frame`)
    frames: VecDeque<Vec<AudioEvent>>,
    pub gain: f32,
    pub resample: Resample,
    pub muted: bool,
}

impl AudioEngine {
    /// The bank must outlive the engine; hosts leak it once per content load (`Box::leak`).
    pub fn new(bank: &'static SoundBank, out_rate: u32) -> AudioEngine {
        AudioEngine {
            driver: Driver::new(bank),
            step: MIX_RATE / out_rate.max(1) as f64,
            t: 0.0,
            src: VecDeque::new(),
            chunk: Vec::new(),
            frame_acc: 0.0,
            pending: VecDeque::new(),
            frames: VecDeque::new(),
            gain: 1.0,
            resample: Resample::Linear,
            muted: false,
        }
    }

    pub fn set_out_rate(&mut self, out_rate: u32) {
        self.step = MIX_RATE / out_rate.max(1) as f64;
    }

    /// A request with no frame of its own: it takes effect at the next driver update.
    pub fn push(&mut self, e: AudioEvent) {
        self.pending.push_back(e);
    }

    /// One game frame's requests (empty for a frame without any). The game's driver takes each frame's requests at
    /// its own update, ~267 samples after the frame before's: two sounds asked for a frame apart start a frame of
    /// samples apart. The host's audio callback runs in bursts of several frames, so without the grouping every
    /// request of a burst started in the same update. While more than [`MAX_BACKLOG`] frames wait (the host steps a
    /// little faster than the GBA's 59.73 Hz, or the device stalled) two are taken in one update.
    pub fn push_frame(&mut self, evs: Vec<AudioEvent>) {
        self.frames.push_back(evs);
    }

    fn apply(&mut self, e: AudioEvent) {
        let d = &mut self.driver;
        match e {
            AudioEvent::PlaySong(id) => {
                if (id as usize) < d.bank.songs.len() {
                    d.play_song(id as usize)
                }
            }
            AudioEvent::StopSong => d.stop_song(),
            AudioEvent::PlaySfx { id, volume, pan } => {
                if (id as usize) < d.bank.sfx.len() {
                    d.play_sfx(id, volume, pan);
                }
            }
            AudioEvent::StopSfx(id) => d.stop_sfx(id),
            AudioEvent::StopAllSfx => d.stop_all_sfx(),
            AudioEvent::SetMasterVolumes { music, sfx } => d.set_master_volumes(music, sfx),
        }
    }

    /// One driver update (one game frame of samples).
    fn update(&mut self) {
        while let Some(e) = self.pending.pop_front() {
            self.apply(e);
        }
        let take = if self.frames.len() > MAX_BACKLOG { 2 } else { 1 };
        for _ in 0..take {
            if let Some(g) = self.frames.pop_front() {
                for e in g {
                    self.apply(e);
                }
            }
        }
        // (the game's update mixes what the DMA has played since the last one, counted by the timer interrupt in
        // blocks of 16 samples - 0x03000AE0: a frame's update is 256 or 272 samples, never 267 or 268; the running
        // game's updates, wf-audio re/fifo_live.py)
        self.frame_acc += MIX_RATE / GBA_FPS;
        let n = update_samples(&mut self.frame_acc);
        self.chunk.clear();
        self.driver.render_into(&mut self.chunk, n);
        self.src.extend(self.chunk.iter().copied());
    }

    /// Fill interleaved stereo `out` (`[l, r, l, r, ...]`) at the device rate.
    pub fn render(&mut self, out: &mut [f32]) {
        let g = if self.muted { 0.0 } else { self.gain / 128.0 };
        for o in out.chunks_exact_mut(2) {
            while self.src.len() < self.t as usize + 2 {
                self.update();
            }
            let i = self.t as usize;
            let (a, b) = (self.src[i], self.src[i + 1]);
            let (l, r) = match self.resample {
                Resample::Hold => (a.0 as f32, a.1 as f32),
                Resample::Linear => {
                    let f = (self.t - i as f64) as f32;
                    (a.0 as f32 + (b.0 as f32 - a.0 as f32) * f, a.1 as f32 + (b.1 as f32 - a.1 as f32) * f)
                }
            };
            o[0] = l * g;
            o[1] = r * g;
            self.t += self.step;
            while self.t >= 1.0 {
                self.src.pop_front();
                self.t -= 1.0;
            }
        }
    }

    pub fn busy_sfx_voices(&self) -> usize {
        self.driver.busy_voices()
    }
}

