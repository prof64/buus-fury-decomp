//! wf-mcp: the agent interface to the deterministic Buu's Fury core (`wf-core`).
//!
//! * [`tools::Server`]: the tool API (discovery, battles, observations, macros, time travel, replays, Rhai
//!   controllers, multi-seed evaluation, co-op slots, screenshots) behind one `call(name, args)`.
//! * `bin/wf-mcp`: a stdio MCP server over it (JSON-RPC 2.0, newline-delimited).
//! * [`capi`]: a C ABI (gym-style fast path + the whole tool API as JSON) for Python (`python/wf_gym.py`).
//!
//! Everything is headless and muted: nothing here opens a window or an audio device. See `agent/DESIGN-rust.md`.
pub mod capi;
pub mod catalog;
pub mod content;
pub mod controller;
pub mod env;
pub mod macros;
pub mod mcp;
pub mod obs;
pub mod tools;
