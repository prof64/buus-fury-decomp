//! A deterministic recording host for differential tests (bytecode VM vs decompiled Rhai) and rollback tests.
//!
//! * Pure queries answer from a hash of (seed, effect count so far, native, args): two runtimes that perform
//!   the same effects in the same order see the same answers, regardless of how often they ask.
//! * `rand` / `chance` use the game's PRNG (`0x0803FAAC`) seeded from the seed — they are effects.
//! * Effects (actions, setters, text/choice nodes) are recorded in order with their arguments.
//! * Blocking follows the ROM: modal natives and nodes block; queued cutscene commands block unless a
//!   `begin_parallel` batch is open; `end_parallel` blocks.

use crate::natives;
use crate::vm::{ext_info, Call, ExtCall, ExtKind, Globals, Host, NativeResult, NodeCall};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Native { native: u8, args: Vec<i32>, ret: Option<i32> },
    Set { var: u8, value: i32 },
    Text { text: String, arg: u8, args: Vec<i32> },
    Choice { text: String, arg: u8 },
    Ext { name: String, args: Vec<i32>, who: i32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostState {
    pub seed: u64,
    pub s0: u32,
    pub s1: u32,
    pub effects: u64,
    pub in_batch: bool,
    /// Value to deliver when the blocked call resumes.
    pub pending: Option<i32>,
    pub trace: Vec<Effect>,
    /// Number of players answered by `player_count()` (0 = 1).
    pub players: i32,
}

impl HostState {
    pub fn new(seed: u64) -> HostState {
        HostState {
            seed,
            s0: (seed as u32).wrapping_mul(0x9E37_79B9) ^ 0x1234_5678,
            s1: ((seed >> 32) as u32) ^ (seed as u32).rotate_left(13) ^ 0x0BAD_F00D,
            ..Default::default()
        }
    }

    /// The game's PRNG step (VERIFIED: re/buu/notes.md §1).
    pub fn next(&mut self) -> u32 {
        let a = self.s0.wrapping_add(0x02CB_BC71) ^ self.s1;
        let b = self.s1.rotate_left(3).wrapping_add(0x0632_D80F) ^ a;
        self.s0 = a;
        self.s1 = b;
        a.wrapping_sub(b)
    }

    fn hash(&self, n: u8, args: &[i32]) -> u64 {
        let mut h: u64 = self.seed ^ 0xcbf2_9ce4_8422_2325 ^ (self.effects << 20) ^ ((n as u64) << 8);
        for &a in args {
            h = (h ^ a as u32 as u64).wrapping_mul(0x0100_0000_01b3);
            h ^= h >> 29;
        }
        h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^ (h >> 31)
    }
}

/// Cloneable handle; the state is shared so tests can inspect / reset it while a runtime owns the host.
#[derive(Clone, Default)]
pub struct TestHost(pub Rc<RefCell<HostState>>);

impl TestHost {
    pub fn new(seed: u64) -> TestHost {
        TestHost(Rc::new(RefCell::new(HostState::new(seed))))
    }
    pub fn reset(&self, seed: u64) {
        *self.0.borrow_mut() = HostState::new(seed);
    }
    pub fn trace(&self) -> Vec<Effect> {
        self.0.borrow().trace.clone()
    }
    pub fn take_pending(&self) -> Option<i32> {
        self.0.borrow_mut().pending.take()
    }
}

fn kind(n: u8) -> &'static str {
    natives::info(n).and_then(|i| i.kind.as_deref()).map(|k| match k {
        "query" => "query",
        "enqueue" => "enqueue",
        "modal" => "modal",
        _ => "immediate",
    }).unwrap_or("immediate")
}

impl Host for TestHost {
    fn native(&mut self, call: Call<'_>) -> Result<NativeResult, String> {
        let n = call.native;
        let mut s = self.0.borrow_mut();
        let sig = natives::sig(n);
        if natives::is_pure_query(n) {
            let v = (s.hash(n, call.args) % 3) as i32;
            return Ok(NativeResult::Done(Some(v)));
        }
        if n == 0x19 || n == 0x1a {
            let x = s.next() & 0xffff;
            let a = call.args[0];
            let v = if n == 0x19 {
                ((x as i64 * a as i64) >> 16) as i32
            } else {
                (x < ((a.wrapping_mul(0x28F5D)) >> 8) as u32) as i32
            };
            s.effects += 1;
            s.trace.push(Effect::Native { native: n, args: call.args.to_vec(), ret: Some(v) });
            return Ok(NativeResult::Done(Some(v)));
        }
        let ret = if sig.pushes { Some((s.hash(n, call.args) % 3) as i32) } else { None };
        s.effects += 1;
        s.trace.push(Effect::Native { native: n, args: call.args.to_vec(), ret });
        let block = match n {
            0x38 => {
                s.in_batch = true;
                false
            }
            0x3a => {
                s.in_batch = false;
                true
            }
            _ => match kind(n) {
                "modal" => true,
                "enqueue" => !s.in_batch,
                _ => false,
            },
        };
        if block {
            s.pending = ret;
            Ok(NativeResult::Block)
        } else {
            Ok(NativeResult::Done(ret))
        }
    }

    fn set_var(&mut self, var: u8, value: i32, _p: &mut [i32; 4], _g: &mut Globals) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        s.effects += 1;
        s.trace.push(Effect::Set { var, value });
        Ok(())
    }

    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        let mut s = self.0.borrow_mut();
        s.effects += 1;
        match call {
            NodeCall::Text { text, arg, args, .. } => s.trace.push(Effect::Text { text, arg, args }),
            NodeCall::Choice { text, arg, .. } => s.trace.push(Effect::Choice { text, arg }),
        }
        s.pending = None;
        Ok(NativeResult::Block)
    }

    fn ext(&mut self, call: ExtCall<'_>) -> Result<NativeResult, String> {
        let mut s = self.0.borrow_mut();
        let (arity, kind) = ext_info(call.name).ok_or_else(|| format!("unknown extension {}", call.name))?;
        if call.args.len() != arity as usize {
            return Err(format!("{}: {} args, expected {arity}", call.name, call.args.len()));
        }
        let n = s.players.max(1);
        match kind {
            ExtKind::Query => {
                let v = match call.name {
                    "player_count" => n,
                    // actor ids: leader = 0 (as in the original), others 0x200 + n (host-defined)
                    "player" => {
                        let p = call.args[0];
                        if !(1..=n).contains(&p) {
                            -1
                        } else if p == 1 {
                            0
                        } else {
                            0x200 + p
                        }
                    }
                    "player_of_actor" => match call.args[0] {
                        0 => 1,
                        a if a > 0x201 && a <= 0x200 + n => a - 0x200,
                        _ => 0,
                    },
                    _ => {
                        let mut h = s.seed ^ (s.effects << 24);
                        for b in call.name.bytes().chain(call.args.iter().flat_map(|a| a.to_le_bytes())) {
                            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
                        }
                        ((h >> 33) % 3) as i32
                    }
                };
                Ok(NativeResult::Done(Some(v)))
            }
            _ => {
                s.effects += 1;
                s.trace.push(Effect::Ext { name: call.name.to_string(), args: call.args.to_vec(), who: call.who });
                s.pending = None;
                Ok(if kind == ExtKind::Action { NativeResult::Block } else { NativeResult::Done(None) })
            }
        }
    }
}
