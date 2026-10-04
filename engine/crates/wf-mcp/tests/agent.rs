//! End-to-end checks of the tool API and the MCP framing (needs the ROM at roms/buu.gba, or WF_CONTENT; skipped
//! with a message otherwise): a fight won with macros, replay hash reproduced, time travel, controller sandbox.

use serde_json::{json, Value};
use wf_mcp::tools::{Out, Server};

fn call(s: &mut Server, tool: &str, a: Value) -> Value {
    match s.call(tool, &a) {
        Ok(Out::Json(v)) => v,
        Ok(Out::Image(png, cap)) => {
            assert_eq!(&png[1..4], b"PNG");
            cap
        }
        Err(e) => panic!("{tool}: {e}"),
    }
}

fn content() -> bool {
    match wf_mcp::content::game_data() {
        Ok(_) => true,
        Err(e) => {
            eprintln!("skipped: {e}");
            false
        }
    }
}

#[test]
fn macro_fight_replay_and_time_travel() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    let r = call(&mut s, "new_battle", json!({"fighter": "goku", "level": 80, "enemy": "Majin Fighter", "count": 3, "arena": "forest"}));
    let id = r["session"].as_str().unwrap().to_string();
    call(&mut s, "save_state", json!({"session": id, "name": "start"}));
    let mut o = r["observation"].clone();
    for _ in 0..40 {
        if o["outcome"] != "running" {
            break;
        }
        o = call(&mut s, "attack", json!({"session": id, "stop_on": ["hurt"]}))["observation"].clone();
    }
    assert_eq!(o["outcome"], "won");
    let rp = call(&mut s, "get_replay", json!({"session": id}));
    let chk = call(&mut s, "play_replay", json!({"replay": rp, "keep": false}));
    assert_eq!(chk["deterministic"], true, "{chk}");
    // rewind + replay the same inputs = same hash; load_state goes back to frame 0
    let end = o["frame"].as_u64().unwrap();
    let r = call(&mut s, "rewind", json!({"session": id, "frames": 200}));
    assert_eq!(r["frame"].as_u64().unwrap(), end - 200);
    let r = call(&mut s, "load_state", json!({"session": id, "name": "start"}));
    assert_eq!(r["frame"], 0);
    // screenshot renders
    call(&mut s, "screenshot", json!({"session": id, "view": "gba"}));
}

#[test]
fn controller_sandbox_and_evaluation() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    let id = call(&mut s, "new_battle", json!({"preset": "dabura"}))["session"].as_str().unwrap().to_string();
    let r = call(&mut s, "run_controller", json!({"session": id, "code": "loop {}", "frames": 10}));
    assert_eq!(r["result"]["stopped"], "controller_error");
    assert!(s.call("run_controller", &json!({"session": id, "code": "eval(\"1\")", "frames": 1})).is_err());
    let code = "if obs.enemies.len() == 0 { return \"\"; } let e = obs.enemies[0]; if \"melee\" in e { if e.melee == \"in_range\" { mem.a = !(mem.a == true); return if mem.a { \"A\" } else { \"\" }; } return e.melee.sub_string(5).to_upper(); } obs.suggest.attack";
    let r = call(&mut s, "evaluate_controller", json!({"battle": {"preset": "majin_fighters", "level": 80}, "code": code, "seeds": [1, 2, 3], "frames": 6000}));
    assert_eq!(r["runs"], 3);
    assert_eq!(r["errors"], 0);
    // same seed twice = same hash
    let r2 = call(&mut s, "evaluate_controller", json!({"battle": {"preset": "majin_fighters", "level": 80}, "code": code, "seeds": [1], "frames": 6000}));
    assert_eq!(r["per_seed"][0]["hash"], r2["per_seed"][0]["hash"]);
}

#[test]
fn mcp_framing() {
    let mut s = Server::new();
    let r = wf_mcp::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})).unwrap();
    assert_eq!(r["result"]["serverInfo"]["name"], "buu-battle");
    assert!(wf_mcp::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
    let r = wf_mcp::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    assert!(r["result"]["tools"].as_array().unwrap().len() >= 25);
    let r = wf_mcp::mcp::handle(&mut s, &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "observe", "arguments": {"session": "nope"}}})).unwrap();
    assert_eq!(r["result"]["isError"], true);
}

fn tap_a(s: &mut Server, id: &str) -> Value {
    call(s, "press", json!({"session": id, "steps": [{"buttons": "A", "frames": 1}, {"buttons": "", "frames": 5}]}))["observation"].clone()
}

/// story sessions: the title through New Game, the prologue's dialogue to Snakeway, an exit taken (an area load:
/// `World.frame` stands still there, the session's frame counts steps), rewind / save / load / replay exact
#[test]
fn story_new_game_time_travel_and_replay() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    let r = call(&mut s, "new_game", json!({"seed": 1}));
    let id = r["session"].as_str().unwrap().to_string();
    assert!(r["title_steps"].as_u64().unwrap() > 0);
    // the prologue: A through its boxes until the leader is free in Snakeway
    let mut o = r["observation"].clone();
    for _ in 0..3000 {
        let st = &o["story"];
        if st.get("dialog").is_some() || st.get("menu").is_some() {
            o = tap_a(&mut s, &id);
        } else if st["event_running"] == true || o["player"].is_null() {
            o = call(&mut s, "wait", json!({"session": id, "frames": 10}))["observation"].clone();
        } else {
            break;
        }
    }
    assert_eq!(o["field"]["area"], "1-1", "{o}");
    let f0 = o["frame"].as_u64().unwrap();
    call(&mut s, "save_state", json!({"session": id, "name": "snakeway"}));
    let h0 = call(&mut s, "get_replay", json!({"session": id}))["hash"].clone();
    // the exit east (an event stops the macro first: its boxes, then the exit again)
    for _ in 0..200 {
        let r = call(&mut s, "take_exit", json!({"session": id, "to": "1-2"}));
        if r["observation"]["field"]["area"] == "1-2" {
            break;
        }
        if r["observation"]["story"].get("dialog").is_some() {
            tap_a(&mut s, &id);
        } else {
            call(&mut s, "wait", json!({"session": id, "frames": 10}));
        }
    }
    let o = call(&mut s, "observe", json!({"session": id}));
    assert_eq!(o["field"]["area"], "1-2");
    let rep = call(&mut s, "get_replay", json!({"session": id}));
    assert_eq!(rep["frames"], o["frame"]);
    let r = call(&mut s, "play_replay", json!({"replay": rep, "keep": false}));
    assert_eq!(r["deterministic"], true, "{r}");
    // rewind across the load to the save's step: the saved state's hash
    let r = call(&mut s, "rewind", json!({"session": id, "to_frame": f0}));
    assert_eq!(r["frame"].as_u64().unwrap(), f0);
    assert_eq!(call(&mut s, "get_replay", json!({"session": id}))["hash"], h0);
    call(&mut s, "press", json!({"session": id, "buttons": "RIGHT", "frames": 100}));
    call(&mut s, "load_state", json!({"session": id, "name": "snakeway"}));
    assert_eq!(call(&mut s, "get_replay", json!({"session": id}))["hash"], h0);
}
