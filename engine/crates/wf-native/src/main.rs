//! Native shell. `buus-fury [rom.gba] [--content DIR] [--area 8-3] [--players N] [--gba] [--zoom N]
//! [--reextract] [--frames N --shot out.ppm]`
//!
//! First launch: a window asking for the ROM (drop the file on the window, press O for a file dialog, or pass the
//! path). The ROM is verified (friendly message for other games / regions / bad dumps), extracted on a background
//! thread into `<data dir>/wf-buu/content/base/` (macOS: `~/Library/Application Support/wf-buu`), and kept as
//! `rom.gba` there so an engine update can re-extract without asking. Mods go in `content/mods/<name>/`.
//! Keys: mGBA defaults (arrows, X = A, Z = B, A = L, S = R, Enter = Start, Backspace = Select), rebindable in
//! `<data dir>/wf-buu/bindings.json`; Esc pause, F1 PC / classic view, F2 zoom, F3 stats (window title), F11
//! fullscreen. Gamepads via gilrs.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use wf_app::audio::AudioEngine;
use wf_app::demo::DemoConfig;
use wf_app::input::{Bindings, KeyState};
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_app::wf_render::{font, ViewMode};
use wf_app::{blit_scaled, App};
use wf_view::AudioEvent;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

const EXTRACTOR: &str = concat!("wf-extract ", env!("CARGO_PKG_VERSION"));

struct Args {
    rom: Option<PathBuf>,
    content: PathBuf,
    data: PathBuf,
    area: String,
    players: usize,
    mode: ViewMode,
    reextract: bool,
    frames: Option<u64>,
    shot: Option<PathBuf>,
    /// never open an audio device (`--mute`, `WF_MUTE=1`, or any `CI` environment)
    mute: bool,
    /// no window: run `--frames` steps offscreen, write `--shot`, exit (implies `--mute`)
    headless: bool,
    size: (u32, u32),
    input: String,
    spectate: Option<String>,
    realtime: bool,
    show_controls: bool,
    demo: bool,
    arena: String,
    enemy: String,
    count: u32,
    fighter: String,
    form: Option<i32>,
    level: Option<i32>,
    seed: u32,
}

fn parse_args() -> Args {
    let data = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("wf-buu");
    let mut a = Args {
        rom: None,
        content: data.join("content"),
        data,
        area: "8-3".into(),
        players: 1,
        mode: ViewMode::Pc { zoom: 0 },
        reextract: false,
        frames: None,
        shot: None,
        mute: std::env::var("WF_MUTE").is_ok_and(|v| v != "0") || std::env::var_os("CI").is_some(),
        headless: wf_app::session::automation_default(),
        size: (960, 640),
        input: String::new(),
        spectate: None,
        realtime: false,
        show_controls: false,
        demo: false,
        arena: "forest".into(),
        enemy: "t68".into(),
        count: 1,
        fighter: "goku".into(),
        form: None,
        level: None,
        seed: 1,
    };
    if a.headless {
        a.mute = true;
    }
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let next = |i: &mut usize| {
            *i += 1;
            v.get(*i).cloned().unwrap_or_default()
        };
        match v[i].as_str() {
            "--content" => a.content = PathBuf::from(next(&mut i)),
            "--data" => {
                a.data = PathBuf::from(next(&mut i));
                a.content = a.data.join("content");
            }
            "--area" => {
                a.area = next(&mut i);
                a.demo = true;
            }
            "--demo" => a.demo = true,
            "--arena" => a.arena = next(&mut i),
            "--enemy" => a.enemy = next(&mut i),
            "--count" => a.count = next(&mut i).parse().unwrap_or(1),
            "--fighter" => a.fighter = next(&mut i),
            "--form" => a.form = next(&mut i).parse().ok(),
            "--level" => a.level = next(&mut i).parse().ok(),
            "--seed" => a.seed = next(&mut i).parse().unwrap_or(1),
            "--players" => a.players = next(&mut i).parse().unwrap_or(1),
            "--gba" => a.mode = ViewMode::Gba { scale: 0 },
            "--zoom" => a.mode = ViewMode::Pc { zoom: next(&mut i).parse().unwrap_or(0) },
            "--reextract" => a.reextract = true,
            "--mute" => a.mute = true,
            "--headless" => {
                a.headless = true;
                a.mute = true;
            }
            "--size" => {
                let v = next(&mut i);
                if let Some((w, h)) = v.split_once('x') {
                    a.size = (w.parse().unwrap_or(960), h.parse().unwrap_or(640));
                }
            }
            "--input" => a.input = next(&mut i),
            "--input-file" => a.input = std::fs::read_to_string(next(&mut i)).unwrap_or_default().trim().to_string(),
            "--window" => a.headless = false,
            "--spectate" => {
                a.spectate = Some(next(&mut i));
                a.headless = true;
                a.mute = true;
            }
            "--realtime" => a.realtime = true,
            "--show-controls" => a.show_controls = true,
            "--frames" => a.frames = next(&mut i).parse().ok(),
            "--shot" => a.shot = Some(PathBuf::from(next(&mut i))),
            "-h" | "--help" => {
                println!("{}", include_str!("main.rs").lines().take(4).map(|l| l.trim_start_matches("//! ")).collect::<Vec<_>>().join("\n"));
                std::process::exit(0);
            }
            s => a.rom = Some(PathBuf::from(s)),
        }
        i += 1;
    }
    a
}

