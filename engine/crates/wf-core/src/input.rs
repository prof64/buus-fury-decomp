//! Per-player input of one frame (GBA KEYINPUT order). `pressed` / `released` are edges against the previous
//! frame's `held`; [`InputFrame::from_held`] derives them.

use serde::{Deserialize, Serialize};

pub const A: u16 = 1 << 0;
pub const B: u16 = 1 << 1;
pub const SELECT: u16 = 1 << 2;
pub const START: u16 = 1 << 3;
pub const RIGHT: u16 = 1 << 4;
pub const LEFT: u16 = 1 << 5;
pub const UP: u16 = 1 << 6;
pub const DOWN: u16 = 1 << 7;
pub const R: u16 = 1 << 8;
pub const L: u16 = 1 << 9;
pub const DPAD: u16 = RIGHT | LEFT | UP | DOWN;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputFrame {
    pub held: u16,
    pub pressed: u16,
    pub released: u16,
}

impl InputFrame {
    pub fn from_held(held: u16, prev_held: u16) -> InputFrame {
        InputFrame {
            held,
            pressed: held & !prev_held,
            released: prev_held & !held,
        }
    }
}

/// button names as the traces write them ("A+RIGHT", "" = none)
pub fn parse_keys(s: &str) -> u16 {
    s.split('+')
        .map(|k| match k.trim() {
            "A" => A,
            "B" => B,
            "SELECT" => SELECT,
            "START" => START,
            "RIGHT" => RIGHT,
            "LEFT" => LEFT,
            "UP" => UP,
            "DOWN" => DOWN,
            "R" => R,
            "L" => L,
            _ => 0,
        })
        .fold(0, |a, b| a | b)
}
