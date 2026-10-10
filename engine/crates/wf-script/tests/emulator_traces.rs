//! Differential verification (a): the real game (emulator traces recorded by `re/buu/script_trace.py`,
//! `work/buu/script/traces/*.json`) vs the bytecode interpreter.
//!
//! Every root activation in a trace (a scene/event the engine started, or a script the engine ran) is
//! replayed in `vm::Thread` with a host that answers each native with the result the game produced. At every
//! host call the interpreter must issue the same native, with the same arguments, at the same code offset,
//! with the same G0..G3 / P0..P3; game-variable writes and text/choice nodes must match too; scripts started
//! by nested `run_script` / `run_event` are followed inside the same replay. Unrelated scripts that the game
//! ran while ours was blocked (nested main loop) are skipped. Skipped when the traces or the ROM are absent.

use serde_json::Value;
use std::path::PathBuf;
use wf_script::natives;
use wf_script::rom::{Entry, ScriptId};
use wf_script::vm::{Act, Call, EventAct, Globals, Host, NativeResult, NodeCall, Thread};
use wf_script::ScriptSet;

#[derive(Debug, Clone)]
enum Exp {
    Native { id: u8, args: Vec<i32>, off: usize, g: Vec<i32>, p: Vec<i32>, ret: Option<i32> },
    Set { id: u8, value: i32 },
    Node { state: u16, kind: u8, arg: u8 },
}

#[derive(Debug)]
enum Root {
    Scene { scene: u16, state: u16 },
    Unit { id: ScriptId, g: [i32; 4], p: [i32; 4] },
}

struct Replay {
    exp: Vec<Exp>,
    i: usize,
    err: Option<String>,
}

fn host_level(id: u8) -> bool {
    id >= 8 && !(natives::RUN_SCRIPT..=natives::RUN_SCRIPT_P0_P1).contains(&id)
        && !(natives::RUN_EVENT..=natives::RUN_EVENT_P0_P3).contains(&id)
}

impl Replay {
    fn next(&mut self) -> Option<Exp> {
        let e = self.exp.get(self.i).cloned();
        self.i += 1;
        e
    }
    fn fail(&mut self, m: String) -> Result<NativeResult, String> {
        if self.err.is_none() {
            self.err = Some(m.clone());
        }
        Err(m)
    }
}

impl Host for Replay {
    fn native(&mut self, c: Call<'_>) -> Result<NativeResult, String> {
        match self.next() {
            Some(Exp::Native { id, args, off, g, p, ret }) => {
                if id != c.native || args != c.args || off != c.pc || g != c.globals.g || p != c.params[..] {
                    return self.fail(format!(
                        "call #{}: game native 0x{id:02x}{args:?} @+{off:#x} G{g:?} P{p:?}; vm 0x{:02x}{:?} @+{:#x} G{:?} P{:?}",
                        self.i - 1, c.native, c.args, c.pc, c.globals.g, c.params
                    ));
                }
                Ok(NativeResult::Done(ret))
            }
            other => self.fail(format!("call #{}: vm native 0x{:02x}{:?}, game {other:?}", self.i - 1, c.native, c.args)),
        }
    }
    fn set_var(&mut self, var: u8, value: i32, _p: &mut [i32; 4], _g: &mut Globals) -> Result<(), String> {
        match self.next() {
            Some(Exp::Set { id, value: v }) if id == var && v == value => Ok(()),
            other => self.fail(format!("call #{}: vm set {var}={value}, game {other:?}", self.i - 1)).map(|_| ()),
        }
    }
    fn node(&mut self, call: NodeCall) -> Result<NativeResult, String> {
        let (n, k, a) = match &call {
            NodeCall::Text { node, kind, arg, .. } => (*node, *kind, *arg),
            NodeCall::Choice { node, arg, .. } => (*node, 3, *arg),
        };
        match self.next() {
            Some(Exp::Node { state, kind, arg }) if state == n && kind == k && arg == a => Ok(NativeResult::Done(None)),
            other => self.fail(format!("call #{}: vm node {n} kind {k} arg {a}, game {other:?}", self.i - 1)),
        }
    }
}

fn hexs(v: &Value) -> u32 {
    v.as_str().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)).unwrap_or(0)
}
fn ints(v: &Value) -> Vec<i32> {
    v.as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as i32).collect()).unwrap_or_default()
}
fn arr4(v: &Value) -> [i32; 4] {
    let a = ints(v);
    [a.first().copied().unwrap_or(0), a.get(1).copied().unwrap_or(0), a.get(2).copied().unwrap_or(0), a.get(3).copied().unwrap_or(0)]
}

