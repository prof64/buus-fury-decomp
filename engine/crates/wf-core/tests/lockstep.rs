//! Frame-by-frame lockstep of the player against the emulator (data/battle/buu_lockstep/*.json, recorded by
//! tools/battle/lockstep/record.py; format tools/battle/LOCKSTEP.md): the same start state and inputs; state, raw
//! animation, frame, facing, 24.8 position, HP, EP, form, ROM frame record, RNG word and every sound call must match
//! on every frame. The pass set must equal the reference simulator's (`LOCKSTEP_EXPECT=<file of names>` checks it;
//! `LOCKSTEP=<substring>` runs a subset and prints the first divergence).
mod common;
use common::{battle_data, data};
use serde_json::Value;
use wf_core::battle::{self, BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::input::InputFrame;
use wf_core::player;
use wf_core::session::Session;
use wf_core::world::{Body, Event, InjectedHit, Injector, World};

#[derive(Clone, Debug, PartialEq)]
struct Row {
    x: i64,
    y: i64,
    dir: i64,
    raw: i64,
    frame: i64,
    state: String,
    hp: i64,
    ep: i64,
    form: i64,
    rom: i64,
    rng: Option<i64>,
    sfx: Vec<String>,
}

const FIGHTER_OF_CHAR: [&str; 8] = ["goku", "gohan", "goten", "trunks", "vegeta", "gotenks", "vegito", "gogeta"];

/// scenarios whose divergence is understood and not modelled (the reference's KNOWN list)
fn known(name: &str) -> bool {
    let pre = |p: &[&str]| p.iter().any(|x| name.starts_with(x));
    pre(&["f24_", "f25_", "f32_", "f33_", "f42_", "f43_"])
        || ["f23_bump", "f23_move", "f23_punch", "f23_skill11", "f46_punch", "f14_skill20", "f14_ssdrain"].contains(&name)
        || ["f26_skill2", "f27_skill2", "f28_skill2", "f29_skill2", "f30_skill2"].contains(&name)
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

fn expand(t: &Value) -> Vec<Row> {
    let states: Vec<String> = t["states"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
    let mut out = Vec::new();
    for g in t["rows"].as_array().unwrap() {
        let g: Vec<i64> = g.as_array().unwrap().iter().map(i).collect();
        for k in 0..g[0] {
            out.push(Row { x: g[1] + g[3] * k, y: g[2] + g[4] * k, dir: g[5], raw: g[6], frame: g[7], state: states[g[8] as usize].clone(), hp: g[9], ep: g[10], form: g[11], rom: g[12], rng: g.get(13).copied(), sfx: vec![] });
        }
    }
    for s in t["sfx"].as_array().unwrap() {
        let a = s.as_array().unwrap();
        let f = i(&a[0]) as usize;
        if a[1].as_str() == Some("dmg") {
            continue;
        }
        if let Some(r) = out.get_mut(f.wrapping_sub(1)) {
            r.sfx.push(a[1..].iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(":"));
        }
    }
    out
}

fn setup(t: &Value) -> Session {
    let d = data();
    let ini = &t["init"];
    let rec = &ini["rec"];
    let scn_form = i(&t["scenario"]["form"]) as i32;
    let form = rec.get("form").map(i).unwrap_or(scn_form as i64) as i32;
    let ch = d.form(scn_form).unwrap().char_;
    let eq: Vec<i32> = rec["equip"].as_array().unwrap().iter().map(|x| i(x) as i32).collect();
    let levels: Vec<i64> = rec["levels"].as_array().unwrap().iter().map(i).collect();
    let fighter = FighterSetup {
        fighter: if ch == 8 { format!("f{form}") } else { FIGHTER_OF_CHAR[ch as usize].to_string() },
        form,
        level: 60,
        equip: [eq[0], eq[1], eq[2], eq[3]],
        skill_level: *levels.first().unwrap_or(&0) as i32,
        ..Default::default()
    };
    let arena = t["scenario"]["arena"].as_str().unwrap_or("htc").to_string();
    let cfg = BattleConfig { arena, players: vec![fighter], phases: vec![Phase { enemy: "t-1".into(), count: 1 }], rng: RngInit::State(0, 0), xp: false };
    let mut s = Session::new(d, cfg);
    let w = &mut s.world;
    let pi = w.player_idx(0).unwrap();
    {
        let p = w.pl(pi);
        p.rec.hp = i(&rec["hp"]) as i32;
        p.rec.max_hp = i(&rec["maxHp"]) as i32;
        p.rec.ep = i(&rec["ep"]) as i32;
        p.rec.max_ep = i(&rec["maxEp"]) as i32;
        let base: Vec<i32> = rec["base"].as_array().unwrap().iter().map(|x| i(x) as i32).collect();
        p.rec.str_ = base[0];
        p.rec.pow = base[1];
        p.rec.end = base[2];
        p.rec.spd = base[3];
        let skills: Vec<i32> = rec["skills"].as_array().unwrap().iter().map(|x| i(x) as i32).collect();
        p.skills = if skills.is_empty() { vec![7] } else { skills };
        p.sel = i(&rec["slot"]) as usize;
        p.skill_level = *levels.first().unwrap_or(&0) as i32;
        p.ss_gauge = i(&rec["ssGauge"]) as i32;
        p.ss3_gauge = i(&rec["ss3Gauge"]) * 65536;
    }
    let mut p = w.pl_ref(pi).clone();
    p.apply_form(w, form);
    p.a.x = i(&ini["x"]) as i32;
    p.a.y = i(&ini["y"]) as i32;
    p.a.facing = i(&ini["dir"]) as u8;
    *w.pl(pi) = p;
    player::lockstep_init(w, pi, i(&ini["raw"]) as i32, i(&ini["frame"]) as i32, i(&ini["tick"]) as i32);
    let obj = &ini["obj"];
    let (ss, ss3) = {
        let f = w.data.form(w.pl_ref(pi).form).unwrap();
        (f.ss, f.ss3)
    };
    {
        let p = w.pl(pi);
        if let Some(v) = obj.get("regen").filter(|v| !v.is_null()) {
            p.regen_timer = (i(v) & 0xffff) as i32;
        }
        if let Some(v) = obj.get("ssacc").filter(|v| !v.is_null()) {
            if ss || ss3 {
                p.ss_acc = (i(v) & 0xffff) as i32;
            }
        }
        if let Some(v) = obj.get("cooldown").filter(|v| !v.is_null()) {
            p.ss_cooldown = i(v) as i32;
        }
        if let Some(v) = obj.get("idlecnt").filter(|v| !v.is_null()) {
            p.s_cnt = i(v) as i32;
        }
    }
    if let Some(c) = obj.get("camera").filter(|v| !v.is_null()) {
        battle::set_camera(w, 0, i(&c[0]) as i32, i(&c[1]) as i32);
    }
    if let Some(v) = obj.get("beepAge").filter(|v| !v.is_null()) {
        w.g.last_beep = -i(v);
    }
    w.rng.set_state([ini["rng"][0].as_u64().unwrap() as u32, ini["rng"][1].as_u64().unwrap() as u32]);
    let hits: Vec<InjectedHit> = t["scenario"]["hits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|h| InjectedHit {
                    at: i(&h["at"]) as u32,
                    dir: h.get("dir").map(i).unwrap_or(2) as u8,
                    atk: h.get("atk").map(i).unwrap_or(2) as u8,
                    mult: h.get("mult").map(i).unwrap_or(0x400) as i32,
                    str_: h.get("str").map(i).unwrap_or(30) as i32,
                    pow: h.get("pow").map(i).unwrap_or(30) as i32,
                })
                .collect()
        })
        .unwrap_or_default();
    let pid = w.ents[pi].id;
    w.add(Body::Injector(Box::new(Injector { hits, player: pid })));
    s
}

