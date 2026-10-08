//! Compact, LLM-oriented observations (JSON, integers only so Rhai controllers see the same shape): the player,
//! enemies sorted by distance with derived hints (melee reach, AI intent, what damages them *right now*), threats
//! (hostile shots / hands with velocity and ETA), a look-ahead forecast of the next hit if you stand still, wall
//! distances and the structured events since the last observation.

use crate::env::{can_act, coarse, describe, outcome_str, Agent};
use crate::macros::{compass, fits, isqrt, live_enemies, mask_names, melee_facings, skill_cost, usable, DIRS};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use wf_core::ai::{BehTree, K, P};
use wf_core::content::GameData;
use wf_core::geom::{overlaps, Rect};
use wf_core::world::Body;

#[derive(Clone, Copy, Debug)]
pub struct ObsOpts {
    pub full: bool,
    pub map: bool,
    /// look-ahead frames for the forecast (0 = none)
    pub forecast: u32,
    /// take (and clear) the events since the last observation
    pub drain: bool,
}
impl Default for ObsOpts {
    fn default() -> Self {
        ObsOpts { full: false, map: false, forecast: 45, drain: true }
    }
}

// ---- AI intent ----------------------------------------------------------------------------------------------------

fn is_flow(k: K) -> bool {
    matches!(k, K::Sequence | K::Selector | K::ParallelAll | K::ParallelAny | K::Succeed | K::SubState)
}
fn is_cond_name(n: &str) -> bool {
    n.starts_with("If") || n == "Chance"
}

/// attack kinds by node, for the `attacking` hint
fn attack_kind(k: K) -> Option<&'static str> {
    use K::*;
    Some(match k {
        MeleeAttack | BioMechPunch | HeavyPunch | WindupMelee | HeavyStrike | ChargedPunch | MiniYakonLunge | DriftContact | BuuStretchPunch | JumpSmash | MiniRollAttack | LungeStep => "melee",
        KiBlast | GunShot | RifleShot | NinjaStarThrow | MummyWrapShot | MummyWrapBurst | TankShoot | SoldierThrow | CyborgShot | TurretFire | BombThrow | FlameBreath | EnergyBarrage | FinalFlash | MouthBeamDown
        | KidBuuKamehameha | StoneSpit | CandyBeam | ThrowBigBall | DarkSphere | SuperJanembaWave | AimAtPlayer | LineUpForShot => "ranged",
        VampireGrab | LatchDrain | GrabWindup | BearHug | EnergyDrain | FreezePlayer | GrappleArm | EatCandy => "grab",
        JanembaHandStrikeA | JanembaHandStrikeB | HandAmbush => "hands",
        _ => return None,
    })
}

/// the running chain of the top AI state: (names of the meaningful nodes, the active leaf kind, its frame count)
pub fn ai_chain(t: &BehTree, rt: &[wf_core::ai::NodeRt]) -> (Vec<String>, Option<(K, i32)>) {
    let mut names = Vec::new();
    let mut leaf = None;
    let mut n = 0usize;
    for _ in 0..32 {
        let Some(node) = t.nodes.get(n) else { break };
        let r = rt.get(n);
        if !is_flow(node.k) {
            names.push(node.name.clone());
        }
        let next = match node.k {
            K::Sequence => r.and_then(|r| node.kids.get(r.i.max(0) as usize)).copied(),
            K::Selector => node.kids.iter().copied().find(|&c| {
                let cn = &t.nodes[c as usize];
                (is_cond_name(&cn.name) && rt.get(c as usize).is_some_and(|x| x.on)) || (!is_cond_name(&cn.name) && !is_flow(cn.k))
            }),
            K::ParallelAll | K::ParallelAny => node.kids.iter().enumerate().find(|(j, _)| r.is_none_or(|r| r.done & (1u64 << j) == 0)).map(|x| *x.1),
            _ if is_cond_name(&node.name) => {
                if r.is_some_and(|r| r.on) {
                    node.kids.first().copied()
                } else {
                    None
                }
            }
            _ => {
                leaf = Some((node.k, r.map(|r| r.f).unwrap_or(0)));
                None
            }
        };
        match next {
            Some(c) => n = c as usize,
            None => break,
        }
    }
    (names, leaf)
}

