//! The staff credits (mode `0x080335BC(0, unskippable)`, vt `0x08049338`): `run_credits` 0xD1 (unskippable), the
//! title screen's and the Options page's Credits (skippable). 47 pages of string pairs 1355 / 1356 .. 1447 / 1448
//! (text/strings.json): the heading in the dialog font centred at (120, 24) (lines 16 px apart) on BG1 (rows 6..13:
//! screen y 48), the lines in the small font centred at (120, 16) (8 px apart) on BG0 (rows 10..17: y 80), both
//! with the shadowed glyphs (colour 2, shadow 1). Both BGs are alpha blended over the black backdrop (BLDCNT
//! 0x2043, EVB 0) and fade by EVA.
//!
//! Step `0x0803377C` (t starts at 144): skippable and not quitting: A or B pressed -> quit + sound 38; t += 1;
//! t < 128: quitting or DOWN held -> t = 127 (hold skipped); t == 128: next pair (+2; past 1448 -> quit);
//! 129..=144: EVA = 144 - t; t == 144: done when quitting, else the next pair drawn; 145..=160: EVA = t - 144,
//! t == 160 -> 0. EVA is written straight to the register. The enter takes 2 frames (the first step in the 2nd
//! frame after the call), an iteration drawing a new pair 2 frames.

use super::kit::Container;
use super::text::{Canvas, Pen};
use super::{Action, MenuData, Page, Screen, Transition};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuBlend, MenuFont, MenuLayer, MenuView};

/// the first and the end string of the credits (`0x54B`, `0x5A9`)
pub const FIRST: i32 = 0x54B;
pub const END: i32 = 0x5A9;
/// sound of a skip (`0x08319470`)
pub const SFX_SKIP: i32 = 38;
/// no fades around it: the mode replaces the screen at once; the world gets the frame it ends in
pub const CUT: Transition = Transition { fade_out: 0, enter: 0, leave: 1, fade_in: 0 };

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Credits {
    pub c: Container,
    /// +4 frame counter, +8 quitting, +0xC unskippable, +0x10 string index
    pub t: i32,
    pub quit: bool,
    pub unskippable: bool,
    pub idx: i32,
    /// the pair on BG1 / BG0 now
    pub page: i32,
    /// EVA (BLDALPHA, written directly: on screen at once)
    pub eva: i32,
}

impl Credits {
    /// the Options page's / title screen's credits (skippable with A / B)
    pub fn new(_w: &World) -> Credits {
        Credits::with(false)
    }
    pub fn with(unskippable: bool) -> Credits {
        Credits { c: Container::default(), t: 0x90, quit: false, unskippable, idx: FIRST, page: FIRST, eva: 0 }
    }
}

impl Page for Credits {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn step(&mut self, w: &mut World) -> Action {
        // the registers change every iteration: redraw (the picture is cheap; no lag)
        self.c.drawn = false;
        let (pressed, held) = (w.menu.pressed, w.menu.held);
        if !self.unskippable && !self.quit && pressed & (crate::input::A | crate::input::B) != 0 {
            self.quit = true;
            w.sfx(SFX_SKIP, 64);
        }
        self.t += 1;
        if self.t < 0x80 {
            if self.quit || held & crate::input::DOWN != 0 {
                self.t = 0x7F;
            }
        } else if self.t == 0x80 {
            self.idx += 2;
            if self.idx >= END {
                self.quit = true;
                self.idx = FIRST;
            }
        } else if self.t <= 0x90 {
            self.eva = 0x90 - self.t;
            if self.t == 0x90 {
                if self.quit {
                    // the mode ends now: the screen keeps this iteration's registers
                    w.menu.shown = Some(Box::new(Screen::Credits(self.clone())));
                    return Action::Pop(1);
                }
                self.page = self.idx;
            }
        } else if self.t <= 0xA0 {
            self.eva = (self.t + 0x70) & 0xFF;
            if self.t == 0xA0 {
                self.t = 0;
            }
        }
        Action::None
    }
    /// the enter (VRAM / palette clears, the first pair) takes 2 frames
    fn enter_lag(&self) -> u8 {
        1
    }
    /// a new pair drawn (two surfaces + 2 x 15 KB uploads) makes the iteration take 2 frames
    fn redraw_cost(&self) -> u8 {
        if self.t == 0x90 {
            2
        } else {
            1
        }
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut title = Canvas::new();
        let mut p = Pen::new(MenuFont::Dialog, 120, 24, 1, 2, Some(1));
        p.line_h = 16;
        p.clip = [0, 0, 240, 64];
        title.text(t, &mut p, t.s(self.page as usize));
        let mut names = Canvas::new();
        p.font = MenuFont::Small;
        p.x = 120;
        p.y = 16;
        p.line_h = 8;
        names.text(t, &mut p, t.s(self.page as usize + 1));
        MenuView {
            layers: vec![
                MenuLayer { w: 240, h: 64, x: 0, y: 48, priority: 3, ops: title.ops },
                MenuLayer { w: 240, h: 64, x: 0, y: 80, priority: 3, ops: names.ops },
            ],
            backdrop: 0,
            blend: Some(MenuBlend { first: 0b11, second: 1 << 30, eva: self.eva.clamp(0, 16) as u8, evb: 0 }),
            ..Default::default()
        }
    }
}

/// `run_credits` (0xD1): unskippable; pushes nothing; `World.menu.result` = 1 when it ends
pub fn open_credits(w: &mut World) {
    let held = w.input[0].held;
    super::open(w, Screen::Credits(Credits::with(true)), CUT, true);
    w.menu.phase = super::Phase::Run;
    w.menu.held = held;
    if let Some(Screen::Credits(mut m)) = w.menu.stack.pop() {
        m.c.drawn = true;
        w.menu.shown = Some(Box::new(Screen::Credits(m.clone())));
        w.menu.lag = m.enter_lag();
        w.menu.stack.push(Screen::Credits(m));
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "credits", "skippable": bool}`
pub fn open_json(_w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    if v["kind"].as_str()? != "credits" {
        return None;
    }
    let c = Credits::with(!v["skippable"].as_bool().unwrap_or(false));
    Some((Screen::Credits(c), Transition { enter: 1, ..CUT }, true))
}
