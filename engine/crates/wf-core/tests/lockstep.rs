//! Frame-by-frame lockstep against the emulator (data/battle/buu_lockstep/*.json, recorded by
//! tools/battle/lockstep/record.py; format tools/battle/LOCKSTEP.md): the same start state and inputs; state, raw
//! animation, frame, facing, 24.8 position, HP, EP, form, ROM frame record, RNG word, every sound call and - in the enemy
//! scenarios - the enemy's position / facing / HP and every ComputeDamage result must match on every frame. Lag frames
//! (the game logic did not run) are skipped as the emulator did.
//! `LOCKSTEP_EXPECT=<file of names>` checks the pass set against the reference simulator's; `LOCKSTEP=<substring>`
//! runs a subset (known divergences included) and prints the first divergence.
mod common;
use common::{battle_data, data};
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use wf_core::ai::{BehTree, P};
use wf_core::battle::{self, BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::GameData;
use wf_core::input::InputFrame;
use wf_core::player::{self, Partner};
use wf_core::session::Session;
use wf_core::world::{Body, EntityId, Event, InjectedHit, Injector, World};

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
    dmg: Vec<String>,
    e: Option<(i64, i64, i64, i64)>,
}

const FIGHTER_OF_CHAR: [&str; 8] = [
    "goku", "gohan", "goten", "trunks", "vegeta", "gotenks", "vegito", "gogeta",
];

