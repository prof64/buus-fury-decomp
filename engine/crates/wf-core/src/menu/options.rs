//! The Options page (mode vt `0x080498F0`, 0x120 bytes; enter `0x080147B8`, redraw `0x08014770`, events
//! `0x08014AC8`, done check `0x08014BBC`, dtor `0x08014A9C`) and the game options.
//!
//! Children (input order): A prompt (enabled only on Credits / Sleep, set when drawn), "Select", B, "Cancel", the
//! three sliders (`0x08006880`: SFX volume, music volume, text speed, 0..8), the five controls (`0x080060A8`:
//! Sound FX Volume / Music Volume / Text Speed cycling UP / DOWN, Credits <-> Sleep with LEFT / RIGHT), L, R, the
//! Equip / Journal labels, the cursor. A on Credits runs the credits as a nested mode (then the page's VRAM is
//! cleared and redrawn); A on Sleep closes the menu and queues event s007 ("To awaken from Sleep Mode..." ->
//! `sleep_mode`) for the field. B / START close; L -> Equip, R -> Journal.
//!
//! The options live at `0x03001914` (text speed, SFX volume, music volume, 0), loaded from the cartridge's options
//! block at boot (`0x08006B10`: defaults 4 / 6 / 4 when invalid) and written back when the page closes with a
//! change (`0x08006B5E`): global, not part of a save game. Here: `World.menu.settings` (snapshotted, hashed once
//! changed). A slider move applies at once (`0x0803E42A` SFX master, `0x0803F18C` music master): the frontend
//! reads [`settings`] each frame (`SetMasterVolumes { music, sfx }` = the raw 0..8 bytes, `wf-audio`'s masters).

use super::kit::{self, Button, Container, Cursor, Font, Label, Slider, TextCtl, Widget, BTN_A, BTN_B, BTN_L, BTN_R};
use super::text::Canvas;
use super::{Action, MenuData, Page, Screen};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::{MenuFont, MenuView};

/// the options block (`0x03001914`)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Settings {
    /// dialogue text speed 0..8 (read by the dialogue box: literal `0x08071728`)
    pub text_speed: u8,
    pub sfx: u8,
    pub music: u8,
    pub pad: u8,
}

impl Default for Settings {
    /// `0x08006B36`
    fn default() -> Self {
        Settings { text_speed: 4, sfx: 6, music: 4, pad: 0 }
    }
}

/// the options (frontend: volumes for the sound driver, text speed for the dialogue typewriter)
pub fn settings(w: &World) -> Settings {
    w.menu.settings
}

/// skill table (`0x0871DAF8`) / quest table (`0x08720564`) data the pages need (`ui/menu/opts.json`, the quest
/// conditions' bytecode from `scripts/bytecode.json`)
#[derive(Clone, Debug, Default)]
pub struct OptsData {
    pub skills: Vec<SkillUi>,
    pub quests: Vec<QuestUi>,
}

#[derive(Clone, Debug, Default)]
pub struct SkillUi {
    /// +0: 0xFF = no levels
    pub level_byte: u8,
    pub name: u16,
    pub desc: u16,
    pub icon: String,
}

#[derive(Clone, Debug, Default)]
pub struct QuestUi {
    /// 0 gold, 1 silver, 2 bronze
    pub star: u8,
    pub text: u16,
    /// condition bytecode: hidden, completed, goal
    pub code: [Vec<u8>; 3],
    pub ids: [String; 3],
}

impl OptsData {
    pub fn load(src: &dyn crate::content::ContentSource) -> OptsData {
        let mut d = OptsData::default();
        let Some(v) = src.read("ui/menu/opts.json").and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { return d };
        for s in v["skills"].as_array().into_iter().flatten() {
            let i = s["skill"].as_u64().unwrap_or(0) as usize;
            if d.skills.len() <= i {
                d.skills.resize(i + 1, SkillUi::default());
            }
            d.skills[i] = SkillUi {
                level_byte: s["level_byte"].as_u64().unwrap_or(0xFF) as u8,
                name: s["name"].as_u64().unwrap_or(0) as u16,
                desc: s["desc"].as_u64().unwrap_or(0) as u16,
                icon: s["icon"].as_str().unwrap_or("").to_string(),
            };
        }
        let bc = src.read("scripts/bytecode.json").and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        let code = |id: &str| -> Vec<u8> {
            let h = bc.as_ref().and_then(|b| b["inline"][id]["code"].as_str()).unwrap_or("");
            (0..h.len() / 2).filter_map(|k| u8::from_str_radix(&h[2 * k..2 * k + 2], 16).ok()).collect()
        };
        for q in v["quests"].as_array().into_iter().flatten() {
            let ids = [q["hidden"].as_str().unwrap_or("").to_string(), q["done"].as_str().unwrap_or("").to_string(), q["goal"].as_str().unwrap_or("").to_string()];
            d.quests.push(QuestUi {
                star: q["star"].as_u64().unwrap_or(0) as u8,
                text: q["text"].as_u64().unwrap_or(0) as u16,
                code: [code(&ids[0]), code(&ids[1]), code(&ids[2])],
                ids,
            });
        }
        d
    }
}

/// video frames one redraw of the Options page takes (frame + 3 sliders + 9 short strings; `timing.py`)
pub const REDRAW_FRAMES: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Options {
    pub c: Container,
    pub a_btn: usize,
    pub sliders: [usize; 3],
    /// after the credits: VRAM / palette cleared until the page is drawn again
    pub blank: bool,
}

