//! The Status page (mode vt `0x0804AC88`, 0x158 bytes; built in the field main loop `0x08006E3C` on START, enter
//! `0x080158F4` → children `0x08014DC2`, redraw `0x080154D4` = frame `0x08006718` + content `0x0801514A` +
//! children, events `0x08015516`, update `0x080150F4`, done check `0x080157A4`).
//!
//! Party tabs (characters 0..7 in the party) switch the shown character; with stat points the A prompt "Use Points"
//! starts the allocation (cursor on STR / POW / END, LEFT / RIGHT move points, A accepts = record written +
//! RecalcStats `0x08013C66`, B cancels); DOWN opens Skills; L / R switch to Journal / Inventory; B or START close.

use super::kit::{
    self, Button, Container, Cursor, Font, Label, Tabs, TextCtl, Widget, BTN_DOWN, BTN_L, BTN_R,
};
use super::text::{Arg, Canvas, Pen};
use super::{Action, MenuData, Page};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuView};

/// video frames one redraw of this page takes (the page draw + portrait decode + 38 400-byte copy): 5 (the real
/// route's stat allocations: the blinking LevelUp! redraw holds the loop 4 frames every 8 updates, the Use Points
/// press lands in the iteration before it; menu_parity stays exact, Earth / ship segments match more frames)
pub const REDRAW_FRAMES: u8 = 5;
/// the redraw's CPU cycles (sndsim.rs; fit on 39 redraws of segment e_2_9_13, work/earth/timing): constant part,
/// per 10,000 glyph-routine instructions, the cursor picture; cycles before the redraw in its frame
const REDRAW_BASE: i64 = 327_464;
const REDRAW_BASE_ALLOC: i64 = 327_223;
const REDRAW_GLYPH: i64 = 35_664;
const REDRAW_CURSOR: i64 = 7_284;
/// a portrait without `decode_cycles` (older content): a typical one
const PORTRAIT_CYCLES: u32 = 100_000;
const T0_IDLE: i64 = 4_641;
const T0_IDLE_ALLOC: i64 = 4_733;
const T0_HELD: i64 = 1_954;
const T0_NAV: i64 = 2_838;
const T0_DIR: i64 = 5_260;
const T0_ENTER: i64 = 52_600;
const T0_ACCEPT: i64 = 28_400;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Status {
    pub c: Container,
    /// +0x130: allocating stat points
    pub alloc: bool,
    /// +0x154: update counter (the "LevelUp!" blink, bit 3)
    pub blink: u32,
    /// +0x134..+0x140: points, STR, POW, END when the allocation started
    pub orig: [i32; 4],
    /// +0x144..+0x150: STR, POW, END, points being allocated
    pub vals: [i32; 4],
    /// child indices: the Skills prompt (+0x11C), the tabs (+0x12C), the cursor (+0x128), the A / B prompts
    pub skills_btn: usize,
    pub tabs: usize,
    pub cursor: Option<usize>,
    pub prompts: [Option<usize>; 4],
    /// play time (frames) when the surface was drawn (TIME)
    pub time: u64,
}

fn char_rec(w: &World, c: i32) -> Option<&crate::progression::Record> {
    w.story.chars.get(c.max(0) as usize).map(|s| &s.rec)
}

