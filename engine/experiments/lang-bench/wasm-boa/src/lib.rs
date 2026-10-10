//! Minimal wasm embedding: plain `extern "C"` exports (no wasm-bindgen).

#[no_mangle]
pub extern "C" fn scenario_digest() -> u64 {
    let mut rt = rt_boa::BoaRt::new(rt_boa::STORY);
    host::scenario(&mut rt, 600)
}

#[no_mangle]
pub extern "C" fn updates_digest() -> u64 {
    let mut rt = rt_boa::BoaRt::new(rt_boa::STORY);
    host::updates_only(&mut rt, 300)
}
