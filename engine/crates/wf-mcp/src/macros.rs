//! Deterministic high-level actions. A macro is a policy that returns one held-button mask per frame, so it is
//! exactly the button presses it produced: the input log, replays and rewinds need nothing special.

use crate::env::{can_act, Agent, Delta};
use serde_json::{json, Value};
use wf_core::geom::{overlaps, Rect};
use wf_core::input::{A, B, DOWN, L, LEFT, R, RIGHT, UP};
use wf_core::player::PState;

pub enum Step {
    Press(u16),
    /// finished; an empty reason lets a `Chain` continue with its next part
    Done(String),
}

pub trait Policy {
    fn next(&mut self, a: &Agent, slot: usize) -> Step;
}

/// a closure policy
pub struct FnPolicy<F: FnMut(&Agent, usize) -> Step>(pub F);
impl<F: FnMut(&Agent, usize) -> Step> Policy for FnPolicy<F> {
    fn next(&mut self, a: &Agent, slot: usize) -> Step {
        (self.0)(a, slot)
    }
}
pub fn pol<F: FnMut(&Agent, usize) -> Step + 'static>(f: F) -> Box<dyn Policy> {
    Box::new(FnPolicy(f))
}

/// parts run one after another; a part that finishes with a non-empty reason ends the chain with it
pub struct Chain(pub Vec<Box<dyn Policy>>, pub usize);
impl Policy for Chain {
    fn next(&mut self, a: &Agent, slot: usize) -> Step {
        while self.1 < self.0.len() {
            match self.0[self.1].next(a, slot) {
                Step::Press(m) => return Step::Press(m),
                Step::Done(r) if r.is_empty() => self.1 += 1,
                Step::Done(r) => return Step::Done(r),
            }
        }
        Step::Done("done".into())
    }
}
pub fn chain(v: Vec<Box<dyn Policy>>) -> Box<dyn Policy> {
    Box::new(Chain(v, 0))
}

pub const DIRS: [&str; 4] = ["down", "up", "left", "right"];
pub const DIR_BTN: [u16; 4] = [DOWN, UP, LEFT, RIGHT];
/// player melee rects 0x083C344C per facing (relative to the footprint's top-left)
pub const PUNCH_RECT: [[i32; 4]; 4] = [[0, 0, 16, 16], [0, -24, 16, 0], [-8, -24, 0, 8], [16, -24, 24, 8]];

pub fn dir_index(d: &str) -> Result<usize, String> {
    DIRS.iter().position(|x| *x == d.to_lowercase()).ok_or_else(|| format!("direction must be one of {}", DIRS.join(", ")))
}

const BUTTONS: [(&str, u16); 10] = [("A", A), ("B", B), ("SELECT", 1 << 2), ("START", 1 << 3), ("RIGHT", RIGHT), ("LEFT", LEFT), ("UP", UP), ("DOWN", DOWN), ("R", R), ("L", L)];

/// "RIGHT+A", "right a", ["RIGHT","A"], 17 -> mask. Aliases: punch=A, skill=B, block=R, cycle=L.
pub fn parse_buttons(v: &Value) -> Result<u16, String> {
    match v {
        Value::Null => Ok(0),
        Value::Number(n) => Ok((n.as_u64().unwrap_or(0) & 0x3ff) as u16),
        Value::Array(a) => a.iter().map(parse_buttons).try_fold(0, |m, x| x.map(|x| m | x)),
        Value::String(s) => parse_button_str(s),
        _ => Err("buttons must be a string like \"RIGHT+A\"".into()),
    }
}
pub fn parse_button_str(s: &str) -> Result<u16, String> {
    let mut m = 0;
    for raw in s.split(|c: char| c == '+' || c == ',' || c == '|' || c.is_whitespace()) {
        let p = raw.trim().to_uppercase();
        if p.is_empty() || p == "NONE" || p == "-" {
            continue;
        }
        let name = match p.as_str() {
            "PUNCH" => "A",
            "SKILL" => "B",
            "BLOCK" => "R",
            "CYCLE" => "L",
            "U" => "UP",
            "D" => "DOWN",
            x => x,
        };
        let b = BUTTONS.iter().find(|b| b.0 == name).ok_or_else(|| format!("unknown button \"{raw}\" (use A, B, L, R, UP, DOWN, LEFT, RIGHT, START, SELECT)"))?;
        m |= b.1;
    }
    Ok(m)
}
pub fn mask_names(m: u16) -> String {
    let v: Vec<&str> = BUTTONS.iter().filter(|b| m & b.1 != 0).map(|b| b.0).collect();
    if v.is_empty() {
        "".into()
    } else {
        v.join("+")
    }
}

// ---- geometry helpers ---------------------------------------------------------------------------------------------

pub fn punch_rect(px: i32, py: i32, f: usize) -> Rect {
    Rect::at(PUNCH_RECT[f], px, py)
}