/// A message for a ROM that is not the supported dump (None = OK).
fn rom_problem(rom: &[u8]) -> Option<String> {
    let sha = wf_rom::sha1_hex(rom);
    if sha == wf_rom::BUU_US_SHA1 {
        return None;
    }
    if rom.len() < 0xC0 || rom[0xB2] != 0x96 {
        return Some("That file is not a Game Boy Advance ROM.".into());
    }
    let s = |a: usize, b: usize| String::from_utf8_lossy(&rom[a..b]).trim_end_matches('\0').to_string();
    let (title, code) = (s(0xA0, 0xAC), s(0xAC, 0xB0));
    Some(if let Some(r) = code.strip_prefix("BG3") {
        match r {
            "P" => format!("This is the European release of Buu's Fury ({code}). The engine needs the USA release (BG3E)."),
            "E" => format!("This is Buu's Fury (USA) but not the known good dump (SHA-1 {sha}). It may be patched, trimmed or corrupted."),
            _ => format!("This is another region of Buu's Fury ({code}). The engine needs the USA release (BG3E)."),
        }
    } else {
        format!("That ROM is \"{title}\" ({code}), not Dragon Ball Z: Buu's Fury.")
    })
}

fn content_ok(dir: &Path) -> bool {
    std::fs::read(dir.join("base/pack.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|p| p["generated_by"] == EXTRACTOR)
}

enum Progress {
    Step(String, f32),
    Done(f64),
    Failed(String),
}

/// Verify + extract on a thread, writing `<content>/base` (via a temp dir, swapped in at the end).
fn spawn_extract(rom: Vec<u8>, content: PathBuf, data: PathBuf) -> mpsc::Receiver<Progress> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        if let Some(e) = rom_problem(&rom) {
            let _ = tx.send(Progress::Failed(e));
            return;
        }
        let _ = std::fs::create_dir_all(&data);
        let _ = std::fs::write(data.join("rom.gba"), &rom);
        let rom = match wf_extract::Rom::new(rom) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Progress::Failed(e.to_string()));
                return;
            }
        };
        let n = wf_extract::CATEGORIES.len() as f32;
        let mut k = 0.0;
        let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |cat| {
            let _ = tx.send(Progress::Step(cat.to_string(), k / (n + 1.0)));
            k += 1.0;
        });
        let _ = tx.send(Progress::Step("writing files".into(), n / (n + 1.0)));
        let tmp = content.join("base.tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        for (p, d) in &c.files {
            let f = tmp.join(p);
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&f, d) {
                let _ = tx.send(Progress::Failed(format!("cannot write {}: {e}", f.display())));
                return;
            }
        }
        let _ = std::fs::remove_dir_all(content.join("base"));
        if let Err(e) = std::fs::rename(&tmp, content.join("base")) {
            let _ = tx.send(Progress::Failed(format!("cannot move content into place: {e}")));
            return;
        }
        let _ = tx.send(Progress::Done(t0.elapsed().as_secs_f64()));
    });
    rx
}