// ---- damage profile of an AI state (its on-hit tree) -----------------------------------------------------------------

const DMG: u8 = 1;
const HEAL: u8 = 2;

/// abstract run of an on-hit tree for attack kind `k` (0 melee, 1 energy): possible (effects, return) pairs.
/// IfAttack is decided by the kind; every other condition may go either way.
fn hit_paths(d: &GameData, t: &BehTree, n: u16, k: u8, depth: u32) -> Vec<(u8, i32)> {
    if depth > 12 {
        return vec![(0, 0)];
    }
    let node = &t.nodes[n as usize];
    let kid = |i: usize| -> Vec<(u8, i32)> { node.kids.get(i).map(|&c| hit_paths(d, t, c, k, depth + 1)).unwrap_or_else(|| vec![(0, 0)]) };
    let mut v: Vec<(u8, i32)> = match node.k {
        K::Sequence => {
            let mut acc: Vec<(u8, i32, bool)> = vec![(0, 1, false)];
            for &c in &node.kids {
                let sub = hit_paths(d, t, c, k, depth + 1);
                let mut next = Vec::new();
                for &(e, r, stop) in &acc {
                    if stop {
                        next.push((e, r, true));
                        continue;
                    }
                    for &(e2, r2) in &sub {
                        if r2 == 2 {
                            next.push((e | e2, 2, true));
                        } else {
                            next.push((e | e2, r & r2, false));
                        }
                    }
                }
                next.sort();
                next.dedup();
                next.truncate(16);
                acc = next;
            }
            acc.into_iter().map(|(e, r, _)| (e, r)).collect()
        }
        K::Selector => {
            let mut out = Vec::new();
            let mut acc: Vec<u8> = vec![0];
            for &c in &node.kids {
                let sub = hit_paths(d, t, c, k, depth + 1);
                let mut cont = Vec::new();
                for &e in &acc {
                    for &(e2, r2) in &sub {
                        if r2 != 0 {
                            out.push((e | e2, r2));
                        } else {
                            cont.push(e | e2);
                        }
                    }
                }
                cont.sort();
                cont.dedup();
                acc = cont;
                if acc.is_empty() {
                    break;
                }
            }
            out.extend(acc.into_iter().map(|e| (e, 0)));
            out
        }
        K::IfAttack => {
            let a = node.p.g(P::Atk);
            if a == k as i64 {
                kid(0)
            } else if a == 0 || a == 1 {
                vec![(0, 0)]
            } else {
                let mut x = kid(0);
                x.push((0, 0));
                x
            }
        }
        K::ApplyDamage => vec![(DMG, 1)],
        K::HealFraction => vec![(HEAL, 1)],
        K::Succeed | K::PopState => vec![(0, 2)],
        K::GotoState | K::PushState | K::Block | K::StunHold | K::HoverDrift | K::SetHitDir | K::AbsorbPose | K::AbsorbPose2 | K::FreezeHelperHit | K::ShieldHit => vec![(0, 1)],
        K::GlideToWaypoint => vec![(0, 1), (0, 2)],
        K::SubState => match d.aux.get(&(node.p.g(P::State) as u16)) {
            Some(aux) => hit_paths(d, aux, 0, k, depth + 1),
            None => vec![(0, 0)],
        },
        _ if is_cond_name(&node.name) => {
            let mut x = kid(0);
            x.push((0, 0));
            x
        }
        _ => vec![(0, 0)],
    };
    v.sort();
    v.dedup();
    v
}

thread_local! {
    static TAKES: RefCell<BTreeMap<(u16, u8), &'static str>> = const { RefCell::new(BTreeMap::new()) };
}

