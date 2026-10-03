//! The mini-games (re/buu/menu/NOTES_minigames.md). All three are modes of the "field drawn, not updated" class
//! (base vt `0x0804A920` methods: map update + object *draw* each frame, no object update) that add their own
//! objects to the object world, so they show over the frozen field: [`MenuView::overlay`].
//!
//! * **Power struggle** (`run_power_struggle` 0xC6, mode vt `0x0804B5D8`, gauge object `0x0803018C` vt
//!   `0x0804D5BC`): every frame any of A B RIGHT LEFT UP DOWN *pressed* adds 10 (max 85: done), else
//!   `v = v * 31 >> 5`; the peak marker follows the highest value. Done -> a 30-frame wait mode (`0x08049978`),
//!   then the mode ends. Pushes nothing.
//! * **Punch** (`run_punch_minigame(mode)` 0xC7, mode vt `0x0804D8F0`, gauge `0x080309F4` vt `0x0804D20C` + the
//!   label "Press A Button To Punch" `0x08030860`): a marker sweeps the full gauge at `mode * 0x290 >> 8` (8.8 px
//!   per frame, bouncing at 0 and 85); A *pressed* stops it, then it blinks (shown while bit 2 of the mode loop's
//!   frame counter is set, else hidden = -1) for the 30-frame wait; the native pushes the marker value at the end
//!   (-1 when the last frame was a hidden one).
//! * **Watch This!** (`watch_this_game(level)` 0xA7): see [`Watch`].
//!
//! Gauge drawing `0x08030274`: 3 OBJs 32x8 (8bpp, priority 1) at the object's (x, y) with the bar tiles (rebuilt
//! when the value changes: content `ui/menu/mg_bar.png` frame = value), the 8x16 marker OBJ at (x + peak + 1, y - 3)
//! when peak != -1 (OAM entry first). The OBJ attributes reach the screen one frame late (shadow OAM copied at
//! the next VBlank), the tiles at once.

use super::kit::Container;
use super::text::{Canvas, Pen};
use super::{Action, MenuData, Page, Screen, Transition};
use crate::input::{A, B, DOWN, LEFT, RIGHT, UP};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuLayer, MenuOp, MenuView};

/// the gauge's full value (85 steps)
pub const BAR_MAX: i32 = 85;
/// the wait after a finished game (mode `0x08049978` with 30)
pub const WAIT: i32 = 30;
/// screens over the field: no fades, no black frames; the world gets the frame the game ends in
pub const OVERLAY: Transition = Transition { fade_out: 0, enter: 0, leave: 1, fade_in: 0 };

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Power,
    Punch,
    Watch,
}

/// the gauge object (`0x080301F8` base: +0x10 x, +0x14 y, +0x18 drawn value, +0x1C peak, +0x20 value)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Gauge {
    pub x: i32,
    pub y: i32,
    pub value: i32,
    /// the marker's value (-1 = hidden)
    pub peak: i32,
    pub done: bool,
    /// punch: position (8.8) and speed
    pub pos: i32,
    pub vel: i32,
}

/// what the OAM held for the gauge at the last draw: (peak), None = no OBJs
pub type Oam = Option<i32>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Minigame {
    pub c: Container,
    pub kind: Kind,
    pub g: Gauge,
    /// frames left of the wait after the game (0 = playing)
    pub wait: i32,
    /// OAM of the previous draw (on screen now) / of the last draw
    pub oam_shown: Oam,
    pub oam_last: Oam,
    /// the mode loop's frame counter (`0x03000F94` since boot; the punch marker's blink): the remake has no boot
    /// counter, it starts from the play time (a harness passes the capture's)
    pub counter: u32,
    pub watch: Option<Watch>,
    /// Watch This!: the construction (VRAM eviction, box decode) makes the first iteration 2 frames long
    pub enter_lag: u8,
    /// this iteration was one of the punch wait mode's (the object world updates in it)
    #[serde(default)]
    pub waited: bool,
}

