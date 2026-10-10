//! `wf-turn [options]` — a small TURN relay (webrtc-rs `turn`, UDP) for peers that cannot reach each other
//! directly (symmetric NATs, strict firewalls), and for testing the relay path on one machine
//! (`--net-relay-only`). Production: coturn is the battle-tested choice (engine/deploy/netplay/); this one speaks the
//! same credentials, so `wf-signal --turn-secret` works with either.
//!
//!   --listen ADDR        (WF_TURN_LISTEN, 0.0.0.0:3478)  UDP listen address
//!   --public-ip IP       (WF_TURN_PUBLIC_IP, 127.0.0.1)  the address relay candidates advertise (the server's
//!                                                        public IP on a VPS)
//!   --relay-bind IP      (0.0.0.0)                       where relay sockets are opened
//!   --secret S           (WF_TURN_SECRET)                TURN REST API credentials (username `<expiry>[:user]`,
//!                                                        password base64(HMAC-SHA1(S, username))), as coturn
//!                                                        `use-auth-secret` / `static-auth-secret`
//!   --user NAME=PASS     (WF_TURN_USERS, comma separated) static long-term users
//!   --realm R            (wf)
//!
//! Prints `wf-turn: listening` when ready; stops on Ctrl-C.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::net::UdpSocket;
use turn::auth::{generate_auth_key, AuthHandler};
use turn::relay::relay_static::RelayAddressGeneratorStatic;
use turn::server::config::{ConnConfig, ServerConfig};
use turn::server::Server;
use webrtc_util::vnet::net::Net;
use wf_net::ice::{base64, hmac_sha1, rest_expiry};

struct Auth {
    secret: String,
    users: HashMap<String, Vec<u8>>,
    log: bool,
    /// clients seen (logged once)
    seen: std::sync::Mutex<std::collections::HashSet<SocketAddr>>,
}

impl Auth {
    fn ok(&self, src: SocketAddr, username: &str) {
        if self.log && self.seen.lock().unwrap().insert(src) {
            eprintln!("wf-turn: client {src} authenticated as {username}");
        }
    }
}

impl AuthHandler for Auth {
    fn auth_handle(&self, username: &str, realm: &str, src: SocketAddr) -> Result<Vec<u8>, turn::Error> {
        if let Some(k) = self.users.get(username) {
            self.ok(src, username);
            return Ok(k.clone());
        }
        if !self.secret.is_empty() {
            if let Some(exp) = rest_expiry(username) {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                if exp < now {
                    if self.log {
                        eprintln!("wf-turn: {src}: expired credentials {username}");
                    }
                    return Err(turn::Error::Other(format!("expired username {username}")));
                }
                let pass = base64(&hmac_sha1(self.secret.as_bytes(), username.as_bytes()));
                self.ok(src, username);
                return Ok(generate_auth_key(username, realm, &pass));
            }
        }
        if self.log {
            eprintln!("wf-turn: {src}: unknown user {username}");
        }
        Err(turn::Error::Other(format!("unknown user {username}")))
    }
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let mut listen = env("WF_TURN_LISTEN").unwrap_or_else(|| "0.0.0.0:3478".into());
    let mut public_ip = env("WF_TURN_PUBLIC_IP").unwrap_or_else(|| "127.0.0.1".into());
    let mut relay_bind = "0.0.0.0".to_string();
    let mut secret = env("WF_TURN_SECRET").unwrap_or_default();
    let mut realm = "wf".to_string();
    let mut users_s: Vec<String> = env("WF_TURN_USERS").map(|v| v.split(',').map(String::from).collect()).unwrap_or_default();
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let arg = v[i].clone();
        let mut next = || {
            i += 1;
            v.get(i).cloned().unwrap_or_default()
        };
        match arg.as_str() {
            "--listen" => listen = next(),
            "--public-ip" => public_ip = next(),
            "--relay-bind" => relay_bind = next(),
            "--secret" => secret = next(),
            "--realm" => realm = next(),
            "--user" => users_s.push(next()),
            "-h" | "--help" => {
                println!("{}", include_str!("wf-turn.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim_start_matches(' ')).collect::<Vec<_>>().join("\n"));
                return;
            }
            a => {
                eprintln!("wf-turn: unknown option {a} (--help)");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let mut users = HashMap::new();
    for u in users_s.iter().filter(|u| !u.is_empty()) {
        let (n, p) = u.split_once('=').or_else(|| u.split_once(':')).expect("--user NAME=PASS");
        users.insert(n.to_string(), generate_auth_key(n, &realm, p));
    }
    if users.is_empty() && secret.is_empty() {
        eprintln!("wf-turn: needs --secret or --user NAME=PASS (an open relay would relay anything for anyone)");
        std::process::exit(2);
    }
    let conn = Arc::new(UdpSocket::bind(&listen).await.unwrap_or_else(|e| panic!("wf-turn: bind {listen}: {e}")));
    let ip: IpAddr = public_ip.parse().expect("--public-ip IP");
    // every allocation that ends: who relayed how much
    let (closed_tx, mut closed_rx) = tokio::sync::mpsc::channel::<turn::allocation::AllocationInfo>(64);
    tokio::spawn(async move {
        while let Some(a) = closed_rx.recv().await {
            eprintln!("wf-turn: allocation {} for {} ({}) closed: {} bytes relayed", a.relay_addr, a.five_tuple.src_addr, a.username, a.relayed_bytes);
        }
    });
    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig { conn, relay_addr_generator: Box::new(RelayAddressGeneratorStatic { relay_address: ip, address: relay_bind, net: Arc::new(Net::new(None)) }) }],
        realm: realm.clone(),
        auth_handler: Arc::new(Auth { secret: secret.clone(), users, log: true, seen: Default::default() }),
        channel_bind_timeout: std::time::Duration::from_secs(0),
        alloc_close_notify: Some(closed_tx),
    })
    .await
    .unwrap_or_else(|e| panic!("wf-turn: {e}"));
    eprintln!("wf-turn: listening on udp {listen}, relaying as {ip} (realm {realm}, {})", if secret.is_empty() { "static users" } else { "REST credentials" });
    let _ = tokio::signal::ctrl_c().await;
    let _ = server.close().await;
    eprintln!("wf-turn: bye");
}