/// what an attack kind does to an enemy in AI state `st`: "damage" | "heals" | "no damage" | "sometimes" | ...
pub fn takes_in_state(d: &GameData, st: u16, k: u8) -> &'static str {
    if let Some(v) = TAKES.with(|c| c.borrow().get(&(st, k)).copied()) {
        return v;
    }
    let r = match d.aux.get(&st) {
        None => "no damage",
        Some(t) => {
            let paths = hit_paths(d, t, 0, k, 0);
            let any_d = paths.iter().any(|p| p.0 & DMG != 0);
            let all_d = paths.iter().all(|p| p.0 & DMG != 0);
            let any_h = paths.iter().any(|p| p.0 & HEAL != 0);
            match (any_d, all_d, any_h) {
                (true, true, false) => "damage",
                (true, true, true) => "damage + heal",
                (true, false, false) => "damage (conditional)",
                (true, false, true) => "heals or damages (conditional)",
                (false, _, true) => "heals it",
                _ => "no damage",
            }
        }
    };
    TAKES.with(|c| c.borrow_mut().insert((st, k), r));
    r
}

fn mult(res: i32) -> String {
    let v = res * 100 / 128;
    format!("x{}.{:02}", v / 100, v % 100)
}

// ---- observation ----------------------------------------------------------------------------------------------------

pub fn walls(a: &Agent, slot: usize, cap: i32) -> Value {
    let Some(p) = a.player(slot) else { return Value::Null };
    let vec = [(0, 1), (0, -1), (-1, 0), (1, 0)];
    let mut m = Map::new();
    for (k, (dx, dy)) in vec.iter().enumerate() {
        let mut n = 0;
        while n < cap && fits(a, slot, p.a.px() + dx * (n + 1), p.a.py() + dy * (n + 1)) {
            n += 1;
        }
        m.insert(DIRS[k].into(), json!(n));
    }
    Value::Object(m)
}

/// ASCII map of the arena: '#' wall, '.' walkable, 'P' you, '1'-'3' allies, a-z enemies (legend), '*' hostile shot,
/// 'o' your shot. Row 0 / column 0 start at `origin`.
pub fn ascii_map(a: &Agent, slot: usize, cell: i32) -> Value {
    let w = a.w();
    let b = w.arena.bounds;
    // keep big field maps readable: at most ~100 columns / rows
    let cell = cell.max((b.x1 - b.x0 + 99) / 100).max((b.y1 - b.y0 + 99) / 100);
    let Some(p) = a.player(slot) else { return Value::Null };
    let cols = ((b.x1 - b.x0) + cell - 1) / cell;
    let rows = ((b.y1 - b.y0) + cell - 1) / cell;
    let mut g: Vec<Vec<u8>> = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let cx = b.x0 + c * cell + (cell >> 1) - (p.a.foot_w >> 1);
                    let cy = b.y0 + r * cell + (cell >> 1) - (p.a.foot_h >> 1);
                    if fits(a, slot, cx, cy) {
                        b'.'
                    } else {
                        b'#'
                    }
                })
                .collect()
        })
        .collect();
    let mut put = |x: i32, y: i32, ch: u8| {
        let (c, r) = ((x - b.x0).div_euclid(cell), (y - b.y0).div_euclid(cell));
        if r >= 0 && r < rows && c >= 0 && c < cols {
            g[r as usize][c as usize] = ch;
        }
    };
    for (i, e) in w.ents.iter().enumerate() {
        if e.removed {
            continue;
        }
        match &e.body {
            Body::Shot(s) => {
                let r = wf_core::shots::rect(w, i);
                put((r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1, if s.team == 1 { b'o' } else { b'*' });
            }
            Body::Hand(h) => {
                let (x, y) = h.a.foot_center();
                put(x, y, b'*');
            }
            _ => {}
        }
    }
    let mut legend = Map::new();
    for (k, id) in live_enemies(a).into_iter().enumerate() {
        let ch = b'a' + (k % 26) as u8;
        legend.insert((ch as char).to_string(), json!(describe(w, id, None)));
        let (_, e) = w.enemy_by_id(id).unwrap();
        let (x, y) = e.a.foot_center();
        put(x, y, ch);
    }
    for (s, _) in w.players.iter().enumerate() {
        if let Some(q) = a.player(s) {
            let (x, y) = q.a.foot_center();
            put(x, y, if s == slot { b'P' } else { b'1' + s as u8 });
        }
    }
    json!({"origin": [b.x0, b.y0], "cell": cell, "rows": g.into_iter().map(|r| String::from_utf8(r).unwrap()).collect::<Vec<_>>(), "legend": legend})
}

