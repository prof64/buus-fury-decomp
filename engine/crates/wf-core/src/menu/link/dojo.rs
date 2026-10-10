//! The Dojo's link pages: the host's setup page and "Waiting for a friend..." (the character pick is `chars.rs`).
//!
//! * **Setup** (host only; vt `0x0804A3BC`: enter `0x080237F0`, step `0x08023C20`, events `0x08023AAA`, redraw
//!   `0x08023A78`): the page frame with the heading "Dojo" (95), "Character Mode:" (99) at (28, 40) over two
//!   selectable boxes BALANCED MODE / IN-GAME MODE (97 / 98, the tab box class `0x08015C62` at (28, 44) / (120, 44)),
//!   "Rounds:" (100) at (28, 82) over 1 / 3 / 5 (101..103 at (28, 86) / (68, 86) / (108, 86)), START FIGHT (96) at
//!   (156, 136); a box's colour is green (index 1) when chosen, red (0) when not. The A / B prompts (A + "Select" at
//!   (12, 132) / (28, 144), B + "Cancel" after it) and the blinking cursor, which starts on START FIGHT. The arrows
//!   move between the boxes (sound 39); A (on its release, the A prompt's event 0) chooses the box the cursor is on
//!   (no sound): the mode byte of the link config (`[0x0872C34C + 4]` byte 0: 1 Balanced, 0 In-Game), the rounds
//!   code (+4: 0 / 2 / 4 for 1 / 3 / 5 rounds, 3 at the start = 3 rounds); A on START FIGHT runs the wait box. B
//!   (no sound): back to Select Saved Game (a new list). The step ends the page when the link is down
//!   (`[0x03002F50]`: back to the Multiplayer page).
//! * **Wait box** (vt `0x0804B550`: enter `0x08024574` = the title's box `0x08024120` + the link message object vt
//!   `0x0804A1F4`; step `0x080245D8`; events `0x080242C2`): "Waiting for a friend..." with B + "Cancel" on a 136x56 BG0
//!   box at (56, 64), run nested over the page that opened it (the page is not updated meanwhile: its picture and
//!   figures stay as they were). The host's message carries the link config, the guest's whether it has it; the
//!   friend's answer sets `[0x03002F54]` ([`super::Friend::answered`]): the box then counts its steps and ends with 2
//!   on the 11th (`0x08040182` clears the flag); B ends it with 1 (back to the page).

use super::super::kit::{self, Button, Container, Cursor, Label, TabBox, Widget, BTN_A, BTN_B};
use super::super::text::{Canvas, TextData};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::MenuFont;

/// the setup page's boxes: (string, x, y, id, nav up / down / left / right) in creation order
const BOXES: [(usize, i32, i32, i32, [i32; 4]); 6] = [
    (97, 0x1C, 0x2C, 0, [-1, 2, -1, 1]),
    (98, 0x78, 0x2C, 1, [-1, 2, 0, -1]),
    (101, 0x1C, 0x56, 2, [0, 9, -1, 4]),
    (102, 0x44, 0x56, 4, [0, 9, 2, 6]),
    (103, 0x6C, 0x56, 6, [0, 9, 4, -1]),
    (96, 0x9C, 0x88, 9, [2, -1, -1, -1]),
];

/// the host's setup page
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Setup {
    pub c: Container,
    /// child indices of the boxes (in [`BOXES`] order) and of the A prompt
    pub boxes: [usize; 6],
    pub a_btn: usize,
    /// Character Mode: Balanced (else In-Game)
    pub balanced: bool,
    /// the rounds code: 0 / 2 / 4 chosen (1 / 3 / 5 rounds), 3 at the start (3 rounds)
    pub rounds_code: i32,
    /// the wait box ended with B: the page's picture is the one under the box (the A prompt pressed) until its
    /// redraw in the next iteration
    pub unpress: bool,
}

