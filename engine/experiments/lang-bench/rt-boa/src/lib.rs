//! Boa (pure-Rust JavaScript) embedding of the host API.

use boa_engine::object::builtins::JsArray;
use boa_engine::{js_string, Context, JsObject, JsResult, JsString, JsValue, NativeFunction, Source};
use host::{api, Action};

pub const PRELUDE: &str = include_str!("../../scripts/prelude.js");
pub const STORY: &str = include_str!("../../scripts/story.js");

fn int(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<i64> {
    let v = args.get(i).cloned().unwrap_or_default();
    Ok(v.to_number(ctx)? as i64)
}
fn ints(v: &JsValue, ctx: &mut Context) -> JsResult<Vec<i64>> {
    let Some(o) = v.as_object() else { return Ok(vec![]) };
    let arr = JsArray::from_object(o)?;
    let n = arr.length(ctx)?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        out.push(arr.get(i, ctx)?.to_number(ctx)? as i64);
    }
    Ok(out)
}
fn prop(o: &JsObject, k: &str, ctx: &mut Context) -> JsResult<JsValue> {
    o.get(JsString::from(k), ctx)
}

/// Walk a plain JS action object into the engine's `Action`.
fn to_action(v: &JsValue, ctx: &mut Context) -> JsResult<Action> {
    let o = v.as_object().ok_or_else(|| boa_engine::JsNativeError::typ().with_message("action"))?;
    let k = prop(&o, "k", ctx)?.to_string(ctx)?.to_std_string_escaped();
    let a = ints(&prop(&o, "a", ctx)?, ctx)?;
    let at = |i: usize| a.get(i).copied().unwrap_or(0);
    Ok(match k.as_str() {
        "say" => Action::Say { speaker: at(0), text: at(1) },
        "wait" => Action::Wait(at(0)),
        "walk_to" => Action::WalkTo { actor: at(0), x: at(1), y: at(2) },
        "face" => Action::Face { actor: at(0), dir: at(1) },
        "transform" => Action::Transform { actor: at(0), form: at(1) },
        "play_sfx" => Action::PlaySfx(at(0)),
        "start_battle" => Action::StartBattle(at(0)),
        "set_flag" => Action::SetFlag(at(0)),
        "op" => Action::Op { id: prop(&o, "id", ctx)?.to_number(ctx)? as i64, args: a.clone() },
        "call" => Action::Call { name: prop(&o, "name", ctx)?.to_string(ctx)?.to_std_string_escaped(), arg: at(0) },
        "sequence" | "parallel" => {
            let l = prop(&o, "list", ctx)?;
            let arr = JsArray::from_object(l.as_object().unwrap())?;
            let n = arr.length(ctx)?;
            let mut v = Vec::new();
            for i in 0..n {
                v.push(to_action(&arr.get(i, ctx)?, ctx)?);
            }
            if k == "sequence" {
                Action::Sequence(v)
            } else {
                Action::Parallel(v)
            }
        }
        _ => return Err(boa_engine::JsNativeError::typ().with_message("unknown action").into()),
    })
}

macro_rules! reg {
    ($ctx:expr, $name:literal, $n:expr, $f:expr) => {
        $ctx.register_global_callable(js_string!($name), $n, NativeFunction::from_fn_ptr($f)).unwrap();
    };
}

fn b(v: bool) -> JsResult<JsValue> {
    Ok(JsValue::new(v))
}
fn i(v: i64) -> JsResult<JsValue> {
    Ok(JsValue::new(v as i32))
}
fn u() -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

struct FixedClock;
impl boa_engine::context::Clock for FixedClock {
    fn now(&self) -> boa_engine::context::time::JsInstant {
        boa_engine::context::time::JsInstant::new(0, 0)
    }
    fn system_time_millis(&self) -> i64 {
        0
    }
}