fn player_json(a: &Agent, slot: usize, full: bool) -> Value {
    let Some(p) = a.player(slot) else { return Value::Null };
    let d = a.data();
    let (x, y) = p.a.foot_center();
    let cur = p.skill();
    let f = d.form(p.form);
    let mut v = json!({
        "pos": [x, y], "facing": DIRS[p.a.facing as usize & 3], "hp": [p.hp(), p.max_hp()], "ep": [p.ep(), p.max_ep()],
        "state": coarse(p.state), "can_act": can_act(p),
        "skill": {"slot": p.sel, "name": crate::catalog::skill_name(d, cur), "ep": skill_cost(a, slot, cur), "usable": usable(a, slot, cur)},
    });
    if !p.a.hittable {
        v["invulnerable"] = json!(true);
    }
    if p.held_by != 0 {
        v["held_by"] = json!(describe(a.w(), p.held_by, None));
    }
    if p.a.blocking {
        v["blocking"] = json!(true);
    }
    if f.is_some_and(|f| f.ss || f.ss3) {
        v["ss_gauge"] = json!(if f.unwrap().ss3 { (p.ss3_gauge >> 16) as i32 } else { p.ss_gauge });
    }
    if p.ss_cooldown > 0 {
        v["ss_cooldown"] = json!(p.ss_cooldown >> 8);
    }
    let held = a.w().input[slot.min(3)].held;
    if held != 0 {
        v["holding"] = json!(mask_names(held));
    }
    if let Some((id, t)) = p.sk.as_ref().map(|s| (s.id, s.t)) {
        v["skill_running"] = json!({"name": crate::catalog::skill_name(d, id), "t": t});
    }
    if full {
        v["form"] = json!(format!("{} ({})", crate::catalog::form_name(d, p.form), p.form));
        v["level"] = json!(p.rec.level);
        v["stats"] = json!({"STR": p.a.stats.str_, "POW": p.a.stats.pow, "END": p.a.stats.end, "SPD": p.eff.spd, "walk_256ths_px_per_frame": p.walk});
        v["skill_level"] = json!(p.skill_level);
        v["skills"] = Value::Array(p.skills.iter().enumerate().map(|(i, &id)| json!({"slot": i, "name": crate::catalog::skill_name(d, id), "ep": skill_cost(a, slot, id), "usable": usable(a, slot, id)})).collect());
        v["foot"] = json!([p.a.foot_w, p.a.foot_h]);
    }
    v
}

fn enemies_json(a: &Agent, slot: usize) -> Vec<Value> {
    let w = a.w();
    let d = a.data();
    let Some(p) = a.player(slot) else { return vec![] };
    let (px, py) = p.a.foot_center();
    let multi = a.slots() > 1;
    let mut out: Vec<(i64, Value)> = Vec::new();
    for id in live_enemies(a) {
        let (_, e) = w.enemy_by_id(id).unwrap();
        let t = d.enemy(e.ty);
        let (ex, ey) = e.a.foot_center();
        let (dx, dy) = (ex - px, ey - py);
        let dist = isqrt((dx as i64).pow(2) + (dy as i64).pow(2));
        let mut v = json!({
            "id": id, "name": t.map(|t| t.name.as_str()).unwrap_or("?"), "pos": [ex, ey], "hp": [e.hp(), e.max_hp1024 / 1024],
            "dist": dist, "dir": compass(dx, dy), "facing": DIRS[e.a.facing as usize & 3],
        });
        if let Some(top) = e.ai.stack.last() {
            let (names, leaf) = ai_chain(&top.tree, &top.rt);
            let n = names.len();
            v["ai"] = json!(if n == 0 { "-".to_string() } else { names[n.saturating_sub(3)..].join(">") });
            v["state"] = json!(format!("0x{:x}", top.state));
            if let Some(f) = a.in_state(id) {
                v["in_state"] = json!(f);
            }
            if let Some((k, f)) = leaf {
                if let Some(kind) = attack_kind(k) {
                    v["attacking"] = json!({"kind": kind, "node": format!("{k:?}"), "t": f});
                }
            }
            if let Some(t) = t {
                let m = takes_in_state(d, top.state, 0);
                let en = takes_in_state(d, top.state, 1);
                let fmt = |eff: &str, res: i32| if eff.starts_with("damage") && res != 128 { eff.replacen("damage", &format!("damage {}", mult(res)), 1) } else { eff.to_string() };
                let (ms, es) = (fmt(m, t.res_m), fmt(en, t.res_e));
                let plain = |x: &str| x.starts_with("damage") && !x.contains("heal") && !x.contains(" x");
                if !plain(&ms) || !plain(&es) {
                    v["takes"] = json!({"melee": ms, "energy": es});
                }
            }
        }
        let mf = melee_facings(a, slot, id);
        if !mf.is_empty() && e.a.hittable {
            v["melee"] = json!(if mf.contains(&(p.a.facing as usize)) { "in_range".to_string() } else { format!("turn_{}", DIRS[mf[0]]) });
        }
        if !e.a.hittable {
            v["hittable"] = json!(false);
        }
        if e.a.blocking {
            v["blocking"] = json!(true);
        }
        if e.a.z < -4 {
            v["airborne"] = json!(-e.a.z);
        }
        if !e.primary {
            v["optional"] = json!(true);
        }
        if e.stunned {
            v["stunned"] = json!(true);
        }
        if multi {
            v["targets_slot"] = json!(e.target);
        }
        out.push((dist, v));
    }
    out.sort_by_key(|x| x.0);
    out.into_iter().map(|x| x.1).collect()
}

