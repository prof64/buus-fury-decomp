//! cargo run --release -p bench
use host::{frame, scenario, scenario_setup, set_world, take_world, updates_only, with_world, NativeRt, Runtime};
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn per_frame_ns(rt: &mut dyn Runtime, frames: u64) -> f64 {
    set_world(host::World::new(50, 2));
    // warm up
    for a in 0..50 {
        rt.call("on_update", a);
    }
    let mut samples = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..frames {
            for a in 0..50 {
                rt.call("on_update", a);
            }
            // keep the world from growing unboundedly: clear timelines (not part of the measured script cost)
            with_world(|w| {
                w.timelines.clear();
                w.trace.clear()
            });
        }
        samples.push(t.elapsed().as_nanos() as f64 / frames as f64);
    }
    median(samples)
}

/// Rollback check: run to frame 60, snapshot, run 60 more (A); restore, run 60 more (B); A must equal B.
fn rollback_ok(rt: &mut dyn Runtime) -> (bool, f64, usize) {
    scenario_setup(rt);
    for _ in 0..60 {
        frame(rt);
    }
    let t = Instant::now();
    let mut snap = take_world();
    for _ in 0..999 {
        snap = std::hint::black_box(take_world());
    }
    let snap_ns = t.elapsed().as_nanos() as f64 / 1000.0;
    let approx_bytes = std::mem::size_of_val(&snap)
        + snap.trace.iter().map(|s| s.len() + 24).sum::<usize>()
        + snap.actors.len() * std::mem::size_of::<host::Actor>()
        + snap.flags.len() * 8;
    for _ in 0..60 {
        frame(rt);
    }
    let a = with_world(|w| w.digest());
    set_world(snap);
    for _ in 0..60 {
        frame(rt);
    }
    let b = with_world(|w| w.digest());
    (a == b, snap_ns, approx_bytes)
}

