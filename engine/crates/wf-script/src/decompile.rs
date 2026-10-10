//! Bytecode → structured IR.
//!
//! 1. Basic blocks + a dataflow pass for the operand-stack and loop-stack depth at every instruction
//!    (the compiler keeps them consistent; we check it).
//! 2. Each block is lifted with a symbolic stack. Evaluation order is preserved exactly: before any
//!    statement with effects, pending stack entries that read mutable state are spilled into temps in
//!    push order. Values live across block edges go through `Slot` variables.
//! 3. Structuring: the only backward branch is the counted `loop`; `jmp`/`jz` are forward-only, so
//!    if/else regions + repeat loops cover compiler output. Anything else falls back to a block graph.

use crate::bytecode::{successors, Insn, Op};
use crate::ir::*;
use crate::natives;
use crate::rom::Script;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct DecompileError(pub String);

impl std::fmt::Display for DecompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stack / loop depth before every instruction, and the `end` instructions reached with different stack
/// depths ("ragged" ends: e.g. `jz L; push 10; L: end` — a node that only sometimes picks the next node).
pub fn depths(sc: &Script) -> Result<(BTreeMap<usize, (usize, usize)>, BTreeSet<usize>), DecompileError> {
    let mut d: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut ragged = BTreeSet::new();
    let mut work = vec![(0usize, 0usize, 0usize)];
    while let Some((o, s, l)) = work.pop() {
        if let Some(&prev) = d.get(&o) {
            if prev != (s, l) {
                if matches!(sc.insns.get(&o).map(|i| &i.op), Some(Op::End)) {
                    ragged.insert(o);
                    continue;
                }
                return Err(DecompileError(format!(
                    "inconsistent depth at +{o:#x}: {prev:?} vs {:?}",
                    (s, l)
                )));
            }
            continue;
        }
        d.insert(o, (s, l));
        let ins = sc.insns.get(&o).ok_or_else(|| DecompileError(format!("no insn at +{o:#x}")))?;
        let (pop, push) = stack_effect(&ins.op);
        if s < pop {
            return Err(DecompileError(format!("stack underflow at +{o:#x} ({s} < {pop})")));
        }
        let s2 = s - pop + push;
        let next = ins.off + ins.len;
        match ins.op {
            Op::End => {}
            Op::LoopPush => work.push((next, s2, l + 1)),
            Op::Loop(t) => {
                if l == 0 {
                    return Err(DecompileError(format!("loop without counter at +{o:#x}")));
                }
                work.push((t, s2, l));
                work.push((next, s2, l - 1));
            }
            _ => {
                for t in successors(ins) {
                    work.push((t, s2, l));
                }
            }
        }
    }
    Ok((d, ragged))
}

pub fn stack_effect(op: &Op) -> (usize, usize) {
    match op {
        Op::PushU8(_) | Op::Push(_) => (0, 1),
        Op::PushN(v) => (0, v.len()),
        Op::Native(n) => {
            let s = natives::sig(*n);
            (s.pops as usize, s.pushes as usize)
        }
        Op::Set(_) | Op::Pop | Op::Jz(_) | Op::LoopPush => (1, 0),
        Op::And | Op::Or | Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Eq | Op::Ne | Op::Gt | Op::Ge
        | Op::Lt | Op::Le => (2, 1),
        Op::Neg => (1, 1),
        Op::End | Op::Jmp(_) | Op::Loop(_) | Op::Inc(_) | Op::Dec(_) => (0, 0),
        Op::PreInc(_) | Op::PostInc(_) | Op::PreDec(_) | Op::PostDec(_) => (0, 1),
    }
}

struct RawBlock {
    off: usize,
    end: usize,
    insns: Vec<Insn>,
}

struct Lifted {
    off: usize,
    end: usize,
    stmts: Vec<Stmt>,
    exit: Exit,
}

struct Lifter {
    temps: u16,
    max_slot: u8,
    max_counter: u8,
    ragged: BTreeSet<usize>,
    /// Return blocks made for edges into ragged ends (offsets from `synth_base` up).
    synth: Vec<Lifted>,
    synth_base: usize,
}