impl Minigame {
    fn base(kind: Kind, g: Gauge, w: &World) -> Minigame {
        Minigame { c: Container::default(), kind, g, wait: 0, oam_shown: None, oam_last: None, counter: (w.story.play_frames as i64 + w.story.lf_bias) as u32, watch: None, enter_lag: 0, waited: false }
    }
    /// `0x0803018C`: the gauge at (72, 144), empty, peak 0
    pub fn power(w: &World) -> Minigame {
        Minigame::base(Kind::Power, Gauge { x: 0x48, y: 0x90, value: 0, peak: 0, ..Default::default() }, w)
    }
    /// `0x080309F4(mode)`: the full gauge at (72, 128), no marker until the first update, the marker sweeping at `mode * 0x290 >> 8`
    pub fn punch(w: &World, mode: i32) -> Minigame {
        Minigame::base(Kind::Punch, Gauge { x: 0x48, y: 0x80, value: BAR_MAX, peak: -1, vel: (mode.wrapping_mul(0x290)) >> 8, ..Default::default() }, w)
    }

    /// the gauge's update (vt +8)
    fn gauge_update(&mut self, pressed: u16) {
        let g = &mut self.g;
        match self.kind {
            Kind::Power => {
                // 0x080301B8
                if pressed & (A | B | RIGHT | LEFT | UP | DOWN) != 0 {
                    g.value += 10;
                    if g.value >= BAR_MAX {
                        g.value = BAR_MAX;
                        g.done = true;
                    }
                    if g.peak < g.value {
                        g.peak = g.value;
                    }
                } else {
                    g.value = (g.value * 31) >> 5;
                }
            }
            Kind::Punch => {
                // 0x08030A4C
                if g.done {
                    return;
                }
                if pressed & A != 0 {
                    g.done = true;
                    return;
                }
                g.pos += g.vel;
                if g.pos < 0 {
                    g.pos = -g.pos;
                    g.vel = -g.vel;
                } else if g.pos > BAR_MAX << 8 {
                    g.pos = (BAR_MAX << 9) - g.pos;
                    g.vel = -g.vel;
                }
                g.peak = g.pos >> 8;
            }
            Kind::Watch => {}
        }
    }

    /// the mode ends in this iteration (no update after it): the screen shows the OAM of the last update
    fn finish(&mut self, w: &mut World, r: i32) -> Action {
        if let Some(wt) = self.watch.as_mut() {
            wt.oam_shown = wt.oam_last;
        } else {
            self.oam_shown = self.oam_last;
        }
        w.menu.shown = Some(Box::new(Screen::Minigame(self.clone())));
        Action::Pop(r)
    }

    /// the object's pre-draw (vt +0xC, punch `0x08030A2C`): after the press the marker blinks with the frame counter
    fn pre_draw(&mut self) {
        if self.kind == Kind::Punch && self.g.done {
            self.g.peak = if self.counter & 4 != 0 { self.g.pos >> 8 } else { -1 };
        }
    }
}

