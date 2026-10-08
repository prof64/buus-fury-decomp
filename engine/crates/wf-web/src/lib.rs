//! Browser build of the frontend (no wasm-bindgen: plain `extern "C"` exports + a handful of host imports, like
//! `wf-wasm`). One binary serves two instances:
//!
//! * the **game instance** (main thread): content via host imports, the demo world / `wf-core`, input bindings,
//!   the software renderer → an RGBA buffer JS puts on a canvas, audio events JS forwards to the worklet;
//! * the **audio instance** (AudioWorklet): the same content's `audio/` files → `wf-audio` driver → f32 PCM.
//!
//! Host imports (module `host`): `pack_count() -> u32`, `file_size(pack, path_ptr, path_len) -> i32` (-1 =
//! missing), `file_read(pack, path_ptr, path_len, dst_ptr)`, `log(ptr, len)`, `now_ms() -> f64`.
//! Pack 0 is `base`, the others are mods in any order (the loader sorts them, see CONTENT.md).
//! See `engine/web/app/` for the JS side and `engine/docs/frontend.md` for the design.

use std::cell::RefCell;

use std::collections::VecDeque;

/// online multiplayer (wf-net): lobby + rollback session behind `wf_net_*` exports
mod net;
/// the launcher (title / story / battle / online / replays / mods / settings) before the game starts
mod launcher;
/// link play: the title's Multiplayer -> Dojo / Trading Post over online play (`{"link": {...}}`)
mod link;

use wf_app::audio::AudioEngine;
use wf_app::control::{Control, Request};
use wf_app::hotkeys::{Action, Hotkeys, Mods};
use wf_app::demo::DemoConfig;
use wf_app::input::{Bindings, KeyState};
use wf_app::wf_content::{FileSource, Packs};
use wf_app::wf_render::ViewMode;
use wf_app::App;
use wf_view::AudioEvent;

// Native builds of this crate (cargo test / check of the workspace) get inert stubs.
#[cfg(not(target_arch = "wasm32"))]
mod native_stubs {
    pub unsafe fn pack_count() -> u32 {
        0
    }
    pub unsafe fn file_size(_: u32, _: *const u8, _: usize) -> i32 {
        -1
    }
    pub unsafe fn file_read(_: u32, _: *const u8, _: usize, _: *mut u8) {}
    pub unsafe fn log(_: *const u8, _: usize) {}
    pub unsafe fn now_ms() -> f64 {
        0.0
    }
}
#[cfg(not(target_arch = "wasm32"))]
use native_stubs::*;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "host")]
extern "C" {
    fn pack_count() -> u32;
    fn file_size(pack: u32, path: *const u8, len: usize) -> i32;
    fn file_read(pack: u32, path: *const u8, len: usize, dst: *mut u8);
    fn log(ptr: *const u8, len: usize);
    fn now_ms() -> f64;
}

fn host_log(s: &str) {
    unsafe { log(s.as_ptr(), s.len()) }
}

struct HostSource(u32);

impl FileSource for HostSource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let n = unsafe { file_size(self.0, path.as_ptr(), path.len()) };
        if n < 0 {
            return None;
        }
        let mut v = vec![0u8; n as usize];
        if n > 0 {
            unsafe { file_read(self.0, path.as_ptr(), path.len(), v.as_mut_ptr()) };
        }
        Some(v)
    }
    fn exists(&self, path: &str) -> bool {
        unsafe { file_size(self.0, path.as_ptr(), path.len()) >= 0 }
    }
}

fn host_packs() -> wf_app::wf_content::Result<Packs> {
    let n = unsafe { pack_count() };
    Packs::new((0..n).map(|i| Box::new(HostSource(i)) as Box<dyn FileSource>).collect())
}

struct Game {
    app: App,
    bindings: Bindings,
    keys: Vec<KeyState>,
    pads: Vec<u16>,
    rgba: Vec<u8>,
    fb_w: u32,
    fb_h: u32,
    fb_scale: u32,
    audio: Vec<i32>,
    last_ms: f64,
    /// replays, rewind, save states, training tools (see engine/docs/replays.md)
    ctl: Control,
    requests: VecDeque<Request>,
    /// what the picture last shown was made from (window size, view mode, sim frame, input generation): a frame
    /// call with no new step and none of these changed renders nothing (`wf_app_frame` on a 120 / 144 Hz display:
    /// every other call)
    shown: Option<(u32, u32, ViewMode, u64, u32)>,
    input_gen: u32,
}

