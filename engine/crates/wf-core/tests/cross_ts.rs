//! Cross-check against the TypeScript reference sim: the same fights (enemy groups, arenas, seeds, recorded inputs)
//! run in both, compared frame by frame (RNG state, camera, player, every enemy incl. its AI stack, every projectile,
//! the entity list). Dumps come from tools/battle/buu_core_dump.test.ts:
//!   cd site && WF_DUMP_DIR=<dir> npx vitest run --dir .. tools/battle/buu_core_dump.test.ts
//!   WF_TS_DUMPS=<dir> cargo test -p wf-core --test cross_ts
//! Without dumps the test reports that it was skipped. `CROSS=<name>` runs one scenario with a divergence report.
mod common;
use common::data;
use serde_json::{json, Value};
use std::path::PathBuf;
use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::session::Session;
use wf_core::world::{Body, World};

fn dumps_dir() -> Option<PathBuf> {
    let d = std::env::var("WF_TS_DUMPS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/wf-core-ts-dumps")
        });
    d.is_dir().then_some(d)
}

/// TS entity ids are ours + 1 (the reference adds a collision-overlay entity first)
const ID_OFF: u32 = 1;

fn frame_state(w: &World) -> Value {
    let pi = w.player_idx(0).unwrap();
    let p = w.pl_ref(pi);
    let held = if p.held_by != 0 {
        p.held_by + ID_OFF
    } else {
        0
    };
    let mut e = vec![];
    let mut ea = vec![];
    let mut s = vec![];
    let mut k = vec![];
    for (i, x) in w.ents.iter().enumerate() {
        if matches!(x.body, Body::Injector(_)) {
            continue;
        }
        k.push(format!("{}:{}", x.id + ID_OFF, x.kind()));
        match &x.body {
            Body::Enemy(n) => {
                ea.push(json!([x.id + ID_OFF, n.ea.pos.raw, n.ea.pos.frame]));
                e.push(json!([
                x.id + ID_OFF,
                n.ty,
                n.a.x,
                n.a.y,
                n.a.z,
                n.a.facing,
                n.hp1024,
                n.defeated as i32,
                n.a.hittable as i32,
                n.ai.states()
                    .iter()
                    .map(|s| format!("{s:x}"))
                    .collect::<Vec<_>>()
                    .join(">")
                ]))
            }
            Body::Shot(sh) => s.push(json!([x.id + ID_OFF, sh.kind_name(), sh.x, sh.y])),
            Body::Hand(_) => {}
            _ => {}
        }
        let _ = i;
    }
    let cam = w.camera(0);
    json!({
        "rng": [w.rng.a, w.rng.b],
        "cam": [cam.x0, cam.y0],
        "p": [p.a.x, p.a.y, p.a.facing, p.hp(), p.ep(), p.gs.name(), p.ap.raw, p.ap.frame, p.form, p.dead as i32, p.wraps, held],
        "e": e,
        "s": s,
        "k": k.join(" "),
        "ea": ea,
    })
}

fn norm_ts(f: &Value) -> Value {
    // TS AI stack ids are decimal numbers joined by '>'; ours hex
    let mut f = f.clone();
    if let Some(es) = f["e"].as_array_mut() {
        for e in es {
            let st = e[9]
                .as_str()
                .unwrap_or("")
                .split('>')
                .filter(|x| !x.is_empty())
                .map(|x| format!("{:x}", x.parse::<u32>().unwrap_or(0)))
                .collect::<Vec<_>>()
                .join(">");
            e[9] = json!(st);
        }
    }
    let gs = f["p"][5].as_str().unwrap_or("").to_string();
    f["p"][5] = json!(if gs == "idle2" {
        "idle".to_string()
    } else {
        gs
    });
    f
}

fn first_diff(a: &Value, b: &Value) -> Vec<String> {
    let mut out = vec![];
    for k in ["rng", "cam", "p", "e", "s", "k", "ea"] {
        // (each enemy's animation position: dumps written before it have none)
        if k == "ea" && a[k].is_null() {
            continue;
        }
        if a[k] != b[k] {
            out.push(k.to_string());
        }
    }
    out
}

#[test]
fn cross_check_vs_typescript_sim() {
    let Some(dir) = dumps_dir() else {
        eprintln!("cross_ts: no dumps (set WF_TS_DUMPS, see the file header) - skipped");
        return;
    };
    let only = std::env::var("CROSS").ok();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut fails = vec![];
    let mut total = 0;
    for f in files {
        let d: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        let sc = &d["scenario"];
        let name = sc["name"].as_str().unwrap().to_string();
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        let seed = sc["seed"].as_u64().unwrap() as u32;
        let fighter = FighterSetup {
            fighter: sc["fighterId"].as_str().unwrap_or("goku").into(),
            form: sc["form"].as_i64().unwrap_or(2) as i32,
            level: sc["level"].as_i64().unwrap_or(60) as i32,
            engage: sc["engage"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        let cfg = BattleConfig {
            arena: sc["arena"].as_str().unwrap().into(),
            players: vec![fighter],
            phases: vec![Phase {
                enemy: sc["enemy"].as_str().unwrap().into(),
                count: sc["count"].as_i64().unwrap_or(1) as i32,
            }],
            rng: RngInit::State(seed, seed.wrapping_mul(3).wrapping_add(1)),
            xp: false,
            spawn_near: None,
            arena_rules: false,
            even_level: None,
        };
        let mut s = Session::new(data(), cfg);
        // (`ehp`: the starting enemies' whole HP - a fight that reaches the deaths)
        if let Some(hp) = sc["ehp"].as_i64() {
            let ids = s.world.enemies.clone();
            for id in ids {
                if let Some((k, _)) = s.world.enemy_by_id(id) {
                    s.world.en(k).hp1024 = hp << 10;
                }
            }
        }
        let inputs: Vec<u16> = d["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as u16)
            .collect();
        let frames = d["frames"].as_array().unwrap();
        let mut bad = None;
        for (k, held) in inputs.iter().enumerate() {
            s.step(&[*held]);
            total += 1;
            let got = frame_state(&s.world);
            let exp = norm_ts(&frames[k]);
            let diff = first_diff(&exp, &got);
            if !diff.is_empty() {
                if only.is_some() {
                    for j in k.saturating_sub(2)..k {
                        eprintln!("  ts  {}: {}", j + 1, norm_ts(&frames[j]));
                    }
                    eprintln!(">> ts  {}: {}", k + 1, exp);
                    eprintln!(">> rs  {}: {}", k + 1, got);
                }
                bad = Some(format!("{name}: frame {} ({})", k + 1, diff.join(", ")));
                break;
            }
        }
        match bad {
            Some(b) => fails.push(b),
            None => eprintln!("cross_ts: {name} ok ({} frames)", inputs.len()),
        }
    }
    for f in &fails {
        eprintln!("cross_ts FAIL {f}");
    }
    eprintln!(
        "cross_ts: {total} frames compared, {} scenario failures",
        fails.len()
    );
    assert!(fails.is_empty());
}