fn reads_state(e: &Expr) -> bool {
    match e {
        Expr::Int(_) => false,
        Expr::Var(Var::Temp(_)) => false,
        Expr::Var(_) => true,
        Expr::Call { .. } | Expr::IncDec(..) => true,
        Expr::Bin(_, a, b) => reads_state(a) || reads_state(b),
        Expr::Neg(a) => reads_state(a),
    }
}

fn reads_slot(e: &Expr) -> bool {
    match e {
        Expr::Var(Var::Slot(_)) => true,
        Expr::Bin(_, a, b) => reads_slot(a) || reads_slot(b),
        Expr::Neg(a) => reads_slot(a),
        Expr::Call { args, .. } => args.iter().any(reads_slot),
        _ => false,
    }
}

impl Lifter {
    fn temp(&mut self) -> Var {
        self.temps += 1;
        Var::Temp(self.temps - 1)
    }

    /// Spill every stack entry that reads mutable state (keeps evaluation order and read-before-write).
    fn spill(&mut self, st: &mut [Expr], out: &mut Vec<Stmt>) {
        for e in st.iter_mut() {
            if reads_state(e) {
                let t = self.temp();
                out.push(Stmt::Set(t, std::mem::replace(e, Expr::Var(t))));
            }
        }
    }

    fn lift(&mut self, b: &RawBlock, depth: &BTreeMap<usize, (usize, usize)>) -> Result<Lifted, DecompileError> {
        let (d0, _) = depth[&b.off];
        let mut st: Vec<Expr> = (0..d0).map(|i| Expr::Var(Var::Slot(i as u8))).collect();
        let mut out = Vec::new();
        let mut exit = None;
        let pop = |st: &mut Vec<Expr>, o: usize| st.pop().ok_or_else(|| DecompileError(format!("underflow +{o:#x}")));
        for ins in &b.insns {
            let o = ins.off;
            let next = ins.off + ins.len;
            match &ins.op {
                Op::PushU8(v) => st.push(Expr::Int(*v as i32)),
                Op::Push(v) => st.push(Expr::Int(*v)),
                Op::PushN(vs) => st.extend(vs.iter().map(|&v| Expr::Int(v))),
                Op::Native(n) => {
                    let n = *n;
                    let sig = natives::sig(n);
                    let k = st.len().checked_sub(sig.pops as usize).ok_or_else(|| DecompileError(format!("underflow +{o:#x}")))?;
                    let args = st.split_off(k);
                    match n {
                        0..=7 => st.push(Expr::Var(Var::from_index(n))),
                        natives::RUN_SCRIPT | natives::RUN_SCRIPT_P0 | natives::RUN_SCRIPT_P0_P1 => {
                            self.spill(&mut st, &mut out);
                            let mut it = args.into_iter();
                            out.push(Stmt::RunScript { script: it.next().unwrap(), p0: it.next(), p1: it.next() });
                        }
                        _ => {
                            let blocks = natives::info(n).map(|i| i.may_block()).unwrap_or(false);
                            if blocks {
                                self.spill(&mut st, &mut out);
                                let into = if sig.pushes { Some(self.temp()) } else { None };
                                out.push(Stmt::Await { native: n, args, into });
                                if let Some(t) = into {
                                    st.push(Expr::Var(t));
                                }
                            } else if sig.pushes {
                                // stays an expression; earlier effectful entries are spilled first so that a
                                // later statement cannot reorder them relative to this call
                                st.push(Expr::Call { native: n, args });
                            } else {
                                self.spill(&mut st, &mut out);
                                out.push(Stmt::Call { native: n, args });
                            }
                        }
                    }
                    // (expressions consume their operands in push order and target languages evaluate
                    // left to right, so calls nested in one expression keep their order without spilling)
                }
                Op::Set(n) => {
                    let v = pop(&mut st, o)?;
                    self.spill(&mut st, &mut out);
                    out.push(Stmt::Set(Var::from_index(*n), v));
                }
                Op::And | Op::Or | Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Eq | Op::Ne | Op::Gt
                | Op::Ge | Op::Lt | Op::Le => {
                    let b2 = pop(&mut st, o)?;
                    let a = pop(&mut st, o)?;
                    let op = match ins.op {
                        Op::And => BinOp::And,
                        Op::Or => BinOp::Or,
                        Op::Add => BinOp::Add,
                        Op::Sub => BinOp::Sub,
                        Op::Mul => BinOp::Mul,
                        Op::Div => BinOp::Div,
                        Op::Mod => BinOp::Mod,
                        Op::Eq => BinOp::Eq,
                        Op::Ne => BinOp::Ne,
                        Op::Gt => BinOp::Gt,
                        Op::Ge => BinOp::Ge,
                        Op::Lt => BinOp::Lt,
                        _ => BinOp::Le,
                    };
                    st.push(Expr::Bin(op, Box::new(a), Box::new(b2)));
                }
                Op::Neg => {
                    let a = pop(&mut st, o)?;
                    st.push(Expr::Neg(Box::new(a)));
                }
                Op::Pop => {
                    let a = pop(&mut st, o)?;
                    if a.has_effects() {
                        self.spill(&mut st, &mut out);
                        out.push(Stmt::Eval(a));
                    }
                }
                Op::Inc(n) | Op::Dec(n) => {
                    self.spill(&mut st, &mut out);
                    let v = Var::from_index(*n);
                    let op = if matches!(ins.op, Op::Inc(_)) { BinOp::Add } else { BinOp::Sub };
                    out.push(Stmt::Set(v, Expr::Bin(op, Box::new(Expr::Var(v)), Box::new(Expr::Int(1)))));
                }
                Op::PreInc(n) | Op::PostInc(n) | Op::PreDec(n) | Op::PostDec(n) => {
                    // has effects: spill older effectful entries so this one cannot overtake them
                    self.spill(&mut st, &mut out);
                    let k = match ins.op {
                        Op::PreInc(_) => IncDec::PreInc,
                        Op::PostInc(_) => IncDec::PostInc,
                        Op::PreDec(_) => IncDec::PreDec,
                        _ => IncDec::PostDec,
                    };
                    // materialise immediately: the write happens now
                    let t = self.temp();
                    out.push(Stmt::Set(t, Expr::IncDec(k, Var::from_index(*n))));
                    st.push(Expr::Var(t));
                }
                Op::LoopPush => {
                    let c = pop(&mut st, o)?;
                    self.spill(&mut st, &mut out);
                    let (_, l) = depth[&o];
                    self.max_counter = self.max_counter.max(l as u8 + 1);
                    out.push(Stmt::Set(Var::Counter(l as u8), c));
                }
                Op::End => {
                    exit = Some(Exit::Return(std::mem::take(&mut st)));
                }
                Op::Jmp(t) => {
                    if self.ragged.contains(t) {
                        exit = Some(Exit::Return(std::mem::take(&mut st)));
                    } else {
                        self.flush_slots(&mut st, &mut out);
                        exit = Some(Exit::Goto(*t));
                    }
                }
                Op::Jz(t) if self.ragged.contains(t) || self.ragged.contains(&next) => {
                    // one side goes straight to a ragged `end`: give that edge its own return block
                    let c = pop(&mut st, o)?;
                    self.spill(&mut st, &mut out);
                    let ret = |l: &mut Lifter, st: &Vec<Expr>| {
                        let off = l.synth_base + l.synth.len();
                        l.synth.push(Lifted { off, end: off, stmts: vec![], exit: Exit::Return(st.clone()) });
                        off
                    };
                    let else_ = if self.ragged.contains(t) { ret(self, &st) } else { *t };
                    let then_ = if self.ragged.contains(&next) { ret(self, &st) } else { next };
                    let cond = if !st.is_empty() && (then_ == next || else_ == *t) {
                        let tv = self.temp();
                        out.push(Stmt::Set(tv, c));
                        Expr::Var(tv)
                    } else {
                        c
                    };
                    if then_ == next || else_ == *t {
                        let mut st2 = st.clone();
                        self.flush_slots(&mut st2, &mut out);
                    }
                    exit = Some(Exit::Branch { cond, then_, else_ });
                }
                Op::Jz(t) => {
                    let c = pop(&mut st, o)?;
                    if !st.is_empty() {
                        // keep the condition's evaluation before the slot writes
                        let tv = self.temp();
                        self.spill(&mut st, &mut out);
                        out.push(Stmt::Set(tv, c));
                        self.flush_slots(&mut st, &mut out);
                        exit = Some(Exit::Branch { cond: Expr::Var(tv), then_: next, else_: *t });
                    } else {
                        exit = Some(Exit::Branch { cond: c, then_: next, else_: *t });
                    }
                }
                Op::Loop(t) => {
                    self.flush_slots(&mut st, &mut out);
                    let (_, l) = depth[&o];
                    exit = Some(Exit::Loop { counter: Var::Counter(l as u8 - 1), back: *t, out: next });
                }
            }
        }
        let exit = match exit {
            Some(e) => e,
            None if self.ragged.contains(&b.end) => Exit::Return(st),
            None => {
                self.flush_slots(&mut st, &mut out);
                Exit::Goto(b.end)
            }
        };
        Ok(Lifted { off: b.off, end: b.end, stmts: out, exit })
    }

