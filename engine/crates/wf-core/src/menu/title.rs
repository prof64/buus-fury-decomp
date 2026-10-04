//! The title screen and the New Game / Continue / Delete Game flow.
//!
//! * Title (mode vt `0x0804A404`, enter `0x080053CC` (palettes, song 45), children `0x08005228`, redraw
//!   `0x080051E8`, events `0x080052C4`; back from the next page: vt `0x080494D8`, children only): the title picture,
//!   the menu box darkened (`0x0803F9D4`, table `0x08056A2C`, (80, 104)-(160, 144)), START / OPTIONS (outlined small
//!   font, centred at (120, 120) / (120, 134)), the blinking cursor.
//! * START → Single Player / Multiplayer (vt `0x08049B00`: enter `0x0801E4E0`, update `0x0801E6DA`, events
//!   `0x0801E578`, done check `0x0801E68A`: B back to the title); the same picture and box.
//! * Single Player → Select Game (the game list `0x08005460(9 "Select Game", 10 "New Game", arrows)`, vt
//!   `0x0804947C`: enter `0x08005A5A` adds L / R prompts at (8, 8) / (208, 8) (codes 6 / 7), events `0x08005ABE`, done
//!   check `0x08005E3A`): A = New Game on an empty slot (GameData reset `0x080042FC`, the prologue event #4 =
//!   header `0x0877CBF0`) or Continue (the slot read back, its chosen character current, EnterArea at the saved
//!   location, the 16-step fade-out `0x0804BBA4`, the fade-in `0x080495B4` by 4); L / R toggle Delete Game (title
//!   "Delete Game", selected row colour 3; A on a saved slot asks "Are you sure?" (vt `0x08049CEC`: a 160x88 BG0 box
//!   at (40, 40), Delete / Cancel) and erases it); LEFT / RIGHT on a saved slot pick the character to continue with
//!   (`0x08004C8C` / `0x08004CE0`: party members that are current or, with the save flag, flagged).
//!
//! Not reproduced: Multiplayer (link cable) and the title's OPTIONS page (vt `0x0804A430`: the options and Credits;
//! it plays its sound and stays here), the party sprites a saved slot shows (character objects with their
//! animations, `0x080054BA`) and the arrows around the chosen one (`0x08005E9A`).

use super::kit::{self, Button, Container, Cursor, ItemCtl, Label, Widget, BTN_A, BTN_B, BTN_L, BTN_R};
use super::save::{GameList, SaveData, Summary, SLOTS};
use super::text::{Canvas, Pen, TextData};
use super::{Action, MenuData, Page, Phase, Transition};
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wf_view::{MenuFont, MenuLayer, MenuOp, MenuView};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TPage {
    #[default]
    Title,
    Mode,
    Select,
}

