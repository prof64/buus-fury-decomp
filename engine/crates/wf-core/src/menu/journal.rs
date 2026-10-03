//! The Journal page (ctor `0x08014188`, vt `0x080498C4`, 0x574 bytes; enter `0x0801425C`, redraw `0x08014526`,
//! list `0x08014424`, events `0x08014588`, done check `0x080146B6`).
//!
//! Built from the quest table `0x08720564` (40 × {star, string, hidden / completed / goal condition scripts}) when
//! the page is created: hidden → skipped, else completed → the Completed Goals list, else goal → the Goals list.
//! LEFT / RIGHT pick the list (tab headers `0x08015C62`), the UP / DOWN prompts scroll it (3 entries shown, each
//! word-wrapped to 176 px and centred on its row by its line count, with its star), L → Options, R → Status.

use super::kit::{self, Button, Container, Font, Label, TabBox, Widget, BTN_B, BTN_DOWN, BTN_L, BTN_R, BTN_UP};
use super::text::{count_lines, Canvas, Pen};
use super::{Action, MenuData, Page, Screen};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuView};

/// video frames one redraw takes (`timing.py`)
pub const REDRAW_FRAMES: u8 = 4;

/// star pictures by the quest's star byte (`0x086B8028`)
const STARS: [&str; 3] = ["ui/images/star_gold.png", "ui/images/star_silver.png", "ui/images/star_bronze.png"];

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Journal {
    pub c: Container,
    /// +0x11C: 0 Goals, 1 Completed Goals
    pub tab: u8,
    /// +0x128: first entry shown
    pub scroll: i32,
    /// +0x12C / +0x1D0: (star, string id)
    pub goals: Vec<(u8, u16)>,
    pub done: Vec<(u8, u16)>,
    pub up: usize,
    pub down: usize,
    pub tabs: [usize; 2],
}

/// a quest condition script (bytecode run by the VM `0x08004038`; the core evaluates the 115 condition scripts'
/// subset of natives, `story::eval_condition`); a script it cannot run counts as false
fn cond(w: &World, code: &[u8]) -> bool {
    if code.is_empty() {
        return false;
    }
    crate::story::eval_condition(code, &w.story, w.story.current).unwrap_or(false)
}

