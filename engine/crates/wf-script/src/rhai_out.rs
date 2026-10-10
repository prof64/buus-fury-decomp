//! Rhai source printer for decompiled scripts (the mod language, see `engine/docs/scripting.md`).
//!
//! Conventions (the runtime in `wf-script-rhai` implements them):
//! * every native is a function with its API name (`walk_to(149, 30, 60)`); values are 32-bit ints;
//! * `g(i)`/`set_g(i, v)`: globals G0..G3; `p(i)`/`set_p(i, v)`: the run's parameter block;
//!   `set_<var>(v)`: game-variable setters (`set_hp`, `set_zenie`, …);
//! * `local(i)`/`set_local(i, v)`: thread locals that survive an await (engine data, snapshot-safe);
//! * a handler that must wait ends with the waiting call followed by `return next(k)`: the engine finishes
//!   the action and then calls `step_k(result)` (or `machine(k, result)`);
//! * a plain `return v` ends the script (`v` = the node's result: an int picks the next event node);
//! * `both(a, b)` / `either(a, b)`: non-short-circuit and/or (the original always evaluates both sides);
//!   `int(cond)` turns a comparison into 0/1.

use crate::ir::*;
use crate::natives;
use crate::rom::{Entry, Event, NodeBody, ScriptId, ScriptSet};
use crate::steps::Program;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Naming / annotation hooks (e.g. string-table lookups for comments).
pub struct Names<'a> {
    pub string: &'a dyn Fn(i32) -> Option<String>,
}

struct P<'a> {
    out: String,
    ind: usize,
    locals: BTreeMap<Var, usize>,
    names: &'a Names<'a>,
}

fn var_local_name(v: Var) -> String {
    match v {
        Var::Temp(i) => format!("t{i}"),
        Var::Slot(i) => format!("s{i}"),
        Var::Counter(i) => format!("n{i}"),
        _ => unreachable!(),
    }
}

fn setter_name(n: u8) -> String {
    format!("set_{}", natives::var_name(n).to_lowercase())
}

fn native_name(n: u8) -> String {
    natives::name(n)
}

fn binop(op: BinOp) -> &'static str {
    match op {
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
    }
}

fn prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Eq | BinOp::Ne => 3,
        BinOp::Gt | BinOp::Ge | BinOp::Lt | BinOp::Le => 4,
        BinOp::Add | BinOp::Sub => 5,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 6,
    }
}

/// Does evaluating `e` do anything observable (beyond pure reads)? Then `and`/`or` must not short-circuit.
fn impure(e: &Expr) -> bool {
    match e {
        Expr::Call { native, args } => !natives::is_pure_query(*native) || args.iter().any(impure),
        Expr::IncDec(..) => true,
        Expr::Bin(_, a, b) => impure(a) || impure(b),
        Expr::Neg(a) => impure(a),
        _ => false,
    }
}

#[allow(dead_code)]
fn is_bool(e: &Expr) -> bool {
    matches!(e, Expr::Bin(op, ..) if prec(*op) <= 4)
}

