//! The Trading Post's pages (after Select Saved Game with +0xB38 = 0): the game's own modes, drawn and stepped as
//! they are (static disassembly; gated against two linked emulators, re/buu/menu/scenarios_trade.py).
//!
//! * **The picker** (mode vt `0x0804A60C`, ctor `0x0801F506` = 0xBF8 bytes, enter `0x0801F61E`, list build
//!   `0x0801F44E`, redraw `0x0801F99E` = frame `0x08006718` (title 79 "Trading Post", children) + content
//!   `0x0801F882`, events `0x0801FD60` -> list `0x0801FC7A` / "Your Trade:" `0x0801FA3C`, done check `0x0801FE92`,
//!   cursor point `0x0801F7D8`, step `0x0801FF02`). The list (+0x12C: count, 8-byte entries {u32 amount, u8 kind
//!   (0 equipment, 1 supply, 2 zenie), u8 id}): every equipment entry (one each), the supplies whose record bit 0 is
//!   clear with their count, Zenie when not 0. Six rows of 14 px from (22, 49) (the cursor at (18, 46 + 14 row)),
//!   "Your Trade:" at (140, 49) with the offer (+0xBA0, at most 5 entries) under it from y 63 (cursor (136, 60 + 14
//!   k)); the separator `0x080051B0(117, 55, 112)`; equipment names cut to 84 px with "..", a supply with several and
//!   Zenie as "%s (%d)". Side +0x115: 0 the list, 1 "Your Trade:", 2 nothing to trade (no cursor, A greyed).
//!   Children: A (12, 132) "Select", B "Cancel", R "Propose Trade" (code 7), the list's UP / DOWN prompts at (110, 40)
//!   / (110, 112) - their enabled state is set by the content draw, after the children were drawn (one redraw late).
//!   The list: UP / DOWN move the row (no sound; the 6 rows scroll), A takes the entry: one at once, several through
//!   "How many to trade?"; a taken entry moves onto the offer (equipment appended, the rest merged), the cursor stays
//!   (the row above when it was the last; the offer side when the list is empty). RIGHT (done check, sound 39) goes
//!   over to the offer when it has something; there UP / DOWN (sound 39) move and A (sound 40) gives an entry back
//!   (through the count box when several); LEFT back to the list. B: back to Select Saved Game. R: Propose Trade.
//! * **"How many to trade?"** (vt `0x0804BC04`, enter `0x0801ED34`, redraw `0x0801EECA`, step `0x0801EF8E`, events
//!   `0x0801EF5C`): a 136x80 BG0 box at (56, 48) (BG0CNT 0x1F83): the entry, the string, LEFT / RIGHT prompts, the
//!   count " %d" centred at (68, 47) from 1; held LEFT / RIGHT change it at once, then every 10, 9, .. updates (the
//!   delay shrinks by one per change), by 10 after 50 held changes at a multiple of 100, by 100 likewise at 1000; 0 ..
//!   min(amount, 50000). A: the count (none when 0), B: none.
//! * **Propose Trade**: "Waiting for a friend..." (vt `0x0804CAD4`: the link box `0x08024120` over the offers'
//!   exchange) until both proposed (then the proposal at once) or B (back to the picker). **The proposal** (vt
//!   `0x0804B604`, enter `0x0801EA0C`, redraw `0x0801EBFE`, content `0x0801EB40`, events `0x0801EC38`): "Your Trade:"
//!   / "Friend's Trade:" (22 / 140, 40; entries from y 54), ACCEPT / DECLINE (selectable boxes `0x08015C62` at (64,
//!   120) / (130, 120), the cursor, LEFT / RIGHT sound 39). A (sound 40) answers: "Waiting for a friend..." (vt
//!   `0x0804C0F0`, the answers' exchange) until the friend answered - both ACCEPT: TRADE COMPLETE!, else back to the
//!   picker (the offer kept) - or B (back to the proposal). B (sound 40) leaves the proposal for the picker.
//! * **TRADE COMPLETE!** (vt `0x08049B5C`, enter `0x08020406`, done check `0x08020504`): a 128x56 BG0 box at (56,
//!   64) over the last page; the trade applied (`0x0801F556`) and the save written in its first update; 61 updates,
//!   then the Multiplayer page (vt `0x0804B5AC`).
//!
//! The boxes are nested modes: while one runs the page under it is not redrawn (its last picture stays); a box that
//! ends with `0x0803D876` leaves an exit-stub iteration (the page's last picture, the box gone) in which the page's
//! handler goes on (the result applied, the page redrawn); one whose step returns (the link's exchange) goes on at
//! once.