/// what the title asks the session to build
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Boot {
    NewGame { slot: i32 },
    Continue { slot: i32, chosen: i32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Leaving {
    #[default]
    No,
    /// A chose New Game: the next frame hands over to the prologue
    NewGame,
    /// A chose Continue: the 16-step fade-out runs (level)
    Continue(u16),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Title {
    pub page: TPage,
    pub c: Container,
    pub list: GameList,
    pub delete: bool,
    /// the character each slot continues with (summary +0x7C)
    pub chosen: Vec<i32>,
    pub confirm: Option<Container>,
    pub a_btn: usize,
    pub leaving: Leaving,
    /// the "Are you sure?" mode ended: its exit stub iteration runs before the page's own (no input, no update)
    #[serde(default)]
    pub stub: bool,
    /// Delete: the erase runs in the handler before the list's redraw; the old figures stay in OAM until the next
    /// update (iterations left)
    #[serde(default)]
    pub held: Option<(u8, Box<GameList>)>,
}

/// redraw frames (the title picture is decoded each time): measured from the captures
pub const TITLE_FRAMES: u8 = 6;
pub const SELECT_FRAMES: u8 = 5;

fn item(id: i32, nav: [i32; 4], text: &[u16], y: i32, code: i32) -> Widget {
    Widget::Item(ItemCtl { id, nav, text: text.to_vec(), x: 0x78, y, code, enabled: true })
}

impl Title {
    pub fn new(w: &World) -> Title {
        let mut t = Title { page: TPage::Title, c: Container::default(), list: GameList::new(w, 9, 10, true), delete: false, chosen: Vec::new(), confirm: None, a_btn: 0, leaving: Leaving::No, stub: false, held: None };
        t.set_page(w, TPage::Title);
        t
    }

    fn set_page(&mut self, w: &World, p: TPage) {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        self.page = p;
        self.c = Container::default();
        match p {
            TPage::Title => {
                self.c.add(item(0, [1, 1, -1, -1], t.s(1), 0x78, 0));
                self.c.add(item(1, [0, 0, -1, -1], t.s(2), 0x86, 1));
                self.c.add(Widget::Cursor(Cursor { t: 0 }));
            }
            TPage::Mode => {
                self.c.add(item(0, [1, 1, -1, -1], t.s(3), 0x78, 0));
                self.c.add(item(1, [0, 0, -1, -1], t.s(4), 0x86, 1));
                self.c.add(Widget::Cursor(Cursor { t: 0 }));
            }
            TPage::Select => {
                self.list = GameList::new(w, 9, 10, true);
                self.a_btn = 0;
                self.list.c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
                self.list.c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
                self.chosen = (0..SLOTS).map(|i| w.saves.slot(i).map(|d| d.story.current).unwrap_or(0)).collect();
                self.delete = false;
            }
        }
    }

    fn summary(&self, k: i32) -> Option<&Summary> {
        self.list.summaries.get(k.max(0) as usize).filter(|s| s.exists)
    }

    /// `0x08004C8C` / `0x08004CE0`: the previous / next selectable party member of the slot
    fn cycle(&mut self, w: &World, slot: i32, dir: i32) -> bool {
        let Some(d) = w.saves.slot(slot.max(0) as usize) else { return false };
        let party: Vec<(i32, bool)> = party_of(d);
        let cur = self.chosen.get(slot as usize).copied().unwrap_or(0);
        let Some(i) = party.iter().position(|p| p.0 == cur) else { return false };
        let mut j = i as i32 + dir;
        while j >= 0 && (j as usize) < party.len() {
            if party[j as usize].1 {
                self.chosen[slot as usize] = party[j as usize].0;
                if let Some(s) = self.list.summaries.get_mut(slot as usize) {
                    s.chosen = party[j as usize].0;
                }
                return true;
            }
            j += dir;
        }
        false
    }

    fn select_event(&mut self, w: &mut World, code: i32) -> Option<Action> {
        let sel = self.list.sel;
        match code {
            0 => {
                w.sfx(super::SFX_ACCEPT, 64);
                if self.delete {
                    if self.summary(sel).is_some() {
                        // a nested mode run from the release handler: the list is not redrawn under it (its A prompt
                        // stays pressed) and its objects stop
                        self.confirm = Some(confirm_kids(&w.data.menu.as_ref().expect("menu content").text));
                        self.list.c.focus = Some(self.a_btn);
                        self.list.c.drawn = false;
                    }
                    return None;
                }
                self.list.c.drawn = false;
                self.leaving = if self.summary(sel).is_some() { Leaving::Continue(0) } else { Leaving::NewGame };
                None
            }
            1 => {
                w.sfx(super::SFX_MENU, 64);
                self.set_page(w, TPage::Mode);
                None
            }
            2 | 3 => {
                let moved = if code == 2 { sel != 0 } else { sel < 2 };
                if moved {
                    w.sfx(super::SFX_CURSOR, 64);
                    self.list.sel += if code == 2 { -1 } else { 1 };
                    if self.delete {
                        let ex = self.summary(self.list.sel).is_some();
                        if let Some(Widget::Button(b)) = self.list.c.kids.get_mut(self.a_btn) {
                            b.enabled = ex;
                        }
                    }
                }
                self.list.c.drawn = false;
                None
            }
            6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                self.delete = !self.delete;
                self.list.title = if self.delete { 11 } else { 9 };
                self.list.sel_colour = if self.delete { 3 } else { 6 };
                let en = !self.delete || self.summary(sel).is_some();
                if let Some(Widget::Button(b)) = self.list.c.kids.get_mut(self.a_btn) {
                    b.enabled = en;
                }
                self.list.c.drawn = false;
                None
            }
            _ => None,
        }
    }
}

/// a saved game's party in member order: (character, selectable)
fn party_of(d: &SaveData) -> Vec<(i32, bool)> {
    let s = &d.story;
    let mut out = Vec::new();
    for (k, c) in s.chars.iter().enumerate().take(9) {
        let k = k as i32;
        if s.current == k || c.flags & 1 != 0 {
            let mut sel = s.current == k;
            if d.flag {
                sel |= c.flags & 2 != 0;
            }
            out.push((k, sel));
        }
    }
    out
}

/// "Are you sure?" (`0x0803957C`): A + "Delete" at (12, 60) / (28, 72), B + "Cancel" after it
fn confirm_kids(t: &TextData) -> Container {
    let mut c = Container::default();
    c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x3C, 0, true, false)));
    c.add(Widget::Label(Label { x: 0x1C, y: 0x48, text: t.s(25).to_vec(), colour: 5, align: 0, font: kit::Font::Menu }));
    let x = t.font(MenuFont::Menu).measure(t.s(25)) + 0x1E;
    c.add(Widget::Button(Button::new(BTN_B, x, 0x3C, 1, true, false)));
    c.add(Widget::Label(Label { x: x + 0x10, y: 0x48, text: t.s(26).to_vec(), colour: 5, align: 0, font: kit::Font::Menu }));
    c
}

