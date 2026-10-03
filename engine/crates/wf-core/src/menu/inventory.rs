//! The Inventory page (mode vt `0x08049D48`, ctor `0x08016524`, 0x3D4 bytes; lists `0x080164D0`, enter
//! `0x08016574`, redraw `0x08016962` = frame + list `0x080167A0` + children + the leader's HP / EP gauge, events
//! `0x08016AA8`, done check `0x08016C5C`, cursor point `0x08016928`) and its Use dialog (mode vt `0x0804ACB4`,
//! 0x118 bytes: enter `0x0801E1E8`, redraw `0x0801E318`, events `0x0801E3EA`, done check `0x0801E43C`).
//!
//! Two lists built from the supply counts (GameData +2; supply def `0x083BD2F8` + 0x34 i, byte 0 bit 0 = story
//! item): Supplies / Story Items, switched with LEFT / RIGHT (no sound). Five rows of 18 px from the scroll top;
//! the UP / DOWN repeating prompts move the selection. A on an item hides the cursor, redraws the page at once,
//! then runs the Use dialog as a nested mode (a 208x104 surface on BG0 at (16, 40)): Use (enabled when the supply
//! has a use script, def +0x30) plays sound 41 and runs the script (`0x0801E14C` → the script VM, synchronously:
//! the remake emits `FieldEvent::Run { Callback::Script("inline_0x..."), Cause::ItemUse }` from the step, the
//! script host runs it right after, `menu::post` rebuilds the lists like the game after the dialog returns 1);
//! Cancel plays 40. UP / DOWN in the dialog only toggle an unused selection (sound 39, redraw).

use super::kit::{self, Button, Container, Cursor, Font, Label, TabBox, Widget, BTN_A, BTN_B, BTN_DOWN, BTN_L, BTN_R, BTN_UP};
use super::shop::{text_wrap, vsep};
use super::text::{units, Arg, Canvas, Pen};
use super::{Action, MenuData, Page, Screen};
use crate::world::World;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wf_view::{MenuFont, MenuView};

/// video frames one redraw of the page / the dialog takes (measured: `re/buu/menu/timing.py`)
pub const REDRAW_FRAMES: u8 = 4;
pub const DIALOG_FRAMES: u8 = 2;
/// the timing model (sndsim.rs; emulator measurements `re/buu/ship/menu_timing.py` on the real route's heal menus
/// 8-14 / 8-51 / 8-64, six lists: fit within 250 cycles): the list redraw's constant part, per listed row, per text
/// line; the page's enter (its first redraw); the Use dialog's first iteration on top of the page's redraw; a redraw
/// of the dialog alone (its buttons' press: the dialog surface, not the page under it) and its text lines
const LIST_CYCLES: i64 = 192_506;
const ROW_CYCLES: i64 = 2_933;
const LINE_CYCLES: i64 = 1_251;
/// cycles per 1000 glyph-routine instructions beyond `sndsim::layer_cycles`' 3.17 each (joint fit: the three
/// route menus and the inventory_use scenario's 5-row lists, residuals within 2.5k)
const LIST_GN_MILLI: i64 = 439;
const ENTER_CYCLES: i64 = 141_636;
/// (six dialogs: the redraw alone = 113,741 + 4,000 per line + 3.617 per glyph instruction + pictures; the first
/// iteration = the page's redraw + the dialog's + 60,200, seven cases within 600)
const DIALOG_CYCLES: i64 = 60_200;
const DIALOG_REDRAW_CYCLES: i64 = 113_741;
const DIALOG_LINE_CYCLES: i64 = 4_000;
const DIALOG_GN_MILLI: i64 = 447;
/// the idle step (frame start to the sound update)
const T0: i64 = 3_240;

