//! Opponent visual audit - the comparer (headless): the emulator's per-frame record of an enemy scenario
//! (tools/battle/lockstep/visual.py: a lockstep trace + `vis` + the screen of every frame) against our engine's battle
//! sim on the same arena (8-3, arena rules OFF - the faithful core), frame by frame.
//!
//!   cargo run --release -p wf-app --example opp_audit -- work/ub/oa/oa_t44_*.json
//!
//! Per scenario one line: frames compared; the first diverging frame of each category -
//!   player (the lockstep row: state / anim / position / HP / EP / sounds / RNG), dmg (every ComputeDamage), enemy
//!   (the scenario's enemy: x / y / facing / HP), anim (its raw / frame / tick), ai (its AI state), others (the other
//!   live enemies: parts / summons - count, type, position, anim), pix (the picture: OBJ pixels; bg = a background
//!   pixel differs)
//! and the count of frames whose OBJ pixels differ. `OA_OUT=dir` (default work/ub/oa/out): NAME.json (per frame the
//! differing fields, emulator vs ours) and NAME_fNNN.png triptychs (game | ours | diff) of the first `OA_PNG` (6)
//! frames with OBJ pixel differences. The trace setup below is wf-core/tests/lockstep.rs's (copied: keep it in step).
//! Usage and the scenario format: tools/battle/LOCKSTEP.md, "Visual audit".
#![allow(dead_code, clippy::all)]

