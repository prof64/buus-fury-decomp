//! Platform-independent frontend core shared by the native shell (`wf-native`) and the browser shell (`wf-web`).
//!
//! [`App`] owns the content packs, a [`Sim`] (the demo world until `wf-core` lands), the renderer and the fixed-step
//! clock. Hosts feed it wall-clock time and per-player buttons, ask it to render a local player's viewport, and
//! forward the audio events it collects to their audio thread ([`audio::AudioEngine`]).

pub mod audio;
pub mod battle;
pub mod control;
pub mod demo;
#[cfg(not(target_arch = "wasm32"))]
pub mod dev;
pub mod hotkeys;
pub mod input;
pub mod launcher;
pub mod link;
#[cfg(not(target_arch = "wasm32"))]
pub mod modcheck;
pub mod trade;
pub mod newmod;
pub mod mods;
pub mod netmenu;
pub mod overlay;
pub mod rebind;
pub mod replay_menu;
pub mod session;
pub mod story;
pub mod sysmenu;

use wf_content::Packs;
use wf_render::{fit, AutoZoom, Fit, Renderer, ViewMode};
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
    /// "Original pacing" (Settings, off by default): after a step the sim reports as a GBA lag (`Sim::lag_hint`:
    /// an area load) the picture holds for that many steps without stepping the sim. Presentation only.
    pub original_pacing: bool,
    hold: u32,
    /// "PC wide (auto)": the zoom held per map (`wf_render::AutoZoom`); `auto_zoom.max` = Settings "Auto zoom: max"
    pub auto_zoom: AutoZoom,
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
            original_pacing: false,
            hold: 0,
            auto_zoom: AutoZoom::default(),
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

    /// Swap the simulation (an online match starts / ends): fresh inputs, view and clock.
    pub fn set_sim(&mut self, sim: Box<dyn Sim>) {
        self.sim = sim;
        self.inputs = vec![0; self.sim.players()];
        self.acc = 0.0;
        self.paused = false;
        self.events.clear();
        self.audio_out.push(AudioEvent::StopAllSfx);
        self.audio_out.push(AudioEvent::StopSong);
        self.view = RenderView::default();
        self.sim.view(&mut self.view);
    }

    /// One simulation step with the current inputs.
    pub fn step(&mut self) {
        self.events.clear();
        if self.original_pacing && self.hold > 0 {
            // the GBA's dropped frames: the picture and the sim hold, the audio driver keeps playing
            self.hold -= 1;
            return;
        }
        self.sim.step(&self.inputs, &mut self.events);
        if self.original_pacing {
            self.hold = self.sim.lag_hint();
        }
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
            // an online match keeps running behind the menu (the local player stands still / holds the shared
            // pause); everything else stops
            match self.sim.paused_input() {
                None => return 0,
                Some(b) => {
                    self.inputs.iter_mut().for_each(|i| *i = 0);
                    if let Some(i) = self.inputs.first_mut() {
                        *i = b;
                    }
                }
            }
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
        // (the map's showable area: the painted part the game's camera can show - PC wide clamps and zooms to it)
        let world = self.renderer.view_area(&self.packs, &self.view).map(|a| (a[2] - a[0], a[3] - a[1])).or_else(|| self.world_size());
        // auto zoom: picked when the shown map changes, kept through frames without one (menu screens, black
        // loads) - see `AutoZoom`
        let mode = match self.mode {
            ViewMode::Pc { zoom: 0 } => {
                let id = self.view.map.as_ref().filter(|_| self.view.menu.is_none()).map(|m| m.id.clone());
                let shown = id.as_deref().filter(|i| !i.is_empty()).zip(world);
                ViewMode::Pc { zoom: self.auto_zoom.zoom(win_w.max(1), win_h.max(1), shown) }
            }
            m => m,
        };
        // a menu screen (title, Select Game, the pause menu: a 240x160 picture) in PC wide: shown like Classic, the
        // picture scaled by the largest integer the window takes - in the wide view it was scaled only as far as the
        // view held it (a 960x518 browser window: a 480x259 view, the menu 1x in black, playtest 42)
        let mode = match (mode, &self.view.menu) {
            (ViewMode::Pc { .. }, Some(m)) if !m.overlay => ViewMode::Gba { scale: 0 },
            (m, _) => m,
        };
        let f = fit(mode, win_w, win_h, world);
        self.renderer.render(&self.packs, &self.view, player, f.view_w as usize, f.view_h as usize);
        f
    }

    pub fn take_audio(&mut self) -> Vec<AudioEvent> {
        std::mem::take(&mut self.audio_out)
    }
}

