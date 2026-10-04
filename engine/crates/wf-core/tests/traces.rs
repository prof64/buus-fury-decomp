//! Emulator traces of the ported systems, replayed with exact equality (as site/tests/battle/buu_collision.test.ts):
//! the player's try-move on the map grids, the enemy path follower, the enemy step nodes, knockback / wall splat.
mod common;
use common::{data, json};
use serde_json::Value;
use wf_core::ai::PathRt;
use wf_core::battle::{BattleConfig, Phase, RngInit, ARENAS};
use wf_core::enemy::{self, KnockState};
use wf_core::input::{self, InputFrame};
use wf_core::session::Session;
use wf_core::world::Body;

fn sim(enemy: &str, count: i32, arena: &str) -> Session {
    let cfg = BattleConfig {
        arena: arena.into(),
        phases: vec![Phase {
            enemy: enemy.into(),
            count,
        }],
        rng: RngInit::State(12345, 12345 * 3 + 1),
        ..Default::default()
    };
    Session::new(data(), cfg)
}
fn enemies(s: &Session) -> Vec<usize> {
    s.world
        .ents
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.body, Body::Enemy(_)))
        .map(|(i, _)| i)
        .collect()
}
fn player(s: &Session) -> usize {
    s.world.player_idx(0).unwrap()
}
fn i32s(v: &Value) -> Vec<i32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap() as i32)
        .collect()
}
fn pair(v: &Value) -> (i32, i32) {
    let a = i32s(v);
    (a[0], a[1])
}
fn u32pair(v: &Value) -> [u32; 2] {
    [v[0].as_u64().unwrap() as u32, v[1].as_u64().unwrap() as u32]
}
fn mark_tiles(s: &mut Session, v: &Value, val: u8) {
    let c = s.world.col.as_mut().unwrap();
    for t in v.as_array().unwrap() {
        let (tx, ty) = pair(t);
        c.enemy[ty as usize * wf_core::content::GRID + tx as usize] = val;
    }
}

#[test]
fn player_try_move_vs_emulator() {
    let t = json("buu_collision_traces.json");
    let mut n = 0;
    for tr in t["traces"].as_array().unwrap() {
        let map = tr["map"].as_str().unwrap();
        let arena = ARENAS.iter().find(|a| a.1 == map).unwrap().0;
        let mut s = sim("t68", 1, arena);
        for e in enemies(&s) {
            enemy::unmark_self(&mut s.world, e);
            s.world.ents[e].removed = true;
        }
        let pi = player(&s);
        let fp = pair(&tr["footprint"]);
        assert_eq!(
            (s.world.pl_ref(pi).a.foot_w, s.world.pl_ref(pi).a.foot_h),
            fp
        );
        let (sx, sy) = pair(&tr["start"]);
        {
            let p = s.world.pl(pi);
            p.a.x = sx << 8;
            p.a.y = sy << 8;
        }
        let pos: Vec<(i32, i32)> = tr["pos"].as_array().unwrap().iter().map(pair).collect();
        let mut got = Vec::new();
        let mut prev = 0u16;
        for k in tr["keys"].as_array().unwrap() {
            let held = input::parse_keys(
                &k[0]
                    .as_str()
                    .unwrap()
                    .chars()
                    .map(|c| match c {
                        'L' => "LEFT+",
                        'R' => "RIGHT+",
                        'U' => "UP+",
                        'D' => "DOWN+",
                        'A' => "A+",
                        _ => "",
                    })
                    .collect::<String>(),
            );
            for _ in 0..k[1].as_i64().unwrap() {
                let w = &mut s.world;
                w.events.clear();
                w.input[0] = InputFrame::from_held(held, prev);
                prev = held;
                w.step_entities();
                wf_core::battle::post_step(w);
                let pi = w.player_idx(0).unwrap();
                got.push((w.pl_ref(pi).a.x, w.pl_ref(pi).a.y));
            }
        }
        for (f, p) in pos.iter().enumerate() {
            assert_eq!(got[f], *p, "{} frame {f}", tr["name"]);
        }
        n += 1;
    }
    assert!(n > 0);
}

