//! Run-to-completion form of a decompiled script ("action queue" model).
//!
//! The remake runs scripts as run-to-completion handlers (rollback netcode: a snapshot must be plain engine
//! data, never a suspended interpreter stack). The model:
//!
//! * every native **with an effect** (no result: `walk_to`, `say`, `play_sfx`, `set_hp`, …) is an *action*:
//!   calling it appends it to the thread's action queue; the engine runs queued actions in order after the
//!   handler returns, each one finishing (possibly over many frames) before the next starts — exactly the
//!   order and blocking of the original, where every such native ran to completion before returning;
//! * natives **with a result** (`get_flag`, `actor_x`, `rand`, …) are *queries*: they run immediately and
//!   are only legal while the queue is empty (they must observe the world after earlier actions);
//! * script variables (G, P, locals) are plain data, read and written immediately;
//! * `return next(k)` ends the handler; after the queue drains the engine calls step `k` with the last
//!   action's result (menus / mini-games that return a value);
//! * a plain `return v` ends the script once the queue has drained (`v` = node result).
//!
//! The decompiler inserts a split (`next`) exactly where a query (or a nested `run_script`, which runs
//! immediately) follows a queued action, after blocking natives that produce a result, after `run_event`
//! (it hands over the param block), and after a `run_script` whose callee itself splits. Control flow
//! around splits is converted to continuation steps (CPS); counters/temps that cross a split live in the
//! thread's `locals`.

use crate::ir::*;
use crate::natives;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub index: usize,
    /// Variable receiving the result of the action that ended the previous step.
    pub resume_into: Option<Var>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Program {
    pub name: String,
    /// Step 0 is the entry point. A program with one step never waits on its own.
    pub steps: Vec<Step>,
    /// Locals that cross a split: kept in the thread's `locals` (engine data), index = position.
    pub persistent: Vec<Var>,
    /// Built from the block graph (control flow was not structurable).
    pub from_blocks: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Q {
    Empty,
    /// Actions may be queued.
    Busy,
}

fn join(a: Q, b: Q) -> Q {
    if a == Q::Empty && b == Q::Empty {
        Q::Empty
    } else {
        Q::Busy
    }
}

/// Is this native a modal action that produces a value (menus, mini-games)?
pub fn is_result_action(n: u8) -> bool {
    natives::sig(n).pushes && natives::info(n).and_then(|i| i.kind.as_deref()) == Some("modal")
}

/// Instant natives (immediate, never block): with an empty queue they run at once and the queue stays
/// empty; behind queued actions they are queued like any action.
pub fn is_instant(n: u8) -> bool {
    natives::info(n)
        .map(|i| i.kind.as_deref() == Some("immediate") && !i.may_block() && !is_run_event(n))
        .unwrap_or(false)
        && !matches!(n, natives::RUN_SCRIPT | natives::RUN_SCRIPT_P0 | natives::RUN_SCRIPT_P0_P1)
}

fn is_run_event(n: u8) -> bool {
    (natives::RUN_EVENT..=natives::RUN_EVENT_P0_P3).contains(&n)
}

/// Does evaluating `e` call a query?
pub fn queries(e: &Expr) -> bool {
    match e {
        Expr::Int(_) => false,
        Expr::Var(Var::Game(_)) => true,
        Expr::Var(_) => false,
        Expr::Call { .. } | Expr::IncDec(..) => true,
        Expr::Bin(_, a, b) => queries(a) || queries(b),
        Expr::Neg(a) => queries(a),
    }
}

/// How a statement interacts with the queue.
enum Kind {
    /// Reads/writes only script state.
    Plain,
    /// Appends to the queue.
    Action,
    /// Runs now if the queue is empty, else queued.
    Instant,
    /// Action that ends the step (result, run_event); `into` gets the result.
    ActionThenSplit(Option<Var>),
    /// Nested script: needs an empty queue, may queue actions, splits after if the callee waits.
    Script(bool, bool),
    Compound,
}

struct Cps<'a> {
    steps: Vec<Option<Step>>,
    may_split_script: &'a dyn Fn(&Expr) -> (bool, bool),
}

impl<'a> Cps<'a> {
    fn kind(&self, s: &Stmt) -> Kind {
        match s {
            Stmt::Call { native, .. } => {
                if is_run_event(*native) {
                    Kind::ActionThenSplit(None)
                } else if is_instant(*native) {
                    Kind::Instant
                } else {
                    Kind::Action
                }
            }
            Stmt::Await { native, into, .. } => {
                if into.is_some() || is_result_action(*native) || is_run_event(*native) {
                    Kind::ActionThenSplit(*into)
                } else {
                    Kind::Action
                }
            }
            Stmt::RunScript { script, .. } => {
                let (split, queues) = (self.may_split_script)(script);
                Kind::Script(split, queues)
            }
            Stmt::Set(Var::Game(_), _) => Kind::Instant,
            Stmt::Set(_, Expr::IncDec(_, Var::Game(_))) => Kind::Instant,
            Stmt::If { .. } | Stmt::Repeat { .. } => Kind::Compound,
            _ => Kind::Plain,
        }
    }

