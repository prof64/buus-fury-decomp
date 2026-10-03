//! Save games: the slots (`World.saves`), the game list screen shared by the Save Game screen and the title's
//! Select Game (base ctor `0x08005460(obj, title, empty, arrows)`, vt `0x08049728`: enter `0x08005740`, redraw
//! `0x08005632`, rows `0x08005578`, events `0x08005714`, cursor point `0x08005882`), and the Save Game screen
//! (`save_menu(flag)` 0x8C: `0x08002720`, vt `0x08049504`).
//!
//! The Save Game screen does not wait for a choice: the cursor starts on the current game's slot (`0x03001910`,
//! the slot the game was started from), and its done check (`0x08005A50` → `0x080058C6`), which runs at the first
//! logic frame nothing consumed the input, redraws, opens the "Saving..." popup (mode vt `0x080496FC`: a 96x40
//! BG0 box at (72, 64), `0x08005096`) and writes the slot from the popup's step (`0x08004962`: GameData + the
//! location (world, area, variant, the player's x, y) + the save flag in GameData +1 bit 3, then checksummed into
//! flash — about 120 frames of CPU), then clears the palettes and ends with 2 (`save_menu` pushes 2 − 1 = 1). B at
//! that first frame ends it with 1 (pushes 0).
//!
//! What the remake writes: [`SaveData`] = the whole [`crate::story::Story`] + the location + the flag; slots are
//! plain data in the world (deterministic, snapshotted); the frontend persists a slot whenever
//! `Saves.generation` changes ([`Saves::slot_bytes`] / [`Saves::set_slot_bytes`]).

use super::kit::{self, Button, Container, Cursor, Font, Label, Widget, BTN_A, BTN_B, BTN_DOWN, BTN_UP};
use super::text::{Arg, Canvas, Pen, TextData};
use super::{Action, MenuData, Page, Transition};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuLayer, MenuView};

/// save slots (the game's flash holds 3 records of 0x8B0 bytes)
pub const SLOTS: usize = 3;

