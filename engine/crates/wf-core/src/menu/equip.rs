//! The Equip page (mode vt `0x08049588`, 0x118 bytes, +0x114 = the slot; enter `0x08016CE8`, redraw `0x08016E4A`,
//! events `0x08016FC4`, done check `0x08017078`), the equipment list of a slot (mode vt `0x0804A8F4`, ctor
//! `0x0801D674`, 0x924 bytes; enter `0x0801D798`, redraw `0x0801D988`, events `0x0801DCD4`, preview `0x0801D5F8`),
//! the item's action dialog (nested mode vt `0x0804AFCC`, ctor `0x0801D0A8`: Equip to / Unequip from / Drop, a
//! 192x96 surface on BG0 at (24, 40)) and the "Can't drop here!" message (nested mode vt `0x0804A8C8`, 128x40 at
//! (56, 64)).
//!
//! Equipment state: the owned list (GameData +0xE4, `Story.equipment`) whose entries the game marks with the wearer
//! (high nibble = character + 1) and the characters' slots (record +0x3C, `CharState.equip`). The remake keeps no
//! marks: they are derived from the slots (each character in index order marks the first unmarked copy of each
//! worn item), which equals the game's marks unless a character wears a later copy of a duplicated item.
//! Equip `0x080045C8` / unequip `0x08004544` / remove `0x08004656` change the story and the live fighters
//! (`status::apply_record`: RecalcStats on the object).

use super::inventory::dialog_frame;
use super::kit::{
    self, Button, Container, Cursor, Font, Label, Widget, BTN_A, BTN_B, BTN_DOWN, BTN_L, BTN_R,
    BTN_UP,
};
use super::shop::{hsep, text_trunc, text_wrap, vsep};
use super::text::{Arg, Canvas, Pen};
use super::{Action, MenuData, Page, Screen};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuView};

/// video frames one redraw takes (page / list; the dialogs)
pub const REDRAW_FRAMES: u8 = 4;
pub const DIALOG_FRAMES: u8 = 2;

/// the "-None-" item of each slot (`0x080555B8`)
pub const NONE_ITEM: [i32; 4] = [0, 44, 77, 112];
/// the wearers' colours of the " E" marks (`0x086B7590`, by character)
pub const WEARER_COLOUR: [u8; 9] = [4, 6, 7, 9, 8, 117, 3, 5, 2];

/// the wearer marks of the owned list (0 = none, else character + 1)
pub fn marks(w: &World) -> Vec<u8> {
    let mut m = vec![0u8; w.story.equipment.len()];
    for (c, cs) in w.story.chars.iter().enumerate() {
        for s in 0..4 {
            let id = cs.equip[s];
            if id == NONE_ITEM[s] {
                continue;
            }
            if let Some(k) = (0..m.len()).find(|&k| m[k] == 0 && w.story.equipment[k] == id) {
                m[k] = (c + 1).min(15) as u8;
            }
        }
    }
    m
}

fn slot_of(d: &MenuData, item: i32) -> u8 {
    d.items
        .equipment
        .get(item.max(0) as usize)
        .map(|e| e.slot)
        .unwrap_or(0)
}

/// RecalcStats (`0x08013C66`) of character `ci` wearing `equip`: STR, POW, END, SPD (record +0x4C..+0x54); the timed
/// buff of a live fighter playing it counts
pub fn eff_with(w: &World, ci: i32, equip: [i32; 4]) -> [i32; 4] {
    let Some(cs) = w.story.chars.get(ci.max(0) as usize) else {
        return [0; 4];
    };
    let mut rec = cs.rec.clone();
    let mut boost = None;
    for s in 0..w.players.len() {
        let Some(i) = w.player_idx(s) else { continue };
        if let crate::world::Body::Player(p) = &w.ents[i].body {
            if crate::story::char_of_form(&w.data, p.form) == Some(ci) {
                rec = p.rec.clone();
                if p.holy_left > 0 {
                    boost = Some(p.boost);
                }
            }
        }
    }
    let eq: Vec<Option<&crate::content::Equip>> = equip
        .iter()
        .map(|&e| w.data.equipment.iter().find(|q| q.id == e))
        .collect();
    let e = crate::progression::recalc_stats_with(&rec, &eq, boost);
    [e.str_, e.pow, e.end, e.spd]
}

fn cur_char(w: &World) -> i32 {
    w.story.current
}

fn cur_equip(w: &World) -> [i32; 4] {
    w.story
        .chars
        .get(cur_char(w).max(0) as usize)
        .map(|c| c.equip)
        .unwrap_or(NONE_ITEM)
}

fn level(w: &World) -> i32 {
    super::status::level(w, &super::inventory::leader_rec(w))
}

