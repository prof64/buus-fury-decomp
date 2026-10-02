//! Platform-independent frontend core shared by the native shell (`wf-native`) and the browser shell (`wf-web`).
//!
//! [`App`] owns the content packs, a [`Sim`] (the demo world until `wf-core` lands), the renderer and the fixed-step
//! clock. Hosts feed it wall-clock time and per-player buttons, ask it to render a local player's viewport, and
//! forward the audio events it collects to their audio thread ([`audio::AudioEngine`]).

pub mod audio;
pub mod battle;
pub mod control;
pub mod demo;
pub mod hotkeys;
pub mod input;
pub mod overlay;
pub mod rebind;
pub mod replay_menu;
pub mod session;

use wf_content::Packs;
use wf_render::{fit, Fit, Renderer, ViewMode};
use wf_view::{AudioEvent, Buttons, RenderView, Sim, SimEvent, GBA_FPS};

pub use wf_content;
pub use wf_render;
pub use wf_replay;
pub use wf_view;

/// Frame pacing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pacing {
    /// Steps per second: the GBA's 59.7275 Hz by default (exact game speed).
    pub fps: f64,
    /// Never run more than this many steps per host frame (after a stall the game slows down instead of
    /// fast-forwarding through seconds of input).
    pub max_steps: u32,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing { fps: GBA_FPS, max_steps: 4 }
    }
}

pub struct App {
    pub packs: Packs,
    pub sim: Box<dyn Sim>,
    pub renderer: Renderer,
    pub view: RenderView,
    pub mode: ViewMode,
    pub pacing: Pacing,
    pub paused: bool,
    /// Buttons per player slot for the next step(s).
    pub inputs: Vec<Buttons>,
    acc: f64,
    events: Vec<SimEvent>,
    audio_out: Vec<AudioEvent>,
    /// Timing of the last render (ms) / steps, for the perf overlay.
    pub stats: Stats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub steps: u64,
    pub last_step_ms: f64,
    pub last_render_ms: f64,
}

impl App {
    pub fn new(packs: Packs, sim: Box<dyn Sim>) -> App {
        let n = sim.players();
        let mut view = RenderView::default();
        sim.view(&mut view);
        App {
            packs,
            sim,
            renderer: Renderer::new(),
            view,
            mode: ViewMode::default(),
            pacing: Pacing::default(),
            paused: false,
            inputs: vec![0; n],
            acc: 0.0,
            events: Vec::new(),
            audio_out: Vec::new(),
            stats: Stats::default(),
        }
    }

    /// Start the demo world on `cfg`.
    pub fn demo(packs: Packs, cfg: &demo::DemoConfig) -> wf_content::Result<App> {
        let w = demo::DemoWorld::new(&packs, cfg)?;
        Ok(App::new(packs, Box::new(w)))
    }

    /// Start a real battle (`wf-core`) on `cfg`.
    pub fn battle(packs: Packs, cfg: battle::BattleConfig) -> Result<App, String> {
        let s = battle::session(&packs, cfg)?;
        Ok(App::new(packs, Box::new(battle::CoreSim(s))))
    }

    /// One simulation step with the current inputs.
    pub fn step(&mut self) {
        self.events.clear();
        self.sim.step(&self.inputs, &mut self.events);
        for e in &self.events {
            if let SimEvent::Audio(a) = e {
                self.audio_out.push(*a);
            }
        }
        self.stats.steps += 1;
    }

    /// Advance the fixed-step clock by `dt` seconds of wall time; returns the number of steps run. `now_ms` is
    /// only used for the stats.
    pub fn advance(&mut self, dt: f64, now_ms: &dyn Fn() -> f64) -> u32 {
        if self.paused {
            return 0;
        }
        let period = 1.0 / self.pacing.fps;
        self.acc += dt.clamp(0.0, 0.25);
        let mut n = 0;
        let t0 = now_ms();
        while self.acc >= period && n < self.pacing.max_steps {
            self.step();
            self.acc -= period;
            n += 1;
        }
        if n == self.pacing.max_steps {
            self.acc = self.acc.min(period);
        }
        if n > 0 {
            self.stats.last_step_ms = (now_ms() - t0) / n as f64;
            self.sim.view(&mut self.view);
        }
        n
    }

    /// World size of the current map (for the auto zoom).
    pub fn world_size(&mut self) -> Option<(i32, i32)> {
        let id = self.view.map.as_ref()?.id.clone();
        self.renderer.map(&self.packs, &id).map(|m| (m.world_w, m.world_h))
    }

    /// Render local player `player` for a `win_w` x `win_h` (device pixels) window. The framebuffer is
    /// `self.renderer.fb` (`fit.view_w` x `fit.view_h`, ARGB), to be shown scaled by `fit.scale`.
    pub fn render(&mut self, player: usize, win_w: u32, win_h: u32) -> Fit {
        let world = self.world_size();
        let f = fit(self.mode, win_w, win_h, world);
        self.renderer.render(&self.packs, &self.view, player, f.view_w as usize, f.view_h as usize);
        f
    }

    pub fn take_audio(&mut self) -> Vec<AudioEvent> {
        std::mem::take(&mut self.audio_out)
    }
}

/// Integer nearest-neighbour blit of `src` (`sw` x `sh`) scaled by `scale`, centred in `dst` (`dw` x `dh`),
/// black borders. Used by hosts that present a CPU buffer (native softbuffer).
pub fn blit_scaled(src: &[u32], sw: usize, sh: usize, scale: usize, dst: &mut [u32], dw: usize, dh: usize) {
    let scale = scale.max(1);
    let (ow, oh) = (sw * scale, sh * scale);
    let ox = (dw as isize - ow as isize) / 2;
    let oy = (dh as isize - oh as isize) / 2;
    for y in 0..dh {
        let row = &mut dst[y * dw..(y + 1) * dw];
        let sy = y as isize - oy;
        if sy < 0 || sy >= oh as isize {
            row.fill(0);
            continue;
        }
        let srow = &src[(sy as usize / scale) * sw..(sy as usize / scale + 1) * sw];
        for (x, d) in row.iter_mut().enumerate() {
            let sx = x as isize - ox;
            *d = if sx < 0 || sx >= ow as isize { 0 } else { srow[sx as usize / scale] & 0x00FF_FFFF };
        }
    }
}