use super::super::kit::{self, Button, Container, Cursor, Font, Label, TabBox, Widget, BTN_A, BTN_B, BTN_DOWN, BTN_LEFT, BTN_R, BTN_RIGHT, BTN_UP};
use super::super::shop::{text_trunc, vsep};
use super::super::text::{Arg, Canvas, Pen, TextData};
use super::super::{MenuData, SFX_CURSOR, SFX_MENU};
use super::{Link, LinkAction};
use crate::input::{LEFT, RIGHT};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuLayer};

pub const KIND_EQUIP: u8 = 0;
pub const KIND_SUPPLY: u8 = 1;
pub const KIND_ZENIE: u8 = 2;

/// one entry of the trade's lists (8 bytes in the game: amount, kind, id)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TradeEntry {
    pub amount: u32,
    /// [`KIND_EQUIP`] (an equipment id, one copy), [`KIND_SUPPLY`] (a supply index), [`KIND_ZENIE`]
    pub kind: u8,
    pub id: u8,
}

/// the list's limit (`0x0801F3DC`: 0x14E entries)
const LIST_MAX: usize = 0x14E;
/// the offer's limit (`0x0801FBD0`)
pub const OFFER_MAX: usize = 5;
/// the count box's top (`0x0801EF8E`)
const COUNT_MAX: u32 = 50000;
/// the TRADE COMPLETE! box's updates (`0x08020504`: ends when its counter passes 60)
const COMPLETE_UPDATES: u32 = 60;

/// `0x0801F3DC` (the list) / `0x0801FBD0` (the offer, at most 5): add `n` of `e` - equipment appended, the rest
/// merged into the entry of the same kind and id or appended; false when full (or n = 0)
fn add(list: &mut Vec<TradeEntry>, e: TradeEntry, n: u32, max: usize) -> bool {
    if n == 0 {
        return false;
    }
    if e.kind != KIND_EQUIP {
        if let Some(x) = list.iter_mut().find(|x| x.kind == e.kind && x.id == e.id) {
            x.amount += n;
            return true;
        }
    }
    if list.len() >= max {
        return false;
    }
    list.push(TradeEntry { amount: n, ..e });
    true
}

/// `0x0801FA00` / `0x0801FC3E`: move `n` of entry `i` of `from` to `to` (when it fits); the entry goes at 0
fn moved(from: &mut Vec<TradeEntry>, i: usize, to: &mut Vec<TradeEntry>, n: u32, max: usize) {
    let Some(e) = from.get(i).copied() else { return };
    if add(to, e, n, max) {
        from[i].amount = from[i].amount.saturating_sub(n);
        if from[i].amount == 0 {
            from.remove(i);
        }
    }
}

/// the picker's list from the running game's data (`0x0801F44E`): equipment entries, tradable supplies, zenie
pub fn trade_list(w: &World) -> Vec<TradeEntry> {
    let mut l = Vec::new();
    for &e in &w.story.equipment {
        add(&mut l, TradeEntry { amount: 1, kind: KIND_EQUIP, id: (e & 0xFF) as u8 }, 1, LIST_MAX);
    }
    let story = w.data.menu.as_ref().map(|d| d.inv.story.clone()).unwrap_or_default();
    for (i, &n) in w.story.supplies.iter().enumerate().take(crate::story::SUPPLIES) {
        if !story.get(i).copied().unwrap_or(false) {
            add(&mut l, TradeEntry { amount: 0, kind: KIND_SUPPLY, id: i as u8 }, n as u32, LIST_MAX);
        }
    }
    add(&mut l, TradeEntry { amount: 0, kind: KIND_ZENIE, id: 0 }, w.story.zenie, LIST_MAX);
    l
}

