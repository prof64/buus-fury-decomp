//! Native signaling client for `matchbox_socket` (replaces its built-in one): WebSocket over tokio with rustls, so
//! `wss://` works with the public CA roots **and** an extra CA (`--net-ca FILE`, `WF_NET_CA`: a self-signed
//! test proxy), plus the **relay-only** filter: outgoing and incoming ICE candidates that are not `typ relay` (and
//! such lines inside offers / answers from browsers, which embed their candidates) are dropped, so every byte has to
//! go through the TURN server — what a symmetric NAT on both sides forces in the wild.
//!
//! Also `fetch_ice`: `GET <signal origin>/ice?key=…` (wf-signal's time-limited TURN credentials).

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use matchbox_socket::async_trait::async_trait;
use matchbox_socket::{PeerEvent, PeerRequest, PeerSignal, SignalingError, Signaller, SignallerBuilder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use crate::ice::{candidate_is_relay, sdp_relay_only};

/// TLS client config: webpki roots + the extra CA certificates in `extra_ca` (PEM file), if any.
pub fn tls_config(extra_ca: Option<&str>) -> Result<Arc<rustls::ClientConfig>, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(path) = extra_ca.filter(|p| !p.is_empty()) {
        use rustls::pki_types::pem::PemObject;
        let certs: Vec<_> = rustls::pki_types::CertificateDer::pem_file_iter(path).map_err(|e| format!("{path}: {e}"))?.collect::<Result<_, _>>().map_err(|e| format!("{path}: {e}"))?;
        if certs.is_empty() {
            return Err(format!("{path}: no PEM certificate"));
        }
        for c in certs {
            roots.add(c).map_err(|e| format!("{path}: {e}"))?;
        }
    }
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

#[derive(Debug, Clone)]
pub struct WsSignallerBuilder {
    pub tls: Arc<rustls::ClientConfig>,
    pub relay_only: bool,
}

#[async_trait]
impl SignallerBuilder for WsSignallerBuilder {
    async fn new_signaller(&self, mut attempts: Option<u16>, room_url: String) -> Result<Box<dyn Signaller>, SignalingError> {
        loop {
            let r = tokio_tungstenite::connect_async_tls_with_config(room_url.as_str(), None, true, Some(Connector::Rustls(self.tls.clone()))).await;
            match r {
                Ok((ws, _)) => return Ok(Box::new(WsSignaller { ws, relay_only: self.relay_only })),
                Err(e) => {
                    let msg = match &e {
                        tokio_tungstenite::tungstenite::Error::Http(r) => format!("the signaling server refused the connection (HTTP {}): wrong room token / server key, or the room is full", r.status()),
                        e => format!("cannot reach the signaling server: {e}"),
                    };
                    if matches!(e, tokio_tungstenite::tungstenite::Error::Http(_)) || attempts.is_some_and(|a| a <= 1) {
                        eprintln!("wf-net: {msg}");
                    }
                    match attempts.as_mut() {
                        Some(a) if *a <= 1 => return Err(SignalingError::UserImplementationError(msg)),
                        Some(a) => *a -= 1,
                        None => {}
                    }
                    if matches!(e, tokio_tungstenite::tungstenite::Error::Http(_)) {
                        return Err(SignalingError::UserImplementationError(msg));
                    }
                    eprintln!("wf-net: {msg}; retrying in 2 s");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
}

struct WsSignaller {
    ws: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    relay_only: bool,
}

fn filter_signal(s: PeerSignal, relay_only: bool) -> Option<PeerSignal> {
    if !relay_only {
        return Some(s);
    }
    match s {
        PeerSignal::IceCandidate(c) => candidate_is_relay(&c).then_some(PeerSignal::IceCandidate(c)),
        PeerSignal::Offer(sdp) => Some(PeerSignal::Offer(sdp_relay_only(&sdp))),
        PeerSignal::Answer(sdp) => Some(PeerSignal::Answer(sdp_relay_only(&sdp))),
    }
}

#[async_trait]
impl Signaller for WsSignaller {
    async fn send(&mut self, request: PeerRequest) -> Result<(), SignalingError> {
        let request = match request {
            PeerRequest::Signal { receiver, data } => match filter_signal(data, self.relay_only) {
                Some(data) => PeerRequest::Signal { receiver, data },
                None => return Ok(()),
            },
            r => r,
        };
        let text = serde_json::to_string(&request).map_err(|e| SignalingError::UserImplementationError(e.to_string()))?;
        self.ws.send(Message::Text(text.into())).await.map_err(|e| SignalingError::UserImplementationError(e.to_string()))
    }

    async fn next_message(&mut self) -> Result<PeerEvent, SignalingError> {
        loop {
            let m = match self.ws.next().await {
                Some(Ok(Message::Text(t))) => t,
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                Some(Ok(Message::Binary(_))) => return Err(SignalingError::UnknownFormat),
                Some(Ok(Message::Close(_))) | None => return Err(SignalingError::StreamExhausted),
                Some(Err(e)) => return Err(SignalingError::UserImplementationError(e.to_string())),
            };
            let ev: PeerEvent = serde_json::from_str(&m).map_err(|_| SignalingError::UnknownFormat)?;
            if let PeerEvent::Signal { sender, data } = ev {
                match filter_signal(data, self.relay_only) {
                    Some(data) => return Ok(PeerEvent::Signal { sender, data }),
                    None => continue,
                }
            }
            return Ok(ev);
        }
    }
}

/// `GET {origin}/ice?key=…` -> the ICE servers wf-signal hands out (time-limited TURN credentials). Plain HTTP/1.1
/// over TCP or TLS (no HTTP client dependency).
pub async fn fetch_ice(signal: &str, key: &str, tls: Arc<rustls::ClientConfig>) -> Result<String, String> {
    let origin = crate::ice::http_origin(signal).ok_or_else(|| format!("not a ws(s):// URL: {signal}"))?;
    let (https, hostport) = match origin.split_once("://") {
        Some(("https", h)) => (true, h.to_string()),
        Some((_, h)) => (false, h.to_string()),
        None => return Err("bad URL".into()),
    };
    let default_port = if https { 443 } else { 80 };
    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (h, p) = v6.split_once(']').ok_or("bad IPv6 host")?;
        (h.to_string(), p.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(default_port))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|e| e.to_string())?),
            None => (hostport.clone(), default_port),
        }
    };
    let path = if key.is_empty() { "/ice".to_string() } else { format!("/ice?key={}", crate::lobby::url_escape(key)) };
    let req = format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\nUser-Agent: wf-net\r\nAccept: application/json\r\nConnection: close\r\n\r\n");
    let tcp = tokio::time::timeout(std::time::Duration::from_secs(5), tokio::net::TcpStream::connect((host.as_str(), port))).await.map_err(|_| "timeout".to_string())?.map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    if https {
        let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
        let mut s = tokio_rustls::TlsConnector::from(tls).connect(name, tcp).await.map_err(|e| e.to_string())?;
        s.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), s.read_to_end(&mut body)).await;
    } else {
        let mut s = tcp;
        s.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), s.read_to_end(&mut body)).await;
    }
    let text = String::from_utf8_lossy(&body).to_string();
    let (head, rest) = text.split_once("\r\n\r\n").ok_or("bad HTTP response")?;
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("GET {path}: HTTP {status}"));
    }
    // chunked bodies (proxies): take the JSON object
    let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        let a = rest.find('{').unwrap_or(0);
        let b = rest.rfind('}').map(|i| i + 1).unwrap_or(rest.len());
        rest[a..b].to_string()
    } else {
        rest.to_string()
    };
    Ok(body)
}