/// the four page buttons and labels every Equip mode builds (A Select / B Cancel at y `yb`, L / R, the neighbours)
fn common_children(
    c: &mut Container,
    t: &super::text::TextData,
    yb: i32,
    lb: i32,
    lr: bool,
) -> usize {
    let sel_s = t.s(23);
    let a = c.add(Widget::Button(Button::new(BTN_A, 0xC, yb, 0, true, false)));
    c.add(Widget::Label(Label {
        x: 0x1C,
        y: lb,
        text: sel_s.to_vec(),
        colour: 5,
        align: 0,
        font: Font::Menu,
    }));
    let bx = t.font(MenuFont::Menu).measure(sel_s) + 0x1E;
    c.add(Widget::Button(Button::new(BTN_B, bx, yb, 1, true, false)));
    c.add(Widget::Label(Label {
        x: bx + 0x10,
        y: lb,
        text: t.s(26).to_vec(),
        colour: 5,
        align: 0,
        font: Font::Menu,
    }));
    if lr {
        c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
        c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
        c.add(Widget::Label(Label {
            x: 0x22,
            y: 0x14,
            text: t.s(33).to_vec(),
            colour: 2,
            align: 0,
            font: Font::Menu,
        }));
        c.add(Widget::Label(Label {
            x: 0xCE,
            y: 0x14,
            text: t.s(2).to_vec(),
            colour: 2,
            align: 2,
            font: Font::Menu,
        }));
    }
    a
}

/// the slots page
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Slots {
    pub c: Container,
    pub slot: i32,
    /// sampled when drawn: the slots' items, STR / POW / END / SPD
    pub items: [i32; 4],
    pub eff: [i32; 4],
}

/// one entry of a slot's list (+0x154: the GameData entry, its list index)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entry {
    pub item: i32,
    pub mark: u8,
    pub index: i32,
}

/// the equipment list of a slot
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct List {
    pub c: Container,
    pub slot: i32,
    pub entries: Vec<Entry>,
    /// +0x138 top, +0x13C selection
    pub top: i32,
    pub cur: i32,
    /// +0x118 the stats now, +0x128 with the selected entry worn
    pub now: [i32; 4],
    pub preview: [i32; 4],
    pub a: usize,
    pub up: Option<usize>,
    pub down: Option<usize>,
    pub cursor: Option<usize>,
    /// sampled when drawn: the character's level
    pub level: i32,
    pub dialog: Option<Action2>,
    pub cant: Option<Container>,
    /// the list as it is on screen when a nested mode opens (its BG2 surface is not redrawn meanwhile)
    pub under: Option<Box<List>>,
    /// a nested mode ended with this result: its exit stub's iteration first (see `inventory.rs`)
    pub stub: Option<i32>,
    pub hold: Container,
}

/// the action dialog (`0x0801D0A8`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Action2 {
    pub c: Container,
    /// +0x114 GameData list index, +0x118 item
    pub index: i32,
    pub item: i32,
    /// +0x120 may equip (level, not fused), +0x124 "Equip to", +0x128 "Unequip from", +0x12C last option
    pub can: bool,
    pub equip_opt: bool,
    pub unequip_opt: bool,
    pub last: i32,
    /// the entry's wearer mark (+0x130 -> the GameData entry)
    pub mark: u8,
    /// sampled: the current character's form, the wearer's form
    pub form: i32,
    pub wearer_form: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Equip {
    pub slots: Option<Slots>,
    pub list: Option<List>,
}

impl Equip {
    /// `0x08016BCE` (R from Inventory): slot 0
    pub fn new(w: &World) -> Equip {
        Equip::with_slot(w, 0)
    }
    pub fn with_slot(w: &World, slot: i32) -> Equip {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let mut c = Container::default();
        common_children(&mut c, t, 0x84, 0x90, true);
        c.add(Widget::Cursor(Cursor { t: 0 }));
        Equip {
            slots: Some(Slots {
                c,
                slot,
                items: NONE_ITEM,
                eff: [0; 4],
            }),
            list: None,
        }
    }
    /// a cut inside the page (wf-import): the slots page at `slot` (its object +0x114 = the slot, +0x10C the
    /// selected control), or - `list` = (+0x138 top, +0x13C selection, the action dialog's +0x10C selection when that
    /// nested mode runs) - the slot's equipment list
    pub fn imported(w: &World, slot: i32, sel: i32, list: Option<(i32, i32, Option<i32>)>) -> Equip {
        let Some((top, cur, dialog)) = list else {
            let mut e = Equip::with_slot(w, slot);
            if let Some(s) = e.slots.as_mut() {
                s.c.sel = sel;
            }
            return e;
        };
        let mut l = List::new(w, slot);
        l.top = top;
        l.cur = cur;
        l.preview(w);
        if let Some(dsel) = dialog {
            if let Some(e) = l.entries.get(cur.max(0) as usize).cloned() {
                l.under = Some(Box::new(l.clone()));
                let mut dl = Action2::new(w, e.index, e.item, e.mark);
                dl.c.sel = dsel;
                l.dialog = Some(dl);
            }
        }
        Equip { slots: None, list: Some(l) }
    }

