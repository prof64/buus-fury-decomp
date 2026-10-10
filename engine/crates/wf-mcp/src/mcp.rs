//! Minimal MCP (Model Context Protocol) over stdio: JSON-RPC 2.0, one message per line. Implements `initialize`,
//! `ping`, `tools/list`, `tools/call` (text + image content); notifications are accepted and ignored.

use crate::tools::{base64, tool_list, Out, Server};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Buu's Fury battle simulator (deterministic Rust core of the GBA game). The game is paused between calls; \
actions run frames (60 = 1 s) and return {result, observation}. Start with new_battle, read the observation, act with attack / \
use_skill / move_to / block / dash / press, explore with save_state / load_state / rewind, encode reflexes in a Rhai controller \
(run_controller, evaluate_controller). Load the buu-battle skill for the full guide.";

fn result(id: &Value, r: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": r})
}
fn error(id: &Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": msg}})
}

/// handle one JSON-RPC message; None for notifications
pub fn handle(srv: &mut Server, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let v = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOL).to_string();
            result(&id, json!({"protocolVersion": v, "capabilities": {"tools": {"listChanged": false}}, "serverInfo": {"name": "buu-battle", "version": env!("CARGO_PKG_VERSION")}, "instructions": INSTRUCTIONS}))
        }
        "ping" => result(&id, json!({})),
        "tools/list" => result(&id, json!({"tools": tool_list()})),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| srv.call(name, &args)));
            match r {
                Ok(Ok(Out::Json(v))) => result(&id, json!({"content": [{"type": "text", "text": serde_json::to_string(&v).unwrap()}], "isError": false})),
                Ok(Ok(Out::Image(png, cap))) => result(&id, json!({"content": [{"type": "image", "data": base64(&png), "mimeType": "image/png"}, {"type": "text", "text": cap.to_string()}], "isError": false})),
                Ok(Err(e)) => result(&id, json!({"content": [{"type": "text", "text": e}], "isError": true})),
                Err(p) => {
                    let m = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    result(&id, json!({"content": [{"type": "text", "text": format!("internal error (panic): {m}")}], "isError": true}))
                }
            }
        }
        "resources/list" => result(&id, json!({"resources": []})),
        "prompts/list" => result(&id, json!({"prompts": []})),
        _ => error(&id, -32601, &format!("method not found: {method}")),
    })
}

/// serve stdin -> stdout until EOF
pub fn serve() {
    let mut srv = Server::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let r: Vec<Value> = batch.iter().filter_map(|m| handle(&mut srv, m)).collect();
                if r.is_empty() {
                    None
                } else {
                    Some(Value::Array(r))
                }
            }
            Ok(m) => handle(&mut srv, &m),
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = reply {
            let _ = writeln!(out, "{}", serde_json::to_string(&r).unwrap());
            let _ = out.flush();
        }
    }
}