fn row(w: &World, sfx: Vec<String>) -> Row {
    let pi = w.player_idx(0).unwrap();
    let p = w.pl_ref(pi);
    let (state, raw, frame, rom) = player::lockstep_view(w, pi);
    Row { x: p.a.x as i64, y: p.a.y as i64, dir: p.a.facing as i64, raw: raw as i64, frame: frame as i64, state, hp: p.hp() as i64, ep: p.ep() as i64, form: p.form as i64, rom: rom as i64, rng: Some(w.rng.a as i64), sfx }
}

fn replay(t: &Value) -> Vec<Row> {
    let mut s = setup(t);
    let mut out = Vec::new();
    let mut prev = 0u16;
    for seg in t["input"].as_array().unwrap() {
        let keys = i(&seg[0]) as u16;
        for _ in 0..i(&seg[1]) {
            let mut inp = [InputFrame::default(); 4];
            inp[0] = InputFrame::from_held(keys, prev);
            prev = keys;
            let ev = s.step_frames(inp);
            let sfx = ev
                .iter()
                .filter_map(|e| match e {
                    Event::Sfx { id, vol } => Some(format!("play:{id}:{vol}")),
                    Event::SfxStop { id } => Some(format!("stop:{id}")),
                    Event::Song { id } => Some(format!("song:{id}")),
                    _ => None,
                })
                .collect();
            out.push(row(&s.world, sfx));
        }
    }
    out
}