/// `0x0801F810(entry, pen)`: an entry's line
fn entry(cv: &mut Canvas, d: &MenuData, p: &mut Pen, e: &TradeEntry) {
    let t = &d.text;
    let fmt: Vec<u16> = "%s (%d)".encode_utf16().collect();
    match e.kind {
        KIND_EQUIP => {
            let name = d.items.get(1, e.id as i32).map(|i| i.name.clone()).unwrap_or_default();
            text_trunc(cv, t, p, 0x54, &name);
        }
        KIND_SUPPLY => {
            let name = d.items.get(0, e.id as i32).map(|i| i.name.clone()).unwrap_or_default();
            if e.amount == 1 {
                cv.text(t, p, &name);
            } else {
                cv.textf(t, p, &fmt, &[Arg::S(&name), Arg::I(e.amount as i32)]);
            }
        }
        _ => cv.textf(t, p, &fmt, &[Arg::S(t.s(82)), Arg::I(e.amount as i32)]),
    }
}

/// the content pen (`0x0801F882` / `0x0801EB40`): menu font, colour 2, shadow 1, lines of 14
fn pen() -> Pen {
    Pen { line_h: 14, ..Pen::new(MenuFont::Menu, 0, 0, 0, 2, Some(1)) }
}

/// the A "Select" / B "Cancel" prompts at the bottom (the trade pages' enters)
fn ab(c: &mut Container, t: &TextData) -> i32 {
    let lab = |x: i32, s: usize| Widget::Label(Label { x, y: 0x90, text: t.s(s).to_vec(), colour: 5, align: 0, font: Font::Menu });
    c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
    c.add(lab(0x1C, 23));
    let bx = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
    c.add(Widget::Button(Button::new(BTN_B, bx, 0x84, 1, true, false)));
    c.add(lab(bx + 0x10, 26));
    bx
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Picker {
    pub c: Container,
    /// +0x12C the list, +0xBA0 our offer ("Your Trade:"), +0xBCC the friend's (as proposed)
    pub list: Vec<TradeEntry>,
    pub mine: Vec<TradeEntry>,
    pub friend: Vec<TradeEntry>,
    /// +0x115: 0 the list, 1 the offer, 2 nothing to trade
    pub side: u8,
    /// +0x120 the list's first shown entry, +0x124 the cursor's row, +0x128 the offer's entry
    pub first: i32,
    pub row: i32,
    pub idx: i32,
    pub up: usize,
    pub down: usize,
}

impl Picker {
    /// ctor `0x0801F506` + enter `0x0801F61E`
    pub fn new(w: &World) -> Picker {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let list = trade_list(w);
        let side = if list.is_empty() { 2 } else { 0 };
        let mut c = Container::default();
        let bx = ab(&mut c, t);
        let rx = t.font(MenuFont::Menu).measure(t.s(26)) + bx + 0x14;
        c.add(Widget::Button(Button::new(BTN_R, rx, 0x84, 7, true, false)));
        c.add(Widget::Label(Label { x: rx + 0x1A, y: 0x90, text: t.s(84).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let up = c.add(Widget::Button(Button::new(BTN_UP, 0x6E, 0x28, 2, true, false)));
        let down = c.add(Widget::Button(Button::new(BTN_DOWN, 0x6E, 0x70, 3, true, false)));
        if side != 2 {
            c.add(Widget::Cursor(Cursor { t: 0 }));
        } else if let Some(Widget::Button(a)) = c.kids.get_mut(0) {
            a.enabled = false;
        }
        Picker { c, list, mine: Vec::new(), friend: Vec::new(), side, first: 0, row: 0, idx: 0, up, down }
    }

    /// cursor point `0x0801F7D8`
    pub fn prepare(&mut self) {
        self.c.point = Some(match self.side {
            0 => Some((0x12, self.row * 14 + 0x2E)),
            1 => Some((0x88, self.idx * 14 + 0x3C)),
            _ => None,
        });
    }

    /// `0x0801F882`'s side effect after the children were drawn: the arrows' enabled state
    pub fn arrows(&mut self) {
        let (u, dn) = match self.side {
            0 => (self.first + self.row != 0, self.list.len() as i32 - 1 > self.first + self.row),
            1 => (self.idx != 0, self.mine.len() as i32 - 1 > self.idx),
            _ => (false, false),
        };
        for (i, en) in [(self.up, u), (self.down, dn)] {
            if let Some(Widget::Button(b)) = self.c.kids.get_mut(i) {
                b.visible = true;
                b.enabled = en;
            }
        }
    }

    /// redraw `0x0801F99E`
    pub fn draw(&self, d: &MenuData, cv: &mut Canvas) {
        let t = &d.text;
        kit::page_frame(t, cv, 79);
        self.c.draw(t, cv);
        vsep(cv, 0x75, 0x37, 0x70);
        let mut p = pen();
        for k in 0..6 {
            let Some(e) = self.list.get((self.first + k) as usize) else { break };
            p.x = 0x16;
            p.y = 14 * k + 0x31;
            entry(cv, d, &mut p, e);
        }
        p.x = 0x8C;
        p.y = 0x31;
        cv.text(t, &mut p, t.s(80));
        for (k, e) in self.mine.iter().enumerate().take(6) {
            p.x = 0x8C;
            p.y = 14 * k as i32 + 0x3F;
            entry(cv, d, &mut p, e);
        }
    }

    /// the list side's events (`0x0801FC7A`): (the count box to open for entry `i`)
    fn list_event(&mut self, code: i32) -> Option<usize> {
        match code {
            0 => {
                let i = (self.first + self.row).max(0) as usize;
                let e = *self.list.get(i)?;
                if e.amount != 1 {
                    return Some(i);
                }
                self.take(1);
            }
            2 => {
                if self.row != 0 {
                    self.row -= 1;
                } else {
                    self.first -= 1;
                }
                self.c.drawn = false;
            }
            3 => {
                if self.row < 5 {
                    self.row += 1;
                } else {
                    self.first += 1;
                }
                self.c.drawn = false;
            }
            _ => {}
        }
        None
    }

    /// `n` of the list's current entry onto the offer, the cursor kept in the list (`0x0801FC7A`'s tail)
    pub fn take(&mut self, n: u32) {
        let i = (self.first + self.row).max(0) as usize;
        moved(&mut self.list, i, &mut self.mine, n, OFFER_MAX);
        if self.list.len() as i32 <= self.first + self.row {
            if self.row != 0 {
                self.row -= 1;
            } else if self.first != 0 {
                self.first -= 1;
            } else {
                self.side = 1;
            }
        }
        self.c.drawn = false;
    }

    /// `n` of the offer's current entry back onto the list (`0x0801FA3C`'s A, sound 40)
    pub fn give_back(&mut self, w: &mut World, n: u32) {
        w.sfx(SFX_MENU, 64);
        let i = self.idx.max(0) as usize;
        moved(&mut self.mine, i, &mut self.list, n, LIST_MAX);
        if self.mine.len() as i32 <= self.idx {
            if self.idx != 0 {
                self.idx -= 1;
            } else {
                self.side = 0;
            }
        }
        self.c.drawn = false;
    }

    /// the offer side's events (`0x0801FA3C`): (the count box to open for entry `i`)
    fn offer_event(&mut self, w: &mut World, code: i32) -> Option<usize> {
        match code {
            0 => {
                let i = self.idx.max(0) as usize;
                let e = *self.mine.get(i)?;
                if e.amount != 1 {
                    return Some(i);
                }
                self.give_back(w, 1);
            }
            2 => {
                if self.idx != 0 {
                    w.sfx(SFX_CURSOR, 64);
                    self.idx -= 1;
                }
                self.c.drawn = false;
            }
            3 => {
                if self.mine.len() as i32 - 1 > self.idx {
                    w.sfx(SFX_CURSOR, 64);
                    self.idx += 1;
                }
                self.c.drawn = false;
            }
            _ => {}
        }
        None
    }

    /// done check `0x0801FE92`: RIGHT over to the offer, LEFT back to the list (sound 39)
    fn done_check(&mut self, w: &mut World) {
        let p = w.menu.pressed;
        if self.side == 0 && !self.mine.is_empty() && p & RIGHT != 0 {
            w.sfx(SFX_CURSOR, 64);
            self.side = 1;
            self.c.drawn = false;
        } else if self.side == 1 && !self.list.is_empty() && p & LEFT != 0 {
            w.sfx(SFX_CURSOR, 64);
            self.side = 0;
            self.c.drawn = false;
        }
    }
}

/// the proposal (vt `0x0804B604`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proposal {
    pub c: Container,
}

impl Proposal {
    /// enter `0x0801EA0C`
    pub fn new(t: &TextData) -> Proposal {
        let mut c = Container::default();
        ab(&mut c, t);
        let bx = |x: i32, s: usize, active: bool, id: i32, nav: [i32; 4]| {
            let mut b = TabBox::new(t, x, 0x78, t.s(s), active);
            b.id = id;
            b.nav = nav;
            Widget::TabBox(b)
        };
        c.add(bx(0x40, 85, true, 0, [-1, -1, -1, 1]));
        c.add(bx(0x82, 86, false, 1, [-1, -1, 0, -1]));
        c.add(Widget::Cursor(Cursor { t: 0 }));
        Proposal { c }
    }

    /// redraw `0x0801EBFE`: frame + children, content `0x0801EB40`
    fn draw(&self, pick: &Picker, d: &MenuData, cv: &mut Canvas) {
        let t = &d.text;
        kit::page_frame(t, cv, 79);
        self.c.draw(t, cv);
        vsep(cv, 0x75, 0x1E, 0x6E);
        let mut p = pen();
        for (x, s, l) in [(0x16, 80, &pick.mine), (0x8C, 81, &pick.friend)] {
            p.x = x;
            p.y = 0x28;
            cv.text(t, &mut p, t.s(s));
            for (k, e) in l.iter().enumerate().take(6) {
                p.x = x;
                p.y = 14 * k as i32 + 0x36;
                entry(cv, d, &mut p, e);
            }
        }
    }
}

/// "How many to trade?" (vt `0x0804BC04`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CountBox {
    pub c: Container,
    /// +0x114 the entry
    pub e: TradeEntry,
    /// +0x11C the count, +0x120 the held changes, +0x124 the step, +0x128 the delay to set, +0x12C the delay
    pub count: u32,
    pub held_n: i32,
    pub step: u32,
    pub delay0: i32,
    pub delay: i32,
    pub left: usize,
    pub right: usize,
}

impl CountBox {
    /// enter `0x0801ED34`
    fn new(t: &TextData, e: TradeEntry) -> CountBox {
        let mut c = Container::default();
        let lab = |x: i32, y: i32, s: usize| Widget::Label(Label { x, y, text: t.s(s).to_vec(), colour: 5, align: 0, font: Font::Menu });
        c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x36, 0, true, false)));
        c.add(lab(0x1C, 0x42, 23));
        let bx = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x36, 1, true, false)));
        c.add(lab(bx + 0x10, 0x42, 26));
        c.add(lab(0x10, 0x1C, 83));
        let left = c.add(Widget::Button(Button::new(BTN_LEFT, 0x24, 0x24, 4, true, false)));
        let right = c.add(Widget::Button(Button::new(BTN_RIGHT, 0x56, 0x24, 5, true, false)));
        CountBox { c, e, count: 1, held_n: 0, step: 1, delay0: 0, delay: 0, left, right }
    }

    fn top(&self) -> u32 {
        self.e.amount.min(COUNT_MAX)
    }

    /// the step's count part (`0x0801EF8E`, held LEFT / RIGHT)
    fn hold(&mut self, held: u16) {
        if held & (LEFT | RIGHT) == 0 {
            self.delay0 = 10;
            self.delay = 0;
            self.step = 1;
            self.held_n = 0;
            return;
        }
        if self.delay != 0 {
            self.delay -= 1;
            return;
        }
        self.held_n += 1;
        if self.held_n > 50 {
            if self.step == 1 && self.count % 100 == 0 {
                self.step = 10;
                self.held_n = 0;
            } else if self.step == 10 && self.count % 1000 == 0 {
                self.step = 100;
                self.held_n = 0;
            }
        }
        if held & LEFT != 0 {
            self.count = self.count.saturating_sub(self.step);
        }
        if held & RIGHT != 0 {
            self.count = (self.count + self.step).min(self.top());
        }
        if self.delay0 != 0 {
            self.delay0 -= 1;
        }
        self.delay = self.delay0;
        self.c.drawn = false;
    }

    /// redraw `0x0801EECA` (136x80): the prompts' state, the box, children, the entry, the count
    fn layer(&self, d: &MenuData) -> MenuLayer {
        let t = &d.text;
        let mut c = self.c.clone();
        for (i, en) in [(self.left, self.count > 0), (self.right, self.count < self.top())] {
            if let Some(Widget::Button(b)) = c.kids.get_mut(i) {
                b.enabled = en;
            }
        }
        let mut cv = Canvas::new();
        super::super::title::popup_box(&mut cv, 0x88, 0x50);
        c.draw(t, &mut cv);
        // (the pen as the children's last label left it: colour 5, shadow 1)
        let mut p = Pen::new(MenuFont::Menu, 0x10, 0x10, 0, 5, Some(1));
        entry(&mut cv, d, &mut p, &self.e);
        let mut p = Pen::new(MenuFont::Menu, 0x44, 0x2F, 1, 5, Some(1));
        let fmt: Vec<u16> = " %d".encode_utf16().collect();
        cv.textf(t, &mut p, &fmt, &[Arg::I(self.count as i32)]);
        MenuLayer { w: 0x88, h: 0x50, x: 56, y: 48, priority: 3, ops: cv.ops }
    }

    fn sync_enabled(&mut self) {
        let (l, r) = (self.count > 0, self.count < self.top());
        for (i, en) in [(self.left, l), (self.right, r)] {
            if let Some(Widget::Button(b)) = self.c.kids.get_mut(i) {
                b.enabled = en;
            }
        }
    }
}

