//! Rhai embedding of the host API. Integer-only (`no_float`, `only_i64`), no clock (`no_time`).

use host::{api, Action};
use rhai::{Array, Dynamic, Engine, Scope, AST};

pub const STORY: &str = include_str!("../../scripts/story.rhai");

fn ints(a: Array) -> Vec<i64> {
    a.into_iter().map(|d| d.as_int().unwrap_or(0)).collect()
}
fn acts(a: Array) -> Vec<Action> {
    a.into_iter().filter_map(|d| d.try_cast::<Action>()).collect()
}

pub fn engine() -> Engine {
    #[cfg(not(feature = "slim"))]
    let mut e = Engine::new();
    // slim: only the packages the scripts use (size measurement)
    #[cfg(feature = "slim")]
    let mut e = {
        use rhai::packages::Package;
        let mut e = Engine::new_raw();
        e.register_global_module(rhai::packages::CorePackage::new().as_shared_module());
        e.register_global_module(rhai::packages::BasicArrayPackage::new().as_shared_module());
        e
    };
    // operation budget: a runaway mod script cannot hang the simulation (deterministic: counts ops, not time)
    e.set_max_operations(1_000_000);
    e.set_max_call_levels(32);
    e.register_type_with_name::<Action>("Action");
    // timeline constructors (plain data)
    e.register_fn("say", |s: i64, t: i64| Action::Say { speaker: s, text: t });
    e.register_fn("wait", Action::Wait);
    e.register_fn("walk_to", |a: i64, x: i64, y: i64| Action::WalkTo { actor: a, x, y });
    e.register_fn("face", |a: i64, d: i64| Action::Face { actor: a, dir: d });
    e.register_fn("transform", |a: i64, f: i64| Action::Transform { actor: a, form: f });
    e.register_fn("play_sfx", Action::PlaySfx);
    e.register_fn("start_battle", Action::StartBattle);
    e.register_fn("flag_later", Action::SetFlag);
    e.register_fn("op", |id: i64, args: Array| Action::Op { id, args: ints(args) });
    e.register_fn("sequence", |v: Array| Action::Sequence(acts(v)));
    e.register_fn("parallel", |v: Array| Action::Parallel(acts(v)));
    e.register_fn("then_call", |name: &str, arg: i64| Action::Call { name: name.into(), arg });
    e.register_fn("run", api::run);
    // immediate queries / state
    e.register_fn("get_flag", api::get_flag);
    e.register_fn("set_flag", api::set_flag);
    e.register_fn("rand", api::rand);
    e.register_fn("chance", api::chance);
    e.register_fn("player", api::player);
    e.register_fn("actor_x", api::actor_x);
    e.register_fn("actor_y", api::actor_y);
    e.register_fn("set_state", api::set_state);
    e.register_fn("give_item", api::give_item);
    e.register_fn("is_equipped", api::is_equipped);
    e.register_fn("stat", api::stat);
    e.register_fn("set_stat", api::set_stat);
    e.register_fn("current_character", || api::stat(31));
    e.register_fn("g", api::var_g);
    e.register_fn("set_g", api::set_g);
    e.register_fn("p", api::var_p);
    e.register_fn("set_p", api::set_p);
    e.register_fn("run_script", api::run_script);
    e.register_fn("op_now", |id: i64, args: Array| api::op_now(id, &ints(args)));
    e
}

pub struct RhaiRt {
    pub engine: Engine,
    pub ast: AST,
    pub scope: Scope<'static>,
}

impl RhaiRt {
    pub fn new(src: &str) -> RhaiRt {
        let engine = engine();
        let ast = engine.compile(src).expect("compile");
        RhaiRt { engine, ast, scope: Scope::new() }
    }
}

impl host::Runtime for RhaiRt {
    fn name(&self) -> &'static str {
        "rhai"
    }
    fn call(&mut self, handler: &str, arg: i64) -> i64 {
        let opts = rhai::CallFnOptions::new().eval_ast(false).rewind_scope(true);
        let r: Dynamic = self
            .engine
            .call_fn_with_options(opts, &mut self.scope, &self.ast, handler, (arg,))
            .unwrap_or_else(|e| panic!("{handler}: {e}"));
        r.as_int().unwrap_or(0)
    }
}
