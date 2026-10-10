//! Minimal wasm embedding of the plain-data VM (size baseline: host + VM, no language front end).

#[no_mangle]
pub extern "C" fn updates_digest() -> u64 {
    let mut rt = rt_mini::MiniRt::new();
    host::updates_only(&mut rt, 300)
}