/// The simulation: `wf-core`'s battle by default, the frontend demo world with `--demo`.
fn make_app(a: &Args, packs: Packs) -> Result<App, String> {
    if a.demo {
        let cfg = DemoConfig { area: a.area.clone(), players: a.players, ..Default::default() };
        return App::demo(packs, &cfg).map_err(|e| e.to_string());
    }
    let cfg = wf_app::battle::config(&a.arena, &a.enemy, a.count, &a.fighter, a.form, a.level, a.players, a.seed);
    App::battle(packs, cfg)
}

fn load_packs(content: &Path) -> wf_app::wf_content::Result<Packs> {
    let mut src: Vec<Box<dyn FileSource>> = vec![Box::new(DirSource(content.join("base")))];
    if let Ok(rd) = std::fs::read_dir(content.join("mods")) {
        let mut mods: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        mods.sort();
        for m in mods {
            src.push(Box::new(DirSource(m)));
        }
    }
    Packs::new(src)
}

fn start_audio(packs: &Packs) -> Result<(cpal::Stream, mpsc::Sender<AudioEvent>), String> {
    let bank = wf_app::wf_content::audio::load_bank(packs).map_err(|e| e.to_string())?;
    let bank: &'static _ = Box::leak(Box::new(bank));
    let dev = cpal::default_host().default_output_device().ok_or("no audio output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let rate = cfg.sample_rate().0;
    let ch = cfg.channels() as usize;
    let (tx, rx) = mpsc::channel::<AudioEvent>();
    let mut eng = AudioEngine::new(bank, rate);
    let mut tmp: Vec<f32> = Vec::new();
    let stream = dev
        .build_output_stream(
            &cfg.into(),
            move |out: &mut [f32], _| {
                while let Ok(e) = rx.try_recv() {
                    eng.push(e);
                }
                let frames = out.len() / ch.max(1);
                tmp.resize(frames * 2, 0.0);
                eng.render(&mut tmp);
                for f in 0..frames {
                    for c in 0..ch {
                        out[f * ch + c] = tmp[f * 2 + c.min(1)];
                    }
                }
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, tx))
}

struct Running {
    app: App,
    keys: KeyState,
    bindings: Bindings,
    gilrs: Option<gilrs::Gilrs>,
    audio: Option<(cpal::Stream, mpsc::Sender<AudioEvent>)>,
    /// `--mute`: the driver still runs (no device), clocked by the frame loop
    muted: Option<AudioEngine>,
    stats: bool,
    fps: (u32, Instant, f64),
    /// F5: in-game controls screen
    menu: wf_app::rebind::RebindMenu,
    bindings_path: PathBuf,
}

enum Phase {
    NeedRom(Option<String>),
    Extracting(mpsc::Receiver<Progress>, String, f32),
    Ready(Box<Running>),
    Failed(String),
}

struct Shell {
    args: Args,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    phase: Phase,
    last: Instant,
    ui: Vec<u32>,
}

impl Shell {
    fn begin_extract(&mut self, rom: Vec<u8>) {
        if let Some(e) = rom_problem(&rom) {
            self.phase = Phase::NeedRom(Some(e));
            return;
        }
        let rx = spawn_extract(rom, self.args.content.clone(), self.args.data.clone());
        self.phase = Phase::Extracting(rx, "starting".into(), 0.0);
    }

    fn boot(&mut self) {
        let packs = match load_packs(&self.args.content) {
            Ok(p) => p,
            Err(e) => {
                self.phase = Phase::Failed(e.to_string());
                return;
            }
        };
        for w in &packs.warnings {
            eprintln!("content: {w}");
        }
        let audio = match if self.args.mute { Err("muted".to_string()) } else { start_audio(&packs) } {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("audio disabled: {e}");
                None
            }
        };
        let muted = if audio.is_none() {
            wf_app::wf_content::audio::load_bank(&packs).ok().map(|b| {
                let mut e = AudioEngine::new(Box::leak(Box::new(b)), 48_000);
                e.muted = true;
                e
            })
        } else {
            None
        };
        match make_app(&self.args, packs) {
            Ok(mut app) => {
                app.mode = self.args.mode;
                let bpath = self.args.data.join("bindings.json");
                let bindings = std::fs::read_to_string(&bpath).ok().and_then(|s| Bindings::from_json(&s)).unwrap_or_else(|| {
                    let b = Bindings::default();
                    let _ = std::fs::create_dir_all(&self.args.data);
                    let _ = std::fs::write(&bpath, b.to_json());
                    b
                });
                self.phase = Phase::Ready(Box::new(Running {
                    app,
                    keys: KeyState::default(),
                    bindings,
                    gilrs: gilrs::Gilrs::new().ok(),
                    audio,
                    muted,
                    stats: false,
                    menu: Default::default(),
                    bindings_path: bpath.clone(),
                    fps: (0, Instant::now(), 0.0),
                }));
            }
            Err(e) => self.phase = Phase::Failed(e),
        }
    }

    /// Text screens (ROM prompt / progress / errors) on a small canvas.
    fn draw_screen(&mut self, lines: &[(String, u32)], frac: Option<f32>) -> (usize, usize) {
        let (w, h) = (320usize, 180usize);
        self.ui.clear();
        self.ui.resize(w * h, 0xFF0D_0D0F);
        let mut y = 50;
        for (l, c) in lines {
            // wrap at 50 columns
            let mut line = String::new();
            for word in l.split(' ') {
                if line.len() + word.len() + 1 > 50 {
                    font::draw_text(&mut self.ui, w, h, (w as i32 - font::text_width(&line)) / 2, y, &line, *c);
                    y += font::LINE;
                    line.clear();
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            font::draw_text(&mut self.ui, w, h, (w as i32 - font::text_width(&line)) / 2, y, &line, *c);
            y += font::LINE + 4;
        }
        if let Some(f) = frac {
            font::fill_rect(&mut self.ui, w, h, 60, y + 6, 200, 6, 0xFF2E_2D33);
            font::fill_rect(&mut self.ui, w, h, 60, y + 6, (200.0 * f) as i32, 6, 0xFFF0_A030);
        }
        (w, h)
    }

    fn redraw(&mut self) {
        let Some(win) = self.window.clone() else { return };
        let size = win.inner_size();
        let (ww, wh) = (size.width.max(1), size.height.max(1));
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;

        // phase transitions
        if let Phase::Extracting(rx, status, frac) = &mut self.phase {
            let mut next = None;
            while let Ok(p) = rx.try_recv() {
                match p {
                    Progress::Step(s, f) => {
                        *status = s;
                        *frac = f;
                    }
                    Progress::Done(s) => {
                        eprintln!("extracted + written in {s:.2} s");
                        next = Some(None);
                    }
                    Progress::Failed(e) => next = Some(Some(e)),
                }
            }
            match next {
                Some(None) => self.boot(),
                Some(Some(e)) => self.phase = Phase::NeedRom(Some(e)),
                None => {}
            }
        }

        let (src_w, src_h, scale, use_ui) = match &mut self.phase {
            Phase::Ready(r) => {
                if let Some(g) = &mut r.gilrs {
                    while g.next_event().is_some() {}
                    let pads: Vec<(u32, f32, f32)> = g
                        .gamepads()
                        .map(|(_, p)| {
                            use gilrs::{Axis, Button as B};
                            let map = [
                                (B::South, 0),
                                (B::East, 1),
                                (B::West, 2),
                                (B::North, 3),
                                (B::LeftTrigger, 4),
                                (B::RightTrigger, 5),
                                (B::LeftTrigger2, 6),
                                (B::RightTrigger2, 7),
                                (B::Select, 8),
                                (B::Start, 9),
                                (B::DPadUp, 12),
                                (B::DPadDown, 13),
                                (B::DPadLeft, 14),
                                (B::DPadRight, 15),
                            ];
                            let bits = map.iter().filter(|(b, _)| p.is_pressed(*b)).fold(0u32, |a, (_, i)| a | 1 << i);
                            (bits, p.value(Axis::LeftStickX), -p.value(Axis::LeftStickY))
                        })
                        .collect();
                    for (i, inp) in r.app.inputs.iter_mut().enumerate() {
                        let pad = pads.get(i).map(|&(b, x, y)| r.bindings.pad(b, x, y)).unwrap_or(0);
                        *inp = pad | if i == 0 { r.keys.buttons(&r.bindings) } else { 0 };
                    }
                } else if let Some(inp) = r.app.inputs.first_mut() {
                    *inp = r.keys.buttons(&r.bindings);
                }
                if r.app.advance(dt, &|| 0.0) > 0 {
                    r.keys.consumed();
                }
                if let Some((_, tx)) = &r.audio {
                    for e in r.app.take_audio() {
                        let _ = tx.send(e);
                    }
                } else if let Some(m) = &mut r.muted {
                    for e in r.app.take_audio() {
                        m.push(e);
                    }
                    let mut sink = vec![0.0f32; ((dt.min(0.25) * 48_000.0) as usize) * 2];
                    m.render(&mut sink);
                } else {
                    r.app.take_audio();
                }
                let t0 = Instant::now();
                let f = r.app.render(0, ww, wh);
                r.app.stats.last_render_ms = t0.elapsed().as_secs_f64() * 1e3;
                r.fps.0 += 1;
                if r.fps.1.elapsed() >= Duration::from_millis(500) {
                    r.fps.2 = r.fps.0 as f64 / r.fps.1.elapsed().as_secs_f64();
                    r.fps = (0, Instant::now(), r.fps.2);
                    if r.stats {
                        win.set_title(&format!(
                            "Buu's Fury - {:.1} fps, render {:.2} ms, view {}x{} @{}x, frame {}",
                            r.fps.2,
                            r.app.stats.last_render_ms,
                            f.view_w,
                            f.view_h,
                            f.scale,
                            r.app.sim.frame()
                        ));
                    }
                }
                if let (Some(n), Some(path)) = (self.args.frames, &self.args.shot) {
                    if r.app.sim.frame() >= n {
                        write_ppm(path, &r.app.renderer.fb, f.view_w as usize, f.view_h as usize);
                        std::process::exit(0);
                    }
                }
                let (vw, vh) = (f.view_w as usize, f.view_h as usize);
                r.menu.draw(&r.app.packs, &mut r.app.renderer.fb, vw, vh, &r.bindings);
                (vw, vh, f.scale as usize, false)
            }
            Phase::NeedRom(err) => {
                let mut l = vec![
                    ("DROP YOUR BUU'S FURY ROM HERE".to_string(), 0xFFF0_A030),
                    ("Dragon Ball Z: Buu's Fury (USA). Drop the .gba file on this window or press O to choose it.".into(), 0xFFEC_EBE7),
                    ("It is converted locally; nothing is uploaded.".into(), 0xFF9A_988F),
                ];
                if let Some(e) = err {
                    l.push((e.clone(), 0xFFFF_7A6B));
                }
                let (w, h) = self.draw_screen(&l, None);
                (w, h, ((ww as usize / w).min(wh as usize / h)).max(1), true)
            }
            Phase::Extracting(_, s, f) => {
                let l = vec![("EXTRACTING GAME CONTENT".to_string(), 0xFFF0_A030), (s.clone(), 0xFF9A_988F)];
                let f = *f;
                let (w, h) = self.draw_screen(&l, Some(f));
                (w, h, ((ww as usize / w).min(wh as usize / h)).max(1), true)
            }
            Phase::Failed(e) => {
                let l = vec![("COULD NOT START".to_string(), 0xFFFF_7A6B), (e.clone(), 0xFFEC_EBE7)];
                let (w, h) = self.draw_screen(&l, None);
                (w, h, ((ww as usize / w).min(wh as usize / h)).max(1), true)
            }
        };
        let Some(surface) = self.surface.as_mut() else { return };
        let _ = surface.resize(NonZeroU32::new(ww).unwrap(), NonZeroU32::new(wh).unwrap());
        if let Ok(mut buf) = surface.buffer_mut() {
            let src: &[u32] = match (&self.phase, use_ui) {
                (Phase::Ready(r), false) => &r.app.renderer.fb,
                _ => &self.ui,
            };
            if src.len() >= src_w * src_h {
                blit_scaled(src, src_w, src_h, scale, &mut buf, ww as usize, wh as usize);
            }
            let _ = buf.present();
        }
    }
}

fn write_ppm(path: &Path, fb: &[u32], w: usize, h: usize) {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for &p in &fb[..w * h] {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    let _ = std::fs::write(path, out);
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Buu's Fury")
            .with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));
        let win = Rc::new(el.create_window(attrs).expect("window"));
        let ctx = softbuffer::Context::new(win.clone()).expect("softbuffer context");
        self.surface = Some(softbuffer::Surface::new(&ctx, win.clone()).expect("surface"));
        self.window = Some(win);
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::DroppedFile(p) => {
                if matches!(self.phase, Phase::NeedRom(_)) {
                    match std::fs::read(&p) {
                        Ok(b) => self.begin_extract(b),
                        Err(e) => self.phase = Phase::NeedRom(Some(format!("cannot read {}: {e}", p.display()))),
                    }
                }
            }
            WindowEvent::Focused(false) => {
                if let Phase::Ready(r) = &mut self.phase {
                    r.keys.clear();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                let down = event.state == ElementState::Pressed;
                if matches!(self.phase, Phase::NeedRom(_)) && down && code == KeyCode::KeyO {
                    if let Some(p) = rfd::FileDialog::new().add_filter("GBA ROM", &["gba", "agb", "bin"]).pick_file() {
                        match std::fs::read(&p) {
                            Ok(b) => self.begin_extract(b),
                            Err(e) => self.phase = Phase::NeedRom(Some(format!("cannot read {}: {e}", p.display()))),
                        }
                    }
                    return;
                }
                if down && code == KeyCode::F11 {
                    if let Some(w) = &self.window {
                        w.set_fullscreen(if w.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
                    }
                    return;
                }
                let Phase::Ready(r) = &mut self.phase else { return };
                if down && (code == KeyCode::F5 || r.menu.open) {
                    let name = format!("{code:?}");
                    if code == KeyCode::F5 && !r.menu.waiting {
                        r.menu.toggle();
                    } else {
                        match r.menu.key(&name, &mut r.bindings) {
                            wf_app::rebind::MenuResult::Changed => {
                                let _ = std::fs::write(&r.bindings_path, r.bindings.to_json());
                            }
                            wf_app::rebind::MenuResult::Closed | wf_app::rebind::MenuResult::None => {}
                        }
                    }
                    r.app.paused = r.menu.open;
                    r.keys.clear();
                    return;
                }
                if r.menu.open {
                    return;
                }
                if down && !event.repeat {
                    match code {
                        KeyCode::Escape => r.app.paused = !r.app.paused,
                        KeyCode::F1 => {
                            r.app.mode = match r.app.mode {
                                ViewMode::Pc { .. } => ViewMode::Gba { scale: 0 },
                                ViewMode::Gba { .. } => ViewMode::Pc { zoom: 0 },
                            }
                        }
                        KeyCode::F2 => {
                            if let ViewMode::Pc { zoom } = r.app.mode {
                                r.app.mode = ViewMode::Pc { zoom: (zoom + 1) % 7 };
                            }
                        }
                        KeyCode::F3 => r.stats = !r.stats,
                        _ => {}
                    }
                }
                r.keys.set(&format!("{code:?}"), down);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_micros(4000)));
    }
}

/// `--headless`: no window, no event loop, no audio device (the driver still runs muted; its commands are
/// logged). Extract if needed, step `--frames` (default 120; with `--spectate` and no `--frames`: forever) with
/// the `--input` script (`R60,D40,RU10`: letters R L U D A B S(tart) E(select), then frames), render a
/// `--size` window-sized frame on demand, write `--shot` (PPM). `--spectate ADDR` serves a read-only viewer
/// (http://ADDR/) and paces the simulation in real time; `--realtime` paces without a viewer.
fn headless(a: &Args) -> Result<(), String> {
    let t0 = Instant::now();
    if a.reextract || !content_ok(&a.content) {
        let rom = match &a.rom {
            Some(p) => std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
            None => std::fs::read(a.data.join("rom.gba")).map_err(|_| "no content and no ROM (pass the ROM path)".to_string())?,
        };
        let rx = spawn_extract(rom, a.content.clone(), a.data.clone());
        loop {
            match rx.recv().map_err(|e| e.to_string())? {
                Progress::Step(s, _) => eprintln!("extracting {s}"),
                Progress::Done(s) => {
                    eprintln!("extracted + written in {s:.2} s");
                    break;
                }
                Progress::Failed(e) => return Err(e),
            }
        }
    }
    let packs = load_packs(&a.content).map_err(|e| e.to_string())?;
    let bank = wf_app::wf_content::audio::load_bank(&packs).ok().map(|b| &*Box::leak(Box::new(b)));
    let mut app = make_app(a, packs)?;
    app.mode = a.mode;
    let mut s = wf_app::session::Session::new(app, bank);
    eprintln!("content ready + world loaded in {:.0} ms (headless, muted)", t0.elapsed().as_secs_f64() * 1e3);
    let mut script: Vec<u16> = Vec::new();
    for item in a.input.split(',').filter(|s| !s.is_empty()) {
        let k = item.trim_end_matches(|c: char| c.is_ascii_digit());
        let n: usize = item[k.len()..].parse().unwrap_or(1);
        use wf_view::buttons as b;
        let bits = k.chars().fold(0, |acc, c| {
            acc | match c {
                'R' => b::RIGHT,
                'L' => b::LEFT,
                'U' => b::UP,
                'D' => b::DOWN,
                'A' => b::A,
                'B' => b::B,
                'S' => b::START,
                'E' => b::SELECT,
                _ => 0,
            }
        });
        script.extend(std::iter::repeat_n(bits, n));
    }
    let spect = match &a.spectate {
        Some(addr) => Some(spectator::serve(addr).map_err(|e| format!("spectator on {addr}: {e}"))?),
        None => None,
    };
    let frames = a.frames.unwrap_or(if spect.is_some() { u64::MAX } else { 120 });
    let realtime = a.realtime || spect.is_some();
    let period = Duration::from_secs_f64(1.0 / wf_view::GBA_FPS);
    let t1 = Instant::now();
    let mut next = Instant::now();
    let mut f = 0u64;
    while f < frames {
        s.step(&[script.get(f as usize).copied().unwrap_or(0)]);
        f += 1;
        if let Some(sp) = &spect {
            sp.serve_pending(&mut s);
        }
        if realtime {
            next += period;
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(d);
            }
        }
    }
    let step_ms = t1.elapsed().as_secs_f64() * 1e3 / frames.max(1) as f64;
    let (ww, wh) = a.size;
    let t2 = Instant::now();
    let mut shot = s.screenshot(0, ww, wh);
    if a.show_controls {
        let mut m = wf_app::rebind::RebindMenu::default();
        m.toggle();
        m.cursor = 4;
        m.waiting = true;
        let b = std::fs::read_to_string(a.data.join("bindings.json")).ok().and_then(|s| Bindings::from_json(&s)).unwrap_or_default();
        m.draw(&s.app.packs, &mut shot.px, shot.w as usize, shot.h as usize, &b);
    }
    eprintln!(
        "{frames} steps ({step_ms:.4} ms/step), render {}x{} @{}x: {:.2} ms, state {:016x}, sfx voices busy {}, audio log {:?}",
        shot.w,
        shot.h,
        shot.scale,
        t2.elapsed().as_secs_f64() * 1e3,
        s.state_hash(),
        s.sfx_voices(),
        s.audio_log
    );
    if let Some(p) = &a.shot {
        std::fs::write(p, shot.ppm(1)).map_err(|e| e.to_string())?;
        eprintln!("wrote {}", p.display());
    }
    Ok(())
}