pub fn context() -> Context {
    // No wall clock: a fixed clock keeps Date/timers deterministic and works on wasm32-unknown-unknown
    // (the default clock calls std::time::Instant::now, which panics there).
    let mut ctx = boa_engine::context::ContextBuilder::default().clock(std::rc::Rc::new(FixedClock)).build().unwrap();
    reg!(ctx, "run", 1, |_, a, c| {
        let act = to_action(&a.first().cloned().unwrap_or_default(), c)?;
        api::run(act);
        u()
    });
    reg!(ctx, "get_flag", 1, |_, a, c| b(api::get_flag(int(a, 0, c)?)));
    reg!(ctx, "set_flag", 1, |_, a, c| {
        api::set_flag(int(a, 0, c)?);
        u()
    });
    reg!(ctx, "rand", 1, |_, a, c| i(api::rand(int(a, 0, c)?)));
    reg!(ctx, "chance", 1, |_, a, c| b(api::chance(int(a, 0, c)?)));
    reg!(ctx, "player", 1, |_, a, c| i(api::player(int(a, 0, c)?)));
    reg!(ctx, "actor_x", 1, |_, a, c| i(api::actor_x(int(a, 0, c)?)));
    reg!(ctx, "actor_y", 1, |_, a, c| i(api::actor_y(int(a, 0, c)?)));
    reg!(ctx, "set_state", 2, |_, a, c| {
        api::set_state(int(a, 0, c)?, int(a, 1, c)?);
        u()
    });
    reg!(ctx, "give_item", 2, |_, a, c| {
        api::give_item(int(a, 0, c)?, int(a, 1, c)?);
        u()
    });
    reg!(ctx, "is_equipped", 1, |_, a, c| b(api::is_equipped(int(a, 0, c)?)));
    reg!(ctx, "stat", 1, |_, a, c| i(api::stat(int(a, 0, c)?)));
    reg!(ctx, "set_stat", 2, |_, a, c| {
        api::set_stat(int(a, 0, c)?, int(a, 1, c)?);
        u()
    });
    reg!(ctx, "current_character", 0, |_, _, _| i(api::stat(31)));
    reg!(ctx, "g", 1, |_, a, c| i(api::var_g(int(a, 0, c)?)));
    reg!(ctx, "set_g", 2, |_, a, c| {
        api::set_g(int(a, 0, c)?, int(a, 1, c)?);
        u()
    });
    reg!(ctx, "p", 1, |_, a, c| i(api::var_p(int(a, 0, c)?)));
    reg!(ctx, "set_p", 2, |_, a, c| {
        api::set_p(int(a, 0, c)?, int(a, 1, c)?);
        u()
    });
    reg!(ctx, "run_script", 2, |_, a, c| {
        api::run_script(int(a, 0, c)?, int(a, 1, c)?);
        u()
    });
    reg!(ctx, "op_now", 2, |_, a, c| {
        let id = int(a, 0, c)?;
        let v = ints(&a.get(1).cloned().unwrap_or_default(), c)?;
        api::op_now(id, &v);
        u()
    });
    ctx
}

pub fn eval_src(ctx: &mut Context, src: &str) {
    ctx.eval(Source::from_bytes(src)).expect("eval");
}

pub struct BoaRt {
    pub ctx: Context,
    fns: Vec<(String, JsObject)>,
}

impl BoaRt {
    pub fn new(src: &str) -> BoaRt {
        let mut ctx = context();
        ctx.eval(Source::from_bytes(PRELUDE)).expect("prelude");
        ctx.eval(Source::from_bytes(src)).expect("story");
        BoaRt { ctx, fns: Vec::new() }
    }
    fn func(&mut self, name: &str) -> JsObject {
        if let Some((_, f)) = self.fns.iter().find(|(n, _)| n == name) {
            return f.clone();
        }
        let v = self.ctx.global_object().get(JsString::from(name), &mut self.ctx).expect("lookup");
        let f = v.as_object().expect("function");
        self.fns.push((name.to_string(), f.clone()));
        f
    }
}

impl host::Runtime for BoaRt {
    fn name(&self) -> &'static str {
        "boa"
    }
    fn call(&mut self, handler: &str, arg: i64) -> i64 {
        let f = self.func(handler);
        let r = f
            .call(&JsValue::undefined(), &[JsValue::new(arg as i32)], &mut self.ctx)
            .unwrap_or_else(|e| panic!("{handler}: {e}"));
        r.to_number(&mut self.ctx).unwrap_or(0.0) as i64
    }
}