    /// Write the remaining symbolic stack into Slot variables at a block exit.
    fn flush_slots(&mut self, st: &mut Vec<Expr>, out: &mut Vec<Stmt>) {
        if st.is_empty() {
            return;
        }
        let need_copy = st.iter().enumerate().any(|(i, e)| *e != Expr::Var(Var::Slot(i as u8)) && reads_slot(e));
        if need_copy {
            for e in st.iter_mut() {
                if reads_slot(e) {
                    let t = self.temp();
                    out.push(Stmt::Set(t, std::mem::replace(e, Expr::Var(t))));
                }
            }
        } else {
            self.spill(st, out);
        }
        for (i, e) in st.drain(..).enumerate() {
            self.max_slot = self.max_slot.max(i as u8 + 1);
            if e != Expr::Var(Var::Slot(i as u8)) {
                out.push(Stmt::Set(Var::Slot(i as u8), e));
            }
        }
    }
}

fn split_blocks(sc: &Script) -> Vec<RawBlock> {
    let mut leaders: BTreeSet<usize> = BTreeSet::new();
    leaders.insert(0);
    for ins in sc.insns.values() {
        match ins.op {
            Op::Jmp(t) | Op::Jz(t) | Op::Loop(t) => {
                leaders.insert(t);
                leaders.insert(ins.off + ins.len);
            }
            Op::End => {
                leaders.insert(ins.off + ins.len);
            }
            _ => {}
        }
    }
    let mut blocks = Vec::new();
    let mut cur: Option<RawBlock> = None;
    for ins in sc.insns.values() {
        let starts = leaders.contains(&ins.off) || cur.as_ref().map(|b| b.end != ins.off).unwrap_or(true);
        if starts {
            if let Some(b) = cur.take() {
                blocks.push(b);
            }
            cur = Some(RawBlock { off: ins.off, end: ins.off, insns: Vec::new() });
        }
        let b = cur.as_mut().unwrap();
        b.end = ins.off + ins.len;
        b.insns.push(ins.clone());
        if matches!(ins.op, Op::End | Op::Jmp(_) | Op::Jz(_) | Op::Loop(_)) {
            blocks.push(cur.take().unwrap());
        }
    }
    if let Some(b) = cur.take() {
        blocks.push(b);
    }
    blocks
}