/// facings from which a punch right now overlaps the enemy's body box
pub fn melee_facings(a: &Agent, slot: usize, eid: u32) -> Vec<usize> {
    let (Some(p), Some((_, e))) = (a.player(slot), a.w().enemy_by_id(eid)) else { return vec![] };
    let b = e.a.body_box();
    (0..4).filter(|&f| overlaps(&punch_rect(p.a.px(), p.a.py(), f), &b)).collect()
}

pub fn isqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = (v as f64).sqrt() as i64;
    while x * x > v {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= v {
        x += 1;
    }
    x
}

pub fn compass(dx: i32, dy: i32) -> String {
    let (ax, ay) = (dx.abs(), dy.abs());
    if ax < 4 && ay < 4 {
        return "here".into();
    }
    let h = if dx > 0 { "right" } else { "left" };
    let v = if dy > 0 { "down" } else { "up" };
    if ax * 5 > ay * 12 {
        return h.into();
    }
    if ay * 5 > ax * 12 {
        return v.into();
    }
    format!("{v}-{h}")
}

pub fn fits(a: &Agent, slot: usize, x: i32, y: i32) -> bool {
    a.pidx(slot).is_some_and(|i| wf_core::player::fits(a.w(), i, x, y))
}

/// live (not defeated, not removed) non-part enemies
pub fn live_enemies(a: &Agent) -> Vec<u32> {
    let w = a.w();
    w.ents
        .iter()
        .filter_map(|e| match &e.body {
            wf_core::world::Body::Enemy(en) if !e.removed && !en.defeated && !en.part => Some(e.id),
            _ => None,
        })
        .collect()
}

fn enemy_alive(a: &Agent, id: u32) -> bool {
    a.w().enemy_by_id(id).is_some_and(|(i, e)| !e.defeated && !a.w().ents[i].removed)
}

/// resolve an enemy reference: numeric id, "#id", name substring, "nearest" / none = nearest (primary first)
pub fn find_enemy(a: &Agent, slot: usize, r: Option<&str>) -> Option<u32> {
    let live = live_enemies(a);
    let w = a.w();
    let p = a.player(slot)?;
    let (px, py) = p.a.foot_center();
    let nearest = |list: Vec<u32>| -> Option<u32> {
        list.into_iter().min_by_key(|&id| {
            let (_, e) = w.enemy_by_id(id).unwrap();
            let (x, y) = e.a.foot_center();
            ((x - px) as i64).pow(2) + ((y - py) as i64).pow(2)
        })
    };
    match r.map(|s| s.trim()) {
        None | Some("") | Some("nearest") => {
            let prim: Vec<u32> = live.iter().copied().filter(|&id| w.enemy_by_id(id).is_some_and(|(_, e)| e.primary)).collect();
            nearest(if prim.is_empty() { live } else { prim })
        }
        Some(s) => {
            let s = s.trim_start_matches('#');
            if let Ok(n) = s.parse::<u32>() {
                return enemy_alive(a, n).then_some(n);
            }
            let q = crate::catalog::norm(s);
            nearest(live.into_iter().filter(|&id| w.enemy_by_id(id).is_some_and(|(_, e)| w.data.enemy(e.ty).is_some_and(|t| crate::catalog::norm(&t.name).contains(&q)))).collect())
        }
    }
}

// ---- running a policy ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub frames: u32,
    pub stopped: String,
    pub dealt: i64,
    pub hp_lost: i64,
    pub hits_dealt: u32,
    pub no_damage_hits: u32,
    pub healed_enemy: i64,
    pub hits_taken: u32,
    pub kills: u32,
    pub ep_change: i64,
}
impl Summary {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"frames": self.frames, "stopped": self.stopped, "dealt": self.dealt, "hp_lost": self.hp_lost, "hits_dealt": self.hits_dealt, "hits_taken": self.hits_taken, "kills": self.kills, "ep_change": self.ep_change});
        if self.no_damage_hits > 0 {
            v["hits_without_damage"] = json!(self.no_damage_hits);
        }
        if self.healed_enemy > 0 {
            v["healed_enemy_hp"] = json!(self.healed_enemy);
        }
        v
    }
    fn add(&mut self, d: &Delta) {
        self.dealt += d.dealt;
        self.hp_lost += d.hp_lost;
        self.hits_dealt += d.hits_dealt;
        self.no_damage_hits += d.no_damage_hits;
        self.healed_enemy += d.healed_enemy;
        self.hits_taken += d.hits_taken;
        self.kills += d.kills;
    }
}

pub const STOP_ON: [&str; 8] = ["hurt", "kill", "hit", "phase", "danger", "no_damage", "event", "area"];

/// story sessions: the field is not the player's (an event, a dialogue box or a menu screen)
pub fn story_busy(a: &Agent) -> bool {
    let w = a.w();
    a.story.is_some() && (w.scene.depth > 0 || w.scene.dialog.is_some() || wf_core::menu::active(w))
}

