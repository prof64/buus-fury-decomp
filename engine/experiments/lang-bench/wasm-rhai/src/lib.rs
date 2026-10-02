//! Minimal wasm embedding: plain `extern "C"` exports (no wasm-bindgen).

/// Full scenario (600 frames); returns the world digest. Compare with the native run.
#[no_mangle]
pub extern "C" fn scenario_digest() -> u64 {
    let mut rt = rt_rhai::RhaiRt::new(rt_rhai::STORY);
    host::scenario(&mut rt, 600)
}

/// Updates-only scenario (300 frames).
#[no_mangle]
pub extern "C" fn updates_digest() -> u64 {
    let mut rt = rt_rhai::RhaiRt::new(rt_rhai::STORY);
    host::updates_only(&mut rt, 300)
}