    fn list_of(w: &World, slot: i32) -> Equip {
        Equip {
            slots: None,
            list: Some(List::new(w, slot)),
        }
    }
}

impl Slots {
    fn sample(&mut self, w: &World) {
        self.items = cur_equip(w);
        self.eff = eff_with(w, cur_char(w), self.items);
        // 0x08016E36: (0x12, slot * 16 + 0x24)
        self.c.point = Some(Some((0x12, self.slot * 16 + 0x24)));
    }

    fn step(&mut self, w: &mut World) -> Action {
        let slot = self.slot;
        let (consumed, act) = kit::run_step(
            self,
            w,
            |p| &mut p.c,
            |_p, w, _src, code| match code {
                0 => {
                    w.sfx(super::SFX_MENU, 64);
                    (
                        true,
                        Some(Action::Replace(Screen::Equip(Equip::list_of(w, slot)))),
                    )
                }
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    (true, Some(Action::Close))
                }
                6 | 7 => {
                    w.sfx(super::SFX_MENU, 64);
                    (
                        true,
                        Some(Action::Replace(super::page_screen(w, 2, code == 7))),
                    )
                }
                _ => (false, None),
            },
        );
        if let Some(a) = act {
            return a;
        }
        if consumed {
            return Action::None;
        }
        // done check 0x08017078
        let k = w.menu.pressed;
        if k & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::Done;
        }
        if k & crate::input::UP != 0 {
            w.sfx(super::SFX_CURSOR, 64);
            self.slot = (self.slot + 3) & 3;
            self.c.drawn = false;
        }
        if k & crate::input::DOWN != 0 {
            w.sfx(super::SFX_CURSOR, 64);
            self.slot = (self.slot + 1) & 3;
            self.c.drawn = false;
        }
        Action::None
    }

    /// `0x08016E4A`
    fn draw(&self, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        // 0x08006718 with the children
        kit::page_frame(t, &mut cv, 36);
        self.c.draw(t, &mut cv);
        vsep(&mut cv, 0xA5, 0x21, 0x56);
        hsep(&mut cv, 0x10, 0x5E, 0xE0);
        let mut pen = Pen::new(MenuFont::Menu, 0xAF, 0x2D, 0, 5, Some(1));
        for (k, s) in [55usize, 58, 61, 64].iter().enumerate() {
            pen.x = 0xAF;
            pen.y = 0x2D + 12 * k as i32;
            cv.textf(t, &mut pen, t.s(*s), &[Arg::I(self.eff[k])]);
        }
        for k in 0..4 {
            let it = d.items.equipment.get(self.items[k].max(0) as usize);
            pen.x = 0x22;
            pen.y = 16 * k as i32 + 0x28;
            text_trunc(
                &mut cv,
                t,
                &mut pen,
                0x7A,
                it.map(|e| e.name.as_slice()).unwrap_or(&[]),
            );
            cv.image(
                it.map(|e| e.icon.as_str()).unwrap_or(""),
                0,
                16,
                16,
                0x10,
                16 * k as i32 + 0x1C,
            );
            if self.slot == k as i32 {
                pen.x = 0x13;
                pen.y = 0x6A;
                pen.line_h = 0xC;
                text_wrap(
                    &mut cv,
                    t,
                    &mut pen,
                    0xCC,
                    it.map(|e| e.desc.as_slice()).unwrap_or(&[]),
                );
            }
        }
        super::surface_view(cv)
    }
}

impl List {
    /// `0x0801D674` + enter `0x0801D798`
    fn new(w: &World, slot: i32) -> List {
        let d = w.data.menu.as_ref().expect("menu content");
        let t = &d.text;
        let ci = cur_char(w);
        let now = eff_with(w, ci, cur_equip(w));
        let entries = entries_of(w, d, slot);
        let mut cur = 0;
        for (k, e) in entries.iter().enumerate() {
            if e.mark as i32 - 1 == ci {
                cur = k as i32;
            }
        }
        let mut c = Container::default();
        let a = common_children(&mut c, t, 0x84, 0x90, true);
        let (mut up, mut down, mut cursor) = (None, None, None);
        if !entries.is_empty() {
            up = Some(c.add(Widget::Button(Button::new(
                BTN_UP, 0x9E, 0x1E, 2, true, true,
            ))));
            down = Some(c.add(Widget::Button(Button::new(
                BTN_DOWN, 0x9E, 0x4B, 3, true, true,
            ))));
            cursor = Some(c.add(Widget::Cursor(Cursor { t: 0 })));
        } else if let Some(Widget::Button(b)) = c.kids.get_mut(a) {
            b.enabled = false;
        }
        let mut l = List {
            c,
            slot,
            entries,
            top: (cur - 2).max(0),
            cur,
            now,
            preview: now,
            a,
            up,
            down,
            cursor,
            level: 0,
            dialog: None,
            cant: None,
            under: None,
            stub: None,
            hold: Container {
                drawn: true,
                ..Container::default()
            },
        };
        l.preview(w);
        l
    }

