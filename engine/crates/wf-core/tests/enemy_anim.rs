//! Enemy / boss animation timing and animation-layer sounds against the emulator (tools/battle/ENEMY_ANIM.md):
//! data/battle/buu_enemy_anim_traces.json holds per logic tick the raw animation and frame the game showed, the
//! animation calls its code made and the sounds the animation layer played. The model (eanim.rs) is driven with the
//! same calls and must show the same raw / frame on every modelled tick (as site/tests/battle/buu_enemy_anim.test.ts).
mod common;
use common::{data, json};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use wf_core::eanim::{setanim_table, EnemyAnim, Host, Tick};
use wf_core::player::AnimPos;

struct H<'a> {
    nf: &'a BTreeMap<i32, i32>,
    rq: VecDeque<Option<i32>>,
    tick: usize,
    sfx: Vec<(usize, i32)>,
    bad: Option<String>,
}
impl Host for H<'_> {
    fn nframes(&mut self, raw: i32) -> i32 {
        self.nf.get(&raw).copied().unwrap_or(0)
    }
    fn rand(&mut self, n: u32) -> i32 {
        match self.rq.pop_front() {
            Some(v) => v.unwrap_or(0),
            None => {
                self.bad
                    .get_or_insert(format!("tick {}: unexpected rand({n})", self.tick));
                0
            }
        }
    }
    fn sfx(&mut self, id: i32, _vol: i32) {
        self.sfx.push((self.tick, id));
    }
}

fn rle<T: Clone>(v: &Value, f: impl Fn(&Value) -> T) -> Vec<T> {
    let mut out = vec![];
    for x in v.as_array().unwrap() {
        for _ in 0..x[x.as_array().unwrap().len() - 1].as_i64().unwrap() {
            out.push(f(x));
        }
    }
    out
}