fn compare(exp: &[Row], got: &[Row]) -> Option<(usize, Vec<&'static str>)> {
    for (k, e) in exp.iter().enumerate() {
        let Some(g) = got.get(k) else { return Some((k + 1, vec!["missing"])) };
        let no_anim = g.raw == -1 && (e.raw <= 0 || e.rom == 0);
        let swapped = e.raw == -1 && e.rom != 0 && e.rom == g.rom;
        let mut bad = vec![];
        if e.state != g.state {
            bad.push("state");
        }
        if !no_anim && !swapped && e.raw != g.raw {
            bad.push("raw");
        }
        if !no_anim && e.frame != g.frame {
            bad.push("frame");
        }
        if e.dir != g.dir {
            bad.push("dir");
        }
        if e.x != g.x {
            bad.push("x");
        }
        if e.y != g.y {
            bad.push("y");
        }
        if e.hp != g.hp {
            bad.push("hp");
        }
        if e.ep != g.ep {
            bad.push("ep");
        }
        if e.form != g.form {
            bad.push("form");
        }
        if !no_anim && e.rom != g.rom {
            bad.push("rom");
        }
        if e.sfx.join(" ") != g.sfx.join(" ") {
            bad.push("sfx");
        }
        if e.rng.is_some() && e.rng != g.rng {
            bad.push("rng");
        }
        if !bad.is_empty() {
            return Some((k + 1, bad));
        }
    }
    None
}

#[test]
fn lockstep_vs_emulator() {
    let dir = battle_data("buu_lockstep");
    let filter = std::env::var("LOCKSTEP").ok();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let (mut pass, mut fail, mut skipped) = (Vec::new(), Vec::new(), 0);
    let mut frames = 0usize;
    for f in files {
        let j: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        for t in j["traces"].as_array().unwrap() {
            let name = t["name"].as_str().unwrap().to_string();
            if let Some(fl) = &filter {
                if !name.contains(fl.as_str()) {
                    continue;
                }
            } else if known(&name) {
                skipped += 1;
                continue;
            }
            // enemy scenarios (added after the player harness) are covered by their own test
            if !t["scenario"]["enemy"].is_null() {
                continue;
            }
            let exp = expand(t);
            let got = replay(t);
            frames += exp.len();
            match compare(&exp, &got) {
                None => pass.push(name),
                Some((fr, fields)) => {
                    if filter.is_some() {
                        eprintln!("{name}: first divergence at frame {fr} ({})", fields.join(", "));
                        for k in fr.saturating_sub(4)..(fr + 1).min(exp.len()) {
                            eprintln!("  {:5} emu {:?}\n        sim {:?}", k + 1, exp[k], got.get(k));
                        }
                    }
                    fail.push(format!("{name}: frame {fr} ({})", fields.join(", ")));
                }
            }
        }
    }
    eprintln!("lockstep: {} pass, {} fail, {} known-skipped, {frames} frames compared", pass.len(), fail.len(), skipped);
    for f in fail.iter().take(40) {
        eprintln!("  FAIL {f}");
    }
    if let Ok(p) = std::env::var("LOCKSTEP_EXPECT") {
        let want: std::collections::BTreeSet<String> = std::fs::read_to_string(p).unwrap().lines().filter(|l| !l.is_empty()).map(String::from).collect();
        let got: std::collections::BTreeSet<String> = pass.iter().cloned().collect();
        let missing: Vec<_> = want.difference(&got).collect();
        assert!(missing.is_empty(), "reference passes these, wf-core does not: {missing:?}");
    }
    if filter.is_none() {
        assert!(fail.is_empty(), "{} lockstep failures", fail.len());
    }
}
