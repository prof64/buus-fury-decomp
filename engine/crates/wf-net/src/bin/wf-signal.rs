//! `wf-signal [ADDR]` — the Matchbox full-mesh signaling server for Buu's Fury rooms (default 127.0.0.1:3536).
//! Peers connect to `ws://ADDR/wf-<ROOM CODE>`; the server only relays WebRTC offers / answers / ICE candidates
//! between the peers of a room. Game traffic never passes through it.
//! Expose it beyond this machine with `wf-signal 0.0.0.0:3536` (and a TLS proxy for `wss://` on the web).

use std::net::SocketAddr;

use matchbox_signaling::SignalingServer;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let addr: SocketAddr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:3536".into()).parse().expect("address like 127.0.0.1:3536");
    let server = SignalingServer::full_mesh_builder(addr)
        .on_connection_request(|c| {
            eprintln!("wf-signal: connection {} -> {}", c.origin, c.path.as_deref().unwrap_or("/"));
            Ok(true)
        })
        .on_id_assignment(|(origin, id)| eprintln!("wf-signal: {origin} is peer {id}"))
        .cors()
        .build();
    eprintln!("wf-signal: listening on ws://{addr}/<room>");
    tokio::select! {
        r = server.serve() => if let Err(e) = r { eprintln!("wf-signal: {e}") },
        _ = tokio::signal::ctrl_c() => eprintln!("wf-signal: bye"),
    }
}
