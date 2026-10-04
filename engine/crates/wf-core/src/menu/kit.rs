//! The game's menu widget toolkit (container `0x0804920C` and its controls), ported class by class. A page is a
//! [`Container`]: children in creation order, the pressed button (`+0x108`), the selected control id (`+0x10C`) and
//! the drawn flag (`+0x110`; cleared = redraw the whole surface at the next update).
//!
//! Step (`0x080067F4`): every child's step (vt +0x14), then input: only the pressed button if there is one, else
//! the children in order until one consumes it (vt +0x10); then, when nothing consumed it, the page's own check
//! (vt +0x28). Children report events to the page (vt +0x20 `handle_event(src, code)`).

use super::text::{Canvas, Pen, TextData};
use crate::input::{A, B, DOWN, L, LEFT, R, RIGHT, START, UP};
use serde::{Deserialize, Serialize};
use wf_view::MenuFont;

/// the input a mode loop iteration read
#[derive(Clone, Copy, Debug, Default)]
pub struct Keys {
    pub held: u16,
    pub pressed: u16,
    pub released: u16,
}

/// button prompt table `0x086B77A4`: (key mask, w, h, content picture)
pub const BUTTONS: [(u16, u16, u16, &str); 8] = [
    (A, 16, 16, "ui/buttons/a.png"),
    (B, 16, 16, "ui/buttons/b.png"),
    (UP, 16, 16, "ui/buttons/up.png"),
    (DOWN, 16, 16, "ui/buttons/down.png"),
    (LEFT, 16, 16, "ui/buttons/left.png"),
    (RIGHT, 16, 16, "ui/buttons/right.png"),
    (L, 24, 16, "ui/buttons/l.png"),
    (R, 24, 16, "ui/buttons/r.png"),
];
pub const BTN_A: u8 = 0;
pub const BTN_B: u8 = 1;
pub const BTN_UP: u8 = 2;
pub const BTN_DOWN: u8 = 3;
pub const BTN_LEFT: u8 = 4;
pub const BTN_RIGHT: u8 = 5;
pub const BTN_L: u8 = 6;
pub const BTN_R: u8 = 7;

/// a button prompt (ctor `0x08013E64`, vt `0x08049880`): an animated icon that sends its event when its key is
/// released after a press (or repeatedly while held when `repeat`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Button {
    pub btn: u8,
    pub x: i32,
    pub y: i32,
    pub code: i32,
    /// +8: sends events; drawn with frame 2 (greyed) when not
    pub enabled: bool,
    /// +0xC: auto-repeat while held (16 frames, then halving down to 4)
    pub repeat: bool,
    /// +0x10
    pub visible: bool,
    pub countdown: i32,
    pub interval: i32,
}

impl Button {
    pub fn new(btn: u8, x: i32, y: i32, code: i32, enabled: bool, repeat: bool) -> Button {
        Button {
            btn,
            x,
            y,
            code,
            enabled,
            repeat,
            visible: true,
            countdown: 0,
            interval: 0,
        }
    }
}

/// a text label (ctor `0x08014124`, vt `0x0804A254`): string, colour, alignment, font; drawn with the shadowed
/// glyph routine (shadow colour 1)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Label {
    pub x: i32,
    pub y: i32,
    pub text: Vec<u16>,
    pub colour: u8,
    pub align: u8,
    pub font: Font,
}

/// a selectable text control (base `0x08005FBE` + `0x080060F8`, vt `0x0804B2DC`): id, neighbours up / down /
/// left / right (`-1` none), text drawn in the menu font, colour 6 when selected (else 2), event on A / START
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextCtl {
    pub id: i32,
    pub nav: [i32; 4],
    pub text: Vec<u16>,
    pub x: i32,
    pub y: i32,
    pub code: i32,
    pub align: u8,
}

/// one tab of a [`Tabs`] bar: string, value, measured width (small font)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tab {
    pub text: Vec<u16>,
    pub value: i32,
    pub w: i32,
}

