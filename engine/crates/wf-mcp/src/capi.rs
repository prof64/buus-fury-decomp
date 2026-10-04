//! C ABI for fast programmatic use (Python ctypes: `python/wf_gym.py`), two layers:
//!
//! * gym fast path on one session: `wf_gym_new(battle JSON)`, `wf_gym_reset(seed)`, `wf_gym_step(mask, frames)`,
//!   `wf_gym_obs_vec` (fixed-size int32 feature vector), `wf_gym_stats`, `wf_gym_snapshot/restore`, `wf_gym_hash`,
//!   `wf_gym_observe` (the JSON observation). Event recording is off unless `"record": true` (speed).
//! * the whole tool API: `wf_call(tool, args JSON)` -> JSON (`{"error": ...}` on failure; images base64).
//!
//! Strings returned are owned by the library and valid until the next call on the same handle / thread.

use crate::env::{can_act, Agent};
use crate::macros::{live_enemies, melee_facings};
use crate::obs::{ai_chain, observe, ObsOpts};
use crate::tools::{base64, battle_config, Out, Server};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use wf_core::battle::{Outcome, RngInit};
use wf_core::world::Body;

thread_local! {
    static ERR: RefCell<CString> = RefCell::new(CString::default());
    static SRV: RefCell<Option<Server>> = const { RefCell::new(None) };
    static OUT: RefCell<CString> = RefCell::new(CString::default());
}

fn set_err(e: String) {
    ERR.with(|x| *x.borrow_mut() = CString::new(e.replace('\0', " ")).unwrap());
}
unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        CStr::from_ptr(p).to_str().unwrap_or("")
    }
}

pub struct GymSnap {
    s: crate::env::Snap,
    inputs: Vec<crate::env::Held>,
}

pub struct Gym {
    pub agent: Agent,
    args: Value,
    out: CString,
}

fn outcome_code(o: Outcome) -> i32 {
    match o {
        Outcome::Running => 0,
        Outcome::Won => 1,
        Outcome::Lost => 2,
    }
}

fn make(args: &Value, seed: Option<u32>) -> Result<Agent, String> {
    let d = crate::content::game_data()?;
    let (mut cfg, label) = battle_config(&d, args)?;
    if let Some(s) = seed {
        cfg.rng = RngInit::Seed(s);
    }
    let mut a = Agent::new("gym".into(), d, cfg, label, args.clone());
    a.record = args.get("record").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(a)
}

/// last error message of this thread
#[no_mangle]
pub extern "C" fn wf_last_error() -> *const c_char {
    ERR.with(|x| x.borrow().as_ptr())
}

/// new session from new_battle arguments (JSON); null on error (see wf_last_error)
/// # Safety
/// `args` must be a valid NUL-terminated string or null.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_new(args: *const c_char) -> *mut Gym {
    let s = cstr(args);
    let v: Value = if s.trim().is_empty() { json!({}) } else {
        match serde_json::from_str(s) {
            Ok(v) => v,
            Err(e) => {
                set_err(format!("args: {e}"));
                return std::ptr::null_mut();
            }
        }
    };
    match make(&v, None) {
        Ok(agent) => Box::into_raw(Box::new(Gym { agent, args: v, out: CString::default() })),
        Err(e) => {
            set_err(e);
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `g` must come from wf_gym_new (or be null).
#[no_mangle]
pub unsafe extern "C" fn wf_gym_free(g: *mut Gym) {
    if !g.is_null() {
        drop(Box::from_raw(g));
    }
}

/// restart the battle with a new seed (seed < 0: the configured one)
/// # Safety
/// `g` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_reset(g: *mut Gym, seed: i64) -> i32 {
    let g = &mut *g;
    match make(&g.args, (seed >= 0).then_some(seed as u32)) {
        Ok(a) => {
            g.agent = a;
            0
        }
        Err(e) => {
            set_err(e);
            -1
        }
    }
}

/// hold `mask` on slot 0 for `frames` frames (other slots: autopilot); returns 0 running, 1 won, 2 lost
/// # Safety
/// `g` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_step(g: *mut Gym, mask: u16, frames: u32) -> i32 {
    let a = &mut (*g).agent;
    for _ in 0..frames.max(1) {
        if a.outcome() != Outcome::Running {
            break;
        }
        a.step(0, mask);
    }
    outcome_code(a.outcome())
}

/// one frame per mask in `masks[0..n]` (slot 0)
/// # Safety
/// `masks` must point to `n` u16.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_step_many(g: *mut Gym, masks: *const u16, n: usize) -> i32 {
    let a = &mut (*g).agent;
    let m = std::slice::from_raw_parts(masks, n);
    for &x in m {
        if a.outcome() != Outcome::Running {
            break;
        }
        a.step(0, x);
    }
    outcome_code(a.outcome())
}

