//! The Rhai backend of `wf_core::modkit::ModHooks`: the scripts of mod skills (`skills/*.json` `engine.script`),
//! projectiles (`projectiles/*.json` `script`) and AI nodes (`ModCondition` / `ModAction` `call`).
//!
//! A hook is called as `fn name(s)` with `s` = an object map of integers (`s.x`, `s.t`, `s.target`, ... plus
//! `s.vars` and `s.args` arrays); it returns an int (or a bool, or nothing = 0) and issues commands through the
//! functions registered here (`fire`, `anim`, `sfx`, `teleport`, `strike`, `goto_state`, ...). Nothing a script
//! does can reach the world directly or survive the call, so the core stays the only owner of state (snapshots,
//! rollback). Integers only (32-bit wrapping like the GBA), no clock, no I/O, operation budgets are counts: the
//! same results on every target, native and wasm32.
//!
//! Rhai's engine and compiled scripts are not `Send`, so each thread compiles the sources once on first use
//! (thread-local cache); the hook object itself only holds the sources and the function names.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rhai::{Array, Dynamic, Engine, ImmutableString, Map, AST, INT};
use wf_core::content::{ContentSource, GameData};
use wf_core::modkit::{Cmd, Ctx, ModHooks};

type R<T> = Result<T, Box<rhai::EvalAltResult>>;

/// operations one hook call may run (deterministic budget)
pub const MAX_OPS: u64 = 200_000;

pub struct RhaiHooks {
    id: u64,
    sources: BTreeMap<String, String>,
    fns: BTreeMap<String, BTreeSet<String>>,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static ASTS: RefCell<BTreeMap<(u64, String), AST>> = const { RefCell::new(BTreeMap::new()) };
    static OUT: RefCell<Vec<Cmd>> = const { RefCell::new(Vec::new()) };
}

fn push(c: Cmd) {
    OUT.with(|o| o.borrow_mut().push(c));
}

/// the engine: core + logic + arrays + maps, wrapping integer operators, the command functions
pub fn engine() -> Engine {
    let mut e = Engine::new_raw();
    {
        use rhai::packages::Package;
        e.register_global_module(rhai::packages::CorePackage::new().as_shared_module());
        e.register_global_module(rhai::packages::LogicPackage::new().as_shared_module());
        e.register_global_module(rhai::packages::BasicArrayPackage::new().as_shared_module());
        e.register_global_module(rhai::packages::BasicMapPackage::new().as_shared_module());
    }
    e.set_max_call_levels(32);
    e.set_max_expr_depths(128, 128);
    e.set_max_operations(MAX_OPS);
    e.set_fast_operators(false);
    e.register_fn("+", |a: INT, b: INT| a.wrapping_add(b));
    e.register_fn("-", |a: INT, b: INT| a.wrapping_sub(b));
    e.register_fn("*", |a: INT, b: INT| a.wrapping_mul(b));
    e.register_fn("-", |a: INT| a.wrapping_neg());
    e.register_fn("/", |a: INT, b: INT| -> R<INT> { if b == 0 { Err("division by zero".into()) } else { Ok(a.wrapping_div(b)) } });
    e.register_fn("%", |a: INT, b: INT| -> R<INT> { if b == 0 { Err("division by zero".into()) } else { Ok(a.wrapping_rem(b)) } });
    e.register_fn("int", |b: bool| b as INT);
    // helpers
    e.register_fn("isqrt", |n: INT| -> INT {
        // integer square root (floor), bit by bit
        let n = n.max(0) as u32;
        let (mut r, mut bit) = (0u32, 1u32 << 30);
        let mut x = n;
        while bit > n {
            bit >>= 2;
        }
        while bit != 0 {
            if x >= r + bit {
                x -= r + bit;
                r = (r >> 1) + bit;
            } else {
                r >>= 1;
            }
            bit >>= 2;
        }
        r as INT
    });
    e.register_fn("clamp", |v: INT, lo: INT, hi: INT| v.max(lo).min(hi));
    e.register_fn("min", |a: INT, b: INT| a.min(b));
    e.register_fn("max", |a: INT, b: INT| a.max(b));
    e.register_fn("abs", |a: INT| a.wrapping_abs());
    e.register_fn("dir_x", |d: INT| [0, 0, -1, 1][(d & 3) as usize]);
    e.register_fn("dir_y", |d: INT| [1, -1, 0, 0][(d & 3) as usize]);
    // commands
    e.register_fn("fire", |p: ImmutableString| push(Cmd::Fire { proj: p.to_string(), ox: 0, oy: 0, v: None }));
    e.register_fn("fire_at", |p: ImmutableString, ox: INT, oy: INT| push(Cmd::Fire { proj: p.to_string(), ox, oy, v: None }));
    e.register_fn("fire_dir", |p: ImmutableString, vx: INT, vy: INT| push(Cmd::Fire { proj: p.to_string(), ox: 0, oy: 0, v: Some((vx, vy)) }));
    e.register_fn("fire_from", |p: ImmutableString, ox: INT, oy: INT, vx: INT, vy: INT| push(Cmd::Fire { proj: p.to_string(), ox, oy, v: Some((vx, vy)) }));
    e.register_fn("anim", |n: ImmutableString| push(Cmd::Anim { name: n.to_string(), looping: false }));
    e.register_fn("anim_loop", |n: ImmutableString| push(Cmd::Anim { name: n.to_string(), looping: true }));
    e.register_fn("sfx", |id: INT| push(Cmd::Sfx(id)));
    e.register_fn("sfx_stop", |id: INT| push(Cmd::SfxStop(id)));
    e.register_fn("end", || push(Cmd::End));
    e.register_fn("set_var", |k: INT, v: INT| push(Cmd::SetVar(k.max(0) as usize, v)));
    e.register_fn("spend", |n: INT| push(Cmd::Spend(n)));
    e.register_fn("drain", |n: INT| push(Cmd::Drain(n)));
    e.register_fn("teleport", |x: INT, y: INT| push(Cmd::Teleport(x, y)));
    e.register_fn("strike", |w: INT, h: INT, mult: INT| push(Cmd::Strike { w, h, mult }));
    e.register_fn("face_target", || push(Cmd::FaceTarget));
    e.register_fn("face", |d: INT| push(Cmd::Face(d)));
    e.register_fn("goto_state", |s: INT| push(Cmd::Goto(s)));
    e.register_fn("push_state", |s: INT| push(Cmd::Push(s)));
    e.register_fn("pop_state", || push(Cmd::Pop));
    e.register_fn("heal", |n: INT| push(Cmd::Heal(n)));
    e.register_fn("summon", |ty: INT, x: INT, y: INT| push(Cmd::Summon { ty, x, y }));
    e.register_fn("shake", |d: INT, n: INT| push(Cmd::Shake(d, n)));
    e.register_fn("explode", |x: INT, y: INT| push(Cmd::Explode(x, y)));
    e.register_fn("flash", |n: INT| push(Cmd::Flash(n)));
    e.register_fn("hittable", |b: bool| push(Cmd::Hittable(b)));
    e.on_print(|_| {});
    e.on_debug(|_, _, _| {});
    e
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl RhaiHooks {
    /// compile every source once (errors name the file) and remember its functions
    pub fn new(sources: BTreeMap<String, String>) -> Result<RhaiHooks, String> {
        let e = engine();
        let mut fns = BTreeMap::new();
        for (path, src) in &sources {
            let ast = e.compile(src).map_err(|err| format!("{path}: {err}"))?;
            fns.insert(path.clone(), ast.iter_functions().map(|f| f.name.to_string()).collect());
        }
        Ok(RhaiHooks { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), sources, fns })
    }
    pub fn scripts(&self) -> impl Iterator<Item = &String> {
        self.sources.keys()
    }
}

