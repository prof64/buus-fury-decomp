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
//! Replays / rewind / save states / training (`wf_app::control`, docs/replays.md): hold ` rewind, hold Tab fast
//! forward, P pause, N frame advance, -/= speed, [ ] seek, Home start, F6-F9 load / Shift+F6-F9 save state, F10
//! replays, B take over, I input display, F4 training overlay, G ghost; rebindable in `<data dir>/hotkeys.json`.
//! Headless: `--replay FILE` verifies and plays a recording, `--record FILE` writes the run's recording (battles and story mode).
//! Headless frame dumps: `--dump DIR [--dump-every N]` writes `DIR/fNNNNNN.png` every N frames (default 10) at `--size`,
//! and `DIR/launcher_KKK.png` after each `--launcher-keys` key (off by default).
//! Mods (engine/docs/modding.md): every folder in `<content>/mods/` loads; `--mod DIR` (repeatable) adds a mod
//! folder from anywhere (e.g. `--mod engine/mods-examples/new-skill`), `--no-mods` loads only those given;
//! `--modcheck [DIR...]` checks mod folders (default: `<content>/mods/*`) and exits (1 on problems).
//! `--new-mod NAME [--dir PARENT]` makes a new, working mod folder to start from (default: `<content>/mods/NAME`).
//! Launcher (`wf_app::launcher`): with no mode flag (`--story`, `--arena`/`--enemy`/`--fighter`, `--area`/`--demo`,
//! `--online`, `--net-*`, `--replay`) the game opens on the launcher (Story / Battle / Online / Replays / Mods /
//! Settings); `--launcher` forces it, `--no-launcher` skips it. Shift+Esc in a game returns to it. Settings live in
//! `<data dir>/settings.json`, story save slots in `<data dir>/saves/slot{1,2,3}.sav`. Headless: `--launcher`
//! renders it (`--shot`), `--launcher-keys ArrowDown,Enter` drives it first (a chosen game then runs as usual).
//! Link play (engine/docs/netcode.md "Link play: playing it"): `--link host:ROOM` / `--link join:ROOM` opens the title
//! with Multiplayer -> Dojo / Trading Post going online: the "Waiting for a friend..." box joins room ROOM (per page)
//! on the `--signal` server; `--net-name`, `--net-key` and the `--net-*` network options apply. Scripted (headless
//! tests, tools/link/twoproc.sh): `--link-bot dojo|trade` plays the pages itself (`wf_app::link::LinkBot`; the
//! host / guest from `--link`), `--link-report FILE` writes each flow's end as JSON (default with a bot:
//! `<data>/link_report.json`), `--link-quick` the Dojo's short test rules (40 s rounds, at most 3).

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod online;
mod replays;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use wf_app::audio::AudioEngine;
use wf_app::demo::DemoConfig;
use wf_app::control::Control;
use wf_app::hotkeys::{Hotkeys, Mods};
use wf_app::input::{Bindings, KeyState};
use wf_app::replay_menu::ReplayMenu;
use wf_app::wf_content::Packs;
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
    /// Settings "original pacing" (story mode holds the picture for the GBA's load lengths)
    original_pacing: bool,
    /// Settings "Auto zoom: max" (false = steady)
    auto_zoom_max: bool,
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
    /// headless: draw the online lobby menu over the shot (`main`, `host`, `room`)
    show_online: Option<String>,
    demo: bool,
    /// `--story [W-A]`: story mode (field + scripts) from a new game at area W-A (default 1-1)
    story: Option<String>,
    /// `--mod DIR` (repeatable): extra mod folders, loaded with `<content>/mods/*`
    mods: Vec<PathBuf>,
    /// `--no-mods`: ignore `<content>/mods/` (only `--mod` folders load)
    no_mods: bool,
    /// `--modcheck [DIR...]`: check mod folders (default: `<content>/mods/*`) and exit (`wf_app::modcheck`)
    modcheck: bool,
    /// `--dev`: hot reload of the loaded mods' files between frames (`wf_app::dev`; modding.md §7)
    dev: bool,
    /// `--new-mod NAME [--dir PARENT]`: scaffold a new mod folder (default parent: `<content>/mods/`) and exit
    /// (`wf_app::newmod`)
    new_mod: Option<String>,
    new_mod_dir: Option<PathBuf>,
    arena: String,
    enemy: String,
    count: u32,
    fighter: String,
    form: Option<i32>,
    level: Option<i32>,
    seed: u32,
    /// headless: play (and verify) this recording instead of `--input`
    replay: Option<PathBuf>,
    /// headless: write the run's recording here
    record: Option<PathBuf>,
    /// online multiplayer / sync test (`--net-*`, wf_net::cli::HELP)
    net: wf_net::cli::NetArgs,
    /// `--online`: open the online lobby menu at start (F12 any time)
    online: bool,
    /// `--signal` was given (overrides the lobby menu's saved server)
    signal_given: bool,
    /// a mode flag was given on the command line (no launcher unless `--launcher`)
    cli_mode: bool,
    /// `--launcher` (Some(true)) / `--no-launcher` (Some(false))
    launcher: Option<bool>,
    /// headless: keys fed to the launcher (`--launcher-keys ArrowDown,Enter`)
    launcher_keys: Vec<String>,
    /// `--gba` / `--zoom` given (else the launcher settings' view)
    view_given: bool,
    /// launcher: story mode with the save slots (None = new game, Some(i) = continue slot i)
    story_launch: Option<Option<usize>>,
    /// launcher: this battle instead of the `--arena` / `--enemy` ... flags
    battle_cfg: Option<wf_app::battle::BattleConfig>,
    /// launcher: open the replay list once the game is up
    open_replays: bool,
    /// launcher: story mode on the game's title screen
    title_launch: bool,
    /// `--link host:ROOM|join:ROOM`: the title with online link play (host?, room)
    link: Option<(bool, String)>,
    /// `--link-bot dojo|trade`: the scripted link player
    link_bot: Option<wf_app::link::LinkKind>,
    /// `--link-report FILE`
    link_report: Option<PathBuf>,
    /// `--link-quick`: the Dojo's short test rules
    link_quick: bool,
    /// headless: `--dump DIR` writes a PNG every `--dump-every N` frames (default 10) and after each launcher key
    dump: Option<PathBuf>,
    dump_every: u64,
}

