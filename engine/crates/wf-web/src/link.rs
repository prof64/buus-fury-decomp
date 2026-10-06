//! Link play in the browser (engine/docs/netcode.md "Link play: in the browser"): the title with
//! **Multiplayer -> Dojo / Trading Post** ([`wf_app::link`], the same flow and screens as the native `--link`).
//!
//! JS starts it with `wf_app_init({"launch": {"kind": "title"}, "slots": [...], "link": {...}})` (app.js reads
//! `?link=host:ROOM` / `?link=join:ROOM` and `&signal=ws://...`, engine/web/app/link.js). When the title's link box
//! opens, the flow asks for the room's connection: Rust queues the room URL (`ROOM-DOJO` / `ROOM-TRADE` on the
//! signalling server) and hands the flow a queue transport; JS polls `wf_link_poll` each frame, opens the same
//! Matchbox signalling + WebRTC data channel as online battles and feeds that queue through the `wf_net_*`
//! exports (net.rs: they serve the link's queue while no online battle is prepared).
//!
//! **Saves and the trade journal.** The Trading Post's durable side ([`wf_app::trade::FileStore`]) runs on a
//! [`Disk`] kept here in memory and mirrored to `localStorage` (`wf.link.<path>`, hex): every write is queued and JS
//! takes the queue (`wf_link_disk_next`, one path a call, in order) **before** it sends the frame's outgoing
//! packets (`wf_net_out`). `localStorage.setItem` is synchronous and atomic per key, so each write is durable
//! before any message that depends on it leaves the page, and the write-ahead log keeps the native store's
//! atomicity (journal with the post-trade bytes -> slot -> journal Committed; a reload re-opens the store from the
//! mirrored keys and finishes an interrupted commit). The slot the trade commits is also handed to the running
//! title (`Out::Slot`), which app.js persists as the story save slot (`wf.save.N`) like any other save.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use wf_app::link::{LinkConfig, LinkKind};
use wf_app::trade::Disk;
use wf_app::wf_content::Packs;
use wf_app::App;
use wf_net::lobby::Role;
use wf_net::transport::QueueTransport;

use super::set_str;

thread_local! {
    /// the link's queue transport (the `wf_net_*` exports fill / drain it when no online battle is prepared)
    pub(crate) static LINK_Q: RefCell<Option<QueueTransport>> = const { RefCell::new(None) };
    /// a room URL the flow asked for (JS opens it)
    static PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
    /// disk writes not yet mirrored to localStorage
    static DISK_OUT: RefCell<VecDeque<(String, Vec<u8>)>> = const { RefCell::new(VecDeque::new()) };
    /// the flow dropped its connection: JS closes the room's WebSocket / peer connections (the friend then sees the
    /// link go, as with the native shell's socket)
    static CLOSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// the flow's transport: the queue (net.rs) + a drop that asks JS to close the room
struct LinkTransport(Box<dyn wf_net::transport::Transport>);

impl wf_net::transport::Transport for LinkTransport {
    fn local_id(&self) -> Option<String> {
        self.0.local_id()
    }
    fn send(&mut self, to: &str, data: Vec<u8>, now: f64) {
        self.0.send(to, data, now)
    }
    fn recv(&mut self, now: f64) -> Vec<(String, Vec<u8>)> {
        self.0.recv(now)
    }
    fn changes(&mut self) -> Vec<wf_net::transport::PeerChange> {
        self.0.changes()
    }
    fn closed(&self) -> bool {
        self.0.closed()
    }
    fn error(&self) -> Option<String> {
        self.0.error()
    }
}

impl Drop for LinkTransport {
    fn drop(&mut self) {
        CLOSE.with(|c| c.set(true));
        LINK_Q.with(|q| *q.borrow_mut() = None);
    }
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|k| u8::from_str_radix(s.get(2 * k..2 * k + 2)?, 16).ok()).collect()
}

/// the trade store's files: in memory, every write queued for localStorage (see the module doc)
struct WebDisk {
    files: RefCell<BTreeMap<String, Vec<u8>>>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl Disk for WebDisk {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.borrow().get(&key(path)).cloned()
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let k = key(path);
        self.files.borrow_mut().insert(k.clone(), bytes.to_vec());
        DISK_OUT.with(|q| q.borrow_mut().push_back((k, bytes.to_vec())));
        Ok(())
    }
}