/// one frame with explicit buttons for `n` slots (multiplayer)
/// # Safety
/// `held` must point to `n` u16.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_step_slots(g: *mut Gym, held: *const u16, n: usize) -> i32 {
    let a = &mut (*g).agent;
    let mut h = [0u16; wf_core::world::MAX_PLAYERS];
    for (k, v) in std::slice::from_raw_parts(held, n).iter().enumerate().take(h.len()) {
        h[k] = *v;
    }
    a.step_held(h, 0);
    outcome_code(a.outcome())
}

#[no_mangle]
/// # Safety
/// `g` must be a live handle.
pub unsafe extern "C" fn wf_gym_frame(g: *const Gym) -> u32 {
    (*g).agent.frame()
}
#[no_mangle]
/// # Safety
/// `g` must be a live handle.
pub unsafe extern "C" fn wf_gym_hash(g: *const Gym) -> u64 {
    (*g).agent.s.hash()
}

/// [frame, outcome, hp, hp_max, ep, ep_max, enemies_alive, enemy_hp_total, enemy_hp_max_total, damage_dealt_total,
/// damage_taken_total, kills] -> 12 i64 into `out`
/// # Safety
/// `out` must point to 12 i64.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_stats(g: *const Gym, slot: u32, out: *mut i64) {
    let a = &(*g).agent;
    let o = std::slice::from_raw_parts_mut(out, 12);
    let w = a.w();
    let p = a.player(slot as usize);
    let live = live_enemies(a);
    let (mut hp, mut mx) = (0i64, 0i64);
    for &id in &live {
        let (_, e) = w.enemy_by_id(id).unwrap();
        if e.primary {
            hp += e.hp().max(0);
            mx += e.max_hp1024 / 1024;
        }
    }
    let v = [
        w.frame as i64,
        outcome_code(a.outcome()) as i64,
        p.map(|p| p.hp()).unwrap_or(0) as i64,
        p.map(|p| p.max_hp()).unwrap_or(0) as i64,
        p.map(|p| p.ep()).unwrap_or(0) as i64,
        p.map(|p| p.max_ep()).unwrap_or(0) as i64,
        live.len() as i64,
        hp,
        mx,
        w.stats.damage_dealt,
        w.stats.damage_taken,
        w.stats.kills as i64,
    ];
    o.copy_from_slice(&v);
}

pub const OBS_ENEMIES: usize = 8;
pub const OBS_THREATS: usize = 8;
pub const OBS_LEN: usize = 12 + OBS_ENEMIES * 8 + OBS_THREATS * 6;

/// fixed-size int32 features (length OBS_LEN = 124), zero-padded:
/// [frame, outcome, x, y, facing, hp, hp_max, ep, ep_max, state(0 idle 1 walk 2 dash 3 punch 4 block 5 hurt 6 dying
///  7 skill 8 transform 9 held), can_act, n_enemies,
///  8 nearest enemies x (id, dx, dy, hp, hp_max, melee(-1 none, else the facing 0 down 1 up 2 left 3 right that
///  hits; +4 when it is the current facing), attacking(0 no 1 melee 2 ranged 3 grab 4 hands), stunned),
///  8 nearest hostile shots/hands x (dx, dy, vx, vy, w, h)]
/// # Safety
/// `out` must point to `cap` i32.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_obs_vec(g: *const Gym, slot: u32, out: *mut i32, cap: usize) -> usize {
    let a = &(*g).agent;
    let slot = slot as usize;
    let o = std::slice::from_raw_parts_mut(out, cap);
    let mut v = vec![0i32; OBS_LEN];
    let w = a.w();
    let Some(p) = a.player(slot) else { return 0 };
    let (px, py) = p.a.foot_center();
    v[0] = w.frame as i32;
    v[1] = outcome_code(a.outcome());
    v[2] = px;
    v[3] = py;
    v[4] = p.a.facing as i32;
    v[5] = p.hp();
    v[6] = p.max_hp();
    v[7] = p.ep();
    v[8] = p.max_ep();
    v[9] = p.state as i32;
    v[10] = can_act(p) as i32;
    let mut en: Vec<(i64, u32)> = live_enemies(a)
        .into_iter()
        .map(|id| {
            let (_, e) = w.enemy_by_id(id).unwrap();
            let (x, y) = e.a.foot_center();
            (((x - px) as i64).pow(2) + ((y - py) as i64).pow(2), id)
        })
        .collect();
    en.sort();
    v[11] = en.len() as i32;
    for (k, &(_, id)) in en.iter().take(OBS_ENEMIES).enumerate() {
        let (_, e) = w.enemy_by_id(id).unwrap();
        let (x, y) = e.a.foot_center();
        let mf = melee_facings(a, slot, id);
        let melee = if !e.a.hittable || mf.is_empty() { -1 } else if mf.contains(&(p.a.facing as usize)) { p.a.facing as i32 + 4 } else { mf[0] as i32 };
        let atk = e.ai.stack.last().and_then(|t| ai_chain(&t.tree, &t.rt).1).map(|(kk, _)| {
            let s = format!("{kk:?}");
            match () {
                _ if s.contains("Grab") || s.contains("Hug") || s.contains("Drain") || s.contains("Latch") => 3,
                _ if s.contains("Hand") => 4,
                _ if s.contains("Melee") || s.contains("Punch") || s.contains("Strike") || s.contains("Smash") || s.contains("Lunge") => 1,
                _ if s.contains("Blast") || s.contains("Shot") || s.contains("Shoot") || s.contains("Beam") || s.contains("Throw") || s.contains("Wave") || s.contains("Fire") || s.contains("Flash") => 2,
                _ => 0,
            }
        });
        let b = 12 + k * 8;
        v[b..b + 8].copy_from_slice(&[id as i32, x - px, y - py, e.hp() as i32, (e.max_hp1024 / 1024) as i32, melee, atk.unwrap_or(0), e.stunned as i32]);
    }
    let mut th: Vec<(i64, [i32; 6])> = Vec::new();
    for (i, e) in w.ents.iter().enumerate() {
        if e.removed {
            continue;
        }
        let (r, pos) = match &e.body {
            Body::Shot(s) if s.team != 1 => (wf_core::shots::rect(w, i), (s.px(), s.py())),
            Body::Hand(h) => (h.a.body_box(), (h.a.px(), h.a.py())),
            _ => continue,
        };
        let (cx, cy) = ((r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1);
        let (vx, vy) = a.prev_pos.get(&e.id).map(|q| (pos.0 - q.0, pos.1 - q.1)).unwrap_or((0, 0));
        th.push((((cx - px) as i64).pow(2) + ((cy - py) as i64).pow(2), [cx - px, cy - py, vx, vy, r.x1 - r.x0, r.y1 - r.y0]));
    }
    th.sort_by_key(|t| t.0);
    for (k, t) in th.iter().take(OBS_THREATS).enumerate() {
        let b = 12 + OBS_ENEMIES * 8 + k * 6;
        v[b..b + 6].copy_from_slice(&t.1);
    }
    let n = cap.min(OBS_LEN);
    o[..n].copy_from_slice(&v[..n]);
    n
}

