//! Differential verification (b): the exact bytecode interpreter vs the decompiled Rhai, for EVERY script
//! of the ROM (all 522 events as whole graphs, every event node 0 via run_script semantics, every inline
//! script), several host seeds each. Both runtimes talk to the same deterministic recording host; the
//! effect traces (natives with args/results, game-variable writes, text/choice nodes) and the results
//! must be identical. Also: rollback (snapshot mid-script, restore, re-run → identical trace).

use std::collections::BTreeMap;
use wf_script::rom::{Entry, NodeBody, ScriptId};
use wf_script::testhost::{Effect, TestHost};
use wf_script::vm::{Globals, Status, Thread};
use wf_script::ScriptSet;
use wf_script_rhai::{Ret, Runtime, Status as RStatus, UnitRef};

#[derive(Debug, PartialEq)]
struct Outcome {
    trace: Vec<Effect>,
    result: Option<i32>,
    event_result: Option<u16>,
    g: [i32; 4],
    error: bool,
}

fn run_vm(set: &ScriptSet, mut t: Thread, seed: u64, g0: [i32; 4]) -> Outcome {
    let mut h = TestHost::new(seed);
    let mut g = Globals { g: g0 };
    t.max_steps = 200_000;
    let mut st = t.run(set, &mut g, &mut h);
    let mut error = false;
    loop {
        match st {
            Ok(Status::Finished) => break,
            Ok(Status::Blocked) => {
                let v = h.take_pending();
                st = t.resume(v, set, &mut g, &mut h);
            }
            Err(e) => { if std::env::var("DIFF_VERBOSE").is_ok() { eprintln!("vm error: {e:?}"); }
                error = true;
                break;
            }
        }
    }
    // the runner's callers read stack[0]; event nodes use a single value
    let result = t.result.as_ref().and_then(|s| s.first().copied());
    Outcome { trace: h.trace(), result, event_result: t.event_result, g: g.g, error }
}

fn run_rhai(rt: &mut Runtime, h: &TestHost, seed: u64, g0: [i32; 4]) -> Outcome {
    h.reset(seed);
    rt.set_globals(Globals { g: g0 });
    let mut st = rt.run();
    let mut error = false;
    let mut n = 0;
    loop {
        n += 1;
        match st {
            Ok(RStatus::Finished) => break,
            Ok(RStatus::Blocked) if n < 100_000 => {
                let v = h.take_pending();
                st = rt.resume(v);
            }
            _ => {
                if let Err(e) = &st {
                    if std::env::var("DIFF_VERBOSE").is_ok() {
                        eprintln!("rhai error: {e}");
                    }
                }
                error = true;
                break;
            }
        }
    }
    let s = rt.state();
    let result = match s.result {
        Some(Ret::Int(v)) => Some(v),
        Some(Ret::Many(v)) => v.first().copied(),
        _ => None,
    };
    Outcome { trace: h.trace(), result, event_result: s.event_result, g: rt.globals().g, error }
}

fn setup() -> Option<(ScriptSet, BTreeMap<String, String>)> {
    let rom = wf_rom::testutil::rom()?;
    let set = ScriptSet::load_all(&rom);
    let (files, errors) = wf_script::export::export(&rom, &set);
    assert!(errors.is_empty(), "{errors:?}");
    Some((set, files))
}

fn node0_unit(set: &ScriptSet, ev: u16) -> Option<(ScriptId, UnitRef)> {
    let id = set.run_script_target(ev as i32)?;
    let e = &set.events[&ev];
    let Some(Entry::Node(n)) = e.entries.get(&0) else { return None };
    let NodeBody::Code(_) = n.body else { return None };
    Some((id, UnitRef::new(format!("s{ev:03}"), "n0_", false)))
}