/// the box of `0x08005096` on a w x h surface
fn popup_box(cv: &mut Canvas, w: i32, h: i32) {
    cv.background(12, 4, w - 24, h - 16);
    cv.panel(8, 0, w - 8, h - 8);
}

impl Page for Title {
    fn container(&mut self) -> &mut Container {
        match (&mut self.confirm, self.page) {
            (Some(c), _) => c,
            (None, TPage::Select) => &mut self.list.c,
            _ => &mut self.c,
        }
    }
    fn container_ref(&self) -> &Container {
        match (&self.confirm, self.page) {
            (Some(c), _) => c,
            (None, TPage::Select) => &self.list.c,
            _ => &self.c,
        }
    }
    fn update(&mut self, _w: &World) {
        // the Continue fade is a nested mode: the page's update (the objects' animation) stops after its first step
        let fading = matches!(self.leaving, Leaving::Continue(l) if l > CONTINUE_TICKS);
        if let Some((n, _)) = self.held.as_mut() {
            *n -= 1;
            if *n == 0 {
                self.held = None;
            }
        }
        if self.page == TPage::Select && !fading && self.confirm.is_none() && !self.stub {
            self.list.tick_figs();
        }
    }
    fn before_draw(&mut self, w: &World) {
        if self.page == TPage::Select {
            self.list.prepare();
            self.list.build_figs(w);
        }
    }
    fn objs(&self, w: &World) -> Vec<wf_view::MenuObj> {
        if let Some((_, l)) = &self.held {
            return l.fig_objs(w);
        }
        if self.page == TPage::Select {
            self.list.fig_objs(w)
        } else {
            Vec::new()
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        match self.leaving {
            Leaving::NewGame => {
                w.saves.pending = Some(Boot::NewGame { slot: self.list.sel });
                return Action::Close;
            }
            Leaving::Continue(l) => {
                let l = l + 16;
                if l >= 256 {
                    let slot = self.list.sel;
                    let chosen = self.chosen.get(slot as usize).copied().unwrap_or(0);
                    w.saves.pending = Some(Boot::Continue { slot, chosen });
                    return Action::Close;
                }
                self.leaving = Leaving::Continue(l);
                self.list.c.drawn = false;
                return Action::None;
            }
            Leaving::No => {}
        }
        if self.stub {
            self.stub = false;
            return Action::None;
        }
        if self.confirm.is_some() {
            let (_, act) = kit::run_step(self, w, |p| p.confirm.as_mut().expect("confirm"), |p, w, _src, code| match code {
                0 => {
                    w.sfx(super::SFX_MENU, 64);
                    let k = p.list.sel.max(0) as usize;
                    p.held = Some((3, Box::new(p.list.clone())));
                    w.saves.set(k, None);
                    if let Some(s) = p.list.summaries.get_mut(k) {
                        *s = Summary::default();
                    }
                    p.confirm = None;
                    p.list.c.focus = None;
                    p.stub = true;
                    // the box's BG0 goes off at once; the erase runs before the list's redraw
                    if let Some(s) = w.menu.shown.as_mut() {
                        if let super::Screen::Title(x) = &mut **s {
                            x.confirm = None;
                        }
                    }
                    if let Some(Widget::Button(b)) = p.list.c.kids.get_mut(p.a_btn) {
                        b.enabled = false;
                    }
                    p.list.c.drawn = false;
                    (true, Some(Action::None))
                }
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    p.confirm = None;
                    p.list.c.focus = None;
                    p.list.c.drawn = false;
                    p.stub = true;
                    // the box's BG0 goes off at once (no redraw of the list)
                    if let Some(s) = w.menu.shown.as_mut() {
                        if let super::Screen::Title(x) = &mut **s {
                            x.confirm = None;
                        }
                    }
                    (true, Some(Action::None))
                }
                _ => (false, None),
            });
            return act.unwrap_or(Action::None);
        }
        match self.page {
            TPage::Title => {
                let (_, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| {
                    w.sfx(super::SFX_MENU, 64);
                    if code == 0 {
                        p.set_page(w, TPage::Mode);
                    }
                    (true, Some(Action::None))
                });
                act.unwrap_or(Action::None)
            }
            TPage::Mode => {
                let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| {
                    w.sfx(super::SFX_MENU, 64);
                    if code == 0 {
                        p.set_page(w, TPage::Select);
                    }
                    (true, Some(Action::None))
                });
                if act.is_some() {
                    return Action::None;
                }
                if !consumed && w.menu.pressed & crate::input::B != 0 {
                    w.sfx(super::SFX_MENU, 64);
                    self.set_page(w, TPage::Title);
                }
                Action::None
            }
            TPage::Select => {
                let (consumed, act) = kit::run_step(self, w, |p| &mut p.list.c, |p, w, _src, code| match p.select_event(w, code) {
                    Some(a) => (true, Some(a)),
                    None => (false, None),
                });
                if let Some(a) = act {
                    return a;
                }
                if !consumed {
                    let sel = self.list.sel;
                    if self.summary(sel).is_some() {
                        let p = w.menu.pressed;
                        let mut changed = false;
                        if p & crate::input::LEFT != 0 {
                            changed |= self.cycle(w, sel, -1);
                        }
                        if p & crate::input::RIGHT != 0 {
                            changed |= self.cycle(w, sel, 1);
                        }
                        if changed {
                            self.list.c.drawn = false;
                        }
                    }
                }
                Action::None
            }
        }
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = Canvas::new();
        match self.page {
            TPage::Title | TPage::Mode => {
                cv.ops.push(MenuOp::Image { path: "ui/images/screen_title.png".into(), frame: 0, fw: 240, fh: 160, x: 0, y: 0, key0: false });
                if d.items.title_shade.len() == 256 {
                    cv.ops.push(MenuOp::Remap { x0: 80, y0: 104, x1: 160, y1: 144, table: d.items.title_shade.clone() });
                }
                self.c.draw(t, &mut cv);
                let mut v = super::surface_view(cv);
                v.layers[0].priority = 3;
                v
            }
            TPage::Select => {
                self.list.draw(t, &mut cv);
                let mut v = super::surface_view(cv);
                v.layers[0].priority = 3;
                if let Leaving::Continue(l) = self.leaving {
                    if l > 0 {
                        v.fade = Some(wf_view::PaletteFade { level: l, target: 0 });
                    }
                }
                if let Some(c) = &self.confirm {
                    let (pw, ph) = (160, 88);
                    let mut pc = Canvas::new();
                    popup_box(&mut pc, pw, ph);
                    let mut p = Pen::new(MenuFont::Bold, 0x50, 0x14, 1, 3, Some(1));
                    pc.text(t, &mut p, t.s(11));
                    let mut p = Pen { align: 1, line_h: 12, shadow: Some(1), colour: 2, x: 0x50, y: 0x2C, font: MenuFont::Menu, clip: [0, 0, pw, ph], ..Pen::default() };
                    super::shop::text_wrap(&mut pc, t, &mut p, 0x84, t.s(27));
                    c.draw(t, &mut pc);
                    v.layers.push(MenuLayer { w: pw as u16, h: ph as u16, x: 40, y: 40, priority: 0, ops: pc.ops });
                }
                v
            }
        }
    }
    /// the title / mode page's redraw decodes the whole title picture: a long iteration whose end (6 or 7 frames)
    /// the mixer's load decides (whole run from the soft reset, `menu_timing.measure` on video frames 705 .. 800: own
    /// cycles title 1,343,860 .. 1,351,045, its first draw after the boot 1,404,063, mode page 1,389,795 .. 1,406,658)
    fn redraw_time(&self, w: &World, d: &super::MenuData) -> Option<crate::sndsim::Redraw> {
        if self.confirm.is_some() || matches!(self.leaving, Leaving::Continue(_)) || self.delete {
            return None;
        }
        // the blinking cursor drawn this time (its OBJ copy): + ~7 k (title 1,351,045 with, 1,343,860 / 1,344,002
        // without; mode page 1,397,808 with, 1,389,795 without; Select Game 454,998 with, 448,222 without)
        let cursor = self.view(w, d).layers.iter().any(|l| crate::sndsim::ops_summary(&l.ops).contains("cursor"));
        let prev_page = match w.menu.shown.as_deref() {
            Some(super::Screen::Title(t)) => Some(t.page),
            _ => None,
        };
        let own: i64 = match self.page {
            // (nothing on screen yet: the first draw after the boot screens)
            TPage::Title if w.menu.shown.is_none() => 1_404_063,
            TPage::Title => if cursor { 1_351_045 } else { 1_343_931 },
            TPage::Mode => if cursor { 1_397_808 } else { 1_389_795 },
            // A on an empty slot: the New Game iteration (GameData reset, the prologue's start)
            TPage::Select if self.leaving == Leaving::NewGame => 471_957,
            // the page's enter (its list, the slots' summaries)
            TPage::Select if prev_page != Some(TPage::Select) => 815_222,
            TPage::Select => if cursor { 454_998 } else { 448_222 },
        };
        Some(crate::sndsim::Redraw { t0: 4_641, base: own - 4_641 })
    }
    fn redraw_cost(&self) -> u8 {
        match (self.page, self.leaving) {
            // the release redraw (A up) shows a frame late; the fade then shows one picture per frame
            (_, Leaving::Continue(0)) => 2,
            (_, Leaving::Continue(_)) => 1,
            (TPage::Select, _) => SELECT_FRAMES,
            _ => TITLE_FRAMES,
        }
    }
}