#[test]
fn enemy_animation_model_vs_emulator() {
    let d = data();
    let ar = &d.anim;
    let table = setanim_table();
    let setanim = |id: usize| -> (i32, u32) {
        let raw = table[id].0;
        (raw, ar.setanim[&raw])
    };
    let t = json("buu_enemy_anim_traces.json");
    let (mut total, mut skipped_all, mut n) = (0, 0, 0);
    let mut fails = vec![];
    for tr in t["traces"].as_array().unwrap() {
        let name = format!(
            "{} {}",
            tr["name"].as_str().unwrap(),
            tr["label"].as_str().unwrap_or("")
        );
        let ty = tr["type"].as_i64().unwrap() as i32;
        let rows: Vec<(i32, i32)> = rle(&tr["rows"], |x| {
            (x[0].as_i64().unwrap() as i32, x[1].as_i64().unwrap() as i32)
        });
        let defs: Vec<String> = rle(&tr["defs"], |x| x[0].as_str().unwrap().to_string());
        let nf: BTreeMap<String, BTreeMap<i32, i32>> = tr["nframes"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_object()
                        .unwrap()
                        .iter()
                        .map(|(r, n)| (r.parse().unwrap(), n.as_i64().unwrap() as i32))
                        .collect(),
                )
            })
            .collect();
        let canon: BTreeMap<String, BTreeMap<i32, i32>> = tr["canon"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_object()
                        .unwrap()
                        .iter()
                        .map(|(r, n)| (r.parse().unwrap(), n.as_i64().unwrap() as i32))
                        .collect(),
                )
            })
            .collect();
        let empty = BTreeMap::new();
        let init = tr["init"].as_array().unwrap();
        let mut ea = EnemyAnim {
            pos: AnimPos {
                raw: init[0].as_i64().unwrap() as i32,
                frame: init[1].as_i64().unwrap() as i32,
                tick: init[2].as_i64().unwrap() as i32,
            },
            ..Default::default()
        };
        let ctl0 = init[3].as_str().unwrap();
        let r0 = ar.for_controller(ctl0, ty);
        ea.rule = r0.unwrap_or(ar.keep);
        let mut known = r0.is_some();
        let mut cmds: BTreeMap<usize, Vec<&Value>> = BTreeMap::new();
        for c in tr["cmds"].as_array().unwrap() {
            cmds.entry(c[0].as_u64().unwrap() as usize)
                .or_default()
                .push(c);
        }
        let mut skip_at = BTreeMap::new();
        let mut reenter_at = BTreeMap::new();
        for s in tr["skips"].as_array().unwrap() {
            let k = s[0].as_u64().unwrap() as usize;
            if s[1].as_str() == Some("reenter") {
                reenter_at.insert(k, s[2].as_i64().unwrap_or(0) as i32);
            } else {
                skip_at.insert(k, s[1].as_i64().unwrap());
            }
        }
        let mut rand_at: BTreeMap<usize, Vec<Option<i32>>> = BTreeMap::new();
        for r in tr["rand"].as_array().unwrap() {
            rand_at
                .entry(r[0].as_u64().unwrap() as usize)
                .or_default()
                .push(r[2].as_i64().map(|v| v as i32));
        }
        let mut h = H {
            nf: &empty,
            rq: VecDeque::new(),
            tick: 0,
            sfx: vec![],
            bad: None,
        };
        let (mut compared, mut skipped) = (0, 0);
        let mut bad: Option<String> = None;
        for tick in 1..rows.len() {
            let nfm = nf.get(&defs[tick]).unwrap_or(&empty);
            h.nf = nfm;
            h.tick = tick;
            for v in rand_at.get(&tick).into_iter().flatten() {
                h.rq.push_back(*v);
            }
            let cs: Vec<&Value> = cmds.get(&tick).cloned().unwrap_or_default();
            let mut apply = |c: &Value, ea: &mut EnemyAnim, h: &mut H, known: &mut bool| match c[2]
                .as_str()
                .unwrap()
            {
                "anim" => {
                    let (raw, r) = setanim(c[3].as_u64().unwrap() as usize);
                    ea.enter(&ar.rules, raw, r, h);
                    *known = true;
                }
                "raw" => ea.pos.raw = c[3].as_i64().unwrap() as i32,
                "state" => {
                    let r = ar.for_controller(c[3].as_str().unwrap(), ty);
                    *known = r.is_some();
                    let raw = ea.pos.raw;
                    ea.enter(&ar.rules, raw, r.unwrap_or(ar.keep), h);
                }
                "hold" => {
                    let (raw, _) = setanim(c[3].as_u64().unwrap() as usize);
                    ea.enter(&ar.rules, raw, ar.keep, h);
                    ea.pos.frame = c[4].as_i64().unwrap() as i32;
                    *known = true;
                }
                _ => {}
            };
            for c in &cs {
                if c[1].as_str() == Some("pre") {
                    apply(c, &mut ea, &mut h, &mut known);
                }
            }
            let end_cmd = cs.iter().any(|c| c[1].as_str() == Some("end"));
            let sk = skip_at.get(&tick).copied();
            let mut res = if sk == Some(0) {
                Tick::None
            } else {
                ea.tick(&ar.rules, &mut h)
            };
            if sk == Some(2) && matches!(res, Tick::None) {
                res = ea.tick(&ar.rules, &mut h);
            }
            let idle = matches!(res, Tick::Idle);
            if idle || end_cmd {
                let r = &ar.rules[ea.rule as usize];
                if !idle && r.kind == wf_core::eanim::RuleKind::Seq && r.fixed != Some(false) {
                    bad = Some(format!(
                        "tick {tick}: the game ended the fixed-length state, the model did not"
                    ));
                    break;
                }
                let (raw, r) = setanim(0);
                ea.enter(&ar.rules, raw, r, &mut h);
                known = true;
            }
            for c in &cs {
                if c[1].as_str() == Some("post") {
                    apply(c, &mut ea, &mut h, &mut known);
                }
            }
            if let Some(&v) = reenter_at.get(&tick) {
                h.rq.push_back(Some(v));
                let (raw, r) = (ea.pos.raw, ea.rule);
                ea.enter(&ar.rules, raw, r, &mut h);
            }
            h.rq.clear();
            let (eraw, eframe) = rows[tick];
            if !known || eraw == -1 {
                skipped += 1;
                continue;
            }
            let c = canon
                .get(&defs[tick])
                .and_then(|m| m.get(&ea.pos.raw))
                .copied()
                .unwrap_or(ea.pos.raw);
            compared += 1;
            if c != eraw || ea.pos.frame != eframe {
                bad = Some(format!(
                    "tick {tick}: emulator raw {eraw} frame {eframe}, model raw {c} frame {}",
                    ea.pos.frame
                ));
                break;
            }
        }
        if let Some(b) = h.bad.take().or(bad) {
            fails.push(format!("{name}: {b}"));
        } else {
            assert!(
                compared * 10 > 9 * (compared + skipped),
                "{name}: too few modelled ticks"
            );
            let got: Vec<String> = h
                .sfx
                .iter()
                .filter(|s| s.0 > 0)
                .map(|s| format!("{}:{}", s.0, s.1))
                .collect();
            for s in tr["sfx"].as_array().unwrap() {
                let want = format!("{}:{}", s[0].as_i64().unwrap(), s[1].as_i64().unwrap());
                if !got.contains(&want) {
                    fails.push(format!("{name}: missing sound {want}"));
                    break;
                }
            }
        }
        total += compared;
        skipped_all += skipped;
        n += 1;
    }
    eprintln!(
        "enemy anim: {n} traces, {total} ticks compared, {skipped_all} not modelled, {} failures",
        fails.len()
    );
    for f in &fails {
        eprintln!("  FAIL {f}");
    }
    assert!(fails.is_empty());
    assert!(total > 50000);
}

