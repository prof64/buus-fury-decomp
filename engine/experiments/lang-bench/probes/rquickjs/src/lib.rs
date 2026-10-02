//! Probe: does rquickjs (QuickJS, C) build for wasm32-unknown-unknown without system installs?
#[no_mangle]
pub extern "C" fn probe() -> i32 {
    let rt = rquickjs::Runtime::new().unwrap();
    let ctx = rquickjs::Context::full(&rt).unwrap();
    ctx.with(|c| c.eval::<i32, _>("1 + 2").unwrap())
}