/// run a policy for at most `max` frames on `slot`; stops early on the battle's end or a `stop_on` condition
pub fn run_policy(a: &mut Agent, slot: usize, p: &mut dyn Policy, max: u32, stop_on: &[String]) -> Summary {
    let ep0 = a.player(slot).map(|p| p.ep()).unwrap_or(0) as i64;
    let mut s = Summary { stopped: "max_frames".into(), ..Default::default() };
    let has = |k: &str| stop_on.iter().any(|x| x == k);
    for _ in 0..max {
        if a.outcome() != wf_core::battle::Outcome::Running {
            s.stopped = format!("battle_{}", crate::env::outcome_str(a.outcome()));
            break;
        }
        let m = match p.next(a, slot) {
            Step::Press(m) => m,
            Step::Done(r) => {
                s.stopped = if r.is_empty() { "done".into() } else { r };
                break;
            }
        };
        let busy0 = story_busy(a);
        let area0 = a.w().field.as_ref().map(|f| (f.area.world, f.area.area));
        let d = a.step(slot, m);
        s.frames += 1;
        s.add(&d);
        // an exit taken: the macro's targets were the old area's
        let area1 = a.w().field.as_ref().map(|f| (f.area.world, f.area.area));
        if area1 != area0 && has("area") {
            s.stopped = format!("area changed: now in {}-{}", area1.map_or(0, |x| x.0), area1.map_or(0, |x| x.1));
            break;
        }
        if has("event") && !busy0 && story_busy(a) {
            let w = a.w();
            s.stopped = match (&w.scene.dialog, wf_core::menu::active(w)) {
                (Some(_), _) => "event: a dialogue box opened (observe, then A)".into(),
                (None, true) => "event: a menu screen opened (observe)".into(),
                _ => "event: an event took over the field (wait / observe)".into(),
            };
            break;
        }
        if has("hurt") && d.hp_lost > 0 {
            s.stopped = "hurt".into();
            break;
        }
        if has("kill") && d.kills > 0 {
            s.stopped = "kill".into();
            break;
        }
        if has("hit") && d.hits_dealt > 0 {
            s.stopped = "hit".into();
            break;
        }
        if has("no_damage") && d.no_damage_hits > 0 {
            s.stopped = "no_damage".into();
            break;
        }
        if has("phase") && d.phase {
            s.stopped = "phase".into();
            break;
        }
        if has("danger") && s.frames.is_multiple_of(2) && a.outcome() == wf_core::battle::Outcome::Running {
            if let Some((k, by, hp)) = a.forecast(slot, 12, 0) {
                s.stopped = format!("danger: {by} hits you for {hp} in {k} frames if you stand still");
                break;
            }
        }
    }
    if s.stopped == "max_frames" && a.outcome() != wf_core::battle::Outcome::Running {
        s.stopped = format!("battle_{}", crate::env::outcome_str(a.outcome()));
    }
    s.ep_change = a.player(slot).map(|p| p.ep()).unwrap_or(0) as i64 - ep0;
    s
}

// ---- primitives -----------------------------------------------------------------------------------------------

pub fn sequence(steps: Vec<(u16, u32)>) -> Box<dyn Policy> {
    let mut i = 0usize;
    let mut k = 0u32;
    pol(move |_, _| {
        while i < steps.len() && k >= steps[i].1 {
            i += 1;
            k = 0;
        }
        if i >= steps.len() {
            return Step::Done(String::new());
        }
        k += 1;
        Step::Press(steps[i].0)
    })
}
pub fn hold(mask: u16, frames: u32) -> Box<dyn Policy> {
    sequence(vec![(mask, frames)])
}

/// wait until the player can act (max `max` frames)
pub fn wait_can_act(max: u32) -> Box<dyn Policy> {
    let mut n = 0;
    pol(move |a, slot| {
        if a.player(slot).is_some_and(can_act) || n >= max {
            return Step::Done(String::new());
        }
        n += 1;
        Step::Press(0)
    })
}

// ---- path finding on the walk grid ----------------------------------------------------------------------------

const LAT: i32 = 4;