/// one saved game
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SaveData {
    pub story: crate::story::Story,
    /// GameData +0x8A0..: world, area, variant, x, y (the player's position, px)
    pub world: i32,
    pub area: i32,
    pub variant: i32,
    pub x: i32,
    pub y: i32,
    /// GameData +1 bit 3 (the `save_menu` argument)
    pub flag: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Saves {
    pub slots: Vec<Option<SaveData>>,
    /// the current game's slot (`0x03001910`): where Save Game writes
    pub current: i32,
    /// bumped on every write / delete: the frontend persists the slots when it changes
    pub generation: u32,
    /// the title chose New Game / Continue: the session builds the new field world (`menu::title::boot`)
    #[serde(default)]
    pub pending: Option<super::title::Boot>,
}

impl Saves {
    pub fn slot(&self, i: usize) -> Option<&SaveData> {
        self.slots.get(i).and_then(|s| s.as_ref())
    }
    pub fn set(&mut self, i: usize, d: Option<SaveData>) {
        if self.slots.len() < SLOTS {
            self.slots.resize(SLOTS, None);
        }
        if i < self.slots.len() {
            self.slots[i] = d;
            self.generation = self.generation.wrapping_add(1);
        }
    }
    /// a slot as bytes for the frontend's store (postcard; empty = no game)
    pub fn slot_bytes(&self, i: usize) -> Vec<u8> {
        self.slot(i).map(|d| postcard::to_allocvec(d).unwrap_or_default()).unwrap_or_default()
    }
    /// load a slot from the frontend's store (at session start); bad / empty bytes = no game
    pub fn set_slot_bytes(&mut self, i: usize, b: &[u8]) {
        if self.slots.len() < SLOTS {
            self.slots.resize(SLOTS, None);
        }
        if i < self.slots.len() {
            self.slots[i] = if b.is_empty() { None } else { postcard::from_bytes(b).ok() };
        }
    }
}

/// `0x08004962` (in the remake's terms): the story + where the leader stands
pub fn save_data(w: &World, flag: bool) -> SaveData {
    // the field writes the fighters back into the story every frame: it is current
    let story = w.story.clone();
    let (mut x, mut y) = (0, 0);
    if let Some(i) = w.player_idx(0) {
        if let crate::world::Body::Player(p) = &w.ents[i].body {
            x = p.a.x >> 8;
            y = p.a.y >> 8;
        }
    }
    let (world, area, variant) = w.field.as_ref().map(|f| (f.area.world, f.area.area, f.area.variant)).unwrap_or((0, 0, 0));
    SaveData { story, world, area, variant, x, y, flag }
}

// ---------------------------------------------------------------------------------------------------------------
// the game list

/// a slot's summary (`0x08004BC0`, 128 bytes): chapter string (event #6), area name, minutes played, the party
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Summary {
    pub exists: bool,
    pub chapter: i32,
    pub area_name: i32,
    pub minutes: i32,
    /// (form, is the current character)
    pub party: Vec<(i32, bool)>,
}

/// evaluate event #6's node (flags -> the chapter string id)
pub fn chapter(code: &[u8], s: &crate::story::Story) -> i32 {
    let mut st: Vec<i32> = Vec::new();
    let mut pc = 0usize;
    let varint = |code: &[u8], pc: &mut usize| -> i32 {
        let mut v: u32 = 0;
        while *pc < code.len() {
            let b = code[*pc];
            *pc += 1;
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                break;
            }
        }
        let v = v as i32;
        if v & 1 != 0 {
            (v >> 1).wrapping_neg()
        } else {
            v >> 1
        }
    };
    let mut steps = 0;
    while pc < code.len() && steps < 10_000 {
        steps += 1;
        let op = code[pc];
        pc += 1;
        match op {
            0x00 => {
                st.push(code.get(pc).copied().unwrap_or(0) as i32);
                pc += 1;
            }
            0x01 => st.push(varint(code, &mut pc)),
            0x02 => {
                let n = code.get(pc).copied().unwrap_or(0);
                pc += 1;
                let f = st.pop().unwrap_or(0);
                st.push(match n {
                    0x1B => s.flag(f) as i32,
                    0x1C => !s.flag(f) as i32,
                    _ => 0,
                });
            }
            0x04 | 0x05 => {
                let b = st.pop().unwrap_or(0);
                let a = st.pop().unwrap_or(0);
                st.push(if op == 4 { (a != 0 && b != 0) as i32 } else { (a != 0 || b != 0) as i32 });
            }
            0x0B | 0x0C => {
                let b = st.pop().unwrap_or(0);
                let a = st.pop().unwrap_or(0);
                st.push(if op == 0x0B { (a == b) as i32 } else { (a != b) as i32 });
            }
            0x11 => break,
            0x12 => {
                let o = pc;
                let d = u16::from_be_bytes([code.get(pc).copied().unwrap_or(0), code.get(pc + 1).copied().unwrap_or(0)]) as usize;
                pc = o + d;
            }
            0x13 => {
                let o = pc;
                let d = u16::from_be_bytes([code.get(pc).copied().unwrap_or(0), code.get(pc + 1).copied().unwrap_or(0)]) as usize;
                let v = st.pop().unwrap_or(0);
                pc = if v == 0 { o + d } else { o + 2 };
            }
            0x14 => {
                st.pop();
            }
            _ => break,
        }
    }
    st.last().copied().unwrap_or(0)
}

pub fn summary(w: &World, d: &SaveData) -> Summary {
    let code = w.data.menu.as_ref().map(|m| m.items.chapter_code.clone()).unwrap_or_default();
    let area_name = w
        .data
        .field
        .as_ref()
        .and_then(|f| f.areas.get(&(d.world, d.area)))
        .map(|a| if (0..0x10000).contains(&a.name_id) { a.name_id as i32 } else { 0 })
        .unwrap_or(0);
    let s = &d.story;
    let mut party = Vec::new();
    for (k, c) in s.chars.iter().enumerate().take(9) {
        if s.current == k as i32 || c.flags & 1 != 0 {
            let mut cur = s.current == k as i32;
            if d.flag {
                cur |= c.flags & 2 != 0;
            }
            party.push((c.form, cur));
        }
    }
    Summary { exists: true, chapter: chapter(&code, s), area_name, minutes: (s.play_frames / 3600) as i32, party }
}

/// the game list's state (+0x100..): `sel` (+0x10C), first shown row (+0x128), title / empty strings, arrows flag
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameList {
    pub c: Container,
    pub title: usize,
    pub empty: usize,
    pub arrows: bool,
    pub sel: i32,
    pub first: i32,
    /// +0x124: the selected row's colour
    pub sel_colour: u8,
    pub summaries: Vec<Summary>,
    pub up: usize,
    pub down: usize,
}

