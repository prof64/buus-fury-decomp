//! Online multiplayer in the browser (wf-net). JS owns the WebSocket to the signaling server and the
//! RTCPeerConnections / data channels (engine/web/app/net.js, same protocol and channel setup as Matchbox, so
//! browser and native peers meet in one room); this side runs the lobby and the rollback session on a
//! `QueueTransport` that JS fills (`wf_net_recv`, `wf_net_peer`) and drains (`wf_net_out`).
//!
//! Flow: `wf_net_lobby(cfg)` -> per animation frame `wf_net_poll()` (+ `wf_net_out` loop) until it returns 1 ->
//! `wf_app_init({"net": true, ...})` runs the match (`NetSim`) through the normal app loop; `wf_net_status()` for
//! the overlay, `wf_net_report()` for a desync report. `{"mode": "synctest", "synctest": 8}` = local sync test.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wf_app::wf_content::Packs;
use wf_net::cli::{NetArgs, NetMode};
use wf_net::netsim::{content_id, current_status, Clock, NetLobby};
use wf_net::transport::{Conditioner, PeerChange, QueueTransport, Transport};
use wf_view::Sim;

use super::{arg_str, host_packs, now_ms, set_str, STR};

struct Net {
    args: NetArgs,
    battle: wf_app::battle::BattleConfig,
    data: Arc<wf_net::wf_core::content::GameData>,
    q: QueueTransport,
    lobby: Option<NetLobby>,
}

thread_local! {
    static NET: RefCell<Option<Net>> = const { RefCell::new(None) };
}

fn clock() -> Clock {
    Rc::new(|| unsafe { now_ms() })
}

/// Prepare an online match (or a sync test). `cfg` JSON: `mode` host | join | spectate | synctest, `room`,
/// `players`, `pvp`, `pvp_enemies`, `delay`, `prediction`, `latency`, `jitter`, `loss`, `net_seed`, `name`,
/// `synctest` (distance), and the battle: `arena`, `enemy`, `count`, `fighter`, `form`, `level`, `seed`.
/// Returns 0 (room code in the string buffer) or 1 (error text).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_net_lobby(ptr: *const u8, len: usize) -> u32 {
    let cfg: serde_json::Value = serde_json::from_str(&unsafe { arg_str(ptr, len) }).unwrap_or_default();
    let s = |k: &str| cfg[k].as_str().map(String::from).or_else(|| cfg[k].as_f64().map(|v| v.to_string())).or_else(|| cfg[k].as_bool().filter(|b| *b).map(|_| String::new()));
    let mut args = NetArgs::default();
    let mut set = |flag: &str, v: Option<String>| {
        if let Some(v) = v {
            let mut once = Some(v);
            args.flag(flag, &mut || once.take().unwrap_or_default());
        }
    };
    match cfg["mode"].as_str().unwrap_or("host") {
        "join" => set("--net-join", s("room")),
        "spectate" => set("--net-spectate", s("room")),
        "synctest" => set("--net-synctest", s("synctest").or(Some("8".into()))),
        _ => {
            set("--net-host", Some(String::new()));
            set("--room", s("room").filter(|r| !r.is_empty()));
        }
    }
    for (k, f) in [
        ("players", "--net-players"),
        ("delay", "--net-delay"),
        ("prediction", "--net-prediction"),
        ("latency", "--net-latency"),
        ("jitter", "--net-jitter"),
        ("loss", "--net-loss"),
        ("net_seed", "--net-seed"),
        ("name", "--net-name"),
    ] {
        set(f, s(k));
    }
    if cfg["pvp"].as_bool() == Some(true) {
        set("--pvp", Some(String::new()));
    }
    if cfg["pvp_enemies"].as_bool() == Some(true) {
        set("--pvp-enemies", Some(String::new()));
    }
    if cfg["synctest_bytes"].as_bool() == Some(true) {
        set("--net-synctest-bytes", Some(String::new()));
    }
    let st = |k: &str, d: &str| cfg[k].as_str().unwrap_or(d).to_string();
    let battle = wf_app::battle::config(
        &st("arena", "forest"),
        &st("enemy", "t68"),
        cfg["count"].as_u64().unwrap_or(1) as u32,
        &st("fighter", "goku"),
        cfg["form"].as_i64().map(|v| v as i32),
        cfg["level"].as_i64().map(|v| v as i32),
        if matches!(args.mode, NetMode::SyncTest(_)) { cfg["players"].as_u64().unwrap_or(1).clamp(1, 4) as usize } else { 1 },
        cfg["seed"].as_u64().unwrap_or(1) as u32,
    );
    let packs: Packs = match host_packs() {
        Ok(p) => p,
        Err(e) => {
            set_str(&e.to_string());
            return 1;
        }
    };
    let data = match wf_app::battle::game_data(&packs) {
        Ok(d) => d,
        Err(e) => {
            set_str(&e);
            return 1;
        }
    };
    let q = QueueTransport::default();
    let mut lobby = None;
    let mut code = String::new();
    if !matches!(args.mode, NetMode::SyncTest(_)) {
        code = args.room_code(unsafe { now_ms() }.to_bits() ^ 0x9E37_79B9_7F4A_7C15);
        let t: Box<dyn Transport> = if args.cond != Default::default() { Box::new(Conditioner::new(q.clone(), args.cond, args.seed)) } else { Box::new(q.clone()) };
        lobby = Some(NetLobby::new(t, args.lobby_settings(&battle, content_id(&data)), &code, clock()));
    }
    set_str(&code);
    NET.with(|n| *n.borrow_mut() = Some(Net { args, battle, data, q, lobby }));
    0
}

