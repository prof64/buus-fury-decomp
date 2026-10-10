//! ICE configuration: STUN / TURN servers for WebRTC (native and browser), and the TURN "REST API" credentials
//! (time-limited username + HMAC password from a secret shared with the TURN server: coturn's
//! `use-auth-secret` / `static-auth-secret`, `wf-turn --secret`) that `wf-signal --turn-secret` hands out at
//! `GET /ice`.
//!
//! * Peers on one machine / LAN: no servers needed (host candidates).
//! * Across the internet: STUN finds each peer's public address (most home NATs); peers behind symmetric NATs /
//!   strict firewalls need a TURN relay. `IceConfig::relay_only` forces the relay (testing, privacy).

use serde::{Deserialize, Serialize};

/// One `RTCIceServer`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IceConfig {
    #[serde(rename = "iceServers", default)]
    pub servers: Vec<IceServer>,
    /// only relay (TURN) candidates: every byte goes through the TURN server
    #[serde(default, rename = "relayOnly")]
    pub relay_only: bool,
}

pub const PUBLIC_STUN: &str = "stun:stun.l.google.com:19302";

impl IceConfig {
    pub fn is_empty(&self) -> bool {
        self.servers.iter().all(|s| s.urls.is_empty())
    }
    pub fn stun(url: &str) -> IceConfig {
        IceConfig { servers: vec![IceServer { urls: vec![url.to_string()], ..Default::default() }], relay_only: false }
    }
    /// Parse `{"iceServers": [...]}` (the WebRTC / `GET /ice` shape), a bare array of servers, or one server.
    pub fn from_json(s: &str) -> Result<IceConfig, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("ICE config: {e}"))?;
        let servers = if v.is_array() {
            v.clone()
        } else if v.get("iceServers").is_some() {
            v["iceServers"].clone()
        } else {
            serde_json::Value::Array(vec![v.clone()])
        };
        let mut out = IceConfig { relay_only: v.get("relayOnly").and_then(|b| b.as_bool()).unwrap_or(false), servers: Vec::new() };
        for s in servers.as_array().cloned().unwrap_or_default() {
            let urls = match &s["urls"] {
                serde_json::Value::String(u) => vec![u.clone()],
                serde_json::Value::Array(a) => a.iter().filter_map(|u| u.as_str().map(String::from)).collect(),
                _ => match s["url"].as_str() {
                    Some(u) => vec![u.to_string()],
                    None => Vec::new(),
                },
            };
            if urls.is_empty() {
                continue;
            }
            out.servers.push(IceServer { urls, username: s["username"].as_str().map(String::from), credential: s["credential"].as_str().map(String::from) });
        }
        Ok(out)
    }
    /// The browser's `RTCConfiguration` fields as JSON: `{"iceServers": [...], "iceTransportPolicy": "relay"|"all"}`.
    pub fn to_rtc_json(&self) -> String {
        serde_json::json!({ "iceServers": self.servers, "iceTransportPolicy": if self.relay_only { "relay" } else { "all" } }).to_string()
    }
    /// Add `--net-turn-user` / `--net-turn-pass` to every TURN url without credentials.
    pub fn with_credentials(mut self, user: &str, pass: &str) -> Self {
        if user.is_empty() && pass.is_empty() {
            return self;
        }
        for s in &mut self.servers {
            if s.urls.iter().any(|u| u.starts_with("turn")) && s.username.is_none() {
                s.username = Some(user.to_string());
                s.credential = Some(pass.to_string());
            }
        }
        self
    }
    /// One server entry for clients that take a single `RTCIceServer` (matchbox natively): every url, the first
    /// credentials. Returns a warning when servers with different credentials had to be merged.
    pub fn merged(&self) -> (IceServer, Option<String>) {
        let mut m = IceServer::default();
        let mut warn = None;
        for s in &self.servers {
            m.urls.extend(s.urls.iter().cloned());
            if s.username.is_some() {
                if m.username.is_none() {
                    m.username = s.username.clone();
                    m.credential = s.credential.clone();
                } else if m.username != s.username || m.credential != s.credential {
                    warn = Some("several TURN servers with different credentials: using the first's for all".to_string());
                }
            }
        }
        (m, warn)
    }
    pub fn has_turn(&self) -> bool {
        self.servers.iter().any(|s| s.urls.iter().any(|u| u.starts_with("turn:") || u.starts_with("turns:")))
    }
}