/// scenarios whose divergence is understood and not modelled (the reference's KNOWN list)
fn known(name: &str) -> bool {
    [
        "f32_ssdrain",
        "f33_ssdrain",
        "f23_bump",
        "f23_move",
        "f23_punch",
        "f23_skill11",
        "f46_punch",
        "f14_skill20",
        "f14_ssdrain",
    ]
    .contains(&name)
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

fn expand(t: &Value) -> Vec<Row> {
    let states: Vec<String> = t["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    let mut out = Vec::new();
    for g in t["rows"].as_array().unwrap() {
        let g: Vec<i64> = g.as_array().unwrap().iter().map(i).collect();
        for k in 0..g[0] {
            out.push(Row {
                x: g[1] + g[3] * k,
                y: g[2] + g[4] * k,
                dir: g[5],
                raw: g[6],
                frame: g[7],
                state: states[g[8] as usize].clone(),
                hp: g[9],
                ep: g[10],
                form: g[11],
                rom: g[12],
                rng: g.get(13).copied(),
                sfx: vec![],
                dmg: vec![],
                e: None,
            });
        }
    }
    for s in t["sfx"].as_array().unwrap() {
        let a = s.as_array().unwrap();
        let f = i(&a[0]) as usize;
        let Some(r) = out.get_mut(f.wrapping_sub(1)) else {
            continue;
        };
        if a[1].as_str() == Some("dmg") {
            r.dmg.push(format!("{}:{}", i(&a[2]), i(&a[3])));
        } else {
            r.sfx.push(
                a[1..]
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .map(String::from)
                            .unwrap_or_else(|| x.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(":"),
            );
        }
    }
    if let Some(er) = t["erows"].as_array() {
        let mut k = 0;
        for g in er {
            let g: Vec<i64> = g.as_array().unwrap().iter().map(i).collect();
            for j in 0..g[0] {
                if let Some(r) = out.get_mut(k) {
                    r.e = Some((g[1] + g[3] * j, g[2] + g[4] * j, g[5], g[6]));
                }
                k += 1;
            }
        }
    }
    out
}

/// the scenarios' ROM patches applied to the decoded AI: the Majin Fighter's aggro test (IfPlayerWithin2 at ROM
/// 0x087043F8) r2 = -1 (never) unless the scenario wants aggro; a target dummy: state 0x14B's on-hit tree replaced by
/// the bare ApplyDamage node 0x086FDA00 (HP loss only, no reaction)
fn patched(aggro: bool, dummy: bool) -> Arc<GameData> {
    static CACHE: OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<(bool, bool), Arc<GameData>>>,
    > = OnceLock::new();
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
        let node = d
            .aux
            .values()
            .flat_map(|t| t.nodes.iter())
            .find(|n| n.addr == 0x086f_da00)
            .cloned();
        if let Some(mut n) = node {
            n.kids.clear();
            d.aux.insert(
                0x14b,
                Arc::new(BehTree {
                    state: 0x14b,
                    nodes: vec![n],
                }),
            );
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
    let d = patched(
        en.is_none_or(|e| e["aggro"].as_bool().unwrap_or(false)),
        en.is_some_and(|e| e["dummy"].as_bool() != Some(false)),
    );
    let scn_form = i(&t["scenario"]["form"]) as i32;
    let form = rec.get("form").map(i).unwrap_or(scn_form as i64) as i32;
    let ch = d.form(scn_form).unwrap().char_;
    let eq: Vec<i32> = rec["equip"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| i(x) as i32)
        .collect();
    let levels: Vec<i64> = rec["levels"].as_array().unwrap().iter().map(i).collect();
    let fighter = FighterSetup {
        fighter: if ch == 8 {
            format!("f{form}")
        } else {
            FIGHTER_OF_CHAR[ch as usize].to_string()
        },
        form,
        level: 60,
        equip: [eq[0], eq[1], eq[2], eq[3]],
        skill_level: *levels.first().unwrap_or(&0) as i32,
        ..Default::default()
    };
    let ety = ini
        .get("enemy")
        .filter(|v| !v.is_null())
        .map(|e| i(&e["type"]))
        .or_else(|| en.map(|e| i(&e["type"])))
        .unwrap_or(-1);
    let i2 = ini.get("enemy2").filter(|v| !v.is_null());
    let arena = t["scenario"]["arena"]
        .as_str()
        .unwrap_or(if en.is_some() { "forest" } else { "htc" })
        .to_string();
    let phase = match i2 {
        Some(e2) => format!("t{ety}+t{}", i(&e2["type"])),
        None => format!("t{ety}"),
    };
    let cfg = BattleConfig {
        arena,
        players: vec![fighter],
        phases: vec![Phase {
            enemy: phase,
            count: 1,
        }],
        rng: RngInit::State(0, 0),
        xp: false,
        spawn_near: None,
        arena_rules: false,
        even_level: None,
    };
    let mut s = Session::new(d, cfg);
    let w = &mut s.world;
    let pi = w.player_idx(0).unwrap();
    {
        let p = w.pl(pi);
        p.rec.hp = i(&rec["hp"]) as i32;
        p.rec.max_hp = i(&rec["maxHp"]) as i32;
        p.rec.ep = i(&rec["ep"]) as i32;
        p.rec.max_ep = i(&rec["maxEp"]) as i32;
        let base: Vec<i32> = rec["base"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| i(x) as i32)
            .collect();
        p.rec.str_ = base[0];
        p.rec.pow = base[1];
        p.rec.end = base[2];
        p.rec.spd = base[3];
        let skills: Vec<i32> = rec["skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| i(x) as i32)
            .collect();
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
    player::lockstep_init(
        w,
        pi,
        i(&ini["raw"]) as i32,
        i(&ini["frame"]) as i32,
        i(&ini["tick"]) as i32,
        ini["state"].as_str().unwrap_or("idle"),
    );
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
            let base: Vec<i32> = pt["base"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| i(x) as i32)
                .collect();
            p.partner = Some(Partner {
                form: i(&pt["form"]) as i32,
                hp: i(&pt["hp"]) as i32,
                max_hp: i(&pt["maxHp"]) as i32,
                ep: i(&pt["ep"]) as i32,
                max_ep: i(&pt["maxEp"]) as i32,
                base: [base[0], base[1], base[2], base[3]],
                skills: pt["skills"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| i(x) as i32)
                    .collect(),
                slot: i(&pt["slot"]) as usize,
                level0: pt["levels"]
                    .as_array()
                    .and_then(|l| l.first())
                    .map(i)
                    .unwrap_or(0) as i32,
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
    // the frame counter 0x03000F94 at frame 0: every f02_vs_enemy recording starts from the same setup, 9634
    w.g.fc0 = ini.get("fc").filter(|v| !v.is_null()).map(i).unwrap_or(
        if ini.get("enemy").is_some_and(|v| !v.is_null()) {
            9634
        } else {
            0
        },
    );
    // the recorded mark ages are relative to the frame counter (the AI's clock = fc0 + frame, `Globals::clock`)
    let fc0 = w.g.fc0;
    let mut ai_starts: Vec<(u32, EntityId, u16)> = vec![];
    let mut enemy = None;
    if let (Some(_), Some(ie)) = (en, ini.get("enemy").filter(|v| !v.is_null())) {
        // the recorded enemy: the fight's own (not a part it spawned; Worm Head's segments come before it in the list)
        if let Some(ei) = w
            .ents
            .iter()
            .position(|e| matches!(&e.body, Body::Enemy(x) if x.parent == 0))
        {
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
                if let Some(&k) =
                    e.a.sprite
                        .and_then(|s| dd.sprite(s))
                        .and_then(|s| s.by_raw.get(&raw))
                {
                    e.a.anim = Some(k);
                }
                e.ea.pos = player::AnimPos {
                    raw,
                    frame: ie.get("frame").map(i).unwrap_or(0) as i32,
                    tick: ie.get("tick").map(i).unwrap_or(0) as i32,
                };
            }
            if let Some(a) = ie.get("mark1Age").map(i).filter(|&a| a < 1_000_000) {
                e.mark1 = fc0 - a;
            }
            if let Some(a) = ie.get("mark2Age").map(i).filter(|&a| a < 1_000_000) {
                e.mark2 = fc0 - a;
            }
            enemy = Some(w.ents[ei].id);
            let scn = en.unwrap();
            // a real enemy starts its AI fresh at frame 1 (the emulator's injected GotoState(initial state))
            if scn["dummy"].as_bool() == Some(false) {
                let ty = w.en_ref(ei).ty;
                let st = scn
                    .get("state")
                    .filter(|v| !v.is_null())
                    .map(|v| i(v) as u16)
                    .unwrap_or_else(|| w.data.enemy(ty).unwrap().ai);
                ai_starts.push((1, w.ents[ei].id, st));
            }
            if let Some(e2i) = i2 {
                let t2 = i(&e2i["type"]) as i32;
                let me = w.ents[ei].id;
                if let Some(k) = w
                    .ents
                    .iter()
                    .position(|x| x.id != me && matches!(&x.body, Body::Enemy(e) if e.ty == t2))
                {
                    wf_core::enemy::unmark_self(w, k);
                    {
                        let e = w.en(k);
                        e.a.x = i(&e2i["x"]) as i32;
                        e.a.y = i(&e2i["y"]) as i32;
                    }
                    wf_core::enemy::mark_self(w, k);
                    {
                        let e = w.en(k);
                        e.a.facing = i(&e2i["dir"]) as u8;
                        e.hp1024 = i(&e2i["hp"]);
                    }
                    // the emulator keeps the partner in the inert state 0xC (root: the Idle node) until frame 2
                    let partner = scn.get("partner").filter(|v| !v.is_null());
                    if partner.is_some() {
                        wf_core::nodes::reset(w, k, 0xc);
                    }
                    let dd = Arc::clone(&w.data);
                    let e = w.en(k);
                    let raw = i(&e2i["raw"]) as i32;
                    if raw > 0 {
                        if let Some(&a) =
                            e.a.sprite
                                .and_then(|s| dd.sprite(s))
                                .and_then(|s| s.by_raw.get(&raw))
                        {
                            e.a.anim = Some(a);
                            e.ea.pos = player::AnimPos {
                                raw,
                                frame: e2i.get("frame").map(i).unwrap_or(0) as i32,
                                tick: e2i.get("tick").map(i).unwrap_or(0) as i32,
                            };
                        }
                    }
                    if let Some(a) = e2i.get("mark1Age").map(i).filter(|&a| a < 1_000_000) {
                        e.mark1 = fc0 - a;
                    }
                    if let Some(a) = e2i.get("mark2Age").map(i).filter(|&a| a < 1_000_000) {
                        e.mark2 = fc0 - a;
                    }
                    if let Some(pt) = partner {
                        let ty = w.en_ref(k).ty;
                        let st = pt
                            .get("state")
                            .filter(|v| !v.is_null())
                            .map(|v| i(v) as u16)
                            .unwrap_or_else(|| w.data.enemy(ty).unwrap().ai);
                        ai_starts.push((2, w.ents[k].id, st));
                    }
                }
            }
        }
    }
    w.rng.set_state([
        ini["rng"][0].as_u64().unwrap() as u32,
        ini["rng"][1].as_u64().unwrap() as u32,
    ]);
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
    let injector = w.add(Body::Injector(Box::new(Injector {
        hits,
        player: pid,
        now: 0,
        ai: ai_starts,
    })));
    Setup { s, enemy, injector }
}

fn row(w: &World, sfx: Vec<String>, dmg: Vec<String>, enemy: Option<EntityId>) -> Row {
    let pi = w.player_idx(0).unwrap();
    let p = w.pl_ref(pi);
    let (state, raw, frame, rom) = player::lockstep_view(w, pi);
    let e = enemy
        .and_then(|id| w.enemy_by_id(id))
        .map(|(_, e)| (e.a.x as i64, e.a.y as i64, e.a.facing as i64, e.hp1024));
    // the eaten player: the recorder finds no player object (position / facing 0)
    let (x, y, dir) = if p.gone {
        (0, 0, 0)
    } else {
        (p.a.x as i64, p.a.y as i64, p.a.facing as i64)
    };
    Row {
        x,
        y,
        dir,
        raw: raw as i64,
        frame: frame as i64,
        state,
        hp: p.hp() as i64,
        ep: p.ep() as i64,
        form: p.form as i64,
        rom: rom as i64,
        rng: Some(w.rng.a as i64),
        sfx,
        dmg,
        e,
    }
}

fn replay(t: &Value) -> Vec<Row> {
    let Setup {
        mut s,
        enemy,
        injector,
    } = setup(t);
    let lag: std::collections::BTreeSet<u32> = t["lag"]
        .as_array()
        .map(|a| a.iter().map(|x| i(x) as u32).collect())
        .unwrap_or_default();
    let pid = s.world.players[0];
    // the last known enemy row (an enemy removed from the world keeps its last values in the reference)
    let mut last_e = None;
    let mut out = Vec::new();
    let mut prev = 0u16;
    let mut f = 0u32;
    for seg in t["input"].as_array().unwrap() {
        let keys = i(&seg[0]) as u16;
        for _ in 0..i(&seg[1]) {
            f += 1;
            if lag.contains(&f) {
                let mut r = row(&s.world, vec![], vec![], enemy);
                if r.e.is_none() {
                    r.e = last_e;
                }
                out.push(r);
                continue;
            }
            if let Some(k) = s.world.find(injector) {
                if let Body::Injector(j) = &mut s.world.ents[k].body {
                    j.now = f;
                }
            }
            let mut inp = [InputFrame::default(); 4];
            inp[0] = InputFrame::from_held(keys, prev);
            prev = keys;
            let ev = s.step_frames(inp).to_vec();
            let mut sfx = vec![];
            let mut dmg = vec![];
            // the logged damage is ComputeDamage's before the block factor
            let mut roll = None;
            for e in &ev {
                match e {
                    Event::Sfx { id, vol } => sfx.push(format!("play:{id}:{vol}")),
                    Event::SfxStop { id } => sfx.push(format!("stop:{id}")),
                    Event::SfxStopMatch { id } => sfx.push(format!("stopmatch:{id}")),
                    Event::SfxStopAll => sfx.push("stopall".into()),
                    Event::Song { id } => sfx.push(format!("song:{id}")),
                    Event::DamageRoll { dmg: d, .. } => roll = Some(*d),
                    Event::Damage { target, dmg: d, .. } => {
                        let d = roll.take().unwrap_or(*d);
                        dmg.push(format!(
                            "{}:{d}",
                            if *target == pid {
                                0
                            } else if Some(*target) == enemy {
                                1
                            } else {
                                -1
                            }
                        ))
                    }
                    _ => {}
                }
            }
            if let Ok(range) = std::env::var("LOCKSTEP_TRACE") {
                // debugging: per frame in FROM-TO the RNG calls and every enemy's AI stack / animation
                let (a, b) = range
                    .split_once('-')
                    .map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0)))
                    .unwrap_or((0, 0));
                if f >= a && f <= b {
                    let w = &s.world;
                    let mut line = format!("{f} rng#{} {:08x}", w.rng.calls, w.rng.a);
                    for k in 0..w.ents.len() {
                        if let Body::Enemy(e) = &w.ents[k].body {
                            line += &format!(
                                " | e{} t{:x} {:x?} raw{} f{} t{} rule{} {},{} d{}",
                                w.ents[k].id,
                                e.ty,
                                e.ai.states(),
                                e.ea.pos.raw,
                                e.ea.pos.frame,
                                e.ea.pos.tick,
                                e.ea.rule,
                                e.a.x,
                                e.a.y,
                                e.a.facing
                            );
                        } else {
                            line += &format!(" | {}", w.ents[k].kind());
                        }
                    }
                    eprintln!("{line}");
                }
            }
            // (diagnostics: the coverage report's recorder, off without WF_COVERAGE - every row is compared)
            wf_core::coverage::commit_world(&s.world);
            let mut r = row(&s.world, sfx, dmg, enemy);
            if r.e.is_none() {
                r.e = last_e;
            }
            last_e = r.e;
            out.push(r);
        }
    }
    out
}

fn compare(exp: &[Row], got: &[Row]) -> Option<(usize, Vec<&'static str>)> {
    for (k, e) in exp.iter().enumerate() {
        // game over: at KO + 440 the game frees the player object (vtable 0, no frame record); the rest is the
        // game-over screen
        if e.state == "vt_00000000" && e.rom == 0 && e.hp == 0 && k > 0 && exp[k - 1].hp == 0 {
            break;
        }
        let Some(g) = got.get(k) else {
            return Some((k + 1, vec!["missing"]));
        };
        let no_anim = g.raw == -1 && (e.raw <= 0 || e.rom == 0);
        let swapped = e.raw == -1 && e.rom != 0 && e.rom == g.rom;
        let mut bad = vec![];
        let mut chk = |c: bool, n: &'static str| {
            if !c {
                bad.push(n)
            }
        };
        chk(e.state == g.state, "state");
        chk(no_anim || swapped || e.raw == g.raw, "raw");
        chk(no_anim || e.frame == g.frame, "frame");
        chk(e.dir == g.dir, "dir");
        chk(e.x == g.x, "x");
        chk(e.y == g.y, "y");
        chk(e.hp == g.hp, "hp");
        chk(e.ep == g.ep, "ep");
        chk(e.form == g.form, "form");
        chk(no_anim || e.rom == g.rom, "rom");
        chk(e.sfx.join(" ") == g.sfx.join(" "), "sfx");
        if e.e.is_some() {
            let mut a = e.dmg.clone();
            let mut b = g.dmg.clone();
            a.sort();
            b.sort();
            chk(a == b, "dmg");
            chk(e.e == g.e, "enemy");
        }
        chk(e.rng.is_none() || e.rng == g.rng, "rng");
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
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
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
            wf_core::coverage::context(&name);
            let exp = expand(t);
            let got = replay(t);
            frames += exp.len();
            match compare(&exp, &got) {
                None => pass.push(name),
                Some((fr, fields)) => {
                    if filter.is_some() {
                        eprintln!(
                            "{name}: first divergence at frame {fr} ({})",
                            fields.join(", ")
                        );
                        for k in fr.saturating_sub(4)..(fr + 1).min(exp.len()) {
                            eprintln!(
                                "  {:5} emu {:?}\n        sim {:?}",
                                k + 1,
                                exp[k],
                                got.get(k)
                            );
                        }
                    }
                    fail.push(format!("{name}: frame {fr} ({})", fields.join(", ")));
                }
            }
        }
    }
    eprintln!(
        "lockstep: {} pass, {} fail, {} known-skipped, {frames} frames compared",
        pass.len(),
        fail.len(),
        skipped
    );
    for f in fail.iter().take(60) {
        eprintln!("  FAIL {f}");
    }
    if let Ok(p) = std::env::var("LOCKSTEP_EXPECT") {
        let want: std::collections::BTreeSet<String> = std::fs::read_to_string(p)
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        let got: std::collections::BTreeSet<String> = pass.iter().cloned().collect();
        let missing: Vec<_> = want.difference(&got).collect();
        assert!(
            missing.is_empty(),
            "reference passes these, wf-core does not: {missing:?}"
        );
    }
    if filter.is_none() {
        assert!(fail.is_empty(), "{} lockstep failures", fail.len());
    }
}