struct Structurer {
    blocks: BTreeMap<usize, Lifted>,
    /// header offset → latch offset
    loops: BTreeMap<usize, usize>,
    emitted: BTreeSet<usize>,
    synth_base: usize,
}

impl Structurer {
    fn err<T>(m: String) -> Result<T, DecompileError> {
        Err(DecompileError(m))
    }

    /// Structure the region starting at `start` up to (not including) `end`; a `goto follow` leaves it.
    fn region(&mut self, start: usize, end: usize, follow: usize, out: &mut Vec<Stmt>) -> Result<(), DecompileError> {
        let mut pc = start;
        while pc < end {
            if let Some(&latch) = self.loops.get(&pc) {
                // repeat loop: the preceding statement set the counter
                let (count, counter) = match out.pop() {
                    Some(Stmt::Set(c @ Var::Counter(_), e)) => (e, c),
                    other => {
                        return Self::err(format!("loop header +{pc:#x} without counter set: {other:?}"));
                    }
                };
                let lb = self.blocks.get(&latch).ok_or_else(|| DecompileError("latch".into()))?;
                let (latch_end, out_off) = match lb.exit {
                    Exit::Loop { out, .. } => (lb.end, out),
                    _ => return Self::err("latch".into()),
                };
                let mut body = Vec::new();
                self.loops.remove(&pc);
                self.region(pc, latch_end, out_off, &mut body)?;
                out.push(Stmt::Repeat { count, counter, body });
                pc = out_off;
                continue;
            }
            let Some(b) = self.blocks.get(&pc) else { return Self::err(format!("no block at +{pc:#x}")) };
            if !self.emitted.insert(pc) {
                return Self::err(format!("block +{pc:#x} emitted twice"));
            }
            out.extend(b.stmts.iter().cloned());
            let (bend, exit) = (b.end, b.exit.clone());
            match exit {
                Exit::Return(v) => {
                    out.push(Stmt::Return(v));
                    pc = self.blocks.range(bend..end).next().map(|(k, _)| *k).unwrap_or(end);
                }
                Exit::Goto(t) => {
                    if t == bend {
                        pc = t;
                    } else if t == follow && (end == follow || bend == end) {
                        // leaving the region
                        if bend != end {
                            return Self::err(format!("early exit from +{pc:#x} to +{t:#x}"));
                        }
                        return Ok(());
                    } else {
                        return Self::err(format!("unstructured jmp at +{pc:#x} → +{t:#x}"));
                    }
                }
                Exit::Loop { out: o, .. } => {
                    // latch of the enclosing repeat
                    if bend == end && o == follow {
                        return Ok(());
                    }
                    return Self::err(format!("stray loop latch at +{pc:#x}"));
                }
                Exit::Branch { cond, then_, else_ } if then_ >= self.synth_base || else_ >= self.synth_base => {
                    let ret = |o: usize, me: &mut Self| -> Vec<Stmt> {
                        if o >= me.synth_base {
                            me.emitted.insert(o);
                            match &me.blocks[&o].exit {
                                Exit::Return(v) => vec![Stmt::Return(v.clone())],
                                _ => unreachable!(),
                            }
                        } else {
                            vec![]
                        }
                    };
                    let then = ret(then_, self);
                    let els = ret(else_, self);
                    out.push(Stmt::If { cond, then, els });
                    let cont = if then_ < self.synth_base { then_ } else if else_ < self.synth_base { else_ } else { bend };
                    if cont < pc {
                        return Self::err(format!("backward branch at +{pc:#x}"));
                    }
                    pc = self.blocks.range(cont..end).next().map(|(k, _)| *k).unwrap_or(end);
                    if cont > end {
                        return Self::err(format!("branch to follow at +{pc:#x}"));
                    }
                }
                Exit::Branch { cond, then_, else_ } => {
                    if else_ <= pc || else_ > end.max(follow) {
                        return Self::err(format!("branch at +{pc:#x} to +{else_:#x} leaves region [{start:#x},{end:#x})"));
                    }
                    if else_ > end {
                        return Self::err(format!("branch to follow at +{pc:#x}"));
                    }
                    // last block of the then-part
                    let last = self.blocks.range(then_..else_).next_back().map(|(_, b)| (b.off, b.end, b.exit.clone()));
                    let mut then = Vec::new();
                    let mut els = Vec::new();
                    match last {
                        Some((_, lend, Exit::Goto(e))) if lend == else_ && e > else_ && e <= end => {
                            self.region(then_, else_, e, &mut then)?;
                            self.region(else_, e, e, &mut els)?;
                            pc = e;
                        }
                        Some((_, lend, Exit::Goto(e))) if lend == else_ && e > else_ && e == follow && end < follow => {
                            // jump threading: the then-part jumps straight to the enclosing follow; the
                            // else-part runs to the end of this region (which also continues at `follow`)
                            let tail = self.blocks.range(else_..end).next_back().map(|(_, b)| b.exit.clone());
                            if !matches!(tail, Some(Exit::Goto(t)) if t == follow) && !matches!(tail, Some(Exit::Return(_))) {
                                return Self::err(format!("threaded if-else at +{pc:#x} without exit to follow"));
                            }
                            self.region(then_, else_, follow, &mut then)?;
                            self.region(else_, end, follow, &mut els)?;
                            pc = end;
                        }
                        _ => {
                            if then_ < else_ {
                                self.region(then_, else_, else_, &mut then)?;
                            }
                            pc = else_;
                        }
                    }
                    out.push(Stmt::If { cond, then, els });
                }
            }
        }
        Ok(())
    }
}