impl Status {
    /// `0x08014DC2`
    pub fn new(w: &World) -> Status {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let mut c = Container::default();
        // "Skills" (right-aligned at 224, 0x90) and its DOWN prompt
        let sk = t.s(29);
        c.add(Widget::Label(Label {
            x: 0xE0,
            y: 0x90,
            text: sk.to_vec(),
            colour: 5,
            align: 2,
            font: Font::Menu,
        }));
        let skx = 0xCE - t.font(MenuFont::Menu).measure(sk);
        let skills_btn = c.add(Widget::Button(Button::new(
            BTN_DOWN, skx, 0x84, 3, true, false,
        )));
        c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
        c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
        c.add(Widget::Label(Label {
            x: 0x22,
            y: 0x14,
            text: t.s(30).to_vec(),
            colour: 2,
            align: 0,
            font: Font::Menu,
        }));
        c.add(Widget::Label(Label {
            x: 0xCE,
            y: 0x14,
            text: t.s(33).to_vec(),
            colour: 2,
            align: 2,
            font: Font::Menu,
        }));
        // STR / POW / END cycle (ids 1..3); SPD is not selectable
        for (id, nav, s, y) in [
            (1, [3, 2, -1, -1], 47, 0x4F),
            (2, [1, 3, -1, -1], 48, 0x5B),
            (3, [2, 1, -1, -1], 49, 0x67),
            (-1, [-1, -1, -1, -1], 50, 0x73),
        ] {
            c.add(Widget::Text(TextCtl {
                id,
                nav,
                text: t.s(s).to_vec(),
                x: 0xB2,
                y,
                code: -1,
                align: 0,
            }));
        }
        // party tabs in (16, 30)-(224, 46), event 100; arrows are children after it
        let tabs = c.kids.len();
        c.add(Widget::Tabs(Tabs::new(
            0,
            [0x10, 0x1E, 0xE0, 0x2E],
            100,
            tabs + 1,
            tabs + 2,
        )));
        c.add(Widget::Button(Button::new(
            kit::BTN_LEFT,
            0x10,
            0x1E,
            100,
            true,
            false,
        )));
        c.add(Widget::Button(Button::new(
            kit::BTN_RIGHT,
            0xE0 - 0x10,
            0x1E,
            100,
            true,
            false,
        )));
        if let Some(Widget::Tabs(tb)) = c.kids.get_mut(tabs) {
            for i in 0..8 {
                if w.story.chars.get(i).is_some_and(|s| s.flags & 1 != 0) {
                    tb.push(t, t.s(158 + i), i as i32);
                }
            }
            // mod party members (characters/*.json index 9+) after the game's eight, named from their content
            for i in 9..w.story.chars.len() {
                if w.story.chars[i].flags & 1 != 0 {
                    let name = w
                        .data
                        .chars
                        .iter()
                        .find(|c| c.index == i as i32)
                        .map(|c| super::text::units(&c.name))
                        .unwrap_or_default();
                    tb.push(t, &name, i as i32);
                }
            }
        }
        let mut s = Status {
            c,
            alloc: false,
            blink: 0,
            orig: [0; 4],
            vals: [0; 4],
            skills_btn,
            tabs,
            cursor: None,
            prompts: [None; 4],
            time: w.story.play_frames,
        };
        s.prepare_tabs();
        let cur = w.story.current;
        let arrows = s.arrows_visible();
        if let Some(Widget::Tabs(tb)) = s.c.kids.get_mut(s.tabs) {
            tb.select_value(cur, arrows);
        }
        s.update_prompts(w);
        s
    }

    fn arrows_visible(&self) -> bool {
        matches!(self.c.kids.get(self.tabs + 1), Some(Widget::Button(b)) if b.visible)
    }

    /// the shown character (the tabs' current value)
    pub fn char_index(&self) -> i32 {
        match self.c.kids.get(self.tabs) {
            Some(Widget::Tabs(t)) => t.value(),
            _ => 0,
        }
    }

    /// draw-time side effects of the tab bar (`0x080160CA`): layout, the arrows' visibility / state
    pub fn prepare_tabs(&mut self) {
        let sel = self.c.sel;
        let Some(Widget::Tabs(tb)) = self.c.kids.get_mut(self.tabs) else {
            return;
        };
        let mut arrows = None;
        if !tb.laid_out {
            arrows = Some(tb.layout());
        }
        let (cur, n, id) = (tb.cur, tb.tabs.len() as i32, tb.id);
        let (l, r) = (tb.left, tb.right);
        if let Some(a) = arrows {
            for i in [l, r] {
                if let Some(Widget::Button(b)) = self.c.kids.get_mut(i) {
                    b.visible = a;
                    b.enabled = a;
                }
            }
        }
        let vis = matches!(self.c.kids.get(l), Some(Widget::Button(b)) if b.visible);
        if vis {
            let selected = sel == id;
            if let Some(Widget::Button(b)) = self.c.kids.get_mut(l) {
                b.enabled = selected && cur != 0;
            }
            if let Some(Widget::Button(b)) = self.c.kids.get_mut(r) {
                b.enabled = selected && cur != n - 1;
            }
        }
    }

