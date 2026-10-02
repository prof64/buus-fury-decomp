//! `wf-script`: the story/event bytecode of *Dragon Ball Z: Buu's Fury* (Webfoot engine).
//!
//! * [`bytecode`]: instruction set, decoder, disassembler text.
//! * [`rom`]: the resource table `0x0879B704` and inline scripts → [`rom::ScriptSet`].
//! * [`natives`]: the 248 builtins (signatures and names from `engine/annotations/script_natives.json`).
//! * [`vm`]: exact, resumable, snapshot-able interpreter with a [`vm::Host`] trait for engine effects.
//! * [`decompile`] + [`ir`]: bytecode → structured IR; printers in [`print`].
//!
//! Spec: `re/buu/SCRIPT.md`; scripting API for mods: `engine/docs/scripting.md`.
//! No file system access; builds for `wasm32-unknown-unknown`.

pub mod analysis;
pub mod bytecode;
pub mod decompile;
pub mod export;
pub mod ir;
pub mod natives;
pub mod rhai_out;
pub mod rom;
pub mod steps;
pub mod testhost;
pub mod vm;

pub use rom::{Script, ScriptId, ScriptSet};
pub use vm::{Call, Globals, Host, NativeResult, Status, Thread};