impl Page for Minigame {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn enter_lag(&self) -> u8 {
        self.enter_lag
    }
    fn update(&mut self, w: &World) {
        if let Some(wt) = self.watch.as_mut() {
            wt.update(w.menu.pressed);
        }
    }
    fn before_draw(&mut self, w: &World) {
        if let Some(wt) = self.watch.as_mut() {
            wt.before_draw();
            return;
        }
        let _ = w;
        self.pre_draw();
        self.oam_shown = self.oam_last;
        self.oam_last = Some(self.g.peak);
    }
    fn step(&mut self, w: &mut World) -> Action {
        // every iteration redraws (the object world is drawn every frame)
        self.c.drawn = false;
        self.counter = self.counter.wrapping_add(1);
        if let Some(wt) = &self.watch {
            // the mode's step returns the verdict: non-zero ends the mode
            let v = wt.verdict;
            return if v != 0 { self.finish(w, v) } else { Action::None };
        }
        // a wait iteration: not the press / the step that starts a wait
        self.waited = self.wait > 0 && self.wait <= WAIT;
        if self.wait > 0 {
            // the wait mode 0x08049978: 30 iterations, then the mode's step returns marker + 1 (the native pushes
            // code - 1); 0 (the blinking marker hidden, peak -1) does not end the mode: the step runs again and
            // starts another wait (7-3: four waits, 123 frames)
            self.wait -= 1;
            if self.wait == 0 {
                let r = match self.kind {
                    Kind::Punch => self.g.peak + 1,
                    _ => 0,
                };
                if self.kind == Kind::Punch && r == 0 {
                    self.wait = WAIT + 1;
                    return Action::None;
                }
                return self.finish(w, r);
            }
            return Action::None;
        }
        let p = w.menu.pressed;
        self.gauge_update(p);
        if self.g.done {
            self.wait = WAIT;
        }
        Action::None
    }
    fn view(&self, w: &World, d: &MenuData) -> MenuView {
        if let Some(wt) = &self.watch {
            return wt.view(w, d);
        }
        let mut v = MenuView { overlay: true, ..Default::default() };
        let Some(peak) = self.oam_shown else { return v };
        let g = &self.g;
        // the bar: 3 OBJs 32x8 (tiles of this frame's draw)
        let mut cv = Canvas::new();
        cv.image("ui/menu/mg_bar.png", g.value.clamp(0, BAR_MAX) as u16, 96, 8, 0, 0);
        v.layers.push(MenuLayer { w: 96, h: 8, x: g.x, y: g.y, priority: 1, ops: cv.ops });
        // the marker OBJ comes first in OAM: on top
        if peak != -1 {
            let mut cv = Canvas::new();
            cv.image("ui/menu/mg_marker.png", 0, 8, 16, 0, 0);
            v.layers.push(MenuLayer { w: 8, h: 16, x: g.x + peak + 1, y: g.y - 3, priority: 1, ops: cv.ops });
        }
        if self.kind == Kind::Punch {
            label(d, 152, 0x78, 0x94, &mut v);
        }
        v
    }
}

/// `0x08030860`: a text label OBJ: a surface of `w` = (width + 33) rounded down to 32 px by 16, the dialog font
/// centred at (w / 2, 12), colour 2 with the outline glyphs (`0x0803F70C`, outline colour 1), shown as 32x16 OBJs
/// at (cx - w / 2, cy - 8), priority 0
pub fn label(d: &MenuData, s: usize, cx: i32, cy: i32, v: &mut MenuView) {
    let t = &d.text;
    let text = t.s(s);
    let w = ((t.font(MenuFont::Dialog).measure(text) + 0x21) >> 5) << 5;
    let mut cv = Canvas::new();
    let m = t.font(MenuFont::Dialog).measure_n(text, text.len());
    let pen = Pen::new(MenuFont::Dialog, w / 2, 12, 1, 2, Some(1));
    let x = pen.x - ((m + 1) >> 1);
    cv.ops.push(MenuOp::TextOutline { font: MenuFont::Dialog, x, y: 12, colour: 2, outline: 1, clip: [0, 0, w, 16], text: text.to_vec() });
    v.layers.push(MenuLayer { w: w as u16, h: 16, x: cx - w / 2, y: cy - 8, priority: 0, ops: cv.ops });
}