#[test]
fn enemy_path_follower_vs_emulator() {
    let t = json("buu_path_traces.json");
    for r in t["runs"].as_array().unwrap() {
        let mut s = sim("t68", 1, "forest");
        let e = enemies(&s)[0];
        s.world.ents[e].removed = false;
        enemy::unmark_self(&mut s.world, e);
        mark_tiles(&mut s, &r["marked"], 0);
        assert_eq!(
            (s.world.en_ref(e).a.foot_w, s.world.en_ref(e).a.foot_h),
            pair(&r["footprint"])
        );
        let (sx, sy) = pair(&r["start"]);
        s.world.en(e).a.x = sx;
        s.world.en(e).a.y = sy;
        enemy::mark_self(&mut s.world, e);
        let (gx, gy) = pair(&r["goal"]);
        let mut p = PathRt {
            speed: r["speed"].as_i64().unwrap() as i32,
            budget: 50,
            gx: gx << 8,
            gy: gy << 8,
            ..Default::default()
        };
        let pos = r["pos"].as_array().unwrap();
        for f in 0..pos.len() {
            wf_core::path::update(&mut s.world, e, &mut p);
            assert_eq!(
                (p.wx, p.wy),
                pair(&r["waypoint"][f]),
                "{} frame {f} waypoint",
                r["name"]
            );
            let a = &s.world.en_ref(e).a;
            assert_eq!((a.x, a.y), pair(&pos[f]), "{} frame {f}", r["name"]);
        }
    }
}

#[test]
fn enemy_steps_vs_emulator() {
    let t = json("buu_step_traces.json");
    for r in t["runs"].as_array().unwrap() {
        let mut s = sim("t68", 1, "forest");
        let e = enemies(&s)[0];
        let pi = player(&s);
        enemy::unmark_self(&mut s.world, e);
        mark_tiles(&mut s, &r["marked"], 0);
        assert_eq!(
            (s.world.en_ref(e).a.foot_w, s.world.en_ref(e).a.foot_h),
            pair(&r["footprint"])
        );
        assert_eq!(
            (s.world.pl_ref(pi).a.foot_w, s.world.pl_ref(pi).a.foot_h),
            pair(&r["playerFootprint"])
        );
        let (px, py) = pair(&r["player"]);
        s.world.pl(pi).a.x = px;
        s.world.pl(pi).a.y = py;
        let (sx, sy) = pair(&r["start"]);
        s.world.en(e).a.x = sx;
        s.world.en(e).a.y = sy;
        s.world.en(e).a.facing = r["face"].as_i64().unwrap() as u8;
        enemy::mark_self(&mut s.world, e);
        let mut st = r["param2"].as_i64().unwrap() as i32;
        let speed = r["speed"].as_i64().unwrap() as i32;
        let fname = r["fn"].as_str().unwrap();
        for (f, g) in r["frames"].as_array().unwrap().iter().enumerate() {
            if g["ret"].as_u64() == Some(0xdeadbeef) {
                continue;
            }
            s.world.rng.set_state(u32pair(&g["rng"]));
            let (gpx, gpy) = pair(&g["player"]);
            s.world.pl(pi).a.x = gpx;
            s.world.pl(pi).a.y = gpy;
            let ret: Option<bool> = match fname {
                "0x08018C3A" => Some(enemy::step_from_player(&mut s.world, e, speed, true)),
                "0x0801CBA1" => Some(enemy::step_from_player(&mut s.world, e, speed, false)),
                "0x0802B708" => {
                    enemy::wander_step(&mut s.world, e, speed, r["param2"].as_i64().unwrap());
                    None
                }
                _ => Some(enemy::walk_to_player(
                    &mut s.world,
                    e,
                    speed,
                    &mut st,
                    g["b11"].as_i64().unwrap() as i32,
                )),
            };
            let a = &s.world.en_ref(e).a;
            let (gx, gy) = pair(&g["pos"]);
            assert_eq!(
                (a.x, a.y, a.facing as i64),
                (gx, gy, g["face"].as_i64().unwrap()),
                "{} frame {f}",
                r["name"]
            );
            if fname == "0x0802CDE8" {
                assert_eq!(
                    st as i64,
                    g["n8"].as_i64().unwrap(),
                    "{} frame {f} n8",
                    r["name"]
                );
            }
            if let Some(ret) = ret {
                assert_eq!(
                    ret as i64,
                    g["ret"].as_i64().unwrap(),
                    "{} frame {f} ret",
                    r["name"]
                );
            }
        }
    }
}

