//! The headless simulation (`wf-core::Session`) through plain `extern "C"` exports (no wasm-bindgen).
//!
//! ```text
//! wf_extract(...)                       // as before; the extracted content is kept for the session
//! wf_core_load_last()                   // game data from the last extraction (0 = ok)
//! wf_core_load_tar(ptr, len)            // ... or from a cached base-pack tar (the bytes wf_extract returned)
//! wf_core_reset(cfg_ptr, cfg_len)       // BattleConfig JSON ("" = default); 0 = ok
//! wf_core_step(keys_ptr, n_players)     // u16 held buttons per player; returns the event count, result = events JSON
//! wf_core_observe(player)               // result = observation JSON
//! wf_core_snapshot()                    // result = snapshot bytes;  wf_core_restore(ptr, len) -> 1 ok
//! wf_core_hash_lo() / wf_core_hash_hi() // per-frame state hash
//! ```
//! Results are read like the extractor's: `wf_result_ptr()` / `wf_result_len()`. Nothing here draws or plays sound.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use wf_core::battle::BattleConfig;
use wf_core::content::{GameData, MapSource};
use wf_core::session::Session;

thread_local! {
    static LAST: RefCell<Option<BTreeMap<String, Vec<u8>>>> = const { RefCell::new(None) };
    static DATA: RefCell<Option<Arc<GameData>>> = const { RefCell::new(None) };
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

pub fn remember(c: wf_extract::Content) {
    LAST.with(|l| *l.borrow_mut() = Some(c.files));
}

fn set_result(v: Vec<u8>) {
    crate::RESULT.with(|r| *r.borrow_mut() = v);
}

fn load(files: BTreeMap<String, Vec<u8>>) -> u32 {
    match GameData::load(&MapSource::new(files)) {
        Ok(d) => {
            DATA.with(|x| *x.borrow_mut() = Some(Arc::new(d)));
            0
        }
        Err(e) => {
            set_result(e.0.into_bytes());
            2
        }
    }
}

#[no_mangle]
pub extern "C" fn wf_core_load_last() -> u32 {
    match LAST.with(|l| l.borrow_mut().take()) {
        Some(f) => load(f),
        None => 1,
    }
}

/// # Safety
/// `ptr`/`len` must describe readable memory (e.g. from `wf_alloc`).
#[no_mangle]
pub unsafe extern "C" fn wf_core_load_tar(ptr: *const u8, len: usize) -> u32 {
    let b = unsafe { std::slice::from_raw_parts(ptr, len) };
    let mut files = BTreeMap::new();
    let mut o = 0;
    while o + 512 <= b.len() {
        let h = &b[o..o + 512];
        if h.iter().all(|&x| x == 0) {
            break;
        }
        let cstr = |s: &[u8]| String::from_utf8_lossy(&s[..s.iter().position(|&x| x == 0).unwrap_or(s.len())]).into_owned();
        let name = cstr(&h[..100]);
        let prefix = cstr(&h[345..500]);
        let size = usize::from_str_radix(cstr(&h[124..136]).trim(), 8).unwrap_or(0);
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let start = o + 512;
        if start + size > b.len() {
            break;
        }
        files.insert(path, b[start..start + size].to_vec());
        o = start + size.div_ceil(512) * 512;
    }
    load(files)
}

/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_core_reset(ptr: *const u8, len: usize) -> u32 {
    let Some(d) = DATA.with(|x| x.borrow().clone()) else { return 1 };
    let cfg: BattleConfig = if len == 0 || ptr.is_null() {
        BattleConfig::default()
    } else {
        match serde_json::from_slice(unsafe { std::slice::from_raw_parts(ptr, len) }) {
            Ok(c) => c,
            Err(e) => {
                set_result(e.to_string().into_bytes());
                return 2;
            }
        }
    };
    SESSION.with(|s| *s.borrow_mut() = Some(Session::new(d, cfg)));
    0
}

/// a field session (`wf_core::field::FieldConfig` JSON; empty = new game in Southwest Forest). Needs a pack with the
/// `world` (+ `items`, `scripts`) categories.
///
/// # Safety
/// `ptr` must point at `len` bytes (or be null with `len` 0).
#[no_mangle]
pub unsafe extern "C" fn wf_core_reset_field(ptr: *const u8, len: usize) -> u32 {
    let Some(d) = DATA.with(|x| x.borrow().clone()) else { return 1 };
    if d.field.is_none() {
        set_result(b"content has no world/ category".to_vec());
        return 3;
    }
    let cfg: wf_core::field::FieldConfig = if len == 0 || ptr.is_null() {
        Default::default()
    } else {
        match serde_json::from_slice(unsafe { std::slice::from_raw_parts(ptr, len) }) {
            Ok(c) => c,
            Err(e) => {
                set_result(e.to_string().into_bytes());
                return 2;
            }
        }
    };
    SESSION.with(|s| *s.borrow_mut() = Some(Session::new_field(d, cfg)));
    0
}

/// # Safety
/// `keys` must point at `n` u16 values.
#[no_mangle]
pub unsafe extern "C" fn wf_core_step(keys: *const u16, n: usize) -> u32 {
    let k: Vec<u16> = if n == 0 || keys.is_null() { vec![] } else { unsafe { std::slice::from_raw_parts(keys, n) }.to_vec() };
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let Some(s) = s.as_mut() else { return 0 };
        let ev = s.step(&k).to_vec();
        set_result(serde_json::to_vec(&ev).unwrap_or_default());
        ev.len() as u32
    })
}

#[no_mangle]
pub extern "C" fn wf_core_observe(player: u32) -> u32 {
    SESSION.with(|s| match s.borrow().as_ref() {
        Some(s) => {
            set_result(serde_json::to_vec(&s.observe(player as usize)).unwrap_or_default());
            0
        }
        None => 1,
    })
}

#[no_mangle]
pub extern "C" fn wf_core_snapshot() -> u32 {
    SESSION.with(|s| match s.borrow().as_ref() {
        Some(s) => {
            set_result(s.snapshot_bytes());
            0
        }
        None => 1,
    })
}

/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_core_restore(ptr: *const u8, len: usize) -> u32 {
    let b = unsafe { std::slice::from_raw_parts(ptr, len) };
    SESSION.with(|s| s.borrow_mut().as_mut().map(|s| s.restore_bytes(b) as u32).unwrap_or(0))
}

fn hash() -> u64 {
    SESSION.with(|s| s.borrow().as_ref().map(|s| s.hash()).unwrap_or(0))
}
#[no_mangle]
pub extern "C" fn wf_core_hash_lo() -> u32 {
    hash() as u32
}
#[no_mangle]
pub extern "C" fn wf_core_hash_hi() -> u32 {
    (hash() >> 32) as u32
}
#[no_mangle]
pub extern "C" fn wf_core_frame() -> u32 {
    SESSION.with(|s| s.borrow().as_ref().map(|s| s.frame()).unwrap_or(0))
}
