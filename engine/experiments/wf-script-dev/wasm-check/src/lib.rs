//! `digest(rom)` exported for wasm (plain extern "C"); the native side calls the same function.
//! Native: `cargo run --release -p wasm-check --example native -- <rom>`;
//! wasm: `cargo build --release --target wasm32-unknown-unknown -p wasm-check` then `node run.mjs <rom>`.

static mut OUT: [u64; 2] = [0, 0];

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Returns a pointer to [digest, effects] (u64 × 2), or null on error.
#[no_mangle]
pub extern "C" fn digest(ptr: *mut u8, len: usize) -> *const u64 {
    let rom = unsafe { Vec::from_raw_parts(ptr, len, len) };
    match wf_script_rhai::check::digest(rom, None) {
        Ok((d, n)) => unsafe {
            OUT = [d, n];
            std::ptr::addr_of!(OUT) as *const u64
        },
        Err(_) => std::ptr::null(),
    }
}