/// Yakon (0x91) energy absorb vs the emulator (re/buu/YAKON.md, data/battle/buu_yakon_absorb.json): every hit replayed
/// through the on-hit tree with the recorded damage; HP after every hit and death must match
#[test]
fn yakon_energy_absorb() {
    use wf_core::battle::{BattleConfig, Phase, RngInit};
    use wf_core::session::Session;
    use wf_core::world::{Body, HitMsg};
    let rec = json("buu_yakon_absorb.json");
    for run in rec["runs"].as_array().unwrap() {
        let cfg = BattleConfig {
            arena: "yakon".into(),
            phases: vec![Phase {
                enemy: "t145".into(),
                count: 1,
            }],
            rng: RngInit::State(1, 4),
            ..Default::default()
        };
        let mut s = Session::new(data(), cfg);
        let w = &mut s.world;
        let e = w
            .ents
            .iter()
            .position(|x| matches!(x.body, Body::Enemy(_)))
            .unwrap();
        w.en(e).hp1024 = run["hp_start"].as_i64().unwrap();
        let hits = run["hits"].as_array().unwrap();
        let (mut got, mut want) = (vec![], vec![]);
        for (k, h) in hits.iter().enumerate() {
            if w.en_ref(e).defeated {
                break;
            }
            let st = h["state"].as_i64().unwrap() as u16;
            if w.en_ref(e).ai.top_state() != Some(st) {
                wf_core::nodes::reset(w, e, st);
            }
            let pid = w.players[0];
            let a = wf_core::damage::atk(h["sub"].as_i64().unwrap() as u8);
            assert_eq!(a.kind as i64, h["kind"].as_i64().unwrap());
            let msg = HitMsg {
                team: 1,
                rect: w.en_ref(e).a.body_box(),
                dir: 3,
                mult: h["mult"].as_i64().unwrap() as i32,
                atk: a,
                attacker: pid,
                att_stats: Default::default(),
                att_is_player: true,
                source: pid,
                crit: false,
                raw: 0,
                dmg: h["dmg"].as_i64().unwrap(),
                no_react: false,
                foot: false,
            };
            wf_core::nodes::on_hit(w, e, &msg);
            got.push(w.en_ref(e).hp1024);
            want.push(if k + 1 < hits.len() {
                hits[k + 1]["hp"].as_i64().unwrap()
            } else {
                run["hp_end"].as_i64().unwrap()
            });
        }
        assert_eq!(got, want, "{}", run["name"]);
        assert_eq!(
            w.en_ref(e).defeated,
            run["dead"].as_bool().unwrap(),
            "{}",
            run["name"]
        );
    }
}