/// Result of decompiling one script: the structured function (or the block graph when the control flow is
/// not structurable) and always the block graph (used for the state-machine form).
#[derive(Clone, Debug)]
pub struct Decompiled {
    /// Why structuring failed (the body is then the block graph).
    pub structure_error: Option<String>,
    pub func: Func,
    pub blocks: Vec<Block>,
}

/// Decompile one script to IR. Fails only if the bytecode itself is inconsistent.
pub fn decompile(sc: &Script, name: &str) -> Result<Decompiled, DecompileError> {
    let (depth, ragged) = depths(sc)?;
    let raw = split_blocks(sc);
    let code_end = raw.last().map(|b| b.end).unwrap_or(0);
    let synth_base = code_end + 1;
    let mut lifter =
        Lifter { temps: 0, max_slot: 0, max_counter: 0, ragged: ragged.clone(), synth: vec![], synth_base };
    let mut lifted = BTreeMap::new();
    for b in &raw {
        if ragged.contains(&b.off) {
            continue; // every edge into it got its own return
        }
        let l = lifter.lift(b, &depth)?;
        lifted.insert(l.off, l);
    }
    for l in std::mem::take(&mut lifter.synth) {
        lifted.insert(l.off, l);
    }
    let mut loops = BTreeMap::new();
    for b in lifted.values() {
        if let Exit::Loop { back, .. } = b.exit {
            if loops.insert(back, b.off).is_some() {
                loops.clear();
                break;
            }
        }
    }
    let ids: BTreeMap<usize, usize> = lifted.keys().enumerate().map(|(i, &o)| (o, i)).collect();
    let id = |o: usize| ids.get(&o).copied().unwrap_or(usize::MAX);
    let blocks: Vec<Block> = lifted
        .values()
        .map(|b| Block {
            id: id(b.off),
            stmts: b.stmts.clone(),
            exit: match &b.exit {
                Exit::Goto(t) => Exit::Goto(id(*t)),
                Exit::Branch { cond, then_, else_ } => Exit::Branch { cond: cond.clone(), then_: id(*then_), else_: id(*else_) },
                Exit::Loop { counter, back, out } => Exit::Loop { counter: *counter, back: id(*back), out: id(*out) },
                Exit::Return(v) => Exit::Return(v.clone()),
            },
        })
        .collect();
    let mut s = Structurer { blocks: lifted, loops, emitted: BTreeSet::new(), synth_base };
    let mut body = Vec::new();
    let r = s.region(0, code_end, code_end, &mut body);
    let structure_error = match r {
        Err(e) => Some(e.0),
        Ok(()) if s.emitted.len() != s.blocks.len() => {
            Some(format!("{} of {} blocks placed", s.emitted.len(), s.blocks.len()))
        }
        Ok(()) => None,
    };
    let body = if structure_error.is_none() { Body::Structured(body) } else { Body::Blocks(blocks.clone()) };
    Ok(Decompiled {
        structure_error,
        func: Func { name: name.to_string(), body, temps: lifter.temps, slots: lifter.max_slot, counters: lifter.max_counter },
        blocks,
    })
}