impl Options {
    /// `0x080147B8`
    pub fn new(w: &World) -> Options {
        let t = &w.data.menu.as_ref().expect("menu content").text;
        let st = w.menu.settings;
        let mut c = Container::default();
        let a_btn = c.add(Widget::Button(Button::new(BTN_A, 0xC, 0x84, 0, true, false)));
        c.add(Widget::Label(Label { x: 0x1C, y: 0x90, text: t.s(23).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let x = t.font(MenuFont::Menu).measure(t.s(23)) + 0x1E;
        c.add(Widget::Button(Button::new(BTN_B, x, 0x84, 1, true, false)));
        c.add(Widget::Label(Label { x: x + 0x10, y: 0x90, text: t.s(26).to_vec(), colour: 5, align: 0, font: Font::Menu }));
        let vals = [st.sfx, st.music, st.text_speed];
        let mut sliders = [0; 3];
        for (k, y) in [0x21, 0x33, 0x45].iter().enumerate() {
            sliders[k] = c.add(Widget::Slider(Slider { id: k as i32, x0: 0x6D, y: *y, w: 0xDD - 0x6D - 2, value: vals[k] as i32, max: 8, code: 0x100 + k as i32 }));
        }
        for (id, nav, s, x, y, align) in [
            (0, [3, 1, -1, -1], 16, 0x10, 0x28, 0),
            (1, [0, 2, -1, -1], 17, 0x10, 0x3A, 0),
            (2, [1, 3, -1, -1], 18, 0x10, 0x4C, 0),
            (3, [2, 0, 4, 4], 20, 0x46, 0x70, 1),
            (4, [2, 0, 3, 3], 19, 0xAA, 0x70, 1),
        ] {
            c.add(Widget::Text(TextCtl { id, nav, text: t.s(s).to_vec(), x, y, code: -1, align }));
        }
        c.add(Widget::Button(Button::new(BTN_L, 8, 8, 6, true, false)));
        c.add(Widget::Button(Button::new(BTN_R, 0xD0, 8, 7, true, false)));
        c.add(Widget::Label(Label { x: 0x22, y: 0x14, text: t.s(36).to_vec(), colour: 2, align: 0, font: Font::Menu }));
        c.add(Widget::Label(Label { x: 0xCE, y: 0x14, text: t.s(30).to_vec(), colour: 2, align: 2, font: Font::Menu }));
        c.add(Widget::Cursor(Cursor { t: 0 }));
        Options { c, a_btn, sliders, blank: false }
    }

    fn event(&mut self, w: &mut World, code: i32) -> (bool, Option<Action>) {
        match code {
            0 => match self.c.sel {
                // Credits: a nested mode (credits.rs), then the page's VRAM is reset and the page drawn again
                3 => (true, Some(Action::Push(Screen::Credits(super::credits::Credits::new(w))))),
                4 => {
                    w.menu.then = Some("s007".to_string());
                    (true, Some(Action::Close))
                }
                _ => (true, None),
            },
            1 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(Action::Close))
            }
            6 | 7 => {
                w.sfx(super::SFX_MENU, 64);
                (true, Some(Action::Replace(super::page_screen(w, 3, code == 7))))
            }
            0x100..=0x102 => {
                let k = (code - 0x100) as usize;
                let v = match self.c.kids.get(self.sliders[k]) {
                    Some(Widget::Slider(s)) => s.value as u8,
                    _ => 0,
                };
                let st = &mut w.menu.settings;
                match k {
                    0 => st.sfx = v,
                    1 => st.music = v,
                    _ => st.text_speed = v,
                }
                if k < 2 {
                    let st = w.menu.settings;
                    w.events.push(crate::world::Event::MasterVolumes { music: st.music as i32, sfx: st.sfx as i32 });
                }
                (true, None)
            }
            _ => (false, None),
        }
    }
}

impl Page for Options {
    fn container(&mut self) -> &mut Container {
        &mut self.c
    }
    fn container_ref(&self) -> &Container {
        &self.c
    }
    fn before_draw(&mut self, _w: &World) {
        // 0x08014770: the A prompt only for Credits / Sleep
        let sel = self.c.sel;
        if let Some(Widget::Button(b)) = self.c.kids.get_mut(self.a_btn) {
            b.enabled = sel > 2;
        }
        self.blank = false;
    }
    fn step(&mut self, w: &mut World) -> Action {
        let (consumed, act) = kit::run_step(self, w, |p| &mut p.c, |p, w, _src, code| p.event(w, code));
        if let Some(a) = act {
            return a;
        }
        if !consumed && w.menu.pressed & crate::input::START != 0 {
            w.sfx(super::SFX_MENU, 64);
            return Action::Done;
        }
        Action::None
    }
    fn child_result(&mut self, w: &mut World, _r: i32) -> Action {
        // 0x08014B50: VRAM / palettes reset (black) until the redraw lands
        self.blank = true;
        w.menu.shown = Some(Box::new(Screen::Options(self.clone())));
        Action::None
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        if self.blank {
            return MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() };
        }
        let t = &d.text;
        let mut cv = Canvas::new();
        // status_page(.., "Options", children): title then the children
        kit::page_frame(t, &mut cv, 2);
        self.c.draw(t, &mut cv);
        super::surface_view(cv)
    }
    fn redraw_cost(&self) -> u8 {
        REDRAW_FRAMES
    }
    /// options_sliders / options_pages / journal_scroll: 51 redraws, residual std 0.7k, max 1.7k
    fn redraw_time(&self, w: &World, d: &MenuData) -> Option<crate::sndsim::Redraw> {
        if self.blank {
            return None;
        }
        Some(super::page_time(d, &self.view(w, d), !self.c.ever, 4_641, 232_905, 0, 0, 100_222))
    }
}

/// a harness opening this screen from JSON (`menu::open_from_json`): `{"kind": "options"}`
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    (v["kind"] == "options").then(|| (Screen::Options(Options::new(w)), super::PAUSE, false))
}