/// a box over the page (nested modes on BG0)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TBox {
    Count(CountBox),
    /// "Waiting for a friend..." (the link box `0x08024120`): after Propose Trade (`answer` None) or after ACCEPT /
    /// DECLINE (`answer` = ours)
    Wait { c: Container, answer: Option<bool> },
    /// TRADE COMPLETE! (`t`: its updates)
    Complete { c: Container, t: u32 },
}

/// what the page's handler does in the exit stub's iteration of a box / the proposal that ended with a result
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum After {
    /// the count box's result (for the list's / the offer's entry); -1 none
    Count(i32),
    /// a box ended without a change (cancelled): the page under it is drawn again
    Redraw,
    /// the proposal ended: 1 B, 2 both accepted (TRADE COMPLETE!), 3 declined
    Proposal(i32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trade {
    pub pick: Picker,
    /// the proposal page (over the picker, both BG1 pages)
    pub prop: Option<Proposal>,
    pub boxed: Option<TBox>,
    /// the BG1 page as last drawn while a nested mode keeps it on screen
    pub under: Option<Box<Trade>>,
    /// the exit stub's iteration is next: what the handler does then
    pub after: Option<After>,
}

impl Trade {
    pub fn new(w: &World) -> Trade {
        Trade { pick: Picker::new(w), prop: None, boxed: None, under: None, after: None }
    }

    /// the container the menu's redraw and input go by
    pub fn container(&mut self) -> &mut Container {
        match (&mut self.boxed, &mut self.prop) {
            (Some(TBox::Count(b)), _) => &mut b.c,
            (Some(TBox::Wait { c, .. }), _) | (Some(TBox::Complete { c, .. }), _) => c,
            (None, Some(p)) => &mut p.c,
            (None, None) => &mut self.pick.c,
        }
    }
    pub fn container_ref(&self) -> &Container {
        match (&self.boxed, &self.prop) {
            (Some(TBox::Count(b)), _) => &b.c,
            (Some(TBox::Wait { c, .. }), _) | (Some(TBox::Complete { c, .. }), _) => c,
            (None, Some(p)) => &p.c,
            (None, None) => &self.pick.c,
        }
    }

    pub fn before_draw(&mut self) {
        self.pick.prepare();
        if let Some(TBox::Count(b)) = &mut self.boxed {
            b.sync_enabled();
        }
    }
    pub fn after_draw(&mut self) {
        if self.boxed.is_none() && self.prop.is_none() && self.under.is_none() {
            self.pick.arrows();
        }
    }

    /// the screen: the BG1 page (the one kept under a nested mode, else the live one) and the box
    pub fn view(&self, d: &MenuData) -> wf_view::MenuView {
        let pic = self.under.as_deref().unwrap_or(self);
        let mut cv = Canvas::new();
        match &pic.prop {
            Some(p) => p.draw(&pic.pick, d, &mut cv),
            None => pic.pick.draw(d, &mut cv),
        }
        let mut v = super::super::surface_view(cv);
        v.layers[0].priority = 3;
        let t = &d.text;
        match &self.boxed {
            Some(TBox::Count(b)) => v.layers.push(b.layer(d)),
            Some(TBox::Wait { c, .. }) => {
                let mut cv = Canvas::new();
                super::super::title::popup_box(&mut cv, 136, 56);
                c.draw(t, &mut cv);
                v.layers.push(MenuLayer { w: 136, h: 56, x: 56, y: 64, priority: 0, ops: cv.ops });
            }
            Some(TBox::Complete { c, .. }) => {
                let mut cv = Canvas::new();
                super::super::title::popup_box(&mut cv, 128, 56);
                c.draw(t, &mut cv);
                v.layers.push(MenuLayer { w: 128, h: 56, x: 56, y: 64, priority: 3, ops: cv.ops });
            }
            None => {}
        }
        v
    }

    /// the page as on screen now (to keep under a nested mode): the shown copy's
    fn shown(w: &World) -> Option<Box<Trade>> {
        match w.menu.shown.as_deref() {
            Some(super::super::Screen::Link(Link { page: super::LinkPage::Trade(t), .. })) => {
                Some(t.under.clone().unwrap_or_else(|| {
                    let mut x = t.clone();
                    x.boxed = None;
                    x
                }))
            }
            _ => None,
        }
    }

    /// the shown copy loses its box (BG0 off at the box's exit)
    fn unbox_shown(w: &mut World) {
        if let Some(super::super::Screen::Link(Link { page: super::LinkPage::Trade(t), .. })) = w.menu.shown.as_deref_mut() {
            t.boxed = None;
        }
    }

    fn open_box(&mut self, w: &World, b: TBox) {
        if self.under.is_none() {
            self.under = Trade::shown(w);
        }
        self.boxed = Some(b);
    }
}

fn trade_of(l: &mut Link) -> &mut Trade {
    match &mut l.page {
        super::LinkPage::Trade(t) => t,
        _ => unreachable!("not the trade"),
    }
}

/// one iteration of the Trading Post's pages (the [`Link`] screen's step on [`super::LinkPage::Trade`])
pub fn step(l: &mut Link, w: &mut World) -> super::super::Action {
    use super::super::Action;
    let t = trade_of(l);
    let text = w.data.menu.clone().expect("menu content");
    // the exit stub's iteration: the handler goes on with the result (no input)
    if let Some(a) = t.after.take() {
        match a {
            After::Count(n) => {
                t.under = None;
                if n > 0 {
                    if t.pick.side == 0 {
                        t.pick.take(n as u32);
                    } else {
                        t.pick.give_back(w, n as u32);
                    }
                }
                t.pick.c.drawn = false;
            }
            After::Redraw => {
                t.under = None;
                match &mut t.prop {
                    Some(p) => p.c.drawn = false,
                    None => t.pick.c.drawn = false,
                }
            }
            After::Proposal(2) => {
                // 0x0801FDCC: the trade applied (the app's: LinkAction::Complete), TRADE COMPLETE! over the last page
                t.prop = None;
                let mut c = Container::default();
                for (y, s) in [(0x14, 88), (0x22, 89)] {
                    c.add(Widget::Label(Label { x: 0x3C, y, text: text.text.s(s).to_vec(), colour: 5, align: 1, font: Font::Menu }));
                }
                t.boxed = Some(TBox::Complete { c, t: 0 });
                let (mine, friend) = (t.pick.mine.clone(), t.pick.friend.clone());
                l.actions.push(LinkAction::Complete { mine, friend });
            }
            After::Proposal(_) => {
                t.prop = None;
                t.under = None;
                t.pick.c.drawn = false;
            }
        }
        return Action::None;
    }
    match &mut t.boxed {
        Some(TBox::Complete { t: n, .. }) => {
            // done check 0x08020504 (the save written in the first)
            *n += 1;
            if *n > COMPLETE_UPDATES {
                return Action::Replace(super::super::Screen::Title(super::super::title::Title::multi(w)));
            }
            let (_, _) = kit::run_step(l, w, |p| trade_of(p).container(), |_, _, _, _| (false, None));
            return Action::None;
        }
        Some(TBox::Wait { answer, .. }) => {
            // the link's exchange (the box's step `0x0801F0FE` / `0x0801F2E6`, before its widgets)
            match *answer {
                None => {
                    if let Some(o) = l.friend.offer.take() {
                        let t = trade_of(l);
                        t.boxed = None;
                        t.under = None;
                        t.pick.friend = o;
                        t.prop = Some(Proposal::new(&text.text));
                        return Action::None;
                    }
                }
                Some(ours) => {
                    if let Some(theirs) = l.friend.answer.take() {
                        let t = trade_of(l);
                        t.boxed = None;
                        Trade::unbox_shown(w);
                        t.after = Some(After::Proposal(if ours && theirs { 2 } else { 3 }));
                        return Action::None;
                    }
                }
            }
            let (_, act) = kit::run_step(l, w, |p| trade_of(p).container(), |p, w, _, code| {
                if code != 1 {
                    return (false, None);
                }
                // events 0x080242C2: B ends the box (result 1, no sound)
                let ours = matches!(trade_of(p).boxed, Some(TBox::Wait { answer: Some(_), .. }));
                p.actions.push(if ours { LinkAction::CancelAnswer } else { LinkAction::CancelPropose });
                let t = trade_of(p);
                t.boxed = None;
                t.after = Some(After::Redraw);
                Trade::unbox_shown(w);
                (true, Some(Action::None))
            });
            return act.unwrap_or(Action::None);
        }
        Some(TBox::Count(b)) => {
            b.hold(w.menu.held);
            let (_, act) = kit::run_step(l, w, |p| trade_of(p).container(), |p, w, _, code| {
                let t = trade_of(p);
                let Some(TBox::Count(b)) = &t.boxed else { return (false, None) };
                let r = match code {
                    // events 0x0801EF5C: A the count (none for 0), B none
                    0 => if b.count > 0 { b.count as i32 } else { -1 },
                    1 => -1,
                    _ => return (false, None),
                };
                t.boxed = None;
                t.after = Some(After::Count(r));
                Trade::unbox_shown(w);
                (true, Some(Action::None))
            });
            return act.unwrap_or(Action::None);
        }
        None => {}
    }
    if t.prop.is_some() {
        let (_, act) = kit::run_step(l, w, |p| trade_of(p).container(), |p, w, _, code| {
            let t = trade_of(p);
            match code {
                // events 0x0801EC38: A answers (sound 40, the answer's box), B leaves (sound 40, result 1)
                0 => {
                    w.sfx(SFX_MENU, 64);
                    let accept = t.prop.as_ref().map(|p| p.c.sel == 0).unwrap_or(true);
                    let c = super::super::title::wait_kids(&w.data.menu.as_ref().expect("menu content").text, 0);
                    t.open_box(w, TBox::Wait { c, answer: Some(accept) });
                    p.actions.push(LinkAction::Answer(accept));
                    (true, None)
                }
                1 => {
                    w.sfx(SFX_MENU, 64);
                    if t.under.is_none() {
                        t.under = Trade::shown(w);
                    }
                    t.after = Some(After::Proposal(1));
                    p.actions.push(LinkAction::LeaveProposal);
                    (true, Some(Action::None))
                }
                _ => (false, None),
            }
        });
        return act.unwrap_or(Action::None);
    }
    // the picker (step 0x0801FF02: the container step, then the done check when nothing took the input)
    let (consumed, act) = kit::run_step(l, w, |p| trade_of(p).container(), |p, w, _, code| {
        let t = trade_of(p);
        match code {
            1 => {
                // B: back to Select Saved Game (a new list mode)
                p.actions.push(LinkAction::Unpick);
                let mut s = Link::saves(w, true);
                s.actions = std::mem::take(&mut p.actions);
                s.friend = p.friend.clone();
                *p = s;
                // (its figures are made in this iteration, the handler's mode switch: their first update is its own)
                if let super::LinkPage::Saves(g) = &mut p.page {
                    g.prepare();
                    g.build_figs(w);
                }
                for k in super::super::Page::container(p).kids.iter_mut() {
                    if let Widget::Cursor(c) = k {
                        c.t = 1;
                    }
                }
                (true, Some(Action::None))
            }
            7 => {
                let c = super::super::title::wait_kids(&w.data.menu.as_ref().expect("menu content").text, 0);
                t.open_box(w, TBox::Wait { c, answer: None });
                let mine = t.pick.mine.clone();
                p.actions.push(LinkAction::Propose(mine));
                (true, None)
            }
            _ => {
                let open = if t.pick.side == 0 {
                    t.pick.list_event(code).map(|i| t.pick.list[i])
                } else if t.pick.side == 1 {
                    t.pick.offer_event(w, code).map(|i| t.pick.mine[i])
                } else {
                    None
                };
                if let Some(e) = open {
                    let b = CountBox::new(&w.data.menu.as_ref().expect("menu content").text, e);
                    t.open_box(w, TBox::Count(b));
                }
                (false, None)
            }
        }
    });
    if let Some(a) = act {
        return a;
    }
    if !consumed {
        if let super::LinkPage::Trade(t) = &mut l.page {
            if t.boxed.is_none() {
                t.pick.done_check(w);
            }
        }
    }
    Action::None
}
