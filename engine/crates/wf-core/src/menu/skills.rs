//! The Skills sub-page of Status (ctor `0x08017160(0, char)`, vt `0x0804B024`, 0x12C bytes; enter `0x080171A8`,
//! redraw `0x08017350`, events `0x080174E8`, done check `0x080175FC`, cursor point `0x0801765E`), run as a nested
//! mode by Status' DOWN prompt. Result: 1 B (back to Status), 2 L (Journal), 3 R (Inventory), 4 START (close).
//!
//! The known skills two per row (two rows shown: x 64 / 160, y 40 / 92): the name word-wrapped to 96 px and
//! centred (colour 6 selected, else 2), the icon (`0x08029138`: `ui/menu/skill_icon_NNN.png`) under it, "Level %d"
//! (the chosen level + 1) for skills with levels. LEFT / RIGHT flip the column, the UP / DOWN prompts (with more
//! than two skills) move a row. A opens the description popup (vt `0x0804AFF8`, its own mode: a 208x104 BG0
//! window at (16, 40) over the page; LEFT / RIGHT prompts pick the chosen level between 0 and the learned one, B
//! closes it).
//!
//! Kept quirk: the first row shown follows the selection only down to (sel & ~1) - 4 (`0x08017392`), so with five
//! or more skills the selection can leave the two visible rows.

use super::kit::{self, Button, Container, Cursor, Font, Label, Widget, BTN_A, BTN_B, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R, BTN_RIGHT, BTN_UP};
use super::text::{Arg, Canvas, Pen};
use super::{Action, MenuData, Page, Screen};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuLayer, MenuView};

/// video frames one redraw takes (`timing.py`)
pub const REDRAW_FRAMES: u8 = 3;
pub const POPUP_FRAMES: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Skills {
    pub c: Container,
    /// +0x114: the character
    pub ci: i32,
    /// +0x118: known skills (record +0xC)
    pub count: i32,
    /// +0x11C: first skill shown
    pub first: i32,
    /// +0x120: the selected skill
    pub sel: i32,
    pub a_btn: usize,
    pub up: Option<usize>,
    pub down: Option<usize>,
    /// the description popup (a nested mode)
    pub popup: Option<Box<Popup>>,
    /// the first row when the cursor child was drawn (before this draw's update of `first`)
    pub cursor_first: i32,
}

/// the skill description popup (`0x08029014` enter, `0x08029192` redraw, `0x080292E0` events)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Popup {
    pub c: Container,
    pub left: Option<usize>,
    pub right: Option<usize>,
    /// the page as it was last drawn (BG2 under the window)
    pub under: Box<Skills>,
    /// closed (B): its exit stub iteration runs, then the page resumes
    pub closing: bool,
}

fn skill_id(w: &World, ci: i32, k: i32) -> i32 {
    // 0x080135A2: no skills -> entry 0
    let cs = w.story.chars.get(ci.max(0) as usize);
    match cs {
        Some(c) if !c.skills.is_empty() => c.skills.get(k.max(0) as usize).copied().unwrap_or(0),
        _ => 0,
    }
}

impl Skills {
    /// `0x08017160` + enter `0x080171A8`
    pub fn new(w: &World, ci: i32) -> Skills {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let count = w.story.chars.get(ci.max(0) as usize).map(|c| c.skills.len() as i32).unwrap_or(0);
        let mut c = Container::default();
        let a_btn = c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: t.s(23).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let x = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, x, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: x + 0x10, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let (mut up, mut down) = (None, None);
        if count > 2 {
            up = Some(c.add(Widget::Button(Button::new(BTN_UP, 0xD4, 0x28, 2, false, false))));
            down = Some(c.add(Widget::Button(Button::new(BTN_DOWN, 0xD4, 0x70, 3, true, false))));
        }
        c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
        c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
        c.add(Widget::Label(Label { x: 0x22, y: 0x14, text: t.s(30).to_vec(), colour: 2, align: 0, font: Font::Menu }));
        c.add(Widget::Label(Label { x: 0xCE, y: 0x14, text: t.s(33).to_vec(), colour: 2, align: 2, font: Font::Menu }));
        c.add(Widget::Cursor(Cursor { t: 0 }));
        Skills { c, ci, count, first: 0, sel: 0, a_btn, up, down, popup: None, cursor_first: 0 }
    }