impl GameList {
    /// `0x08005460` + enter `0x08005740`
    pub fn new(w: &World, title: usize, empty: usize, arrows: bool) -> GameList {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let mut c = Container::default();
        c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: t.s(23).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let bx = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: bx + 0x10, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let up = c.add(Widget::Button(Button::new(BTN_UP, 0xD4, 0x1E, 2, true, false)));
        let down = c.add(Widget::Button(Button::new(BTN_DOWN, 0xD4, 0x74, 3, true, false)));
        c.add(Widget::Cursor(Cursor { t: 0 }));
        let summaries = (0..SLOTS).map(|i| w.saves.slot(i).map(|d| summary(w, d)).unwrap_or_default()).collect();
        GameList { c, title, empty, arrows, sel: 0, first: 0, sel_colour: 6, summaries, up, down }
    }

    /// base events `0x08005714`: UP / DOWN prompts move (0..2), redraw
    pub fn event(&mut self, code: i32) {
        match code {
            2 => {
                if self.sel != 0 {
                    self.sel -= 1;
                }
                self.c.drawn = false;
            }
            3 => {
                if self.sel < 2 {
                    self.sel += 1;
                }
                self.c.drawn = false;
            }
            _ => {}
        }
    }

    /// draw-time side effects: the first row follows the cursor, the arrows' state, the cursor point
    pub fn prepare(&mut self) {
        if self.sel < self.first {
            self.first = self.sel;
        }
        if self.sel > self.first + 1 {
            self.first = self.sel - 1;
        }
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.up) {
            b.enabled = self.arrows && self.sel != 0;
        }
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.down) {
            b.enabled = self.arrows && self.sel != 2;
        }
        self.c.point = Some(Some((0x10, (self.sel - self.first) * 0x34 + 0x26)));
    }

    /// `0x08005632`: the page frame with the children, two rows, separators
    pub fn draw(&self, t: &TextData, cv: &mut Canvas) {
        kit::page_frame(t, cv, self.title);
        self.c.draw(t, cv);
        let mut p = Pen { shadow: Some(1), colour: 5, font: MenuFont::Bold, ..Pen::default() };
        self.row(t, cv, &mut p, self.first, 0x20);
        self.row(t, cv, &mut p, self.first + 1, 0x54);
        crate::menu::shop::hsep(cv, 0x14, 0x50, 0xD0);
        crate::menu::shop::vsep(cv, 0xDB, 0x2E, 0x74);
    }

    /// `0x08005578`
    fn row(&self, t: &TextData, cv: &mut Canvas, p: &mut Pen, slot: i32, y: i32) {
        p.colour = if self.sel == slot { self.sel_colour } else { 2 };
        p.font = MenuFont::Small;
        p.align = 0;
        p.x = 0x10;
        p.y = y + 8;
        cv.textf(t, p, t.s(14), &[Arg::I(slot + 1)]);
        p.font = MenuFont::Menu;
        p.x = 0x64;
        p.y = y + 0xE;
        match self.summaries.get(slot.max(0) as usize).filter(|s| s.exists) {
            Some(s) => {
                crate::menu::shop::text_trunc(cv, t, p, 0x6C, t.s(s.chapter.max(0) as usize));
                p.x = 0x64;
                p.y = y + 0x1B;
                crate::menu::shop::text_trunc(cv, t, p, 0x6C, t.s(s.area_name.max(0) as usize));
                p.x = 0x64;
                p.y = y + 0x28;
                let fmt: Vec<u16> = "%d:%02d".encode_utf16().collect();
                cv.textf(t, p, &fmt, &[Arg::I(s.minutes / 60), Arg::I(s.minutes % 60)]);
            }
            None => cv.text(t, p, t.s(self.empty)),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    /// the list; the first idle frame saves
    #[default]
    List,
    /// "Saving..." shown (the popup mode drawn)
    Popup,
    /// written, palettes cleared (black)
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Save {
    pub list: GameList,
    pub flag: bool,
    pub stage: Stage,
    /// the palettes were cleared (B): black
    pub black: bool,
}

/// redraw frames: the list (with the redraw from the done check, then the popup's enter + draw) and the write
pub const LIST_FRAMES: u8 = 1;
pub const POPUP_FRAMES: u8 = 3;
pub const WRITE_FRAMES: u8 = 121;

impl Save {
    pub fn new(w: &World, flag: bool) -> Save {
        let mut list = GameList::new(w, 7, 13, false);
        list.sel = w.saves.current.clamp(0, SLOTS as i32 - 1);
        Save { list, flag, stage: Stage::List, black: false }
    }
}

impl Page for Save {
    fn container(&mut self) -> &mut Container {
        &mut self.list.c
    }
    fn container_ref(&self) -> &Container {
        &self.list.c
    }
    fn before_draw(&mut self, _w: &World) {
        self.list.prepare();
    }
    fn step(&mut self, w: &mut World) -> Action {
        if self.stage == Stage::Popup {
            // the popup's step writes (its CPU time: the long redraw that follows), then the palettes are cleared
            self.stage = Stage::Done;
            self.list.c.drawn = false;
            return Action::None;
        }
        if self.stage == Stage::Done {
            return Action::Pop(2);
        }
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.list.c, |p, w, _src, code| match code {
            0 => {
                p.save(w);
                (true, None)
            }
            1 => {
                p.black = true;
                if let Some(s) = w.menu.shown.as_mut() {
                    if let super::Screen::Save(x) = &mut **s {
                        x.black = true;
                    }
                }
                (true, Some(Action::Pop(1)))
            }
            c => {
                p.list.event(c);
                (false, None)
            }
        });
        if let Some(a) = act {
            return a;
        }
        if !consumed {
            self.save(w);
        }
        Action::None
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        let _ = w;
        if self.black || self.stage == Stage::Done {
            return MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() };
        }
        let t = &d.text;
        let mut cv = Canvas::new();
        self.list.draw(t, &mut cv);
        let mut v = super::surface_view(cv);
        v.layers[0].priority = 3;
        if self.stage == Stage::Popup {
            v.layers.push(popup(t, 8));
        }
        v
    }
    fn redraw_cost(&self) -> u8 {
        match self.stage {
            Stage::List => LIST_FRAMES,
            Stage::Popup => POPUP_FRAMES,
            Stage::Done => WRITE_FRAMES,
        }
    }
}