/// supply def strings and flags (`items/*.json`: name = def +4, plural = +6, description = +8)
#[derive(Clone, Debug, Default)]
pub struct InvData {
    pub name: Vec<Vec<u16>>,
    pub plural: Vec<Vec<u16>>,
    pub desc: Vec<Vec<u16>>,
    pub icon: Vec<String>,
    pub story: Vec<bool>,
    /// the use script (def +0x30) as the script host's id `inline_0xADDR`; None = no script (Use disabled)
    pub script: Vec<Option<String>>,
}

impl InvData {
    pub fn load(src: &dyn crate::content::ContentSource) -> InvData {
        let mut d = InvData::default();
        for p in src.list() {
            if !p.starts_with("items/") || !p.ends_with(".json") || p.ends_with("index.json") {
                continue;
            }
            let Some(b) = src.read(&p) else { continue };
            let Ok(v) = serde_json::from_slice::<Value>(&b) else { continue };
            let Some(i) = v["item"].as_u64() else { continue };
            let i = i as usize;
            if d.name.len() <= i {
                let n = i + 1;
                d.name.resize(n, Vec::new());
                d.plural.resize(n, Vec::new());
                d.desc.resize(n, Vec::new());
                d.icon.resize(n, String::new());
                d.story.resize(n, false);
                d.script.resize(n, None);
            }
            let s = |k: &str| super::text::decode_escaped(v[k].as_str().unwrap_or(""));
            d.name[i] = s("name");
            d.plural[i] = if v["plural"].is_string() { s("plural") } else { s("name") };
            d.desc[i] = s("description");
            d.icon[i] = v["icon"].as_str().unwrap_or("").to_string();
            d.story[i] = v["story_item"].as_bool().unwrap_or(false);
            let sc = v["use_script"].as_str().map(|h| u32::from_str_radix(h.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0)).unwrap_or(0);
            d.script[i] = (sc != 0).then(|| format!("inline_0x{sc:08X}"));
        }
        d
    }
}

/// the Use dialog (nested mode)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UseDialog {
    pub c: Container,
    pub item: i32,
    /// drawn once (the timing model: later redraws are the dialog surface alone)
    #[serde(default)]
    pub shown: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Inventory {
    pub c: Container,
    /// +0x134 supplies, +0x284 story items (supply indices with a count)
    pub lists: [Vec<i32>; 2],
    /// +0x11C tab, +0x120 scroll top, +0x124 selection
    pub tab: u8,
    pub top: i32,
    pub cur: i32,
    /// child indices: UP / DOWN prompts (+0x128 / +0x12C), the tabs (+0x114 / +0x118), the cursor (+0x130)
    pub up: usize,
    pub down: usize,
    pub tabs: [usize; 2],
    pub cursor: usize,
    /// sampled when drawn: the counts of the listed supplies, the leader's HP / EP gauge steps (0..=41)
    pub counts: Vec<u8>,
    pub gauge: (i32, i32),
    pub dialog: Option<UseDialog>,
    /// the dialog ended with this result: the next iteration is its exit stub's (`0x0803D876`), after which the
    /// page's event handler goes on (the lists again for 1) and the page redraws
    pub stub: Option<i32>,
    /// not drawn yet since the page was built (the timing model: the first redraw carries the page's enter)
    #[serde(default)]
    pub fresh: bool,
    /// stands in for the page's container during the stub iteration (drawn: no redraw yet)
    pub hold: Container,
}

fn list_of(w: &World, story: bool) -> Vec<i32> {
    let d = w.data.menu.as_ref();
    (0..w.story.supplies.len().min(83))
        .filter(|&i| w.story.supplies[i] != 0)
        .filter(|&i| d.and_then(|d| d.inv.story.get(i)).copied().unwrap_or(false) == story)
        .map(|i| i as i32)
        .collect()
}

/// the current character's record as the game has it (the live fighter's when one plays it)
pub fn leader_rec(w: &World) -> crate::progression::Record {
    let cur = w.story.current;
    for s in 0..w.players.len() {
        let Some(i) = w.player_idx(s) else { continue };
        if let crate::world::Body::Player(p) = &w.ents[i].body {
            if crate::story::char_of_form(&w.data, p.form) == Some(cur) {
                return p.rec.clone();
            }
        }
    }
    w.story.chars.get(cur.max(0) as usize).map(|c| c.rec.clone()).unwrap_or_default()
}

