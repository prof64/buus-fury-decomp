//! Native WebRTC transport: `matchbox_socket` (webrtc-rs) with one unreliable, unordered data channel per peer,
//! signaled through a Matchbox server (`wf-signal`, or any `matchbox_server`). The socket's message loop runs on a
//! small tokio runtime owned by the transport; the game thread only polls queues. The browser side speaks the same
//! signaling protocol and channel setup (engine/web/app/net.js), so native and browser peers meet in one room.

use std::collections::BTreeMap;

use matchbox_socket::{ChannelConfig, PeerId, PeerState, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder};

use crate::transport::{PeerChange, Transport};

pub struct MatchboxTransport {
    socket: WebRtcSocket,
    _rt: tokio::runtime::Runtime,
    id: Option<String>,
    ids: BTreeMap<String, PeerId>,
    pending: Vec<PeerChange>,
    pub closed: bool,
}

impl MatchboxTransport {
    /// `room_url`: `ws://host:3536/wf-CODE`. `stun`: ICE servers (empty = host candidates only, enough on one
    /// machine / LAN; the default for a localhost signaling server).
    pub fn connect(room_url: &str, stun: Vec<String>) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string())?;
        let ice = RtcIceServerConfig { urls: stun, username: None, credential: None };
        let (socket, fut) = WebRtcSocketBuilder::new(room_url).ice_server(ice).reconnect_attempts(Some(5)).add_channel(ChannelConfig::unreliable()).build();
        rt.spawn(async move {
            if let Err(e) = fut.await {
                eprintln!("wf-net: signaling / WebRTC loop ended: {e}");
            }
        });
        Ok(MatchboxTransport { socket, _rt: rt, id: None, ids: BTreeMap::new(), pending: Vec::new(), closed: false })
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
}
