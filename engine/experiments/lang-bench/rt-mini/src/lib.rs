//! Candidate (d): a tiny deterministic stack VM whose entire state is plain `Clone` data (what a
//! Lua/JS-subset compiler would target; the same shape as the original Webfoot bytecode VM).
//! Only `on_update` is hand-assembled here — enough to measure dispatch cost and mid-execution snapshots.

use host::api;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Push(i64),
    Load(u8),
    Store(u8),
    Native(u8),
    Add,
    Sub,
    Lt,
    Abs,
    Jz(u16),
    Jmp(u16),
    Ret,
}

/// Whole VM state: snapshot = clone, mid-execution included.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Vm {
    pub pc: usize,
    pub stack: Vec<i64>,
    pub locals: [i64; 8],
}

const PLAYER: u8 = 0;
const AX: u8 = 1;
const AY: u8 = 2;
const SET_STATE: u8 = 3;
const CHANCE: u8 = 4;
const RUN_FACE: u8 = 5;

pub fn on_update_code() -> Vec<Op> {
    use Op::*;
    vec![
        Push(1), Native(PLAYER), Store(1),
        Load(0), Native(AX), Load(1), Native(AX), Sub, Abs,
        Load(0), Native(AY), Load(1), Native(AY), Sub, Abs, Add, Store(2),
        Load(2), Push(64), Lt, Jz(31),       // 17..20
        Load(0), Push(1), Native(SET_STATE), // 21..23
        Push(10), Native(CHANCE), Jz(34),    // 24..26
        Load(0), Push(3), Native(RUN_FACE),  // 27..29
        Jmp(34),                             // 30
        Load(0), Push(0), Native(SET_STATE), // 31..33 (else)
        Load(2), Ret,                        // 34
    ]
}

impl Vm {
    fn native(&mut self, n: u8) {
        let s = &mut self.stack;
        match n {
            PLAYER => {
                let a = s.pop().unwrap();
                s.push(api::player(a));
            }
            AX => {
                let a = s.pop().unwrap();
                s.push(api::actor_x(a));
            }
            AY => {
                let a = s.pop().unwrap();
                s.push(api::actor_y(a));
            }
            SET_STATE => {
                let v = s.pop().unwrap();
                let a = s.pop().unwrap();
                api::set_state(a, v);
            }
            CHANCE => {
                let p = s.pop().unwrap();
                s.push(api::chance(p) as i64);
            }
            RUN_FACE => {
                let d = s.pop().unwrap();
                let a = s.pop().unwrap();
                api::run(host::Action::Face { actor: a, dir: d });
            }
            _ => unreachable!(),
        }
    }

    /// Run up to `budget` instructions; returns Some(result) when finished.
    pub fn step(&mut self, code: &[Op], budget: usize) -> Option<i64> {
        for _ in 0..budget {
            let op = code[self.pc];
            self.pc += 1;
            match op {
                Op::Push(v) => self.stack.push(v),
                Op::Load(i) => self.stack.push(self.locals[i as usize]),
                Op::Store(i) => self.locals[i as usize] = self.stack.pop().unwrap(),
                Op::Native(n) => self.native(n),
                Op::Add | Op::Sub | Op::Lt => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    self.stack.push(match op {
                        Op::Add => a.wrapping_add(b),
                        Op::Sub => a.wrapping_sub(b),
                        _ => (a < b) as i64,
                    });
                }
                Op::Abs => {
                    let a = self.stack.pop().unwrap();
                    self.stack.push(a.wrapping_abs());
                }
                Op::Jz(t) => {
                    if self.stack.pop().unwrap() == 0 {
                        self.pc = t as usize;
                    }
                }
                Op::Jmp(t) => self.pc = t as usize,
                Op::Ret => return self.stack.pop(),
            }
        }
        None
    }
}

pub struct MiniRt {
    pub code: Vec<Op>,
    pub vm: Vm,
}

impl MiniRt {
    pub fn new() -> MiniRt {
        MiniRt { code: on_update_code(), vm: Vm::default() }
    }
}

impl Default for MiniRt {
    fn default() -> Self {
        Self::new()
    }
}

impl host::Runtime for MiniRt {
    fn name(&self) -> &'static str {
        "mini-vm"
    }
    fn call(&mut self, handler: &str, arg: i64) -> i64 {
        if handler != "on_update" {
            return 0; // only the benchmark handler is assembled
        }
        self.vm = Vm::default();
        self.vm.locals[0] = arg;
        self.vm.step(&self.code, usize::MAX).unwrap_or(0)
    }
}