    /// `0x0801D5F8`: the stats with the selected entry worn
    fn preview(&mut self, w: &World) {
        let Some(e) = self.entries.get(self.cur.max(0) as usize) else {
            return;
        };
        let mut eq = cur_equip(w);
        eq[self.slot.clamp(0, 3) as usize] = e.item;
        self.preview = eff_with(w, cur_char(w), eq);
    }

    fn sample(&mut self, w: &World) {
        self.level = level(w);
        let n = self.entries.len() as i32;
        if n > 0 {
            if let Some(Widget::Button(b)) = self.up.and_then(|i| self.c.kids.get_mut(i)) {
                b.enabled = self.cur > 0;
            }
            if let Some(Widget::Button(b)) = self.down.and_then(|i| self.c.kids.get_mut(i)) {
                b.enabled = n - 1 > self.cur;
            }
            if self.top < self.cur - 3 {
                self.top = self.cur - 3;
            }
            if self.cur < self.top {
                self.top = self.cur;
            }
        }
        // 0x0801D970
        self.c.point = Some(Some((0x12, (self.cur - self.top) * 16 + 0x24)));
    }

    fn container(&mut self) -> &mut Container {
        if self.stub.is_some() {
            return &mut self.hold;
        }
        if let Some(c) = self.cant.as_mut() {
            return c;
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
        if let Some(c) = self.cant.as_ref() {
            return c;
        }
        match self.dialog.as_ref() {
            Some(d) => &d.c,
            None => &self.c,
        }
    }

    fn step(&mut self, w: &mut World) -> Action {
        if let Some(r) = self.stub.take() {
            return self.nested_done(w, r);
        }
        if self.cant.is_some() {
            // 0x0803982C: any key ends it (its step returns done: no exit stub); the list redraws
            if w.menu.pressed & 0x3FF != 0 {
                self.cant = None;
                close_nested(w);
                self.c.drawn = false;
            }
            return Action::None;
        }
        if self.dialog.is_some() {
            return self.step_dialog(w);
        }
        let slot = self.slot;
        let (consumed, act) = kit::run_step(
            self,
            w,
            |p| &mut p.c,
            |p, w, _src, code| match code {
                0 => {
                    if p.entries.is_empty() {
                        return (false, None);
                    }
                    w.sfx(super::SFX_MENU, 64);
                    let e = p.entries[p.cur.max(0) as usize].clone();
                    p.under = shown_list(w);
                    p.dialog = Some(Action2::new(w, e.index, e.item, e.mark));
                    (false, None)
                }
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    (
                        true,
                        Some(Action::Replace(Screen::Equip(Equip::with_slot(w, slot)))),
                    )
                }
                2 => {
                    if p.cur != 0 {
                        w.sfx(super::SFX_CURSOR, 64);
                        p.cur -= 1;
                        p.preview(w);
                        p.c.drawn = false;
                    }
                    (false, None)
                }
                3 => {
                    if (p.entries.len() as i32) - 1 > p.cur {
                        w.sfx(super::SFX_CURSOR, 64);
                        p.cur += 1;
                        p.preview(w);
                        p.c.drawn = false;
                    }
                    (false, None)
                }
                6 | 7 => {
                    w.sfx(super::SFX_MENU, 64);
                    (
                        true,
                        Some(Action::Replace(super::page_screen(w, 2, code == 7))),
                    )
                }
                _ => (false, None),
            },
        );
        if let Some(a) = act {
            return a;
        }
        if !consumed && w.menu.pressed & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::Done;
        }
        Action::None
    }

    fn step_dialog(&mut self, w: &mut World) -> Action {
        let mut dl = self.dialog.take().expect("dialog");
        let mut result: Option<i32> = None;
        let slot = self.slot;
        let (consumed, _) = kit::run_step(
            &mut dl,
            w,
            |p| &mut p.c,
            |p, w, _src, code| {
                match code {
                    0 => {
                        w.sfx(super::SFX_MENU, 64);
                        let mut opt = p.c.sel;
                        if !p.equip_opt {
                            opt += 1;
                        }
                        if !p.unequip_opt && opt >= 1 {
                            opt += 1;
                        }
                        match opt {
                            0 => {
                                if p.can {
                                    equip(w, cur_char(w), p.index, p.item, slot);
                                    result = Some(1);
                                }
                            }
                            1 => {
                                unequip(
                                    w,
                                    p.mark as i32 - 1,
                                    slot_of(w.data.menu.as_ref().unwrap(), p.item) as i32,
                                );
                                result = Some(3);
                            }
                            _ => {
                                result = Some(if drop(w, p.index, p.item) { 2 } else { 4 });
                            }
                        }
                    }
                    1 => {
                        w.sfx(super::SFX_MENU, 64);
                        result = Some(3);
                    }
                    _ => {}
                }
                (result.is_some(), None)
            },
        );
        let Some(r) = result else {
            // done check 0x0801D576: UP / DOWN wrap over the options
            let k = w.menu.pressed;
            if !consumed {
                if k & crate::input::UP != 0 {
                    w.sfx(super::SFX_CURSOR, 64);
                    dl.c.sel -= 1;
                    if dl.c.sel < 0 {
                        dl.c.sel = dl.last;
                    }
                    dl.c.drawn = false;
                }
                if k & crate::input::DOWN != 0 {
                    w.sfx(super::SFX_CURSOR, 64);
                    dl.c.sel += 1;
                    if dl.c.sel > dl.last {
                        dl.c.sel = 0;
                    }
                    dl.c.drawn = false;
                }
            }
            self.dialog = Some(dl);
            return Action::None;
        };
        close_nested(w);
        self.stub = Some(r);
        Action::None
    }

    /// after a nested mode's exit stub: the list's handler goes on with its result (`0x0801DD34`)
    fn nested_done(&mut self, w: &mut World, r: i32) -> Action {
        let slot = self.slot;
        let ci = cur_char(w);
        match r {
            0 => self.c.drawn = false,
            1 => return Action::Replace(Screen::Equip(Equip::with_slot(w, slot))),
            2 => {
                // the entry gone: the list again from the owned equipment
                let d = w.data.menu.clone().expect("menu content");
                self.entries = entries_of(w, &d, slot);
                let n = self.entries.len() as i32;
                if n <= self.cur {
                    self.cur -= 1;
                }
                if n == 0 {
                    for i in [self.up.take(), self.down.take(), self.cursor.take()]
                        .into_iter()
                        .flatten()
                    {
                        self.c.remove(i);
                    }
                    if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.a) {
                        b.enabled = false;
                    }
                } else {
                    self.preview(w);
                }
            }
            3 => {
                let cur = self.cur.max(0) as usize;
                if let Some(e) = self.entries.get_mut(cur) {
                    if e.mark as i32 - 1 == ci {
                        return Action::Replace(Screen::Equip(Equip::with_slot(w, slot)));
                    }
                    e.mark = 0;
                }
                self.c.drawn = false;
            }
            _ => {
                // 4: "Can't drop here!"
                let t = &w.data.menu.as_ref().expect("menu content").text;
                let _ = t;
                self.under = shown_list(w);
                self.cant = Some(Container::default());
            }
        }
        Action::None
    }

    /// `0x0801D988`; with a nested mode up: the list as on screen + its BG0 layer
    fn draw(&self, d: &MenuData) -> MenuView {
        if std::env::var("EQ_DEBUG").is_ok() {
            eprintln!(
                "list view focus {:?} dialog {} under {:?}",
                self.c.focus,
                self.dialog.is_some(),
                self.under.as_ref().map(|u| u.c.focus)
            );
        }
        if self.dialog.is_some() || self.cant.is_some() {
            let base = self
                .under
                .as_deref()
                .map(|u| u.draw_page(d))
                .unwrap_or_else(|| self.draw_page(d));
            return self.add_nested(d, base);
        }
        self.draw_page(d)
    }

    fn draw_page(&self, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        cv.background(0, 0, 240, 160);
        cv.panel(8, 24, 232, 152);
        let mut pen = Pen::new(MenuFont::Bold, 120, 16, 1, 5, Some(1));
        cv.text(t, &mut pen, t.s(36));
        pen.font = MenuFont::Menu;
        if self.entries.is_empty() {
            pen.align = 1;
            pen.colour = 2;
            pen.x = 0x78;
            pen.y = 0x58;
            cv.text(t, &mut pen, t.s(73));
        } else {
            vsep(&mut cv, 0xA5, 0x2D, 0x4B);
            hsep(&mut cv, 0x10, 0x5E, 0xE0);
            pen.align = 0;
            for (k, base) in [55usize, 58, 61, 64].iter().enumerate() {
                pen.x = 0xAF;
                pen.y = 0x28 + 12 * k as i32;
                let s = if self.preview[k] == self.now[k] {
                    *base
                } else if self.preview[k] > self.now[k] {
                    base + 1
                } else {
                    base + 2
                };
                cv.textf(t, &mut pen, t.s(s), &[Arg::I(self.preview[k])]);
            }
            let sel = &self.entries[self.cur.max(0) as usize];
            let rq = d
                .items
                .equipment
                .get(sel.item.max(0) as usize)
                .map(|e| e.level as i32)
                .unwrap_or(0);
            pen.x = 0xAF;
            pen.y = 0x58;
            cv.textf(
                t,
                &mut pen,
                t.s(if self.level >= rq { 67 } else { 68 }),
                &[Arg::I(rq)],
            );
            for k in 0..4 {
                let idx = self.top + k;
                if idx >= self.entries.len() as i32 {
                    break;
                }
                let e = &self.entries[idx as usize];
                let it = d.items.equipment.get(e.item.max(0) as usize);
                let rq = it.map(|e| e.level as i32).unwrap_or(0);
                if idx == self.cur {
                    pen.colour = 2;
                    pen.x = 0x13;
                    pen.y = 0x6A;
                    pen.line_h = 0xC;
                    text_wrap(
                        &mut cv,
                        t,
                        &mut pen,
                        0xCC,
                        it.map(|e| e.desc.as_slice()).unwrap_or(&[]),
                    );
                    pen.colour = if self.level >= rq { 6 } else { 0x75 };
                } else {
                    pen.colour = if self.level >= rq { 2 } else { 0x75 };
                }
                cv.image(
                    it.map(|e| e.icon.as_str()).unwrap_or(""),
                    0,
                    16,
                    16,
                    0x10,
                    16 * k + 0x1C,
                );
                pen.x = 0x22;
                pen.y = 16 * k + 0x28;
                let name = it.map(|e| e.name.as_slice()).unwrap_or(&[]);
                if e.mark != 0 {
                    let mark = t.s(69);
                    text_trunc(
                        &mut cv,
                        t,
                        &mut pen,
                        0x7A - t.font(MenuFont::Menu).measure(mark),
                        name,
                    );
                    pen.colour = WEARER_COLOUR.get(e.mark as usize - 1).copied().unwrap_or(2);
                    cv.text(t, &mut pen, mark);
                } else {
                    text_trunc(&mut cv, t, &mut pen, 0x7A, name);
                }
            }
        }
        self.c.draw(t, &mut cv);
        super::surface_view(cv)
    }

    fn add_nested(&self, d: &MenuData, mut mv: MenuView) -> MenuView {
        let t = &d.text;
        if let Some(dl) = &self.dialog {
            mv.layers.push(wf_view::MenuLayer {
                w: 192,
                h: 96,
                x: 24,
                y: 40,
                priority: 0,
                ops: dl.draw(d).ops,
            });
        }
        if self.cant.is_some() {
            // 0x080397CC: 128x40, "Can't drop here!" centred at (0x40, 0x14), colour 2
            let mut c2 = Canvas::new();
            dialog_frame(&mut c2, 128, 40);
            let mut p = Pen::new(MenuFont::Menu, 0x40, 0x14, 1, 2, Some(1));
            p.clip = [0, 0, 128, 40];
            c2.text(t, &mut p, t.s(74));
            mv.layers.push(wf_view::MenuLayer {
                w: 128,
                h: 40,
                x: 56,
                y: 64,
                priority: 0,
                ops: c2.ops,
            });
        }
        mv
    }
}

