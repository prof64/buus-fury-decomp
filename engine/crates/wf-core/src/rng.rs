//! The game's PRNG (0x0803FAAC, state IWRAM 0x03004A5C/60; re/buu/notes.md §1):
//!   r1 = (a + 0x02CBBC71) ^ b ; r0 = (rol(b, 3) + 0x0632D80F) ^ r1 ; a = r1 ; b = r0 ; return r1 - r0
//!   rand(n) = ((next & 0xFFFF) * n) >> 16, chance(p) = (next & 0xFFFF) < p

use serde::{Deserialize, Serialize};

/// diagnostics: print every generator call with its caller (lockstep harnesses switch it on for chosen frames)
pub static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rng {
    pub a: u32,
    pub b: u32,
    /// generator calls so far (diagnostics; part of the state hash)
    pub calls: u32,
}

impl Rng {
    pub fn new(a: u32, b: u32) -> Rng {
        Rng { a, b, calls: 0 }
    }
    /// the reference sim's seeding convention (`BuuRng.fromSeed`): seed % 100000 steps from (0, 0)
    pub fn from_seed(seed: u32) -> Rng {
        let mut r = Rng::new(0, 0);
        for _ in 0..(seed % 100000) {
            r.step();
        }
        r.calls = 0;
        r
    }
    #[inline]
    fn step(&mut self) -> u32 {
        let r1 = self.a.wrapping_add(0x02cb_bc71) ^ self.b;
        let r0 = self.b.rotate_left(3).wrapping_add(0x0632_d80f) ^ r1;
        self.a = r1;
        self.b = r0;
        r1.wrapping_sub(r0)
    }
    #[inline]
    #[track_caller]
    pub fn next(&mut self) -> u32 {
        self.calls = self.calls.wrapping_add(1);
        if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("      rng call at {}", std::panic::Location::caller());
            if std::env::var("RNG_BT").is_ok() { eprintln!("{}", std::backtrace::Backtrace::force_capture().to_string().lines().filter(|l| l.contains("wf_core::")).take(8).collect::<Vec<_>>().join(" | ")); }
        }
        self.step()
    }
    /// 0..n-1
    #[inline]
    #[track_caller]
    pub fn rand(&mut self, n: u32) -> i32 {
        let v = self.next();
        (((v & 0xffff) as u64 * n as u64) >> 16) as i32
    }
    /// probability p / 65536
    #[inline]
    #[track_caller]
    pub fn chance(&mut self, p: i64) -> bool {
        let v = self.next();
        ((v & 0xffff) as i64) < p
    }
    pub fn state(&self) -> [u32; 2] {
        [self.a, self.b]
    }
    pub fn set_state(&mut self, s: [u32; 2]) {
        self.a = s[0];
        self.b = s[1];
    }
}