    /// `0x0801765E`: the cursor point of the selected skill
    fn cursor_point(&self) -> Option<(i32, i32)> {
        if self.count == 0 {
            return None;
        }
        let r = self.sel - self.cursor_first;
        Some(((r & 1) * 0x60 + 0x10, (r >> 1) * 0x34 + 0x25))
    }

    fn event(&mut self, w: &mut World, code: i32) -> (bool, Option<Action>) {
        match code {
            0 => {
                // the description popup (its own mode over the page as last drawn)
                w.sfx(super::SFX_MENU, 64);
                let under = match w.menu.shown.as_deref() {
                    Some(Screen::Skills(s)) => Box::new(s.clone()),
                    _ => Box::new(self.clone()),
                };
                self.popup = Some(Box::new(Popup::new(w, self, under)));
                (false, None)
            }
            1 | 6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(Action::Pop(match code {
                    1 => 1,
                    6 => 2,
                    _ => 3,
                })))
            }
            2 => {
                w.sfx(super::SFX_CURSOR, 64);
                self.sel -= 2;
                if self.sel < 0 {
                    self.sel = 0;
                }
                self.c.drawn = false;
                (false, None)
            }
            3 => {
                w.sfx(super::SFX_CURSOR, 64);
                self.sel += 2;
                if self.sel >= self.count {
                    self.sel = self.count - 1;
                }
                self.c.drawn = false;
                (false, None)
            }
            _ => (false, None),
        }
    }

    fn draw(&self, w: &World, d: &MenuData) -> wf_view::MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        // status_page(.., "Status", children): the title, the children, then the page content
        kit::page_frame(t, &mut cv, 28);
        self.c.draw(t, &mut cv);
        // the cursor child (last) at the page's own cursor point
        if let Some(Widget::Cursor(c)) = self.c.kids.last() {
            if c.t & 8 == 0 {
                if let Some((x, y)) = self.cursor_point() {
                    cv.image("ui/images/cursor.png", 0, 16, 16, x - 16, y - 8);
                }
            }
        }
        if self.count > 2 {
            super::journal::separator(&mut cv, 0xDB, 0x37, 0x70);
            hsep(&mut cv, 0x28, 0x50, 0x58);
            hsep(&mut cv, 0x88, 0x50, 0xB8);
        }
        let cs = w.story.chars.get(self.ci.max(0) as usize).cloned().unwrap_or_default();
        let mut p = Pen::new(MenuFont::Menu, 0, 0, 1, 2, Some(1));
        p.line_h = 12;
        for k in 0..4 {
            let idx = self.first + k;
            if idx >= self.count {
                break;
            }
            let x = (k & 1) * 0x60 + 0x40;
            let y = (k >> 1) * 0x34 + 0x28;
            let sid = skill_id(w, self.ci, idx);
            let sk = d.opts.skills.get(sid.max(0) as usize).cloned().unwrap_or_default();
            p.x = x;
            p.y = y;
            p.colour = if self.sel == idx { 6 } else { 2 };
            cv.text_wrapped(t, &mut p, 0x60, t.s(sk.name as usize));
            cv.image(&sk.icon, 0, 24, 11, p.x - 12, p.y - 8);
            if sk.level_byte != 0xFF {
                p.y += 12;
                cv.textf(t, &mut p, t.s(151), &[Arg::I(cs.skill_cur(idx as usize) + 1)]);
            }
        }
        super::surface_view(cv)
    }
}

/// `0x08005154`: a horizontal green separator (x0, y)..(x1, y + 2): 0x2F 0x30.. 0x31 / 0x2F.. / 0x31..
fn hsep(cv: &mut Canvas, x0: i32, y: i32, x1: i32) {
    let n = x1 - x0;
    cv.pixel(x0, y, 0x2F);
    if n - 2 > 0 {
        cv.fill(x0 + 1, y, x0 + 1 + (n - 2), y + 1, 0x30);
    }
    cv.pixel(x0 + n - 1, y, 0x31);
    if n - 2 > 0 {
        cv.fill(x0 + 1, y - 1, x0 + 1 + (n - 2), y, 0x2F);
        cv.fill(x0 + 1, y + 1, x0 + 1 + (n - 2), y + 2, 0x31);
    }
}

