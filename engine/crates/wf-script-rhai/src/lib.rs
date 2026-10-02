//! Rhai runtime for Buu's Fury scripts (decompiled originals and mods).
//!
//! **Action-queue model** (see `wf_script::steps` and `engine/docs/scripting.md`): a handler runs to
//! completion; natives with an effect are queued *actions* that the runtime executes in order after the
//! handler returns (each may span frames — the host says so by returning `Block`); natives with a result are
//! *queries* and need an empty queue; `return next(k)` continues at step `k` once the queue has drained.
//!
//! **Rollback.** All thread state ([`ThreadState`]: frames, locals, params, queue, pending result) is plain
//! data (`Clone + Serialize + PartialEq`); between frames nothing lives inside Rhai. Snapshot = clone it.
//! Rhai is used integer-only (`only_i32`, `no_float`), with wrapping arithmetic like the GBA.

pub mod check;

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, NativeCallContext, AST, INT};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use wf_script::natives;
use wf_script::steps::is_instant;
use wf_script::vm::{ext_info, Call, ExtCall, ExtKind, Globals, Host, NativeResult, NodeCall, SceneCall, ScriptKey, EXTENSIONS};

type R<T> = Result<T, Box<EvalAltResult>>;

/// Continue-at-step marker returned by `next(k)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Next(pub i32);

/// Which compiled program a frame runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub struct UnitRef {
    /// File key: `e450`, `inline_08072a48`.
    pub file: String,
    /// Function prefix inside the file (`n0_`, `n3_args_`, ``).
    pub prefix: String,
    /// `main` (single handler) or `steps`.
    pub steps: bool,
    /// Named entry function called with the triggering player (`on_talk(who)`), for mod handlers;
    /// continuations are `<prefix>step_k(result)` as usual. None = `main()` / `step_0()`.
    #[serde(default)]
    pub entry: Option<String>,
}