    /// `0x08014D56`: RecalcStats of the shown character, then the A / B prompts: Accept / Cancel while allocating,
    /// Use Points / Cancel with points, else Cancel
    fn update_prompts(&mut self, w: &World) {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        for i in self.prompts.iter().flatten() {
            self.c.remove(*i);
        }
        let pts = char_rec(w, self.char_index())
            .map(|r| r.points)
            .unwrap_or(0);
        let (a, b) = if self.alloc {
            (24, 26)
        } else if pts != 0 {
            (54, 26)
        } else {
            (0, 26)
        };
        self.prompts = kit::ab_prompts(&mut self.c, t, a, b);
    }

    /// update (vt +0xC, before the redraw check): the blink counter; every 8 updates with points a redraw
    fn update_blink(&mut self, w: &World) {
        if !self.alloc {
            self.blink = self.blink.wrapping_add(1);
            if self.blink & 7 == 0 && char_rec(w, self.char_index()).is_some_and(|r| r.points != 0)
            {
                self.c.drawn = false;
            }
        }
    }

    fn step_page(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(
            self,
            w,
            |p| &mut p.c,
            |p, w, src, code| match p.event(w, src, code) {
                Some(a) => (true, Some(a)),
                None => (false, None),
            },
        );
        if let Some(a) = act {
            return a;
        }
        if consumed {
            return Action::None;
        }
        // done check 0x080157A4
        let p = w.menu.pressed;
        if p & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::Done;
        }
        if self.alloc {
            let sel = self.c.sel.clamp(1, 3) as usize;
            if p & crate::input::LEFT != 0 {
                w.sfx(super::SFX_MENU, 64);
                if self.vals[sel - 1] > self.orig[sel] {
                    self.vals[sel - 1] -= 1;
                    self.vals[3] += 1;
                    self.c.drawn = false;
                }
            }
            if p & crate::input::RIGHT != 0 {
                w.sfx(super::SFX_MENU, 64);
                let lv = char_rec(w, self.char_index())
                    .map(|r| level(w, r))
                    .unwrap_or(0);
                if lv > self.vals[sel - 1] && self.vals[3] > 0 {
                    self.vals[sel - 1] += 1;
                    self.vals[3] -= 1;
                    self.c.drawn = false;
                }
            }
        }
        Action::None
    }

    /// handle_event `0x08015516`
    fn event(&mut self, w: &mut World, src: usize, code: i32) -> Option<Action> {
        match code {
            0 => {
                let ci = self.char_index();
                if !self.alloc {
                    self.c.sel = 1;
                    self.cursor = Some(self.c.add(Widget::Cursor(Cursor { t: 0 })));
                    self.alloc = true;
                    if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.skills_btn) {
                        b.enabled = false;
                    }
                    let r = char_rec(w, ci).cloned().unwrap_or_default();
                    self.orig = [r.points, r.str_, r.pow, r.end];
                    self.vals = [r.str_, r.pow, r.end, r.points];
                    w.sfx(super::SFX_MENU, 64);
                } else {
                    if let Some(i) = self.cursor.take() {
                        self.c.remove(i);
                    }
                    self.c.sel = 0;
                    self.alloc = false;
                    if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.skills_btn) {
                        b.enabled = true;
                    }
                    let v = self.vals;
                    if let Some(cs) = w.story.chars.get_mut(ci.max(0) as usize) {
                        cs.rec.str_ = v[0] & 0xFF;
                        cs.rec.pow = v[1] & 0xFF;
                        cs.rec.end = v[2] & 0xFF;
                        cs.rec.points = v[3] & 0xFFFF;
                    }
                    apply_record(w, ci);
                    w.sfx(super::SFX_ACCEPT, 64);
                }
                self.update_prompts(w);
                self.c.drawn = false;
                None
            }
            1 => {
                w.sfx(super::SFX_MENU, 64);
                if !self.alloc {
                    return Some(Action::Close);
                }
                self.alloc = false;
                if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.skills_btn) {
                    b.enabled = true;
                }
                if let Some(i) = self.cursor.take() {
                    self.c.remove(i);
                }
                self.c.sel = 0;
                self.update_prompts(w);
                self.c.drawn = false;
                None
            }
            3 => {
                // Skills: a nested mode; its result picks Journal (2) / Inventory (3) / close (4)
                w.sfx(super::SFX_MENU, 64);
                Some(Action::Push(super::skills_screen(w, self.char_index())))
            }
            6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                Some(Action::Replace(super::page_screen(w, 0, code == 7)))
            }
            100 => {
                w.sfx(super::SFX_CURSOR, 64);
                let (l, r) = match self.c.kids.get(self.tabs) {
                    Some(Widget::Tabs(t)) => (t.left, t.right),
                    _ => (0, 0),
                };
                if let Some(Widget::Tabs(t)) = self.c.kids.get_mut(self.tabs) {
                    if src == l {
                        t.step_left();
                    } else if src == r {
                        t.step_right();
                    }
                    t.mark = t.cur;
                }
                self.update_prompts(w);
                None
            }
            _ => None,
        }
    }

    /// the Skills sub-page closed with `r`
    fn skills_closed(&mut self, w: &mut World, r: i32) -> Action {
        match r {
            2 => Action::Replace(super::page_screen(w, 0, false)),
            3 => Action::Replace(super::page_screen(w, 0, true)),
            4 => {
                w.sfx(super::SFX_MENU, 64);
                Action::Close
            }
            _ => {
                self.c.drawn = false;
                Action::None
            }
        }
    }

    /// redraw `0x080154D4`
    fn draw(&self, w: &World, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        kit::page_frame(t, &mut cv, 28);
        self.content(w, d, &mut cv);
        self.c.draw(t, &mut cv);
        super::surface_view(cv)
    }

    /// `0x0801514A`
    fn content(&self, w: &World, d: &MenuData, cv: &mut Canvas) {
        let t = &d.text;
        let ci = self.char_index();
        let cs = w
            .story
            .chars
            .get(ci.max(0) as usize)
            .cloned()
            .unwrap_or_default();
        let r = &cs.rec;
        let lv = level(w, r);
        let form = d
            .forms
            .get(cs.form.max(0) as usize)
            .cloned()
            .unwrap_or_default();
        let mut p = Pen::new(MenuFont::Bold, 0x10, 0x37, 0, 5, Some(1));
        cv.textf(t, &mut p, t.s(40), &[Arg::S(&form.upper), Arg::I(lv)]);
        p.font = MenuFont::Menu;
        let rows: [(i32, usize, Vec<Arg>); 6] = [
            (0x43, 41, vec![Arg::I(r.hp), Arg::I(r.max_hp)]),
            (0x4F, 42, vec![Arg::I(r.ep), Arg::I(r.max_ep)]),
            (0x5B, 43, vec![Arg::I(r.xp as i32)]),
            (0x67, 44, vec![Arg::I(next_level_xp(w, r))]),
            (0x73, 45, vec![Arg::I(w.story.zenie as i32)]),
            (0x7F, 46, {
                // frames / 3600 = minutes (0x080530C8), then hours / minutes (0x080530F4)
                let m = (self.time / 3600) as i32;
                vec![Arg::I(m / 60), Arg::I(m % 60)]
            }),
        ];
        for (y, s, args) in rows.iter() {
            p.x = 0x52;
            p.y = *y;
            cv.textf(t, &mut p, t.s(*s), args);
        }
        separator(cv, 0xA8, 0x46, 0x7A);
        let eff = effective(w, &cs);
        if !self.alloc {
            if r.points != 0 {
                if self.blink & 8 != 0 {
                    p.x = 0xB2;
                    p.y = 0x35;
                    cv.text(t, &mut p, t.s(51));
                }
                p.x = 0xB2;
                p.y = 0x43;
                cv.text(t, &mut p, t.s(52));
                p.align = 2;
                p.colour = 0x75;
                p.x = 0xE0;
                p.y = 0x43;
                cv.textf(t, &mut p, &pct_d(), &[Arg::I(r.points)]);
            }
            p.align = 2;
            for (y, e, b) in [
                (0x4F, eff[0], r.str_),
                (0x5B, eff[1], r.pow),
                (0x67, eff[2], r.end),
            ] {
                p.y = y;
                p.x = 0xE0;
                p.colour = stat_colour(e, b);
                cv.textf(t, &mut p, &pct_d(), &[Arg::I(e)]);
            }
        } else {
            p.x = 0xB2;
            p.y = 0x43;
            cv.text(t, &mut p, t.s(52));
            p.align = 2;
            p.colour = 6;
            p.y = 0x43;
            p.x = 0xE0;
            cv.textf(t, &mut p, &pct_d(), &[Arg::I(self.vals[3])]);
            for (k, y) in [(0usize, 0x4F), (1, 0x5B), (2, 0x67)] {
                p.y = y;
                p.x = 0xE0;
                p.colour = if self.vals[k] == self.orig[k + 1] {
                    2
                } else {
                    6
                };
                cv.textf(t, &mut p, &pct_d(), &[Arg::I(self.vals[k])]);
            }
        }
        p.x = 0xE0;
        p.y = 0x73;
        p.colour = stat_colour(eff[3], r.spd);
        cv.textf(t, &mut p, &pct_d(), &[Arg::I(eff[3])]);
        // portrait (form portrait byte 0, table 0x086F393C; content index = byte - 8) at (16, 64)
        let pb = form.portraits[0] as i32;
        if pb >= 8 {
            if let Some(path) = d.portraits.get((pb - 8) as usize).filter(|s| !s.is_empty()) {
                cv.image(path, 0, 64, 64, 0x10, 0x40);
            }
        }
    }
}

