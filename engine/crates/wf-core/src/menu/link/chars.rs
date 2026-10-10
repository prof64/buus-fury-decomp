//! The Dojo's character pick, both sides (mode vt `0x0804CAA8`: enter `0x08022B6C`, update `0x0802361A`, step
//! `0x0802362E`, events `0x080232A6`, redraw `0x0802309C`, the figures' row `0x08022DF4`, BG0 / figures reset
//! `0x08022AF6`).
//!
//! The page frame with the heading "Dojo"; "Battle with your friends in the Dojo!" (104) at (120, 38) and "Choose
//! your character!" (105) at (120, 130), centred, colour 0x75; the A / B prompts ("Select" / "Cancel"); LEFT / RIGHT
//! prompts at (11, 86) / (214, 86) (codes 4 / 5, auto-repeat), LEFT enabled when the selection is above 0, RIGHT
//! below the last (Balanced: below 31 - the roster has 32). The redraw writes the selected character's name centred
//! at (120, 48) (Balanced: strings 116 + index; In-Game: 158 + the character id) and a 48x56 BG0 picture at (96, 72):
//! the panel (10, 2)-(37, 44), the frame around the selected figure.
//!
//! The figures: up to five character objects (class `0x0800EB60`, the idle controller `0x03001B0C`: made with tick
//! = rand(255)), slots 0..4 for the selection - 2 .. + 2, at x = 0x23 + 0x2A slot - foot w / 2, y = 0x6A - foot h / 2,
//! facing down. The controller animates whatever animation the object has: tick + 1, a 2-cel animation shows cel 1
//! while (tick & 0xFF) > 0xF0, else cel = tick >> 3 (both back to 0 past the last). The selected one has the walk
//! (vt +0x28(1): only the animation pointer changes, the tick runs on), the others idle (0). The row changes in the
//! redraw, by the page's state (+0x140): 1 = made anew (the enter, a cancelled pick), 2 = RIGHT (slot 0 goes, the
//! rest move left, the next one made in slot 4), 3 = LEFT (the mirror); every redraw sets the selected one's walk.
//! RIGHT / LEFT (sound 39) move the selection by one and set the state; A (sound 40): the figures and the frame go,
//! the pick's wait box runs (its end: the fight); B (sound 40): back to the setup page (host) / Select Saved Game
//! (guest).

use super::super::kit::{Button, Container, Font, Label, Widget, BTN_A, BTN_B, BTN_LEFT, BTN_RIGHT};
use super::super::text::{Canvas, Pen, TextData};
use crate::rng::Rng;
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuLayer, MenuObj};

/// the Balanced roster (records `0x0872BFCC`, 28 bytes: the sprite id, HP 15000, EP 100, STR, POW, END, SPD; names
/// strings 116..147): the ROM sprite ids, in order (Goku .. SS Gogeta, Hercule, Videl, Pikkon, Piccolo, Broly, Majin
/// Buu, Super Buu 1..3, Kid Buu, Uub)
pub const ROSTER: [i32; 32] = [
    0x73, 0x76, 0x78, 0x6B, 0x71, 0x6E, 0x6F, 0x70, 0x72, 0x10D, 0x10E, 0x117, 0x11A, 0x7B, 0x7C, 0x7E, 0x81, 0x82,
    0x11C, 0x11D, 0x66, 0x8A, 0x11F, 0xE0, 0xDE, 0x26, 0x2B, 0x31, 0x33, 0x34, 0x2F, 0x111,
];

const TICK_IMPORT: i32 = 1;

/// ROM sprite ids are the remake's sprite index + 9
const SPRITE_BIAS: i32 = 9;
const DIR: u8 = 0;