use serde_json::Value;
use std::sync::{Arc, OnceLock};
use wf_core::ai::{BehTree, P};
use wf_core::battle::{self, BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::GameData;
use wf_core::input::InputFrame;
use wf_core::player::{self, Partner};
use wf_core::session::Session;
use wf_core::world::{Body, EntityId, Event, InjectedHit, Injector, World};

static DATA: OnceLock<Arc<GameData>> = OnceLock::new();
fn data() -> Arc<GameData> {
    DATA.get().expect("data").clone()
}

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
    static CACHE: std::sync::OnceLock<
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


// ---- the visual audit (above: the lockstep test's trace setup, copied) ------------------------------------------------

const W: usize = 240;
const H: usize = 160;

fn read_fb(p: &std::path::Path) -> Option<Vec<Vec<u32>>> {
    use std::io::{BufRead, Read};
    let f = std::fs::File::open(p).ok()?;
    let mut r = std::io::BufReader::new(f);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let head: Value = serde_json::from_str(&line).ok()?;
    let sizes: Vec<usize> = head["sizes"].as_array()?.iter().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
    let mut out = vec![];
    for &n in &sizes {
        let mut z = vec![0u8; n];
        r.read_exact(&mut z).ok()?;
        let b = miniz_oxide::inflate::decompress_to_vec_zlib(&z).ok()?;
        out.push(b.chunks(3).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect());
    }
    Some(out)
}

fn save_png(p: &std::path::Path, imgs: &[&[u32]]) {
    let (w, h) = (W * imgs.len(), H);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for im in imgs {
            for x in 0..W {
                let c = im[y * W + x];
                rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    let f = std::fs::File::create(p).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

/// one enemy as the recorder lists it: [type, x, y, dir, raw, frame, tick, hp1024, ai, z, hittable]
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct En {
    ty: i64,
    x: i64,
    y: i64,
    dir: i64,
    raw: i64,
    frame: i64,
    tick: i64,
    hp: i64,
    ai: i64,
    z: i64,
}

fn emu_en(v: &Value) -> (En, i64) {
    let g = |k: usize| v[k].as_i64().unwrap_or(-1);
    (En { ty: g(0), x: g(1), y: g(2), dir: g(3), raw: g(4), frame: g(5), tick: g(6), hp: g(7), ai: g(8), z: g(9) }, g(11))
}

fn our_en(w: &World, k: usize) -> Option<En> {
    let Body::Enemy(e) = &w.ents[k].body else { return None };
    Some(En {
        ty: e.ty as i64,
        x: e.a.x as i64,
        y: e.a.y as i64,
        dir: e.a.facing as i64,
        // the recorder names an animation by its first slot (raws sharing ROM records: Spopovich's 5 / 6 / 7)
        raw: e.a.sprite.and_then(|s| w.data.sprite(s)).and_then(|s| s.anim_raw(e.ea.pos.raw)).map(|a| a.canon_raw).unwrap_or(e.ea.pos.raw) as i64,
        frame: e.ea.pos.frame as i64,
        tick: e.ea.pos.tick as i64,
        hp: e.hp1024,
        ai: e.ai.top_state().map(|s| s as i64).unwrap_or(-1),
        z: e.a.z as i64,
    })
}

/// our live enemies in list order (the world's enemy list, without the removed / parked ones)
fn our_live(w: &World) -> Vec<(EntityId, En)> {
    let mut v: Vec<(EntityId, En)> = w
        .enemies
        .iter()
        .filter_map(|&id| {
            let (k, e) = w.enemy_by_id(id)?;
            if w.ents[k].removed || (e.a.x >> 8) >= 0x6c00 {
                return None;
            }
            our_en(w, k).map(|n| (id, n))
        })
        .collect();
    // Janemba's summoned hand (type 0x36): an enemy object in the game's list, its own body here (no AI / anim index)
    for e in w.ents.iter().filter(|e| !e.removed) {
        if let Body::Hand(h) = &e.body {
            v.push((e.id, En { ty: 0x36, x: h.a.x as i64, y: h.a.y as i64, dir: h.a.facing as i64, raw: -2, frame: -2, tick: 0, hp: 0, ai: -1, z: h.a.z as i64 }));
        }
    }
    v
}

#[derive(Default)]
struct First {
    at: std::collections::BTreeMap<&'static str, usize>,
}
impl First {
    fn hit(&mut self, cat: &'static str, f: usize) {
        self.at.entry(cat).or_insert(f);
    }
}

const CATS: [&str; 8] = ["player", "dmg", "enemy", "anim", "ai", "others", "pix", "bg"];

fn audit(path: &std::path::Path, packs: &wf_app::wf_content::Packs, out_dir: &std::path::Path, max_png: usize, tile_hint: &mut Option<u32>) -> String {
    let t: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let name = t["name"].as_str().unwrap_or("?").to_string();
    let Some(game) = read_fb(&path.with_extension("fb")) else {
        return format!("{name}: no screen record (.fb)");
    };
    let exp = expand(&t);
    let vis = t["vis"].as_array().cloned().unwrap_or_default();
    let Setup { mut s, enemy, injector } = setup(&t);
    let lag: std::collections::BTreeSet<u32> = t["lag"].as_array().map(|a| a.iter().map(|x| i(x) as u32).collect()).unwrap_or_default();
    // the tracked enemy's emulator object (its pointer at frame 0)
    let ety = i(&t["init"]["enemy"]["type"]);
    let tracked = t["vis0"]["en"].as_array().and_then(|a| a.iter().map(emu_en).find(|(e, _)| e.ty == ety)).map(|(_, o)| o);
    let mut renderer = wf_render::Renderer::new();
    let mut presenter = wf_core::view::Presenter::default();
    let mut first = First::default();
    let mut obj_frames = 0usize;
    let mut pngs = 0usize;
    let mut log: Vec<Value> = vec![];
    let pid = s.world.players[0];
    let mut prev = 0u16;
    let mut last_e = None;
    let mut f = 0u32;
    // (the map's props: a battle world draws them itself now, `fieldview::map_props`)
    // (and the solid ones block the grids as the field's breakables do - `fieldobj` 0x0800C858: Southwest Forest 8-3's
    // stumps; oa_t50_idle f580, the Ninja Boss's align step into prop 0's hit box blocked, the node ends and he throws)
    {
        let w = &mut s.world;
        let mid = w.arena.map.split_once('-').and_then(|(a, b)| Some(format!("area_{:02}_{:03}", a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))).unwrap_or_default();
        let boxes: Vec<[i32; 4]> = w
            .data
            .field
            .as_ref()
            .and_then(|d| d.maps.get(&mid))
            .map(|md| md.props.iter().filter(|p| p.tile >> 24 != 0).filter_map(|p| p.hit_box).collect())
            .unwrap_or_default();
        if let Some(c) = w.col.as_mut() {
            for b in boxes {
                c.mark_both(b, 0);
            }
        }
    }
    // our presented view of each frame (rendered after the sim pass, once the tile clock is fitted)
    let snap = |w: &World, presenter: &wf_core::view::Presenter| -> wf_view::RenderView {
        let mut view = wf_view::RenderView::default();
        wf_core::view::render_view(w, &mut view);
        // (battle mode's see-through players behind scenery is a PC-only aid, not the game's picture)
        view.see_through.clear();
        presenter.apply(&mut view);
        view
    };
    let mut views: Vec<(usize, wf_view::RenderView)> = vec![(0, snap(&s.world, &presenter))];
    for seg in t["input"].as_array().unwrap() {
        let keys = i(&seg[0]) as u16;
        for _ in 0..i(&seg[1]) {
            f += 1;
            let fi = f as usize;
            // OA_RNG=f1,f2..: every generator call of those frames with its caller (stderr)
            let tr = std::env::var("OA_RNG").is_ok_and(|v| v.split(',').any(|x| x.parse() == Ok(fi)));
            wf_core::rng::TRACE.store(tr, std::sync::atomic::Ordering::Relaxed);
            if tr {
                eprintln!("f{fi} rng:");
            }
            if !lag.contains(&f) {
                if let Some(k) = s.world.find(injector) {
                    if let Body::Injector(j) = &mut s.world.ents[k].body {
                        j.now = f;
                    }
                }
                let mut inp = [InputFrame::default(); 4];
                inp[0] = InputFrame::from_held(keys, prev);
                prev = keys;
                presenter.before_step(&s.world);
                let ev = s.step_frames(inp).to_vec();
                // OA_SHOTS=f1,f2..: our shots after those frames (stderr)
                if std::env::var("OA_SHOTS").is_ok_and(|v| v.split(',').any(|x| x.parse() == Ok(fi))) {
                    for e in s.world.ents.iter().filter(|e| !e.removed) {
                        if let Body::Shot(sh) = &e.body {
                            eprintln!("f{fi} shot {} x {} y {} t {} dir {} owner {}", sh.kind_name(), sh.px(), sh.py(), sh.t, sh.dir, sh.owner);
                        }
                    }
                    let mut v = wf_view::RenderView::default();
                    wf_core::view::render_view(&s.world, &mut v);
                    eprintln!("f{fi} draw order: {:?}", v.sprites.iter().map(|d| (d.entity, d.sheet.as_str(), d.x, d.y)).collect::<Vec<_>>());
                }
                let mut sfx = vec![];
                let mut dmg = vec![];
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
                            dmg.push(format!("{}:{d}", if *target == pid { 0 } else if Some(*target) == enemy { 1 } else { -1 }))
                        }
                        _ => {}
                    }
                }
                let mut r = row(&s.world, sfx, dmg, enemy);
                // (our enemy object is gone once it is dead - the game's stays listed at HP 0: the last place, HP 0)
                if r.e.is_none() {
                    r.e = last_e.map(|(x, y, d, _)| (x, y, d, 0));
                }
                last_e = r.e;
                // the player / damage / enemy position rows (the lockstep gate's comparison, this frame alone)
                if let Some(e) = exp.get(fi - 1) {
                    if let Some((_, bad)) = compare(std::slice::from_ref(e), std::slice::from_ref(&r)) {
                        for b in &bad {
                            let cat = match *b {
                                "dmg" => "dmg",
                                "enemy" => "enemy",
                                _ => "player",
                            };
                            first.hit(cat, fi);
                        }
                        log.push(serde_json::json!({"f": fi, "row": bad, "emu": format!("{e:?}"), "ours": format!("{r:?}")}));
                    }
                }
            }
            // the enemies: the tracked one's animation / AI, the others as a list
            if let Some(v) = vis.get(fi - 1) {
                let emu: Vec<(En, i64)> = v["en"].as_array().map(|a| a.iter().map(emu_en).collect()).unwrap_or_default();
                let ours = our_live(&s.world);
                // OA_DBG=f1,f2..: our shots at those frames (stderr)
                if std::env::var("OA_DBG").is_ok_and(|v| v.split(',').any(|x| x.parse() == Ok(fi))) {
                    if let Some((k, e)) = enemy.and_then(|id| s.world.enemy_by_id(id)) {
                        let (x, y, det) = (e.a.px(), e.a.py(), e.detached);
                        let fx = wf_core::enemy::fits(&mut s.world, k, x + 1, y);
                        eprintln!("f{fi} enemy ({x}, {y}) detached {det} fits(x+1) {fx} rng {} {} calls {}", s.world.rng.a, s.world.rng.b, s.world.rng.calls);
                        let e = s.world.en_ref(k);
                        eprintln!("f{fi} enemy ea rule {} pos {:?} t {} done {} anim {:?} ai {:?}", e.ea.rule, (e.ea.pos.raw, e.ea.pos.frame, e.ea.pos.tick), e.ea.t, e.ea.done, e.a.anim, e.ai.stack.iter().map(|r| r.state).collect::<Vec<_>>());
                    }
                    for e in s.world.ents.iter().filter(|e| !e.removed) {
                        if let Body::Shot(sh) = &e.body {
                            eprintln!("f{fi} shot {} ({}, {}) dir {} {:?}", e.id, sh.x >> 8, sh.y >> 8, sh.dir, sh.kind);
                        }
                    }
                }
                // (`OA_DBG=f`: our hands / parts at that frame on stderr)
                if std::env::var("OA_DBG").ok().and_then(|v| v.parse::<usize>().ok()) == Some(fi) {
                    for k in 0..s.world.ents.len() {
                        if let wf_core::world::Body::Hand(h) = &s.world.ents[k].body {
                            eprintln!("f{fi} hand {k} anim {:?} t {} facing {} cel {:?} xy {} {}", h.a.anim, h.a.anim_t, h.a.facing, wf_core::view::actor_cel(&s.world, k), h.a.px(), h.a.py());
                        }
                    }
                }
                let me = emu.iter().find(|(_, o)| Some(*o) == tracked).map(|(e, _)| e.clone());
                let mo = enemy.and_then(|id| ours.iter().find(|(i2, _)| *i2 == id)).map(|(_, e)| e.clone());
                match (&me, &mo) {
                    (Some(a), Some(b)) => {
                        // two raw slots of one def can hold the same animation (Ghost 1 = 2): the recorder names the first
                        let spr = enemy.and_then(|id| s.world.enemy_by_id(id)).and_then(|(_, e)| e.a.sprite).and_then(|sp| s.world.data.sprite(sp));
                        let same_raw = a.raw == b.raw || spr.and_then(|sp| sp.anim_raw(b.raw as i32)).is_some_and(|an| an.canon_raw == a.raw as i32);
                        // (a recorded frame sequence - rule `seq` - keeps the cels, not the state object's tick)
                        let seq = enemy.and_then(|id| s.world.enemy_by_id(id)).is_some_and(|(_, e)| s.world.data.anim.rules[e.ea.rule as usize].kind == wf_core::eanim::RuleKind::Seq);
                        if !same_raw || a.frame != b.frame || (a.tick != b.tick && !seq) || a.z != b.z {
                            first.hit("anim", fi);
                            log.push(serde_json::json!({"f": fi, "anim": {"emu": [a.raw, a.frame, a.tick, a.z], "ours": [b.raw, b.frame, b.tick, b.z]}}));
                        }
                        if a.ai != b.ai {
                            first.hit("ai", fi);
                            log.push(serde_json::json!({"f": fi, "ai": {"emu": a.ai, "ours": b.ai}}));
                        }
                    }
                    (None, None) => {}
                    _ => {
                        first.hit("anim", fi);
                        log.push(serde_json::json!({"f": fi, "alive": {"emu": me.is_some(), "ours": mo.is_some()}}));
                    }
                }
                let eo: Vec<&En> = emu.iter().filter(|(_, o)| Some(*o) != tracked).map(|(e, _)| e).collect();
                let oo: Vec<&En> = ours.iter().filter(|(i2, _)| Some(*i2) != enemy).map(|(_, e)| e).collect();
                // (Janemba's hand: type and place only - ours has no anim index for it)
                let key = |e: &En| if e.ty == 0x36 { (e.ty, e.x >> 8, e.y >> 8, 0, 0, 0) } else { (e.ty, e.x >> 8, e.y >> 8, e.raw, e.frame, e.dir) };
                let mut ka: Vec<_> = eo.iter().map(|e| key(e)).collect();
                let mut kb: Vec<_> = oo.iter().map(|e| key(e)).collect();
                ka.sort();
                kb.sort();
                if ka != kb {
                    first.hit("others", fi);
                    log.push(serde_json::json!({"f": fi, "others": {"emu": ka, "ours": kb}}));
                }
            }
            views.push((fi, snap(&s.world, &presenter)));
            // game over (as the lockstep gate): the player object freed - the rest is the game-over screen
            if exp.get(fi - 1).is_some_and(|e| e.state == "vt_00000000" && e.rom == 0 && e.hp == 0) {
                break;
            }
        }
    }
    // the pictures: -> ours and the screen boxes around our sprites' anchors and the emulator's enemies (where an OBJ
    // difference can be); `tile_off` = the map's tile-animation clock ahead of ours by that many frames (the
    // emulator's field has run since its map loaded) - fitted on the background outside those boxes
    let render = |renderer: &mut wf_render::Renderer, view: &wf_view::RenderView, f: usize, tile_off: u32| -> (Vec<u32>, Vec<[i32; 4]>) {
        let mut view = view.clone();
        if let Some(m) = view.map.as_mut() {
            m.tile_frame = Some(m.tile_frame.unwrap_or(m.anim_frame).wrapping_add(tile_off));
        }
        let cam = view.cameras.first().copied().unwrap_or_default();
        if std::env::var_os("OA_DN").is_some() {
            eprintln!("f{f} cam {:?} dn {:?} hud {:?}", (cam.x, cam.y), view.damage_numbers.iter().map(|d| (d.x, d.y, d.value, d.crit)).collect::<Vec<_>>(), view.hud.iter().map(|h| (h.hp, h.ep)).collect::<Vec<_>>());
        }
        // OA_SPR=f0-f1: the sprites drawn at those frames (entity, sheet, frame, x, y, priority), in the view's order
        if let Some((a, b)) = std::env::var("OA_SPR").ok().and_then(|v| v.split_once('-').and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))) {
            if (a..=b).contains(&f) {
                for s in &view.sprites {
                    eprintln!("spr f{f} e{} {} {:?} ({}, {}) p{}", s.entity, s.sheet, s.frame, s.x, s.y, s.priority);
                }
            }
        }
        let mut boxes: Vec<[i32; 4]> = view.sprites.iter().map(|s| [s.x - cam.x - 48, s.y - cam.y - 80, s.x - cam.x + 48, s.y - cam.y + 24]).collect();
        if let Some(v) = if f == 0 { t.get("vis0") } else { vis.get(f - 1) } {
            let (cx, cy) = (v["cam"][0].as_i64().unwrap_or(0) as i32 >> 10, v["cam"][1].as_i64().unwrap_or(0) as i32 >> 10);
            for e in v["en"].as_array().into_iter().flatten() {
                let (x, y) = ((e[1].as_i64().unwrap_or(0) >> 8) as i32 - cx, (e[2].as_i64().unwrap_or(0) >> 8) as i32 - cy);
                boxes.push([x - 48, y - 80, x + 64, y + 24]);
            }
        }
        (renderer.render(packs, &view, 0, W, H).to_vec(), boxes)
    };
    let inside = |boxes: &[[i32; 4]], k: usize| {
        let (x, y) = ((k % W) as i32, (k / W) as i32);
        boxes.iter().any(|b| x >= b[0] && x < b[2] && y >= b[1] && y < b[3])
    };
    let sample: Vec<&(usize, wf_view::RenderView)> = views.iter().filter(|(f, _)| f % 16 == 2 && *f < 200).collect();
    // (coarse: every 16th frame over the offsets; then the best candidates on every frame up to 64)
    let fit_max: u32 = std::env::var("OA_FITMAX").ok().and_then(|v| v.parse().ok()).unwrap_or(1024);
    let mut score = |renderer: &mut wf_render::Renderer, frames: &[&(usize, wf_view::RenderView)], o: u32| -> usize {
        frames
            .iter()
            .filter_map(|(f, v)| {
                let g = game.get(*f)?;
                let (px, boxes) = render(renderer, v, *f, o);
                Some((0..W * H).filter(|&k| px[k] != g[k] && !inside(&boxes, k)).count())
            })
            .sum::<usize>()
    };
    let dense: Vec<&(usize, wf_view::RenderView)> = views.iter().filter(|(f, _)| (2..64).contains(f)).collect();
    // (the last scenario's offset first: the recordings share their start, so it usually fits)
    let tile_off = match *tile_hint {
        Some(o) if score(&mut renderer, &dense, o) == 0 => o,
        _ => {
            let coarse: Vec<usize> = (0..fit_max).map(|o| score(&mut renderer, &sample, o)).collect();
            let best = coarse.iter().min().copied().unwrap_or(0);
            let mut cands: Vec<(usize, u32)> = (0..fit_max).filter(|&o| coarse[o as usize] <= best).map(|o| (score(&mut renderer, &dense, o), o)).collect();
            cands.sort();
            if std::env::var_os("OA_FITDBG").is_some() {
                eprintln!("{name} fit coarse best {best} candidates {cands:?}");
            }
            cands.first().map(|c| c.1).unwrap_or(0)
        }
    };
    *tile_hint = Some(tile_off);
    // (frames 0 and 1 are not compared: the emulator's enemy waits off-screen until it is placed at frame 0 and its OBJ
    // shows one frame late - the recording's set-up, not the game)
    for (f, v) in views.iter().filter(|(f, _)| *f >= 2) {
        let f = *f;
        let Some(g) = game.get(f) else { continue };
        let (ours, boxes) = render(&mut renderer, v, f, tile_off);
        if &ours == g {
            // (`OA_PNGAT` frames are saved even when equal: a reference to look at)
            if std::env::var("OA_PNGAT").ok().is_some_and(|v| v.split(',').any(|x| x.trim().parse() == Ok(f))) {
                save_png(&out_dir.join(format!("{name}_f{f:03}.png")), &[g, &ours, &ours]);
            }
            continue;
        }
        // (`OA_NODN`: pixels around our damage numbers are not counted - the digits' drawing is audited apart)
        let dn: Vec<[i32; 4]> = if std::env::var_os("OA_NODN").is_some() {
            let cam = v.cameras.first().copied().unwrap_or_default();
            v.damage_numbers.iter().map(|d| [d.x - cam.x - 24, d.y - cam.y - 24, d.x - cam.x + 24, d.y - cam.y + 12]).collect()
        } else {
            vec![]
        };
        let (mut no, mut nb) = (0usize, 0usize);
        // (the OBJ differences' bounding box, for the log)
        let mut ob = [W as i32, H as i32, -1, -1];
        let mut diff = vec![0xFF00_0000u32; W * H];
        for k in 0..W * H {
            if ours[k] == g[k] || inside(&dn, k) {
                continue;
            }
            if inside(&boxes, k) {
                no += 1;
                let (x, y) = ((k % W) as i32, (k / W) as i32);
                ob = [ob[0].min(x), ob[1].min(y), ob[2].max(x), ob[3].max(y)];
                diff[k] = 0xFFFF_0000;
            } else {
                nb += 1;
                diff[k] = 0xFF00_80FF;
            }
        }
        if no > 0 {
            first.hit("pix", f);
            obj_frames += 1;
            // (`OA_PNGAT=f,f..`: those frames' triptychs too)
            let at = std::env::var("OA_PNGAT").ok().is_some_and(|v| v.split(',').any(|x| x.trim().parse() == Ok(f)));
            if pngs < max_png || at {
                pngs += 1;
                save_png(&out_dir.join(format!("{name}_f{f:03}.png")), &[g, &ours, &diff]);
            }
        }
        if nb > 0 {
            first.hit("bg", f);
        }
        log.push(serde_json::json!({"f": f, "pix_obj": no, "pix_bg": nb, "obj_box": ob}));
    }
    let n = f;
    let firsts: Vec<String> = CATS.iter().map(|c| format!("{c} {}", first.at.get(c).map(|v| v.to_string()).unwrap_or("-".into()))).collect();
    let summary = format!("{name}: {n} frames | {} | OBJ-diff frames {obj_frames}", firsts.join(" "));
    let j = serde_json::json!({"name": name, "frames": n, "tile_off": tile_off, "first": first.at, "obj_diff_frames": obj_frames, "log": log});
    std::fs::write(out_dir.join(format!("{name}.json")), serde_json::to_vec(&j).unwrap()).unwrap();
    summary
}

fn main() {
    use wf_app::wf_content::{DirSource, FileSource, Packs};
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = wf_rom::testutil::repo_root();
    let out_dir = std::env::var("OA_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| repo.join("work/ub/oa/out"));
    let max_png: usize = std::env::var("OA_PNG").ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    std::fs::create_dir_all(&out_dir).unwrap();
    // the segment harness's shared extraction (as examples/pixels.rs)
    let only = ["tables", "sprites", "effects", "portraits", "maps", "audio", "ui", "world", "items", "scripts"];
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    if !wf_extract::cache::fresh(&content, &only) {
        wf_extract::cache::ensure(&content, &repo.join("roms/buu.gba"), &only);
    }
    let packs = Packs::new(vec![Box::new(DirSource(content.clone())) as Box<dyn FileSource>]).unwrap();
    let d = wf_app::battle::load_data(&packs).expect("data");
    let _ = DATA.set(Arc::new((*d).clone()));
    let mut tile_hint = std::env::var("OA_TILE").ok().and_then(|v| v.parse().ok());
    for a in &args {
        println!("{}", audit(std::path::Path::new(a), &packs, &out_dir, max_png, &mut tile_hint));
    }
}