/// BFS over a 4 px lattice of footprint top-left positions the player fits at; returns waypoints (top-left px) and
/// whether the goal was reached (else the path ends at the closest reachable point)
pub fn plan_path(a: &Agent, slot: usize, tx: i32, ty: i32) -> (Vec<(i32, i32)>, bool) {
    let Some(p) = a.player(slot) else { return (vec![], false) };
    let b = a.w().arena.bounds;
    let cols = (b.x1 - b.x0 + LAT - 1) / LAT + 1;
    let rows = (b.y1 - b.y0 + LAT - 1) / LAT + 1;
    let mut cache = vec![0u8; (cols * rows) as usize];
    let mut fit = |c: i32, r: i32| -> bool {
        if c < 0 || r < 0 || c >= cols || r >= rows {
            return false;
        }
        let k = (r * cols + c) as usize;
        if cache[k] == 0 {
            cache[k] = if fits(a, slot, b.x0 + c * LAT, b.y0 + r * LAT) { 1 } else { 2 };
        }
        cache[k] == 1
    };
    let to_c = |x: i32| (x - b.x0 + LAT / 2).div_euclid(LAT);
    let to_r = |y: i32| (y - b.y0 + LAT / 2).div_euclid(LAT);
    let (mut sc, mut sr) = (to_c(p.a.px()), to_r(p.a.py()));
    if !fit(sc, sr) {
        'o: for d in 1..4 {
            for dr in -d..=d {
                for dc in -d..=d {
                    if fit(sc + dc, sr + dr) {
                        sc += dc;
                        sr += dr;
                        break 'o;
                    }
                }
            }
        }
    }
    let (gc, gr) = (to_c(tx), to_r(ty));
    let n = (cols * rows) as usize;
    let mut prev = vec![-1i32; n];
    let key = |c: i32, r: i32| r * cols + c;
    let start = key(sc, sr);
    if start < 0 || start as usize >= n {
        return (vec![], false);
    }
    prev[start as usize] = start;
    let mut q = std::collections::VecDeque::from([start]);
    let mut best = start;
    let d2 = |c: i32, r: i32| ((c - gc) as i64).pow(2) + ((r - gr) as i64).pow(2);
    let mut best_d = d2(sc, sr);
    while let Some(k) = q.pop_front() {
        let (c, r) = (k % cols, k / cols);
        let d = d2(c, r);
        if d < best_d {
            best_d = d;
            best = k;
            if d == 0 {
                break;
            }
        }
        for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (nc, nr) = (c + dc, r + dr);
            if !fit(nc, nr) {
                continue;
            }
            if dc != 0 && dr != 0 && (!fit(c + dc, r) || !fit(c, r + dr)) {
                continue;
            }
            let nk = key(nc, nr);
            if prev[nk as usize] != -1 {
                continue;
            }
            prev[nk as usize] = k;
            q.push_back(nk);
        }
    }
    let mut nodes = Vec::new();
    let mut k = best;
    loop {
        nodes.push((b.x0 + (k % cols) * LAT, b.y0 + (k / cols) * LAT));
        if k == start {
            break;
        }
        k = prev[k as usize];
    }
    nodes.reverse();
    let mut path = Vec::new();
    for i in 1..nodes.len() {
        let (pa, c) = (nodes[i - 1], nodes[i]);
        match nodes.get(i + 1) {
            Some(n) if (c.0 - pa.0).signum() == (n.0 - c.0).signum() && (c.1 - pa.1).signum() == (n.1 - c.1).signum() => {}
            _ => path.push(c),
        }
    }
    let reached = best_d <= 1;
    if reached {
        if let Some(l) = path.last_mut() {
            *l = (tx, ty);
        }
    }
    (path, reached)
}

/// d-pad mask that moves the footprint top-left toward (tx, ty)
pub fn steer(a: &Agent, slot: usize, tx: i32, ty: i32) -> u16 {
    let Some(p) = a.player(slot) else { return 0 };
    let (x, y) = (p.a.x, p.a.y);
    let (dx, dy) = ((tx << 8) - x, (ty << 8) - y);
    let dead = 154; // 0.6 px
    let mut m = 0;
    if dx > dead {
        m |= RIGHT;
    } else if dx < -dead {
        m |= LEFT;
    }
    if dy > dead {
        m |= DOWN;
    } else if dy < -dead {
        m |= UP;
    }
    m
}

/// walk the footprint top-left to (tx, ty), re-planning when stuck
pub fn walk_to(tx: i32, ty: i32, tol: i32) -> Box<dyn Policy> {
    let mut plan: Option<(Vec<(i32, i32)>, bool)> = None;
    let mut wp = 0usize;
    let mut last = (i32::MIN, i32::MIN);
    let mut still = 0;
    pol(move |a, slot| {
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        if (p.a.px() - tx).abs() <= tol && (p.a.py() - ty).abs() <= tol {
            return Step::Done("arrived".into());
        }
        if !can_act(p) {
            return Step::Press(0);
        }
        let (path, reached) = plan.get_or_insert_with(|| plan_path(a, slot, tx, ty));
        while wp + 1 < path.len() && (p.a.px() - path[wp].0).abs() <= 1 && (p.a.py() - path[wp].1).abs() <= 1 {
            wp += 1;
        }
        let t = path.get(wp).copied().unwrap_or((tx, ty));
        if wp + 1 >= path.len() && !*reached && (p.a.px() - t.0).abs() <= 1 && (p.a.py() - t.1).abs() <= 1 {
            return Step::Done("unreachable (stopped at the closest point)".into());
        }
        let m = steer(a, slot, t.0, t.1);
        if (p.a.x, p.a.y) == last {
            still += 1;
            if still > 8 {
                plan = None;
                wp = 0;
                still = 0;
            }
        } else {
            still = 0;
        }
        last = (p.a.x, p.a.y);
        Step::Press(m)
    })
}