/// a nested mode ended: its BG0 layer goes off at once (the page's last picture shows)
fn close_nested(w: &mut World) {
    if let Some(Screen::Equip(e)) = w.menu.shown.as_deref_mut() {
        if let Some(l) = e.list.as_mut() {
            // the BG2 surface under the nested mode is the list as it was on screen (e.g. its A prompt still
            // pressed from the press that opened the dialog) until the next redraw
            if let Some(u) = l.under.take() {
                *l = *u;
            }
            l.dialog = None;
            l.cant = None;
        }
    }
}

/// `0x0801DC80`: the owned entries of a slot type, in list order
fn entries_of(w: &World, d: &MenuData, slot: i32) -> Vec<Entry> {
    let m = marks(w);
    w.story
        .equipment
        .iter()
        .enumerate()
        .filter(|(_, &it)| slot_of(d, it) as i32 == slot)
        .map(|(k, &it)| Entry {
            item: it,
            mark: m.get(k).copied().unwrap_or(0),
            index: k as i32,
        })
        .collect()
}

impl Action2 {
    /// `0x0801D0A8` + enter `0x0801D15C`
    fn new(w: &World, index: i32, item: i32, mark: u8) -> Action2 {
        let d = w.data.menu.as_ref().expect("menu content");
        let t = &d.text;
        let ci = cur_char(w);
        let rq = d
            .items
            .equipment
            .get(item.max(0) as usize)
            .map(|e| e.level as i32)
            .unwrap_or(0);
        let form = w
            .story
            .chars
            .get(ci.max(0) as usize)
            .map(|c| c.form)
            .unwrap_or(0);
        let fused = w.data.form(form).is_some_and(|f| f.fusion);
        let can = level(w) >= rq && !fused;
        let equip_opt = mark as i32 - 1 != ci;
        let unequip_opt = mark != 0;
        let mut c = Container::default();
        common_children(&mut c, t, 0x44, 0x50, false);
        c.add(Widget::Cursor(Cursor { t: 0 }));
        let wearer_form = if mark != 0 {
            w.story
                .chars
                .get(mark as usize - 1)
                .map(|c| c.form)
                .unwrap_or(0)
        } else {
            0
        };
        Action2 {
            c,
            index,
            item,
            can,
            equip_opt,
            unequip_opt,
            last: equip_opt as i32 + unequip_opt as i32,
            mark,
            form,
            wearer_form,
        }
    }