#[derive(Clone, PartialEq)]
enum Ctx {
    Scene { obj: u32, ours: bool },
    Interp { addr: u32, ours: bool, last: u8 },
}

/// Split a trace into roots with their expected host-call sequences.
fn roots(set: &ScriptSet, events: &[Value]) -> Vec<(Root, Vec<Exp>)> {
    let mut out: Vec<(Root, Vec<Exp>)> = Vec::new();
    let mut stack: Vec<Ctx> = Vec::new();
    let mut last_run: Option<i32> = None;
    let mut last_step: Option<(u16, u16)> = None;
    for e in events {
        let kind = e["ev"].as_str().unwrap_or("");
        let ours_top = |stack: &Vec<Ctx>| match stack.last() {
            Some(Ctx::Scene { ours, .. }) => *ours,
            Some(Ctx::Interp { ours, .. }) => *ours,
            None => false,
        };
        match kind {
            "scene" => {
                let obj = 0; // the scene object appears on its steps
                let ours = match stack.last() {
                    None => {
                        out.push((Root::Scene { scene: e["scene"].as_u64().unwrap_or(0) as u16, state: 0 }, vec![]));
                        true
                    }
                    Some(Ctx::Interp { ours, last, .. }) => *ours && (natives::RUN_EVENT..=natives::RUN_EVENT_P0_P3).contains(last),
                    Some(Ctx::Scene { .. }) => false,
                };
                stack.push(Ctx::Scene { obj, ours });
            }
            "step" => {
                let obj = hexs(&e["scene_obj"]);
                let scene = e["scene"].as_u64().unwrap_or(0) as u16;
                let state = e["state"].as_u64().unwrap_or(0) as u16;
                // bind the scene object on its first step; a step with no open scene starts a partial root
                match stack.last_mut() {
                    Some(Ctx::Scene { obj: o, .. }) if *o == 0 || *o == obj => *o = obj,
                    None => {
                        out.push((Root::Scene { scene, state }, vec![]));
                        stack.push(Ctx::Scene { obj, ours: true });
                    }
                    _ => {}
                }
                let ours = ours_top(&stack) && matches!(stack.last(), Some(Ctx::Scene { obj: o, .. }) if *o == obj);
                last_step = Some((scene, state));
                if e.get("exit").is_some() {
                    if matches!(stack.last(), Some(Ctx::Scene { obj: o, .. }) if *o == obj) {
                        stack.pop();
                    }
                    continue;
                }
                let k = e["kind"].as_u64().unwrap_or(0) as u8;
                if ours && matches!(k, 1 | 2 | 3 | 5 | 6) {
                    let arg = e["arg"].as_u64().unwrap_or(0) as u8;
                    out.last_mut().unwrap().1.push(Exp::Node { state, kind: k, arg });
                }
            }
            "run" => last_run = e["script"].as_i64().map(|x| x as i32),
            "enter" => {
                let addr = hexs(&e["interp"]);
                let ours = match stack.last() {
                    None => {
                        let code = hexs(&e["code"]);
                        let lr = hexs(&e["lr"]);
                        let id = if lr == 0x0801_1f91 {
                            last_step.map(|(s, n)| ScriptId::Node { event: s, node: n })
                        } else if let (Some(r), true) = (last_run, lr == 0x0800_40e1 || lr == 0x0800_40f3) {
                            set.run_script_target(r)
                        } else if set.get(ScriptId::Inline(code)).is_some() {
                            Some(ScriptId::Inline(code))
                        } else {
                            set.scripts.values().find(|s| s.code_addr == Some(code)).map(|s| s.id)
                        };
                        match id {
                            Some(id) => {
                                out.push((Root::Unit { id, g: arr4(&e["G"]), p: arr4(&e["P"]) }, vec![]));
                                true
                            }
                            None => false,
                        }
                    }
                    Some(Ctx::Scene { ours, .. }) => *ours,
                    Some(Ctx::Interp { ours, last, .. }) => {
                        *ours && (natives::RUN_SCRIPT..=natives::RUN_SCRIPT_P0_P1).contains(last)
                    }
                };
                stack.push(Ctx::Interp { addr, ours, last: 0 });
            }
            "exit" => {
                if matches!(stack.last(), Some(Ctx::Interp { .. })) {
                    stack.pop();
                }
            }
            "native" | "set" => {
                let addr = hexs(&e["interp"]);
                let Some(Ctx::Interp { addr: a, ours, last }) = stack.last_mut() else { continue };
                if *a != addr {
                    continue;
                }
                let id = e["id"].as_u64().unwrap_or(0) as u8;
                if kind == "native" {
                    *last = id;
                }
                if !*ours {
                    continue;
                }
                let before = ints(&e["before"]);
                let after = ints(&e["after"]);
                let list = &mut out.last_mut().unwrap().1;
                if kind == "set" {
                    if id >= 8 {
                        list.push(Exp::Set { id, value: *before.last().unwrap_or(&0) });
                    }
                    continue;
                }
                if !host_level(id) {
                    continue;
                }
                let sig = natives::sig(id);
                let k = before.len().saturating_sub(sig.pops as usize);
                let args = before[k..].to_vec();
                let ret = if sig.pushes && after.len() == k + 1 { after.last().copied() } else { None };
                list.push(Exp::Native {
                    id,
                    args,
                    off: e["off"].as_u64().unwrap_or(0) as usize,
                    g: ints(&e["G"]),
                    p: ints(&e["P"]),
                    ret,
                });
            }
            _ => {}
        }
    }
    out
}