/// the tab bar (ctor `0x08015D10`, vt `0x08049C68`): a selectable control with tabs in a rect, LEFT / RIGHT move
/// (event `code`), arrow buttons (children 1 and 2 of the page: `left` / `right` widget indices) when the tabs do
/// not fit
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tabs {
    pub id: i32,
    pub nav: [i32; 4],
    /// rect x0, y0, x1, y1 (+0x24..+0x30)
    pub rect: [i32; 4],
    pub code: i32,
    pub tabs: Vec<Tab>,
    /// +0x34 first shown, +0x38 end shown, +0x3C current, +0x40 marked (drawn selected)
    pub first: i32,
    pub end: i32,
    pub cur: i32,
    pub mark: i32,
    /// +0x48 x of the first shown tab
    pub x0: i32,
    /// +0x110 layout done
    pub laid_out: bool,
    /// widget indices of the arrow buttons (left code = `code`, right)
    pub left: usize,
    pub right: usize,
}

/// the blinking cursor (ctor `0x08016208`, vt `0x08049898`): drawn 16 px left / 12 px above the selected control's
/// cursor point while bit 3 of its counter is clear; redraws the page every 8 frames
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cursor {
    pub t: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Font {
    Small,
    Dialog,
    Digits,
    #[default]
    Menu,
    Bold,
}

impl Font {
    pub fn view(self) -> MenuFont {
        match self {
            Font::Small => MenuFont::Small,
            Font::Dialog => MenuFont::Dialog,
            Font::Digits => MenuFont::Digits,
            Font::Menu => MenuFont::Menu,
            Font::Bold => MenuFont::Bold,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Widget {
    Button(Button),
    Label(Label),
    Text(TextCtl),
    Tabs(Tabs),
    Cursor(Cursor),
    /// a removed child slot (keeps indices stable for pages that refer to their children by index)
    Gone,
    /// an option slider (ctor `0x08006880`, vt `0x0804BF3C`)
    Slider(Slider),
    /// a tab header box (ctor `0x08015C62`, vt `0x080493F4`; Journal's Goals / Completed Goals)
    TabBox(TabBox),
    /// a title-menu item (`0x080060A8` with vt `0x0804D82C`, `0x0804E358`): small font, outlined glyphs
    Item(ItemCtl),
}

/// a title-menu item (START / OPTIONS, Single Player / Multiplayer): a selectable control like [`TextCtl`] drawn
/// centred in the small font with the outlined glyph routine `0x0803F70C` (outline colour 1), colour 6 selected,
/// else 2 (0x75 when disabled, `0x0801E6FC`); cursor point (x − width / 2, y − 3) (`0x08006248`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemCtl {
    pub id: i32,
    pub nav: [i32; 4],
    pub text: Vec<u16>,
    pub x: i32,
    pub y: i32,
    pub code: i32,
    pub enabled: bool,
}

/// an option slider (`0x08006880`): id (selected = the page's selected control id), the bar at (x0, y), width
/// `w` (x1 - x0 - 2), value 0..=max; LEFT / RIGHT while selected change it by one (sound 39, redraw) and send
/// `code` (the page stores the value); draw `0x080068B8`
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Slider {
    pub id: i32,
    pub x0: i32,
    pub y: i32,
    pub w: i32,
    pub value: i32,
    pub max: i32,
    pub code: i32,
}

/// a tab header box (`0x08015C62`): box (x, y)-(x + width(small font) + 6, y + 10), text at (x + 3, y + 8) in the
/// small font, colour 2, drawn with the tab colours (`0x08015BB8`), `active` = the marked colours
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabBox {
    pub rect: [i32; 4],
    pub text: Vec<u16>,
    pub active: bool,
}

impl TabBox {
    pub fn new(t: &TextData, x: i32, y: i32, text: &[u16], active: bool) -> TabBox {
        let w = t.font(MenuFont::Small).measure(text);
        TabBox {
            rect: [x, y, x + w + 6, y + 10],
            text: text.to_vec(),
            active,
        }
    }
}

impl Slider {
    /// `0x080068B8`: fill = value * w / max; row 0 and 4 the top / bottom edges, rows 1..3 the bar
    pub fn draw(&self, cv: &mut Canvas) {
        let (x0, y, w) = (self.x0, self.y, self.w);
        let f = if self.max != 0 {
            self.value * w / self.max
        } else {
            0
        };
        let hl = |cv: &mut Canvas, x: i32, y: i32, n: i32, c: u8| {
            if n > 0 {
                cv.fill(x, y, x + n, y + 1, c);
            }
        };
        if f == 0 {
            hl(cv, x0 + 1, y, w, 1);
            for r in 1..4 {
                cv.pixel(x0, y + r, 0x73);
                hl(cv, x0 + 1, y + r, w, 0x70);
                cv.pixel(x0 + w + 1, y + r, 1);
            }
            hl(cv, x0 + 1, y + 4, w, 0x73);
        } else if f == w {
            hl(cv, x0 + 1, y, w, 0x0A);
            for r in 1..4 {
                cv.pixel(x0, y + r, 0x0A);
                hl(cv, x0 + 1, y + r, w, 3);
                cv.pixel(x0 + w + 1, y + r, 0x68);
            }
            hl(cv, x0 + 1, y + 4, w, 0x68);
        } else {
            hl(cv, x0 + 1, y, f, 0x0A);
            hl(cv, x0 + 1 + f, y, w - f, 1);
            for r in 1..4 {
                cv.pixel(x0, y + r, 0x0A);
                hl(cv, x0 + 1, y + r, f, 3);
                cv.pixel(x0 + f + 1, y + r, 0x68);
                hl(cv, x0 + f + 2, y + r, w - f - 1, 0x70);
                cv.pixel(x0 + w + 1, y + r, 1);
            }
            hl(cv, x0 + 1, y + 4, f, 0x68);
            hl(cv, x0 + 1 + f, y + 4, w - f, 0x73);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Container {
    pub kids: Vec<Widget>,
    /// +0x108: the pressed button (child index)
    pub focus: Option<usize>,
    /// +0x10C: the selected control's id
    pub sel: i32,
    /// +0x110: drawn (false = redraw at the next update)
    pub drawn: bool,
    /// drawn at least once (the timing model: a page's first redraw carries its enter)
    #[serde(default)]
    pub ever: bool,
    /// the page's own cursor point (page vt +0x1C overrides, e.g. the shop list); `Some(None)` = no cursor;
    /// None = ask the children
    #[serde(default)]
    pub point: Option<Option<(i32, i32)>>,
}

/// how an event's sender treats the page's answer
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvMode {
    /// the sender consumed the input whatever the page answers (a button released / repeating)
    Consume,
    /// the sender returns the page's answer (a control's A / START activation, `0x08006182`)
    Handler,
    /// the sender lets the input go on to the next children (the tab bar, `0x08016188`)
    Continue,
}

/// one step of the input dispatch
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// done: whether a child consumed the input
    Done(bool),
    /// child `src` sent `code`; the dispatch resumes at `next` unless it is consumed
    Event {
        src: usize,
        code: i32,
        mode: EvMode,
        next: usize,
    },
}

impl Container {
    pub fn add(&mut self, w: Widget) -> usize {
        self.kids.push(w);
        self.kids.len() - 1
    }
    /// `0x0800666C`: remove a child (the slot stays as `Gone`: the order of the others is kept, like the game's
    /// compaction; new children go after them)
    pub fn remove(&mut self, i: usize) {
        if let Some(k) = self.kids.get_mut(i) {
            *k = Widget::Gone;
        }
        if self.focus == Some(i) {
            self.focus = None;
        }
    }
    pub fn dirty(&mut self) {
        self.drawn = false;
    }

    /// the children's step (vt +0x14): the cursor's blink counter (a redraw every 8)
    pub fn step_children(&mut self) {
        let mut redraw = false;
        for w in self.kids.iter_mut() {
            if let Widget::Cursor(c) = w {
                c.t = c.t.wrapping_add(1);
                if c.t & 7 == 0 {
                    redraw = true;
                }
            }
        }
        if redraw {
            self.drawn = false;
        }
    }

    /// the input part of the container step from child `start`: the pressed button alone if there is one, else
    /// the children in order until one consumes the input or sends an event; sounds go to `sfx`
    pub fn input_from(&mut self, k: Keys, start: usize, sfx: &mut Vec<i32>) -> Input {
        if let Some(f) = self.focus {
            if start > 0 {
                return Input::Done(true);
            }
            return match self.input(f, k, sfx) {
                (_, Some((code, mode))) => Input::Event {
                    src: f,
                    code,
                    mode,
                    next: usize::MAX,
                },
                (c, None) => Input::Done(c),
            };
        }
        for i in start..self.kids.len() {
            match self.input(i, k, sfx) {
                (_, Some((code, mode))) => {
                    return Input::Event {
                        src: i,
                        code,
                        mode,
                        next: i + 1,
                    }
                }
                (true, None) => return Input::Done(true),
                _ => {}
            }
        }
        Input::Done(false)
    }

    /// one child's input (vt +0x10): (consumed, event)
    fn input(&mut self, i: usize, k: Keys, sfx: &mut Vec<i32>) -> (bool, Option<(i32, EvMode)>) {
        let sel = self.sel;
        let focus = self.focus;
        match &mut self.kids[i] {
            Widget::Button(b) => {
                let mask = BUTTONS[b.btn as usize].0;
                if focus == Some(i) {
                    // 0x08013F7C focused: release -> event; held with repeat -> events; always consumed
                    if k.released & mask != 0 {
                        self.focus = None;
                        if b.enabled {
                            let code = b.code;
                            self.drawn = false;
                            return (true, Some((code, EvMode::Consume)));
                        }
                    } else if b.repeat {
                        b.countdown -= 1;
                        if b.countdown == 0 {
                            if b.interval > 4 {
                                b.interval /= 2;
                            }
                            b.countdown = b.interval;
                            if b.enabled {
                                let code = b.code;
                                self.drawn = false;
                                return (true, Some((code, EvMode::Consume)));
                            }
                        }
                    }
                    (true, None)
                } else {
                    if k.pressed & mask != 0 && b.enabled {
                        b.countdown = 16;
                        b.interval = 16;
                        self.focus = Some(i);
                        self.drawn = false;
                    }
                    (false, None)
                }
            }
            Widget::Text(t) => {
                // 0x08005FD4: the selected control moves the selection; else its activate check (vt +0x18)
                if sel != t.id {
                    return (false, None);
                }
                if let Some(to) = nav_move(&t.nav, k.pressed) {
                    sfx.push(super::SFX_CURSOR);
                    self.sel = to;
                    self.drawn = false;
                    return (true, None);
                }
                // 0x08006182: A / START sends the code; consumed = the page's answer
                if k.pressed & (A | START) != 0 {
                    return (false, Some((t.code, EvMode::Handler)));
                }
                (false, None)
            }
            Widget::Item(t) => {
                if sel != t.id {
                    return (false, None);
                }
                if let Some(to) = nav_move(&t.nav, k.pressed) {
                    sfx.push(super::SFX_CURSOR);
                    self.sel = to;
                    self.drawn = false;
                    return (true, None);
                }
                if k.pressed & (A | START) != 0 {
                    return (false, Some((t.code, EvMode::Handler)));
                }
                (false, None)
            }
            Widget::Tabs(t) => {
                if sel != t.id {
                    return (false, None);
                }
                if let Some(to) = nav_move(&t.nav, k.pressed) {
                    sfx.push(super::SFX_CURSOR);
                    self.sel = to;
                    self.drawn = false;
                    return (true, None);
                }
                let left = t.left;
                // 0x08016188: only when the arrow buttons are hidden (all tabs fit); never consumes
                let arrows = matches!(self.kids.get(left), Some(Widget::Button(b)) if b.visible);
                let Widget::Tabs(t) = &mut self.kids[i] else {
                    unreachable!()
                };
                if arrows {
                    return (false, None);
                }
                if k.pressed & LEFT != 0 {
                    t.step_left();
                    let code = t.code;
                    self.drawn = false;
                    return (false, Some((code, EvMode::Continue)));
                } else if k.pressed & RIGHT != 0 {
                    t.step_right();
                    let code = t.code;
                    self.drawn = false;
                    return (false, Some((code, EvMode::Continue)));
                }
                (false, None)
            }
            Widget::Slider(sl) => {
                // 0x080069E6: LEFT (if above 0) else RIGHT (if below max)
                if sel != sl.id {
                    return (false, None);
                }
                if k.pressed & LEFT != 0 && sl.value > 0 {
                    sfx.push(super::SFX_CURSOR);
                    sl.value -= 1;
                    self.drawn = false;
                    return (true, Some((sl.code, EvMode::Consume)));
                }
                if k.pressed & RIGHT != 0 && sl.value < sl.max {
                    sfx.push(super::SFX_CURSOR);
                    sl.value += 1;
                    self.drawn = false;
                    return (true, Some((sl.code, EvMode::Consume)));
                }
                (false, None)
            }
            _ => (false, None),
        }
    }

    /// container vt +0x1C: the selected control's cursor point (first child answering)
    pub fn cursor_point(&self, t: &TextData) -> Option<(i32, i32)> {
        if let Some(p) = self.point {
            return p;
        }
        for w in &self.kids {
            match w {
                Widget::Text(c) if c.id == self.sel => {
                    let m = t.font(MenuFont::Menu).measure(&c.text);
                    let x = match c.align {
                        1 => c.x - (m >> 1),
                        2 => c.x - m,
                        _ => c.x,
                    };
                    return Some((x, c.y - 4));
                }
                Widget::Item(c) if c.id == self.sel => {
                    let m = t.font(MenuFont::Small).measure(&c.text);
                    return Some((c.x - (m >> 1), c.y - 3));
                }
                Widget::Tabs(tb) if tb.id == self.sel => {
                    let mut x = tb.x0;
                    for k in tb.first.max(0)..tb.cur {
                        x += tb.tabs.get(k as usize).map(|t| t.w).unwrap_or(0) + 10;
                    }
                    return Some((x, tb.rect[3] - 8));
                }
                _ => {}
            }
        }
        None
    }

    /// draw every child (vt +8) in order
    pub fn draw(&self, t: &TextData, cv: &mut Canvas) {
        for (i, w) in self.kids.iter().enumerate() {
            match w {
                Widget::Button(b) => {
                    if b.visible {
                        let frame = if !b.enabled {
                            2
                        } else if self.focus == Some(i) {
                            1
                        } else {
                            0
                        };
                        let (_, bw, bh, path) = BUTTONS[b.btn as usize];
                        cv.image(path, frame, bw, bh, b.x, b.y);
                    }
                }
                Widget::Label(l) => {
                    let mut p = Pen::new(l.font.view(), l.x, l.y, l.align, l.colour, Some(1));
                    cv.text(t, &mut p, &l.text);
                }
                Widget::Text(c) => {
                    let col = if self.sel == c.id { 6 } else { 2 };
                    let mut p = Pen::new(MenuFont::Menu, c.x, c.y, c.align, col, Some(1));
                    cv.text(t, &mut p, &c.text);
                }
                Widget::Tabs(tb) => tb.draw(t, cv, self.sel == tb.id),
                Widget::Cursor(c) => {
                    if c.t & 8 == 0 {
                        if let Some((x, y)) = self.cursor_point(t) {
                            cv.image("ui/images/cursor.png", 0, 16, 16, x - 16, y - 8);
                        }
                    }
                }
                Widget::Gone => {}
                Widget::Slider(sl) => sl.draw(cv),
                Widget::Item(c) => {
                    let col = if self.sel == c.id {
                        6
                    } else if c.enabled {
                        2
                    } else {
                        0x75
                    };
                    let w = t.font(MenuFont::Small).measure(&c.text);
                    cv.ops.push(wf_view::MenuOp::TextOutline {
                        font: MenuFont::Small,
                        x: c.x - ((w + 1) >> 1),
                        y: c.y,
                        colour: col,
                        outline: 1,
                        clip: [0, 0, 240, 160],
                        text: c.text.clone(),
                    });
                }
                Widget::TabBox(tb) => {
                    let c = TAB_COLOURS[tb.active as usize];
                    tab_box(cv, tb.rect[0], tb.rect[1], tb.rect[2], tb.rect[3], c);
                    let mut p =
                        Pen::new(MenuFont::Small, tb.rect[0] + 3, tb.rect[1] + 8, 0, 2, None);
                    cv.text(t, &mut p, &tb.text);
                }
            }
        }
    }
}

/// The container step with a page's event handler (`handle_event`, vt +0x20): children's steps, then the input
/// dispatch; each event goes to `h(page, world, src, code) -> (answer, action)`; an action ends the step.
/// Returns (consumed, action): when nothing consumed the input the page runs its own check (vt +0x28).
pub fn run_step<P>(
    p: &mut P,
    w: &mut crate::world::World,
    c: fn(&mut P) -> &mut Container,
    mut h: impl FnMut(&mut P, &mut crate::world::World, usize, i32) -> (bool, Option<super::Action>),
) -> (bool, Option<super::Action>) {
    let k = Keys {
        held: w.menu.held,
        pressed: w.menu.pressed,
        released: w.menu.released,
    };
    c(p).step_children();
    let mut start = 0;
    loop {
        let mut sfx = Vec::new();
        let r = c(p).input_from(k, start, &mut sfx);
        for s in sfx {
            w.sfx(s, 64);
        }
        match r {
            Input::Done(consumed) => return (consumed, None),
            Input::Event {
                src,
                code,
                mode,
                next,
            } => {
                let (answer, act) = h(p, w, src, code);
                if act.is_some() {
                    return (true, act);
                }
                match mode {
                    EvMode::Consume => return (true, None),
                    EvMode::Handler if answer => return (true, None),
                    _ => {}
                }
                if next == usize::MAX {
                    return (true, None);
                }
                start = next;
            }
        }
    }
}

/// `0x08005FD4`: UP / DOWN / LEFT / RIGHT (in that priority) to a neighbour id >= 0
fn nav_move(nav: &[i32; 4], pressed: u16) -> Option<i32> {
    for (bit, to) in [
        (UP, nav[0]),
        (DOWN, nav[1]),
        (LEFT, nav[2]),
        (RIGHT, nav[3]),
    ] {
        if pressed & bit != 0 {
            return if to >= 0 { Some(to) } else { None };
        }
    }
    None
}

/// tab colours `0x086B8080`: (corner, edge, fill, right / bottom) for a plain and the marked tab
const TAB_COLOURS: [[u8; 4]; 2] = [[0x44, 0x0A, 0x03, 0x21], [0x1E, 0x35, 0x06, 0x31]];

impl Tabs {
    pub fn new(id: i32, rect: [i32; 4], code: i32, left: usize, right: usize) -> Tabs {
        Tabs {
            id,
            nav: [-1; 4],
            rect,
            code,
            tabs: Vec::new(),
            first: 0,
            end: 0,
            cur: 0,
            mark: 0,
            x0: 0,
            laid_out: false,
            left,
            right,
        }
    }
    /// `0x08015FFC`
    pub fn push(&mut self, t: &TextData, text: &[u16], value: i32) {
        let w = t.font(MenuFont::Small).measure(text);
        self.tabs.push(Tab {
            text: text.to_vec(),
            value,
            w,
        });
        self.laid_out = false;
    }
    fn n(&self) -> i32 {
        self.tabs.len() as i32
    }
    fn wsum(&self, from: i32, to: i32) -> i32 {
        (from..to).map(|k| self.tabs[k as usize].w + 10).sum()
    }
    /// `0x08015FA8`: select the tab with this value
    pub fn select_value(&mut self, v: i32, arrows_visible: bool) {
        for k in 0..self.n() {
            if self.tabs[k as usize].value == v {
                self.cur = k;
                self.mark = k;
                if arrows_visible {
                    self.first = (k - 1).max(0);
                    self.laid_out = false;
                }
            }
        }
    }
    pub fn value(&self) -> i32 {
        self.tabs
            .get(self.cur as usize)
            .map(|t| t.value)
            .unwrap_or(0)
    }
    /// shown range and x of the first tab after a scroll (`0x08015E94` / `0x08015F1C` tail): end grows while the
    /// tabs fit in width - 40, tabs centred
    fn fit_from_first(&mut self) {
        let avail = self.rect[2] - self.rect[0] - 40;
        let mut acc = -4;
        self.end = self.first;
        while self.end < self.n() {
            let nacc = acc + self.tabs[self.end as usize].w + 10;
            if nacc > avail {
                break;
            }
            acc = nacc;
            self.end += 1;
        }
        self.x0 = ((self.rect[0] + self.rect[2]) >> 1) - (acc / 2);
    }
    /// `0x08015E94`
    pub fn step_left(&mut self) {
        if self.cur == 0 {
            return;
        }
        self.cur -= 1;
        // scrolling only matters when the arrows are shown (laid_out with arrows); the page refreshes the layout
        if self.cur < self.first {
            self.first = self.cur;
            self.fit_from_first();
        }
    }
    /// `0x08015F1C`
    pub fn step_right(&mut self) {
        if self.cur >= self.n() - 1 {
            return;
        }
        self.cur += 1;
        if self.cur >= self.end {
            self.end = self.cur + 1;
            // back off `first` while the tabs up to `end` fit
            let avail = self.rect[2] - self.rect[0] - 40;
            self.first = self.end;
            let mut acc = -4;
            while self.first > 0 {
                let nacc = acc + self.tabs[(self.first - 1) as usize].w + 10;
                if nacc > avail {
                    break;
                }
                acc = nacc;
                self.first -= 1;
            }
            self.x0 = ((self.rect[0] + self.rect[2]) >> 1) - (acc / 2);
        }
    }
    /// `0x08015DBE`: layout on the first draw: all tabs when they fit in the rect width (no arrows), else from
    /// `first` while they fit in width - 40 (arrows shown). Returns whether the arrows are shown.
    pub fn layout(&mut self) -> bool {
        self.laid_out = true;
        let total: i32 = -4 + self.wsum(0, self.n());
        if self.rect[2] - self.rect[0] >= total {
            self.first = 0;
            self.end = self.n();
            self.x0 = self.rect[0];
            false
        } else {
            self.fit_from_first();
            true
        }
    }
    /// `0x080160CA` (after the layout): the shown tabs from x0, 4 px apart
    pub fn draw(&self, t: &TextData, cv: &mut Canvas, _selected: bool) {
        let mut x = self.x0;
        let (y0, y1) = (self.rect[1] + 3, self.rect[3] - 3);
        let ty = self.rect[3] - 5;
        for k in self.first.max(0)..self.end.min(self.n()) {
            let tab = &self.tabs[k as usize];
            let x1 = x + tab.w + 6;
            let c = TAB_COLOURS[(self.mark == k) as usize];
            tab_box(cv, x, y0, x1, y1, c);
            let mut p = Pen::new(MenuFont::Small, x + 3, ty, 0, 2, None);
            cv.text(t, &mut p, &tab.text);
            x = x1 + 4;
        }
    }
}

/// `0x08015BB8`: a tab's box: top row c0 at x0+1 then c1; rows: left c0 (first) / c1, fill c2, right c3; bottom c3
fn tab_box(cv: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
    let w = x1 - x0;
    let h = y1 - y0;
    cv.pixel(x0 + 1, y0, c[0]);
    cv.fill(x0 + 2, y0, x0 + 2 + (w - 3), y0 + 1, c[1]);
    let mut left = c[0];
    for r in 1..=(h - 2).max(0) {
        let y = y0 + r;
        cv.pixel(x0, y, left);
        cv.fill(x0 + 1, y, x0 + 1 + (w - 2), y + 1, c[2]);
        cv.pixel(x0 + w - 1, y, c[3]);
        left = c[1];
    }
    let y = y0 + (h - 2).max(0) + 1;
    cv.fill(x0 + 1, y, x0 + 1 + (w - 2), y + 1, c[3]);
}

/// The pause pages' shared frame (`0x08006718`): background, the main panel, the page title in the bold font
/// centred at (120, 16), colour 5 (shadow 1).
pub fn page_frame(t: &TextData, cv: &mut Canvas, title: usize) {
    cv.background(0, 0, 240, 160);
    cv.panel(8, 24, 232, 152);
    let mut p = Pen::new(MenuFont::Bold, 120, 16, 1, 5, Some(1));
    cv.text(t, &mut p, t.s(title));
}

/// the pause pages in L / R order with their title string ids
pub const PAGES: [usize; 5] = [
    28, /* Status */
    33, /* Inventory */
    36, /* Equip */
    39, /* Options: "Pause" */
    30, /* Journal */
];

/// page header children (`0x08014DC2` and the other pages' enter): L / R prompts at (8, 8) / (208, 8) (codes 6 / 7),
/// the previous page's name at (34, 20) and the next one's right-aligned at (206, 20), menu font, colour 2
pub fn page_header(c: &mut Container, t: &TextData, prev: usize, next: usize) {
    c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
    c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
    c.add(Widget::Label(Label {
        x: 0x22,
        y: 0x14,
        text: t.s(prev).to_vec(),
        colour: 2,
        align: 0,
        font: Font::Menu,
    }));
    c.add(Widget::Label(Label {
        x: 0xCE,
        y: 0x14,
        text: t.s(next).to_vec(),
        colour: 2,
        align: 2,
        font: Font::Menu,
    }));
}

/// `0x08014C5C(page, a_str, b_str)`: the A / B prompts at the bottom: A icon at (12, 0x84) + its label at (0x1C, 0x90),
/// B icon after it (x = width(a label) + 0x20) + label 16 px right; menu font colour 5. Returns the child indices
/// (a button, b button, a label, b label).
pub fn ab_prompts(
    c: &mut Container,
    t: &TextData,
    a_str: usize,
    b_str: usize,
) -> [Option<usize>; 4] {
    let mut out = [None; 4];
    let mut x = 0xC;
    if a_str != 0 {
        out[0] = Some(c.add(Widget::Button(Button::new(
            BTN_A, 0xC, 0x84, 0, true, false,
        ))));
        out[2] = Some(c.add(Widget::Label(Label {
            x: 0x1C,
            y: 0x90,
            text: t.s(a_str).to_vec(),
            colour: 5,
            align: 0,
            font: Font::Menu,
        })));
        x = t.font(MenuFont::Menu).measure(t.s(a_str)) + 0x20;
    }
    if b_str != 0 {
        out[1] = Some(c.add(Widget::Button(Button::new(BTN_B, x, 0x84, 1, true, false))));
        out[3] = Some(c.add(Widget::Label(Label {
            x: x + 0x10,
            y: 0x90,
            text: t.s(b_str).to_vec(),
            colour: 5,
            align: 0,
            font: Font::Menu,
        })));
    }
    out
}

/// a page that is not implemented yet: frame, header, B closes the menu, L / R switch pages
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StubPage {
    pub page: u8,
    pub c: Container,
}

impl StubPage {
    pub fn new(w: &crate::world::World, page: u8) -> StubPage {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let mut c = Container::default();
        let n = PAGES.len();
        page_header(
            &mut c,
            t,
            PAGES[(page as usize + n - 1) % n],
            PAGES[(page as usize + 1) % n],
        );
        ab_prompts(&mut c, t, 0, 26);
        StubPage { page, c }
    }
    pub fn step_stub(&mut self, w: &mut crate::world::World) -> super::Action {
        let (consumed, act) = run_step(
            self,
            w,
            |p| &mut p.c,
            |p, w, _src, code| match code {
                1 => {
                    w.sfx(super::SFX_MENU, 64);
                    (true, Some(super::Action::Close))
                }
                6 | 7 => {
                    w.sfx(super::SFX_MENU, 64);
                    (
                        true,
                        Some(super::Action::Replace(super::page_screen(
                            w,
                            p.page,
                            code == 7,
                        ))),
                    )
                }
                _ => (false, None),
            },
        );
        if let Some(a) = act {
            return a;
        }
        if !consumed && w.menu.pressed & START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return super::Action::Close;
        }
        super::Action::None
    }
    pub fn draw_stub(&self, w: &crate::world::World, d: &super::MenuData) -> wf_view::MenuView {
        let _ = w;
        let mut cv = Canvas::new();
        page_frame(&d.text, &mut cv, PAGES[self.page as usize]);
        self.c.draw(&d.text, &mut cv);
        super::surface_view(cv)
    }
}

impl super::Page for StubPage {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn step(&mut self, w: &mut crate::world::World) -> super::Action {
        self.step_stub(w)
    }
    fn view(&self, w: &crate::world::World, d: &super::MenuData) -> wf_view::MenuView {
        self.draw_stub(w, d)
    }
}