    /// `0x0801D294`: 192x96
    fn draw(&self, d: &MenuData) -> Canvas {
        let t = &d.text;
        let mut cv = Canvas::new();
        dialog_frame(&mut cv, 192, 96);
        let it = d.items.equipment.get(self.item.max(0) as usize);
        cv.image(
            it.map(|e| e.icon.as_str()).unwrap_or(""),
            0,
            16,
            16,
            0x10,
            6,
        );
        let mut pen = Pen::new(MenuFont::Menu, 0x22, 0x13, 0, 5, Some(1));
        pen.clip = [0, 0, 192, 96];
        cv.text(t, &mut pen, it.map(|e| e.name.as_slice()).unwrap_or(&[]));
        let mut opt = self.c.sel;
        if !self.equip_opt {
            opt += 1;
        }
        if !self.unequip_opt && opt >= 1 {
            opt += 1;
        }
        let (x, mut y) = (0x12, 0x22);
        let fname = |f: i32| {
            d.forms
                .get(f.max(0) as usize)
                .map(|f| f.name.clone())
                .unwrap_or_default()
        };
        if self.equip_opt {
            pen.x = x;
            pen.y = y;
            pen.colour = if !self.can {
                0x75
            } else if opt == 0 {
                6
            } else {
                2
            };
            cv.textf(
                t,
                &mut pen,
                t.s(70),
                &[super::text::Arg::S(&fname(self.form))],
            );
            y += 0xC;
        }
        if self.unequip_opt {
            pen.x = x;
            pen.y = y;
            pen.colour = if opt == 1 { 6 } else { 2 };
            cv.textf(
                t,
                &mut pen,
                t.s(71),
                &[super::text::Arg::S(&fname(self.wearer_form))],
            );
            y += 0xC;
        }
        pen.x = x;
        pen.y = y;
        pen.colour = if opt == 2 { 6 } else { 2 };
        cv.text(t, &mut pen, t.s(72));
        let mut c = self.c.clone();
        c.point = Some(Some((0x12, self.c.sel * 12 + 0x1F)));
        c.draw(t, &mut cv);
        cv
    }
}