    /// Does the statement evaluate a query before it acts?
    fn needs_empty(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Call { args, .. } | Stmt::Await { args, .. } => args.iter().any(queries),
            Stmt::RunScript { .. } => true,
            Stmt::Set(_, e) | Stmt::Eval(e) => queries(e),
            Stmt::If { cond, .. } => queries(cond),
            Stmt::Repeat { count, .. } => queries(count),
            Stmt::Return(v) => v.iter().any(queries),
            Stmt::Next(_) => false,
        }
    }

    /// Dry run: queue state after `stmts`, or None if a split would be needed.
    fn flow(&self, stmts: &[Stmt], mut q: Q) -> Option<Q> {
        for s in stmts {
            if self.needs_empty(s) && q != Q::Empty {
                return None;
            }
            match self.kind(s) {
                Kind::Plain => {
                    if matches!(s, Stmt::Return(_) | Stmt::Next(_)) {
                        return Some(q);
                    }
                }
                Kind::Action => q = Q::Busy,
                Kind::Instant => {}
                Kind::ActionThenSplit(_) => return None,
                Kind::Script(split, queues) => {
                    if split {
                        return None;
                    }
                    if queues {
                        q = Q::Busy;
                    }
                }
                Kind::Compound => match s {
                    Stmt::If { then, els, .. } => {
                        let a = self.flow(then, q)?;
                        let b = self.flow(els, q)?;
                        q = join(a, b);
                    }
                    Stmt::Repeat { body, .. } => {
                        let a = self.flow(body, q)?;
                        let b = self.flow(body, join(q, a))?;
                        q = join(q, join(a, b));
                    }
                    _ => unreachable!(),
                },
            }
        }
        Some(q)
    }

    fn new_step(&mut self) -> usize {
        self.steps.push(None);
        self.steps.len() - 1
    }

    /// Fill step `k`: it starts with an empty queue, runs `stmts`, then `tail` (continuation).
    fn fill(&mut self, k: usize, resume_into: Option<Var>, stmts: &[Stmt], tail: Option<usize>) {
        let mut out = Vec::new();
        self.seq(stmts, Q::Empty, tail, &mut out);
        self.steps[k] = Some(Step { index: k, resume_into, body: out });
    }

    /// Emit `stmts` starting in queue state `q`; at the end continue at step `tail` (or fall off).
    fn seq(&mut self, stmts: &[Stmt], mut q: Q, tail: Option<usize>, out: &mut Vec<Stmt>) {
        for (i, s) in stmts.iter().enumerate() {
            if self.needs_empty(s) && q != Q::Empty {
                let k = self.new_step();
                out.push(Stmt::Next(k));
                self.fill(k, None, &stmts[i..], tail);
                return;
            }
            let rest = &stmts[i + 1..];
            match self.kind(s) {
                Kind::Plain => {
                    out.push(s.clone());
                    if matches!(s, Stmt::Return(_) | Stmt::Next(_)) {
                        return;
                    }
                }
                Kind::Action => {
                    out.push(s.clone());
                    q = Q::Busy;
                }
                Kind::Instant => out.push(s.clone()),
                Kind::ActionThenSplit(into) => {
                    out.push(s.clone());
                    let k = self.new_step();
                    out.push(Stmt::Next(k));
                    self.fill(k, into, rest, tail);
                    return;
                }
                Kind::Script(split, queues) => {
                    out.push(s.clone());
                    if split {
                        let k = self.new_step();
                        out.push(Stmt::Next(k));
                        self.fill(k, None, rest, tail);
                        return;
                    }
                    if queues {
                        q = Q::Busy;
                    }
                }
                Kind::Compound => {
                    if let Some(q2) = self.flow(std::slice::from_ref(s), q) {
                        // no split inside: keep the statement as is
                        out.push(s.clone());
                        q = q2;
                        continue;
                    }
                    // continuation for the rest of this sequence
                    let cont = if rest.is_empty() {
                        tail
                    } else {
                        let k = self.new_step();
                        self.fill(k, None, rest, tail);
                        Some(k)
                    };
                    match s {
                        Stmt::If { cond, then, els } => {
                            let mut t = Vec::new();
                            let mut e = Vec::new();
                            self.seq(then, q, cont, &mut t);
                            self.seq(els, q, cont, &mut e);
                            out.push(Stmt::If { cond: cond.clone(), then: t, els: e });
                        }
                        Stmt::Repeat { count, counter, body } => {
                            // do { body } while (--counter != 0), with steps: head H, latch L
                            out.push(Stmt::Set(*counter, count.clone()));
                            let h = self.new_step();
                            let l = self.new_step();
                            out.push(Stmt::Next(h));
                            self.fill(h, None, body, Some(l));
                            let dec = Stmt::Set(
                                *counter,
                                Expr::Bin(BinOp::Sub, Box::new(Expr::Var(*counter)), Box::new(Expr::Int(1))),
                            );
                            let mut lb = vec![dec];
                            lb.push(Stmt::If {
                                cond: Expr::Bin(BinOp::Ne, Box::new(Expr::Var(*counter)), Box::new(Expr::Int(0))),
                                then: vec![Stmt::Next(h)],
                                els: vec![],
                            });
                            if let Some(c) = cont {
                                lb.push(Stmt::Next(c));
                            }
                            self.steps[l] = Some(Step { index: l, resume_into: None, body: lb });
                        }
                        _ => unreachable!(),
                    }
                    return;
                }
            }
        }
        if let Some(t) = tail {
            out.push(Stmt::Next(t));
        }
    }
}