fn threats_json(a: &Agent, slot: usize) -> (Vec<Value>, usize) {
    let w = a.w();
    let Some(p) = a.player(slot) else { return (vec![], 0) };
    let body = p.a.body_box();
    let me = a.player_id(slot);
    let mut out = Vec::new();
    let mut mine = 0;
    for (i, e) in w.ents.iter().enumerate() {
        if e.removed {
            continue;
        }
        let (r, team, owner, kind, t, pos) = match &e.body {
            Body::Shot(s) => (wf_core::shots::rect(w, i), s.team, s.owner, s.kind_name(), s.t, (s.px(), s.py())),
            Body::Hand(h) => (h.a.body_box(), 0, 0, "hand", h.t as i64, (h.a.px(), h.a.py())),
            _ => continue,
        };
        if team == 1 {
            mine += 1;
            continue;
        }
        if r.x1 <= r.x0 || r.y1 <= r.y0 {
            continue;
        }
        let (vx, vy) = a.prev_pos.get(&e.id).map(|q| (pos.0 - q.0, pos.1 - q.1)).unwrap_or((0, 0));
        let mut v = json!({"id": e.id, "kind": kind, "pos": [(r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1], "size": [r.x1 - r.x0, r.y1 - r.y0], "vel": [vx, vy]});
        if owner != 0 {
            v["from"] = json!(describe(w, owner, me));
        }
        if team == 2 {
            v["hits"] = json!("everyone");
        }
        if kind == "hand" {
            v["note"] = json!("Janemba's hand rising under you: hits if you are on it");
            v["armed_in"] = json!((16 - t).max(0));
            v["gone_in"] = json!((57 - t).max(0));
        }
        if overlaps(&r, &body) {
            v["eta"] = json!(0);
        } else if vx != 0 || vy != 0 {
            for k in 1..=90 {
                let m = Rect::new(r.x0 + vx * k, r.y0 + vy * k, r.x1 + vx * k, r.y1 + vy * k);
                if overlaps(&m, &body) {
                    v["eta"] = json!(k);
                    break;
                }
            }
        }
        out.push(v);
    }
    (out, mine)
}

/// collapse long runs of your own hits into one line per target
fn compact_events(ev: Vec<Value>) -> Vec<Value> {
    let mine = ev.iter().filter(|e| e["type"] == "hit" && e["by"] == "you").count();
    if mine <= 6 {
        return ev;
    }
    let mut by: BTreeMap<String, (u32, i64, u32, u64, u64, i64, BTreeMap<String, u32>)> = BTreeMap::new();
    let mut rest = Vec::new();
    for e in ev {
        if e["type"] == "hit" && e["by"] == "you" {
            let k = e["target"].as_str().unwrap_or("?").to_string();
            let g = by.entry(k).or_insert((0, 0, 0, u64::MAX, 0, 0, BTreeMap::new()));
            g.0 += 1;
            g.1 += -e["hp_change"].as_i64().unwrap_or(0);
            if e["crit"] == true {
                g.2 += 1;
            }
            let f = e["f"].as_u64().unwrap_or(0);
            g.3 = g.3.min(f);
            g.4 = g.4.max(f);
            g.5 = e["target_hp"].as_i64().unwrap_or(0);
            *g.6.entry(e["effect"].as_str().unwrap_or("").to_string()).or_default() += 1;
        } else {
            rest.push(e);
        }
    }
    for (k, g) in by {
        rest.push(json!({"f": g.4, "type": "hits", "by": "you", "target": k, "count": g.0, "frames": [g.3, g.4], "hp_removed": g.1, "crits": g.2, "effects": g.6, "target_hp": g.5}));
    }
    rest.sort_by_key(|e| e["f"].as_u64().unwrap_or(0));
    rest
}

pub fn observe(a: &mut Agent, slot: usize, o: ObsOpts) -> Value {
    let mut v = observe_ro(a, slot, o);
    if o.drain {
        let ev = std::mem::take(&mut a.events);
        let dropped = std::mem::take(&mut a.events_dropped);
        let ev = compact_events(ev);
        let n = ev.len();
        let ev: Vec<Value> = ev.into_iter().skip(n.saturating_sub(40)).collect();
        if !ev.is_empty() {
            v["events"] = Value::Array(ev);
        }
        if dropped + n.saturating_sub(40) > 0 {
            v["events_dropped"] = json!(dropped + n.saturating_sub(40));
        }
    }
    v
}

/// the observation without touching the event buffer
pub fn observe_ro(a: &Agent, slot: usize, o: ObsOpts) -> Value {
    let w = a.w();
    let mut v = json!({
        "session": a.id, "frame": a.frame(), "outcome": outcome_str(a.outcome()),
        "player": player_json(a, slot, o.full),
        "enemies": enemies_json(a, slot),
    });
    if a.slots() > 1 {
        v["slot"] = json!(slot);
        v["allies"] = Value::Array(
            (0..a.slots())
                .filter(|&s| s != slot)
                .filter_map(|s| a.player(s).map(|p| (s, p)))
                .map(|(s, p)| json!({"slot": s, "pos": [p.a.foot_center().0, p.a.foot_center().1], "hp": [p.hp(), p.max_hp()], "state": coarse(p.state), "autopilot": a.autopilot_desc.get(s)}))
                .collect(),
        );
    }
    if w.battle.phases.len() > 1 {
        v["phase"] = json!([w.battle.phase + 1, w.battle.phases.len()]);
        if w.battle.next_phase_at > 0 {
            v["next_phase_in"] = json!(w.battle.next_phase_at as i64 - w.frame as i64);
        }
    }
    let (threats, mine) = threats_json(a, slot);
    if !threats.is_empty() {
        v["threats"] = Value::Array(threats);
    }
    if mine > 0 {
        v["your_shots"] = json!(mine);
    }
    if o.forecast > 0 && a.outcome() == wf_core::battle::Outcome::Running {
        if let Some((k, by, hp)) = a.forecast(slot, o.forecast, 0) {
            let mut f = json!({"hit_in": k, "by": by, "hp": hp, "if": "you stand still"});
            match a.forecast(slot, o.forecast, wf_core::input::R) {
                Some((k2, by2, hp2)) => f["if_blocking"] = json!({"hit_in": k2, "by": by2, "hp": hp2}),
                None => f["if_blocking"] = json!("no HP lost"),
            }
            v["danger"] = f;
        }
    }
    v["walls"] = walls(a, slot, 96);
    if w.g.candy > 0 {
        v["candy"] = json!({"frames": w.g.candy, "note": "you are candy: no skills"});
    }
    if w.field.is_some() {
        v["field"] = field_json(a, slot);
    }
    if a.story.is_some() {
        v["story"] = story_json(a);
    }
    if o.full {
        let b = w.arena.bounds;
        let name = wf_core::battle::ARENAS.iter().find(|x| x.0 == w.arena.id).map(|x| x.2).unwrap_or("");
        v["arena"] = json!({"id": w.arena.id, "name": name, "bounds": [b.x0, b.y0, b.x1, b.y1]});
    }
    if o.map {
        v["map"] = ascii_map(a, slot, 16);
    }
    v
}

/// field sessions: the area, its exits (with destinations) and other triggers, the fade mode
pub fn field_json(a: &Agent, slot: usize) -> Value {
    use wf_core::fieldcontent::{ObjKind, TriggerKind};
    let w = a.w();
    let Some(f) = w.field.as_ref() else { return Value::Null };
    let fd = w.data.field.as_ref();
    let area = fd.and_then(|d| d.area(f.area.world, f.area.area));
    let map = fd.zip(area).and_then(|(d, ar)| d.map_of(ar, f.area.variant.max(1)));
    let name_of = |wd: i32, ar: i32| fd.and_then(|d| d.area(wd, ar)).and_then(|x| x.name.clone()).unwrap_or_default();
    let (px, py) = a.player(slot).map(|p| p.a.foot_center()).unwrap_or((0, 0));
    let mut exits = Vec::new();
    let mut others = Vec::new();
    for t in &f.triggers {
        let def = area.and_then(|ar| ar.objects.iter().find(|o| o.at == t.obj)).or_else(|| map.and_then(|m| m.exits.iter().find(|o| o.at == t.obj)));
        let r = t.rect;
        let (cx, cy) = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let dist = isqrt(((cx - px) as i64).pow(2) + ((cy - py) as i64).pow(2));
        match def.map(|d| &d.kind) {
            Some(ObjKind::Trigger { kind: TriggerKind::Exit { to, .. } | TriggerKind::EdgeExit { to, .. }, .. }) => {
                exits.push((dist, json!({"rect": [r.x0, r.y0, r.x1, r.y1], "center": [cx, cy], "dist": dist, "dir": compass(cx - px, cy - py), "to": format!("{}-{}", to.world, to.area), "to_name": name_of(to.world, to.area)})));
            }
            Some(ObjKind::Trigger { kind, .. }) => {
                let k = match kind {
                    TriggerKind::Script(_) => "script",
                    TriggerKind::Event(_) => "event",
                    TriggerKind::SavePoint(_) => "save point",
                    TriggerKind::Message { .. } => "message",
                    TriggerKind::Region => "region",
                    _ => "unknown",
                };
                let mut v = json!({"kind": k, "rect": [r.x0, r.y0, r.x1, r.y1], "center": [cx, cy], "dist": dist, "dir": compass(cx - px, cy - py), "activation": format!("{:?}", t.act)});
                match t.act {
                    wf_core::fieldcontent::Activation::Button(d) => {
                        v["activation"] = json!(match d {
                            Some(d) => format!("press A inside it facing {}", DIRS[d as usize & 3]),
                            None => "press A inside it".to_string(),
                        })
                    }
                    wf_core::fieldcontent::Activation::Touch | wf_core::fieldcontent::Activation::TouchOnce => v["activation"] = json!("walk into it"),
                    _ => {}
                }
                others.push((dist, v));
            }
            _ => {}
        }
    }
    exits.sort_by_key(|x| x.0);
    others.sort_by_key(|x| x.0);
    let mode = match &f.mode {
        wf_core::field::Mode::Normal => "normal".to_string(),
        wf_core::field::Mode::FadeOut { .. } => "leaving the area (fade-out)".to_string(),
        wf_core::field::Mode::FadeIn { .. } => "arriving (fade-in)".to_string(),
    };
    let mut npcs: Vec<(i64, Value)> = w
        .ents
        .iter()
        .filter(|e| !e.removed)
        .filter_map(|e| match &e.body {
            wf_core::world::Body::Npc(n) => {
                let (nx, ny) = n.a.foot_center();
                let dist = isqrt(((nx - px) as i64).pow(2) + ((ny - py) as i64).pow(2));
                let mut v = json!({"id": e.id, "pos": [nx, ny], "dist": dist, "dir": compass(nx - px, ny - py)});
                if n.talk.is_some() {
                    v["talks"] = json!(true);
                }
                Some((dist, v))
            }
            _ => None,
        })
        .collect();
    npcs.sort_by_key(|x| x.0);
    // the world map's flight (0-50): heading in degrees (0 = north, clockwise), altitude px, speed - to steer by
    let flight = f.flight.as_ref().map(|fl| json!({"heading_deg": ((fl.h >> 10) & 255) * 360 / 256, "altitude": fl.z >> 10, "speed": fl.spd, "reversing": fl.rev}));
    json!({
        "flight": flight,
        "area": format!("{}-{}", f.area.world, f.area.area), "variant": f.area.variant, "name": name_of(f.area.world, f.area.area), "map": f.map, "mode": mode,
        "npcs": npcs.into_iter().take(10).map(|x| x.1).collect::<Vec<_>>(),
        "busy": f.busy, "exits": exits.into_iter().map(|x| x.1).collect::<Vec<_>>(), "triggers": others.into_iter().take(12).map(|x| x.1).collect::<Vec<_>>(),
        "note": if a.story.is_some() { "story session: scripts, dialogue, NPCs and events run as in the game" } else { "scripts, dialogue and NPC events are not run in field sessions (no script host; new_game runs the story); exits, enemies, pickups and breakables work" },
    })
}

/// menu / dialogue text as plain characters (colour codes `\u{8}`+n dropped, page breaks as " / ")
fn plain(t: impl Iterator<Item = u32>) -> String {
    let mut o = String::new();
    let mut skip = false;
    for c in t {
        if skip {
            skip = false;
            continue;
        }
        match c {
            8 => skip = true,
            10 => o.push(' '),
            11 => o.push_str(" / "),
            0x20..=0x7E => o.push(c as u8 as char),
            _ => {}
        }
    }
    o.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// story sessions: the menu screen up (title, pause menu, shop, save ...) with its text, the dialogue box, whether
/// an event holds the field, the save slots
pub fn story_json(a: &Agent) -> Value {
    let w = a.w();
    let mut v = json!({"frame": a.frame(), "event_running": w.scene.depth > 0});
    if wf_core::menu::active(w) {
        let mut m = json!({"open": true});
        if let (Some(sc), Some(d)) = (w.menu.stack.last(), w.data.menu.as_ref()) {
            m["screen"] = json!(sc.page().name());
            let view = sc.page().view(w, d);
            let mut texts = Vec::new();
            for l in &view.layers {
                for op in &l.ops {
                    match op {
                        wf_view::MenuOp::Text { x, y, text, .. } | wf_view::MenuOp::TextOutline { x, y, text, .. } => {
                            let t = plain(text.iter().map(|&c| c as u32));
                            if !t.is_empty() {
                                texts.push((*y + l.y, *x + l.x, t));
                            }
                        }
                        _ => {}
                    }
                }
            }
            texts.sort();
            m["text"] = json!(texts.into_iter().map(|t| t.2).collect::<Vec<_>>());
        }
        v["menu"] = m;
    }
    if let Some(d) = &w.scene.dialog {
        let pages: Vec<String> = d.text.split('\u{b}').map(|p| plain(p.chars().map(|c| c as u32))).collect();
        let mut dj = json!({"text": pages.get(d.page as usize).cloned().unwrap_or_default(), "page": [d.page + 1, d.pages.max(1)]});
        if d.options > 0 {
            dj["options"] = json!(d.options);
            dj["cursor"] = json!(d.cursor);
        }
        v["dialog"] = dj;
        v["hint"] = json!("A advances the text (press, then release); UP / DOWN move a choice's cursor");
    }
    let sv = &w.saves;
    let slots: Vec<Value> = (0..3)
        .map(|i| match sv.slot(i) {
            Some(d) => json!({"slot": i + 1, "area": format!("{}-{}", d.world, d.area)}),
            None => json!({"slot": i + 1, "empty": true}),
        })
        .collect();
    v["save_slots"] = Value::Array(slots);
    v
}