impl Inventory {
    /// `0x08016524` + enter `0x08016574`
    pub fn new(w: &World) -> Inventory {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let mut c = Container::default();
        c.sel = 2;
        let sel_s = t.s(23);
        c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: sel_s.to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let bx = t.font(MenuFont::Menu).measure(sel_s) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: bx + 0x10, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
        c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
        c.add(Widget::Label(Label { x: 0x22, y: 0x14, text: t.s(28).to_vec(), colour: 2, align: 0, font: Font::Menu }));
        c.add(Widget::Label(Label { x: 0xCE, y: 0x14, text: t.s(36).to_vec(), colour: 2, align: 2, font: Font::Menu }));
        let up = c.add(Widget::Button(Button::new(BTN_UP, 0xD4, 0x28, 2, true, true)));
        let down = c.add(Widget::Button(Button::new(BTN_DOWN, 0xD4, 0x70, 3, true, true)));
        let t0 = TabBox::new(t, 0x18, 0x20, t.s(34), true);
        let x1 = (t0.rect[2] - t0.rect[0]) + 0x1C;
        let a = c.add(Widget::TabBox(t0));
        let b = c.add(Widget::TabBox(TabBox::new(t, x1, 0x20, t.s(35), false)));
        let cursor = c.add(Widget::Cursor(Cursor { t: 0 }));
        let mut p = Inventory { c, lists: [Vec::new(), Vec::new()], tab: 0, top: 0, cur: 0, up, down, tabs: [a, b], cursor, counts: Vec::new(), gauge: (0, 0), dialog: None, stub: None, fresh: true, hold: Container { drawn: true, ..Container::default() } };
        p.rebuild(w);
        p
    }

    /// `0x080164D0`
    fn rebuild(&mut self, w: &World) {
        self.lists = [list_of(w, false), list_of(w, true)];
    }

    fn list(&self) -> &Vec<i32> {
        &self.lists[self.tab as usize & 1]
    }

    fn set_tab(&mut self, tab: u8) {
        self.tab = tab;
        self.top = 0;
        self.cur = 0;
        for (k, &i) in self.tabs.iter().enumerate() {
            if let Some(Widget::TabBox(tb)) = self.c.kids.get_mut(i) {
                tb.active = k as u8 == tab;
            }
        }
        self.c.drawn = false;
    }

    /// what the draw reads and changes (`0x080167A0`'s side effects): the prompts' state, the scroll top, the
    /// cursor point, the counts and the gauge
    fn sample(&mut self, w: &World) {
        let n = self.list().len() as i32;
        let (cur, mut top) = (self.cur, self.top);
        if n > 0 {
            if top > cur {
                top = cur;
            }
            if top < cur - 4 {
                top = cur - 4;
            }
        }
        self.top = top;
        for (i, en) in [(self.up, n > 0 && cur != 0), (self.down, n > 0 && cur != n - 1)] {
            if let Some(Widget::Button(b)) = self.c.kids.get_mut(i) {
                b.visible = n > 0;
                b.enabled = en;
            }
        }
        // 0x08016928: (0x14, (cur - top) * 18 + 0x36), none with an empty list
        self.c.point = Some((n > 0).then_some((0x14, (cur - top) * 18 + 0x36)));
        self.counts = self.list().iter().map(|&i| w.story.supplies.get(i as usize).copied().unwrap_or(0)).collect();
        let r = leader_rec(w);
        let step = |v: i32, m: i32| if m > 0 { (v * 41 / m).clamp(0, 41) } else { 0 };
        self.gauge = (step(r.hp, r.max_hp), step(r.ep, r.max_ep));
    }