// ---- variables crossing steps ----

fn expr_vars(e: &Expr, out: &mut BTreeSet<Var>) {
    match e {
        Expr::Var(v) | Expr::IncDec(_, v) => {
            out.insert(*v);
        }
        Expr::Call { args, .. } => args.iter().for_each(|a| expr_vars(a, out)),
        Expr::Bin(_, a, b) => {
            expr_vars(a, out);
            expr_vars(b, out);
        }
        Expr::Neg(a) => expr_vars(a, out),
        Expr::Int(_) => {}
    }
}

pub fn stmt_vars(s: &Stmt, out: &mut BTreeSet<Var>) {
    match s {
        Stmt::Call { args, .. } => args.iter().for_each(|a| expr_vars(a, out)),
        Stmt::Await { args, into, .. } => {
            args.iter().for_each(|a| expr_vars(a, out));
            out.extend(into.iter().copied());
        }
        Stmt::RunScript { script, p0, p1 } => {
            expr_vars(script, out);
            p0.iter().chain(p1.iter()).for_each(|a| expr_vars(a, out));
        }
        Stmt::Set(v, e) => {
            out.insert(*v);
            expr_vars(e, out);
        }
        Stmt::Eval(e) => expr_vars(e, out),
        Stmt::If { cond, then, els } => {
            expr_vars(cond, out);
            then.iter().chain(els.iter()).for_each(|s| stmt_vars(s, out));
        }
        Stmt::Repeat { count, counter, body } => {
            expr_vars(count, out);
            out.insert(*counter);
            body.iter().for_each(|s| stmt_vars(s, out));
        }
        Stmt::Return(v) => v.iter().for_each(|a| expr_vars(a, out)),
        Stmt::Next(_) => {}
    }
}

fn is_local(v: &Var) -> bool {
    matches!(v, Var::Temp(_) | Var::Slot(_) | Var::Counter(_))
}

/// Build the run-to-completion program. `may_split_script(idx_expr)`: may a nested `run_script` split?
pub fn build(f: &Func, blocks: &[Block], may_split_script: &dyn Fn(&Expr) -> (bool, bool)) -> Program {
    let mut c = Cps { steps: vec![None], may_split_script };
    let from_blocks = match &f.body {
        Body::Structured(body) => {
            c.fill(0, None, body, None);
            false
        }
        Body::Blocks(_) => {
            // one step per block (step index = block id + 1 so that step 0 is the entry jump)
            let n = blocks.len();
            c.steps = (0..=n).map(|_| None).collect();
            c.steps[0] = Some(Step { index: 0, resume_into: None, body: vec![Stmt::Next(1)] });
            for b in blocks {
                let mut stmts = b.stmts.clone();
                match &b.exit {
                    Exit::Goto(t) => stmts.push(Stmt::Next(t + 1)),
                    Exit::Branch { cond, then_, else_ } => stmts.push(Stmt::If {
                        cond: cond.clone(),
                        then: vec![Stmt::Next(then_ + 1)],
                        els: vec![Stmt::Next(else_ + 1)],
                    }),
                    Exit::Loop { counter, back, out } => {
                        stmts.push(Stmt::Set(
                            *counter,
                            Expr::Bin(BinOp::Sub, Box::new(Expr::Var(*counter)), Box::new(Expr::Int(1))),
                        ));
                        stmts.push(Stmt::If {
                            cond: Expr::Bin(BinOp::Ne, Box::new(Expr::Var(*counter)), Box::new(Expr::Int(0))),
                            then: vec![Stmt::Next(back + 1)],
                            els: vec![Stmt::Next(out + 1)],
                        });
                    }
                    Exit::Return(v) => stmts.push(Stmt::Return(v.clone())),
                }
                c.fill(b.id + 1, None, &stmts, None);
            }
            true
        }
    };
    let steps: Vec<Step> = c.steps.into_iter().map(|s| s.expect("every step filled")).collect();
    // locals used in more than one step persist
    let mut seen: BTreeMap<Var, BTreeSet<usize>> = BTreeMap::new();
    for st in &steps {
        let mut vs = BTreeSet::new();
        st.body.iter().for_each(|s| stmt_vars(s, &mut vs));
        vs.extend(st.resume_into);
        for v in vs.into_iter().filter(is_local) {
            seen.entry(v).or_default().insert(st.index);
        }
    }
    let persistent = seen.into_iter().filter(|(_, s)| s.len() > 1).map(|(v, _)| v).collect();
    Program { name: f.name.clone(), steps, persistent, from_blocks }
}