/// TURN REST API credentials (draft-uberti-behave-turn-rest, coturn `use-auth-secret`): username
/// `<unix expiry>:<user>`, password `base64(HMAC-SHA1(secret, username))`.
pub fn turn_rest_credentials(secret: &str, user: &str, ttl_s: u64, now_unix: u64) -> (String, String) {
    let username = if user.is_empty() { format!("{}", now_unix + ttl_s) } else { format!("{}:{}", now_unix + ttl_s, user) };
    let pass = base64(&hmac_sha1(secret.as_bytes(), username.as_bytes()));
    (username, pass)
}

/// The expiry of a REST API username (`<expiry>` or `<expiry>:<user>`).
pub fn rest_expiry(username: &str) -> Option<u64> {
    username.split(':').next()?.parse().ok()
}

pub fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    use sha1::{Digest, Sha1};
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..20].copy_from_slice(&Sha1::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha1::new();
    inner.update(k.iter().map(|b| b ^ 0x36).collect::<Vec<u8>>());
    inner.update(msg);
    let ih = inner.finalize();
    let mut outer = Sha1::new();
    outer.update(k.iter().map(|b| b ^ 0x5c).collect::<Vec<u8>>());
    outer.update(ih);
    outer.finalize().into()
}

pub fn base64(b: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// The http(s) origin of a ws(s) signaling URL (`wss://h:443/x` -> `https://h:443`), for `GET /ice`.
pub fn http_origin(signal: &str) -> Option<String> {
    let (scheme, rest) = signal.split_once("://")?;
    let scheme = match scheme {
        "ws" | "http" => "http",
        "wss" | "https" => "https",
        _ => return None,
    };
    let host = rest.split('/').next()?;
    Some(format!("{scheme}://{host}"))
}

/// Is this signaling URL on this machine (then no STUN is needed)?
pub fn is_local(signal: &str) -> bool {
    ["ws://127.0.0.1", "ws://localhost", "ws://[::1]", "wss://127.0.0.1", "wss://localhost"].iter().any(|p| signal.starts_with(p))
}

/// Keep only relay candidates in an SDP (relay-only mode against peers that embed every candidate).
pub fn sdp_relay_only(sdp: &str) -> String {
    let nl = if sdp.contains("\r\n") { "\r\n" } else { "\n" };
    sdp.split(nl).filter(|l| !l.starts_with("a=candidate:") || l.contains(" typ relay")).collect::<Vec<_>>().join(nl)
}

/// Is this (JSON or SDP-line) ICE candidate a relay candidate? (`null` / end-of-candidates counts as yes)
pub fn candidate_is_relay(c: &str) -> bool {
    !c.contains("candidate:") || c.contains(" typ relay")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hmac_and_base64() {
        // RFC 2202 test case 2
        let h = hmac_sha1(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(h.iter().map(|b| format!("{b:02x}")).collect::<String>(), "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        // the same password coturn computes for user "1700000000:wf" with secret "s3cret"
        let (u, p) = turn_rest_credentials("s3cret", "wf", 100, 1_699_999_900);
        assert_eq!(u, "1700000000:wf");
        assert_eq!(p, base64(&hmac_sha1(b"s3cret", b"1700000000:wf")));
        assert_eq!(rest_expiry(&u), Some(1_700_000_000));
    }
    #[test]
    fn parse_configs() {
        let c = IceConfig::from_json(r#"{"iceServers":[{"urls":"stun:a:3478"},{"urls":["turn:a:3478","turns:a:5349"],"username":"u","credential":"p"}]}"#).unwrap();
        assert_eq!(c.servers.len(), 2);
        assert!(c.has_turn());
        let (m, w) = c.merged();
        assert_eq!(m.urls.len(), 3);
        assert_eq!(m.username.as_deref(), Some("u"));
        assert!(w.is_none());
        assert!(c.to_rtc_json().contains("\"iceTransportPolicy\":\"all\""));
        assert_eq!(http_origin("wss://sig.example.com/wf-AB").as_deref(), Some("https://sig.example.com"));
        assert_eq!(http_origin("ws://1.2.3.4:3536").as_deref(), Some("http://1.2.3.4:3536"));
        let sdp = "v=0\r\na=candidate:1 1 udp 1 10.0.0.1 5000 typ host\r\na=candidate:2 1 udp 1 1.2.3.4 6000 typ relay raddr 0.0.0.0 rport 0\r\nm=x";
        assert_eq!(sdp_relay_only(sdp), "v=0\r\na=candidate:2 1 udp 1 1.2.3.4 6000 typ relay raddr 0.0.0.0 rport 0\r\nm=x");
        assert!(!candidate_is_relay(r#"{"candidate":"candidate:1 1 udp 1 10.0.0.1 5000 typ host"}"#));
        assert!(candidate_is_relay("null"));
    }
}