/// a figure of the row (a character object)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CFig {
    /// the roster / party index it shows
    #[serde(default)]
    pub idx: i32,
    /// the remake's sprite index
    pub sprite: i32,
    pub x: i32,
    pub y: i32,
    /// the walk (anim 1) instead of the idle (anim 0)
    pub walk: bool,
    pub frame: i32,
    pub tick: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chars {
    pub c: Container,
    /// Character Mode: Balanced (the 32 of [`ROSTER`]) or In-Game (`party`)
    pub balanced: bool,
    /// In-Game: the save's characters (flag bits 0 and 1) as (character id, sprite index)
    pub party: Vec<(i32, i32)>,
    /// +0x11C: the selection
    pub sel: i32,
    /// +0x140: the row's change at the next redraw
    pub state: u8,
    pub slots: Vec<Option<CFig>>,
    /// the figures as OAM shows them: the object world's draw of the previous update (`oam_next` this update's)
    pub oam: Vec<Option<CFig>>,
    #[serde(default)]
    pub oam_next: Vec<Option<CFig>>,
    /// the A prompt's child index
    #[serde(default)]
    pub a_btn: usize,
    pub left: usize,
    pub right: usize,
    /// the frame (BG0) and the figures are up (A hides them for the pick's wait box)
    pub shown: bool,
    /// the generator after the last redraw's figure changes (`built` applies it)
    pub rng_after: Option<Rng>,
    /// both have picked: the fight starts (the app takes over after the fade)
    pub fight: bool,
    /// the pick's wait box ended with B (`wait_cancelled`)
    #[serde(default)]
    pub unpress: bool,
    /// the fade to the fight (`0x0804BBA4`, 16 per step)
    #[serde(default)]
    pub fade: u16,
}

/// what an event of the page asks of the link screen
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharsEv {
    Moved,
    /// A: the pick (the roster index; In-Game: the character id)
    Pick(i32),
    Back,
}

impl Chars {
    /// enter `0x08022B6C` (+ `0x08022AF6`: state 1, the row made at the first redraw)
    pub fn new(w: &World, t: &TextData, balanced: bool, slot: usize) -> Chars {
        let mut c = Container::default();
        let label = |s: usize, y: i32| {
            Widget::Label(Label { x: 0x78, y, text: t.s(s).to_vec(), colour: 0x75, align: 1, font: Font::Menu })
        };
        c.add(label(104, 0x26));
        c.add(label(105, 0x82));
        let a_btn = c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: t.s(23).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let bx = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: bx + 0x10, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let left = c.add(Widget::Button(Button::new(BTN_LEFT, 0xB, 0x56, 4, true, true)));
        let right = c.add(Widget::Button(Button::new(BTN_RIGHT, 0xD6, 0x56, 5, true, true)));
        // In-Game: the picked save's characters with flag bits 0 and 1 (the GameData the Select Game page loaded,
        // kept by the enter at +0x5F4 before it resets the live one)
        let story = w.saves.slot(slot).map(|d| &d.story).unwrap_or(&w.story);
        let party = (0..9)
            .filter_map(|k| {
                let cs = story.chars.get(k)?;
                (cs.flags & 3 == 3).then(|| (k as i32, super::super::save::form_sprite(w, cs.form).unwrap_or(0)))
            })
            .collect();
        Chars {
            c,
            balanced,
            party,
            sel: 0,
            state: 1,
            slots: vec![None; 5],
            oam: vec![None; 5],
            oam_next: vec![None; 5],
            a_btn,
            left,
            right,
            shown: true,
            rng_after: None,
            fight: false,
            unpress: false,
            fade: 0,
        }
    }

    fn count(&self) -> i32 {
        if self.balanced {
            ROSTER.len() as i32
        } else {
            self.party.len() as i32
        }
    }

    fn sprite_of(&self, i: i32) -> i32 {
        if self.balanced {
            ROSTER.get(i as usize).map(|s| s - SPRITE_BIAS).unwrap_or(0)
        } else {
            self.party.get(i as usize).map(|p| p.1).unwrap_or(0)
        }
    }

    /// `0x08022DAA`: a figure made (the idle controller's enter: tick = rand(255))
    fn make(&self, i: i32, rng: &mut Rng) -> CFig {
        CFig { idx: i, sprite: self.sprite_of(i), x: 0, y: 0, walk: false, frame: 0, tick: rng.rand(255) }
    }

    /// the object world's update (the step's `0x0801157E` / `0x08011A1E`): each figure's controller
    pub fn tick_objects(&mut self, w: &World) {
        if !self.shown {
            return;
        }
        for f in self.slots.iter_mut().flatten() {
            let n = w
                .data
                .sprite(f.sprite)
                .and_then(|sp| sp.anim_raw(if f.walk { 2 } else { 1 }))
                .map(|a| a.steps(DIR).len() as i32)
                .unwrap_or(1)
                .max(1);
            f.tick = (f.tick + 1) & 0xFFFF;
            if n == 2 {
                f.frame = ((f.tick & 0xFF) > 0xF0) as i32;
            } else {
                f.frame = (f.tick >> 3) & 0xFF;
                if f.frame >= n {
                    f.frame = 0;
                    f.tick = 0;
                }
            }
        }
    }