fn pct_d() -> Vec<u16> {
    vec![0x25, 0x64]
}

/// value colour: effective == base 0x75, above 6, below 3
fn stat_colour(eff: i32, base: i32) -> u8 {
    if eff == base {
        0x75
    } else if eff > base {
        6
    } else {
        3
    }
}

/// `0x080051B0`: the green separator: (x, y0) 0x2F, rows of 0x2F 0x30 0x31 from x - 1, then 0x31 at x
fn separator(cv: &mut Canvas, x: i32, y0: i32, y1: i32) {
    cv.pixel(x, y0, 0x2F);
    let n = y1 - y0 - 2;
    let mut y = y0 + 1;
    for _ in 0..n.max(0) {
        cv.pixel(x - 1, y, 0x2F);
        cv.pixel(x, y, 0x30);
        cv.pixel(x + 1, y, 0x31);
        y += 1;
    }
    cv.pixel(x, y, 0x31);
}

/// `0x080135D2`: level from XP (table 0x086B7084)
pub fn level(w: &World, r: &crate::progression::Record) -> i32 {
    crate::progression::level_of(r.xp, &w.data.xp_table)
}

/// `0x0801361C`: XP to the next level (0 at 200)
pub fn next_level_xp(w: &World, r: &crate::progression::Record) -> i32 {
    let lv = level(w, r);
    if lv >= 200 {
        return 0;
    }
    w.data
        .xp_table
        .get((lv - 49) as usize)
        .map(|&t| (t - r.xp) as i32)
        .unwrap_or(0)
}

