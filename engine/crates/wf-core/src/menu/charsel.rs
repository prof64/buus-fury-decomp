//! The Switch Character screen (`character_menu` 0x8D: native `0x080027DE`, mode ctor `0x0802E83C`, vt `0x0804A5E0`:
//! enter `0x0802E97A`, update `0x0802EB94`, step `0x0802EBA4` (the object world's update, then the container step),
//! redraw `0x0802EC3E`, events `0x0802ECEA`, previous / next `0x0802EC00` / `0x0802EBBE`, selection change
//! `0x0802EC9C`).
//!
//! The party (characters 0..7 with flag bit 0) stand as character objects in a row at y 88: spacing
//! min(0x90 / n, 0x30), first x = 0x70 − (n − 1) · spacing / 2. Members without the character flag bit 1 cannot be
//! chosen: silhouettes (`0x0800EECC` with `0x0805682C`), frozen. Every figure starts idle (the idle state's enter:
//! tick = rand(255)), the current character walks in place (anim 1). LEFT / RIGHT (prompts at (24, 74) / (200, 74),
//! codes 4 / 5, enabled when a choosable member exists that way) move the selection (sound 39; the old one idles
//! again, rand(255)); A (sound 40) makes the selection the current character and ends with 2; B (sound 40) ends
//! with 1. The page: the pause-page frame titled "Switch Character", A "Select" / B "Cancel".

use super::kit::{self, Button, Container, Font, Label, Widget, BTN_A, BTN_B, BTN_LEFT, BTN_RIGHT};
use super::text::Canvas;
use super::{Action, MenuData, Page, Transition};
use crate::rng::Rng;
use crate::scene::ActAnim;
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuObj, MenuView};