/// what the 160x64 surface holds (`+0x34`, the box picture `0x08071758` + text + button icons)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Surf {
    /// string id of the bold outlined line at (80, 28), 0 = none
    pub text: u16,
    /// button icon per slot (x 20 + 24 i, y 44): 0 A, 1 B, 2 RIGHT, 3 LEFT, 4 UP, 5 DOWN; -1 none
    pub icons: [i8; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WState {
    /// `0x0802812C`: the box zooms in (+0x14 = 1..8)
    Open,
    /// `0x08028034`: the icons appear one by one, then disappear, every `period` frames
    Show,
    /// `0x08027FB8`: the player's five presses
    Input,
    /// `0x08027F74`: 16 frames
    Wait,
    /// `0x08027EEE`: "Fusion-Ha!" blinking (32 frames)
    Blink,
    /// `0x08027EB0`: the box zooms out, then the verdict
    Close,
}

/// Watch This! (`watch_this_game(level)` 0xA7: mode vt `0x0804A3E8`, game object `0x08028144` vt `0x0804C9C8`).
/// The construction draws 5 x rand(6) (the buttons to repeat); the state runs in the mode's *update* (before the
/// VBlank wait), so it reads the input of the previous iteration. Inputs: any of A B RIGHT LEFT UP DOWN pressed;
/// the button = the lowest pressed bit (SELECT / START count as A / B: `b >= 2 ? b - 2`). Win = the sequence or
/// the fixed code UP B LEFT DOWN A (`0x0874EA60`); the mode ends with 2 (win) / 1, the native pushes code - 1.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Watch {
    pub level: i32,
    /// +0x20: frames per icon = (8 - level) * 3
    pub period: i32,
    pub seq: [u8; 5],
    pub input: [u8; 5],
    pub state: WState,
    /// +0x14 zoom step (0 = not drawn), +0x18 counter, +0x1C countdown, +0x10 verdict
    pub k: i32,
    pub i: i32,
    pub cd: i32,
    pub verdict: i32,
    /// the drawing surface and what the OBJ tiles hold (uploaded copies; `None` = the plain box)
    pub surf: Surf,
    pub vram: Surf,
    /// OAM: the zoom step of the previous / last draw
    pub oam_shown: i32,
    pub oam_last: i32,
}

/// the code that always wins (`0x0874EA60`): UP B LEFT DOWN A
pub const WATCH_CODE: [u8; 5] = [4, 1, 3, 5, 0];
/// the icons' pictures (`0x0874EA68`: the button prompts' first frames)
const ICONS: [&str; 6] = ["ui/buttons/a.png", "ui/buttons/b.png", "ui/buttons/right.png", "ui/buttons/left.png", "ui/buttons/up.png", "ui/buttons/down.png"];

