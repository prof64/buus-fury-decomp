//! wf-core: the deterministic simulation core of the Buu's Fury remake.
//!
//! * `World` holds every bit of simulation state as plain data (`Clone + Hash`); game data is shared behind an Arc.
//! * The only external input is the per-player [`input::InputFrame`]; [`session::Session::step`] advances one
//!   frame. Sounds and other presentation cues leave as [`world::Event`] data; nothing here opens windows or
//!   plays audio.
//! * Integer / fixed-point math with the game's own RNG; see `engine/docs/core.md`.
pub mod actor;
pub mod ai;
pub mod battle;
pub mod content;
pub mod coverage;
pub mod damage;
pub mod eanim;
pub mod enemy;
pub mod field;
pub mod fieldcontent;
pub mod fieldnpc;
pub mod fieldobj;
pub mod fieldpatrol;
pub mod fieldview;
pub mod fieldworld;
pub mod fixed;
pub mod fx;
pub mod geom;
pub mod input;
pub mod menu;
pub mod modkit;
pub mod nodes;
pub mod observe;
pub mod path;
pub mod player;
pub mod progression;
pub mod rng;
pub mod scene;
pub mod session;
pub mod shots;
pub mod snapshot;
pub mod sndsim;
pub mod story;
pub mod tables;
pub mod view;
pub mod vram;
pub mod world;