thread_local! {
    static GAME: RefCell<Option<Game>> = const { RefCell::new(None) };
    static AUDIO: RefCell<Option<AudioEngine>> = const { RefCell::new(None) };
    static AUDIO_BUF: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static STR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static BLOB: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn set_blob(b: Vec<u8>) {
    BLOB.with(|x| *x.borrow_mut() = b);
}

#[no_mangle]
pub extern "C" fn wf_blob_ptr() -> *const u8 {
    BLOB.with(|b| b.borrow().as_ptr())
}
#[no_mangle]
pub extern "C" fn wf_blob_len() -> usize {
    BLOB.with(|b| b.borrow().len())
}

unsafe fn arg_bytes(ptr: *const u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

fn set_str(s: &str) {
    STR.with(|b| *b.borrow_mut() = s.as_bytes().to_vec());
}

unsafe fn arg_str(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned()
}

#[no_mangle]
pub extern "C" fn wf_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len.max(1)];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` from [`wf_alloc`].
#[no_mangle]
pub unsafe extern "C" fn wf_dealloc(ptr: *mut u8, len: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, len.max(1), len.max(1)) });
}

#[no_mangle]
pub extern "C" fn wf_str_ptr() -> *const u8 {
    STR.with(|b| b.borrow().as_ptr())
}
#[no_mangle]
pub extern "C" fn wf_str_len() -> usize {
    STR.with(|b| b.borrow().len())
}

/// Start the game instance. `cfg` = JSON `{"area": "8-3", "players": 1, "bindings": {...}}` (all optional).
/// Returns 0, or 1 with the error text in the string buffer.
///
/// # Safety
/// `cfg_ptr`/`cfg_len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_init(cfg_ptr: *const u8, cfg_len: usize) -> u32 {
    let cfg: serde_json::Value = serde_json::from_str(&unsafe { arg_str(cfg_ptr, cfg_len) }).unwrap_or_default();
    let packs = match host_packs() {
        Ok(p) => p,
        Err(e) => {
            set_str(&e.to_string());
            return 1;
        }
    };
    for w in &packs.warnings {
        host_log(w);
    }
    let mut dc = DemoConfig::default();
    if let Some(a) = cfg["area"].as_str() {
        dc.area = a.into();
    }
    if let Some(n) = cfg["players"].as_u64() {
        dc.players = n.clamp(1, 4) as usize;
    }
    // the real battle (wf-core) unless {"demo": true} / an `area` is asked for
    let demo = cfg["demo"].as_bool().unwrap_or(false) || cfg["area"].is_string();
    // {"net": true}: the online match / sync test prepared by wf_net_lobby (net.rs)
    // {"launch": ...}: what the launcher picked (launcher.rs)
    if let Some(n) = cfg["launch"]["config"]["players"].as_array().map(|a| a.len()) {
        dc.players = n.clamp(1, 4);
    }
    let made = if cfg["link"].is_object() {
        link::link_app(&cfg, packs).unwrap_or_else(|| Err("bad link config".into()))
    } else if matches!(cfg["launch"]["kind"].as_str(), Some("story" | "title" | "battle")) {
        launcher::launch_app(&cfg, packs).unwrap_or_else(|| Err("bad launch".into()))
    } else if let Some(sim) = net::take_sim(&cfg, &packs) {
        sim.map(|s| App::new(packs, s))
    } else if let Some(area) = cfg["story"].as_str() {
        // story mode: a new game at area "W-A" (field + scripts)
        match wf_app::story::parse_area(area) {
            Some(at) => wf_app::story::app(packs, at, vec![], cfg["seed"].as_u64().unwrap_or(1) as u32),
            None => Err(format!("bad story area {area}")),
        }
    } else if demo {
        App::demo(packs, &dc).map_err(|e| e.to_string())
    } else {
        let s = |k: &str, d: &str| cfg[k].as_str().unwrap_or(d).to_string();
        let bc = wf_app::battle::config(
            &s("arena", "forest"),
            &s("enemy", "t68"),
            cfg["count"].as_u64().unwrap_or(1) as u32,
            &s("fighter", "goku"),
            cfg["form"].as_i64().map(|v| v as i32),
            cfg["level"].as_i64().map(|v| v as i32),
            dc.players,
            cfg["seed"].as_u64().unwrap_or(1) as u32,
        );
        App::battle(packs, bc)
    };
    let mut app = match made {
        Ok(a) => a,
        Err(e) => {
            set_str(&e);
            return 1;
        }
    };
    let bindings = serde_json::from_value(cfg["bindings"].clone()).unwrap_or_default();
    let hotkeys: Hotkeys = serde_json::from_value(cfg["hotkeys"].clone()).unwrap_or_default();
    let mut ctl = Control::new(&mut app, hotkeys, cfg["now"].as_f64().unwrap_or(0.0) as u64);
    // a battle ends on the game's "You Win!!" / "You Lose!!" with Retry / Change fighter
    ctl.end_screen = true;
    ctl.link_configured = cfg["link"].is_object();
    let n = dc.players;
    GAME.with(|g| {
        *g.borrow_mut() = Some(Game {
            app,
            bindings,
            keys: vec![KeyState::default(); n],
            pads: vec![0; n],
            rgba: Vec::new(),
            fb_w: 0,
            fb_h: 0,
            fb_scale: 1,
            audio: Vec::new(),
            last_ms: unsafe { now_ms() },
            shown: None,
            input_gen: 0,
            ctl,
            requests: VecDeque::new(),
        })
    });
    0
}

fn with_game<R>(f: impl FnOnce(&mut Game) -> R) -> Option<R> {
    GAME.with(|g| g.borrow_mut().as_mut().map(f))
}

/// Keyboard event (W3C `code`) for a local player.
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_key(ptr: *const u8, len: usize, down: u32, player: u32) -> u32 {
    let code = unsafe { arg_str(ptr, len) };
    with_game(|g| {
        g.input_gen = g.input_gen.wrapping_add(1);
        let bound = g.bindings.key(&code) != 0;
        if let Some(k) = g.keys.get_mut(player as usize) {
            k.set(&code, down != 0);
        }
        bound as u32
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn wf_app_blur() {
    with_game(|g| g.keys.iter_mut().for_each(|k| k.clear()));
}

/// Gamepad state for a player: standard-mapping buttons as bits, left stick axes.
#[no_mangle]
pub extern "C" fn wf_app_pad(player: u32, pressed: u32, ax: f32, ay: f32) {
    with_game(|g| {
        let v = g.bindings.pad(pressed, ax, ay);
        if let Some(p) = g.pads.get_mut(player as usize) {
            if *p != v {
                g.input_gen = g.input_gen.wrapping_add(1);
            }
            *p = v;
        }
    });
}

/// `kind` 0 = PC zoomed out (`zoom` 0 = auto), 1 = classic 240x160 (`zoom` = fixed scale, 0 = fit).
#[no_mangle]
pub extern "C" fn wf_app_set_mode(kind: u32, zoom: u32) {
    with_game(|g| g.app.mode = if kind == 1 { ViewMode::Gba { scale: zoom } } else { ViewMode::Pc { zoom } });
}

/// story mode's "original pacing" (launcher Settings; presentation only)
#[no_mangle]
pub extern "C" fn wf_app_set_pacing(on: u32) {
    with_game(|g| g.app.original_pacing = on != 0);
}

/// "PC wide (auto)": 1 = Settings "Auto zoom: max" (no cap), 0 = steady (the default)
#[no_mangle]
pub extern "C" fn wf_app_set_auto_zoom(max: u32) {
    with_game(|g| g.app.auto_zoom.max = max != 0);
}

#[no_mangle]
pub extern "C" fn wf_app_set_paused(p: u32) {
    with_game(|g| g.app.paused = p != 0);
}

/// Advance by wall time and render player `player` for a `win_w` x `win_h` device-pixel canvas. Returns the
/// number of simulation steps run; the RGBA frame is at `wf_fb_ptr()` (`wf_fb_w` x `wf_fb_h`, show at
/// `wf_fb_scale`).
#[no_mangle]
pub extern "C" fn wf_app_frame(win_w: u32, win_h: u32, player: u32) -> u32 {
    with_game(|g| {
        let now = unsafe { now_ms() };
        let dt = ((now - g.last_ms) / 1000.0).max(0.0);
        g.last_ms = now;
        for (i, inp) in g.app.inputs.iter_mut().enumerate() {
            *inp = g.keys.get(i).map(|k| k.buttons(&g.bindings)).unwrap_or(0) | g.pads.get(i).copied().unwrap_or(0);
        }
        let live = g.app.inputs.clone();
        let t0 = unsafe { now_ms() };
        let steps = g.ctl.advance(&mut g.app, dt, &live);
        if steps > 0 {
            g.app.stats.last_step_ms = (unsafe { now_ms() } - t0) / steps as f64;
        }
        if steps > 0 {
            g.keys.iter_mut().for_each(|k| k.consumed());
        }
        for e in g.app.take_audio() {
            g.audio.extend_from_slice(&e.encode());
        }
        let key = (win_w, win_h, g.app.mode, g.app.sim.frame(), g.input_gen);
        if steps == 0 && !g.app.paused && g.shown == Some(key) {
            return steps;
        }
        g.shown = Some(key);
        let t0 = unsafe { now_ms() };
        let f = g.ctl.render(&mut g.app, player as usize, win_w, win_h);
        g.app.stats.last_render_ms = unsafe { now_ms() } - t0;
        let fb = &g.app.renderer.fb;
        g.rgba.resize(fb.len() * 4, 0);
        for (o, &p) in g.rgba.chunks_exact_mut(4).zip(fb.iter()) {
            o[0] = (p >> 16) as u8;
            o[1] = (p >> 8) as u8;
            o[2] = p as u8;
            o[3] = 255;
        }
        g.fb_w = f.view_w;
        g.fb_h = f.view_h;
        g.fb_scale = f.scale;
        for e in g.app.renderer.errors.drain(..) {
            host_log(&e);
        }
        steps | 1 << 30
    })
    .unwrap_or(0)
}

/// Simulation frame number.
#[no_mangle]
pub extern "C" fn wf_app_frame_no() -> f64 {
    with_game(|g| g.app.sim.frame() as f64).unwrap_or(0.0)
}

#[no_mangle]
pub extern "C" fn wf_fb_ptr() -> *const u8 {
    with_game(|g| g.rgba.as_ptr()).unwrap_or(std::ptr::null())
}
#[no_mangle]
pub extern "C" fn wf_fb_w() -> u32 {
    with_game(|g| g.fb_w).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn wf_fb_h() -> u32 {
    with_game(|g| g.fb_h).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn wf_fb_scale() -> u32 {
    with_game(|g| g.fb_scale).unwrap_or(1)
}

/// Pending audio events as i32 quadruples (see `wf_view::AudioEvent::encode`); call `wf_app_audio_clear` after
/// forwarding them.
#[no_mangle]
pub extern "C" fn wf_app_audio_ptr() -> *const i32 {
    with_game(|g| g.audio.as_ptr()).unwrap_or(std::ptr::null())
}
#[no_mangle]
pub extern "C" fn wf_app_audio_len() -> usize {
    with_game(|g| g.audio.len()).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn wf_app_audio_clear() {
    with_game(|g| g.audio.clear());
}

/// Current bindings as JSON in the string buffer.
#[no_mangle]
pub extern "C" fn wf_app_bindings() {
    let s = with_game(|g| g.bindings.to_json()).unwrap_or_default();
    set_str(&s);
}

/// Rebind GBA button `button` (name) to key `code`.
///
/// # Safety
/// Pointers must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_rebind(bp: *const u8, bl: usize, cp: *const u8, cl: usize) {
    let (b, c) = unsafe { (arg_str(bp, bl), arg_str(cp, cl)) };
    with_game(|g| g.bindings.rebind_key(&b, &c));
}

#[no_mangle]
pub extern "C" fn wf_app_reset_bindings() {
    with_game(|g| g.bindings = Bindings::default());
}

/// Story mode's state for tests / tools (string buffer): `{"area": [world, area], "p": [x, y, facing], "zenie",
/// "play_frames", "menu", "options": [text speed, sfx, music]}`, or `null` outside story mode.
#[no_mangle]
pub extern "C" fn wf_app_story_info() {
    let s = with_game(|g| {
        let Some(st) = g.app.sim.as_any().and_then(|a| a.downcast_ref::<wf_app::story::StorySim>()) else {
            return "null".to_string();
        };
        let w = st.world();
        let area = w.field.as_ref().map(|f| [f.area.world, f.area.area]);
        let p = w.player_idx(0).map(|i| {
            let a = &w.pl_ref(i).a;
            [a.px(), a.py(), a.facing as i32]
        });
        serde_json::json!({
            "area": area,
            "p": p,
            "zenie": w.story.zenie,
            "play_frames": w.story.play_frames,
            "menu": wf_core::menu::active(w),
            "options": [w.menu.settings.text_speed, w.menu.settings.sfx, w.menu.settings.music],
        })
        .to_string()
    })
    .unwrap_or_else(|| "null".into());
    set_str(&s);
}

/// Stats JSON in the string buffer.
#[no_mangle]
pub extern "C" fn wf_app_stats() {
    let s = with_game(|g| {
        let s = g.app.stats;
        format!(
            "{{\"frame\":{},\"steps\":{},\"step_ms\":{:.3},\"render_ms\":{:.3},\"view\":[{},{}],\"scale\":{},\"state_hash\":\"{:016x}\"}}",
            g.app.sim.frame(),
            s.steps,
            s.last_step_ms,
            s.last_render_ms,
            g.fb_w,
            g.fb_h,
            g.fb_scale,
            g.app.sim.state_hash()
        )
    })
    .unwrap_or_default();
    set_str(&s);
}

// ---------------------------------------------------------------- audio instance (AudioWorklet)

/// Load the sound bank from the host's packs and start the driver at `out_rate` Hz. 0 = ok, 1 = error text.
#[no_mangle]
pub extern "C" fn wf_audio_init(out_rate: u32) -> u32 {
    let bank = match host_packs().and_then(|p| wf_app::wf_content::audio::load_bank(&p)) {
        Ok(b) => b,
        Err(e) => {
            set_str(&e.to_string());
            return 1;
        }
    };
    let bank: &'static _ = Box::leak(Box::new(bank));
    AUDIO.with(|a| *a.borrow_mut() = Some(AudioEngine::new(bank, out_rate)));
    0
}

#[no_mangle]
pub extern "C" fn wf_audio_event(kind: i32, a: i32, b: i32, c: i32) {
    if let Some(e) = AudioEvent::decode([kind, a, b, c]) {
        AUDIO.with(|x| {
            if let Some(eng) = x.borrow_mut().as_mut() {
                eng.push(e)
            }
        });
    }
}

#[no_mangle]
pub extern "C" fn wf_audio_set(gain: f32, linear: u32) {
    AUDIO.with(|x| {
        if let Some(eng) = x.borrow_mut().as_mut() {
            eng.gain = gain;
            eng.resample = if linear != 0 { wf_app::audio::Resample::Linear } else { wf_app::audio::Resample::Hold };
        }
    });
}

/// Render `frames` stereo frames; returns a pointer to `2 * frames` interleaved f32.
#[no_mangle]
pub extern "C" fn wf_audio_render(frames: u32) -> *const f32 {
    AUDIO_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        buf.resize(frames as usize * 2, 0.0);
        AUDIO.with(|x| {
            if let Some(eng) = x.borrow_mut().as_mut() {
                eng.render(&mut buf);
            } else {
                buf.fill(0.0);
            }
        });
        buf.as_ptr()
    })
}

// ---------------------------------------------------------------- replays, rewind, save states, training

/// Keyboard event with modifiers (bit 0 shift, 1 ctrl/meta, 2 alt) and key repeat. Returns 0 = not used,
/// 1 = a game button, 2 = a hotkey (replays / rewind / save states / training). Releases always reach the game.
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_key2(ptr: *const u8, len: usize, down: u32, player: u32, mods: u32, repeat: u32) -> u32 {
    let code = unsafe { arg_str(ptr, len) };
    with_game(|g| {
        g.input_gen = g.input_gen.wrapping_add(1);
        let bound = g.bindings.key(&code) != 0;
        if g.ctl.key(&mut g.app, &code, Mods::from_bits(mods), down != 0, repeat != 0, bound) {
            if down == 0 {
                if let Some(k) = g.keys.get_mut(player as usize) {
                    k.set(&code, false);
                }
            }
            return 2;
        }
        if let Some(k) = g.keys.get_mut(player as usize) {
            k.set(&code, down != 0);
        }
        bound as u32
    })
    .unwrap_or(0)
}

/// Wall clock (Unix seconds) for recording metadata.
#[no_mangle]
pub extern "C" fn wf_app_set_clock(unix: f64) {
    with_game(|g| g.ctl.now_unix = unix as u64);
}

/// Next host request: 0 none; 1 save replay (string = `{"id", "header"}`, blob = file); 2 save state (string =
/// `{"slot"}`, blob = file); 3 load state (string = `{"slot"}`: answer with `wf_app_load_state`); 4 open the
/// replay browser; 5 back to the launcher's Battle page (the battle's end screen: "Change fighter").
#[no_mangle]
pub extern "C" fn wf_app_next_request() -> u32 {
    with_game(|g| {
        let more = g.ctl.take_requests();
        g.requests.extend(more);
        match g.requests.pop_front() {
            None => 0,
            Some(Request::SaveReplay { id, bytes, header }) => {
                set_str(&serde_json::json!({"id": id, "header": header}).to_string());
                set_blob(bytes);
                1
            }
            Some(Request::SaveState { slot, bytes }) => {
                set_str(&serde_json::json!({"slot": slot}).to_string());
                set_blob(bytes);
                2
            }
            Some(Request::LoadState { slot }) => {
                set_str(&serde_json::json!({"slot": slot}).to_string());
                3
            }
            Some(Request::OpenReplays) => {
                g.ctl.finalize(&g.app);
                4
            }
            Some(Request::BattleSelect) => {
                g.ctl.finalize(&g.app);
                5
            }
        }
    })
    .unwrap_or(0)
}

fn result(r: Result<(), String>) -> u32 {
    match r {
        Ok(()) => 0,
        Err(e) => {
            set_str(&e);
            1
        }
    }
}

/// Load a save state file. 0 = ok, 1 = error text in the string buffer.
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_load_state(ptr: *const u8, len: usize) -> u32 {
    let b = unsafe { arg_bytes(ptr, len) };
    with_game(|g| result(g.ctl.load_state(&mut g.app, &b))).unwrap_or(1)
}

/// Show a message in the game view (host-side errors: empty slot, storage failures).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_toast(ptr: *const u8, len: usize) {
    let m = unsafe { arg_str(ptr, len) };
    with_game(|g| g.ctl.toast(m));
}

/// Play a replay file (`ghost` != 0: run it as a ghost next to the current session). 0 = ok, 1 = error text.
///
/// # Safety
/// Pointers must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_play_replay(idp: *const u8, idl: usize, ptr: *const u8, len: usize, ghost: u32) -> u32 {
    let (id, b) = unsafe { (arg_str(idp, idl), arg_bytes(ptr, len)) };
    with_game(|g| {
        if ghost != 0 {
            result(g.ctl.set_ghost(&g.app, &id, &b))
        } else {
            g.keys.iter_mut().for_each(|k| k.clear());
            result(g.ctl.play_replay(&mut g.app, &id, &b))
        }
    })
    .unwrap_or(1)
}

/// Verify a replay headless against this build: JSON report in the string buffer (0), or an error (1).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_verify(ptr: *const u8, len: usize) -> u32 {
    let b = unsafe { arg_bytes(ptr, len) };
    with_game(|g| match g.ctl.verify(&g.app, &b) {
        Ok(r) => {
            let mut v = serde_json::to_value(&r).unwrap_or_default();
            v["text"] = match r.divergence {
                Some(d) => d.to_string(),
                None if r.ok => format!("OK: {} frames, {} checkpoints match", r.frames, r.checkpoints),
                None => "could not start".into(),
            }
            .into();
            set_str(&v.to_string());
            0
        }
        Err(e) => {
            set_str(&e);
            1
        }
    })
    .unwrap_or(1)
}

/// Header JSON of a replay file in any supported form (string buffer) and the normalised binary file (blob
/// buffer). 0 = ok, 1 = error text.
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_replay_info(ptr: *const u8, len: usize) -> u32 {
    let b = unsafe { arg_bytes(ptr, len) };
    match wf_app::control::parse_replay(&b) {
        Ok(r) => {
            let bytes = r.to_bytes();
            let h = wf_app::wf_replay::Replay::peek_header(&bytes).map(|h| serde_json::to_string(&h).unwrap_or_default()).unwrap_or_default();
            set_str(&h);
            set_blob(bytes);
            0
        }
        Err(e) => {
            set_str(&e);
            1
        }
    }
}

/// The live recording now: blob = file, string = `{"id", "header"}`. 0 = ok, 1 = nothing recorded (playback).
#[no_mangle]
pub extern "C" fn wf_app_current_replay() -> u32 {
    with_game(|g| match g.ctl.current_replay(&g.app) {
        Some((id, r)) => {
            let bytes = r.to_bytes();
            set_str(&serde_json::json!({"id": id, "header": wf_app::wf_replay::Replay::peek_header(&bytes).ok()}).to_string());
            set_blob(bytes);
            0
        }
        None => 1,
    })
    .unwrap_or(1)
}

/// Queue the live recording for saving (page hide / unload).
#[no_mangle]
pub extern "C" fn wf_app_finalize() {
    with_game(|g| g.ctl.finalize(&g.app));
}

/// Run an action by name (`pause`, `frame_advance`, `take_over`, `save_state_1`, ... see `hotkeys::Action`).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_action(ptr: *const u8, len: usize) -> u32 {
    let name = unsafe { arg_str(ptr, len) };
    let Some(a) = Action::parse(&name) else { return 0 };
    with_game(|g| {
        g.ctl.act(&mut g.app, a);
        1
    })
    .unwrap_or(0)
}

