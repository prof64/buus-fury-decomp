//! Language-neutral structured IR produced by the decompiler (`decompile.rs`) and consumed by the printers.
//!
//! Semantics are exactly the bytecode's: 32-bit wrapping integers; `and`/`or` are *not* short-circuit
//! (both operands are always evaluated, result 0/1); comparisons yield 0/1; `if` tests `!= 0`.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BinOp {
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum IncDec {
    PreInc,
    PostInc,
    PreDec,
    PostDec,
}

/// A storage location visible to scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Var {
    /// G0..G3: global words shared by all scripts.
    G(u8),
    /// P0..P3: the run's parameter block.
    P(u8),
    /// Game variable with getter native n / setter n (8 <= n < 18).
    Game(u8),
    /// Decompiler temporary (evaluation-order spill).
    Temp(u16),
    /// Operand-stack slot live across a block boundary.
    Slot(u8),
    /// Loop counter at loop-stack depth d.
    Counter(u8),
}

impl Var {
    pub fn from_index(n: u8) -> Var {
        match n {
            0..=3 => Var::G(n),
            4..=7 => Var::P(n - 4),
            _ => Var::Game(n),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Expr {
    Int(i32),
    Var(Var),
    /// Native call that returns a value.
    Call { native: u8, args: Vec<Expr> },
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    /// `++v`, `v++`, ... (game var or G/P): reads the getter, writes the setter.
    IncDec(IncDec, Var),
}

impl Expr {
    /// Does evaluating the expression call into the host / write state?
    pub fn has_effects(&self) -> bool {
        match self {
            Expr::Int(_) => false,
            Expr::Var(Var::Game(_)) => true, // getter natives are host calls
            Expr::Var(_) => false,
            Expr::Call { .. } | Expr::IncDec(..) => true,
            Expr::Bin(_, a, b) => a.has_effects() || b.has_effects(),
            Expr::Neg(a) => a.has_effects(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Stmt {
    /// Native call used as a statement (no result, or result discarded with `pop`).
    Call { native: u8, args: Vec<Expr> },
    /// Native call whose completion may take frames (the original blocked in a nested main loop).
    /// `into` receives the result when the native returns one.
    Await { native: u8, args: Vec<Expr>, into: Option<Var> },
    /// `run_script(idx[, p0[, p1]])` — nested script sharing the param block.
    RunScript { script: Expr, p0: Option<Expr>, p1: Option<Expr> },
    Set(Var, Expr),
    /// Evaluate and discard (bytecode `pop` of an expression with effects).
    Eval(Expr),
    If { cond: Expr, then: Vec<Stmt>, els: Vec<Stmt> },
    /// do { body } while (--counter != 0): `count` is evaluated once; 0 means 2^32 iterations on the GBA.
    Repeat { count: Expr, counter: Var, body: Vec<Stmt> },
    /// End of script; `values` = operand stack left behind (bottom first), read by some callers.
    Return(Vec<Expr>),
    /// Run-to-completion form only: end the handler; when the queued actions have finished, continue with
    /// step `k` (which receives the last action's result).
    Next(usize),
}

/// A decompiled script.
#[derive(Clone, Debug, Serialize)]
pub struct Func {
    pub name: String,
    pub body: Body,
    pub temps: u16,
    pub slots: u8,
    pub counters: u8,
}

#[derive(Clone, Debug, Serialize)]
pub enum Body {
    /// Fully structured.
    Structured(Vec<Stmt>),
    /// Fallback: basic blocks with explicit successors (printed as a `pc` state machine).
    Blocks(Vec<Block>),
}

#[derive(Clone, Debug, Serialize)]
pub struct Block {
    pub id: usize,
    pub stmts: Vec<Stmt>,
    pub exit: Exit,
}

#[derive(Clone, Debug, Serialize)]
pub enum Exit {
    Goto(usize),
    /// if cond != 0 goto then_ else goto else_
    Branch { cond: Expr, then_: usize, else_: usize },
    /// --counter != 0 ? back : out (pops the loop counter on exit)
    Loop { counter: Var, back: usize, out: usize },
    Return(Vec<Expr>),
}

impl Stmt {
    pub fn walk<'a>(stmts: &'a [Stmt], f: &mut dyn FnMut(&'a Stmt)) {
        for s in stmts {
            f(s);
            match s {
                Stmt::If { then, els, .. } => {
                    Stmt::walk(then, f);
                    Stmt::walk(els, f);
                }
                Stmt::Repeat { body, .. } => Stmt::walk(body, f),
                _ => {}
            }
        }
    }
}