impl UnitRef {
    pub fn new(file: impl Into<String>, prefix: impl Into<String>, steps: bool) -> UnitRef {
        UnitRef { file: file.into(), prefix: prefix.into(), steps, entry: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Plain,
    EventNode,
    TextArgs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ret {
    Nothing,
    Int(i32),
    Many(Vec<i32>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame {
    Unit {
        unit: UnitRef,
        /// Next step to run (None = not started: run the entry).
        step: Option<i32>,
        locals: Vec<i32>,
        params: usize,
        role: Role,
        /// Returned; waits for the queue to drain before it is popped.
        done: Option<Ret>,
    },
    Event {
        event: u16,
        node: u16,
        params: usize,
        in_node: bool,
        /// the host was told the graph started (`Host::scene`)
        #[serde(default)]
        entered: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Act {
    Native { native: u8, args: Vec<i32>, params: usize, script: ScriptKey },
    SetVar { var: u8, value: i32, params: usize },
    RunEvent { event: u16, params: usize },
    Node(NodeCall),
    Ext { name: String, args: Vec<i32>, params: usize },
    /// the host blocked on an event graph's start
    Scene { event: u16 },
}

/// The complete state of one script thread — plain data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ThreadState {
    pub frames: Vec<Frame>,
    pub params: Vec<[i32; 4]>,
    pub queue: VecDeque<Act>,
    /// Result of the last executed action (passed to the next step).
    pub last: Option<i32>,
    /// The host blocked on the front action; `resume` delivers its result.
    pub blocked: Option<Act>,
    /// Frames of nested `run_script` callees that split, waiting to be pushed when the caller returns.
    pub nested: Vec<Frame>,
    pub result: Option<Ret>,
    pub event_result: Option<u16>,
    /// Text nodes whose argument code is running.
    pub text_node: Vec<u16>,
    /// The player that triggered this thread (1-based; 0 = engine). Original scripts ignore it.
    #[serde(default)]
    pub who: i32,
    pub ops: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GNode {
    End(u16),
    Code { prefix: String, steps: bool, next: u16 },
    Say { text: String, arg: u8, next: u16 },
    SayArgs { text: String, arg: u8, prefix: String, steps: bool, next: u16 },
    Choice { text: String, arg: u8, next: u16 },
    Goto { event: u16 },
}

/// Compiled script files + event graphs.
pub struct Library {
    pub asts: BTreeMap<String, AST>,
    pub graphs: BTreeMap<u16, BTreeMap<u16, GNode>>,
    /// event header address -> event index (from each event file's `// Event N (header 0xADDR)` line)
    pub headers: BTreeMap<u32, u16>,
}

struct Ctx {
    host: Box<dyn Host>,
    globals: Globals,
    t: ThreadState,
}

pub struct Runtime {
    engine: Engine,
    lib: Rc<Library>,
    ctx: Rc<RefCell<Ctx>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Finished,
    Blocked,
}

fn err<T>(m: impl Into<String>) -> R<T> {
    Err(m.into().into())
}

fn int_arg(d: &Dynamic) -> R<i32> {
    d.as_int().map_err(|t| format!("expected int, got {t}").into())
}

impl Library {
    /// Compile the exported sources (`scripts/events/eNNN.rhai`, `scripts/inline/*.rhai`).
    pub fn compile(engine: &Engine, files: &BTreeMap<String, String>) -> Result<Library, String> {
        let mut asts = BTreeMap::new();
        let mut graphs = BTreeMap::new();
        let mut headers = BTreeMap::new();
        for (path, src) in files {
            let key = path.rsplit('/').next().unwrap().trim_end_matches(".rhai").to_string();
            let ast = engine.compile(src).map_err(|e| format!("{path}: {e}"))?;
            if let Some(ev) = key.strip_prefix('s').and_then(|n| n.parse::<u16>().ok()) {
                let g: Array = engine
                    .call_fn(&mut rhai::Scope::new(), &ast, "event", ())
                    .map_err(|e| format!("{path}: event(): {e}"))?;
                graphs.insert(ev, parse_graph(g).map_err(|e| format!("{path}: {e}"))?);
                let hdr = src.lines().take(4).find_map(|l| l.split("(header 0x").nth(1)).and_then(|t| t.split(')').next());
                if let Some(h) = hdr.and_then(|t| u32::from_str_radix(t, 16).ok()) {
                    headers.insert(h, ev);
                }
            }
            asts.insert(key, ast);
        }
        Ok(Library { asts, graphs, headers })
    }
}

fn parse_graph(g: Array) -> Result<BTreeMap<u16, GNode>, String> {
    let mut out = BTreeMap::new();
    for d in g {
        let m = d.try_cast::<rhai::Map>().ok_or("graph entry is not a map")?;
        let geti = |k: &str| m.get(k).and_then(|v| v.as_int().ok());
        let gets = |k: &str| m.get(k).and_then(|v| v.clone().into_immutable_string().ok()).map(|s| s.to_string());
        let node = geti("node").ok_or("node")? as u16;
        let next = geti("next").unwrap_or(0) as u16;
        let arg = geti("box").unwrap_or(0) as u8;
        let steps = gets("entry").as_deref() == Some("steps");
        let n = if let Some(v) = geti("end") {
            GNode::End(v as u16)
        } else if let Some(p) = gets("code") {
            GNode::Code { prefix: p, steps, next }
        } else if let Some(t) = gets("say") {
            match gets("args") {
                Some(p) => GNode::SayArgs { text: t, arg, prefix: p, steps, next },
                None => GNode::Say { text: t, arg, next },
            }
        } else if let Some(t) = gets("choice") {
            GNode::Choice { text: t, arg, next }
        } else if let Some(e) = geti("jump") {
            GNode::Goto { event: e as u16 }
        } else {
            return Err(format!("node {node}: unknown entry"));
        };
        out.insert(node, n);
    }
    Ok(out)
}

/// Wrapping 32-bit arithmetic like the GBA (Rhai's built-ins are checked).
fn register_int_ops(e: &mut Engine) {
    e.set_fast_operators(false);
    e.register_fn("+", |a: INT, b: INT| a.wrapping_add(b));
    e.register_fn("-", |a: INT, b: INT| a.wrapping_sub(b));
    e.register_fn("*", |a: INT, b: INT| a.wrapping_mul(b));
    e.register_fn("-", |a: INT| a.wrapping_neg());
    e.register_fn("/", |a: INT, b: INT| -> R<INT> {
        if b == 0 {
            err("division by zero (the GBA faults)")
        } else {
            Ok(a.wrapping_div(b))
        }
    });
    e.register_fn("%", |a: INT, b: INT| -> R<INT> {
        if b == 0 {
            err("division by zero (the GBA faults)")
        } else {
            Ok(a.wrapping_rem(b))
        }
    });
    e.register_fn("int", |b: bool| b as INT);
    e.register_fn("both", |a: bool, b: bool| a && b);
    e.register_fn("either", |a: bool, b: bool| a || b);
}

impl Runtime {
    /// Build the engine with the full native API bound to `host`, and compile `files`.
    pub fn new(files: &BTreeMap<String, String>, host: Box<dyn Host>) -> Result<Runtime, String> {
        let ctx = Rc::new(RefCell::new(Ctx { host, globals: Globals::default(), t: ThreadState::default() }));
        let lib_cell: Rc<RefCell<Option<Rc<Library>>>> = Rc::new(RefCell::new(None));
        let mut engine = Engine::new_raw();
        {
            use rhai::packages::Package;
            engine.register_global_module(rhai::packages::CorePackage::new().as_shared_module());
            engine.register_global_module(rhai::packages::LogicPackage::new().as_shared_module());
            engine.register_global_module(rhai::packages::BasicArrayPackage::new().as_shared_module());
            engine.register_global_module(rhai::packages::BasicMapPackage::new().as_shared_module());
        }
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(256, 256);
        register_int_ops(&mut engine);
        engine.register_type_with_name::<Next>("Next");
        engine.register_fn("next", |k: INT| Next(k));
        bind_api(&mut engine, &ctx, &lib_cell);
        let lib = Rc::new(Library::compile(&engine, files)?);
        *lib_cell.borrow_mut() = Some(lib.clone());
        Ok(Runtime { engine, lib, ctx })
    }

    pub fn library(&self) -> &Library {
        &self.lib
    }

    /// Snapshot of the thread (plain data).
    pub fn state(&self) -> ThreadState {
        self.ctx.borrow().t.clone()
    }
    pub fn globals(&self) -> Globals {
        self.ctx.borrow().globals.clone()
    }
    pub fn set_globals(&self, g: Globals) {
        self.ctx.borrow_mut().globals = g;
    }
    /// Restore a snapshot.
    pub fn restore(&self, t: ThreadState) {
        self.ctx.borrow_mut().t = t;
    }
    pub fn with_host<T>(&self, f: impl FnOnce(&mut dyn Host) -> T) -> T {
        f(self.ctx.borrow_mut().host.as_mut())
    }

    /// Start a bytecode unit's program (an inline script, or `run_script` semantics on event node 0).
    pub fn start_unit(&self, unit: UnitRef, params: [i32; 4]) {
        let mut c = self.ctx.borrow_mut();
        c.t = ThreadState { params: vec![params], ..Default::default() };
        c.t.frames.push(Frame::Unit { unit, step: None, locals: vec![], params: 0, role: Role::Plain, done: None });
    }

    /// An event id as the core names it: `sNNN` (event index) or `header_0xADDR` (the event whose header is ADDR,
    /// queued by an enemy's RunScript AI node).
    pub fn event_id(&self, id: &str) -> Option<u16> {
        if let Some(h) = id.strip_prefix("header_0x") {
            let h = u32::from_str_radix(h, 16).ok()?;
            return self.library().headers.get(&h).copied();
        }
        id.trim_start_matches('s').parse::<u16>().ok()
    }

    /// Start a whole event.
    pub fn start_event(&self, event: u16, params: [i32; 4]) {
        self.start_event_by(event, params, 0);
    }

    /// Start a whole event triggered by player `who` (1-based; NPC talk, trigger touch, on-defeat by).
    pub fn start_event_by(&self, event: u16, params: [i32; 4], who: i32) {
        let mut c = self.ctx.borrow_mut();
        c.t = ThreadState { params: vec![params], who, ..Default::default() };
        c.t.frames.push(Frame::Event { event, node: 0, params: 0, in_node: false, entered: false });
    }

    /// Start a named handler `file::entry(who)` (mod scripts: `on_talk(who)`, `on_enter(who)`,
    /// `on_defeat(by)`, …). It may continue with `return next(k)` → `step_k(result)` in the same file.
    pub fn start_handler(&self, file: &str, entry: &str, who: i32, params: [i32; 4]) {
        let mut c = self.ctx.borrow_mut();
        c.t = ThreadState { params: vec![params], who, ..Default::default() };
        let unit = UnitRef { file: file.to_string(), prefix: String::new(), steps: true, entry: Some(entry.to_string()) };
        c.t.frames.push(Frame::Unit { unit, step: None, locals: vec![], params: 0, role: Role::Plain, done: None });
    }

    /// Deliver the result of the blocked action and continue.
    pub fn resume(&mut self, value: Option<i32>) -> Result<Status, String> {
        {
            let mut c = self.ctx.borrow_mut();
            let a = c.t.blocked.take().ok_or("not blocked")?;
            c.t.last = value;
            if let Act::Node(_) = a {
                if let Some(Frame::Event { in_node, .. }) = c.t.frames.last_mut() {
                    *in_node = false;
                }
            }
        }
        self.run()
    }

    /// Run until finished or an action blocks.
    pub fn run(&mut self) -> Result<Status, String> {
        loop {
            let mut c = self.ctx.borrow_mut();
            c.t.ops += 1;
            if c.t.ops > 5_000_000 {
                return Err("step limit".into());
            }
            if c.t.blocked.is_some() {
                return Ok(Status::Blocked);
            }
            // 1. queued actions first
            if let Some(a) = c.t.queue.pop_front() {
                let Ctx { host, globals, t } = &mut *c;
                match &a {
                    Act::Native { native, args, params, script } => {
                        let p = *params;
                        let r = host
                            .native(Call { native: *native, args, script: *script, pc: 0, params: &mut t.params[p], globals })
                            .map_err(|e| e.to_string())?;
                        match r {
                            NativeResult::Done(v) => t.last = v,
                            NativeResult::Block => {
                                t.blocked = Some(a);
                                return Ok(Status::Blocked);
                            }
                        }
                    }
                    Act::SetVar { var, value, params } => {
                        host.set_var(*var, *value, &mut t.params[*params], globals)?;
                    }
                    Act::RunEvent { event, params } => {
                        t.frames.push(Frame::Event { event: *event, node: 0, params: *params, in_node: false, entered: false });
                    }
                    Act::Ext { name, args, params } => {
                        let who = t.who;
                        let r = host
                            .ext(ExtCall { name, args, who, params: &mut t.params[*params], globals })
                            .map_err(|e| e.to_string())?;
                        match r {
                            NativeResult::Done(v) => t.last = v,
                            NativeResult::Block => {
                                t.blocked = Some(a);
                                return Ok(Status::Blocked);
                            }
                        }
                    }
                    Act::Scene { .. } => {}
                    Act::Node(n) => match host.node(n.clone())? {
                        NativeResult::Done(_) => {
                            if let Some(Frame::Event { in_node, .. }) = t.frames.last_mut() {
                                *in_node = false;
                            }
                        }
                        NativeResult::Block => {
                            t.blocked = Some(a);
                            return Ok(Status::Blocked);
                        }
                    },
                }
                continue;
            }
            // 2. advance the top frame
            let Some(top) = c.t.frames.last().cloned() else { return Ok(Status::Finished) };
            match top {
                Frame::Unit { done: Some(ret), role, .. } => {
                    c.t.frames.pop();
                    match role {
                        Role::Plain => {
                            if c.t.frames.is_empty() {
                                c.t.result = Some(ret);
                            }
                        }
                        Role::EventNode => {
                            if let Some(Frame::Event { node, in_node, .. }) = c.t.frames.last_mut() {
                                *in_node = false;
                                if let Ret::Int(v) = ret {
                                    *node = v as u16;
                                }
                            }
                        }
                        Role::TextArgs => {
                            let args = match ret {
                                Ret::Nothing => vec![],
                                Ret::Int(v) => vec![v],
                                Ret::Many(v) => v,
                            };
                            let Some(Frame::Event { event, .. }) = c.t.frames.last() else { unreachable!() };
                            let ev = *event;
                            let node = c.t.text_node.pop().ok_or("text node")?;
                            let Some(GNode::SayArgs { text, arg, .. }) = self.lib.graphs.get(&ev).and_then(|g| g.get(&node))
                            else {
                                return Err("text node".into());
                            };
                            let call =
                                NodeCall::Text { event: Some(ev), node, kind: 6, arg: *arg, text: text.clone(), ref_: 0, args };
                            c.t.queue.push_back(Act::Node(call));
                        }
                    }
                }
                Frame::Unit { unit, step, .. } => {
                    drop(c);
                    let (fname, args): (String, Vec<Dynamic>) = match step {
                        None if unit.entry.is_some() => {
                            let who = self.ctx.borrow().t.who;
                            (unit.entry.clone().unwrap(), vec![Dynamic::from_int(who)])
                        }
                        None if unit.steps => (format!("{}step_0", unit.prefix), vec![]),
                        None => (format!("{}main", unit.prefix), vec![]),
                        Some(k) => {
                            let last = self.ctx.borrow().t.last;
                            (format!("{}step_{k}", unit.prefix), vec![last.map(Dynamic::from_int).unwrap_or(Dynamic::UNIT)])
                        }
                    };
                    let v = call_unit(&self.engine, &self.lib, &unit.file, &fname, args)?;
                    let mut c = self.ctx.borrow_mut();
                    let nested = std::mem::take(&mut c.t.nested);
                    let Some(Frame::Unit { step, done, .. }) = c.t.frames.last_mut() else { unreachable!() };
                    if let Some(Next(k)) = v.clone().try_cast::<Next>() {
                        *step = Some(k);
                        // callees that split run first (deepest last = on top)
                        for f in nested {
                            c.t.frames.push(f);
                        }
                    } else {
                        if !nested.is_empty() {
                            return Err(format!("{fname}: returned while a nested script waits"));
                        }
                        *done = Some(to_ret(v)?);
                    }
                }
                Frame::Event { in_node: true, .. } => return Err("event node in progress with nothing to run".into()),
                Frame::Event { event, entered: false, .. } => {
                    if let Some(Frame::Event { entered, .. }) = c.t.frames.last_mut() {
                        *entered = true;
                    }
                    let Ctx { host, t, .. } = &mut *c;
                    if let NativeResult::Block = host.scene(SceneCall::Enter { event: Some(event) })? {
                        t.blocked = Some(Act::Scene { event });
                        return Ok(Status::Blocked);
                    }
                }
                Frame::Event { event, node, params, .. } => {
                    let g = self.lib.graphs.get(&event).ok_or(format!("no graph for event {event}"))?;
                    let n = g.get(&node).ok_or(format!("event {event}: no node {node}"))?.clone();
                    let Some(Frame::Event { node: cur, in_node, event: evf, .. }) = c.t.frames.last_mut() else { unreachable!() };
                    match n {
                        GNode::End(v) => {
                            c.host.scene(SceneCall::Exit { event: Some(event) })?;
                            c.t.frames.pop();
                            if c.t.frames.is_empty() {
                                c.t.event_result = Some(v);
                            }
                        }
                        GNode::Code { prefix, steps, next } => {
                            *cur = next;
                            *in_node = true;
                            let unit = UnitRef::new(format!("s{event:03}"), prefix, steps);
                            c.t.frames.push(Frame::Unit { unit, step: None, locals: vec![], params, role: Role::EventNode, done: None });
                        }
                        GNode::SayArgs { prefix, steps, next, .. } => {
                            *cur = next;
                            *in_node = true;
                            c.t.text_node.push(node);
                            let unit = UnitRef::new(format!("s{event:03}"), prefix, steps);
                            c.t.frames.push(Frame::Unit { unit, step: None, locals: vec![], params, role: Role::TextArgs, done: None });
                        }
                        GNode::Say { text, arg, next } => {
                            *cur = next;
                            *in_node = true;
                            c.t.queue.push_back(Act::Node(NodeCall::Text {
                                event: Some(event),
                                node,
                                kind: 5,
                                arg,
                                text,
                                ref_: 0,
                                args: vec![],
                            }));
                        }
                        GNode::Choice { text, arg, next } => {
                            *cur = next;
                            *in_node = true;
                            c.t.queue.push_back(Act::Node(NodeCall::Choice { event: Some(event), node, arg, text }));
                        }
                        GNode::Goto { event: e } => {
                            *evf = e;
                            *cur = 0;
                        }
                    }
                }
            }
        }
    }
}

fn to_ret(v: Dynamic) -> Result<Ret, String> {
    if v.is_unit() {
        return Ok(Ret::Nothing);
    }
    if let Ok(i) = v.as_int() {
        return Ok(Ret::Int(i));
    }
    if v.is_array() {
        let a: Array = v.cast();
        return Ok(Ret::Many(a.iter().map(|x| x.as_int().unwrap_or(0)).collect()));
    }
    Err(format!("bad script result type {}", v.type_name()))
}

fn call_unit(engine: &Engine, lib: &Library, file: &str, fname: &str, args: Vec<Dynamic>) -> Result<Dynamic, String> {
    let ast = lib.asts.get(file).ok_or(format!("no script file {file}"))?;
    let opts = rhai::CallFnOptions::new().eval_ast(false).rewind_scope(true);
    engine
        .call_fn_with_options::<Dynamic>(opts, &mut rhai::Scope::new(), ast, fname, args)
        .map_err(|e| format!("{file}::{fname}: {e}"))
}

impl ThreadState {
    fn cur_unit(&mut self) -> R<(&mut Vec<i32>, usize, ScriptKey)> {
        for f in self.frames.iter_mut().rev() {
            if let Frame::Unit { locals, params, unit, .. } = f {
                let key = unit_key(unit);
                return Ok((locals, *params, key));
            }
        }
        err("no running script")
    }
}

fn unit_key(u: &UnitRef) -> ScriptKey {
    if let Some(a) = u.file.strip_prefix("inline_0x") {
        return ScriptKey::Inline(u32::from_str_radix(a, 16).unwrap_or(0));
    }
    let event = u.file.trim_start_matches('s').parse().unwrap_or(0);
    let node = u.prefix.trim_start_matches('n').split('_').next().and_then(|n| n.parse().ok()).unwrap_or(0);
    if u.prefix.contains("args") {
        ScriptKey::Args { event, node }
    } else {
        ScriptKey::Node { event, node }
    }
}

fn bind_api(e: &mut Engine, ctx: &Rc<RefCell<Ctx>>, lib: &Rc<RefCell<Option<Rc<Library>>>>) {
    use std::any::TypeId;
    // script variables
    let c = ctx.clone();
    e.register_fn("g", move |i: INT| -> R<INT> { c.borrow().globals.g.get(i as usize).copied().ok_or_else(|| "G index".into()) });
    let c = ctx.clone();
    e.register_fn("set_g", move |i: INT, v: INT| -> R<()> {
        *c.borrow_mut().globals.g.get_mut(i as usize).ok_or("G index")? = v;
        Ok(())
    });
    let c = ctx.clone();
    e.register_fn("p", move |i: INT| -> R<INT> {
        let mut b = c.borrow_mut();
        let (_, p, _) = b.t.cur_unit()?;
        b.t.params[p].get(i as usize).copied().ok_or_else(|| "P index".into())
    });
    let c = ctx.clone();
    e.register_fn("set_p", move |i: INT, v: INT| -> R<()> {
        let mut b = c.borrow_mut();
        let (_, p, _) = b.t.cur_unit()?;
        *b.t.params[p].get_mut(i as usize).ok_or("P index")? = v;
        Ok(())
    });
    let c = ctx.clone();
    e.register_fn("local", move |i: INT| -> R<INT> {
        let mut b = c.borrow_mut();
        let (l, _, _) = b.t.cur_unit()?;
        Ok(l.get(i as usize).copied().unwrap_or(0))
    });
    let c = ctx.clone();
    e.register_fn("set_local", move |i: INT, v: INT| -> R<()> {
        let mut b = c.borrow_mut();
        let (l, _, _) = b.t.cur_unit()?;
        if l.len() <= i as usize {
            l.resize(i as usize + 1, 0);
        }
        l[i as usize] = v;
        Ok(())
    });

    // natives
    for n in 8..natives::NATIVE_COUNT as u8 {
        let info = natives::info(n).unwrap();
        let sig = natives::sig(n);
        let types = vec![TypeId::of::<INT>(); sig.pops as usize];
        let c = ctx.clone();
        let name = info.name.clone();
        if (natives::RUN_SCRIPT..=natives::RUN_SCRIPT_P0_P1).contains(&n) {
            let lib = lib.clone();
            e.register_raw_fn(name, types, move |nc: NativeCallContext, args: &mut [&mut Dynamic]| -> R<()> {
                let a: Vec<i32> = args.iter().map(|d| int_arg(d)).collect::<R<_>>()?;
                run_script(&nc, &c, &lib, &a)
            });
            continue;
        }
        if (natives::RUN_EVENT..=natives::RUN_EVENT_P0_P3).contains(&n) {
            e.register_raw_fn(name, types, move |_nc: NativeCallContext, args: &mut [&mut Dynamic]| -> R<()> {
                let a: Vec<i32> = args.iter().map(|d| int_arg(d)).collect::<R<_>>()?;
                let mut b = c.borrow_mut();
                let (_, p, _) = b.t.cur_unit()?;
                for (k, &v) in a[1..].iter().enumerate() {
                    b.t.params[p][k] = v;
                }
                b.t.queue.push_back(Act::RunEvent { event: a[0] as u16, params: p });
                Ok(())
            });
            continue;
        }
        let query = sig.pushes && !wf_script::steps::is_result_action(n);
        let instant = is_instant(n);
        e.register_raw_fn(name, types, move |_nc: NativeCallContext, args: &mut [&mut Dynamic]| -> R<Dynamic> {
            let a: Vec<i32> = args.iter().map(|d| int_arg(d)).collect::<R<_>>()?;
            let mut b = c.borrow_mut();
            let (_, p, script) = b.t.cur_unit()?;
            let now = query || (instant && b.t.queue.is_empty());
            if !now {
                b.t.queue.push_back(Act::Native { native: n, args: a, params: p, script });
                return Ok(Dynamic::UNIT);
            }
            if query && !b.t.queue.is_empty() {
                return err(format!(
                    "query native 0x{n:02x} called while actions are queued; end the handler with `return next(k)` first"
                ));
            }
            let Ctx { host, globals, t } = &mut *b;
            let r = host
                .native(Call { native: n, args: &a, script, pc: 0, params: &mut t.params[p], globals })
                .map_err(|e| -> Box<EvalAltResult> { e.into() })?;
            match r {
                NativeResult::Done(Some(v)) if query => Ok(Dynamic::from_int(v)),
                NativeResult::Done(_) if !query => Ok(Dynamic::UNIT),
                NativeResult::Done(None) => err(format!("query 0x{n:02x} returned nothing")),
                NativeResult::Block => err(format!("native 0x{n:02x} blocked where it must be instant")),
                _ => unreachable!(),
            }
        });
    }
    // the triggering player of this thread (plain thread data)
    let c = ctx.clone();
    e.register_fn("who", move || -> INT { c.borrow().t.who });
    // multiplayer extensions (same query / instant / action rules as natives)
    for &(name, arity, kind) in EXTENSIONS {
        let c = ctx.clone();
        let types = vec![TypeId::of::<INT>(); arity as usize];
        e.register_raw_fn(name, types, move |_nc: NativeCallContext, args: &mut [&mut Dynamic]| -> R<Dynamic> {
            let a: Vec<i32> = args.iter().map(|d| int_arg(d)).collect::<R<_>>()?;
            let mut b = c.borrow_mut();
            let (_, p, _) = b.t.cur_unit()?;
            let now = kind == ExtKind::Query || (kind == ExtKind::Instant && b.t.queue.is_empty());
            if !now {
                b.t.queue.push_back(Act::Ext { name: name.to_string(), args: a, params: p });
                return Ok(Dynamic::UNIT);
            }
            if kind == ExtKind::Query && !b.t.queue.is_empty() {
                return err(format!("query `{name}` called while actions are queued; end the handler with `return next(k)` first"));
            }
            let Ctx { host, globals, t } = &mut *b;
            let who = t.who;
            let r = host
                .ext(ExtCall { name, args: &a, who, params: &mut t.params[p], globals })
                .map_err(|e| -> Box<EvalAltResult> { e.into() })?;
            match (kind, r) {
                (ExtKind::Query, NativeResult::Done(Some(v))) => Ok(Dynamic::from_int(v)),
                (ExtKind::Query, _) => err(format!("query `{name}` returned nothing")),
                (_, NativeResult::Done(_)) => Ok(Dynamic::UNIT),
                (_, NativeResult::Block) => err(format!("`{name}` blocked where it must be instant")),
            }
        });
        let _ = ext_info(name);
    }
    // game-variable setters (8..17): instant
    for v in 8..natives::SETTER_COUNT as u8 {
        let c = ctx.clone();
        let name = format!("set_{}", natives::var_name(v).to_lowercase());
        e.register_fn(name, move |x: INT| -> R<()> {
            let mut b = c.borrow_mut();
            let (_, p, _) = b.t.cur_unit()?;
            if b.t.queue.is_empty() {
                let Ctx { host, globals, t } = &mut *b;
                host.set_var(v, x, &mut t.params[p], globals).map_err(|e| -> Box<EvalAltResult> { e.into() })
            } else {
                b.t.queue.push_back(Act::SetVar { var: v, value: x, params: p });
                Ok(())
            }
        });
    }
    let _ = ImmutableString::new();
}

/// `run_script(idx[, p0[, p1]])`: runs node 0 of event `idx` now (it must start with an empty queue),
/// sharing the caller's P block. If the callee splits, its frame is pushed above the caller when the caller
/// returns `next(k)` (which the decompiler guarantees).
fn run_script(nc: &NativeCallContext, c: &Rc<RefCell<Ctx>>, lib: &Rc<RefCell<Option<Rc<Library>>>>, a: &[i32]) -> R<()> {
    let lib = lib.borrow().clone().ok_or("library not ready")?;
    let (p, unit) = {
        let mut b = c.borrow_mut();
        if !b.t.queue.is_empty() {
            return err("run_script called while actions are queued; end the handler with `return next(k)` first");
        }
        let (_, p, _) = b.t.cur_unit()?;
        for (k, &v) in a[1..].iter().enumerate() {
            b.t.params[p][k] = v;
        }
        let idx = a[0];
        let g = lib.graphs.get(&(idx as u16)).ok_or_else(|| format!("run_script({idx}): no such event"))?;
        let Some(GNode::Code { prefix, steps, .. }) = g.get(&0) else {
            return err(format!("run_script({idx}): node 0 is not bytecode (the GBA asserts)"));
        };
        let unit = UnitRef::new(format!("s{idx:03}"), prefix.clone(), *steps);
        b.t.frames.push(Frame::Unit { unit: unit.clone(), step: None, locals: vec![], params: p, role: Role::Plain, done: None });
        (p, unit)
    };
    let fname = if unit.steps { format!("{}step_0", unit.prefix) } else { format!("{}main", unit.prefix) };
    let ast = lib.asts.get(&unit.file).ok_or("no file")?;
    let opts = rhai::CallFnOptions::new().eval_ast(false).rewind_scope(true);
    let v = nc.engine().call_fn_with_options::<Dynamic>(opts, &mut rhai::Scope::new(), ast, &fname, ());
    let mut b = c.borrow_mut();
    let frame = b.t.frames.pop().ok_or("frame")?;
    let v = v?;
    if let Some(Next(k)) = v.try_cast::<Next>() {
        let Frame::Unit { unit, locals, .. } = frame else { unreachable!() };
        // callee's own nested frames were collected after it; it goes below them
        let pos = b.t.nested.len();
        let _ = pos;
        b.t.nested.insert(0, Frame::Unit { unit, step: Some(k), locals, params: p, role: Role::Plain, done: None });
    }
    Ok(())
}
