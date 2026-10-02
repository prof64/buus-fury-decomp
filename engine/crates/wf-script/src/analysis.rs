//! Whole-set decompilation: every unit → run-to-completion [`Program`], with the fixpoint over nested
//! `run_script`: a caller must split after calling a script that itself splits, and its queue is busy
//! afterwards if the callee may queue actions.

use crate::decompile::{decompile, Decompiled};
use crate::ir::*;
use crate::rom::{ScriptId, ScriptSet};
use crate::steps::{self, is_instant, is_result_action, Program};
use std::collections::{BTreeMap, BTreeSet};

/// Does the program queue anything (non-instant action, run_event, or a queuing callee)?
fn queues(p: &Program, set: &ScriptSet, q: &BTreeSet<ScriptId>) -> bool {
    let mut any = false;
    for st in &p.steps {
        Stmt::walk(&st.body, &mut |s| match s {
            Stmt::Call { native, .. } if !is_instant(*native) => any = true,
            Stmt::Await { native, .. } if !is_instant(*native) || is_result_action(*native) => any = true,
            Stmt::RunScript { script, .. } => {
                let callee = match script {
                    Expr::Int(i) => set.run_script_target(*i),
                    _ => None,
                };
                if callee.map(|c| q.contains(&c)).unwrap_or(true) {
                    any = true;
                }
            }
            _ => {}
        });
    }
    any
}

/// Decompile every unit of the set and build its run-to-completion program.
#[allow(clippy::type_complexity)]
pub fn decompile_set(
    set: &ScriptSet,
) -> (BTreeMap<ScriptId, Program>, BTreeMap<ScriptId, Decompiled>, Vec<(ScriptId, String)>) {
    let mut decs: BTreeMap<ScriptId, Decompiled> = BTreeMap::new();
    let mut errors = Vec::new();
    for (id, sc) in &set.scripts {
        match decompile(sc, &id.slug()) {
            Ok(d) => {
                decs.insert(*id, d);
            }
            Err(e) => errors.push((*id, e.0)),
        }
    }
    // monotone fixpoint from "nothing splits, nothing queues"
    let mut splits: BTreeSet<ScriptId> = BTreeSet::new();
    let mut queuing: BTreeSet<ScriptId> = BTreeSet::new();
    loop {
        let f = |e: &Expr| match e {
            Expr::Int(i) => match set.run_script_target(*i) {
                Some(t) => (splits.contains(&t), queuing.contains(&t)),
                None => (true, true),
            },
            _ => (true, true),
        };
        let progs: BTreeMap<ScriptId, Program> =
            decs.iter().map(|(id, d)| (*id, steps::build(&d.func, &d.blocks, &f))).collect();
        let s2: BTreeSet<ScriptId> = progs.iter().filter(|(_, p)| p.steps.len() > 1).map(|(k, _)| *k).collect();
        let q2: BTreeSet<ScriptId> =
            progs.iter().filter(|(_, p)| queues(p, set, &queuing)).map(|(k, _)| *k).collect();
        if s2 == splits && q2 == queuing {
            return (progs, decs, errors);
        }
        splits = s2;
        queuing = q2;
    }
}