/// `0x08004544(GD, ch, slot)`: the character's slot back to "-None-"
pub fn unequip(w: &mut World, ch: i32, slot: i32) {
    if ch < 0 || !(0..4).contains(&slot) {
        return;
    }
    if let Some(cs) = w.story.chars.get_mut(ch as usize) {
        cs.equip[slot as usize] = NONE_ITEM[slot as usize];
    }
    super::status::apply_record(w, ch);
}

/// `0x080045C8(GD, ch, index, item)`: from its wearer if any, the character's old item off, then on
pub fn equip(w: &mut World, ch: i32, index: i32, item: i32, slot: i32) {
    let m = marks(w);
    if let Some(&mk) = m.get(index.max(0) as usize) {
        if mk != 0 {
            unequip(w, mk as i32 - 1, slot);
        }
    }
    if let Some(cs) = w.story.chars.get_mut(ch.max(0) as usize) {
        cs.equip[slot.clamp(0, 3) as usize] = item;
    }
    super::status::apply_record(w, ch);
}

/// the action dialog's Drop (`0x0801D420` case 2): allowed on a 2D map with the player there and fewer than 8
/// dropped items; the entry leaves the list (its wearer's slot "-None-", `0x08004656`) and a dropped item object
/// is queued at the player's footprint centre with a random speed (`0x08049ABC`: 2 x rand(512) - 256; the game
/// draws them when the deferred object is added at the field's next update, the remake now: the world is frozen
/// meanwhile, so the RNG stream is the same)
fn drop(w: &mut World, index: i32, item: i32) -> bool {
    let Some(f) = w.field.as_ref() else {
        return false;
    };
    if f.flight.is_some() {
        return false;
    }
    let Some(pi) = w.player_idx(0) else {
        return false;
    };
    let dropped = w.ents.iter().filter(|e| !e.removed && matches!(&e.body, crate::world::Body::Pickup(p) if matches!(p.kind, crate::fieldobj::PickupKind::Supply(_) | crate::fieldobj::PickupKind::Equipment(_)))).count();
    if dropped >= 8 {
        return false;
    }
    let (cx, cy) = match &w.ents[pi].body {
        crate::world::Body::Player(p) => p.a.fc(),
        _ => return false,
    };
    let vx = w.rng.rand(512) - 256;
    let vy = w.rng.rand(512) - 256;
    let m = marks(w);
    if let Some(&mk) = m.get(index.max(0) as usize) {
        if mk != 0 {
            let slot = slot_of(w.data.menu.as_ref().unwrap(), item) as i32;
            unequip(w, mk as i32 - 1, slot);
        }
    }
    if (index as usize) < w.story.equipment.len() {
        w.story.equipment.remove(index as usize);
    }
    let p = crate::fieldobj::Pickup {
        kind: crate::fieldobj::PickupKind::Equipment(item),
        x: (cx - 8) << 8,
        y: (cy - 16) << 8,
        vx,
        vy,
        z: 0x2000,
        vz: 0x800,
        t: 0,
        mv: 0,
        landed: false,
        phase: 0,
        bx: [cx - 8, cy - 16, cx + 8, cy],
        solid: None,
        spawn: None,
        held: None,
    };
    w.add(crate::world::Body::Pickup(Box::new(p)));
    true
}