fn main() {
    let n = 20;
    // cold start: engine/context creation + compile
    let mut rhai_new = Vec::new();
    let mut rhai_compile = Vec::new();
    for _ in 0..n {
        let t = Instant::now();
        let e = rt_rhai::engine();
        rhai_new.push(t.elapsed().as_secs_f64() * 1e6);
        let t = Instant::now();
        let ast = e.compile(rt_rhai::STORY).unwrap();
        rhai_compile.push(t.elapsed().as_secs_f64() * 1e6);
        std::hint::black_box(ast);
    }
    let mut boa_new = Vec::new();
    let mut boa_compile = Vec::new();
    for _ in 0..n {
        let t = Instant::now();
        let mut c = rt_boa::context();
        boa_new.push(t.elapsed().as_secs_f64() * 1e6);
        let t = Instant::now();
        rt_boa::eval_src(&mut c, rt_boa::PRELUDE);
        rt_boa::eval_src(&mut c, rt_boa::STORY);
        boa_compile.push(t.elapsed().as_secs_f64() * 1e6);
    }
    println!("cold rhai: engine {:.0} us, compile story {:.0} us", median(rhai_new), median(rhai_compile));
    println!("cold boa: context {:.0} us, eval prelude+story {:.0} us", median(boa_new), median(boa_compile));

    let mut rh = rt_rhai::RhaiRt::new(rt_rhai::STORY);
    let mut bo = rt_boa::BoaRt::new(rt_boa::STORY);
    let mut mi = rt_mini::MiniRt::new();
    let mut na = NativeRt;

    // determinism / equivalence
    let d_rh = scenario(&mut rh, 600);
    let trace_rh = take_world().trace;
    let d_bo = scenario(&mut bo, 600);
    let trace_bo = take_world().trace;
    println!("scenario digest rhai {d_rh:016x} boa {d_bo:016x} equal={}", d_rh == d_bo);
    if d_rh != d_bo {
        for (x, y) in trace_rh.iter().zip(trace_bo.iter()) {
            if x != y {
                println!("first diff:\n  rhai {x}\n  boa  {y}");
                break;
            }
        }
    }
    println!("scenario trace lines: {}", trace_rh.len());
    for l in trace_rh.iter().take(12) {
        println!("  {l}");
    }
    let u = [
        updates_only(&mut rh, 300),
        updates_only(&mut bo, 300),
        updates_only(&mut mi, 300),
        updates_only(&mut na, 300),
    ];
    println!("updates-only digests rhai/boa/mini/native {:016x} {:016x} {:016x} {:016x}", u[0], u[1], u[2], u[3]);

    // per-frame overhead
    let frames = 2000;
    for rt in [&mut na as &mut dyn Runtime, &mut mi, &mut rh, &mut bo] {
        let ns = per_frame_ns(rt, frames);
        println!("per-frame 50x on_update {:12}: {:9.0} ns/frame ({:6.0} ns/call)", rt.name(), ns, ns / 50.0);
    }

    // same work as the wasm exports (construction + run), for the native/wasm speed ratio
    let time_ms = |f: &mut dyn FnMut() -> u64| {
        f();
        let t = Instant::now();
        for _ in 0..5 {
            std::hint::black_box(f());
        }
        t.elapsed().as_secs_f64() * 1e3 / 5.0
    };
    println!(
        "native equivalents of wasm exports: mini updates {:.2} ms | rhai updates {:.2} ms, scenario {:.2} ms | boa updates {:.2} ms, scenario {:.2} ms",
        time_ms(&mut || updates_only(&mut rt_mini::MiniRt::new(), 300)),
        time_ms(&mut || updates_only(&mut rt_rhai::RhaiRt::new(rt_rhai::STORY), 300)),
        time_ms(&mut || scenario(&mut rt_rhai::RhaiRt::new(rt_rhai::STORY), 600)),
        time_ms(&mut || updates_only(&mut rt_boa::BoaRt::new(rt_boa::STORY), 300)),
        time_ms(&mut || scenario(&mut rt_boa::BoaRt::new(rt_boa::STORY), 600)),
    );
    // Rhai with full AST optimization
    {
        let mut e = rt_rhai::engine();
        e.set_optimization_level(rhai::OptimizationLevel::Full);
        let ast = e.compile(rt_rhai::STORY).unwrap();
        let mut r = rt_rhai::RhaiRt { engine: e, ast, scope: rhai::Scope::new() };
        let ns = per_frame_ns(&mut r, frames);
        println!("per-frame 50x on_update rhai(opt=Full): {ns:9.0} ns/frame ({:6.0} ns/call)", ns / 50.0);
    }

    // rollback
    for rt in [&mut rh as &mut dyn Runtime, &mut bo] {
        let (ok, ns, bytes) = rollback_ok(rt);
        println!("rollback {:5}: restore+replay identical={ok}; World snapshot {ns:.0} ns, ~{bytes} B", rt.name());
    }

    // Boa realm hazard: script-visible mutable global state is not part of the World snapshot.
    let mut bo2 = rt_boa::BoaRt::new("var counter = 0; function on_update(a) { counter++; return counter; }");
    set_world(host::World::new(50, 2));
    let before = bo2.call("on_update", 0);
    let snap = take_world();
    bo2.call("on_update", 0);
    set_world(snap);
    let after = bo2.call("on_update", 0);
    println!("boa global-state hazard: first call {before}, after restore {after} (identical would be {})", before + 1);
    let t = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(rt_boa::BoaRt::new(rt_boa::STORY));
    }
    println!("boa full context rebuild (fallback snapshot): {:.0} us", t.elapsed().as_secs_f64() * 1e6 / 20.0);
    let t = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(rt_rhai::RhaiRt::new(rt_rhai::STORY));
    }
    println!("rhai full rebuild: {:.0} us", t.elapsed().as_secs_f64() * 1e6 / 20.0);
    // Rhai: same hazard is impossible — functions cannot see or mutate globals; the Scope is rewound.
    let mut rh2 = rt_rhai::RhaiRt::new("let counter = 0; fn on_update(a) { counter += 1; counter }");
    set_world(host::World::new(50, 2));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rh2.call("on_update", 0)));
    println!("rhai global mutation from fn: {}", if r.is_err() { "rejected (fn cannot see globals)" } else { "allowed" });

    // mid-execution snapshot (mini VM only)
    set_world(host::World::new(50, 2));
    let code = rt_mini::on_update_code();
    let mut vm = rt_mini::Vm::default();
    vm.locals[0] = 7;
    vm.step(&code, 10);
    let snap_vm = vm.clone();
    let snap_w = take_world();
    let r1 = vm.step(&code, usize::MAX);
    set_world(snap_w);
    let mut vm2 = snap_vm;
    let r2 = vm2.step(&code, usize::MAX);
    println!("mini VM mid-execution snapshot/restore: {r1:?} == {r2:?}: {}", r1 == r2);
}