/// object updates the Continue fade still sees (fade level reached)
pub const CONTINUE_TICKS: u16 = 16;

/// the title boot: no fade, the title on screen at once
pub const BOOT: Transition = Transition { fade_out: 0, enter: 1, leave: 1, fade_in: 0 };

/// show the title (a fresh session): the world under it is frozen until New Game / Continue replaces it
pub fn open_title(w: &mut World) {
    let t = Title::new(w);
    super::open(w, super::Screen::Title(t), BOOT, false);
    // the enter (`0x080053CC`) starts the title song; the placeholder world's area song never plays
    w.events.retain(|e| !matches!(e, crate::world::Event::Song { .. }));
    w.scene.song = -1;
    crate::scene::play_song(w, TITLE_SONG);
}

/// Power-on: the boot screens, then the title. Both screens are fixed-length main-loop modes that read no input:
/// the legal screen (vt `0x08049354`, update `0x08004EA6`: fade in 16, hold to 150, fade out to 166, done at 180) and
/// the logos (vt `0x080492B8`, update `0x08017AE6`: Atari / FUNimation, Webfoot loaded at 320, done after 516), with
/// the mode switches between them; the title's first iteration is video frame [`BOOT_FRAMES`] after the soft reset
/// (the real run's recording, `tools/wholerun/record.py`). The title waits under the boot (its song with it).
pub fn open_title_boot(w: &mut World) {
    open_title(w);
    w.events.retain(|e| !matches!(e, crate::world::Event::Song { .. }));
    w.scene.song = -1;
    w.menu.boot = if w.opts.no_lag { BOOT_LOGIC as u16 } else { BOOT_FRAMES };
    w.menu.boot_draw = Some(());
}