impl Watch {
    /// `0x08028144`: the box surface (uploaded), five rand(6), "Watch This!" drawn (not uploaded yet)
    pub fn new(level: i32, seq: [u8; 5]) -> Watch {
        let none = Surf { text: 0, icons: [-1; 5] };
        Watch {
            level,
            period: (8 - level) * 3,
            seq,
            input: [0; 5],
            state: WState::Open,
            k: 0,
            i: 0,
            cd: 0,
            verdict: 0,
            surf: Surf { text: 148, icons: [-1; 5] },
            vram: none,
            oam_shown: 0,
            oam_last: 0,
        }
    }
    /// the five rand(6) of the construction, in the game's order
    pub fn roll(rng: &mut crate::rng::Rng) -> [u8; 5] {
        let mut s = [0u8; 5];
        for x in s.iter_mut() {
            *x = rng.rand(6) as u8;
        }
        s
    }
    fn upload(&mut self) {
        self.vram = self.surf.clone();
    }
    fn reset_box(&mut self, text: u16) {
        self.surf = Surf { text, icons: [-1; 5] };
    }
    /// the game object's update (the state function at +0x30), once per mode iteration
    pub fn update(&mut self, pressed: u16) {
        match self.state {
            WState::Open => {
                self.k += 1;
                if self.k >= 8 {
                    self.state = WState::Show;
                    self.i = 0;
                    self.cd = 1;
                }
            }
            WState::Show => {
                self.cd -= 1;
                if self.cd != 0 {
                    return;
                }
                if self.i >= 10 {
                    self.reset_box(149);
                    self.upload();
                    self.state = WState::Input;
                    self.i = 0;
                    return;
                }
                if self.i >= 5 {
                    // 0x0803F8E2: the box picture copied back over the icon (index 0 skipped)
                    self.surf.icons[(self.i - 5) as usize] = -1;
                } else {
                    self.surf.icons[self.i as usize] = self.seq[self.i as usize] as i8;
                }
                self.upload();
                self.i += 1;
                self.cd = self.period;
            }
            WState::Input => {
                if pressed & (A | B | RIGHT | LEFT | UP | DOWN) == 0 {
                    return;
                }
                // 0x08040322: the lowest pressed bit
                let mut b = pressed.trailing_zeros() as i32;
                if b >= 2 {
                    b -= 2;
                }
                let k = self.i as usize;
                self.input[k] = b as u8;
                self.surf.icons[k] = b as i8;
                self.i += 1;
                self.upload();
                if self.i == 5 {
                    self.state = WState::Wait;
                    self.i = 0;
                }
            }
            WState::Wait => {
                self.i += 1;
                if self.i >= 16 {
                    self.reset_box(150);
                    self.upload();
                    self.state = WState::Blink;
                    self.i = 0;
                }
            }
            WState::Blink => {
                self.i += 1;
                if self.i >= 32 {
                    self.state = WState::Close;
                }
                if self.i & 7 == 0 {
                    if self.i & 8 != 0 {
                        self.vram = Surf { text: 0, icons: [-1; 5] };
                    } else {
                        self.upload();
                    }
                }
            }
            WState::Close => {
                self.k -= 1;
                if self.k == 0 {
                    self.verdict = if self.input == WATCH_CODE || self.input == self.seq { 2 } else { 1 };
                }
            }
        }
    }
    fn before_draw(&mut self) {
        self.oam_shown = self.oam_last;
        self.oam_last = self.k;
    }
    /// `0x08028224`: three affine OBJs (64x64, 64x64, 32x64; tiles of the 160x64 surface) at y 48, PA = PD =
    /// 2048 / k, x = (40k - 88(k - 8)) / 8, (104k - 88(k - 8)) / 8, (168k - 104(k - 8)) / 8
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let mut v = MenuView { overlay: true, ..Default::default() };
        let k = self.oam_shown;
        if k <= 0 {
            return v;
        }
        let t = &d.text;
        let mut cv = Canvas::new();
        cv.image("ui/images/dialog_box_0.png", 0, 160, 64, 0, 0);
        if self.vram.text != 0 {
            let s = t.s(self.vram.text as usize);
            let m = t.font(MenuFont::Bold).measure_n(s, s.len());
            cv.ops.push(MenuOp::TextOutline { font: MenuFont::Bold, x: 80 - ((m + 1) >> 1), y: 28, colour: 2, outline: 1, clip: [0, 0, 160, 64], text: s.to_vec() });
        }
        for (i, &ic) in self.vram.icons.iter().enumerate() {
            if ic >= 0 {
                cv.image(ICONS[ic as usize], 0, 16, 16, 20 + 24 * i as i32, 44);
            }
        }
        let tr = |n: i32| if n < 0 { -((-n) / 8) } else { n / 8 };
        let x0 = tr(40 * k - 88 * (k - 8));
        let x1 = tr(104 * k - 88 * (k - 8));
        let x2 = tr(168 * k - 104 * (k - 8));
        cv.ops.push(MenuOp::AffineObjs { pieces: vec![[0, 0, 64, 64, x0, 48], [64, 0, 64, 64, x1, 48], [128, 0, 32, 64, x2, 48]], pa: 2048 / k });
        v.layers.push(MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 0, ops: cv.ops });
        v
    }
}

