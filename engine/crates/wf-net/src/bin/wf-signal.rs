//! `wf-signal [ADDR] [options]` — the Matchbox full-mesh signaling server for Buu's Fury rooms (default
//! 127.0.0.1:3536). Peers connect to `ws://ADDR/wf-<ROOM CODE>[?token=…&key=…]`; the server only relays WebRTC
//! offers / answers / ICE candidates between the peers of a room. Game traffic never passes through it.
//!
//! Internet deployment (engine/deploy/netplay/): behind a TLS reverse proxy (Caddy) for `wss://`, next to a TURN
//! server (coturn or `wf-turn`).
//!
//! Options (also as environment variables):
//!   --key KEY            (WF_SIGNAL_KEY)    every connection and `/ice` request must carry `?key=KEY`
//!   --require-token      (WF_REQUIRE_TOKEN=1) refuse rooms without a room token
//!   --max-room N         (WF_MAX_ROOM, 8)   peers per room (4 players + spectators)
//!   --max-peers N        (WF_MAX_PEERS, 2000) connections in all
//!   --turn-url URL       (WF_TURN_URLS, comma separated) TURN urls handed out at /ice, e.g. turn:turn.example.com:3478
//!   --turn-secret S      (WF_TURN_SECRET)   coturn `static-auth-secret` / `wf-turn --secret`: /ice mints
//!                                           time-limited credentials (TURN REST API)
//!   --stun-url URL       (WF_STUN_URLS)     STUN urls handed out at /ice (default: stun: on the TURN hosts)
//!   --turn-ttl SECONDS   (WF_TURN_TTL, 86400)
//!   --tls ADDR --tls-cert FILE --tls-key FILE  (WF_TLS_ADDR, WF_TLS_CERT, WF_TLS_KEY) also serve wss:// / https://
//!                        on ADDR with this PEM certificate (a reverse proxy like Caddy is the usual way; this is
//!                        for a VPS without one and for local tests with a self-signed certificate)
//!
//! Rooms: the first peer of a room fixes its token (`?token=`); later peers must present the same one; the room
//! forgets it when the last peer leaves. Routes: `/{room}` (WebSocket), `/ice` (JSON `{"iceServers": [...]}`),
//! `/healthz`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::Query;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use matchbox_signaling::SignalingServer;
use wf_net::ice::{turn_rest_credentials, IceConfig, IceServer};

#[derive(Clone, Debug, Default)]
struct Config {
    key: String,
    require_token: bool,
    max_room: usize,
    max_peers: usize,
    turn_urls: Vec<String>,
    stun_urls: Vec<String>,
    turn_secret: String,
    turn_ttl: u64,
    tls: Option<(String, String, String)>,
}