/// record +0x4C..+0x54: STR, POW, END, SPD after RecalcStats
pub fn effective(w: &World, cs: &crate::story::CharState) -> [i32; 4] {
    let eq: Vec<Option<&crate::content::Equip>> = cs
        .equip
        .iter()
        .map(|&e| w.data.equipment.iter().find(|q| q.id == e))
        .collect();
    // (the timed buff - Holy Water, boost_stats - is the game data's (GD+0x4DC frames left, +0x4E0.. the bonuses),
    // applied by RecalcStats to the record it recomputes: menu_parity inventory_use_holy, STR 27 + 25 on the Status
    // page; ours keeps it on the live fighter)
    let boost = w.players.iter().filter_map(|&id| w.find(id)).find_map(|i| match &w.ents[i].body {
        crate::world::Body::Player(p) if p.holy_left > 0 => Some(p.boost),
        _ => None,
    });
    let e = crate::progression::recalc_stats_with(&cs.rec, &eq, boost);
    [e.str_, e.pow, e.end, e.spd]
}

/// the record changed in the menu: the live fighter playing this character takes it, then RecalcStats on the
/// object (the field writes fighters back into the story every frame)
pub fn apply_record(w: &mut World, ci: i32) {
    let Some(cs) = w.story.chars.get(ci.max(0) as usize).cloned() else {
        return;
    };
    for s in 0..w.players.len() {
        let Some(i) = w.player_idx(s) else { continue };
        let crate::world::Body::Player(p) = &w.ents[i].body else {
            continue;
        };
        if crate::story::char_of_form(&w.data, p.form) != Some(ci) {
            continue;
        }
        let mut p = p.clone();
        p.rec = cs.rec.clone();
        p.equip = cs.equip;
        p.refresh_stats(w);
        if let crate::world::Body::Player(q) = &mut w.ents[i].body {
            *q = p;
        }
    }
}