#[test]
fn knockback_vs_emulator() {
    let t = json("buu_knockback_traces.json");
    for r in t["runs"].as_array().unwrap() {
        let other = !r["other"].is_null();
        let mut s = sim("t68", if other { 2 } else { 1 }, "forest");
        let es = enemies(&s);
        for &x in &es {
            enemy::unmark_self(&mut s.world, x);
        }
        mark_tiles(&mut s, &r["marked"], 0);
        mark_tiles(&mut s, &r["holes"], 1);
        let e = es[0];
        assert_eq!(
            (s.world.en_ref(e).a.foot_w, s.world.en_ref(e).a.foot_h),
            pair(&r["footprint"])
        );
        let updates = r["updates"].as_array().unwrap();
        if other {
            let o = es[1];
            let (ox, oy) = pair(&r["other"]["at"]);
            s.world.en(o).a.x = ox << 8;
            s.world.en(o).a.y = oy << 8;
            s.world.en(o).hp1024 = updates.last().unwrap()["otherHp"].as_i64().unwrap();
        }
        let (sx, sy) = pair(&r["start"]);
        s.world.en(e).a.x = sx;
        s.world.en(e).a.y = sy;
        enemy::mark_self(&mut s.world, e);
        let en = &r["enter"];
        {
            let x = s.world.en(e);
            x.last_hit_dir = en["hitDir"].as_i64().unwrap() as u8;
            x.crit_hit = en["crit"].as_i64().unwrap() != 0;
            x.splat_hit = en["sub3"].as_i64().unwrap() != 0;
            x.a.blocking = en["blocking"].as_i64().unwrap() != 0;
        }
        s.world.rng.set_state(u32pair(&en["rng"]));
        let mut kb = KnockState {
            n: 16,
            ..Default::default()
        };
        enemy::knock_start(&mut s.world, e, en["K"].as_i64().unwrap() as i32, &mut kb);
        assert_eq!(
            s.world.en_ref(e).a.facing as i64,
            en["hitDir"].as_i64().unwrap() ^ 1
        );
        let mut done: i64 = -1;
        let mut k = 0;
        while k < updates.len() + 4 && done < 0 {
            if let Some(u) = updates.get(k) {
                let a = &s.world.en_ref(e).a;
                assert_eq!((a.x, a.y), pair(&u["pos"]), "{} update {k}", r["name"]);
                s.world.rng.set_state(u32pair(&u["rng"]));
            }
            wf_core::enemy::tick_anim(&mut s.world, e);
            if enemy::knock_step(&mut s.world, e, &mut kb) {
                done = k as i64;
            }
            k += 1;
        }
        assert_eq!(done, updates.len() as i64 - 1, "{}", r["name"]);
        if other {
            let o = es[1];
            let want = r["other"]["hp_after_splat"]
                .as_i64()
                .unwrap_or_else(|| updates.last().unwrap()["otherHp"].as_i64().unwrap());
            assert_eq!(s.world.en_ref(o).hp1024, want, "{} splat damage", r["name"]);
        }
    }
}
