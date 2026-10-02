//! Determinism digest: run every event (bytecode VM and Rhai, recording host, fixed seeds) plus a
//! snapshot/restore round, hash all effect traces. Native and wasm32 builds must produce the same value
//! (`tests/wasm_determinism.rs` builds `engine/experiments/wf-script-dev/wasm-check` and runs it in node).

use crate::{Runtime, Status};
use wf_script::testhost::TestHost;
use wf_script::vm::{Globals, Status as VStatus, Thread};
use wf_script::ScriptSet;

fn fnv(h: &mut u64, s: &str) {
    for b in s.bytes() {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x0100_0000_01b3);
    }
}

/// Returns (digest, number of effects hashed). `events` limits the run (None = all 522).
pub fn digest(rom_bytes: Vec<u8>, events: Option<u16>) -> Result<(u64, u64), String> {
    let rom = wf_rom::Rom::new(rom_bytes).map_err(|e| e.to_string())?;
    let set = ScriptSet::load_all(&rom);
    let (files, _) = wf_script::export::export(&rom, &set);
    let h = TestHost::new(0);
    let mut rt = Runtime::new(&files, Box::new(h.clone()))?;
    let mut d: u64 = 0xcbf2_9ce4_8422_2325;
    let mut n = 0u64;
    for &ev in set.events.keys().take(events.map(|e| e as usize).unwrap_or(usize::MAX)) {
        let seed = ev as u64 * 7 + 1;
        // VM
        let mut t = Thread::new_event(&set, ev, [1, 2, 0, 0]).map_err(|e| e.to_string())?;
        t.max_steps = 200_000;
        let mut vh = TestHost::new(seed);
        let mut g = Globals::default();
        let mut s = t.run(&set, &mut g, &mut vh);
        while let Ok(VStatus::Blocked) = s {
            let v = vh.take_pending();
            s = t.resume(v, &set, &mut g, &mut vh);
        }
        // Rhai, with a snapshot/restore after the first block
        h.reset(seed);
        rt.set_globals(Globals::default());
        rt.start_event(ev, [1, 2, 0, 0]);
        let mut s2 = rt.run();
        let mut first = true;
        while let Ok(Status::Blocked) = s2 {
            if first {
                first = false;
                let snap = (rt.state(), h.0.borrow().clone(), rt.globals());
                let _ = rt.resume(h.take_pending());
                rt.restore(snap.0);
                *h.0.borrow_mut() = snap.1;
                rt.set_globals(snap.2);
            }
            s2 = rt.resume(h.take_pending());
        }
        for (tag, tr) in [("vm", vh.trace()), ("rhai", h.trace())] {
            fnv(&mut d, tag);
            for e in tr {
                fnv(&mut d, &format!("{e:?}"));
                n += 1;
            }
        }
        fnv(&mut d, &format!("{:?}{:?}", s.is_ok(), s2.is_ok()));
    }
    Ok((d, n))
}
