//! Sync test over every emulator lockstep scenario (data/battle/buu_lockstep/*.json): the scenario's start state
//! (setup copied from wf-core's tests/lockstep.rs) and its recorded inputs, run through `SyncTestSession` with a
//! rollback of 8 frames every frame (every 3rd scenario through the serialized snapshot). Lag frames are not
//! replayed (this checks rollback determinism, not emulator parity). Needs the ROM (or WF_CONTENT).
mod common;
use common::{battle_data, data};
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use wf_core::ai::{BehTree, P};
use wf_core::battle::{self, BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::GameData;
use wf_core::player::{self, Partner};
use wf_core::session::Session;
use wf_core::world::{Body, EntityId, InjectedHit, Injector};
use wf_net::core_game::CoreGame;
use wf_net::synctest::SyncTestSession;

const FIGHTER_OF_CHAR: [&str; 8] = ["goku", "gohan", "goten", "trunks", "vegeta", "gotenks", "vegito", "gogeta"];

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

/// the scenarios' ROM patches applied to the decoded AI: the Majin Fighter's aggro test (IfPlayerWithin2 at ROM
/// 0x087043F8) r2 = -1 (never) unless the scenario wants aggro; a target dummy: state 0x14B's on-hit tree replaced by
/// the bare ApplyDamage node 0x086FDA00 (HP loss only, no reaction)
fn patched(aggro: bool, dummy: bool) -> Arc<GameData> {
    static CACHE: OnceLock<std::sync::Mutex<std::collections::BTreeMap<(bool, bool), Arc<GameData>>>> = OnceLock::new();
    let c = CACHE.get_or_init(Default::default);
    if let Some(d) = c.lock().unwrap().get(&(aggro, dummy)) {
        return d.clone();
    }
    let mut d = (*data()).clone();
    if !aggro {
        for t in d.states.values_mut() {
            if t.nodes.iter().any(|n| n.addr == 0x0870_43f8) {
                let mut nt: BehTree = (**t).clone();
                for n in nt.nodes.iter_mut().filter(|n| n.addr == 0x0870_43f8) {
                    n.p.set(P::R2, -1);
                }
                *t = Arc::new(nt);
            }
        }
    }
    if dummy {
        let node = d.aux.values().flat_map(|t| t.nodes.iter()).find(|n| n.addr == 0x086f_da00).cloned();
        if let Some(mut n) = node {
            n.kids.clear();
            d.aux.insert(0x14b, Arc::new(BehTree { state: 0x14b, nodes: vec![n] }));
        }
    }
    let d = Arc::new(d);
    c.lock().unwrap().insert((aggro, dummy), d.clone());
    d
}

struct Setup {
    s: Session,
    enemy: Option<EntityId>,
    injector: EntityId,
}

fn setup(t: &Value) -> Setup {
    let ini = &t["init"];
    let rec = &ini["rec"];
    let en = t["scenario"].get("enemy").filter(|v| !v.is_null());
    let d = patched(en.is_none_or(|e| e["aggro"].as_bool().unwrap_or(false)), en.is_some_and(|e| e["dummy"].as_bool() != Some(false)));
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
    let ety = ini.get("enemy").filter(|v| !v.is_null()).map(|e| i(&e["type"])).or_else(|| en.map(|e| i(&e["type"]))).unwrap_or(-1);
    let arena = t["scenario"]["arena"].as_str().unwrap_or(if en.is_some() { "forest" } else { "htc" }).to_string();
    let cfg = BattleConfig { arena, players: vec![fighter], phases: vec![Phase { enemy: format!("t{ety}"), count: 1 }], rng: RngInit::State(0, 0), xp: false, spawn_near: None, arena_rules: false, even_level: None };
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
    player::lockstep_init(w, pi, i(&ini["raw"]) as i32, i(&ini["frame"]) as i32, i(&ini["tick"]) as i32, ini["state"].as_str().unwrap_or("idle"));
    let obj = &ini["obj"];
    let has = |k: &str| obj.get(k).filter(|v| !v.is_null()).cloned();
    let (ss, ss3, ctl) = {
        let f = w.data.form(w.pl_ref(pi).form).unwrap();
        (f.ss, f.ss3, f.controller)
    };
    let story = w.pl_ref(pi).story_form(w);
    {
        let p = w.pl(pi);
        if let (true, Some(v)) = (story, has("regen")) {
            p.story_cnt = i(&v) as i32;
        }
        if let (true, Some(v)) = (ctl == 0x3001b64, has("ctl10")) {
            p.fus_cnt = i(&v) as i32;
        }
        if let Some(pt) = ini.get("partner").filter(|v| !v.is_null()) {
            let base: Vec<i32> = pt["base"].as_array().unwrap().iter().map(|x| i(x) as i32).collect();
            p.partner = Some(Partner {
                form: i(&pt["form"]) as i32,
                hp: i(&pt["hp"]) as i32,
                max_hp: i(&pt["maxHp"]) as i32,
                ep: i(&pt["ep"]) as i32,
                max_ep: i(&pt["maxEp"]) as i32,
                base: [base[0], base[1], base[2], base[3]],
                skills: pt["skills"].as_array().unwrap().iter().map(|x| i(x) as i32).collect(),
                slot: i(&pt["slot"]) as usize,
                level0: pt["levels"].as_array().and_then(|l| l.first()).map(i).unwrap_or(0) as i32,
                ss_gauge: i(&pt["ssGauge"]) as i32,
                ss3_gauge: i(&pt["ss3Gauge"]),
                cooldown: i(&pt["cooldown"]) as i32,
            });
        }
        if let Some(v) = has("hittable") {
            p.a.hittable = i(&v) != 0;
        }
        if let Some(v) = has("regen") {
            p.regen_timer = (i(&v) & 0xffff) as i32;
        }
        if let (true, Some(v)) = (ss || ss3, has("ssacc")) {
            p.ss_acc = (i(&v) & 0xffff) as i32;
        }
        if let Some(v) = has("cooldown") {
            p.ss_cooldown = i(&v) as i32;
        }
        if let Some(v) = has("idlecnt") {
            p.s_cnt = i(&v) as i32;
        }
    }
    if let Some(c) = has("camera") {
        battle::set_camera(w, 0, i(&c[0]) as i32, i(&c[1]) as i32);
    }
    if let Some(v) = has("beepAge") {
        w.g.last_beep = -i(&v);
    }
    let mut enemy = None;
    if let (Some(_), Some(ie)) = (en, ini.get("enemy").filter(|v| !v.is_null())) {
        if let Some(ei) = w.ents.iter().position(|e| matches!(e.body, Body::Enemy(_))) {
            wf_core::enemy::unmark_self(w, ei);
            {
                let e = w.en(ei);
                e.a.x = i(&ie["x"]) as i32;
                e.a.y = i(&ie["y"]) as i32;
            }
            wf_core::enemy::mark_self(w, ei);
            let dd = Arc::clone(&w.data);
            let e = w.en(ei);
            e.a.facing = i(&ie["dir"]) as u8;
            e.hp1024 = i(&ie["hp"]);
            let raw = ie.get("raw").map(i).unwrap_or(0) as i32;
            if raw > 0 {
                if let Some(&k) = e.a.sprite.and_then(|s| dd.sprite(s)).and_then(|s| s.by_raw.get(&raw)) {
                    e.a.anim = Some(k);
                }
                e.ea.pos = player::AnimPos { raw, frame: ie.get("frame").map(i).unwrap_or(0) as i32, tick: ie.get("tick").map(i).unwrap_or(0) as i32 };
            }
            if let Some(a) = ie.get("mark1Age").map(i).filter(|&a| a < 1_000_000) {
                e.mark1 = -a;
            }
            if let Some(a) = ie.get("mark2Age").map(i).filter(|&a| a < 1_000_000) {
                e.mark2 = -a;
            }
            enemy = Some(w.ents[ei].id);
        }
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
    let injector = w.add(Body::Injector(Box::new(Injector { hits, player: pid, now: 0, ai: vec![] })));
    Setup { s, enemy, injector }
}


#[test]
fn synctest_all_lockstep_scenarios() {
    let dir = battle_data("buu_lockstep");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let (mut n, mut frames, mut resim) = (0usize, 0u64, 0u64);
    let mut fails = Vec::new();
    let t0 = std::time::Instant::now();
    for f in files {
        let j: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        for t in j["traces"].as_array().unwrap() {
            let name = t["name"].as_str().unwrap().to_string();
            let Setup { s, .. } = setup(t);
            let mut g = CoreGame::new(s);
            g.collect_events = false;
            let mut st = SyncTestSession::new(g, 8);
            st.bytes = n % 3 == 0;
            n += 1;
            'run: for seg in t["input"].as_array().unwrap() {
                let keys = i(&seg[0]) as u16;
                for _ in 0..i(&seg[1]) {
                    if let Err(e) = st.advance(&[keys]) {
                        fails.push(format!("{name}: {e}"));
                        break 'run;
                    }
                    frames += 1;
                }
            }
            resim += st.resim_frames;
        }
    }
    eprintln!("synctest over {n} lockstep scenarios: {frames} frames, {resim} re-simulated, {} failures, {:.1} s", fails.len(), t0.elapsed().as_secs_f64());
    for f in &fails {
        eprintln!("  FAIL {f}");
    }
    assert!(n > 600, "scenarios: {n}");
    assert!(fails.is_empty());
}