    /// the update (`0x0802361A`): the object world's draw (OAM) before the redraw
    pub fn update(&mut self, _w: &World) {
        // the object world's draw (`0x080115B0`) shows each object as its last update captured it: the cel record
        // (+0x54, computed after the controller's tick with the animation it has then) and the place - so a redraw's
        // row change (new places, the walk moved, objects made or deleted) shows two iterations on (menu_parity
        // link_chars lf 1,509 RIGHT: the OAM moves at 1,511, the deleted figure still drawn at 1,510)
        let now = if self.shown { self.slots.clone() } else { vec![None; 5] };
        self.oam = std::mem::replace(&mut self.oam_next, now);
    }

    /// the redraw's draw-time part: the arrows' enabled state and the row's change (`0x08022DF4`)
    pub fn before_draw(&mut self, w: &World) {
        let right = if self.balanced { self.sel < 31 } else { self.count() - 1 > self.sel };
        let left = self.sel > 0;
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.right) {
            b.enabled = right;
        }
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.left) {
            b.enabled = left;
        }
        if !self.shown {
            return;
        }
        let mut rng = self.rng_after.unwrap_or(w.rng);
        match self.state {
            1 => {
                for k in 0..5 {
                    let i = self.sel - 2 + k as i32;
                    self.slots[k] = (i >= 0 && i < self.count()).then(|| {
                        let mut f = self.make(i, &mut rng);
                        f.walk = i == self.sel;
                        f
                    });
                }
            }
            2 => {
                self.slots.remove(0);
                self.slots.push(None);
                if let Some(f) = self.slots[1].as_mut() {
                    f.walk = false;
                }
                if let Some(f) = self.slots[2].as_mut() {
                    f.walk = true;
                }
                if self.sel + 2 < self.count() {
                    self.slots[4] = Some(self.make(self.sel + 2, &mut rng));
                }
            }
            3 => {
                self.slots.pop();
                self.slots.insert(0, None);
                if let Some(f) = self.slots[2].as_mut() {
                    f.walk = true;
                }
                if let Some(f) = self.slots[3].as_mut() {
                    f.walk = false;
                }
                if self.sel >= 2 {
                    self.slots[0] = Some(self.make(self.sel - 2, &mut rng));
                }
            }
            _ => {
                if let Some(f) = self.slots[2].as_mut() {
                    f.walk = true;
                }
            }
        }
        // the positions (every state)
        for (k, f) in self.slots.iter_mut().enumerate() {
            if let Some(f) = f {
                let (fw, fh) = w.data.sprite(f.sprite).map(|s| s.foot).unwrap_or((16, 8));
                f.x = 0x23 + 0x2A * k as i32 - (fw >> 1);
                f.y = 0x6A - (fh >> 1);
            }
        }
        self.state = 0;
        self.rng_after = Some(rng);
    }

    pub fn built(&mut self, w: &mut World) {
        if let Some(r) = self.rng_after.take() {
            w.rng = r;
        }
    }

    /// events `0x080232A6`
    pub fn event(&mut self, w: &mut World, code: i32) -> Option<CharsEv> {
        match code {
            5 => {
                w.sfx(super::super::SFX_CURSOR, 64);
                self.sel += 1;
                self.state = 2;
                Some(CharsEv::Moved)
            }
            4 => {
                w.sfx(super::super::SFX_CURSOR, 64);
                self.sel -= 1;
                self.state = 3;
                Some(CharsEv::Moved)
            }
            1 => {
                w.sfx(super::super::SFX_MENU, 64);
                Some(CharsEv::Back)
            }
            0 => {
                w.sfx(super::super::SFX_MENU, 64);
                // the figures go (the object world cleared), BG0 off: the wait box takes it
                self.shown = false;
                self.slots = vec![None; 5];
                self.oam = vec![None; 5];
                self.oam_next = vec![None; 5];
                // (the page's picture under the box keeps the A prompt pressed)
                self.c.focus = Some(self.a_btn);
                let i = if self.balanced { self.sel } else { self.party.get(self.sel as usize).map(|p| p.0).unwrap_or(0) };
                Some(CharsEv::Pick(i))
            }
            _ => None,
        }
    }

    /// a page the harness found running (`open_json`): the selection and the figures (walk, cel, tick) per slot
    pub fn import(&mut self, w: &World, sel: i32, figs: &[Option<(bool, i32, i32)>]) {
        self.sel = sel;
        self.state = 0;
        for k in 0..5 {
            let i = sel - 2 + k as i32;
            self.slots[k] = figs.get(k).copied().flatten().map(|(walk, frame, tick)| {
                let sprite = self.sprite_of(i);
                let (fw, fh) = w.data.sprite(sprite).map(|s| s.foot).unwrap_or((16, 8));
                // (the capture reads the objects before the opening iteration's object update: one tick on)
                CFig { idx: i, sprite, x: 0x23 + 0x2A * k as i32 - (fw >> 1), y: 0x6A - (fh >> 1), walk, frame, tick: tick + TICK_IMPORT }
            });
        }
        self.oam = self.slots.clone();
        self.oam_next = self.slots.clone();
        self.c.drawn = true;
    }

    /// the pick's wait box ended with B (`0x08022AF6`): the frame back, the row made anew at the next redraw
    pub fn wait_cancelled(&mut self) {
        // (the box goes at once; the page's picture - the A prompt pressed, BG0 cleared - stays until its redraw in
        // the next iteration, which makes the row anew: menu_parity link_chars_back lf 1,549 / 1,550)
        self.unpress = true;
        self.c.drawn = false;
    }

    /// the iteration after a cancelled pick: the redraw with the frame, the row made anew
    pub fn after_cancel(&mut self) {
        if std::mem::take(&mut self.unpress) {
            self.c.focus = None;
            self.c.drawn = false;
            self.shown = true;
            self.state = 1;
        }
    }

    /// the figures' object tiles go with the page
    pub fn figures_gone(&mut self, _w: &mut World) {
        self.slots = vec![None; 5];
        self.oam = vec![None; 5];
        self.oam_next = vec![None; 5];
    }

    /// redraw `0x0802309C`: the page frame + children, the name
    pub fn draw(&self, t: &TextData, cv: &mut Canvas) {
        super::super::kit::page_frame(t, cv, 95);
        self.c.draw(t, cv);
        let name = if self.balanced {
            116 + self.sel.max(0) as usize
        } else {
            158 + self.party.get(self.sel.max(0) as usize).map(|p| p.0).unwrap_or(0) as usize
        };
        let mut p = Pen::new(MenuFont::Menu, 0x78, 0x30, 1, 5, Some(1));
        cv.text(t, &mut p, t.s(name));
    }

    /// the frame around the selected figure: BG0, a 48x56 picture at (96, 72)
    pub fn frame_layer(&self) -> Option<MenuLayer> {
        if !self.shown {
            return None;
        }
        let mut cv = Canvas::new();
        cv.panel(0xA, 2, 0x25, 0x2C);
        Some(MenuLayer { w: 48, h: 56, x: 96, y: 72, priority: 0, ops: cv.ops })
    }

    /// the figures' OBJs (body, halo, shadow), as the save list's
    pub fn objs(&self, w: &World) -> Vec<MenuObj> {
        let mut out = Vec::new();
        for f in self.oam.iter().flatten() {
            let Some(sp) = w.data.sprite(f.sprite) else { continue };
            let Some(an) = sp.anim_raw(if f.walk { 2 } else { 1 }) else { continue };
            let steps = an.steps(DIR);
            // (a cel past the direction's - an animation changed under the controller's cel - reads on into the
            // next direction's cels: menu_parity link_chars lf 1,516)
            let k = f.frame.max(0) as usize;
            let st = match steps.get(k) {
                Some(st) => st,
                None => match an.dirs.get(DIR as usize + 1).and_then(|d| d.get(k - steps.len())) {
                    Some(st) if std::env::var("NEXTDIR").is_ok() => st,
                    _ => match steps.last() {
                        Some(st) => st,
                        None => continue,
                    },
                },
            };
            if let Some((hx, hy)) = st.attach {
                out.push(MenuObj { sheet: "effects/halo".into(), x: f.x + hx, y: f.y + hy, priority: 3, ..Default::default() });
            }
            out.push(MenuObj {
                sheet: format!("sprites/{}", sp.id),
                frame: st.frame,
                hflip: st.flip,
                x: f.x,
                y: f.y,
                priority: 3,
                ..Default::default()
            });
            if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
                out.push(MenuObj { sheet: format!("effects/{sh}"), x: f.x, y: f.y, priority: 3, ..Default::default() });
            }
        }
        out
    }
}
