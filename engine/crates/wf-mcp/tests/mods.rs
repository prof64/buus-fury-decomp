//! Mods through the agent tools (engine/docs/modding.md §7 "Testing mods with agents"): sessions built from the base
//! pack + mod folders, the mod checker as a tool, the mods' content listed, replays that record their mods and
//! refuse other content. Headless, muted; needs the ROM (roms/buu.gba) or WF_CONTENT, skipped otherwise.

use serde_json::{json, Value};
use wf_mcp::tools::{Out, Server};

fn call(s: &mut Server, tool: &str, a: Value) -> Result<Value, String> {
    match s.call(tool, &a) {
        Ok(Out::Json(v)) => Ok(v),
        Ok(Out::Image(png, cap)) => {
            assert_eq!(&png[1..4], b"PNG");
            Ok(cap)
        }
        Err(e) => Err(e),
    }
}

fn ok(s: &mut Server, tool: &str, a: Value) -> Value {
    call(s, tool, a).unwrap_or_else(|e| panic!("{tool}: {e}"))
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

fn ex(name: &str) -> String {
    format!("{}/../../mods-examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> String {
    format!("{}/../wf-app/tests/modcheck_fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// a talking NPC within 40 px of the leader
fn talker_near(o: &Value) -> bool {
    o["field"]["npcs"].as_array().is_some_and(|v| v.iter().any(|n| n["talks"] == true && n["dist"].as_i64().unwrap_or(999) < 40))
}

#[test]
fn modcheck_tool_reports_problems() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    let good = ok(&mut s, "modcheck", json!({"mods": [ex("story-tweaks")]}));
    assert_eq!(good["ok"], true, "{good}");
    assert_eq!(good["exit_status"], 0);
    let bad = ok(&mut s, "modcheck", json!({"mods": [fixture("item_sold_at_bad")]}));
    assert_eq!(bad["ok"], false, "{bad}");
    assert_eq!(bad["exit_status"], 1);
    let e = &bad["errors"][0];
    assert!(e["file"].as_str().is_some_and(|f| !f.is_empty()) && e["what"].as_str().is_some() && e["hint"].as_str().is_some_and(|h| !h.is_empty()), "{bad}");
    assert!(bad["errors"].as_array().unwrap().iter().any(|e| e["what"].as_str().unwrap_or("").contains("sold_at") || e["file"].as_str().unwrap_or("").contains("items/")), "{bad}");
    // no folders and no WF_MODS: a clear error, not a crash
    if std::env::var_os("WF_MODS").is_none() {
        assert!(call(&mut s, "modcheck", json!({})).unwrap_err().contains("mods"));
    }
}

#[test]
fn mod_content_listed_and_visible_in_sessions() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    let m = vec![ex("rhai-story-event")];
    // what the mod adds: the new skill (27, Dodon Ray), its events, the area addition
    let c = ok(&mut s, "list_mod_content", json!({"mods": m}));
    let mc = &c["mods"][0];
    assert_eq!(mc["id"], "rhai-story-event", "{c}");
    let skills = mc["content"]["skills"].as_array().expect("skills listed");
    assert!(skills.iter().any(|x| x["id"]["skill"] == 27 && x["how"] == "added"), "{c}");
    assert!(mc["content"]["events"].as_array().is_some_and(|v| v.iter().any(|x| x["file"] == "scripts/events/s930.rhai")), "{c}");
    assert!(mc["content"]["areas"].is_array(), "{c}");
    // a field session in 8-3 by the spot: Chiaotzu stands there with the mod, not without
    let at = json!({"area": "8-3", "x": 344, "y": 520});
    let mut a = at.clone();
    a["mods"] = json!(m);
    let r = ok(&mut s, "new_field", a);
    assert_eq!(r["mods"], json!(["rhai-story-event"]));
    assert!(talker_near(&r["observation"]), "{}", r["observation"]["field"]);
    let mut v = at.clone();
    v["mods"] = json!([]);
    let r0 = ok(&mut s, "new_field", v);
    assert!(!talker_near(&r0["observation"]), "vanilla has nobody there: {}", r0["observation"]["field"]);
    // the sessions list names the mods; a screenshot renders with the mod's art
    let id = r["session"].as_str().unwrap().to_string();
    let l = ok(&mut s, "list_sessions", json!({}));
    assert!(l.as_array().unwrap().iter().any(|x| x["session"] == id.as_str() && x["mods"] == json!(["rhai-story-event"])), "{l}");
    ok(&mut s, "screenshot", json!({"session": id, "view": "gba"}));
}

#[test]
fn story_with_a_file_only_mod() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    // a new game with story-tweaks: Goku starts with the patched STR 40 (vanilla 27) once the prologue lets him go
    let r = ok(&mut s, "new_game", json!({"seed": 1, "mods": [ex("story-tweaks")]}));
    assert_eq!(r["mods"], json!(["story-tweaks"]));
    let id = r["session"].as_str().unwrap().to_string();
    let mut o = r["observation"].clone();
    for _ in 0..3000 {
        let st = &o["story"];
        if st.get("dialog").is_some() || st.get("menu").is_some() {
            o = ok(&mut s, "press", json!({"session": id, "steps": [{"buttons": "A", "frames": 1}, {"buttons": "", "frames": 5}]}))["observation"].clone();
        } else if st["event_running"] == true || o["player"].is_null() {
            o = ok(&mut s, "wait", json!({"session": id, "frames": 10}))["observation"].clone();
        } else {
            break;
        }
    }
    let full = ok(&mut s, "observe", json!({"session": id, "full": true}));
    assert_eq!(full["player"]["stats"]["STR"], 40, "{}", full["player"]);
}

#[test]
fn replays_record_their_mods() {
    if !content() {
        return;
    }
    let mut s = Server::new();
    let m = vec![ex("rhai-story-event")];
    let r = ok(&mut s, "new_field", json!({"area": "8-3", "x": 344, "y": 520, "mods": m}));
    let id = r["session"].as_str().unwrap().to_string();
    ok(&mut s, "press", json!({"session": id, "steps": [{"buttons": "LEFT", "frames": 40}, {"buttons": "UP", "frames": 30}], "stop_on": []}));
    let rep = ok(&mut s, "get_replay", json!({"session": id}));
    assert_eq!(rep["content"]["mods"], json!(["rhai-story-event"]), "{}", rep["content"]);
    assert!(rep["mod_dirs"].as_array().is_some_and(|v| v.len() == 1));
    // the replay loads its own mod folders: exact
    let chk = ok(&mut s, "play_replay", json!({"replay": rep, "keep": false}));
    assert_eq!(chk["deterministic"], true, "{chk}");
    // other content: refused with both mod lists named
    let e = call(&mut s, "play_replay", json!({"replay": rep, "keep": false, "mods": []})).unwrap_err();
    assert!(e.contains("made with mods: rhai-story-event") && e.contains("loaded now: none"), "{e}");
    let e = call(&mut s, "play_replay", json!({"replay": rep, "keep": false, "mods": [ex("story-tweaks")]})).unwrap_err();
    assert!(e.contains("loaded now: story-tweaks"), "{e}");
    // a vanilla replay refuses mods too
    let v = ok(&mut s, "new_battle", json!({"fighter": "goku", "enemy": "t68", "mods": []}));
    let vid = v["session"].as_str().unwrap().to_string();
    ok(&mut s, "wait", json!({"session": vid, "frames": 30}));
    let vrep = ok(&mut s, "get_replay", json!({"session": vid}));
    assert!(vrep.get("content").is_none(), "vanilla replays stay as before: {vrep}");
    assert!(call(&mut s, "play_replay", json!({"replay": vrep, "keep": false, "mods": m})).unwrap_err().contains("made with mods: none"));
    assert_eq!(ok(&mut s, "play_replay", json!({"replay": vrep, "keep": false}))["deterministic"], true);
}