impl Page for Equip {
    fn container(&mut self) -> &mut Container {
        if let Some(s) = self.slots.as_mut() {
            return &mut s.c;
        }
        self.list.as_mut().expect("equip page").container()
    }
    fn container_ref(&self) -> &Container {
        if let Some(s) = self.slots.as_ref() {
            return &s.c;
        }
        self.list.as_ref().expect("equip page").container_ref()
    }
    fn before_draw(&mut self, w: &World) {
        if let Some(s) = self.slots.as_mut() {
            s.sample(w);
        }
        if let Some(l) = self.list.as_mut() {
            if l.dialog.is_none() && l.cant.is_none() {
                l.sample(w);
            }
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        if let Some(s) = self.slots.as_mut() {
            return s.step(w);
        }
        self.list.as_mut().expect("equip page").step(w)
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        if let Some(s) = self.slots.as_ref() {
            return s.draw(d);
        }
        self.list.as_ref().expect("equip page").draw(d)
    }
    fn redraw_cost(&self) -> u8 {
        match self.list.as_ref() {
            Some(l) if l.dialog.is_some() || l.cant.is_some() => DIALOG_FRAMES,
            _ => REDRAW_FRAMES,
        }
    }
    /// the dialog over the list (equip_change, 34 iterations): only the dialog's surface is drawn - 118,700 + its
    /// draw list + 0.3 per glyph instruction, the first time + 66,400 (within 1.5k). The page itself keeps its
    /// constant (its list / slot transitions are not separated yet: `work/buu/menu/cycles/equip_*.json`)
    ///
    /// The page (slots or the item list; equip_page / equip_change, 79 iterations): 180,382 + its draw list + 2,428
    /// per text line + 0.486 per glyph instruction (within 3.6k); the first draw of a new container + 100,600 for
    /// the slots (entering, back from the list), + 62,600 + 5,000 per line for the list (within 6k)
    fn redraw_time(&self, w: &World, d: &MenuData) -> Option<crate::sndsim::Redraw> {
        let open = self
            .list
            .as_ref()
            .is_some_and(|l| l.dialog.is_some() || l.cant.is_some());
        if !open {
            let v = self.view(w, d);
            let l0 = v.layers.first()?;
            let (gn, lines) = (
                crate::sndsim::ops_cost(&d.text, &l0.ops).0,
                crate::sndsim::text_counts(&d.text, &l0.ops).3,
            );
            let mut own =
                180_382 + crate::sndsim::layer_cycles(&d.text, &l0.ops, 2_428) + gn * 486 / 1000;
            if !self.container_ref().ever {
                own += if self.slots.is_some() {
                    100_600
                } else {
                    62_600 + 5_000 * lines
                };
            }
            return Some(crate::sndsim::Redraw {
                t0: 4_641,
                base: own - 4_641,
            });
        }
        let v = self.view(w, d);
        let top = v.layers.get(1)?;
        let own = 118_700
            + crate::sndsim::layer_cycles(&d.text, &top.ops, 0)
            + crate::sndsim::ops_cost(&d.text, &top.ops).0 * 300 / 1000
            + if self.container_ref().ever { 0 } else { 66_400 };
        Some(crate::sndsim::Redraw {
            t0: 4_641,
            base: own - 4_641,
        })
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`); None = not this screen's kind
pub fn open_json(
    _w: &World,
    _v: &serde_json::Value,
) -> Option<(super::Screen, super::Transition, bool)> {
    None
}

/// the equipment list as it is on screen now (without nested layers)
fn shown_list(w: &World) -> Option<Box<List>> {
    match w.menu.shown.as_deref() {
        Some(Screen::Equip(e)) => e.list.as_ref().map(|l| {
            let mut l = l.clone();
            l.dialog = None;
            l.cant = None;
            l.under = None;
            Box::new(l)
        }),
        _ => None,
    }
}