#[test]
fn interpreter_matches_the_game() {
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let dir: PathBuf = wf_rom::testutil::repo_root().join("work/buu/script/traces");
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let set = ScriptSet::load_all(&rom);
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map(|x| x == "json").unwrap_or(false)).collect();
    files.sort();
    let (mut nroots, mut ncalls, mut bad) = (0, 0, Vec::new());
    for f in files {
        let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        let evs = v["events"].as_array().cloned().unwrap_or_default();
        let mut file_calls = 0;
        for (root, exp) in roots(&set, &evs) {
            let n = exp.len();
            let mut h = Replay { exp, i: 0, err: None };
            let mut g = Globals::default();
            let mut t = match &root {
                Root::Scene { scene, state } => {
                    let Some(ev) = set.events.get(scene) else { continue };
                    let mut t = Thread::new_event(&set, *scene, [0; 4]).unwrap();
                    t.acts = vec![Act::Event(EventAct { header: ev.header, event: Some(*scene), node: *state, params: 0, in_node: false, entered: true })];
                    let _ = Entry::Terminal(0);
                    t
                }
                Root::Unit { id, g: g0, p } => {
                    g.g = *g0;
                    Thread::new(*id, *p)
                }
            };
            let mut st = t.run(&set, &mut g, &mut h);
            while let Ok(wf_script::Status::Blocked) = st {
                st = t.resume(None, &set, &mut g, &mut h);
            }
            // a trace may end while the root is still running (capture window): only the recorded prefix counts
            let ok = h.err.is_none() && (st.is_ok() || h.i > n);
            if !ok {
                bad.push(format!("{name} {root:?}: {} (matched {} of {n})", h.err.clone().unwrap_or(format!("{st:?}")), h.i.saturating_sub(1)));
            }
            nroots += 1;
            ncalls += n.min(h.i);
            file_calls += n;
        }
        eprintln!("{name}: {file_calls} host calls");
    }
    eprintln!("emulator traces: {nroots} roots, {ncalls} host calls matched, {} mismatches", bad.len());
    // negative control: a perturbed game result must be detected
    {
        let v: Value =
            serde_json::from_slice(&std::fs::read(dir.join("boss_janemba_handover.json")).unwrap()).unwrap();
        let evs = v["events"].as_array().cloned().unwrap();
        let rs = roots(&set, &evs);
        let (root, mut exp) = rs.into_iter().find(|(r, _)| matches!(r, Root::Scene { scene: 450, .. })).unwrap();
        // flip get_character()'s answer: the cutscene takes the other branch (different sprite swap)
        for e in exp.iter_mut() {
            if let Exp::Native { id: 0x15, ret, .. } = e {
                *ret = Some(1 - ret.unwrap_or(0).min(1));
                break;
            }
        }
        let Root::Scene { scene, state } = root else { unreachable!() };
        let mut h = Replay { exp, i: 0, err: None };
        let ev = &set.events[&scene];
        let mut t = Thread::new_event(&set, scene, [0; 4]).unwrap();
        t.acts = vec![Act::Event(EventAct { header: ev.header, event: Some(scene), node: state, params: 0, in_node: false, entered: true })];
        let mut g = Globals::default();
        let mut st = t.run(&set, &mut g, &mut h);
        while let Ok(wf_script::Status::Blocked) = st {
            st = t.resume(None, &set, &mut g, &mut h);
        }
        assert!(h.err.is_some(), "perturbation not detected");
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