    fn event(&mut self, w: &mut World, code: i32) -> Option<Action> {
        match code {
            0 => {
                w.sfx(super::SFX_MENU, 64);
                if self.list().is_empty() {
                    return None;
                }
                // the cursor hidden, the page redrawn at once (slot +0x24), then the dialog's mode
                if let Some(Widget::Cursor(c)) = self.c.kids.get_mut(self.cursor) {
                    c.t = 8;
                }
                self.sample(w);
                let mut under = self.clone();
                under.c.drawn = true;
                w.menu.pre = Some(Box::new(Screen::Inventory(under)));
                let item = self.list()[self.cur.max(0) as usize];
                self.dialog = Some(UseDialog::new(w, item));
                None
            }
            1 => {
                w.sfx(super::SFX_MENU, 64);
                Some(Action::Close)
            }
            2 => {
                w.sfx(super::SFX_CURSOR, 64);
                self.cur -= 1;
                self.c.drawn = false;
                None
            }
            3 => {
                w.sfx(super::SFX_CURSOR, 64);
                self.cur += 1;
                self.c.drawn = false;
                None
            }
            6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                Some(Action::Replace(super::page_screen(w, 1, code == 7)))
            }
            _ => None,
        }
    }

    fn step_page(&mut self, w: &mut World) -> Action {
        if let Some(r) = self.stub.take() {
            if r == 1 {
                self.after_use(w);
            }
            return Action::None;
        }
        if self.dialog.is_some() {
            return self.step_dialog(w);
        }
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| match p.event(w, code) {
            Some(a) => (true, Some(a)),
            None => (false, None),
        });
        if let Some(a) = act {
            return a;
        }
        if consumed || self.dialog.is_some() {
            return Action::None;
        }
        // done check 0x08016C5C
        let k = w.menu.pressed;
        if k & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::Done;
        }
        if k & crate::input::LEFT != 0 && self.tab != 0 {
            self.set_tab(0);
        }
        if k & crate::input::RIGHT != 0 && self.tab != 1 {
            self.set_tab(1);
        }
        Action::None
    }

    fn step_dialog(&mut self, w: &mut World) -> Action {
        let mut d = self.dialog.take().expect("dialog");
        let mut result = None;
        let (consumed, _) = kit::run_step(&mut d, w, |p| &mut p.c, |p, w, _src, code| {
            match code {
                0 => {
                    w.sfx(super::SFX_ACCEPT, 64);
                    // 0x0801E14C: the supply's use script, run synchronously by the game
                    if let Some(sc) = w.data.menu.as_ref().and_then(|m| m.inv.script.get(p.item as usize).cloned()).flatten() {
                        let run = crate::fieldcontent::Callback::Script(sc);
                        w.events.push(crate::world::Event::Field(crate::field::FieldEvent::Run { run, cause: crate::field::Cause::ItemUse { item: p.item }, slot: 0 }));
                    }
                    result = Some(1);
                }
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    result = Some(2);
                }
                _ => {}
            }
            (result.is_some(), None)
        });
        if let Some(r) = result {
            // the dialog's mode ends: BG0 off (the page's last picture shows), then the page goes on
            if let Some(Screen::Inventory(s)) = w.menu.shown.as_deref_mut() {
                s.dialog = None;
            }
            w.menu.pre = None;
            self.stub = Some(r);
            return Action::None;
        }
        if !consumed && w.menu.pressed & (crate::input::UP | crate::input::DOWN) != 0 {
            w.sfx(super::SFX_CURSOR, 64);
            d.c.sel ^= 1;
            d.c.drawn = false;
        }
        self.dialog = Some(d);
        Action::None
    }

    /// back from the dialog with 1: the lists again, the selection clamped (the counts as the script left them)
    fn after_use(&mut self, w: &World) {
        self.rebuild(w);
        let n = self.list().len() as i32;
        if n <= self.cur {
            self.cur = n - 1;
        }
        if self.cur < 0 {
            self.cur = 0;
        }
    }

    fn draw(&self, w: &World, d: &MenuData) -> MenuView {
        let _ = w;
        let t = &d.text;
        let mut cv = Canvas::new();
        // 0x08006718 without children: the frame and the title, whose pen the list goes on with
        cv.background(0, 0, 240, 160);
        cv.panel(8, 24, 232, 152);
        let mut pen = Pen::new(MenuFont::Bold, 120, 16, 1, 5, Some(1));
        cv.text(t, &mut pen, t.s(33));
        pen.font = MenuFont::Menu;
        let list = self.list();
        if list.is_empty() {
            pen.align = 1;
            pen.x = 0x78;
            pen.y = 0x58;
            cv.text(t, &mut pen, t.s(if self.tab != 0 { 76 } else { 75 }));
        } else {
            vsep(&mut cv, 0xDB, 0x37, 0x70);
            pen.align = 0;
            for k in 0..5 {
                let idx = self.top + k;
                if idx >= list.len() as i32 {
                    break;
                }
                let item = list[idx as usize] as usize;
                let icon = d.inv.icon.get(item).cloned().unwrap_or_default();
                let (iw, ih) = (16, 16);
                cv.image(&icon, 0, iw, ih, 0x25 - (iw as i32 / 2), 18 * k + 0x34 - (ih as i32 / 2));
                let cnt = self.counts.get(idx as usize).copied().unwrap_or(0);
                let name = if cnt > 1 { d.inv.plural.get(item) } else { d.inv.name.get(item) };
                pen.colour = if self.c.sel == 2 && self.cur == idx { 6 } else { 2 };
                pen.x = 0x38;
                pen.y = 18 * k + 0x38;
                cv.text(t, &mut pen, name.map(|v| v.as_slice()).unwrap_or(&[]));
                if cnt > 1 {
                    cv.textf(t, &mut pen, t.s(157), &[Arg::I(cnt as i32)]);
                }
            }
        }
        self.c.draw(t, &mut cv);
        // the leader's gauge (picture 0x086B8294, 53x9) at (0xAC, 0x88): steps past HP / EP in colour 1
        let (gx, gy) = (0xAC, 0x88);
        cv.image("ui/images/gauge_hp_ep.png", 0, 53, 9, gx, gy);
        let (hp, ep) = self.gauge;
        for (row, col0, v) in [(1, 9, hp), (2, 8, hp), (3, 7, hp), (5, 5, ep), (6, 4, ep), (7, 3, ep)] {
            if v < 41 {
                cv.fill(gx + col0 + v, gy + row, gx + col0 + 41, gy + row + 1, 1);
            }
        }
        let mut mv = super::surface_view(cv);
        if let Some(dl) = &self.dialog {
            mv.layers.push(wf_view::MenuLayer { w: 208, h: 104, x: 16, y: 40, priority: 0, ops: dl.draw(d).ops });
        }
        mv
    }
}

