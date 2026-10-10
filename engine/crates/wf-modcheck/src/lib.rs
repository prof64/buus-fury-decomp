//! native == wasm for modded games (engine/docs/modding.md §9): fixed scenarios with scripted mod content (Rhai skill
//! hooks, mod projectiles, scripted AI nodes, story party / events) run frame by frame; the digest folds every
//! frame's state hash and the events. Native: `cargo run --release -p wf-modcheck --example digest -- <content/base>
//! <mod>...`; wasm32: `node engine/web/mods_check.mjs <wf_modcheck.wasm> <content/base> <mod>...` (same packs, same
//! digest expected; the wasm module is built with
//! `cargo build -p wf-modcheck --target wasm32-unknown-unknown --profile release-wasm`).

use std::cell::RefCell;
use std::collections::BTreeMap;

use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_app::wf_view::Sim;
use wf_core::input as btn;

fn fnv(h: u64, x: u64) -> u64 {
    let mut h = h;
    for b in x.to_le_bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The scenarios: a 2-player battle (Goku with the mods' first skill + Shadow Goku if present) against the mods'
/// enemies, inputs that hold / release B, walk and punch; then a story session in the Southwest Forest walking and
/// pressing A. Returns one line per scenario: `name frames digest`.
pub fn digest(packs: &Packs, frames: u32) -> Result<String, String> {
    let d = wf_app::battle::load_data(packs)?;
    let mut out = String::new();
    // battle: every mod enemy type of the packs (types >= 150) plus a Majin Fighter
    let mut enemies: Vec<String> = d.enemies.iter().filter(|e| e.ty >= 150).map(|e| format!("t{}", e.ty)).collect();
    enemies.push("t68".into());
    let cfg = wf_app::battle::config("forest", &enemies.join("+"), 1, "goku", None, None, 2, 9);
    let mut s = wf_core::session::Session::new(d.clone(), cfg);
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for f in 0..frames {
        let a = match (f / 40) % 6 {
            0 => btn::B,
            1 => btn::RIGHT,
            2 => if f % 8 < 2 { btn::A } else { btn::DOWN },
            3 => if f % 20 < 2 { btn::B } else { 0 },
            4 => btn::LEFT | btn::UP,
            _ => if f % 60 < 3 { btn::B } else { btn::RIGHT },
        };
        let b = if f % 50 < 30 { btn::B } else { btn::LEFT };
        let ev = s.step(&[a, b]).len() as u64;
        h = fnv(fnv(h, s.world.state_hash()), ev);
    }
    // the modded world must also survive the byte snapshot (rollback across machines)
    let snap = s.snapshot_bytes();
    let mut s2 = wf_core::session::Session::new(d.clone(), wf_app::battle::config("forest", "t68", 1, "goku", None, None, 2, 9));
    if !s2.restore_bytes(&snap) || s2.world.state_hash() != s.world.state_hash() {
        return Err("byte snapshot restore differs".into());
    }
    out.push_str(&format!("battle {frames} {h:016x}\n"));
    // story: the forest with the mods' area objects / events (no host: the field alone)
    if d.field.is_some() {
        let mut st = wf_app::story::StorySim::new(packs, (8, 3), vec![], 5)?;
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut ev = vec![];
        for f in 0..frames {
            let k = [btn::UP, btn::RIGHT, btn::A, btn::LEFT, btn::DOWN | btn::A][(f / 50) as usize % 5];
            ev.clear();
            st.step(&[k], &mut ev);
            h = fnv(fnv(h, st.state_hash()), st.errors.len() as u64);
        }
        out.push_str(&format!("story {frames} {h:016x}\n"));
    }
    Ok(out)
}

// ---- wasm32 exports (plain extern "C", like wf-wasm) ---------------------------------------------------------------

thread_local! {
    static FILES: RefCell<Vec<BTreeMap<String, Vec<u8>>>> = const { RefCell::new(Vec::new()) };
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn mc_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len.max(1)];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// add a file to pack `pack` (0 = base, 1.. = mods in load order)
///
/// # Safety
/// the pointers must describe readable memory (from `mc_alloc`)
#[no_mangle]
pub unsafe extern "C" fn mc_file(pack: u32, path: *const u8, path_len: usize, data: *const u8, data_len: usize) {
    let p = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(path, path_len) }).into_owned();
    let d = unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec();
    FILES.with(|f| {
        let mut f = f.borrow_mut();
        while f.len() <= pack as usize {
            f.push(BTreeMap::new());
        }
        f[pack as usize].insert(p, d);
    });
}

/// run the scenarios over the added packs; result = the digest lines (0 = ok) or the error (1)
#[no_mangle]
pub extern "C" fn mc_run(frames: u32) -> u32 {
    let files = FILES.with(|f| std::mem::take(&mut *f.borrow_mut()));
    let srcs: Vec<Box<dyn FileSource>> = files.into_iter().map(|m| Box::new(MemSource(m)) as Box<dyn FileSource>).collect();
    let r = Packs::new(srcs).map_err(|e| e.to_string()).and_then(|p| digest(&p, frames));
    let (code, bytes) = match r {
        Ok(s) => (0, s.into_bytes()),
        Err(e) => (1, e.into_bytes()),
    };
    RESULT.with(|r| *r.borrow_mut() = bytes);
    code
}

#[no_mangle]
pub extern "C" fn mc_result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}
#[no_mangle]
pub extern "C" fn mc_result_len() -> usize {
    RESULT.with(|r| r.borrow().len())
}
