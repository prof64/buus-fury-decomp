//! Agent-written controllers in Rhai (the game's mod language): a deterministic sandbox. The script body runs once
//! per decision frame with `obs` (the observation, integers only), `mem` (a map that persists between frames) and
//! `frame`; its value is the buttons to hold ("RIGHT+A", an int mask, or "" / () for none).
//!
//! Sandbox: no I/O (print/debug go to the controller log), no modules, no clock, no floats, no `eval`;
//! instruction metering (`max_ops` operations per decision, deterministic, unlike a wall-clock timeout) plus size
//! limits on strings / arrays / maps and call depth. A controller is a policy like any macro: what goes into the
//! input log are the buttons it returned, so replays never need the script.

use crate::env::Agent;
use crate::macros::{mask_names, parse_button_str, Policy, Step};
use crate::obs::{observe_ro, ObsOpts};
use rhai::{Array, Dynamic, Engine, Map, Scope, AST, INT};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Controller {
    engine: Engine,
    ast: AST,
    scope: Scope<'static>,
    pub logs: Rc<RefCell<Vec<String>>>,
    stop: Rc<RefCell<Option<String>>>,
    ops_now: Rc<Cell<u64>>,
    pub ops_total: u64,
    pub ops_max: u64,
    pub calls: u64,
    pub every: u32,
    pub forecast: u32,
    last: u16,
    n: u32,
    pub error: Option<String>,
    /// the built-in attack macro on the nearest enemy, run alongside as a hint (`obs.suggest`)
    helper: Option<(u32, Box<dyn Policy>)>,
}

pub fn to_dynamic(v: &Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from_bool(*b),
        Value::Number(n) => {
            let x = n.as_i64().unwrap_or_else(|| n.as_f64().map(|f| f.round() as i64).unwrap_or(0));
            Dynamic::from_int(x.clamp(INT::MIN as i64, INT::MAX as i64) as INT)
        }
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Array(a) => Dynamic::from_array(a.iter().map(to_dynamic).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, x) in o {
                m.insert(k.as_str().into(), to_dynamic(x));
            }
            Dynamic::from_map(m)
        }
    }
}

fn pair(a: &Array) -> (INT, INT) {
    let g = |i: usize| a.get(i).and_then(|x| x.as_int().ok()).unwrap_or(0);
    (g(0), g(1))
}

fn toward(from: Array, to: Array) -> String {
    let (fx, fy) = pair(&from);
    let (tx, ty) = pair(&to);
    let mut v = Vec::new();
    if ty > fy + 1 {
        v.push("DOWN");
    } else if ty < fy - 1 {
        v.push("UP");
    }
    if tx > fx + 1 {
        v.push("RIGHT");
    } else if tx < fx - 1 {
        v.push("LEFT");
    }
    v.join("+")
}

impl Controller {
    pub fn new(code: &str, max_ops: u64, every: u32) -> Result<Controller, String> {
        let mut engine = Engine::new();
        let logs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let stop: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let ops_now = Rc::new(Cell::new(0u64));
        engine.set_max_operations(max_ops.max(1000));
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(4096);
        engine.set_max_array_size(4096);
        engine.set_max_map_size(4096);
        engine.disable_symbol("eval");
        let l = logs.clone();
        engine.on_print(move |s| {
            let mut v = l.borrow_mut();
            if v.len() < 2000 {
                v.push(s.to_string());
            }
        });
        let l = logs.clone();
        engine.on_debug(move |s, _, _| {
            let mut v = l.borrow_mut();
            if v.len() < 2000 {
                v.push(s.to_string());
            }
        });
        let o = ops_now.clone();
        engine.on_progress(move |n| {
            o.set(n);
            None
        });
        let l = logs.clone();
        engine.register_fn("log", move |x: Dynamic| {
            let mut v = l.borrow_mut();
            if v.len() < 2000 {
                v.push(x.to_string());
            }
        });
        let s = stop.clone();
        engine.register_fn("stop", move || {
            *s.borrow_mut() = Some("controller_stop".into());
        });
        let s = stop.clone();
        engine.register_fn("stop", move |why: &str| {
            *s.borrow_mut() = Some(format!("controller_stop: {why}"));
        });
        engine.register_fn("toward", toward);
        engine.register_fn("away", |from: Array, to: Array| {
            let (fx, fy) = pair(&from);
            let (tx, ty) = pair(&to);
            toward(from, vec![Dynamic::from_int(2 * fx - tx), Dynamic::from_int(2 * fy - ty)])
        });
        engine.register_fn("dist", |a: Array, b: Array| -> INT {
            let (ax, ay) = pair(&a);
            let (bx, by) = pair(&b);
            crate::macros::isqrt(((ax - bx) as i64).pow(2) + ((ay - by) as i64).pow(2)) as INT
        });
        engine.register_fn("btn", |s: &str| -> INT { parse_button_str(s).unwrap_or(0) as INT });
        engine.register_fn("names", |m: INT| -> String { mask_names(m as u16) });
        let ast = engine.compile(code).map_err(|e| format!("compile error: {e}"))?;
        let mut scope = Scope::new();
        scope.push("mem", Map::new());
        scope.push("obs", Dynamic::UNIT);
        scope.push("frame", 0 as INT);
        Ok(Controller { engine, ast, scope, logs, stop, ops_now, ops_total: 0, ops_max: 0, calls: 0, every: every.max(1), forecast: 20, last: 0, n: 0, error: None, helper: None })
    }