/// `0x08005096(ctx)`: a dialog surface's frame: the corners cleared, background rows inside, the panel
pub fn dialog_frame(cv: &mut Canvas, w: i32, h: i32) {
    cv.fill(0, 0, 8, h - 8, 0);
    cv.fill(w - 8, 0, w, h - 8, 0);
    cv.fill(0, h - 8, w, h, 0);
    cv.background(0xC, 4, w - 24, h - 16);
    cv.panel(8, 0, w - 8, h - 8);
}

impl UseDialog {
    /// enter `0x0801E1E8`
    fn new(w: &World, item: i32) -> UseDialog {
        let d = w.data.menu.as_ref().expect("menu content");
        let t = &d.text;
        let mut c = Container::default();
        let has = d.inv.script.get(item as usize).is_some_and(|s| s.is_some());
        let use_s = t.s(77);
        c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x4C, 0, has, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x58, text: use_s.to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let bx = t.font(MenuFont::Menu).measure(use_s) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x4C, 1, true, false)));
        c.add(Widget::Label(Label { x: bx + 0x10, y: 0x58, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        UseDialog { c, item, shown: false }
    }

    /// redraw `0x0801E318`: 208x104
    fn draw(&self, d: &MenuData) -> Canvas {
        let t = &d.text;
        let mut cv = Canvas::new();
        dialog_frame(&mut cv, 208, 104);
        let i = self.item as usize;
        let (iw, ih) = (16, 16);
        cv.image(d.inv.icon.get(i).map(|s| s.as_str()).unwrap_or(""), 0, iw as u16, ih as u16, 0x10, 6);
        let mut pen = Pen::new(MenuFont::Menu, iw + 0x12, ih / 2 + 0xC, 0, 5, Some(1));
        pen.line_h = 0xC;
        pen.clip = [0, 0, 208, 104];
        cv.text(t, &mut pen, d.inv.name.get(i).map(|v| v.as_slice()).unwrap_or(&[]));
        pen.colour = 0x75;
        pen.x = 0x12;
        pen.y = ih + 0x12;
        text_wrap(&mut cv, t, &mut pen, 0xB0, d.inv.desc.get(i).map(|v| v.as_slice()).unwrap_or(&[]));
        self.c.draw(t, &mut cv);
        cv
    }
}

impl Page for Inventory {
    fn container(&mut self) -> &mut Container {
        if self.stub.is_some() {
            return &mut self.hold;
        }
        match self.dialog.as_mut() {
            Some(d) => &mut d.c,
            None => &mut self.c,
        }
    }
    fn container_ref(&self) -> &Container {
        if self.stub.is_some() {
            return &self.hold;
        }
        match self.dialog.as_ref() {
            Some(d) => &d.c,
            None => &self.c,
        }
    }
    fn before_draw(&mut self, w: &World) {
        if self.dialog.is_none() {
            self.sample(w);
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        self.step_page(w)
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        self.draw(w, d)
    }
    fn redraw_cost(&self) -> u8 {
        if self.dialog.is_some() {
            DIALOG_FRAMES
        } else {
            REDRAW_FRAMES
        }
    }
    fn idle_cycles(&self) -> Option<i64> {
        Some(T0)
    }
    fn after_draw(&mut self, _w: &World) {
        self.fresh = false;
        if let Some(dl) = self.dialog.as_mut() {
            dl.shown = true;
        }
    }
    /// the iteration's own cycles (emulator measurements `re/buu/ship/menu_timing.py`, real route 8-51: the list
    /// 502,271 / 508,867 with the cursor, the first redraw after the page switch 650,893, the Use dialog's first
    /// iteration 1,009,750): the page's constant part + its draw list (`sndsim::layer_cycles`)
    fn redraw_time(&self, w: &World, d: &MenuData) -> Option<crate::sndsim::Redraw> {
        let v = self.draw(w, d);
        let layer = |k: usize, line: i64| v.layers.get(k).map(|l| crate::sndsim::layer_cycles(&d.text, &l.ops, line)).unwrap_or(0);
        let rows = (self.list().len() as i64 - self.top as i64).clamp(0, 5);
        let gn0 = v.layers.first().map(|l| crate::sndsim::ops_cost(&d.text, &l.ops).0).unwrap_or(0);
        let mut own = LIST_CYCLES + ROW_CYCLES * rows + layer(0, LINE_CYCLES) + gn0 * LIST_GN_MILLI / 1000;
        if self.fresh {
            own += ENTER_CYCLES;
        }
        if let Some(dl) = self.dialog.as_ref() {
            let gn1 = v.layers.get(1).map(|l| crate::sndsim::ops_cost(&d.text, &l.ops).0).unwrap_or(0);
            let dlg = DIALOG_REDRAW_CYCLES + layer(1, DIALOG_LINE_CYCLES) + gn1 * DIALOG_GN_MILLI / 1000;
            if dl.shown {
                own = dlg;
            } else {
                own += DIALOG_CYCLES + dlg;
            }
        }
        Some(crate::sndsim::Redraw { t0: T0, base: own - T0 })
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`); None = not this screen's kind
pub fn open_json(_w: &World, _v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    None
}

#[allow(dead_code)]
fn _units(s: &str) -> Vec<u16> {
    units(s)
}
