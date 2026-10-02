//! Probe: piccolo (pure-Rust Lua) on wasm32-unknown-unknown; is its state cloneable?
//! (`Lua` / `Executor` live in a gc-arena; neither is `Clone` nor serializable — checked in the source.)
use piccolo::{Closure, Executor, Lua};

#[no_mangle]
pub extern "C" fn probe() -> i64 {
    let mut lua = Lua::core();
    let ex = lua
        .try_enter(|ctx| {
            let c = Closure::load(ctx, None, &b"local s = 0 for i = 1, 100 do s = s + i end return s"[..])?;
            Ok(ctx.stash(Executor::start(ctx, c.into(), ())))
        })
        .unwrap();
    lua.execute::<i64>(&ex).unwrap()
}

#[cfg(test)]
mod t {
    #[test]
    fn runs() {
        assert_eq!(super::probe(), 5050);
    }
}