/// video frames from the soft reset to the title's first iteration (the boot screens' modes and switches)
pub const BOOT_FRAMES: u16 = 705;
/// the frame counter 0x03000F94 before the title's first iteration (the boot screens' logic frames)
pub const BOOT_LOGIC: i64 = 696;
/// the logos' mode starts song 45 in its first frame after the reset's legal screen (song player 0 from video frame
/// 181 on; the title's enter then finds it playing)
pub const BOOT_SONG_AT: u16 = 181;
/// the same in logic frames (no lag: the legal screen's 180 iterations)
pub const BOOT_SONG_AT_LOGIC: u16 = 180;

/// the title's song (`0x080053CC`)
pub const TITLE_SONG: i32 = 45;

/// the title screen is up (its world is a placeholder field: the game keeps no area there)
pub fn on_title(w: &World) -> bool {
    super::active(w) && (matches!(w.menu.shown.as_deref(), Some(super::Screen::Title(_))) || matches!(w.menu.stack.first(), Some(super::Screen::Title(_))))
}

/// `return_to_title` (0x8E: game over after script #0's "You have died!", the ending): a fresh session on the
/// title screen with the save slots and the options kept. The world under the title is a placeholder (Snakeway's
/// first area) the title's New Game / Continue replaces.
pub fn title_world(data: &Arc<crate::content::GameData>, w: &World) -> World {
    // the game data stays as the session left it (current character, zenie, supplies, equipment, every record) until
    // New Game resets it or Continue reads a slot (real run: after the credits the title holds the cleared game -
    // r072_002_4-21 / sb_044_04_20, the records compared row by row); the placeholder's player is built from it
    let mut old = w.clone();
    crate::story::sync_from_players(&mut old);
    let cfg = crate::field::FieldConfig {
        world: 1,
        area: 1,
        rng: crate::battle::RngInit::State(w.rng.a, w.rng.b),
        story: Some(old.story.clone()),
        chars: vec![old.story.current],
        ..Default::default()
    };
    let mut nw = crate::field::create(Arc::clone(data), &cfg);
    // the placeholder field's load is ours only: the game's RNG is where the last frame left it (ending 4-20 ->
    // the title, segment sb_044 f24056)
    nw.rng = w.rng.clone();
    // ... and no object holds tiles
    if let Some(f) = nw.field.as_mut() {
        if f.vram.is_some() {
            f.vram = Some(Default::default());
        }
    }
    nw.saves = w.saves.clone();
    nw.saves.pending = None;
    nw.menu.settings = w.menu.settings.clone();
    nw.story = old.story;
    nw.events.retain(|e| !matches!(e, crate::world::Event::Field(crate::field::FieldEvent::Run { .. })));
    nw.events.insert(0, crate::world::Event::SongStop);
    open_title(&mut nw);
    crate::scene::note_events(&mut nw);
    nw
}