/// Running from a macOS `.app` bundle (Finder gives no terminal, which would otherwise mean "automation, headless").
fn in_app_bundle() -> bool {
    std::env::current_exe().ok().is_some_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
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
        original_pacing: false,
        auto_zoom_max: false,
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
        show_online: None,
        demo: false,
        story: None,
        mods: Vec::new(),
        no_mods: false,
        modcheck: false,
        dev: false,
        new_mod: None,
        new_mod_dir: None,
        arena: "forest".into(),
        enemy: "t68".into(),
        count: 1,
        fighter: "goku".into(),
        form: None,
        level: None,
        seed: 1,
        replay: None,
        record: None,
        net: Default::default(),
        online: false,
        signal_given: false,
        cli_mode: false,
        launcher: None,
        launcher_keys: Vec::new(),
        view_given: false,
        story_launch: None,
        battle_cfg: None,
        open_replays: false,
        title_launch: false,
        link: None,
        link_bot: None,
        link_report: None,
        link_quick: false,
        dump: None,
        dump_every: 10,
    };
    if a.headless && in_app_bundle() && std::env::var_os("WF_HEADLESS").is_none() && std::env::var_os("WF_AGENT").is_none() && std::env::var_os("CI").is_none() {
        a.headless = false;
    }
    if a.headless {
        a.mute = true;
    }
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut content_given = false;
    while i < v.len() {
        let next = |i: &mut usize| {
            *i += 1;
            v.get(*i).cloned().unwrap_or_default()
        };
        match v[i].as_str() {
            "--content" => {
                a.content = PathBuf::from(next(&mut i));
                content_given = true;
            }
            "--data" => {
                a.data = PathBuf::from(next(&mut i));
                // (the content dir follows the data dir unless `--content` names one, in either order)
                if !content_given {
                    a.content = a.data.join("content");
                }
            }
            "--launcher" => a.launcher = Some(true),
            "--no-launcher" => a.launcher = Some(false),
            "--launcher-keys" => a.launcher_keys = next(&mut i).split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
            "--area" => {
                a.area = next(&mut i);
                a.demo = true;
                a.cli_mode = true;
            }
            "--demo" => {
                a.demo = true;
                a.cli_mode = true;
            }
            "--story" => {
                a.cli_mode = true;
                let v = v.get(i + 1).filter(|n| wf_app::story::parse_area(n).is_some()).cloned();
                if v.is_some() {
                    i += 1;
                }
                a.story = Some(v.unwrap_or_else(|| "1-1".into()));
            }
            "--arena" => {
                a.arena = next(&mut i);
                a.cli_mode = true;
            }
            "--enemy" => {
                a.enemy = next(&mut i);
                a.cli_mode = true;
            }
            "--count" => a.count = next(&mut i).parse().unwrap_or(1),
            "--fighter" => {
                a.fighter = next(&mut i);
                a.cli_mode = true;
            }
            "--form" => a.form = next(&mut i).parse().ok(),
            "--level" => a.level = next(&mut i).parse().ok(),
            "--seed" => a.seed = next(&mut i).parse().unwrap_or(1),
            "--players" => a.players = next(&mut i).parse().unwrap_or(1),
            "--gba" => {
                a.mode = ViewMode::Gba { scale: 0 };
                a.view_given = true;
            }
            "--zoom" => {
                a.mode = ViewMode::Pc { zoom: next(&mut i).parse().unwrap_or(0) };
                a.view_given = true;
            }
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
            "--show-online" => a.show_online = Some(next(&mut i)),
            "--frames" => a.frames = next(&mut i).parse().ok(),
            "--replay" => {
                a.replay = Some(PathBuf::from(next(&mut i)));
                a.cli_mode = true;
            }
            "--record" => a.record = Some(PathBuf::from(next(&mut i))),
            "--shot" => a.shot = Some(PathBuf::from(next(&mut i))),
            "--mod" => a.mods.push(PathBuf::from(next(&mut i))),
            "--modcheck" => {
                a.modcheck = true;
                while v.get(i + 1).is_some_and(|s| !s.starts_with("--")) {
                    a.mods.push(PathBuf::from(next(&mut i)));
                }
            }
            "--no-mods" => a.no_mods = true,
            "--new-mod" => a.new_mod = Some(next(&mut i)),
            "--dir" => a.new_mod_dir = Some(PathBuf::from(next(&mut i))),
            "--dev" => a.dev = true,
            "-h" | "--help" => {
                println!("{}", include_str!("main.rs").lines().take(4).map(|l| l.trim_start_matches("//! ")).collect::<Vec<_>>().join("\n"));
                println!("{}", wf_net::cli::HELP);
                std::process::exit(0);
            }
            "--online" => {
                a.online = true;
                a.cli_mode = true;
            }
            "--link" => {
                let v = next(&mut i);
                let (role, room) = v.split_once(':').unwrap_or(("join", v.as_str()));
                a.link = Some((role == "host", room.trim().to_ascii_uppercase()));
                a.title_launch = true;
                a.cli_mode = true;
            }
            "--link-bot" => {
                a.link_bot = Some(if next(&mut i) == "trade" { wf_app::link::LinkKind::Trade } else { wf_app::link::LinkKind::Dojo });
            }
            "--link-report" => a.link_report = Some(PathBuf::from(next(&mut i))),
            "--link-quick" => a.link_quick = true,
            "--dump" => a.dump = Some(PathBuf::from(next(&mut i))),
            "--dump-every" => a.dump_every = next(&mut i).parse().unwrap_or(10).max(1),
            s if s.starts_with("--net") || matches!(s, "--pvp" | "--pvp-enemies" | "--signal" | "--room") => {
                a.cli_mode = true;
                a.signal_given |= s == "--signal";
                a.net.flag(s, &mut || next(&mut i));
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
    if let (true, Some((host, room))) = (a.title_launch, &a.link) {
        return wf_app::link::link_title_app(packs, &read_slots(&a.data), a.seed, link_config(a, *host, room));
    }
    if a.title_launch {
        return wf_app::launcher::title_app(packs, &read_slots(&a.data), a.seed);
    }
    if let Some(slot) = a.story_launch {
        return wf_app::launcher::story_app(packs, &read_slots(&a.data), slot, a.seed);
    }
    if let Some(cfg) = &a.battle_cfg {
        return App::battle(packs, cfg.clone());
    }
    if let Some(area) = &a.story {
        let at = wf_app::story::parse_area(area).ok_or_else(|| format!("--story: bad area {area} (W-A)"))?;
        return wf_app::story::app(packs, at, vec![], a.seed);
    }
    if a.demo {
        let cfg = DemoConfig { area: a.area.clone(), players: a.players, ..Default::default() };
        return App::demo(packs, &cfg).map_err(|e| e.to_string());
    }
    let cfg = wf_app::battle::config(&a.arena, &a.enemy, a.count, &a.fighter, a.form, a.level, a.players, a.seed);
    if let wf_net::cli::NetMode::SyncTest(d) = a.net.mode {
        // the local sync test: wf-net's Sim, rendered as usual
        let data = wf_app::battle::game_data(&packs)?;
        return Ok(App::new(packs, Box::new(wf_net::cli::synctest_sim(&a.net, data, &cfg, d))));
    }
    // (online: the local battle runs until the lobby (online.rs) starts the match)
    App::battle(packs, cfg)
}

/// the mod folders this run loads: `<content>/mods/*` (unless `--no-mods`, minus the ones disabled in Settings), then
/// the `--mod` folders
fn mod_list(a: &Args) -> Vec<PathBuf> {
    let off = load_settings(&a.data).disabled_mods;
    let mut mods = if a.no_mods { Vec::new() } else { wf_app::mods::mod_dirs(&a.content.join("mods")) };
    mods.retain(|m| !m.file_name().is_some_and(|n| off.iter().any(|o| o.as_str() == n.to_string_lossy())));
    mods.extend(a.mods.iter().cloned());
    mods
}

/// `--dev`: the watcher over this run's mods (None without `--dev`)
fn dev_watch(a: &Args) -> Option<wf_app::dev::Dev> {
    a.dev.then(|| {
        let mods = mod_list(a);
        eprintln!("dev mode: watching {} mod folder(s) for changes (hot reload between frames)", mods.len());
        wf_app::dev::Dev::new(&a.content.join("base"), &mods)
    })
}

/// `--dev`: a changed mod file -> checked, reloaded and the scene restarted (or the old content kept with the
/// checker's messages). True when the scene was rebuilt (the caller makes a fresh `Control`).
fn dev_poll(dev: &mut Option<wf_app::dev::Dev>, a: &Args, app: &mut App) -> bool {
    let Some(d) = dev else { return false };
    match d.poll() {
        wf_app::dev::Reload::Unchanged => false,
        wf_app::dev::Reload::Rejected(msg) => {
            eprintln!("dev reload: mod errors - keeping the running content:\n{msg}");
            false
        }
        wf_app::dev::Reload::Reloaded(packs) => match wf_app::dev::restart(app, packs, |p| make_app(a, p)) {
            Ok(new) => {
                *app = new;
                eprintln!("dev reload: content reloaded, scene restarted");
                true
            }
            Err(e) => {
                eprintln!("dev reload: the scene could not start with the new content ({e}) - keeping the running one");
                false
            }
        },
    }
}

fn load_packs(a: &Args) -> wf_app::wf_content::Result<Packs> {
    let mods = mod_list(a);
    let packs = wf_app::mods::load(&a.content.join("base"), &mods)?;
    if packs.packs.len() > 1 {
        eprintln!("content: {}", wf_app::mods::describe(&packs).join(", "));
    }
    for w in &packs.warnings {
        eprintln!("content: {w}");
    }
    Ok(packs)
}

fn start_audio(packs: &Packs, gain: f32) -> Result<(cpal::Stream, mpsc::Sender<AudioEvent>), String> {
    let bank = wf_app::wf_content::audio::load_bank(packs).map_err(|e| e.to_string())?;
    let bank: &'static _ = Box::leak(Box::new(bank));
    let dev = cpal::default_host().default_output_device().ok_or("no audio output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let rate = cfg.sample_rate().0;
    let ch = cfg.channels() as usize;
    let (tx, rx) = mpsc::channel::<AudioEvent>();
    let mut eng = AudioEngine::new(bank, rate);
    eng.gain *= gain;
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
    /// replays, rewind, save states, training tools
    ctl: Control,
    replay_menu: ReplayMenu,
    store: replays::Store,
    mods: Mods,
    /// the online lobby menu / room / match switching
    online: online::Online,
    /// story mode: the save slots' generation last written to disk
    saves_gen: u32,
    /// the window size of the picture last presented, and whether an input / window event came since: a redraw
    /// with no new step and neither of those presents nothing (the loop wakes every 4 ms, the sim steps every
    /// 16.7 ms - three of four wake-ups had rendered and blitted the same picture again)
    shown: (u32, u32),
    dirty: bool,
    /// `--dev`: the mod file watcher (hot reload between frames)
    dev: Option<wf_app::dev::Dev>,
}

// ------------------------------------------------------------------ launcher support (settings, save slots)

fn load_settings(data: &Path) -> wf_app::launcher::Settings {
    std::fs::read_to_string(data.join("settings.json")).ok().and_then(|s| wf_app::launcher::Settings::from_json(&s)).unwrap_or_default()
}

fn save_settings(data: &Path, s: &wf_app::launcher::Settings) {
    let _ = std::fs::create_dir_all(data);
    let _ = std::fs::write(data.join("settings.json"), s.to_json());
}

/// `--link`: the link pages' rooms on the signalling server (`ROOM-DOJO` / `ROOM-TRADE`), this player's stable link
/// id (`<data>/link_id`: the Trading Post's peers across reconnections), the save folder
fn link_config(a: &Args, host: bool, room: &str) -> wf_app::link::LinkConfig {
    let id_path = a.data.join("link_id");
    let link_id = match std::fs::read_to_string(&id_path) {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => {
            let id = format!("{:016x}", wf_net::cli::entropy());
            let _ = std::fs::create_dir_all(&a.data);
            let _ = std::fs::write(&id_path, &id);
            id
        }
    };
    let net = a.net.clone();
    let room = room.to_string();
    let name = if a.net.name.is_empty() { if host { "host".to_string() } else { "guest".to_string() } } else { a.net.name.clone() };
    wf_app::link::LinkConfig {
        connect: Box::new(move |kind| {
            let page = if kind == wf_app::link::LinkKind::Dojo { "DOJO" } else { "TRADE" };
            let url = wf_net::lobby::room_url_auth(&net.signal, &format!("{room}-{page}"), "", &net.key);
            let t = wf_net::cli::transport(&net, &url)?;
            Ok((t, if host { wf_net::lobby::Role::Host } else { wf_net::lobby::Role::Player }))
        }),
        clock: wf_net::cli::clock(),
        link_id,
        name,
        saves_dir: Some(a.data.join("saves")),
        rules: if a.link_quick {
            wf_net::dojo::DojoRules { round_frames: 2400, between_frames: 60, max_rounds: 3, ..Default::default() }
        } else {
            Default::default()
        },
        seed: a.seed,
        bot: a.link_bot.map(|k| wf_app::link::LinkBot::new(k, host)),
        report: a.link_report.clone().or_else(|| a.link_bot.map(|_| a.data.join("link_report.json"))),
    }
}

fn slot_path(data: &Path, i: usize) -> PathBuf {
    // (after the games: the options block - text speed, volumes)
    if i == wf_app::launcher::SLOTS {
        return data.join("saves").join("options.sav");
    }
    data.join("saves").join(format!("slot{}.sav", i + 1))
}

/// the story save slots on disk (empty = no game), then the options block
fn read_slots(data: &Path) -> Vec<Vec<u8>> {
    (0..=wf_app::launcher::SLOTS).map(|i| std::fs::read(slot_path(data, i)).unwrap_or_default()).collect()
}

/// write the story save slots when the game saved (the in-game Save screen bumps the generation)
fn persist_saves(data: &Path, app: &App, last: &mut u32) {
    if let Some((g, files)) = wf_app::launcher::saves_of(app) {
        if g != *last {
            *last = g;
            let _ = std::fs::create_dir_all(data.join("saves"));
            for (i, b) in files.iter().enumerate() {
                let p = slot_path(data, i);
                if b.is_empty() {
                    let _ = std::fs::remove_file(&p);
                } else {
                    let _ = std::fs::write(&p, b);
                }
            }
            eprintln!("saved the story slots ({})", data.join("saves").display());
        }
    }
}

fn new_launcher(a: &Args, packs: &Packs) -> wf_app::launcher::Launcher {
    let mut l = wf_app::launcher::Launcher::new(packs, load_settings(&a.data), true);
    l.set_slots(&read_slots(&a.data));
    let off = l.settings.disabled_mods.clone();
    l.mods = wf_app::mods::mod_dirs(&a.content.join("mods"))
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .map(|name| wf_app::launcher::ModEntry { enabled: !off.contains(&name), name })
        .collect();
    l.bindings = std::fs::read_to_string(a.data.join("bindings.json")).ok().and_then(|s| Bindings::from_json(&s)).unwrap_or_default();
    l
}

/// What a launcher choice means for the next boot (false = not a game start).
fn apply_choice(a: &mut Args, l: &wf_app::launcher::Launcher, c: &wf_app::launcher::Choice) -> bool {
    use wf_app::launcher::Choice;
    let start = matches!(c, Choice::Story { .. } | Choice::Title | Choice::Battle(_) | Choice::Online | Choice::Replays);
    if !start {
        return false;
    }
    a.story_launch = None;
    a.battle_cfg = None;
    a.online = false;
    a.open_replays = false;
    a.title_launch = false;
    if !a.view_given {
        a.mode = l.settings.mode();
    }
    a.original_pacing = l.settings.original_pacing;
    a.auto_zoom_max = l.settings.auto_zoom_max;
    match c {
        Choice::Story { slot } => a.story_launch = Some(*slot),
        Choice::Title => a.title_launch = true,
        Choice::Battle(cfg) => a.battle_cfg = Some(cfg.clone()),
        Choice::Online => {
            a.battle_cfg = Some(l.battle_config());
            a.online = true;
        }
        _ => {
            a.battle_cfg = Some(l.battle_config());
            a.open_replays = true;
        }
    }
    true
}

/// open a folder in the platform's file browser (a user action in the launcher)
fn open_folder(p: &Path) {
    let _ = std::fs::create_dir_all(p);
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(p).spawn();
}

struct LauncherState {
    l: wf_app::launcher::Launcher,
    packs: Packs,
    gilrs: Option<gilrs::Gilrs>,
    /// the last drawn frame's time (the Battle page's animations: `Launcher::advance`)
    last: Instant,
}

/// gamepad buttons -> launcher key codes (d-pad, South = confirm, East = back, triggers = page)
fn pad_codes(g: &mut gilrs::Gilrs) -> Vec<&'static str> {
    use gilrs::{Button as B, EventType};
    let mut v = Vec::new();
    while let Some(ev) = g.next_event() {
        if let EventType::ButtonPressed(b, _) = ev.event {
            v.push(match b {
                B::DPadUp => "ArrowUp",
                B::DPadDown => "ArrowDown",
                B::DPadLeft => "ArrowLeft",
                B::DPadRight => "ArrowRight",
                B::South | B::Start => "Enter",
                B::East | B::Select => "Escape",
                B::LeftTrigger => "PageUp",
                B::RightTrigger => "PageDown",
                _ => continue,
            });
        }
    }
    v
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn load_hotkeys(data: &Path) -> Hotkeys {
    let p = data.join("hotkeys.json");
    std::fs::read_to_string(&p).ok().and_then(|s| Hotkeys::from_json(&s)).unwrap_or_else(|| {
        let h = Hotkeys::default();
        let _ = std::fs::create_dir_all(data);
        let _ = std::fs::write(&p, h.to_json());
        h
    })
}

enum Phase {
    NeedRom(Option<String>),
    Launcher(Box<LauncherState>),
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
    /// the launcher's Quit (exits on the next event-loop turn)
    quit: bool,
}

impl Shell {
    /// the launcher unless a mode flag was given (`--launcher` / `--no-launcher` override)
    fn want_launcher(&self) -> bool {
        self.args.launcher.unwrap_or(!self.args.cli_mode)
    }

    /// content is ready: the launcher or straight into the game
    fn start(&mut self) {
        if self.want_launcher() {
            self.enter_launcher();
        } else {
            self.boot();
        }
    }

    fn enter_launcher(&mut self) {
        match load_packs(&self.args) {
            Ok(packs) => {
                let l = new_launcher(&self.args, &packs);
                self.phase = Phase::Launcher(Box::new(LauncherState { l, packs, gilrs: gilrs::Gilrs::new().ok(), last: Instant::now() }));
            }
            Err(e) => self.phase = Phase::Failed(e.to_string()),
        }
    }

    fn launcher_key(&mut self, code: &str) {
        use wf_app::launcher::Choice;
        let Phase::Launcher(ls) = &mut self.phase else { return };
        let c = ls.l.key(code);
        match &c {
            Choice::None => {}
            Choice::Settings => {
                save_settings(&self.args.data, &ls.l.settings);
                if let Some(w) = &self.window {
                    let want = ls.l.settings.fullscreen;
                    if want != w.fullscreen().is_some() {
                        w.set_fullscreen(if want { Some(Fullscreen::Borderless(None)) } else { None });
                    }
                }
            }
            Choice::Bindings => {
                let _ = std::fs::create_dir_all(&self.args.data);
                let _ = std::fs::write(self.args.data.join("bindings.json"), ls.l.bindings.to_json());
            }
            Choice::OpenModsFolder => open_folder(&self.args.content.join("mods")),
            Choice::AddMod => {}
            Choice::Quit => self.quit = true,
            _ => {
                save_settings(&self.args.data, &ls.l.settings);
                if apply_choice(&mut self.args, &ls.l, &c) {
                    self.boot();
                }
            }
        }
    }

    fn begin_extract(&mut self, rom: Vec<u8>) {
        if let Some(e) = rom_problem(&rom) {
            self.phase = Phase::NeedRom(Some(e));
            return;
        }
        let rx = spawn_extract(rom, self.args.content.clone(), self.args.data.clone());
        self.phase = Phase::Extracting(rx, "starting".into(), 0.0);
    }

    fn boot(&mut self) {
        let packs = match load_packs(&self.args) {
            Ok(p) => p,
            Err(e) => {
                self.phase = Phase::Failed(e.to_string());
                return;
            }
        };
        for w in &packs.warnings {
            eprintln!("content: {w}");
        }
        let audio = match if self.args.mute { Err("muted".to_string()) } else { start_audio(&packs, load_settings(&self.args.data).gain()) } {
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
                app.original_pacing = self.args.original_pacing;
                app.auto_zoom.max = self.args.auto_zoom_max;
                let bpath = self.args.data.join("bindings.json");
                let bindings = std::fs::read_to_string(&bpath).ok().and_then(|s| Bindings::from_json(&s)).unwrap_or_else(|| {
                    let b = Bindings::default();
                    let _ = std::fs::create_dir_all(&self.args.data);
                    let _ = std::fs::write(&bpath, b.to_json());
                    b
                });
                let mut ctl = Control::new(&mut app, load_hotkeys(&self.args.data), now_unix());
                // a battle ends on the game's "You Win!!" / "You Lose!!" with Retry / Change fighter
                ctl.end_screen = true;
                let a = &self.args;
                let local = wf_app::battle::config(&a.arena, &a.enemy, a.count, &a.fighter, a.form, a.level, a.players, a.seed);
                let mut online = online::Online::new(a.net.clone(), &a.data, local, a.signal_given);
                if a.net.active() && !matches!(a.net.mode, wf_net::cli::NetMode::SyncTest(_)) {
                    online.start_cli(&mut app);
                } else if a.online {
                    online.toggle(&mut app);
                }
                self.phase = Phase::Ready(Box::new(Running {
                    online,
                    ctl,
                    replay_menu: ReplayMenu::default(),
                    store: replays::Store::new(&self.args.data),
                    mods: Mods::default(),
                    dev: dev_watch(&self.args),
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
                    saves_gen: 0,
                    shown: (0, 0),
                    dirty: true,
                }));
                if self.args.open_replays {
                    if let Phase::Ready(r) = &mut self.phase {
                        r.replay_menu.show(r.store.list());
                    }
                }
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
        // a battle's end screen chose "Change fighter": its recording saved, back to the launcher's Battle page
        let back = match &mut self.phase {
            Phase::Ready(r) => {
                let back = r.ctl.take_battle_select();
                if back {
                    r.ctl.finalize(&r.app);
                    replays::service(&r.store, &mut r.ctl, &mut r.app, &mut r.replay_menu);
                }
                back
            }
            _ => false,
        };
        if back {
            self.enter_launcher();
            if let Phase::Launcher(ls) = &mut self.phase {
                ls.l.show(wf_app::launcher::Screen::Battle);
            }
        }
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
                Some(None) => self.start(),
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
                r.ctl.now_unix = now_unix();
                let net = wf_net::netsim::current_status();
                // the online lobby: pump it; a starting match becomes the app's simulation
                r.online.frame(&mut r.app, &mut r.ctl);
                r.online.check_match(&r.app);
                if r.online.menu.open {
                    r.app.inputs.iter_mut().for_each(|i| *i = 0);
                }
                let live = r.app.inputs.clone();
                let stepped = r.ctl.advance(&mut r.app, dt, &live);
                if stepped > 0 {
                    r.keys.consumed();
                }
                // (after the frame's steps: a safe point for a dev reload; the restarted scene records afresh)
                if dev_poll(&mut r.dev, &self.args, &mut r.app) {
                    r.ctl = Control::new(&mut r.app, load_hotkeys(&self.args.data), now_unix());
                    r.ctl.end_screen = true;
                    r.dirty = true;
                }
                persist_saves(&self.args.data, &r.app, &mut r.saves_gen);
                if let Some((_, tx)) = &r.audio {
                    for e in r.app.take_audio() {
                        let _ = tx.send(e);
                    }
                } else if let Some(m) = &mut r.muted {
                    for e in r.app.take_audio() {
                        m.push(e);
                    }
                    // (the samples go nowhere: one buffer kept for the process, no allocation per frame)
                    thread_local!(static SINK: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) });
                    SINK.with(|b| {
                        let mut b = b.borrow_mut();
                        b.resize(((dt.min(0.25) * 48_000.0) as usize) * 2, 0.0);
                        m.render(&mut b);
                    });
                } else {
                    r.app.take_audio();
                }
                let idle = stepped == 0
                    && !r.dirty
                    && r.shown == (ww, wh)
                    && !r.app.paused
                    && !r.menu.open
                    && !r.replay_menu.open
                    && !r.online.menu.open
                    && net.is_none();
                if idle {
                    replays::service(&r.store, &mut r.ctl, &mut r.app, &mut r.replay_menu);
                    return;
                }
                r.shown = (ww, wh);
                r.dirty = false;
                let t0 = Instant::now();
                // netplay: the local player's camera
                let me = net.as_ref().and_then(|s| s.borrow().local_slot).unwrap_or(0);
                let f = r.ctl.render(&mut r.app, me, ww, wh);
                replays::service(&r.store, &mut r.ctl, &mut r.app, &mut r.replay_menu);
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
                r.replay_menu.draw(&r.app.packs, &mut r.app.renderer.fb, vw, vh);
                r.online.draw(&mut r.app, vw, vh);
                // netplay overlay: connection line, desync / disconnect alerts, result
                if let Some(s) = &net {
                    for (k, (t, alert)) in s.borrow().overlay().iter().enumerate() {
                        let y = vh as i32 - 12 - font::LINE * k as i32;
                        font::fill_rect(&mut r.app.renderer.fb, vw, vh, 2, y - 1, font::text_width(t) + 4, font::LINE + 1, 0xFF00_0000);
                        font::draw_text(&mut r.app.renderer.fb, vw, vh, 4, y, t, if *alert { 0xFFFF_5A40 } else { 0xFFE8_E8E8 });
                    }
                }
                (vw, vh, f.scale as usize, false)
            }
            Phase::Launcher(ls) => {
                let codes = ls.gilrs.as_mut().map(pad_codes).unwrap_or_default();
                let (w, h) = (240usize, 160usize);
                self.ui.clear();
                self.ui.resize(w * h, 0xFF00_0000);
                let now = Instant::now();
                ls.l.advance(now.duration_since(ls.last).as_secs_f64());
                ls.last = now;
                ls.l.draw(&ls.packs, &mut self.ui, w, h);
                for c in codes {
                    self.launcher_key(c);
                }
                if !matches!(self.phase, Phase::Launcher(_)) {
                    return; // a game started: drawn from the next frame
                }
                (w, h, ((ww as usize / w).min(wh as usize / h)).max(1), true)
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

/// `--dump`: a compact state line for the dumped frame (players, enemies, outcome, field area) - battle and story
fn obs_line(app: &App) -> Option<String> {
    let any = app.sim.as_any()?;
    let s = match any.downcast_ref::<wf_app::battle::CoreSim>() {
        Some(c) => &c.0,
        None => &any.downcast_ref::<wf_app::story::StorySim>()?.session,
    };
    let o = wf_core::observe::observe(&s.world, 0);
    let pl: Vec<_> = o
        .players
        .iter()
        .map(|p| serde_json::json!({"hp": p.hp, "hp_max": p.hp_max, "ep": p.ep, "ep_max": p.ep_max, "form": p.form, "state": p.state, "skill": p.selected_skill, "skills": p.skills, "ss": p.ss_gauge, "dead": p.dead, "x": p.actor.x, "y": p.actor.y, "facing": p.actor.facing, "lv": p.level}))
        .collect();
    let en: Vec<_> = o
        .enemies
        .iter()
        .map(|e| serde_json::json!({"name": e.name, "ty": e.ty, "hp": e.hp, "hp_max": e.hp_max, "x": e.actor.x, "y": e.actor.y, "defeated": e.defeated, "primary": e.primary}))
        .collect();
    let field = o.field.as_ref().map(|f| serde_json::json!({"area": f.area, "name": f.name, "busy": f.busy, "transition": f.transition, "zenie": f.zenie}));
    Some(
        serde_json::json!({"frame": o.frame, "outcome": format!("{:?}", s.outcome), "players": pl, "enemies": en, "shots": o.projectiles.len(), "camera": o.camera, "field": field})
            .to_string(),
    )
}

/// `--dump`: an ARGB frame as a PNG
fn write_png(path: &Path, fb: &[u32], w: usize, h: usize) {
    let rgba: Vec<u8> = fb[..w * h].iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]).collect();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(b) = wf_app::wf_content::image::encode_rgba(w as u32, h as u32, &rgba) {
        let _ = std::fs::write(path, b);
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
        if load_settings(&self.args.data).fullscreen {
            win.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let ctx = softbuffer::Context::new(win.clone()).expect("softbuffer context");
        self.surface = Some(softbuffer::Surface::new(&ctx, win.clone()).expect("surface"));
        self.window = Some(win);
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        if !matches!(ev, WindowEvent::RedrawRequested) {
            if let Phase::Ready(r) = &mut self.phase {
                r.dirty = true;
            }
        }
        match ev {
            WindowEvent::CloseRequested => {
                if let Phase::Ready(r) = &mut self.phase {
                    r.ctl.finalize(&r.app);
                    replays::service(&r.store, &mut r.ctl, &mut r.app, &mut r.replay_menu);
                }
                el.exit()
            }
            WindowEvent::ModifiersChanged(m) => {
                if let Phase::Ready(r) = &mut self.phase {
                    let s = m.state();
                    r.mods = Mods { shift: s.shift_key(), ctrl: s.control_key() || s.super_key(), alt: s.alt_key() };
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::DroppedFile(p) => {
                // a dropped replay file: import it and play it
                if let Phase::Ready(r) = &mut self.phase {
                    if p.extension().is_some_and(|x| x == "wfr" || x == "json") {
                        let res = r.store.import(&p).and_then(|id| std::fs::read(r.store.path(&id)).map_err(|e| e.to_string()).and_then(|b| r.ctl.play_replay(&mut r.app, &id, &b)));
                        if let Err(e) = res {
                            r.ctl.toast(e);
                        }
                    }
                    return;
                }
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
                if matches!(self.phase, Phase::Launcher(_)) {
                    if down {
                        self.launcher_key(&format!("{code:?}"));
                        if self.quit {
                            el.exit();
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
                // Shift+Esc: back to the launcher (the run's replay is kept)
                if down && code == KeyCode::Escape && r.mods.shift && !r.online.in_match {
                    r.ctl.finalize(&r.app);
                    replays::service(&r.store, &mut r.ctl, &mut r.app, &mut r.replay_menu);
                    persist_saves(&self.args.data, &r.app, &mut r.saves_gen);
                    self.enter_launcher();
                    return;
                }
                // the online lobby menu (F12; Esc in an online match): every key goes to it while open
                if down && (code == KeyCode::F12 || (code == KeyCode::Escape && r.online.in_match && !r.online.menu.open)) {
                    r.online.toggle(&mut r.app);
                    r.keys.clear();
                    return;
                }
                if r.online.menu.open {
                    if down {
                        r.online.key(&format!("{code:?}"), &mut r.app, &mut r.ctl);
                        if let Some(t) = event.text.as_deref() {
                            r.online.text(t);
                        }
                    }
                    r.keys.clear();
                    return;
                }
                if r.replay_menu.open {
                    if down {
                        let cmd = r.replay_menu.key(&format!("{code:?}"));
                        replays::command(&r.store, &mut r.ctl, &mut r.app, &mut r.replay_menu, cmd);
                    }
                    r.keys.clear();
                    return;
                }
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
                // replay / rewind / save state hotkeys (never a key bound to a GBA button without modifiers)
                let name = format!("{code:?}");
                let bound = r.bindings.key(&name) != 0;
                if r.ctl.key(&mut r.app, &name, r.mods, down, event.repeat, bound) {
                    if !down {
                        r.keys.set(&name, false);
                    }
                    return;
                }
                if down && !event.repeat {
                    match code {
                        KeyCode::Escape => r.app.paused = !r.app.paused,
                        // F1: Classic 240x160 <-> PC wide, kept as the launcher's View setting
                        KeyCode::F1 => {
                            r.app.mode = match r.app.mode {
                                ViewMode::Pc { .. } => ViewMode::Gba { scale: 0 },
                                ViewMode::Gba { .. } => ViewMode::Pc { zoom: 0 },
                            };
                            let classic = matches!(r.app.mode, ViewMode::Gba { .. });
                            r.ctl.toast(if classic { "Classic 240x160 (F1)" } else { "PC wide (F1)" });
                            let mut st = load_settings(&self.args.data);
                            st.view = if classic { "gba:0" } else { "pc:0" }.into();
                            save_settings(&self.args.data, &st);
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
        if self.quit {
            el.exit();
            return;
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_micros(4000)));
    }
}

/// `--headless`: no window, no event loop, no audio device (the driver still runs muted; its commands are
/// logged). Extract if needed, step `--frames` (default 120; with `--spectate` and no `--frames`: forever) with
/// the `--input` script (`R60,D40,RU10`: letters R L U D A B S(tart) E(select), l / r the shoulders, then frames), render a
/// `--size` window-sized frame on demand, write `--shot` (PPM). `--spectate ADDR` serves a read-only viewer
/// (http://ADDR/) and paces the simulation in real time; `--realtime` paces without a viewer.
fn headless(a: &mut Args) -> Result<(), String> {
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
    let mut packs = load_packs(a).map_err(|e| e.to_string())?;
    // the launcher: feed it `--launcher-keys`; a chosen game runs below, otherwise its picture is the shot
    if a.launcher == Some(true) || (!a.cli_mode && !a.launcher_keys.is_empty()) {
        let mut l = new_launcher(a, &packs);
        let mut started = false;
        for (ki, k) in a.launcher_keys.clone().into_iter().enumerate() {
            let c = l.key(&k);
            eprintln!("launcher: {k} -> {:?}", c.to_json()["kind"]);
            if let Some(d) = &a.dump {
                let mut fb = vec![0u32; 240 * 160];
                l.draw(&packs, &mut fb, 240, 160);
                write_png(&d.join(format!("launcher_{ki:03}.png")), &fb, 240, 160);
            }
            if apply_choice(a, &l, &c) {
                started = true;
                break;
            }
        }
        if !started {
            let (w, h) = (240usize, 160usize);
            let mut fb = vec![0u32; w * h];
            l.draw(&packs, &mut fb, w, h);
            eprintln!("launcher screen {:?}", l.screen);
            if let Some(p) = &a.shot {
                write_ppm(p, &fb, w, h);
                eprintln!("wrote {}", p.display());
            }
            return Ok(());
        }
        packs = load_packs(a).map_err(|e| e.to_string())?;
    }
    if a.net.active() {
        // headless netplay / sync test: real-time match with the --input script (or --net-random-input), JSON result
        let cfg = wf_app::battle::config(&a.arena, &a.enemy, a.count, &a.fighter, a.form, a.level, a.players, a.seed);
        let data = wf_app::battle::game_data(&packs)?;
        let script = wf_net::cli::parse_script(&a.input);
        let mut net = a.net.clone();
        net.record_dir.get_or_insert_with(|| a.data.join("replays").display().to_string());
        return wf_net::cli::run_headless(&net, data, &cfg, a.frames.unwrap_or(3600), &script, wf_app::battle::content_id(&packs)).map(|_| ());
    }
    let bank = wf_app::wf_content::audio::load_bank(&packs).ok().map(|b| &*Box::leak(Box::new(b)));
    let mut app = make_app(a, packs)?;
    app.mode = a.mode;
    app.original_pacing = a.original_pacing;
    app.auto_zoom.max = a.auto_zoom_max;
    let mut saves_gen = 0;
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
                // the shoulder buttons: lower case (upper R / L are the d-pad)
                'l' => b::L,
                'r' => b::R,
                _ => 0,
            }
        });
        script.extend(std::iter::repeat_n(bits, n));
    }
    let spect = match &a.spectate {
        Some(addr) => Some(spectator::serve(addr).map_err(|e| format!("spectator on {addr}: {e}"))?),
        None => None,
    };
    // replays: record this run, or play (and verify) a recording instead of the input script
    let mut ctl = Control::new(&mut s.app, Hotkeys::default(), now_unix());
    let mut replay_end = None;
    if let Some(p) = &a.replay {
        let b = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let t = Instant::now();
        let rep = ctl.verify(&s.app, &b)?;
        eprintln!(
            "verify {}: {} ({} frames, {} checkpoints, final state {:016x}) in {:.1} ms",
            p.display(),
            match rep.divergence {
                Some(d) => d.to_string(),
                None if rep.ok => "OK".into(),
                None => "could not start".into(),
            },
            rep.frames,
            rep.checkpoints,
            rep.final_hash,
            t.elapsed().as_secs_f64() * 1e3
        );
        let id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        ctl.play_replay(&mut s.app, &id, &b)?;
        replay_end = Some(wf_app::control::parse_replay(&b)?.end_frame());
    }
    let frames = a.frames.or(replay_end).unwrap_or(if spect.is_some() { u64::MAX } else { 120 });
    let mut dev = dev_watch(a);
    let realtime = a.realtime || spect.is_some();
    let period = Duration::from_secs_f64(1.0 / wf_view::GBA_FPS);
    let t1 = Instant::now();
    let mut next = Instant::now();
    let mut f = 0u64;
    while f < frames {
        let inp = [script.get(f as usize).copied().unwrap_or(0)];
        if !ctl.enabled {
            s.step(&inp); // demo world: no recording
        } else if ctl.step(&mut s.app, &inp) {
            s.after_step();
        } else {
            break; // end of the replay
        }
        f += 1;
        if let Some(d) = a.dump.as_ref().filter(|_| f % a.dump_every == 0) {
            // (as the window draws it: the replay / end-screen overlays included)
            let fit = ctl.render(&mut s.app, 0, a.size.0, a.size.1);
            write_png(&d.join(format!("f{f:06}.png")), &s.app.renderer.fb, fit.view_w as usize, fit.view_h as usize);
            if let Some(line) = obs_line(&s.app) {
                use std::io::Write;
                if let Ok(mut o) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("state.jsonl")) {
                    let _ = writeln!(o, "{{\"dump\":{f},{}", &line[1..]);
                }
            }
        }
        // (between frames: a safe point for a dev reload; the restarted scene records afresh)
        if dev_poll(&mut dev, a, &mut s.app) {
            ctl = Control::new(&mut s.app, Hotkeys::default(), now_unix());
        }
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
    persist_saves(&a.data, &s.app, &mut saves_gen);
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
    if let Some(which) = &a.show_online {
        online::preview(which, &s.app.packs, &mut shot.px, shot.w as usize, shot.h as usize);
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
    if let Some(p) = &a.record {
        let (_, r) = ctl.current_replay(&s.app).ok_or("this simulation does not record (demo world?)")?;
        let b = r.to_bytes();
        std::fs::write(p, &b).map_err(|e| e.to_string())?;
        eprintln!("recorded {} frames into {} ({} bytes)", r.inputs.len(), p.display(), b.len());
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
    let mut args = parse_args();
    if let Some(name) = args.new_mod.clone() {
        // `buus-fury --new-mod NAME [--dir PARENT]`: a new mod folder, valid and playable as it is
        let parent = args.new_mod_dir.clone().unwrap_or_else(|| args.content.join("mods"));
        match wf_app::newmod::scaffold(&parent, &name) {
            Ok(m) => {
                println!("created {}", m.dir.display());
                for f in &m.files {
                    println!("  {f}");
                }
                println!("next: buus-fury --modcheck {}   (check it)", m.dir.display());
                println!("      buus-fury --mod {}   (play it: New Game, walk right, talk to the NPC)", m.dir.display());
                println!("      its README.md and engine/docs/modding.md explain each file");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    if args.modcheck {
        // `buus-fury --modcheck [DIR...]`: the given mod folders (default: every folder in <content>/mods/) over
        // the installed base content; exit 1 on problems
        let base = args.content.join("base");
        if !base.join("pack.json").exists() {
            eprintln!("no extracted game content at {} - start the game once with your ROM (or pass --content DIR)", base.display());
            std::process::exit(2);
        }
        let mods = if args.mods.is_empty() { wf_app::mods::mod_dirs(&args.content.join("mods")) } else { args.mods.clone() };
        let r = wf_app::modcheck::check(&base, &mods);
        print!("{}", r.text());
        std::process::exit(if r.ok() { 0 } else { 1 });
    }
    if args.dev && (args.net.active() || args.online || args.replay.is_some() || args.record.is_some()) {
        eprintln!("--dev (hot reload) is for local play: not with netplay, --online, --replay or --record (a reload changes the content mid-run)");
        std::process::exit(2);
    }
    if args.headless {
        if let Err(e) = headless(&mut args) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let mut shell = Shell { phase: Phase::NeedRom(None), window: None, surface: None, last: Instant::now(), ui: Vec::new(), args, quit: false };
    let stored_rom = shell.args.data.join("rom.gba");
    if let Some(p) = shell.args.rom.clone() {
        match std::fs::read(&p) {
            Ok(b) => shell.begin_extract(b),
            Err(e) => shell.phase = Phase::NeedRom(Some(format!("cannot read {}: {e}", p.display()))),
        }
    } else if content_ok(&shell.args.content) && !shell.args.reextract {
        shell.start();
    } else if let Ok(b) = std::fs::read(&stored_rom) {
        shell.begin_extract(b);
    }
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    let _ = el.run_app(&mut shell);
}