/// JSON observation (flags: 1 full, 2 map, 4 forecast); owned by the handle until the next call
/// # Safety
/// `g` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_observe(g: *mut Gym, slot: u32, flags: u32) -> *const c_char {
    let g = &mut *g;
    let v = observe(&mut g.agent, slot as usize, ObsOpts { full: flags & 1 != 0, map: flags & 2 != 0, forecast: if flags & 4 != 0 { 45 } else { 0 }, drain: true });
    g.out = CString::new(v.to_string()).unwrap();
    g.out.as_ptr()
}

/// # Safety
/// `g` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_snapshot(g: *const Gym) -> *mut GymSnap {
    let a = &(*g).agent;
    Box::into_raw(Box::new(GymSnap { s: a.snap(), inputs: a.inputs.clone() }))
}
/// restore a snapshot (with its input log, so replays stay consistent)
/// # Safety
/// `s` must come from wf_gym_snapshot of the same battle.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_restore(g: *mut Gym, s: *const GymSnap) {
    let s = &*s;
    (*g).agent.restore_with(&s.s, s.inputs.clone());
}
/// # Safety
/// `s` must come from wf_gym_snapshot (or be null).
#[no_mangle]
pub unsafe extern "C" fn wf_gym_snapshot_free(s: *mut GymSnap) {
    if !s.is_null() {
        drop(Box::from_raw(s));
    }
}

/// the replay of the gym session (JSON)
/// # Safety
/// `g` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn wf_gym_replay(g: *mut Gym) -> *const c_char {
    let g = &mut *g;
    g.out = CString::new(serde_json::to_string(&g.agent.replay()).unwrap()).unwrap();
    g.out.as_ptr()
}

/// any tool of the MCP server, in-process: returns JSON (`{"error": msg}` on failure, `{"image_png_base64", ...}`
/// for screenshots). One server (sessions) per thread.
/// # Safety
/// `tool` / `args` must be valid NUL-terminated strings (args may be null).
#[no_mangle]
pub unsafe extern "C" fn wf_call(tool: *const c_char, args: *const c_char) -> *const c_char {
    let name = cstr(tool).to_string();
    let s = cstr(args);
    let v: Value = if s.trim().is_empty() { json!({}) } else { serde_json::from_str(s).unwrap_or(json!({})) };
    let r = SRV.with(|x| {
        let mut x = x.borrow_mut();
        let srv = x.get_or_insert_with(Server::new);
        match srv.call(&name, &v) {
            Ok(Out::Json(v)) => v,
            Ok(Out::Image(png, mut cap)) => {
                cap["image_png_base64"] = json!(base64(&png));
                cap
            }
            Err(e) => json!({"error": e}),
        }
    });
    OUT.with(|o| {
        *o.borrow_mut() = CString::new(r.to_string()).unwrap();
        o.borrow().as_ptr()
    })
}