    pub fn decide(&mut self, obs: Value, frame: u32) -> Result<u16, String> {
        self.scope.set_value("obs", to_dynamic(&obs));
        self.scope.set_value("frame", frame as INT);
        self.ops_now.set(0);
        let r = self.engine.eval_ast_with_scope::<Dynamic>(&mut self.scope, &self.ast);
        let ops = self.ops_now.get();
        self.ops_total += ops;
        self.ops_max = self.ops_max.max(ops);
        self.calls += 1;
        let r = r.map_err(|e| format!("runtime error at frame {frame}: {e}"))?;
        if let Some(s) = r.clone().try_cast::<String>() {
            if s.eq_ignore_ascii_case("stop") {
                *self.stop.borrow_mut() = Some("controller_stop".into());
                return Ok(0);
            }
            return parse_button_str(&s);
        }
        if let Ok(i) = r.as_int() {
            return Ok((i & 0x3ff) as u16);
        }
        if r.is_unit() {
            return Ok(0);
        }
        if let Some(a) = r.clone().try_cast::<Array>() {
            let mut m = 0;
            for x in a {
                if let Some(s) = x.try_cast::<String>() {
                    m |= parse_button_str(&s)?;
                }
            }
            return Ok(m);
        }
        Err(format!("the controller returned {} (expected a button string like \"RIGHT+A\" or an int mask)", r.type_name()))
    }

    pub fn report(&self) -> Value {
        let logs = self.logs.borrow();
        let n = logs.len();
        json!({
            "decisions": self.calls, "ops_total": self.ops_total, "ops_max_per_decision": self.ops_max,
            "logs": logs[n.saturating_sub(40)..].to_vec(), "logs_total": n,
        })
    }
}

impl Policy for Controller {
    fn next(&mut self, a: &Agent, slot: usize) -> Step {
        if let Some(r) = self.stop.borrow().clone() {
            return Step::Done(r);
        }
        if self.error.is_some() {
            return Step::Done("controller_error".into());
        }
        if self.n.is_multiple_of(self.every) {
            let mut obs = observe_ro(a, slot, ObsOpts { full: false, map: false, forecast: self.forecast, drain: false });
            // hint: what the attack macro would press now (path finding on the walk grid, punch timing)
            if let Some(mut e) = crate::macros::find_enemy(a, slot, None) {
                // hysteresis: keep the current target unless another one is clearly nearer
                if let Some(h) = &self.helper {
                    let d = |id: u32| -> i64 {
                        let (Some(p), Some((_, en))) = (a.player(slot), a.w().enemy_by_id(id)) else { return i64::MAX };
                        let (px, py) = p.a.foot_center();
                        let (x, y) = en.a.foot_center();
                        crate::macros::isqrt(((x - px) as i64).pow(2) + ((y - py) as i64).pow(2))
                    };
                    let alive = a.w().enemy_by_id(h.0).is_some_and(|(i, en)| !en.defeated && !a.w().ents[i].removed);
                    if alive && d(e) + 32 > d(h.0) {
                        e = h.0;
                    }
                }
                if self.helper.as_ref().is_none_or(|h| h.0 != e) {
                    self.helper = Some((e, crate::macros::attack(e, None)));
                }
                if let Step::Press(m) = self.helper.as_mut().unwrap().1.next(a, slot) {
                    obs["suggest"] = json!({"attack": mask_names(m), "target": e});
                }
            }
            if !a.last_events.is_empty() {
                obs["events"] = Value::Array(a.last_events.clone());
            }
            match self.decide(obs, a.frame()) {
                Ok(m) => self.last = m,
                Err(e) => {
                    self.error = Some(e);
                    return Step::Done("controller_error".into());
                }
            }
            if let Some(r) = self.stop.borrow().clone() {
                return Step::Done(r);
            }
        }
        self.n += 1;
        Step::Press(self.last)
    }
}