impl Popup {
    fn new(w: &World, s: &Skills, under: Box<Skills>) -> Popup {
        let d = w.data.menu.as_ref().expect("menu content");
        let t = &d.text;
        let mut c = Container::default();
        c.add(Widget::Button(Button::new(BTN_B, 0xC, 0x4C, 1, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x58, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let sid = skill_id(w, s.ci, s.sel);
        let leveled = d.opts.skills.get(sid.max(0) as usize).is_some_and(|k| k.level_byte != 0xFF);
        let (mut left, mut right) = (None, None);
        if leveled {
            left = Some(c.add(Widget::Button(Button::new(BTN_LEFT, 0x8C, 0x4D, 4, true, false))));
            right = Some(c.add(Widget::Button(Button::new(BTN_RIGHT, 0xB4, 0x4D, 5, true, false))));
        }
        Popup { c, left, right, under, closing: false }
    }
}

fn set_cur(w: &mut World, ci: i32, k: i32, delta: i32) {
    let Some(cs) = w.story.chars.get_mut(ci.max(0) as usize) else { return };
    let k = k.max(0) as usize;
    if cs.skill_cur.len() < cs.skills.len() {
        let mut v: Vec<i32> = (0..cs.skills.len()).map(|i| cs.skill_cur(i)).collect();
        std::mem::swap(&mut cs.skill_cur, &mut v);
    }
    if let Some(x) = cs.skill_cur.get_mut(k) {
        *x = (*x + delta) & 0xFF;
    }
}

impl Page for Skills {
    fn container(&mut self) -> &mut Container {
        match self.popup.as_mut() {
            Some(p) => &mut p.c,
            None => &mut self.c,
        }
    }
    fn container_ref(&self) -> &Container {
        match self.popup.as_ref() {
            Some(p) => &p.c,
            None => &self.c,
        }
    }
    fn before_draw(&mut self, w: &World) {
        if self.popup.is_some() {
            let cs = w.story.chars.get(self.ci.max(0) as usize).cloned().unwrap_or_default();
            let k = self.sel.max(0) as usize;
            let (cur, max) = (cs.skill_cur(k), cs.skill_levels.get(k).copied().unwrap_or(0));
            let p = self.popup.as_mut().unwrap();
            if let (Some(l), Some(r)) = (p.left, p.right) {
                if let Some(Widget::Button(b)) = p.c.kids.get_mut(l) {
                    b.enabled = cur != 0;
                }
                if let Some(Widget::Button(b)) = p.c.kids.get_mut(r) {
                    b.enabled = cur != max;
                }
            }
            return;
        }
        // 0x08017350: the children (cursor included) are drawn first, with the first row and the prompts as the
        // last draw left them; then the first row follows the selection (content), then the prompts (after_draw)
        self.cursor_first = self.first;
        let s2 = self.sel & !1;
        if self.first < s2 - 4 {
            self.first = s2 - 4;
        }
        if self.first > self.sel {
            self.first = s2;
        }
        if self.first < 0 {
            self.first = 0;
        }
    }
    fn after_draw(&mut self, _w: &World) {
        if self.popup.is_some() {
            return;
        }
        if self.count > 2 {
            let (sel, n) = (self.sel, self.count);
            if let Some(Widget::Button(b)) = self.up.and_then(|i| self.c.kids.get_mut(i)) {
                b.enabled = sel >= 2;
            }
            if let Some(Widget::Button(b)) = self.down.and_then(|i| self.c.kids.get_mut(i)) {
                b.enabled = (sel >> 1) * 2 < ((n - 1) >> 1) * 2;
            }
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        if let Some(mut p) = self.popup.take() {
            if p.closing {
                // the exit stub's iteration is over: the page resumes (its redraw follows)
                return Action::None;
            }
            let (ci, sel) = (self.ci, self.sel);
            let mut close = false;
            let (_c, act) = kit::run_step(&mut *p, w, |p| &mut p.c, |p, w, _src, code| match code {
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    close = true;
                    (true, None)
                }
                4 | 5 => {
                    set_cur(w, ci, sel, if code == 4 { -1 } else { 1 });
                    p.c.drawn = false;
                    (false, None)
                }
                _ => (false, None),
            });
            debug_assert!(act.is_none());
            if close {
                // the popup's dtor hides BG0 at once: the page as last drawn shows until its redraw lands
                w.menu.shown = Some(Box::new(Screen::Skills((*p.under).clone())));
                p.closing = true;
                p.c.drawn = true;
                self.popup = Some(p);
                return Action::None;
            }
            self.popup = Some(p);
            return Action::None;
        }
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| p.event(w, code));
        if let Some(a) = act {
            return a;
        }
        if self.popup.is_some() || consumed {
            return Action::None;
        }
        let pr = w.menu.pressed;
        if pr & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::PopDone(4);
        }
        if pr & (crate::input::LEFT | crate::input::RIGHT) != 0 {
            w.sfx(super::SFX_CURSOR, 64);
            self.sel ^= 1;
            if self.sel >= self.count {
                self.sel = self.count - 1;
            }
            self.c.drawn = false;
        }
        Action::None
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        match &self.popup {
            None => self.draw(w, d),
            Some(p) => {
                let mut v = p.under.draw(w, d);
                v.layers.push(MenuLayer { w: 208, h: 104, x: 16, y: 40, priority: 0, ops: p.draw(w, d, self).ops });
                v
            }
        }
    }
    fn redraw_cost(&self) -> u8 {
        if self.popup.is_some() {
            POPUP_FRAMES
        } else {
            REDRAW_FRAMES
        }
    }
    /// the page without its popup: skills_basic / skills_levels / skills_start, 54 redraws, residual std 0.6k, max
    /// 2k cycles (the popup's iterations keep the constant: six measured, not separated yet)
    fn redraw_time(&self, w: &World, d: &MenuData) -> Option<crate::sndsim::Redraw> {
        if self.popup.is_some() {
            // the popup's surface alone (two popups measured, first iteration and redraw each: exact on those)
            let v = self.view(w, d);
            let top = v.layers.get(1)?;
            let gn = crate::sndsim::ops_cost(&d.text, &top.ops).0;
            let k = if self.container_ref().ever { 129_600 + gn * 567 / 1000 } else { 239_500 + gn * 63 / 1000 };
            let own = k + crate::sndsim::layer_cycles(&d.text, &top.ops, 0);
            return Some(crate::sndsim::Redraw { t0: 4_641, base: own - 4_641 });
        }
        Some(super::page_time(d, &self.view(w, d), !self.c.ever, 4_641, 146_570, 0, 1_371, 129_107))
    }
}

impl Popup {
    /// `0x08029192`
    fn draw(&self, w: &World, d: &MenuData, s: &Skills) -> Canvas {
        let t = &d.text;
        let mut cv = Canvas::new();
        // 0x08005096: the window frame on the 208x104 surface
        let (sw, sh) = (208, 104);
        cv.fill(0, 0, 8, sh - 8, 0);
        cv.fill(sw - 8, 0, sw, sh - 8, 0);
        cv.fill(0, sh - 8, sw, sh, 0);
        cv.background(12, 4, sw - 24, sh - 16);
        cv.panel(8, 0, sw - 8, sh - 8);
        let sid = skill_id(w, s.ci, s.sel);
        let sk = d.opts.skills.get(sid.max(0) as usize).cloned().unwrap_or_default();
        cv.image(&sk.icon, 0, 24, 11, 0x0E, 6);
        let mut p = Pen::new(MenuFont::Menu, 0x28, 0x0F, 0, 5, Some(1));
        p.line_h = 12;
        p.clip = [0, 0, sw, sh];
        cv.text(t, &mut p, t.s(sk.name as usize));
        p.colour = 0x75;
        p.x = 0x0E;
        p.y = 0x1B;
        cv.text_wrapped(t, &mut p, 0xB4, t.s(sk.desc as usize));
        if sk.level_byte != 0xFF {
            let cs = w.story.chars.get(s.ci.max(0) as usize).cloned().unwrap_or_default();
            p.colour = 3;
            p.x = 0xA9;
            p.y = 0x58;
            p.align = 1;
            cv.textf(t, &mut p, &[0x25, 0x64], &[Arg::I(cs.skill_cur(s.sel.max(0) as usize) + 1)]);
        }
        self.c.draw(t, &mut cv);
        cv
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "skills", "char": n}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(Screen, super::Transition, bool)> {
    (v["kind"] == "skills").then(|| (Screen::Skills(Skills::new(w, v["char"].as_i64().unwrap_or(0) as i32)), super::PAUSE, false))
}
