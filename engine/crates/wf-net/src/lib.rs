//! wf-net: online multiplayer for the Buu's Fury remake. See `engine/docs/netcode.md`.
//!
//! * [`session::P2PSession`]: GGPO-style rollback over any [`game::Game`] (input delay, prediction window, time
//!   sync, per-frame state hashes + hash chain for desync detection, disconnects, spectator forwarding). No I/O,
//!   no clock: the host passes time and moves packets, so it runs the same on native, wasm and a virtual network.
//! * [`spectator::SpectatorSession`], [`synctest::SyncTestSession`] (rollback every frame and compare).
//! * [`core_game`]: wf-core's battle as a `Game`; [`core_game::MatchConfig`] (co-op / PvP).
//! * [`lobby`]: room codes + tokens, hellos (role, fighter / form, ready), the host's `start`, rejoin requests /
//!   grants ([`blob`]: the snapshot transfer).
//! * [`ice`]: STUN / TURN configuration, TURN REST credentials; `wsclient` (feature `webrtc`): wss:// signaling
//!   with rustls, relay-only filter, `GET /ice`.
//! * [`netsim`]: `NetLobby` + `NetSim` / `SyncSim` (`wf_view::Sim`s the frontends run), status, desync reports.
//! * [`transport`]: the datagram trait, a virtual network, a latency / loss conditioner, host-fed queues (browser);
//!   `matchbox` (feature `webrtc`): native WebRTC through a Matchbox signaling server (`wf-signal`, feature `server`).
//! * [`cli`]: the `--net-*` options of the native shell and its headless netplay runner.

pub mod blob;
pub mod cli;
pub mod core_game;
pub mod dojo;
pub mod game;
pub mod ice;
pub mod lobby;
#[cfg(all(feature = "webrtc", not(target_arch = "wasm32")))]
pub mod matchbox;
#[cfg(all(feature = "webrtc", not(target_arch = "wasm32")))]
pub mod wsclient;
pub mod netsim;
pub mod proto;
pub mod session;
pub mod spectator;
pub mod synctest;
pub mod testnet;
pub mod trade;
pub mod transport;

pub use core_game::{CoreGame, MatchConfig};
pub use game::Game;
pub use netsim::{NetLobby, NetSim, NetStatus, SyncSim};
pub use session::{NetConfig, NetEvent, NetStats, P2PSession};
pub use wf_core;
