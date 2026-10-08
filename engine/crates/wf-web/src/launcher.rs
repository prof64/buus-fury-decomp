//! The launcher in the browser (`wf_app::launcher`, the same screens as the native app), before the game instance
//! exists: JS calls `wf_launcher_init`, then per animation frame `wf_launcher_frame` (RGBA 240 x 160 at
//! `wf_launcher_fb_ptr`) and per keydown `wf_launcher_key`. A key that picks something returns 1 with
//! `{"choice": {...}, "settings": {...}, "bindings": {...}}` in the string buffer; JS persists the settings and
//! starts the game with `wf_app_init` (`{"launch": choice, "slots": [...]}`). Story save slots go back to JS with
//! `wf_app_saves` whenever the game saved.

use std::cell::RefCell;

use wf_app::launcher::{Choice, Launcher, ModEntry, Settings};
use wf_app::wf_content::Packs;
use wf_app::App;

struct State {
    l: Launcher,
    packs: Packs,
    rgba: Vec<u8>,
    /// the host clock at the last drawn frame (the Battle page's animations run on elapsed time, like native)
    last_ms: f64,
}

thread_local! {
    static L: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|k| u8::from_str_radix(s.get(2 * k..2 * k + 2)?, 16).ok()).collect()
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `slots` of an init / launch config: hex strings ("" = empty slot)
pub fn slots(cfg: &serde_json::Value) -> Vec<Vec<u8>> {
    cfg["slots"].as_array().map(|a| a.iter().map(|v| hex_decode(v.as_str().unwrap_or(""))).collect()).unwrap_or_default()
}

/// `wf_app_init` with `{"launch": {"kind": "story", "slot": n|null}}` or `{"launch": {"kind": "battle", "config"}}`
pub fn launch_app(cfg: &serde_json::Value, packs: Packs) -> Option<Result<App, String>> {
    let l = &cfg["launch"];
    match l["kind"].as_str()? {
        "title" => Some(wf_app::launcher::title_app(packs, &slots(cfg), cfg["seed"].as_u64().unwrap_or(1) as u32)),
        "story" => Some(wf_app::launcher::story_app(packs, &slots(cfg), l["slot"].as_u64().map(|v| v as usize), cfg["seed"].as_u64().unwrap_or(1) as u32)),
        "battle" => Some(serde_json::from_value(l["config"].clone()).map_err(|e| e.to_string()).and_then(|c| App::battle(packs, c))),
        _ => None,
    }
}

/// Start the launcher. `cfg` = `{"settings": {...}, "slots": [hex, ...], "mods": [{"name", "enabled"}], "bindings"}`.
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_launcher_init(ptr: *const u8, len: usize) -> u32 {
    let cfg: serde_json::Value = serde_json::from_str(&unsafe { crate::arg_str(ptr, len) }).unwrap_or_default();
    let packs = match crate::host_packs() {
        Ok(p) => p,
        Err(e) => {
            crate::set_str(&e.to_string());
            return 1;
        }
    };
    let settings: Settings = serde_json::from_value(cfg["settings"].clone()).unwrap_or_default();
    let mut l = Launcher::new(&packs, settings, false);
    l.set_slots(&slots(&cfg));
    l.mods = cfg["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| ModEntry { name: m["name"].as_str().unwrap_or("?").into(), enabled: m["enabled"].as_bool().unwrap_or(true) })
        .collect();
    l.bindings = serde_json::from_value(cfg["bindings"].clone()).unwrap_or_default();
    L.with(|s| *s.borrow_mut() = Some(State { l, packs, rgba: vec![0; 240 * 160 * 4], last_ms: unsafe { crate::now_ms() } }));
    0
}

/// A keydown (W3C `code`). 1 = a choice (JSON in the string buffer), 0 = nothing for the host to do.
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_launcher_key(ptr: *const u8, len: usize) -> u32 {
    let code = unsafe { crate::arg_str(ptr, len) };
    L.with(|s| {
        let mut s = s.borrow_mut();
        let Some(st) = s.as_mut() else { return 0 };
        let c = st.l.key(&code);
        if c == Choice::None {
            return 0;
        }
        let out = serde_json::json!({
            "choice": c.to_json(),
            "settings": st.l.settings,
            "bindings": st.l.bindings,
            "battle": st.l.battle_config(),
        });
        crate::set_str(&out.to_string());
        1
    })
}

/// Draw the launcher; returns the integer scale for a `win_w` x `win_h` window.
#[no_mangle]
pub extern "C" fn wf_launcher_frame(win_w: u32, win_h: u32) -> u32 {
    L.with(|s| {
        let mut s = s.borrow_mut();
        let Some(st) = s.as_mut() else { return 1 };
        let mut fb = vec![0u32; 240 * 160];
        // the Battle page's idle animations: the time since the last frame (any display rate)
        let now = unsafe { crate::now_ms() };
        st.l.advance((now - st.last_ms) / 1000.0);
        st.last_ms = now;
        st.l.draw(&st.packs, &mut fb, 240, 160);
        for (o, &p) in st.rgba.chunks_exact_mut(4).zip(fb.iter()) {
            o[0] = (p >> 16) as u8;
            o[1] = (p >> 8) as u8;
            o[2] = p as u8;
            o[3] = 255;
        }
        (win_w / 240).min(win_h / 160).max(1)
    })
}

#[no_mangle]
pub extern "C" fn wf_launcher_fb_ptr() -> *const u8 {
    L.with(|s| s.borrow().as_ref().map(|st| st.rgba.as_ptr()).unwrap_or(std::ptr::null()))
}

/// The launcher is done: free it (its fonts / title art / game data).
#[no_mangle]
pub extern "C" fn wf_launcher_close() {
    L.with(|s| *s.borrow_mut() = None);
}

/// Story mode's save slots: returns the generation (bumped by every in-game save; u32::MAX = not story mode) and
/// puts `["hex", "hex", "hex"]` in the string buffer.
#[no_mangle]
pub extern "C" fn wf_app_saves() -> u32 {
    crate::with_game(|g| {
        let Some((gen, files)) = wf_app::launcher::saves_of(&g.app) else { return u32::MAX };
        let v: Vec<String> = files.iter().map(|b| hex_encode(b)).collect();
        crate::set_str(&serde_json::to_string(&v).unwrap_or_default());
        gen
    })
    .unwrap_or(u32::MAX)
}