/// `watch_this_game(level)` (0xA7): draws the five rand(6) from `World.rng` now; the native pushes
/// `World.menu.result.code - 1` (1 = repeated, 0 = failed)
pub fn open_watch(w: &mut World, level: i32) {
    let held = w.input[0].held;
    let seq = Watch::roll(&mut w.rng);
    let mut s = Minigame::base(Kind::Watch, Gauge::default(), w);
    s.watch = Some(Watch::new(level, seq));
    s.enter_lag = 1;
    open_overlay(w, s, held);
}

/// the power struggle's mode keeps the field's object world updating (mode vt `0x0804B5D8` entry 3 = `0x08007018`,
/// which calls the world update `0x080115B0`; story segment sb_01_05b: the Z-Sword, s464 - the actors' animations
/// and Gohan's frame-range pose keep running under the gauge)
/// The punch machine's 30-iteration wait modes after the press (`0x08049978`, step `0x08006BA0`) update the object
/// world too (7-3: the NPCs' idle ticks run from the iteration after the press until the mode ends)
pub fn world_runs(w: &World) -> bool {
    let power = |s: Option<&Screen>| matches!(s, Some(Screen::Minigame(m)) if m.kind == Kind::Power || (m.kind == Kind::Punch && m.waited));
    // the closing frame too (the stack is empty, the screen still shows)
    power(w.menu.stack.last()) || (w.menu.stack.is_empty() && power(w.menu.shown.as_deref()))
}

/// `run_power_struggle` (0xC6): the screen over the field; nothing to push when it closes
pub fn open_power(w: &mut World) {
    let held = w.input[0].held;
    let s = Minigame::power(w);
    open_overlay(w, s, held);
}

/// `run_punch_minigame(mode)` (0xC7): the native pushes `World.menu.result.code - 1` (the marker, -1 if hidden)
pub fn open_punch(w: &mut World, mode: i32) {
    let held = w.input[0].held;
    let s = Minigame::punch(w, mode);
    open_overlay(w, s, held);
}

fn open_overlay(w: &mut World, s: Minigame, held: u16) {
    super::open(w, Screen::Minigame(s), OVERLAY, true);
    // the game's first update draws at once (in the native's frame); the field read this frame's input
    w.menu.phase = super::Phase::Run;
    w.menu.held = held;
    if let Some(Screen::Minigame(mut m)) = w.menu.stack.pop() {
        // the first iteration's update runs in the native's frame (Watch This: its state; then the draw)
        m.update(w);
        m.before_draw(w);
        m.c.drawn = true;
        w.menu.shown = Some(Box::new(Screen::Minigame(m.clone())));
        w.menu.lag = m.enter_lag;
        w.menu.stack.push(Screen::Minigame(m));
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "power_struggle"}`,
/// `{"kind": "punch", "mode": n}`, `{"kind": "watch_this", "level": n}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    let m = match v["kind"].as_str()? {
        "power_struggle" => Minigame::power(w),
        "punch" => {
            let mut m = Minigame::punch(w, v["mode"].as_i64().unwrap_or(0) as i32);
            if let Some(c) = v["counter"].as_u64() {
                m.counter = c as u32;
            }
            m
        }
        "watch_this" => {
            // the sequence: the capture's (the remake's harness world has another RNG state); else rolled from a
            // copy of the world's RNG
            let mut seq = Watch::roll(&mut w.rng.clone());
            if let Some(a) = v["seq"].as_array() {
                for (k, x) in a.iter().take(5).enumerate() {
                    seq[k] = x.as_u64().unwrap_or(0) as u8;
                }
            }
            let mut m = Minigame::base(Kind::Watch, Gauge::default(), w);
            m.watch = Some(Watch::new(v["level"].as_i64().unwrap_or(0) as i32, seq));
            m.enter_lag = 1;
            m
        }
        _ => return None,
    };
    // the harness opens before the frame's step: the first draw happens in that frame (Enter -> Run), the first
    // iteration in the next one
    Some((Screen::Minigame(m), Transition { enter: 1, ..OVERLAY }, true))
}