/// Continue: black frames after the switch (the area load) and the fade-in step (`0x080495B4` +8 = 4)
pub const CONTINUE_BLACK: u16 = 29;
pub const CONTINUE_FADE_IN: u16 = 4;
/// New Game: frames the Select Game picture stays under the prologue's fade-out
pub const NEWGAME_OVERLAY: u32 = 64;

/// The session's hook after a menu step: a pending New Game / Continue builds the new field world (the game's
/// GameData reset / slot read + EnterArea). The new world keeps the save slots and the options.
pub fn boot(data: &Arc<crate::content::GameData>, w: &mut World) -> Option<World> {
    let b = w.saves.pending.take()?;
    let mut saves = w.saves.clone();
    let settings = w.menu.settings.clone();
    let rng = crate::battle::RngInit::State(w.rng.a, w.rng.b);
    let rng0 = (w.rng.a, w.rng.b);
    let clock0 = w.g.clock(w.frame);
    let shown = w.menu.shown.clone();
    let mut nw = match &b {
        Boot::NewGame { slot } => {
            saves.current = *slot;
            let story = crate::story::Story::new_game(data);
            let cfg = crate::field::FieldConfig { world: 0, area: 0, variant: 0, pos: Some((120, 120)), story: Some(story), rng, ..Default::default() };
            let mut nw = crate::field::create(Arc::clone(data), &cfg);
            // the game's state at the prologue hook has no objects but the player
            let pid = nw.players.first().copied();
            nw.ents.retain(|e| Some(e.id) == pid);
            if let Some(f) = nw.field.as_mut() {
                f.placed.clear();
            }
            nw.enemies.clear();
            nw.events.retain(|e| !matches!(e, crate::world::Event::Field(crate::field::FieldEvent::Run { .. })));
            nw.events.push(crate::world::Event::Field(crate::field::FieldEvent::Run {
                run: crate::fieldcontent::Callback::Event("s004".into()),
                cause: crate::field::Cause::Menu,
                slot: 0,
            }));
            nw.menu.overlay = shown;
            nw.menu.overlay_until = NEWGAME_OVERLAY;
            // the new field's build is ours only (the game's GameData reset draws nothing): the RNG as the title left
            // it (the whole run from the reset: [0, 0] until the prologue's first draw)
            nw.rng = crate::rng::Rng::new(rng0.0, rng0.1);
            // the frame counter 0x03000F94 runs on from the title (it counts since the reset)
            // (no lag: the menu step already counted this iteration, the new world's first step counts it again)
            nw.g.fc0 = clock0 - nw.frame as i64 - if w.opts.no_lag { 1 } else { 0 };
            nw.opts.no_lag = w.opts.no_lag;
            nw
        }
        Boot::Continue { slot, chosen } => {
            saves.current = *slot;
            let d = saves.slot(*slot as usize)?.clone();
            let mut story = d.story.clone();
            story.current = *chosen;
            let cfg = crate::field::FieldConfig {
                world: d.world,
                area: d.area,
                variant: d.variant,
                pos: Some((d.x, d.y)),
                story: Some(story),
                rng,
                ..Default::default()
            };
            let mut nw = crate::field::create(Arc::clone(data), &cfg);
            nw.menu.phase = Phase::Leave { t: 1 };
            nw.menu.tr = Transition { fade_out: 0, enter: 1, leave: CONTINUE_BLACK, fade_in: CONTINUE_FADE_IN };
            nw
        }
    };
    // the boot counter (0x03000F94) runs on through the GameData reset / slot read: the new play time's bias is the
    // counter now minus the new play time (whole run 7-3: the punch marker's blink phase, 764 boot + title frames)
    nw.story.lf_bias = w.story.play_frames as i64 + w.story.lf_bias - nw.story.play_frames as i64;
    crate::scene::note_events(&mut nw);
    saves.pending = None;
    nw.saves = saves;
    nw.menu.settings = settings;
    Some(nw)
}

/// a harness opening this screen from JSON: `{"kind": "title", "page": "title", "cursor_t": n, "sel": k}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    if v["kind"].as_str()? != "title" {
        return None;
    }
    let mut t = Title::new(w);
    if let Some(ct) = v["cursor_t"].as_u64() {
        for k in t.c.kids.iter_mut() {
            if let Widget::Cursor(c) = k {
                c.t = ct as u32;
            }
        }
    }
    if let Some(s) = v["sel"].as_i64() {
        t.c.sel = s as i32;
    }
    Some((super::Screen::Title(t), BOOT, false))
}