/// Read-only viewer for a headless session: a tiny HTTP server (std only). `GET /` = a page that polls
/// `/frame.bmp?w=&h=` (rendered by the session thread between steps, from the current state — the simulation
/// is never touched) and `/state` (frame, state hash, audio log tail).
mod spectator {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    pub enum Req {
        Frame(u32, u32, mpsc::Sender<Vec<u8>>),
        State(mpsc::Sender<Vec<u8>>),
    }

    pub struct Spectator {
        rx: mpsc::Receiver<Req>,
    }

    const PAGE: &str = "<!doctype html><meta charset=utf-8><title>Buu's Fury spectator</title>\
<style>html,body{margin:0;height:100%;background:#000;color:#ccc;font:12px monospace}img{image-rendering:pixelated;width:100vw;height:100vh;object-fit:contain}#s{position:fixed;left:8px;bottom:8px}</style>\
<img id=i><div id=s></div><script>const d=devicePixelRatio||1;async function f(){const w=Math.floor(innerWidth*d),h=Math.floor(innerHeight*d);\
const b=await (await fetch(`/frame.bmp?w=${w}&h=${h}`)).blob();const u=URL.createObjectURL(b);i.onload=()=>URL.revokeObjectURL(u);i.src=u;\
s.textContent=await (await fetch('/state')).text();requestAnimationFrame(f)}f()</script>";