/// Seek to a frame (playback scrubbing); returns the frame reached.
#[no_mangle]
pub extern "C" fn wf_app_seek(frame: f64) -> f64 {
    with_game(|g| g.ctl.seek(&mut g.app, frame.max(0.0) as u64) as f64).unwrap_or(0.0)
}

#[no_mangle]
pub extern "C" fn wf_app_clear_ghost() {
    with_game(|g| g.ctl.clear_ghost());
}

/// Status JSON in the string buffer (mode, frame, range, speed, flags, desync, ghost).
#[no_mangle]
pub extern "C" fn wf_app_status() {
    let s = with_game(|g| g.ctl.status_json(&g.app)).unwrap_or_default();
    set_str(&s);
}

/// Hotkeys in the string buffer: `{"config": Hotkeys, "actions": [[name, label, [combos]], ...]}`.
#[no_mangle]
pub extern "C" fn wf_app_hotkeys() {
    let s = with_game(|g| {
        let acts: Vec<_> = Action::all().into_iter().map(|a| serde_json::json!([a.name(), a.label(), g.ctl.hotkeys.keys_for(a)])).collect();
        serde_json::json!({"config": g.ctl.hotkeys, "actions": acts}).to_string()
    })
    .unwrap_or_default();
    set_str(&s);
}

/// Rebind action `action` (name) to `combo` (`Shift+F6`); an empty action resets all hotkeys to the defaults.
///
/// # Safety
/// Pointers must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_app_rebind_hotkey(ap: *const u8, al: usize, cp: *const u8, cl: usize) {
    let (a, c) = unsafe { (arg_str(ap, al), arg_str(cp, cl)) };
    with_game(|g| {
        if a.is_empty() {
            g.ctl.hotkeys = Hotkeys::default();
        } else if let Some(a) = Action::parse(&a) {
            g.ctl.hotkeys.rebind(a, &c);
        }
    });
}