#[derive(Default)]
struct Rooms {
    /// room -> (token, peers)
    rooms: HashMap<String, (String, usize)>,
    /// connection origin -> room (between the upgrade check and the id assignment)
    pending: HashMap<SocketAddr, String>,
    /// peer -> room
    peers: HashMap<String, String>,
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn split(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

fn parse() -> (SocketAddr, Config) {
    let mut c = Config {
        key: env("WF_SIGNAL_KEY").unwrap_or_default(),
        require_token: env("WF_REQUIRE_TOKEN").is_some_and(|v| v != "0"),
        max_room: env("WF_MAX_ROOM").and_then(|v| v.parse().ok()).unwrap_or(8),
        max_peers: env("WF_MAX_PEERS").and_then(|v| v.parse().ok()).unwrap_or(2000),
        turn_urls: env("WF_TURN_URLS").map(|v| split(&v)).unwrap_or_default(),
        stun_urls: env("WF_STUN_URLS").map(|v| split(&v)).unwrap_or_default(),
        turn_secret: env("WF_TURN_SECRET").unwrap_or_default(),
        turn_ttl: env("WF_TURN_TTL").and_then(|v| v.parse().ok()).unwrap_or(86_400),
        tls: None,
    };
    let (mut tls_addr, mut tls_cert, mut tls_key) = (env("WF_TLS_ADDR"), env("WF_TLS_CERT").unwrap_or_default(), env("WF_TLS_KEY").unwrap_or_default());
    let mut addr = env("WF_SIGNAL_ADDR").unwrap_or_else(|| "127.0.0.1:3536".into());
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let arg = v[i].clone();
        let mut next = || {
            i += 1;
            v.get(i).cloned().unwrap_or_default()
        };
        match arg.as_str() {
            "--key" => c.key = next(),
            "--require-token" => c.require_token = true,
            "--max-room" => c.max_room = next().parse().unwrap_or(8),
            "--max-peers" => c.max_peers = next().parse().unwrap_or(2000),
            "--turn-url" => c.turn_urls.extend(split(&next())),
            "--stun-url" => c.stun_urls.extend(split(&next())),
            "--turn-secret" => c.turn_secret = next(),
            "--turn-ttl" => c.turn_ttl = next().parse().unwrap_or(86_400),
            "--tls" => tls_addr = Some(next()),
            "--tls-cert" => tls_cert = next(),
            "--tls-key" => tls_key = next(),
            "-h" | "--help" => {
                println!("{}", include_str!("wf-signal.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim_start_matches(' ')).collect::<Vec<_>>().join("\n"));
                std::process::exit(0);
            }
            a if !a.starts_with('-') => addr = a.to_string(),
            a => {
                eprintln!("wf-signal: unknown option {a} (--help)");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    if let Some(a) = tls_addr {
        if tls_cert.is_empty() || tls_key.is_empty() {
            eprintln!("wf-signal: --tls needs --tls-cert and --tls-key");
            std::process::exit(2);
        }
        c.tls = Some((a, tls_cert, tls_key));
    }
    if c.stun_urls.is_empty() {
        // STUN on the TURN hosts (coturn answers both)
        for u in &c.turn_urls {
            if let Some(rest) = u.strip_prefix("turn:") {
                let host = rest.split('?').next().unwrap_or(rest);
                let s = format!("stun:{host}");
                if !c.stun_urls.contains(&s) {
                    c.stun_urls.push(s);
                }
            }
        }
    }
    (addr.parse().expect("address like 127.0.0.1:3536"), c)
}

/// The ICE servers for `/ice`: STUN + TURN with fresh REST credentials.
fn ice_json(c: &Config) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut servers = Vec::new();
    if !c.stun_urls.is_empty() {
        servers.push(IceServer { urls: c.stun_urls.clone(), ..Default::default() });
    }
    if !c.turn_urls.is_empty() {
        let (u, p) = if c.turn_secret.is_empty() { (String::new(), String::new()) } else { turn_rest_credentials(&c.turn_secret, "wf", c.turn_ttl, now) };
        servers.push(IceServer { urls: c.turn_urls.clone(), username: (!u.is_empty()).then_some(u), credential: (!p.is_empty()).then_some(p) });
    }
    let cfg = IceConfig { servers, relay_only: false };
    let mut v = serde_json::to_value(&cfg).unwrap_or_default();
    v["ttl"] = serde_json::json!(c.turn_ttl);
    v.as_object_mut().map(|o| o.remove("relayOnly"));
    v.to_string()
}

/// TLS in front of the plain listener: accept TLS on `listen`, forward the bytes to `backend`.
async fn tls_front(listen: String, backend: SocketAddr, cert: String, key: String) -> Result<(), String> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(&cert).map_err(|e| format!("{cert}: {e}"))?.collect::<Result<_, _>>().map_err(|e| format!("{cert}: {e}"))?;
    let key = PrivateKeyDer::from_pem_file(&key).map_err(|e| format!("{key}: {e}"))?;
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    let acc = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let l = tokio::net::TcpListener::bind(&listen).await.map_err(|e| format!("{listen}: {e}"))?;
    eprintln!("wf-signal: TLS on wss://{listen}/<room> (https://{listen}/ice)");
    loop {
        let Ok((s, _)) = l.accept().await else { continue };
        let acc = acc.clone();
        tokio::spawn(async move {
            if let Ok(mut tls) = acc.accept(s).await {
                if let Ok(mut b) = tokio::net::TcpStream::connect(backend).await {
                    let _ = tokio::io::copy_bidirectional(&mut tls, &mut b).await;
                }
            }
        });
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let (addr, cfg) = parse();
    let rooms = Arc::new(Mutex::new(Rooms::default()));
    let (r1, r2, r3) = (rooms.clone(), rooms.clone(), rooms.clone());
    let c1 = cfg.clone();
    let c2 = cfg.clone();
    let server = SignalingServer::full_mesh_builder(addr)
        .on_connection_request(move |m| {
            let path = m.path.clone().unwrap_or_default();
            let room = path.trim_start_matches('/').to_string();
            let q = |k: &str| m.query_params.get(k).cloned().unwrap_or_default();
            let who = m.headers.get("x-forwarded-for").and_then(|h| h.to_str().ok()).map(|s| s.to_string()).unwrap_or_else(|| m.origin.ip().to_string());
            if !c1.key.is_empty() && q("key") != c1.key {
                eprintln!("wf-signal: {who} -> {room}: refused (server key)");
                return Ok(false);
            }
            if room.is_empty() || room.len() > 64 {
                return Ok(false);
            }
            let token = q("token");
            let mut r = r1.lock().unwrap();
            if r.peers.len() + r.pending.len() >= c1.max_peers {
                eprintln!("wf-signal: {who} -> {room}: refused (server full)");
                return Ok(false);
            }
            if c1.require_token && token.is_empty() {
                eprintln!("wf-signal: {who} -> {room}: refused (no room token)");
                return Ok(false);
            }
            match r.rooms.get(&room) {
                Some((t, n)) if *n > 0 => {
                    if *t != token {
                        eprintln!("wf-signal: {who} -> {room}: refused (wrong room token)");
                        return Ok(false);
                    }
                    if *n >= c1.max_room {
                        eprintln!("wf-signal: {who} -> {room}: refused (room full)");
                        return Ok(false);
                    }
                }
                _ => {
                    r.rooms.insert(room.clone(), (token, 0));
                }
            }
            r.rooms.get_mut(&room).unwrap().1 += 1;
            r.pending.insert(m.origin, room.clone());
            eprintln!("wf-signal: connection {who} -> {room}");
            Ok(true)
        })
        .on_id_assignment(move |(origin, id)| {
            let mut r = r2.lock().unwrap();
            if let Some(room) = r.pending.remove(&origin) {
                eprintln!("wf-signal: {origin} is peer {id} in {room}");
                r.peers.insert(id.to_string(), room);
            }
        })
        .on_peer_disconnected(move |id| {
            let mut r = r3.lock().unwrap();
            if let Some(room) = r.peers.remove(&id.to_string()) {
                let empty = r.rooms.get_mut(&room).map(|e| {
                    e.1 = e.1.saturating_sub(1);
                    e.1 == 0
                });
                if empty == Some(true) {
                    r.rooms.remove(&room);
                }
                eprintln!("wf-signal: peer {id} left {room}");
            }
        })
        .cors()
        .build_with(move |router| {
            router
                .route(
                    "/ice",
                    get(move |Query(q): Query<HashMap<String, String>>| {
                        let c = c2.clone();
                        async move {
                            if !c.key.is_empty() && q.get("key").map(|s| s.as_str()) != Some(c.key.as_str()) {
                                return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store"), (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], "{\"error\":\"key\"}".to_string()).into_response();
                            }
                            (StatusCode::OK, [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store"), (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], ice_json(&c)).into_response()
                        }
                    }),
                )
                .route("/healthz", get(|| async { "ok" }))
        });
    eprintln!(
        "wf-signal: listening on ws://{addr}/<room>  (key {}, room tokens {}, TURN {})",
        if cfg.key.is_empty() { "off" } else { "on" },
        if cfg.require_token { "required" } else { "optional" },
        if cfg.turn_urls.is_empty() { "none".to_string() } else { format!("{} ({} credentials)", cfg.turn_urls.join(" "), if cfg.turn_secret.is_empty() { "no" } else { "REST" }) }
    );
    if let Some((a, cert, key)) = cfg.tls.clone() {
        let backend = if addr.ip().is_unspecified() { SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), addr.port()) } else { addr };
        tokio::spawn(async move {
            if let Err(e) = tls_front(a, backend, cert, key).await {
                eprintln!("wf-signal: TLS: {e}");
                std::process::exit(1);
            }
        });
    }
    tokio::select! {
        r = server.serve() => if let Err(e) = r { eprintln!("wf-signal: {e}") },
        _ = tokio::signal::ctrl_c() => eprintln!("wf-signal: bye"),
    }
}