impl Page for Status {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn update(&mut self, w: &World) {
        self.update_blink(w)
    }
    fn before_draw(&mut self, w: &World) {
        self.prepare_tabs();
        self.time = w.story.play_frames;
    }
    fn step(&mut self, w: &mut World) -> Action {
        self.step_page(w)
    }
    fn child_result(&mut self, w: &mut World, r: i32) -> Action {
        self.skills_closed(w, r)
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        self.draw(w, d)
    }
    fn redraw_cost(&self) -> u8 {
        REDRAW_FRAMES
    }
    fn redraw_time(&self, w: &World, d: &MenuData) -> Option<crate::sndsim::Redraw> {
        let v = self.draw(w, d);
        let (gn, small, extra) = v
            .layers
            .first()
            .map(|l| crate::sndsim::ops_cost(&d.text, &l.ops))
            .unwrap_or((0, 0, 0));
        // the step before the redraw (cycles from the VBlank to the redraw; work/earth/timing/pre.py)
        let m = &w.menu;
        let sfx = |id: i32| {
            w.events
                .iter()
                .any(|e| matches!(e, crate::world::Event::Sfx { id: i, .. } if *i == id))
        };
        let dir = crate::input::LEFT | crate::input::RIGHT;
        let nav = crate::input::UP | crate::input::DOWN;
        let t0 = if m.released & crate::input::A != 0 && self.alloc {
            // the A prompt's event: the cursor is built (its picture loaded)
            T0_ENTER
        } else if sfx(super::SFX_ACCEPT) {
            // the points are applied (RecalcStats)
            T0_ACCEPT
        } else if self.c.focus.is_some() && m.pressed == 0 {
            // a held prompt takes the input alone
            T0_HELD
        } else if self.alloc && m.pressed & nav != 0 {
            T0_NAV
        } else if self.alloc && m.pressed & dir != 0 {
            T0_DIR
        } else if m.pressed & crate::input::A != 0 {
            if self.alloc {
                T0_IDLE_ALLOC + 298
            } else {
                T0_IDLE + 103
            }
        } else if self.alloc {
            T0_IDLE_ALLOC
        } else {
            T0_IDLE
        };
        let base = if self.alloc {
            REDRAW_BASE_ALLOC
        } else {
            REDRAW_BASE
        };
        // the portrait's decompression (every redraw; data dependent)
        let ci = self.char_index();
        let pb = w
            .story
            .chars
            .get(ci.max(0) as usize)
            .and_then(|cs| d.forms.get(cs.form.max(0) as usize))
            .map(|f| f.portraits[0] as i32)
            .unwrap_or(0);
        let dec = if pb >= 8 {
            d.portrait_cycles
                .get((pb - 8) as usize)
                .copied()
                .filter(|&c| c != 0)
                .unwrap_or(PORTRAIT_CYCLES)
        } else {
            0
        } as i64;
        if crate::sndsim::debug() {
            eprintln!(
                "status redraw: gn {gn} small {small} dec {dec} extra {extra} pressed {:#x} released {:#x} alloc {} focus {:?} events {} img {:?}",
                m.pressed,
                m.released,
                self.alloc,
                self.c.focus,
                w.events.len(),
                v.layers.first().map(|l| crate::sndsim::ops_summary(&l.ops)).unwrap_or_default()
            );
        }
        Some(crate::sndsim::Redraw {
            t0,
            base: base + dec + extra + gn * REDRAW_GLYPH / 10_000 + small * REDRAW_CURSOR,
        })
    }
}