impl Journal {
    pub fn new(w: &World) -> Journal {
        let d = w.data.menu.as_ref().expect("menu content");
        let t = &d.text;
        let mut goals = Vec::new();
        let mut done = Vec::new();
        for q in &d.opts.quests {
            if cond(w, &q.code[0]) {
                continue;
            }
            if cond(w, &q.code[1]) {
                done.push((q.star, q.text));
            } else if cond(w, &q.code[2]) {
                goals.push((q.star, q.text));
            }
        }
        let mut c = Container::default();
        c.add(Widget::Button(Button::new(BTN_B, 0xC, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
        c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
        c.add(Widget::Label(Label { x: 0x22, y: 0x14, text: t.s(2).to_vec(), colour: 2, align: 0, font: Font::Menu }));
        c.add(Widget::Label(Label { x: 0xCE, y: 0x14, text: t.s(28).to_vec(), colour: 2, align: 2, font: Font::Menu }));
        let up = c.add(Widget::Button(Button::new(BTN_UP, 0xD4, 0x28, 2, true, false)));
        let down = c.add(Widget::Button(Button::new(BTN_DOWN, 0xD4, 0x70, 3, true, false)));
        let t0 = TabBox::new(t, 0x18, 0x20, t.s(31), true);
        let x1 = t0.rect[2] - t0.rect[0] + 0x1C;
        let tab0 = c.add(Widget::TabBox(t0));
        let tab1 = c.add(Widget::TabBox(TabBox::new(t, x1, 0x20, t.s(32), false)));
        Journal { c, tab: 0, scroll: 0, goals, done, up, down, tabs: [tab0, tab1] }
    }

    fn list(&self) -> &[(u8, u16)] {
        if self.tab == 0 {
            &self.goals
        } else {
            &self.done
        }
    }

    fn set_btn(&mut self, i: usize, visible: bool, enabled: bool) {
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(i) {
            b.visible = visible;
            b.enabled = enabled;
        }
    }
}

impl Page for Journal {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn before_draw(&mut self, _w: &World) {
        // 0x08014424's side effects: the scroll prompts
        let n = self.list().len() as i32;
        let (up, down, s) = (self.up, self.down, self.scroll);
        if n == 0 {
            self.set_btn(up, false, false);
            self.set_btn(down, false, false);
        } else {
            self.set_btn(up, true, s != 0);
            self.set_btn(down, true, n > s + 3);
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| match code {
            1 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(Action::Close))
            }
            2 | 3 => {
                p.scroll += if code == 2 { -1 } else { 1 };
                w.sfx(super::SFX_CURSOR, 64);
                p.c.drawn = false;
                (false, None)
            }
            6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(Action::Replace(super::page_screen(w, 4, code == 7))))
            }
            _ => (false, None),
        });
        if let Some(a) = act {
            return a;
        }
        if consumed {
            return Action::None;
        }
        let p = w.menu.pressed;
        if p & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::Done;
        }
        for (bit, tab) in [(crate::input::LEFT, 0u8), (crate::input::RIGHT, 1u8)] {
            if p & bit != 0 && self.tab != tab {
                self.tab = tab;
                self.scroll = 0;
                for (k, i) in self.tabs.iter().enumerate() {
                    if let Some(Widget::TabBox(tb)) = self.c.kids.get_mut(*i) {
                        tb.active = k == tab as usize;
                    }
                }
                w.sfx(super::SFX_MENU, 64);
                self.c.drawn = false;
            }
        }
        Action::None
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        kit::page_frame(t, &mut cv, 30);
        let list = self.list();
        if !list.is_empty() {
            separator(&mut cv, 0xDB, 0x37, 0x70);
            let mut p = Pen::new(MenuFont::Menu, 0x20, 0, 0, 2, Some(1));
            p.line_h = 14;
            let fm = t.font(MenuFont::Menu);
            for k in 0..3 {
                let idx = self.scroll + k;
                if idx < 0 || idx as usize >= list.len() {
                    break;
                }
                let (star, s) = list[idx as usize];
                let text = t.s(s as usize);
                let n = count_lines(fm, text, 0xB0);
                p.x = 0x20;
                p.y = 0x43 + 28 * k - 7 * n;
                cv.text_wrapped(t, &mut p, 0xB0, text);
                if let Some(img) = STARS.get(star as usize) {
                    cv.image(img, 0, 16, 16, 0x0E, 28 * k + 0x31);
                }
            }
        }
        self.c.draw(t, &mut cv);
        super::surface_view(cv)
    }
    fn redraw_cost(&self) -> u8 {
        REDRAW_FRAMES
    }
    /// journal_scroll / journal_start / journal_new_game: 36 redraws, residual std 1.5k, max 6k (the empty list)
    /// the idle step measured on this page (`re/buu/menu/cycles.py --all`)
    fn idle_cycles(&self) -> Option<i64> {
        Some(2_784)
    }
    fn redraw_time(&self, w: &World, d: &MenuData) -> Option<crate::sndsim::Redraw> {
        Some(super::page_time(d, &self.view(w, d), !self.c.ever, 2_784, 184_670, 882, 597, 188_273))
    }
}

/// `0x080051B0` (status.rs has the same): the vertical green separator
pub(super) fn separator(cv: &mut Canvas, x: i32, y0: i32, y1: i32) {
    cv.pixel(x, y0, 0x2F);
    let mut y = y0 + 1;
    for _ in 0..(y1 - y0 - 2).max(0) {
        cv.pixel(x - 1, y, 0x2F);
        cv.pixel(x, y, 0x30);
        cv.pixel(x + 1, y, 0x31);
        y += 1;
    }
    cv.pixel(x, y, 0x31);
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "journal"}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(Screen, super::Transition, bool)> {
    (v["kind"] == "journal").then(|| (Screen::Journal(Journal::new(w)), super::PAUSE, false))
}