    pub fn serve(addr: &str) -> std::io::Result<Spectator> {
        let l = TcpListener::bind(addr)?;
        eprintln!("spectator: http://{addr}/ (read-only view)");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let mut c = c;
                    let mut buf = [0u8; 2048];
                    let n = c.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let q = |k: &str| {
                        path.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{k}="))).and_then(|v| v.parse().ok()).unwrap_or(960u32)
                    };
                    let (body, ct) = if path.starts_with("/frame.bmp") {
                        let (rt, rr) = mpsc::channel();
                        let _ = tx.send(Req::Frame(q("w").clamp(240, 7680), q("h").clamp(160, 4320), rt));
                        (rr.recv().unwrap_or_default(), "image/bmp")
                    } else if path.starts_with("/state") {
                        let (rt, rr) = mpsc::channel();
                        let _ = tx.send(Req::State(rt));
                        (rr.recv().unwrap_or_default(), "application/json")
                    } else {
                        (PAGE.as_bytes().to_vec(), "text/html")
                    };
                    let _ = c.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    );
                    let _ = c.write_all(&body);
                });
            }
        });
        Ok(Spectator { rx })
    }

    impl Spectator {
        /// Answer queued viewer requests from the session thread (between steps).
        pub fn serve_pending(&self, s: &mut wf_app::session::Session) {
            while let Ok(r) = self.rx.try_recv() {
                match r {
                    Req::Frame(w, h, tx) => {
                        let _ = tx.send(s.screenshot(0, w, h).bmp());
                    }
                    Req::State(tx) => {
                        let tail: Vec<String> = s.audio_log.iter().rev().take(4).map(|(f, e)| format!("\"{f}: {e:?}\"")).collect();
                        let j = format!(
                            "{{\"frame\":{},\"state_hash\":\"{:016x}\",\"sfx_voices\":{},\"audio\":[{}]}}",
                            s.frame(),
                            s.state_hash(),
                            s.sfx_voices(),
                            tail.join(",")
                        );
                        let _ = tx.send(j.into_bytes());
                    }
                }
            }
        }
    }
}

fn main() {
    let args = parse_args();
    if args.headless {
        if let Err(e) = headless(&args) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let mut shell = Shell { phase: Phase::NeedRom(None), window: None, surface: None, last: Instant::now(), ui: Vec::new(), args };
    let stored_rom = shell.args.data.join("rom.gba");
    if let Some(p) = shell.args.rom.clone() {
        match std::fs::read(&p) {
            Ok(b) => shell.begin_extract(b),
            Err(e) => shell.phase = Phase::NeedRom(Some(format!("cannot read {}: {e}", p.display()))),
        }
    } else if content_ok(&shell.args.content) && !shell.args.reextract {
        shell.boot();
    } else if let Ok(b) = std::fs::read(&stored_rom) {
        shell.begin_extract(b);
    }
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    let _ = el.run_app(&mut shell);
}