impl<'a> P<'a> {
    fn line(&mut self, s: &str) {
        for _ in 0..self.ind {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn read(&self, v: Var) -> String {
        match v {
            Var::G(i) => format!("g({i})"),
            Var::P(i) => format!("p({i})"),
            Var::Game(n) => format!("{}()", native_name(n)),
            _ => match self.locals.get(&v) {
                Some(i) => format!("local({i})"),
                None => var_local_name(v),
            },
        }
    }

    /// Statement writing `v` (`decl`: first assignment of a plain local → `let`).
    fn write(&self, v: Var, val: String, declared: &mut std::collections::BTreeSet<Var>) -> String {
        match v {
            Var::G(i) => format!("set_g({i}, {val});"),
            Var::P(i) => format!("set_p({i}, {val});"),
            Var::Game(n) => format!("{}({val});", setter_name(n)),
            _ => match self.locals.get(&v) {
                Some(i) => format!("set_local({i}, {val});"),
                None => {
                    if declared.insert(v) {
                        format!("let {} = {val};", var_local_name(v))
                    } else {
                        format!("{} = {val};", var_local_name(v))
                    }
                }
            },
        }
    }

    fn call(&self, n: u8, args: &[Expr]) -> String {
        let a: Vec<String> = args.iter().map(|x| self.int(x, 0)).collect();
        let mut s = format!("{}({})", native_name(n), a.join(", "));
        // annotate string ids
        if let Some(info) = natives::info(n) {
            for (i, an) in info.args.iter().enumerate() {
                if an.contains("string") {
                    if let Some(Expr::Int(id)) = args.get(i) {
                        if let Some(t) = (self.names.string)(*id) {
                            s.push_str(&format!(" /* \"{}\" */", t.replace("*/", "* /")));
                        }
                    }
                }
            }
        }
        s
    }

    /// Expression in integer context. `min` = minimum precedence the context requires.
    fn int(&self, e: &Expr, min: u8) -> String {
        match e {
            Expr::Int(v) => {
                if *v < 0 && min > 0 {
                    format!("({v})")
                } else {
                    v.to_string()
                }
            }
            Expr::Var(v) => self.read(*v),
            Expr::Call { native, args } => self.call(*native, args),
            Expr::Neg(a) => format!("-{}", self.int(a, 7)),
            Expr::IncDec(..) => unreachable!("inc/dec is lifted into statements"),
            Expr::Bin(op, a, b) if prec(*op) <= 4 => format!("int({})", self.cond(&Expr::Bin(*op, a.clone(), b.clone()), 0)),
            Expr::Bin(op, a, b) => {
                let p = prec(*op);
                let s = format!("{} {} {}", self.int(a, p), binop(*op), self.int(b, p + 1));
                if p < min {
                    format!("({s})")
                } else {
                    s
                }
            }
        }
    }

    /// Expression in boolean context (`!= 0` test).
    fn cond(&self, e: &Expr, min: u8) -> String {
        match e {
            Expr::Bin(op @ (BinOp::And | BinOp::Or), a, b) => {
                if impure(b) {
                    let f = if *op == BinOp::And { "both" } else { "either" };
                    format!("{f}({}, {})", self.cond(a, 0), self.cond(b, 0))
                } else {
                    let p = prec(*op);
                    let s = format!("{} {} {}", self.cond(a, p), binop(*op), self.cond(b, p + 1));
                    if p < min {
                        format!("({s})")
                    } else {
                        s
                    }
                }
            }
            Expr::Bin(op, a, b) if prec(*op) <= 4 => {
                let p = prec(*op);
                let s = format!("{} {} {}", self.int(a, p + 1), binop(*op), self.int(b, p + 1));
                if p < min {
                    format!("({s})")
                } else {
                    s
                }
            }
            _ => {
                let s = format!("{} != 0", self.int(e, 4));
                if min > 3 {
                    format!("({s})")
                } else {
                    s
                }
            }
        }
    }

    fn not_cond(&self, e: &Expr) -> String {
        match e {
            Expr::Bin(BinOp::Eq, a, b) => format!("{} != {}", self.int(a, 4), self.int(b, 4)),
            Expr::Bin(BinOp::Ne, a, b) => format!("{} == {}", self.int(a, 4), self.int(b, 4)),
            Expr::Bin(BinOp::Lt, a, b) => format!("{} >= {}", self.int(a, 5), self.int(b, 5)),
            Expr::Bin(BinOp::Le, a, b) => format!("{} > {}", self.int(a, 5), self.int(b, 5)),
            Expr::Bin(BinOp::Gt, a, b) => format!("{} <= {}", self.int(a, 5), self.int(b, 5)),
            Expr::Bin(BinOp::Ge, a, b) => format!("{} < {}", self.int(a, 5), self.int(b, 5)),
            Expr::Bin(..) => format!("!({})", self.cond(e, 0)),
            _ => format!("{} == 0", self.int(e, 4)),
        }
    }

    fn ret(&self, v: &[Expr]) -> String {
        match v.len() {
            0 => "return;".into(),
            1 => format!("return {};", self.int(&v[0], 0)),
            _ => format!("return [{}];", v.iter().map(|x| self.int(x, 0)).collect::<Vec<_>>().join(", ")),
        }
    }

    fn stmts(&mut self, ss: &[Stmt], declared: &mut std::collections::BTreeSet<Var>) {
        for s in ss {
            self.stmt(s, declared);
        }
    }

    fn incdec(&mut self, k: IncDec, v: Var, into: Var, declared: &mut std::collections::BTreeSet<Var>) {
        // getter, then setter with old±1; the expression value is new (pre) or old (post)
        let (delta, pre) = match k {
            IncDec::PreInc => ("+ 1", true),
            IncDec::PostInc => ("+ 1", false),
            IncDec::PreDec => ("- 1", true),
            IncDec::PostDec => ("- 1", false),
        };
        let r = self.read(v);
        if pre {
            let l = self.write(into, format!("{r} {delta}"), declared);
            self.line(&l);
            let rv = self.read(into);
            let l = self.write(v, rv, declared);
            self.line(&l);
        } else {
            let l = self.write(into, r, declared);
            self.line(&l);
            let rv = self.read(into);
            let l = self.write(v, format!("{rv} {delta}"), declared);
            self.line(&l);
        }
    }

    fn stmt(&mut self, s: &Stmt, declared: &mut std::collections::BTreeSet<Var>) {
        match s {
            Stmt::Call { native, args } => {
                let c = self.call(*native, args);
                self.line(&format!("{c};"));
            }
            Stmt::Await { native, args, .. } => {
                // an action; a result (menus) arrives as `result` of the next step
                let c = self.call(*native, args);
                self.line(&format!("{c};"));
            }
            Stmt::RunScript { script, p0, p1 } => {
                let mut a = vec![self.int(script, 0)];
                a.extend(p0.iter().chain(p1.iter()).map(|x| self.int(x, 0)));
                self.line(&format!("run_script({});", a.join(", ")));
            }
            Stmt::Set(v, Expr::IncDec(k, var)) => self.incdec(*k, *var, *v, declared),
            Stmt::Set(v, e) => {
                let val = self.int(e, 0);
                let l = self.write(*v, val, declared);
                self.line(&l);
            }
            Stmt::Eval(e) => {
                let v = self.int(e, 0);
                self.line(&format!("{v};"));
            }
            Stmt::If { cond, then, els } => {
                if then.is_empty() && !els.is_empty() {
                    let c = self.not_cond(cond);
                    self.line(&format!("if {c} {{"));
                    self.ind += 1;
                    self.stmts(els, declared);
                    self.ind -= 1;
                    self.line("}");
                    return;
                }
                let c = self.cond(cond, 0);
                self.line(&format!("if {c} {{"));
                self.ind += 1;
                self.stmts(then, declared);
                self.ind -= 1;
                let mut els = els.as_slice();
                // else-if chains
                loop {
                    if els.len() == 1 {
                        if let Stmt::If { cond, then, els: e2 } = &els[0] {
                            if !then.is_empty() {
                                let c = self.cond(cond, 0);
                                self.line(&format!("}} else if {c} {{"));
                                self.ind += 1;
                                self.stmts(then, declared);
                                self.ind -= 1;
                                els = e2.as_slice();
                                continue;
                            }
                        }
                    }
                    break;
                }
                if !els.is_empty() {
                    self.line("} else {");
                    self.ind += 1;
                    self.stmts(els, declared);
                    self.ind -= 1;
                }
                self.line("}");
            }
            Stmt::Repeat { count, counter, body } => {
                let c = self.int(count, 0);
                let l = self.write(*counter, c, declared);
                self.line(&l);
                self.line("loop {");
                self.ind += 1;
                self.stmts(body, declared);
                let r = self.read(*counter);
                let l = self.write(*counter, format!("{r} - 1"), declared);
                self.line(&l);
                let r = self.read(*counter);
                self.line(&format!("if {r} == 0 {{ break; }}"));
                self.ind -= 1;
                self.line("}");
            }
            Stmt::Return(v) => {
                let r = self.ret(v);
                self.line(&r);
            }
            Stmt::Next(k) => self.line(&format!("return next({k});")),
        }
    }

    /// Declare every plain local assigned in `body` up front (Rhai `let` is block-scoped; slots and temps
    /// are assigned in branches and read after the join).
    fn declare(&mut self, body: &[Stmt], extra: Option<Var>) -> std::collections::BTreeSet<Var> {
        // variables assigned once at the top level get a `let` at the assignment; the rest up front
        let mut count: BTreeMap<Var, usize> = BTreeMap::new();
        Stmt::walk(body, &mut |s| match s {
            Stmt::Set(v, _) | Stmt::Repeat { counter: v, .. } => {
                *count.entry(*v).or_default() += 1;
            }
            Stmt::Await { into: Some(v), .. } => {
                *count.entry(*v).or_default() += 1;
            }
            _ => {}
        });
        let mut top: BTreeMap<Var, usize> = BTreeMap::new();
        for s in body {
            if let Stmt::Set(v, _) = s {
                *top.entry(*v).or_default() += 1;
            }
        }
        let mut all: std::collections::BTreeSet<Var> =
            count.iter().filter(|(v, n)| **n > 1 || top.get(v) != Some(&1)).map(|(v, _)| *v).collect();
        all.extend(extra);
        let vs: Vec<Var> = all
            .into_iter()
            .filter(|v| matches!(v, Var::Temp(_) | Var::Slot(_) | Var::Counter(_)) && !self.locals.contains_key(v))
            .collect();
        if !vs.is_empty() {
            let names: Vec<String> = vs.iter().map(|v| format!("{} = 0", var_local_name(*v))).collect();
            // one `let` per variable (Rhai has no multi-declaration)
            for n in names {
                self.line(&format!("let {n};"));
            }
        }
        vs.into_iter().collect()
    }

    fn resume(&mut self, into: Option<Var>, declared: &mut std::collections::BTreeSet<Var>) {
        if let Some(v) = into {
            let l = self.write(v, "result".into(), declared);
            self.line(&l);
        }
    }
}

/// Print one program. `prefix` namespaces its functions inside an event file (`n3_` …); the entry point is
/// `<prefix>main()` when the program has a single step, else `<prefix>step_0()` (then `step_k(result)`).
pub fn program(prog: &Program, prefix: &str, names: &Names) -> String {
    let locals: BTreeMap<Var, usize> = prog.persistent.iter().enumerate().map(|(i, v)| (*v, i)).collect();
    let mut p = P { out: String::new(), ind: 0, locals, names };
    if !p.locals.is_empty() {
        let l: Vec<String> = p.locals.iter().map(|(v, i)| format!("{i} = {}", var_local_name(*v))).collect();
        p.line(&format!("// locals kept across waits: {}", l.join(", ")));
    }
    let single = prog.steps.len() == 1;
    for st in &prog.steps {
        if single {
            p.line(&format!("fn {prefix}main() {{"));
        } else if st.index == 0 {
            p.line(&format!("fn {prefix}step_0() {{"));
        } else {
            p.line(&format!("fn {prefix}step_{}(result) {{", st.index));
        }
        p.ind += 1;
        let mut d = p.declare(&st.body, st.resume_into);
        p.resume(st.resume_into, &mut d);
        p.stmts(&st.body, &mut d);
        p.ind -= 1;
        p.line("}");
    }
    p.out
}

fn rhai_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            _ => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Entry-point kind of a printed program.
pub fn entry_kind(prog: &Program) -> &'static str {
    if prog.steps.len() == 1 {
        "main"
    } else {
        "steps"
    }
}

/// Print a whole event: its graph as data (`fn event()`) followed by the node programs.
pub fn event(ev: &Event, set: &ScriptSet, progs: &BTreeMap<ScriptId, Program>, names: &Names, header_comment: &str) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "// Event {} (header 0x{:08X}). {header_comment}", ev.index, ev.header);
    let _ = writeln!(o, "// Graph: one entry per node; code nodes name their functions (prefix `nN_`).");
    let _ = writeln!(o, "fn event() {{");
    let _ = writeln!(o, "    [");
    for (k, e) in &ev.entries {
        match e {
            Entry::Terminal(v) => {
                let _ = writeln!(o, "        #{{ node: {k}, end: {v} }},");
            }
            Entry::Node(n) => {
                let body = match &n.body {
                    NodeBody::Code(id) => {
                        let kind = progs.get(id).map(entry_kind).unwrap_or("main");
                        format!("code: \"n{k}_\", entry: \"{kind}\"")
                    }
                    NodeBody::Text { text, .. } => format!("say: {}, box: {}", rhai_str(text), n.arg),
                    NodeBody::TextArgs { text, args, .. } => {
                        let kind = progs.get(args).map(entry_kind).unwrap_or("main");
                        format!("say: {}, box: {}, args: \"n{k}_args_\", entry: \"{kind}\"", rhai_str(text), n.arg)
                    }
                    NodeBody::Choice { text } => format!("choice: {}, box: {}", rhai_str(text), n.arg),
                    NodeBody::Goto { header, event } => match event {
                        Some(e) => format!("jump: {e}"),
                        None => format!("jump_header: {header}"),
                    },
                    NodeBody::Unknown { kind } => format!("unknown: {kind}"),
                };
                let _ = writeln!(o, "        #{{ node: {k}, {body}, next: {} }},", n.next);
            }
        }
    }
    let _ = writeln!(o, "    ]");
    let _ = writeln!(o, "}}");
    for (k, e) in &ev.entries {
        if let Entry::Node(n) = e {
            let (id, pre) = match &n.body {
                NodeBody::Code(id) => (*id, format!("n{k}_")),
                NodeBody::TextArgs { args, .. } => (*args, format!("n{k}_args_")),
                _ => continue,
            };
            if let Some(pr) = progs.get(&id) {
                let _ = writeln!(o, "\n// node {k}");
                o.push_str(&program(pr, &pre, names));
            }
            let _ = set;
        }
    }
    o
}