impl Setup {
    /// enter `0x080237F0`
    pub fn new(t: &TextData) -> Setup {
        let mut c = Container::default();
        let label = |s: usize, x: i32, y: i32| {
            Widget::Label(Label {
                x,
                y,
                text: t.s(s).to_vec(),
                colour: 5,
                align: 0,
                font: kit::Font::Menu,
            })
        };
        let mut boxes = [0; 6];
        let mut add_box = |c: &mut Container, k: usize, active: bool| {
            let (s, x, y, id, nav) = BOXES[k];
            let mut b = TabBox::new(t, x, y, t.s(s), active);
            b.id = id;
            b.nav = nav;
            boxes[k] = c.add(Widget::TabBox(b));
        };
        c.add(label(99, 0x1C, 0x28));
        add_box(&mut c, 0, true);
        add_box(&mut c, 1, false);
        c.add(label(100, 0x1C, 0x52));
        add_box(&mut c, 2, false);
        add_box(&mut c, 3, true);
        add_box(&mut c, 4, false);
        add_box(&mut c, 5, true);
        let a_btn = c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(label(23, 0x1C, 0x90));
        let bx = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x84, 1, true, false)));
        c.add(label(26, bx + 0x10, 0x90));
        c.add(Widget::Cursor(Cursor { t: 0 }));
        c.sel = 9;
        Setup { c, boxes, a_btn, balanced: true, rounds_code: 3, unpress: false }
    }

    /// the rounds the setup gives (the fight plays until a player has more than rounds / 2 wins)
    pub fn rounds(&self) -> u8 {
        if self.rounds_code == 3 {
            3
        } else {
            self.rounds_code as u8 + 1
        }
    }

    fn colour(&mut self, k: usize, on: bool) {
        if let Some(Widget::TabBox(b)) = self.c.kids.get_mut(self.boxes[k]) {
            b.active = on;
        }
    }

    /// events `0x08023AAA`, A (code 0) on the selected box: Some(true) = START FIGHT
    pub fn choose(&mut self) -> bool {
        match self.c.sel {
            0 | 1 => {
                self.colour(0, self.c.sel == 0);
                self.colour(1, self.c.sel == 1);
                self.balanced = self.c.sel == 0;
            }
            2 | 4 | 6 => {
                for k in 2..5 {
                    self.colour(k, BOXES[k].3 == self.c.sel);
                }
                self.rounds_code = self.c.sel - 2;
            }
            9 => return true,
            _ => {}
        }
        false
    }

    /// set the page as the harness found it (`open_json`: selection, mode, rounds code, cursor blink)
    pub fn set(&mut self, sel: i32, balanced: bool, rounds_code: i32) {
        self.c.sel = sel;
        self.balanced = balanced;
        self.rounds_code = rounds_code;
        self.colour(0, balanced);
        self.colour(1, !balanced);
        for k in 2..5 {
            self.colour(k, BOXES[k].3 - 2 == rounds_code || (rounds_code == 3 && k == 3));
        }
    }

    /// redraw `0x08023A78`: the page frame, heading "Dojo", the children
    pub fn draw(&self, t: &TextData, cv: &mut Canvas) {
        kit::page_frame(t, cv, 95);
        self.c.draw(t, cv);
    }
}

/// which wait box: what it waits for
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WaitKind {
    /// the save / setup exchange (vt `0x0804B550`, step `0x080245D8`): the flag `0x03002F54`, the 11th step ends it
    Answer,
    /// the character pick (vt `0x0804AFA0`, enter `0x080242EC`, step `0x080243F4`): the friend's pick
    /// (`[0x03002FB0 + 4]`), the 31st step ends it
    Pick,
    /// "No Saved Games" (the box's state 2, string 91) over a Select Saved Game without a saved game: B leaves the
    /// link (two linked emulators with erased saves: menu_parity link_nosaves)
    NoSaves,
}

/// "Waiting for a friend..." nested over a link page
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wait {
    pub kind: WaitKind,
    pub c: Container,
    /// +0x118 / +0x11C: steps since the friend's answer
    pub count: i32,
}

/// how the wait box ended
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitEnd {
    /// B: back to the page under it (1)
    Cancel,
    /// the friend answered (2)
    Done,
}

impl Wait {
    pub fn new(t: &TextData, kind: WaitKind) -> Wait {
        Wait { kind, c: super::super::title::wait_kids(t, if kind == WaitKind::NoSaves { 2 } else { 0 }), count: 0 }
    }

    /// step `0x080245D8` / `0x080243F4`: the friend's answer counted (the 11th / 31st step ends the box), else the
    /// box's controls (B: events `0x080242C2` end it with 1)
    pub fn step(&mut self, w: &mut World, answered: bool) -> Option<WaitEnd> {
        if answered {
            self.count += 1;
            let last = match self.kind {
                WaitKind::Answer => 10,
                WaitKind::Pick => 30,
                WaitKind::NoSaves => i32::MAX - 1,
            };
            if self.count > last {
                return Some(WaitEnd::Done);
            }
        }
        let (_, act) = kit::run_step(self, w, |p| &mut p.c, |_p, _w, _src, code| {
            if code == 1 {
                (true, Some(super::super::Action::Pop(1)))
            } else {
                (false, None)
            }
        });
        act.map(|_| WaitEnd::Cancel)
    }

    /// redraw `0x0802428E`: the box (`0x08005096`) and the controls on a 136x56 surface, BG0 at (56, 64)
    pub fn layer(&self, t: &TextData) -> wf_view::MenuLayer {
        let (pw, ph) = (136, 56);
        let mut pc = Canvas::new();
        super::super::title::popup_box(&mut pc, pw, ph);
        self.c.draw(t, &mut pc);
        wf_view::MenuLayer { w: pw as u16, h: ph as u16, x: 56, y: 64, priority: 0, ops: pc.ops }
    }
}


/// Select Saved Game's A on a game, Dojo (`0x08020054`): the host goes on to the setup page; the guest waits for the
/// host's setup in the wait box over its list (the list was redrawn first: its vt +0x24)
pub fn save_picked(l: &mut super::Link, w: &mut World) {
    let d = std::sync::Arc::clone(&w.data);
    let t = &d.menu.as_ref().expect("menu content").text;
    if l.host {
        if let super::LinkPage::Saves(g) = &mut l.page {
            g.figures_gone(w);
        }
        l.page = super::LinkPage::Setup(Setup::new(t));
        l.figs_shown = None;
    } else {
        l.wait = Some(Wait::new(t, WaitKind::Answer));
    }
}