fn ctx_map(c: &Ctx) -> Map {
    let mut m = Map::new();
    for (k, v) in &c.vals {
        m.insert((*k).into(), Dynamic::from_int(*v));
    }
    m.insert("vars".into(), Dynamic::from_array(c.vars.iter().map(|&v| Dynamic::from_int(v)).collect::<Array>()));
    m.insert("args".into(), Dynamic::from_array(c.args.iter().map(|&v| Dynamic::from_int(v)).collect::<Array>()));
    m
}

impl ModHooks for RhaiHooks {
    fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (p, src) in &self.sources {
            for b in p.bytes().chain([0]).chain(src.bytes()).chain([0]) {
                h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    }
    fn has(&self, script: &str, func: &str) -> bool {
        self.fns.get(script).is_some_and(|f| f.contains(func))
    }
    fn call(&self, script: &str, func: &str, ctx: &Ctx, out: &mut Vec<Cmd>) -> Result<i32, String> {
        let src = self.sources.get(script).ok_or_else(|| format!("no script {script}"))?;
        ENGINE.with(|eng| {
            let mut eng = eng.borrow_mut();
            let e = eng.get_or_insert_with(engine);
            ASTS.with(|asts| {
                let mut asts = asts.borrow_mut();
                let key = (self.id, script.to_string());
                if !asts.contains_key(&key) {
                    let ast = e.compile(src).map_err(|err| format!("{script}: {err}"))?;
                    asts.insert(key.clone(), ast);
                }
                let ast = &asts[&key];
                // a hook issuing commands may (through the core) run another hook: keep an outer call's commands
                let saved = OUT.with(|o| std::mem::take(&mut *o.borrow_mut()));
                let opts = rhai::CallFnOptions::new().eval_ast(false).rewind_scope(true);
                let r = e.call_fn_with_options::<Dynamic>(opts, &mut rhai::Scope::new(), ast, func, (Dynamic::from_map(ctx_map(ctx)),));
                let cmds = OUT.with(|o| std::mem::replace(&mut *o.borrow_mut(), saved));
                let v = r.map_err(|err| format!("{script}::{func}: {err}"))?;
                out.extend(cmds);
                Ok(if let Ok(i) = v.as_int() {
                    i
                } else if let Ok(b) = v.as_bool() {
                    b as i32
                } else {
                    0
                })
            })
        })
    }
}

/// Attach the hooks of every script the content refers to (`modkit::referenced_scripts`); without scripted content
/// `d.hooks` stays None. Errors: a referenced file that is missing or does not compile.
pub fn attach(d: &mut GameData, src: &dyn ContentSource) -> Result<(), String> {
    let wanted = wf_core::modkit::referenced_scripts(d);
    if wanted.is_empty() {
        return Ok(());
    }
    let mut sources = BTreeMap::new();
    for p in wanted {
        let b = src.read(&p).ok_or_else(|| format!("mod script {p} not found"))?;
        sources.insert(p.clone(), String::from_utf8(b).map_err(|_| format!("{p}: not UTF-8"))?);
    }
    d.hooks = Some(Arc::new(RhaiHooks::new(sources)?));
    Ok(())
}