/// a figure of the row
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CFig {
    pub ch: i32,
    pub sprite: i32,
    pub x: i32,
    pub y: i32,
    pub choosable: bool,
    pub anim: ActAnim,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharSelect {
    pub c: Container,
    /// A chose another character than the current one (the timing model: the close loads the new leader)
    #[serde(default)]
    pub switched: bool,
    pub figs: Vec<CFig>,
    /// the selected figure (+0x160)
    pub sel: usize,
    pub left: usize,
    pub right: usize,
    /// the figures as the object update left them (OAM is written before the container step handles a press)
    pub oam: Vec<CFig>,
    /// a harness built the screen with a copy of the RNG: the world's RNG takes it at the first step
    pub rng_after: Option<Rng>,
    /// the figures' idle draws are still to come: the game builds the screen (the figures' idle enters) when the
    /// field's fade-out reaches black (segment sb 21-14: 16 frames after `character_menu`)
    #[serde(default)]
    pub idle_at_step: bool,
}

/// the figures' facing (down) and the walk animation id
const DIR: u8 = 0;
const WALK: i32 = 1;

fn idle(rng: &mut Rng) -> ActAnim {
    let mut a = ActAnim::of(0);
    a.tick = rng.rand(255);
    a
}

impl CharSelect {
    /// `0x0802E83C` + enter `0x0802E97A`
    pub fn new(w: &World, rng: &mut Rng) -> CharSelect {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let party: Vec<usize> = (0..8).filter(|&k| w.story.chars.get(k).is_some_and(|c| c.flags & 1 != 0)).collect();
        let n = party.len().max(1) as i32;
        let sp = (0x90 / n).min(0x30);
        let mut x = 0x70 - (n - 1) * sp / 2;
        let mut figs = Vec::new();
        let mut sel = 0;
        for &k in &party {
            let cs = &w.story.chars[k];
            let sprite = super::save::form_sprite(w, cs.form).unwrap_or(0);
            if w.story.current == k as i32 {
                sel = figs.len();
            }
            figs.push(CFig { ch: k as i32, sprite, x, y: 0x58, choosable: cs.flags & 2 != 0, anim: idle(rng) });
            x += sp;
        }
        if let Some(f) = figs.get_mut(sel) {
            f.anim = ActAnim::of(WALK);
        }
        let mut c = Container::default();
        c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: t.s(23).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let bx = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, bx, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: bx + 0x10, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let left = c.add(Widget::Button(Button::new(BTN_LEFT, 0x18, 0x4A, 4, true, false)));
        let right = c.add(Widget::Button(Button::new(BTN_RIGHT, 0xC8, 0x4A, 5, true, false)));
        CharSelect { c, switched: false, oam: figs.clone(), figs, sel, left, right, rng_after: None, idle_at_step: false }
    }

    /// the figures' idle enters (rand(255) each) when the screen is built (`idle_at_step`; no-op after)
    pub fn build_idle(&mut self, w: &mut World) {
        if !std::mem::take(&mut self.idle_at_step) {
            return;
        }
        let mut rng = w.rng;
        for (k, f) in self.figs.iter_mut().enumerate() {
            let a = idle(&mut rng);
            if k != self.sel {
                f.anim = a;
            }
        }
        w.rng = rng;
        self.oam = self.figs.clone();
        // the object world gives way to the figures (their object tiles: vram.rs)
        crate::vram::world_hide(w);
        for (k, f) in self.figs.iter().enumerate() {
            crate::vram::figure_created(w, k as u32, f.sprite);
        }
    }

    /// `0x0802EC00` / `0x0802EBBE`: the previous / next choosable figure
    fn prev(&self) -> Option<usize> {
        (0..self.sel).rev().find(|&k| self.figs[k].choosable)
    }
    fn next(&self) -> Option<usize> {
        (self.sel + 1..self.figs.len()).find(|&k| self.figs[k].choosable)
    }

    /// `0x0802EC9C`
    fn select(&mut self, w: &mut World, to: usize) {
        w.sfx(super::SFX_CURSOR, 64);
        let mut rng = w.rng;
        if let Some(f) = self.figs.get_mut(self.sel) {
            // a new idle controller too: its first tick a frame late
            f.anim = idle(&mut rng);
            f.anim.tick -= 1;
        }
        w.rng = rng;
        self.sel = to;
        if let Some(f) = self.figs.get_mut(to) {
            // a new walk controller (a world object made during the container step): its first tick comes a frame late
            f.anim = ActAnim::of(WALK);
            f.anim.tick = -1;
        }
    }
}

impl Page for CharSelect {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn before_draw(&mut self, _w: &World) {
        let (p, n) = (self.prev().is_some(), self.next().is_some());
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.left) {
            b.enabled = p;
        }
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.right) {
            b.enabled = n;
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        if let Some(r) = self.rng_after.take() {
            w.rng = r;
        }
        self.build_idle(w);
        // the object world's update: the choosable figures animate
        let d = std::sync::Arc::clone(&w.data);
        for f in self.figs.iter_mut().filter(|f| f.choosable) {
            f.anim.tick(&d, Some(f.sprite), DIR);
        }
        self.oam = self.figs.clone();
        let (_, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| match code {
            0 => {
                w.sfx(super::SFX_MENU, 64);
                if let Some(f) = p.figs.get(p.sel) {
                    if w.story.current != f.ch {
                        p.switched = true;
                        if let Some(super::Screen::CharSelect(sh)) = w.menu.shown.as_deref_mut() {
                            sh.switched = true;
                        }
                    }
                    w.story.current = f.ch;
                }
                (true, Some(Action::Pop(2)))
            }
            1 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(Action::Pop(1)))
            }
            4 | 5 => {
                let to = if code == 4 { p.prev() } else { p.next() };
                if let Some(to) = to {
                    p.select(w, to);
                }
                p.c.drawn = false;
                (true, None)
            }
            _ => (false, None),
        });
        act.unwrap_or(Action::None)
    }
    /// the page frame + prompts: 2 frames (`re/buu/menu/timing.py`)
    fn redraw_cost(&self) -> u8 {
        2
    }
    /// the redraw's own cycles: 349,559 .. 389,604 measured (a prompt's press, a selection change; the figures
    /// are objects, not drawn into the page) - always 2 frames, the value places the sound update
    fn redraw_time(&self, _w: &World, _d: &MenuData) -> Option<crate::sndsim::Redraw> {
        Some(crate::sndsim::Redraw { t0: 4_641, base: 360_000 })
    }
    fn leave_black(&self) -> bool {
        true
    }
    fn objs(&self, w: &World) -> Vec<MenuObj> {
        let mut out = Vec::new();
        let sil = w.data.menu.as_ref().map(|d| d.items.silhouette.clone()).unwrap_or_default();
        for f in &self.oam {
            let Some(sp) = w.data.sprite(f.sprite) else { continue };
            // the members that cannot be chosen stand frozen, turned away (as on the save list)
            let (raw, dir, frame) =
                if f.choosable { (f.anim.raw, DIR, f.anim.frame) } else { (super::save::FIG_FROZEN_ANIM, super::save::FIG_FROZEN_DIR, 0) };
            let Some(an) = sp.anim_raw(raw) else { continue };
            let steps = an.steps(dir);
            let Some(st) = steps.get((frame.max(0) as usize).min(steps.len().saturating_sub(1))) else { continue };
            let remap = if f.choosable { Vec::new() } else { sil.clone() };
            if let Some((hx, hy)) = st.attach {
                out.push(MenuObj { sheet: "effects/halo".into(), x: f.x + hx, y: f.y + hy, priority: 3, remap: remap.clone(), ..Default::default() });
            }
            out.push(MenuObj { sheet: format!("sprites/{}", sp.id), frame: st.frame, hflip: st.flip, x: f.x, y: f.y, priority: 3, remap, ..Default::default() });
            if let Some(sh) = sp.shadow.as_ref().filter(|sh| sh.as_str() != "shadow_0x0") {
                out.push(MenuObj { sheet: format!("effects/{sh}"), x: f.x, y: f.y, priority: 3, ..Default::default() });
            }
        }
        out
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let mut cv = Canvas::new();
        kit::page_frame(&d.text, &mut cv, 12);
        self.c.draw(&d.text, &mut cv);
        let mut v = super::surface_view(cv);
        v.layers[0].priority = 3;
        v
    }
}

/// the screen change (quick fade; the page shows on the 3rd black frame)
pub const SCRIPTED: Transition = Transition { fade_out: 16 | super::FADE_HOLD, enter: 3, leave: 25, fade_in: 16 };

/// native 0x8D `character_menu`: `World.menu.result` = 2 when a character was chosen (`World.story.current` set:
/// the caller rebuilds the leader for it, `0x080027DE`: form sprite + controller), 1 when cancelled. No player
/// object = nothing happens (the caller checks).
pub fn open_charsel(w: &mut World) {
    let mut rng = w.rng;
    let mut s = CharSelect::new(w, &mut rng);
    s.idle_at_step = true;
    super::open(w, super::Screen::CharSelect(s), SCRIPTED, true);
}

/// a harness opening this screen from JSON: `{"kind": "charsel", "rng": [a, b]}` (the RNG at the ctor)
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    if v["kind"].as_str()? != "charsel" {
        return None;
    }
    let mut rng = match v["rng"].as_array() {
        Some(a) => Rng { a: a[0].as_u64().unwrap_or(0) as u32, b: a[1].as_u64().unwrap_or(0) as u32, calls: w.rng.calls },
        None => w.rng,
    };
    let mut s = CharSelect::new(w, &mut rng);
    s.rng_after = Some(rng);
    Some((super::Screen::CharSelect(s), SCRIPTED, true))
}
