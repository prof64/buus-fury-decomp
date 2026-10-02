//! Deep JSON comparison used by the parity tests (numbers compare by value, so `5` == `5.0`).

use serde_json::Value;

/// Paths where `a` and `b` differ (at most `limit` entries).
pub fn json_diff(a: &Value, b: &Value, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    walk(a, b, String::new(), &mut out, limit);
    out
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 120 {
        format!("{}...", &s[..s.char_indices().nth(117).map(|x| x.0).unwrap_or(s.len())])
    } else {
        s
    }
}

fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => walk(v, w, format!("{path}.{k}"), out, limit),
                    None => out.push(format!("{path}.{k}: only in ours = {}", short(v))),
                }
            }
            for (k, w) in y {
                if !x.contains_key(k) {
                    out.push(format!("{path}.{k}: only in reference = {}", short(w)));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: length {} vs reference {}", x.len(), y.len()));
            }
            for (i, (v, w)) in x.iter().zip(y.iter()).enumerate() {
                walk(v, w, format!("{path}[{i}]"), out, limit);
            }
        }
        (Value::Number(x), Value::Number(y)) => {
            if x.as_f64() != y.as_f64() {
                out.push(format!("{path}: {x} vs reference {y}"));
            }
        }
        _ => {
            if a != b {
                out.push(format!("{path}: {} vs reference {}", short(a), short(b)));
            }
        }
    }
}