impl Save {
    /// `0x080058C6`: write the current slot, the popup
    fn save(&mut self, w: &mut World) {
        let d = save_data(w, self.flag);
        let k = self.list.sel.max(0) as usize;
        w.saves.set(k, Some(d));
        w.saves.current = k as i32;
        self.stage = Stage::Popup;
        self.list.c.drawn = false;
    }
}

/// the popup box (`0x08005096` on a 96x40 surface, mode vt `0x080496FC`): text `s` centred at (48, 20), BG0 at
/// (72, 64), priority 0
pub fn popup(t: &TextData, s: usize) -> MenuLayer {
    let (w, h) = (96, 40);
    let mut cv = Canvas::new();
    cv.background(12, 4, w - 24, h - 16);
    cv.panel(8, 0, w - 8, h - 8);
    let mut p = Pen::new(MenuFont::Menu, 0x30, 0x14, 1, 2, Some(1));
    cv.text(t, &mut p, t.s(s));
    MenuLayer { w: w as u16, h: h as u16, x: 72, y: 64, priority: 0, ops: cv.ops }
}

/// the script-opened transition (quick fade; the list shows on the 3rd black frame)
pub const SCRIPTED: Transition = Transition { fade_out: 16, enter: 4, leave: 25, fade_in: 16 };

/// native 0x8C `save_menu(flag)`: `World.menu.result` = 2 saved / 1 cancelled (the native pushes result − 1); no
/// player object = nothing happens and nothing is pushed (the caller checks `w.player_idx(0)`)
pub fn open_save(w: &mut World, flag: bool) {
    let s = Save::new(w, flag);
    super::open(w, super::Screen::Save(s), SCRIPTED, true);
}

/// a harness opening this screen from JSON: `{"kind": "save", "flag": 0}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    if v["kind"].as_str()? != "save" {
        return None;
    }
    Some((super::Screen::Save(Save::new(w, v["flag"].as_i64().unwrap_or(0) != 0)), SCRIPTED, true))
}
