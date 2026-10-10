//! Native WebRTC transport: `matchbox_socket` (webrtc-rs) with one unreliable, unordered data channel per peer,
//! signaled through a Matchbox server (`wf-signal`, or any `matchbox_server`) with our own signaling client
//! (`wsclient.rs`: `wss://` with rustls + an optional extra CA, relay-only candidate filter). The socket's message loop
//! runs on a small tokio runtime owned by the transport; the game thread only polls queues. The browser side speaks
//! the same signaling protocol and channel setup (engine/web/app/net.js), so native and browser peers meet in one
//! room.

use std::collections::BTreeMap;
use std::sync::Arc;

use matchbox_socket::{ChannelConfig, PeerId, PeerState, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder};

use crate::ice::IceConfig;
use crate::transport::{PeerChange, Transport};
use crate::wsclient::{fetch_ice, tls_config, WsSignallerBuilder};

/// How to reach the room: ICE servers (or `auto` = ask the signaling server), TLS trust, server key.
#[derive(Clone, Debug, Default)]
pub struct ConnectOptions {
    pub ice: IceConfig,
    /// fetch the ICE servers from `GET <signal>/ice` (wf-signal with TURN credentials); `ice` is the fallback
    pub ice_auto: bool,
    /// extra CA certificate (PEM) for `wss://` / `https://` with a self-signed certificate
    pub ca: Option<String>,
    /// the signaling server's access key (`?key=`)
    pub key: String,
}

pub struct MatchboxTransport {
    socket: WebRtcSocket,
    _rt: tokio::runtime::Runtime,
    id: Option<String>,
    ids: BTreeMap<String, PeerId>,
    pending: Vec<PeerChange>,
    pub closed: bool,
    /// why the signaling / WebRTC loop ended
    pub error: Arc<std::sync::Mutex<Option<String>>>,
    /// the ICE servers in use (after `auto`)
    pub ice: IceConfig,
}

impl MatchboxTransport {
    /// `room_url`: `ws(s)://host[:port]/wf-CODE[?token=…&key=…]`. ICE: empty = host candidates only (enough on one
    /// machine / LAN; the default for a localhost signaling server).
    pub fn connect(room_url: &str, stun: Vec<String>) -> Result<Self, String> {
        let ice = if stun.is_empty() { IceConfig::default() } else { IceConfig { servers: vec![crate::ice::IceServer { urls: stun, ..Default::default() }], relay_only: false } };
        Self::connect_with(room_url, &ConnectOptions { ice, ..Default::default() })
    }

    pub fn connect_with(room_url: &str, o: &ConnectOptions) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string())?;
        let ca = o.ca.clone().or_else(|| std::env::var("WF_NET_CA").ok());
        let tls = tls_config(ca.as_deref())?;
        let mut ice = o.ice.clone();
        if o.ice_auto {
            match rt.block_on(fetch_ice(room_url, &o.key, tls.clone())) {
                Ok(j) => match IceConfig::from_json(&j) {
                    Ok(mut got) if !got.is_empty() => {
                        got.relay_only = ice.relay_only;
                        eprintln!("wf-net: ICE servers from the signaling server: {}", got.servers.iter().flat_map(|s| s.urls.iter().cloned()).collect::<Vec<_>>().join(" "));
                        ice = got;
                    }
                    Ok(_) => eprintln!("wf-net: the signaling server has no ICE servers; using {}", if ice.is_empty() { "none".into() } else { format!("{:?}", ice.servers) }),
                    Err(e) => eprintln!("wf-net: {e}"),
                },
                Err(e) => eprintln!("wf-net: no ICE servers from the signaling server ({e}); using the defaults"),
            }
        }
        if ice.relay_only && !ice.has_turn() {
            return Err("relay-only needs a TURN server (--net-ice turn:HOST:3478 --net-turn-user U --net-turn-pass P, or a signaling server with --turn-secret)".into());
        }
        let (server, warn) = ice.merged();
        if let Some(w) = warn {
            eprintln!("wf-net: {w}");
        }
        let cfg = RtcIceServerConfig { urls: server.urls.clone(), username: server.username.clone(), credential: server.credential.clone() };
        let builder = Arc::new(WsSignallerBuilder { tls, relay_only: ice.relay_only });
        let (socket, fut) = WebRtcSocketBuilder::new(room_url).ice_server(cfg).signaller_builder(builder).reconnect_attempts(Some(5)).add_channel(ChannelConfig::unreliable()).build();
        let error = Arc::new(std::sync::Mutex::new(None));
        let err2 = error.clone();
        rt.spawn(async move {
            if let Err(e) = fut.await {
                eprintln!("wf-net: signaling / WebRTC loop ended: {e}");
                *err2.lock().unwrap() = Some(e.to_string());
            }
        });
        Ok(MatchboxTransport { socket, _rt: rt, id: None, ids: BTreeMap::new(), pending: Vec::new(), closed: false, error, ice })
    }

    fn update(&mut self) {
        if self.id.is_none() {
            self.id = self.socket.id().map(|p| p.0.to_string());
        }
        match self.socket.try_update_peers() {
            Ok(ch) => {
                for (p, st) in ch {
                    let s = p.0.to_string();
                    self.ids.insert(s.clone(), p);
                    self.pending.push(match st {
                        PeerState::Connected => PeerChange::Connected(s),
                        PeerState::Disconnected => PeerChange::Disconnected(s),
                    });
                }
            }
            Err(_) => self.closed = true,
        }
    }
}

impl Transport for MatchboxTransport {
    fn local_id(&self) -> Option<String> {
        self.id.clone()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, _now: f64) {
        if self.closed {
            return;
        }
        if let Some(&p) = self.ids.get(to) {
            if let Ok(ch) = self.socket.get_channel_mut(0) {
                let _ = ch.try_send(data.into_boxed_slice(), p);
            }
        }
    }
    fn recv(&mut self, _now: f64) -> Vec<(String, Vec<u8>)> {
        self.update();
        match self.socket.get_channel_mut(0) {
            Ok(ch) => ch.receive().into_iter().map(|(p, d)| (p.0.to_string(), d.into_vec())).collect(),
            Err(_) => Vec::new(),
        }
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        self.update();
        std::mem::take(&mut self.pending)
    }
    fn closed(&self) -> bool {
        self.closed || self.error.lock().map(|e| e.is_some()).unwrap_or(false)
    }
    fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }
}