/// `wf_app_init` with a `link` object: the title with link play. `link`: `host` (bool), `room`, `signal`, `key`,
/// `link_id` (this browser's stable id, persisted by JS), `name`, `disk` ({path: hex} - the mirrored files),
/// `info` (true: the test hook `wf_link_info`), `bot` ("dojo" / "trade": the scripted player, tests)
pub fn link_app(cfg: &serde_json::Value, packs: Packs) -> Option<Result<App, String>> {
    let l = cfg.get("link").filter(|v| v.is_object())?;
    let host = l["host"].as_bool().unwrap_or(false);
    let room = l["room"].as_str().unwrap_or("").trim().to_ascii_uppercase();
    if room.is_empty() {
        return Some(Err("link play needs a room (?link=host:ROOM or ?link=join:ROOM)".into()));
    }
    let signal = l["signal"].as_str().unwrap_or("ws://127.0.0.1:3536").to_string();
    let skey = l["key"].as_str().unwrap_or("").to_string();
    let files: BTreeMap<String, Vec<u8>> =
        l["disk"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), hex_decode(v.as_str().unwrap_or("")))).collect()).unwrap_or_default();
    wf_app::trade::set_disk(Some(Rc::new(WebDisk { files: RefCell::new(files) })));
    wf_app::link::set_link_info(l["info"].as_bool().unwrap_or(false));
    let name = l["name"].as_str().map(String::from).unwrap_or_else(|| if host { "host".into() } else { "guest".into() });
    let lcfg = LinkConfig {
        connect: Box::new(move |kind| {
            let page = if kind == LinkKind::Dojo { "DOJO" } else { "TRADE" };
            let url = wf_net::lobby::room_url_auth(&signal, &format!("{room}-{page}"), "", &skey);
            let q = QueueTransport::default();
            LINK_Q.with(|x| *x.borrow_mut() = Some(q.clone()));
            PENDING.with(|p| *p.borrow_mut() = Some(url));
            CLOSE.with(|c| c.set(false));
            Ok((Box::new(LinkTransport(super::net::link_transport(&q))) as Box<dyn wf_net::transport::Transport>, if host { Role::Host } else { Role::Player }))
        }),
        clock: super::net::clock(),
        link_id: l["link_id"].as_str().unwrap_or("web").to_string(),
        name,
        saves_dir: Some(PathBuf::from("saves")),
        // `quick`: the Dojo's short test rules (as the native --link-quick: 40 s rounds, at most 3)
        rules: if l["quick"].as_bool() == Some(true) {
            wf_net::dojo::DojoRules { round_frames: 2400, between_frames: 60, max_rounds: 3, ..Default::default() }
        } else {
            Default::default()
        },
        seed: cfg["seed"].as_u64().unwrap_or(1) as u32,
        // `bot`: "dojo" / "trade" - the scripted player of `--link-bot` (the two-page test)
        bot: match l["bot"].as_str() {
            Some("dojo") => Some(wf_app::link::LinkBot::new(LinkKind::Dojo, host)),
            Some("trade") => Some(wf_app::link::LinkBot::new(LinkKind::Trade, host)),
            _ => None,
        },
        // (the flows' report: `wf_link_info`'s `report`, no file in the browser)
        report: None,
    };
    Some(wf_app::link::link_title_app(packs, &super::launcher::slots(cfg), cfg["seed"].as_u64().unwrap_or(1) as u32, lcfg))
}

/// A room URL to open: 1 (the URL in the string buffer), 2 (the flow dropped its connection: close the room) or 0
/// (nothing). JS opens the signalling + data channel for a URL and feeds the link's queue through the `wf_net_*`
/// exports.
#[no_mangle]
pub extern "C" fn wf_link_poll() -> u32 {
    if CLOSE.with(|c| c.replace(false)) && PENDING.with(|p| p.borrow().is_none()) {
        return 2;
    }
    match PENDING.with(|p| p.borrow_mut().take()) {
        Some(u) => {
            set_str(&u);
            1
        }
        None => 0,
    }
}

/// The next disk write to mirror to localStorage: its total length (0 = none); the string buffer holds
/// `[path length u8][path][bytes]`. Call until 0 **before** draining `wf_net_out` (see the module doc).
#[no_mangle]
pub extern "C" fn wf_link_disk_next() -> u32 {
    match DISK_OUT.with(|q| q.borrow_mut().pop_front()) {
        Some((k, b)) => {
            let mut v = Vec::with_capacity(1 + k.len() + b.len());
            v.push(k.len().min(255) as u8);
            v.extend_from_slice(&k.as_bytes()[..k.len().min(255)]);
            v.extend_from_slice(&b);
            let n = v.len() as u32;
            super::STR.with(|s| *s.borrow_mut() = v);
            n
        }
        None => 0,
    }
}

/// The link state after the last step (JSON in the string buffer; `{"link": {"info": true}}` switches it on):
/// the tests' hook (`wf_app::link::link_info`).
#[no_mangle]
pub extern "C" fn wf_link_info() {
    set_str(&wf_app::link::link_info().to_string());
}
