//! The agent server's story session against the app's (`wf_app::launcher::title_app`, what the native and web shells
//! run from the launcher's Story -> title): the same inputs, the world compared every step (state hash; on a
//! difference the differing fields). Headless.
//!
//!   cargo run --release -p wf-mcp --example appcheck -- [STEPS]

use serde_json::{json, Value};
use wf_mcp::tools::Server;

fn diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    if out.len() >= 40 || a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                diff(v, y.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"), out);
            }
            for k in y.keys().filter(|k| !x.contains_key(*k)) {
                out.push(format!("{path}.{k}: only in the app"));
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                diff(v, w, &format!("{path}[{i}]"), out);
            }
        }
        _ => out.push(format!("{path}: agent {} | app {}", trunc(a), trunc(b))),
    }
}

/// the world's serializable parts
fn parts(w: &wf_core::world::World) -> Value {
    let j = |v: Result<Value, serde_json::Error>| v.unwrap_or(Value::Null);
    json!({
        "frame": w.frame, "next_id": w.next_id, "serial": w.serial, "players": w.players, "enemies": w.enemies,
        "ents": j(serde_json::to_value(&w.ents)), "rng": j(serde_json::to_value(&w.rng)), "g": j(serde_json::to_value(&w.g)),
        "story": j(serde_json::to_value(&w.story)), "field": j(serde_json::to_value(&w.field)), "scene": j(serde_json::to_value(&w.scene)),
        "menu": j(serde_json::to_value(&w.menu)), "saves": j(serde_json::to_value(&w.saves)), "sound": j(serde_json::to_value(&w.sound)),
        "opts": j(serde_json::to_value(&w.opts)), "battle": j(serde_json::to_value(&w.battle)), "modk": j(serde_json::to_value(&w.modk)),
        "cameras": j(serde_json::to_value(&w.cameras)), "input": j(serde_json::to_value(&w.input)),
    })
}

fn trunc(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 160 {
        format!("{}...", &s[..160])
    } else {
        s
    }
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(700);
    let dir = wf_mcp::content::content_dir().expect("content");
    let packs = wf_app::wf_content::Packs::new(vec![Box::new(wf_app::wf_content::DirSource(dir))]).expect("packs");
    let mut app = wf_app::launcher::title_app(packs, &[vec![], vec![], vec![]], 1).expect("title app");
    let mut srv = Server::new();
    let id = match srv.call("new_game", &json!({"seed": 1, "title": true})) {
        Ok(wf_mcp::tools::Out::Json(v)) => v["session"].as_str().unwrap().to_string(),
        _ => panic!("new_game"),
    };
    let world_of = |app: &wf_app::App| -> Value {
        let s = app.sim.as_any().unwrap().downcast_ref::<wf_app::story::StorySim>().unwrap();
        parts(&s.session.world)
    };
    // run.py's inputs: START taps, then A every 6th step
    let keys: Vec<u16> = (0..n).map(|k| if k % 2 == 1 { 0 } else if k < 60 { 8 } else if k % 6 == 0 { 1 } else { 0 }).collect();
    for k in 0..=n {
        let ag = &srv.sessions[&id];
        let (ha, hb) = (ag.s.world.state_hash(), app.sim.state_hash());
        if ha != hb {
            println!("step {k}: agent {ha:016x} app {hb:016x}");
            let mut out = Vec::new();
            diff(&parts(&ag.s.world), &world_of(&app), "world", &mut out);
            for d in out {
                println!("  {d}");
            }
            return;
        }
        if k == n {
            break;
        }
        srv.sessions.get_mut(&id).unwrap().step(0, keys[k]);
        app.inputs[0] = keys[k];
        app.step();
    }
    println!("equal through {n} steps (state {:016x})", app.sim.state_hash());
}