#[test]
fn bytecode_and_rhai_agree_on_every_script() {
    let Some((set, files)) = setup() else { return };
    let h = TestHost::new(0);
    let mut rt = Runtime::new(&files, Box::new(h.clone())).expect("all decompiled scripts compile");
    let seeds: [u64; 4] = [1, 2, 0xdead_beef, 77];
    let (mut cases, mut runs, mut both_err, mut effects) = (0, 0, 0, 0usize);
    let mut fails: Vec<String> = Vec::new();
    let mut check = |name: String, vm: Outcome, rh: Outcome| {
        runs += 1;
        effects += vm.trace.len();
        if vm.error && rh.error {
            both_err += 1;
            return;
        }
        if vm != rh {
            if fails.len() < 10 {
                let k = vm.trace.iter().zip(rh.trace.iter()).take_while(|(a, b)| a == b).count();
                fails.push(format!(
                    "{name}: vm(err {}, res {:?}/{:?}, {} effects) rhai(err {}, res {:?}/{:?}, {} effects); first diff at {k}: {:?} vs {:?}",
                    vm.error, vm.result, vm.event_result, vm.trace.len(), rh.error, rh.result, rh.event_result,
                    rh.trace.len(), vm.trace.get(k), rh.trace.get(k)
                ));
            } else {
                fails.push(name);
            }
        }
    };
    let g0 = [3, 1, 0, 2];
    // whole events
    for &ev in set.events.keys() {
        cases += 1;
        for &s in &seeds {
            let p = [(s % 5) as i32, 1, 2, -1];
            let vm = run_vm(&set, Thread::new_event(&set, ev, p).unwrap(), s, g0);
            rt.start_event(ev, p);
            let rh = run_rhai(&mut rt, &h, s, g0);
            check(format!("event {ev} seed {s}"), vm, rh);
        }
    }
    // run_script semantics on node 0 (the steps flag is resolved from the graph)
    for &ev in set.events.keys() {
        let Some((id, mut unit)) = node0_unit(&set, ev) else { continue };
        if let Some(wf_script_rhai::GNode::Code { steps, .. }) = rt.library().graphs.get(&ev).and_then(|g| g.get(&0)) {
            unit.steps = *steps;
        }
        cases += 1;
        for &s in &seeds {
            let p = [(s % 7) as i32, -1, 0, 1];
            let vm = run_vm(&set, Thread::new(id, p), s, g0);
            rt.start_unit(unit.clone(), p);
            let rh = run_rhai(&mut rt, &h, s, g0);
            check(format!("run_script({ev}) seed {s}"), vm, rh);
        }
    }
    // inline scripts
    for id in set.scripts.keys() {
        let ScriptId::Inline(a) = id else { continue };
        let file = format!("inline_0x{a:08X}");
        let src = &files[&format!("scripts/inline/{file}.rhai")];
        let steps = src.contains("fn step_0()");
        cases += 1;
        for &s in &seeds {
            let p = [0, 0, 0, 0];
            let vm = run_vm(&set, Thread::new(*id, p), s, g0);
            rt.start_unit(UnitRef::new(file.clone(), "", steps), p);
            let rh = run_rhai(&mut rt, &h, s, g0);
            check(format!("{file} seed {s}"), vm, rh);
        }
    }
    eprintln!("differential: {cases} cases, {runs} runs, {both_err} runs errored identically on both sides, {effects} effects compared, {} mismatches", fails.len());
    assert!(fails.is_empty(), "{} mismatches:\n{}", fails.len(), fails.join("\n"));
}

/// Rollback: snapshot a running script at every blocking point (thread state + globals + host state),
/// finish, restore the snapshot, finish again: the effect traces must be identical. The thread state also
/// round-trips through JSON (it is plain data).
#[test]
fn snapshot_restore_mid_script_is_identical() {
    let Some((set, files)) = setup() else { return };
    let h = TestHost::new(0);
    let mut rt = Runtime::new(&files, Box::new(h.clone())).unwrap();
    let mut checked = 0;
    // a boss handover cutscene, an NPC talk with dialog, the loot event, a multi-step cutscene
    for ev in [450u16, 451, 497, 4, 37, 121, 474, 516] {
        for seed in [5u64] {
            h.reset(seed);
            rt.set_globals(Globals::default());
            rt.start_event(ev, [1, 0, 0, 0]);
            let mut st = rt.run();
            let mut k = 0;
            while let Ok(RStatus::Blocked) = st {
                k += 1;
                // snapshot
                let snap_t = rt.state();
                let json = serde_json::to_string(&snap_t).unwrap();
                let snap_t: wf_script_rhai::ThreadState = serde_json::from_str(&json).unwrap();
                let snap_h = h.0.borrow().clone();
                let snap_g = rt.globals();
                // finish from here
                let mut st2 = rt.resume(h.take_pending());
                while let Ok(RStatus::Blocked) = st2 {
                    st2 = rt.resume(h.take_pending());
                }
                let a = (h.trace(), rt.state().event_result, st2.is_ok());
                // restore and finish again
                rt.restore(snap_t);
                *h.0.borrow_mut() = snap_h.clone();
                rt.set_globals(snap_g.clone());
                let mut st3 = rt.resume(h.take_pending());
                while let Ok(RStatus::Blocked) = st3 {
                    st3 = rt.resume(h.take_pending());
                }
                let b = (h.trace(), rt.state().event_result, st3.is_ok());
                assert_eq!(a, b, "event {ev} seed {seed} snapshot {k}");
                checked += 1;
                // continue the original run one step from the restored point
                rt.restore(serde_json::from_str(&json).unwrap());
                *h.0.borrow_mut() = snap_h;
                rt.set_globals(snap_g);
                st = rt.resume(h.take_pending());
                if k > 8 {
                    break;
                }
            }
            // the bytecode VM thread is plain data too
            let mut t = Thread::new_event(&set, ev, [1, 0, 0, 0]).unwrap();
            let mut vh = TestHost::new(seed);
            let mut g = Globals::default();
            let mut s = t.run(&set, &mut g, &mut vh);
            if let Ok(Status::Blocked) = s {
                let snap = (t.clone(), vh.0.borrow().clone(), g.clone());
                let v = vh.take_pending();
                s = t.resume(v, &set, &mut g, &mut vh);
                while let Ok(Status::Blocked) = s {
                    let v = vh.take_pending();
                    s = t.resume(v, &set, &mut g, &mut vh);
                }
                let a = vh.trace();
                let (mut t2, hs, mut g2) = snap;
                *vh.0.borrow_mut() = hs;
                let v = vh.take_pending();
                let mut s2 = t2.resume(v, &set, &mut g2, &mut vh);
                while let Ok(Status::Blocked) = s2 {
                    let v = vh.take_pending();
                    s2 = t2.resume(v, &set, &mut g2, &mut vh);
                }
                assert_eq!(a, vh.trace(), "vm event {ev}");
                assert_eq!(t, t2);
            }
        }
    }
    eprintln!("rollback: {checked} snapshot/restore points checked");
    assert!(checked > 20);
}