/// Integer nearest-neighbour blit of `src` (`sw` x `sh`) scaled by `scale`, centred in `dst` (`dw` x `dh`),
/// black borders. Used by hosts that present a CPU buffer (native softbuffer).
/// Each source row is widened once into its first destination row (runs of `scale` equal pixels); the other
/// `scale - 1` rows copy it - no per-pixel divide (`examples/pacing.rs`).
pub fn blit_scaled(src: &[u32], sw: usize, sh: usize, scale: usize, dst: &mut [u32], dw: usize, dh: usize) {
    let scale = scale.max(1);
    let (ow, oh) = (sw * scale, sh * scale);
    let ox = (dw as isize - ow as isize) / 2;
    let oy = (dh as isize - oh as isize) / 2;
    // the destination columns the picture covers, and the scaled source column of the first one
    let x0 = ox.clamp(0, dw as isize) as usize;
    let x1 = (ox + ow as isize).clamp(x0 as isize, dw as isize) as usize;
    let sx0 = (x0 as isize - ox) as usize;
    let mut prev: Option<(usize, usize)> = None; // (source row, the destination row it was widened into)
    for y in 0..dh {
        let sy = y as isize - oy;
        if sy < 0 || sy >= oh as isize {
            dst[y * dw..(y + 1) * dw].fill(0);
            continue;
        }
        let r = sy as usize / scale;
        if let Some((pr, py)) = prev {
            if pr == r {
                dst.copy_within(py * dw..(py + 1) * dw, y * dw);
                continue;
            }
        }
        let row = &mut dst[y * dw..(y + 1) * dw];
        let srow = &src[r * sw..(r + 1) * sw];
        row[..x0].fill(0);
        row[x1..].fill(0);
        let (mut sx, mut k) = (sx0, x0);
        while k < x1 {
            let run = (scale - sx % scale).min(x1 - k);
            row[k..k + run].fill(srow[sx / scale] & 0x00FF_FFFF);
            k += run;
            sx += run;
        }
        prev = Some((r, y));
    }
}

#[cfg(test)]
mod blit_tests {
    /// the plain per-pixel form the fast one must equal
    fn reference(src: &[u32], sw: usize, sh: usize, scale: usize, dst: &mut [u32], dw: usize, dh: usize) {
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

    #[test]
    fn blit_equals_reference() {
        let mut seed = 0x1234_5678u32;
        for &(sw, sh) in &[(240usize, 160usize), (7, 5), (641, 361), (1, 1)] {
            let src: Vec<u32> = (0..sw * sh)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    seed
                })
                .collect();
            for scale in 1..=7 {
                let sizes = [(sw * scale, sh * scale), (sw * scale + 13, sh * scale + 6), (sw * scale / 2 + 1, sh * scale / 2 + 1), (3, 2)];
                for &(dw, dh) in &sizes {
                    let mut a = vec![0xDEAD_BEEFu32; dw * dh];
                    let mut b = vec![0x1234_5678u32; dw * dh];
                    super::blit_scaled(&src, sw, sh, scale, &mut a, dw, dh);
                    reference(&src, sw, sh, scale, &mut b, dw, dh);
                    assert_eq!(a, b, "{sw}x{sh} x{scale} into {dw}x{dh}");
                }
            }
        }
    }
}