/// take an exit: walk next to its rect (a door sits in a wall: the walk stops at the closest point), then keep
/// walking into it - toward its centre, and once on it into the wall / border it sits at - until the area changes
/// (the run's `area` stop ends it). `rect` = the exit trigger's rect (world px).
pub fn take_exit(rect: [i32; 4], fw: i32, fh: i32) -> Box<dyn Policy> {
    let (cx, cy) = ((rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2);
    let mut walk = Some(walk_to(cx - (fw >> 1), cy - (fh >> 1), 1));
    pol(move |a, slot| {
        if let Some(w) = walk.as_mut() {
            match w.next(a, slot) {
                Step::Press(m) => return Step::Press(m),
                Step::Done(_) => walk = None,
            }
        }
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        if !can_act(p) {
            return Step::Press(0);
        }
        let (fx, fy) = p.a.foot_center();
        let on = fx >= rect[0] && fx <= rect[2] && fy >= rect[1] && fy <= rect[3];
        if !on {
            // toward its centre on the axes that are free (a diagonal into a corridor's corner sticks)
            let (dx, dy) = (cx - (fw >> 1) - p.a.px(), cy - (fh >> 1) - p.a.py());
            let (sx, sy) = (dx.signum(), dy.signum());
            let mut m = 0;
            if dx.abs() > 1 && fits(a, slot, p.a.px() + sx, p.a.py()) {
                m |= if sx > 0 { RIGHT } else { LEFT };
            }
            if dy.abs() > 1 && fits(a, slot, p.a.px(), p.a.py() + sy) {
                m |= if sy > 0 { DOWN } else { UP };
            }
            if m != 0 {
                return Step::Press(m);
            }
        }
        // into the exit: across its short side (a tall strip at a map's left / right edge, a wide one at the top /
        // bottom), toward the nearer wall or border; a square door: the nearest wall
        let free = |dx: i32, dy: i32| (1..=96).take_while(|k| fits(a, slot, p.a.px() + dx * k, p.a.py() + dy * k)).count() as i32;
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let mut cands: Vec<(i32, u16)> = Vec::new();
        if h >= w {
            cands.push((free(-1, 0), LEFT));
            cands.push((free(1, 0), RIGHT));
        }
        if w >= h {
            cands.push((free(0, -1), UP));
            cands.push((free(0, 1), DOWN));
        }
        cands.sort_by_key(|c| c.0);
        Step::Press(cands[0].1)
    })
}

// ---- melee ----------------------------------------------------------------------------------------------------

/// footprint top-left spots from which a punch with facing f overlaps the enemy body, nearest first
pub fn attack_spots(a: &Agent, slot: usize, eid: u32) -> Vec<(usize, i32, i32)> {
    let (Some(p), Some((_, e))) = (a.player(slot), a.w().enemy_by_id(eid)) else { return vec![] };
    let b = e.a.body_box();
    let (px, py) = (p.a.px(), p.a.py());
    let mut out = Vec::new();
    for f in 0..4 {
        let r = PUNCH_RECT[f];
        let (xlo, xhi, ylo, yhi) = (b.x0 - r[2] + 1, b.x1 - r[0] - 1, b.y0 - r[3] + 1, b.y1 - r[1] - 1);
        if xlo > xhi || ylo > yhi {
            continue;
        }
        let cl = |v: i32, lo: i32, hi: i32| if lo + 2 > hi - 2 { (lo + hi) / 2 } else { v.clamp(lo + 2, hi - 2) };
        // preferred: the outer edge of the reach along the facing axis, on the current line across it
        let (x, y) = match f {
            3 => (xlo + 3, cl(py, ylo, yhi)),
            2 => (xhi - 3, cl(py, ylo, yhi)),
            0 => (cl(px, xlo, xhi), ylo + 3),
            _ => (cl(px, xlo, xhi), yhi - 3),
        };
        let bias = if f < 2 { 24 } else { 0 };
        if fits(a, slot, x, y) {
            out.push((f, x, y, isqrt(((x - px) as i64).pow(2) + ((y - py) as i64).pow(2)) + bias));
            continue;
        }
        // else the nearest fitting spot anywhere in the reach (an enemy next to a wall / tree)
        let mut best: Option<(i32, i32, i64)> = None;
        for yy in (ylo + 1..yhi).step_by(2) {
            for xx in (xlo + 1..xhi).step_by(2) {
                if fits(a, slot, xx, yy) {
                    let d = isqrt(((xx - px) as i64).pow(2) + ((yy - py) as i64).pow(2));
                    if best.is_none_or(|b| d < b.2) {
                        best = Some((xx, yy, d));
                    }
                }
            }
        }
        if let Some((x, y, d)) = best {
            out.push((f, x, y, d + bias));
        }
    }
    out.sort_by_key(|s| s.3);
    out.into_iter().map(|s| (s.0, s.1, s.2)).collect()
}

/// approach a punching spot, turn, and punch on every new press until `hits` hits or the target falls
pub fn attack(eid: u32, hits: Option<u32>) -> Box<dyn Policy> {
    let mut last = 0u16;
    let mut spot: Option<(i32, i32)> = None;
    let mut plan: Vec<(i32, i32)> = vec![];
    let mut plan_at: i64 = -100;
    let mut wp = 0usize;
    let mut hits0: Option<u32> = None;
    let mut lastpos = (i32::MIN, i32::MIN);
    let mut still = 0u32;
    pol(move |a, slot| {
        let w = a.w();
        if !w.enemy_by_id(eid).is_some_and(|(i, e)| !e.defeated && !w.ents[i].removed) {
            return Step::Done("target_defeated".into());
        }
        let h0 = *hits0.get_or_insert(w.stats.hits_dealt);
        if let Some(n) = hits {
            if w.stats.hits_dealt - h0 >= n {
                return Step::Done("hits_done".into());
            }
        }
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        let prev = last;
        let mut emit = |m: u16| {
            last = m;
            Step::Press(m)
        };
        if !can_act(p) {
            return emit(0);
        }
        let (_, e) = w.enemy_by_id(eid).unwrap();
        let mf = melee_facings(a, slot, eid);
        if e.a.hittable && mf.contains(&(p.a.facing as usize)) {
            return emit(if prev & A != 0 { 0 } else { A });
        }
        if e.a.hittable && !mf.is_empty() {
            return emit(DIR_BTN[mf[0]]);
        }
        let spots = attack_spots(a, slot, eid);
        let (sf, sx, sy) = match spots.first() {
            Some(&s) => s,
            None => {
                // no spot fits (the enemy is over a wall / in the air): get as close as the walk grid allows
                let (ex, ey) = e.a.foot_center();
                (facing_toward(a, slot, eid), ex - (p.a.foot_w >> 1), ey - (p.a.foot_h >> 1))
            }
        };
        let moved = spot.is_none_or(|(x, y)| (x - sx).abs() + (y - sy).abs() > 6);
        if (p.a.x, p.a.y) == lastpos {
            still += 1;
        } else {
            still = 0;
        }
        lastpos = (p.a.x, p.a.y);
        if moved || w.frame as i64 - plan_at > 20 || still == 6 {
            spot = Some((sx, sy));
            plan = plan_path(a, slot, sx, sy).0;
            plan_at = w.frame as i64;
            wp = 0;
        }
        while wp + 1 < plan.len() && (p.a.px() - plan[wp].0).abs() <= 1 && (p.a.py() - plan[wp].1).abs() <= 1 {
            wp += 1;
        }
        let t = plan.get(wp).copied().unwrap_or((sx, sy));
        let mut m = steer(a, slot, t.0, t.1);
        if still >= 6 && m & (LEFT | RIGHT) != 0 && m & (UP | DOWN) != 0 {
            // stuck on a corner: one axis at a time
            m &= if (still / 12).is_multiple_of(2) { LEFT | RIGHT } else { UP | DOWN };
        }
        emit(if m != 0 { m } else { DIR_BTN[sf] })
    })
}

/// wait (max `max` frames) until a live enemy exists
pub fn wait_enemy(max: u32) -> Box<dyn Policy> {
    let mut n = 0;
    pol(move |a, _| {
        if !live_enemies(a).is_empty() || n >= max {
            return Step::Done(String::new());
        }
        n += 1;
        Step::Press(0)
    })
}

/// attack the nearest enemy at the time this starts
pub fn auto_attack_once(hits: Option<u32>) -> Box<dyn Policy> {
    let mut cur: Option<Box<dyn Policy>> = None;
    pol(move |a, slot| {
        if cur.is_none() {
            let Some(e) = find_enemy(a, slot, None) else { return Step::Done("no live enemies".into()) };
            cur = Some(attack(e, hits));
        }
        cur.as_mut().unwrap().next(a, slot)
    })
}

/// autopilot: attack the nearest enemy, forever
pub fn auto_attack() -> Box<dyn Policy> {
    let mut cur: Option<(u32, Box<dyn Policy>)> = None;
    pol(move |a, slot| {
        for _ in 0..2 {
            if cur.is_none() {
                let Some(e) = find_enemy(a, slot, None) else { return Step::Press(0) };
                cur = Some((e, attack(e, None)));
            }
            match cur.as_mut().unwrap().1.next(a, slot) {
                Step::Press(m) => return Step::Press(m),
                Step::Done(_) => cur = None,
            }
        }
        Step::Press(0)
    })
}

// ---- facing, skills, dash, block -------------------------------------------------------------------------------

pub fn facing_toward(a: &Agent, slot: usize, eid: u32) -> usize {
    let (Some(p), Some((_, e))) = (a.player(slot), a.w().enemy_by_id(eid)) else { return 0 };
    let (px, py) = p.a.foot_center();
    let (ex, ey) = e.a.foot_center();
    let (dx, dy) = (ex - px, ey - py);
    if dx.abs() >= dy.abs() {
        if dx > 0 {
            3
        } else {
            2
        }
    } else if dy > 0 {
        0
    } else {
        1
    }
}

/// turn without moving (a 1-frame tap)
pub fn face(dir: Option<usize>, target: Option<u32>) -> Box<dyn Policy> {
    let mut phase = 0;
    let mut waited = 0;
    pol(move |a, slot| {
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        if phase == 0 {
            if !can_act(p) && waited < 60 {
                waited += 1;
                return Step::Press(0);
            }
            let d = dir.unwrap_or_else(|| target.map(|t| facing_toward(a, slot, t)).unwrap_or(p.a.facing as usize));
            if p.a.facing as usize == d {
                return Step::Done(String::new());
            }
            phase = 1;
            return Step::Press(DIR_BTN[d]);
        }
        if phase == 1 {
            phase = 2;
            return Step::Press(0);
        }
        Step::Done(String::new())
    })
}

/// a field NPC's foot centre (None: gone)
pub fn npc_pos(a: &Agent, id: u32) -> Option<(i32, i32)> {
    let i = a.w().find(id)?;
    match &a.w().ents[i].body {
        wf_core::world::Body::Npc(n) => Some(n.a.foot_center()),
        _ => None,
    }
}

/// talk to a field NPC: walk up to it (the walk stops next to it - an NPC is solid), turn to it with a tap, press A
/// (the run's `event` stop ends the macro when its talk event starts)
pub fn talk(id: u32) -> Box<dyn Policy> {
    let mut walk: Option<Box<dyn Policy>> = None;
    let mut phase = 0;
    let mut k = 0;
    pol(move |a, slot| {
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        let Some((nx, ny)) = npc_pos(a, id) else { return Step::Done("the NPC is gone".into()) };
        let (fx, fy) = p.a.foot_center();
        let (dx, dy) = (nx - fx, ny - fy);
        if phase == 0 {
            if dx.abs() <= 24 && dy.abs() <= 20 {
                phase = 1;
            } else {
                let w = walk.get_or_insert_with(|| walk_to(nx - (p.a.foot_w >> 1), ny - (p.a.foot_h >> 1), 1));
                match w.next(a, slot) {
                    Step::Press(m) => return Step::Press(m),
                    Step::Done(_) => phase = 1,
                }
            }
        }
        if !can_act(p) {
            k += 1;
            return if k > 120 { Step::Done("could not act".into()) } else { Step::Press(0) };
        }
        let d = if dx.abs() >= dy.abs() { if dx > 0 { 3 } else { 2 } } else if dy > 0 { 0 } else { 1 };
        phase += 1;
        match phase {
            2 if p.a.facing as usize != d => Step::Press(DIR_BTN[d]),
            2 | 3 => Step::Press(0),
            4 => Step::Press(A),
            5..=30 => Step::Press(0),
            _ => Step::Done("no reaction (not facing it, too far, or it has nothing to say)".into()),
        }
    })
}

/// use a button trigger (a desk, a sign, a door that wants A): walk into its rect, turn to `facing` (None: as you
/// are) with a tap, press A (the run's `event` stop ends the macro when its event starts)
pub fn use_trigger(rect: [i32; 4], facing: Option<usize>) -> Box<dyn Policy> {
    let (cx, cy) = ((rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2);
    let mut walk: Option<Box<dyn Policy>> = None;
    let mut phase = 0;
    let mut k = 0;
    pol(move |a, slot| {
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        let (fx, fy) = p.a.foot_center();
        let on = fx >= rect[0] && fx <= rect[2] && fy >= rect[1] && fy <= rect[3];
        if phase == 0 {
            if !on {
                let w = walk.get_or_insert_with(|| walk_to(cx - (p.a.foot_w >> 1), cy - (p.a.foot_h >> 1), 1));
                if let Step::Press(m) = w.next(a, slot) {
                    return Step::Press(m);
                }
            }
            phase = 1;
        }
        if !can_act(p) {
            k += 1;
            return if k > 120 { Step::Done("could not act".into()) } else { Step::Press(0) };
        }
        phase += 1;
        match (phase, facing) {
            (2, Some(d)) if p.a.facing as usize != d => Step::Press(DIR_BTN[d]),
            (2 | 3, _) => Step::Press(0),
            (4, _) => Step::Press(A),
            (5..=30, _) => Step::Press(0),
            _ => Step::Done(if on { "no reaction".into() } else { "no reaction (could not reach the trigger)".into() }),
        }
    })
}

pub fn resolve_skill(a: &Agent, slot: usize, r: &Value) -> Result<i32, String> {
    let p = a.player(slot).ok_or("no player")?;
    let d = a.data();
    let list = || p.skills.iter().enumerate().map(|(i, &id)| format!("{i}={}", crate::catalog::skill_name(d, id))).collect::<Vec<_>>().join(", ");
    let s = match r {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return Err(format!("skill: a slot number or a name (this fighter has: {})", list())),
    };
    if let Ok(n) = s.parse::<usize>() {
        if let Some(&id) = p.skills.get(n) {
            return Ok(id);
        }
    }
    let q = crate::catalog::norm(&s);
    let names: Vec<(i32, String)> = p.skills.iter().map(|&id| (id, crate::catalog::norm(&crate::catalog::skill_name(d, id)))).collect();
    names
        .iter()
        .find(|x| x.1 == q)
        .or_else(|| names.iter().find(|x| x.1.contains(&q)))
        .map(|x| x.0)
        .ok_or_else(|| format!("unknown skill \"{s}\" (this fighter has: {})", list()))
}

/// EP cost of a skill at the fighter's skill level (player.rs skill_cost, scaled by the equipment factor)
pub fn skill_cost(a: &Agent, slot: usize, id: i32) -> i32 {
    let Some(p) = a.player(slot) else { return 0 };
    let d = a.data();
    let Some(s) = d.skills.get(&id) else { return 0 };
    let c = if id == 7 {
        ((p.max_ep() as i64 * 0xccd + 65535) >> 16) as i32
    } else if id == 20 {
        10
    } else if matches!(id, 22 | 18 | 12) {
        0
    } else {
        let lv = if s.max_level == 2 { p.skill_level.min(2) } else { 0 };
        s.costs.get((lv as usize).min(s.costs.len().saturating_sub(1))).copied().unwrap_or(s.min_ep)
    };
    (c * p.eff.ep_cost_factor) >> 4
}

pub fn usable(a: &Agent, slot: usize, id: i32) -> bool {
    a.pidx(slot).is_some_and(|i| wf_core::player::can_use_skill(a.w(), i, id))
}

/// cycle L until skill `id` is selected
pub fn select_skill(id: i32) -> Box<dyn Policy> {
    let mut tries = 0;
    let mut up = false;
    let mut wait = 0;
    pol(move |a, slot| {
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        if p.skill() == id && !up {
            return Step::Done(String::new());
        }
        if up {
            up = false;
            return Step::Press(0);
        }
        if !can_act(p) {
            wait += 1;
            if wait > 120 {
                return Step::Done("could_not_select (busy)".into());
            }
            return Step::Press(0);
        }
        if tries >= 12 {
            return Step::Done("could_not_select".into());
        }
        tries += 1;
        up = true;
        Step::Press(L)
    })
}

/// line up on the target's row (horizontal) or column, then stop
pub fn align(eid: u32, max: u32) -> Box<dyn Policy> {
    let mut n = 0;
    pol(move |a, slot| {
        let w = a.w();
        let (Some(p), Some((_, e))) = (a.player(slot), w.enemy_by_id(eid)) else { return Step::Done("target_defeated".into()) };
        if e.defeated {
            return Step::Done("target_defeated".into());
        }
        n += 1;
        if n > max {
            return Step::Done(String::new());
        }
        if !can_act(p) {
            return Step::Press(0);
        }
        let (px, py) = p.a.foot_center();
        let (ex, ey) = e.a.foot_center();
        let (dx, dy) = (ex - px, ey - py);
        if dx.abs() >= dy.abs() {
            if dy.abs() <= 2 {
                return Step::Done(String::new());
            }
            Step::Press(if dy > 0 { DOWN } else { UP })
        } else {
            if dx.abs() <= 2 {
                return Step::Done(String::new());
            }
            Step::Press(if dx > 0 { RIGHT } else { LEFT })
        }
    })
}

pub struct SkillOpts {
    pub charge: u32,
    pub taps: u32,
    pub gap: u32,
    pub target: Option<u32>,
    pub align: bool,
    pub steer: u16,
    pub settle: u32,
}

pub fn use_skill(id: i32, o: SkillOpts) -> Box<dyn Policy> {
    let mut parts = vec![select_skill(id)];
    if let Some(t) = o.target {
        if o.align {
            parts.push(align(t, 120));
        }
        parts.push(face(None, Some(t)));
    }
    parts.push(wait_can_act(60));
    parts.push(pol(move |a, slot| {
        if usable(a, slot, id) {
            Step::Done(String::new())
        } else {
            let p = a.player(slot).unwrap();
            Step::Done(format!("not_usable (EP {}, needs {}{})", p.ep(), skill_cost(a, slot, id), if a.w().g.candy > 0 { "; candy blocks skills" } else { "" }))
        }
    }));
    let mut seq = Vec::new();
    for k in 0..o.taps.max(1) {
        seq.push((B | o.steer, o.charge.max(1)));
        seq.push((0, 1));
        if k + 1 < o.taps && o.gap > 1 {
            seq.push((0, o.gap - 1));
        }
    }
    parts.push(sequence(seq));
    let settle = o.settle;
    let mut n = 0;
    parts.push(pol(move |a, slot| {
        if a.player(slot).is_some_and(can_act) {
            return Step::Done("skill_done".into());
        }
        n += 1;
        if n > settle {
            return Step::Done("skill_still_running".into());
        }
        Step::Press(0)
    }));
    chain(parts)
}

pub fn dash(dir: usize, frames: u32) -> Box<dyn Policy> {
    let mut waited = 0;
    let wait_idle = pol(move |a, slot| {
        let Some(p) = a.player(slot) else { return Step::Done("no player".into()) };
        if (can_act(p) && p.state == PState::Idle) || waited >= 60 {
            return Step::Done(String::new());
        }
        waited += 1;
        Step::Press(0)
    });
    let b = DIR_BTN[dir];
    chain(vec![wait_idle, sequence(vec![(b, 1), (0, 1), (b, frames.max(1)), (0, 1)])])
}

pub fn block(frames: u32, shield: bool) -> Box<dyn Policy> {
    sequence(vec![(R, 1), (R | if shield { B } else { 0 }, frames.saturating_sub(1)), (0, 1)])
}