/// Our id on the signaling server (`IdAssigned`).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_net_set_id(ptr: *const u8, len: usize) {
    let id = unsafe { arg_str(ptr, len) };
    NET.with(|n| {
        if let Some(n) = n.borrow().as_ref() {
            n.q.0.borrow_mut().id = Some(id);
        }
    });
}

/// A peer's data channel opened (`up` = 1) or closed (0).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_net_peer(ptr: *const u8, len: usize, up: u32) {
    let id = unsafe { arg_str(ptr, len) };
    NET.with(|n| {
        if let Some(n) = n.borrow().as_ref() {
            n.q.0.borrow_mut().changes.push(if up != 0 { PeerChange::Connected(id) } else { PeerChange::Disconnected(id) });
        }
    });
}

/// A datagram from peer `id`.
///
/// # Safety
/// Pointers must describe readable memory.
#[no_mangle]
pub unsafe extern "C" fn wf_net_recv(ip: *const u8, il: usize, dp: *const u8, dl: usize) {
    let id = unsafe { arg_str(ip, il) };
    let data = if dl == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(dp, dl) }.to_vec() };
    NET.with(|n| {
        if let Some(n) = n.borrow().as_ref() {
            n.q.0.borrow_mut().inbox.push_back((id, data));
        }
    });
}

/// Next datagram to send: returns its total length (0 = none); the string buffer holds `[peer id length u8][peer
/// id][payload]`.
#[no_mangle]
pub extern "C" fn wf_net_out() -> u32 {
    let next = NET.with(|n| n.borrow().as_ref().and_then(|n| n.q.0.borrow_mut().outbox.pop_front()));
    match next {
        Some((to, d)) => {
            let mut b = Vec::with_capacity(1 + to.len() + d.len());
            b.push(to.len().min(255) as u8);
            b.extend_from_slice(&to.as_bytes()[..to.len().min(255)]);
            b.extend_from_slice(&d);
            let n = b.len() as u32;
            STR.with(|s| *s.borrow_mut() = b);
            n
        }
        None => 0,
    }
}

/// Lobby step: 0 = waiting, 1 = the match starts (call `wf_app_init` with `"net": true`), 2 = refused / error.
/// The status JSON is in the string buffer.
#[no_mangle]
pub extern "C" fn wf_net_poll() -> u32 {
    let r = NET.with(|n| {
        let mut n = n.borrow_mut();
        let Some(n) = n.as_mut() else { return 2 };
        if matches!(n.args.mode, NetMode::SyncTest(_)) {
            return 1;
        }
        match &mut n.lobby {
            Some(l) => {
                if l.poll() {
                    if l.lobby.rejected.is_some() {
                        2
                    } else {
                        1
                    }
                } else {
                    0
                }
            }
            None => 1,
        }
    });
    wf_net_status();
    r
}

/// Status JSON (phase, room, roster, slot, frame, rollbacks, rtt, desync, ...) in the string buffer.
#[no_mangle]
pub extern "C" fn wf_net_status() {
    let s = current_status().map(|s| s.borrow().to_json()).unwrap_or_else(|| "{}".into());
    set_str(&s);
}

/// The desync report JSON (empty if none) in the string buffer.
#[no_mangle]
pub extern "C" fn wf_net_report() {
    let s = current_status().and_then(|s| s.borrow().desync_report.clone()).unwrap_or_default();
    set_str(&s);
}

/// Local player slot (-1 = spectator / none).
#[no_mangle]
pub extern "C" fn wf_net_slot() -> i32 {
    current_status().and_then(|s| s.borrow().local_slot).map(|s| s as i32).unwrap_or(-1)
}

/// For `wf_app_init` with `"net": true`: the prepared match (or sync test) as the app's simulation.
pub(crate) fn take_sim(cfg: &serde_json::Value) -> Option<Result<Box<dyn Sim>, String>> {
    if cfg["net"].as_bool() != Some(true) {
        return None;
    }
    Some(NET.with(|n| {
        let mut n = n.borrow_mut();
        let n = n.as_mut().ok_or("wf_net_lobby was not called")?;
        if let NetMode::SyncTest(d) = n.args.mode {
            return Ok(Box::new(wf_net::cli::synctest_sim(&n.args, n.data.clone(), &n.battle, d)) as Box<dyn Sim>);
        }
        let l = n.lobby.take().ok_or("the match was already started")?;
        l.into_sim(n.data.clone()).map(|s| Box::new(s) as Box<dyn Sim>)
    }))
}
