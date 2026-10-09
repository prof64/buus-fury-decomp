//! The PC system menu: Esc in every mode (Story, Battle, 2P, link, online, replays) - Resume / Restart fight /
//! Settings (view, volume, full screen, keys) / Controls (every key in plain words) / Quit to main menu / Quit game.
//! Esc or Resume closes it; Esc on a sub-page goes back. Keyboard (arrows, Enter / X, Esc / Z / Backspace) and the
//! mouse (hover highlights, a click picks; on a "< value >" row the click's side steps it).
//!
//! Drawn in the game's menu look (the pause pages' bevelled panel, fonts and UI palette: `wf_core::menu::text::Canvas`
//! -> `wf_render::menu::MenuRenderer`) over the dimmed picture, scaled by the largest integer the view takes. It is a
//! PC menu - not one of the game's own screens (Story's Start menu stays the game's).
//!
//! The menu does not run anything itself: [`SysMenu::key`] / [`SysMenu::pointer`] return what was chosen
//! ([`SysAction`]); [`crate::control::Control`] carries out what it can (resume, restart) and hands the rest to the
//! shell (quit, view, volume, full screen, change keys).

use std::sync::Arc;

use wf_content::Packs;
use wf_core::content::GameData;
use wf_core::menu::text::{units, Canvas, Pen, TextData};
use wf_render::menu::MenuRenderer;
use wf_view::MenuFont;

use crate::hotkeys::{Action, Hotkeys};
use crate::input::Bindings;

/// UI palette indices (`ui/palette.json`)
const WHITE: u8 = 2;
const RED: u8 = 3;
const YELLOW: u8 = 5;
const GREEN: u8 = 6;
const GREY: u8 = 0x75;
const SHADOW: u8 = 1;

/// What the player chose.
#[derive(Clone, Debug, PartialEq)]
pub enum SysAction {
    Resume,
    Restart,
    QuitToLauncher,
    QuitGame,
    /// Classic 240x160 <-> PC wide
    ToggleView,
    /// the new volume (0..=100)
    Volume(u32),
    ToggleFullscreen,
    /// the shell's "change keys" screen
    Rebind,
}

/// What the current game allows / shows (the shell's and the session's facts at opening time).
#[derive(Clone, Debug, Default)]
pub struct SysCtx {
    /// a local battle: "Restart fight"
    pub restart: bool,
    /// the shell can close itself (native): "Quit game"
    pub quit_game: bool,
    /// story progress since the last save would be lost: "Quit to main menu" asks first
    pub unsaved: bool,
    /// an online / link match: the game keeps running under the menu
    pub online: bool,
    /// "Classic 240x160" / "PC wide"
    pub classic: bool,
    pub volume: u32,
    /// Some(on) where the shell has a full screen switch
    pub fullscreen: Option<bool>,
    /// the shell has a key rebinding screen
    pub rebind: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Page {
    #[default]
    Main,
    Settings,
    Controls,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Resume,
    Restart,
    Settings,
    Controls,
    Quit,
    QuitGame,
    View,
    Volume,
    Fullscreen,
    Rebind,
    Back,
    /// Confirm: quit without saving
    QuitAnyway,
    /// a Controls line (not selectable)
    Info,
}

#[derive(Default)]
pub struct SysMenu {
    pub open: bool,
    page: Page,
    cursor: usize,
    /// the Controls page's first line
    scroll: usize,
    pub ctx: SysCtx,
    /// the keys shown on the Controls page
    pub bindings: Bindings,
    pub hotkeys: Hotkeys,
    /// rows drawn last: (x0, y0, x1, y1) in the 240x160 menu, row index, the value's x range (a "< value >" row)
    hits: Vec<([i32; 4], usize, Option<(i32, i32)>)>,
    /// where the menu picture sits in the view: origin and scale
    at: (i32, i32, i32),
    data: Option<Option<Arc<GameData>>>,
    menu: Option<MenuRenderer>,
}

/// a key code in plain words ("KeyX" -> "X", "ArrowUp" -> "Up", "Shift+F6" -> "Shift+F6")
pub fn key_name(code: &str) -> String {
    let (mods, key) = match code.rfind('+') {
        Some(k) if k + 1 < code.len() => (&code[..=k], &code[k + 1..]),
        _ => ("", code),
    };
    let k = if let Some(c) = key.strip_prefix("Key") {
        c.to_string()
    } else if let Some(d) = key.strip_prefix("Digit") {
        d.to_string()
    } else if let Some(a) = key.strip_prefix("Arrow") {
        a.to_string()
    } else {
        match key {
            "Backquote" => "`".into(),
            "Minus" => "-".into(),
            "Equal" => "=".into(),
            "BracketLeft" => "[".into(),
            "BracketRight" => "]".into(),
            "Semicolon" => ";".into(),
            "Quote" => "'".into(),
            "Comma" => ",".into(),
            "Period" => ".".into(),
            "Slash" => "/".into(),
            "Backslash" => "\\".into(),
            "Escape" => "Esc".into(),
            k => k.to_string(),
        }
    };
    format!("{mods}{k}")
}

impl SysMenu {
    /// Open on the main page.
    pub fn show(&mut self, ctx: SysCtx) {
        self.ctx = ctx;
        self.open = true;
        self.page = Page::Main;
        self.cursor = 0;
        self.scroll = 0;
    }

    fn rows(&self) -> Vec<(Item, String, String)> {
        let s = |x: &str| x.to_string();
        match self.page {
            Page::Main => {
                let mut v = vec![(Item::Resume, s("Resume"), String::new())];
                if self.ctx.restart {
                    v.push((Item::Restart, s("Restart fight"), String::new()));
                }
                v.push((Item::Settings, s("Settings"), String::new()));
                v.push((Item::Controls, s("Controls"), String::new()));
                v.push((Item::Quit, s(if self.ctx.online { "Leave match" } else { "Quit to main menu" }), String::new()));
                if self.ctx.quit_game {
                    v.push((Item::QuitGame, s("Quit game"), String::new()));
                }
                v
            }
            Page::Settings => {
                let mut v = vec![
                    (Item::View, s("View"), format!("< {} >", if self.ctx.classic { "Classic 240x160" } else { "PC wide" })),
                    (Item::Volume, s("Volume"), format!("< {} >", self.ctx.volume)),
                ];
                if let Some(f) = self.ctx.fullscreen {
                    v.push((Item::Fullscreen, s("Full screen"), format!("< {} >", if f { "On" } else { "Off" })));
                }
                if self.ctx.rebind {
                    v.push((Item::Rebind, s("Change keys..."), String::new()));
                }
                v.push((Item::Back, s("Back"), String::new()));
                v
            }
            Page::Controls => {
                let mut v: Vec<(Item, String, String)> = Vec::new();
                let keys = |btn: &str| -> String {
                    let ks: Vec<String> = self.bindings.keys.iter().filter(|(_, n)| n.eq_ignore_ascii_case(btn)).map(|(c, _)| key_name(c)).collect();
                    if ks.is_empty() {
                        "-".into()
                    } else {
                        ks.join(" / ")
                    }
                };
                let dirs = ["Up", "Down", "Left", "Right"].map(keys).join(" ");
                for (what, k) in [
                    ("Move", dirs),
                    ("Attack / OK (A)", keys("A")),
                    ("Skill / back (B)", keys("B")),
                    ("Change skill (L)", keys("L")),
                    ("Block / shield (R)", keys("R")),
                    ("Game menu (Start)", keys("Start")),
                    ("Skills (Select)", keys("Select")),
                    ("This menu", "Esc".into()),
                    ("Classic / PC wide", "F1".into()),
                    ("Full screen", "F11".into()),
                    ("Change keys", "F5".into()),
                ] {
                    v.push((Item::Info, what.to_string(), k));
                }
                for a in Action::all() {
                    let ks: Vec<String> = self.hotkeys.keys_for(a).iter().map(|c| key_name(c)).collect();
                    if !ks.is_empty() {
                        v.push((Item::Info, a.label(), ks.join(" / ")));
                    }
                }
                v.push((Item::Back, s("Back"), String::new()));
                v
            }
            Page::Confirm => vec![
                (Item::QuitAnyway, s("Quit without saving"), String::new()),
                (Item::Back, s("Keep playing"), String::new()),
            ],
        }
    }

    fn selectable(&self, i: usize) -> bool {
        self.rows().get(i).is_some_and(|r| r.0 != Item::Info)
    }

    fn back(&mut self) -> Option<SysAction> {
        match self.page {
            Page::Main => {
                self.open = false;
                Some(SysAction::Resume)
            }
            Page::Confirm => {
                self.page = Page::Main;
                self.cursor = 0;
                None
            }
            _ => {
                let from = self.page;
                self.page = Page::Main;
                self.cursor = self.rows().iter().position(|r| (from == Page::Settings && r.0 == Item::Settings) || (from == Page::Controls && r.0 == Item::Controls)).unwrap_or(0);
                None
            }
        }
    }

    fn activate(&mut self, i: usize, d: i32) -> Option<SysAction> {
        let rows = self.rows();
        let item = rows.get(i)?.0;
        match item {
            Item::Resume => {
                self.open = false;
                Some(SysAction::Resume)
            }
            Item::Restart => {
                self.open = false;
                Some(SysAction::Restart)
            }
            Item::Settings | Item::Controls => {
                self.page = if item == Item::Settings { Page::Settings } else { Page::Controls };
                self.cursor = 0;
                self.scroll = 0;
                if self.page == Page::Controls {
                    self.cursor = self.rows().len() - 1;
                }
                None
            }
            Item::Quit if self.ctx.unsaved => {
                self.page = Page::Confirm;
                self.cursor = 1;
                None
            }
            Item::Quit | Item::QuitAnyway => {
                self.open = false;
                Some(SysAction::QuitToLauncher)
            }
            Item::QuitGame => Some(SysAction::QuitGame),
            Item::View => {
                self.ctx.classic = !self.ctx.classic;
                Some(SysAction::ToggleView)
            }
            Item::Volume => {
                let d = if d == 0 { 1 } else { d };
                self.ctx.volume = (self.ctx.volume as i32 + d * 10).clamp(0, 100) as u32;
                Some(SysAction::Volume(self.ctx.volume))
            }
            Item::Fullscreen => {
                self.ctx.fullscreen = self.ctx.fullscreen.map(|f| !f);
                Some(SysAction::ToggleFullscreen)
            }
            Item::Rebind => {
                self.open = false;
                Some(SysAction::Rebind)
            }
            Item::Back => self.back(),
            Item::Info => None,
        }
    }

    fn step(&mut self, d: i32) {
        let n = self.rows().len();
        if n == 0 {
            return;
        }
        if self.page == Page::Controls {
            // a list to read: UP / DOWN scroll it, the cursor stays on Back
            let max = n.saturating_sub(VISIBLE);
            self.scroll = (self.scroll as i32 + d).clamp(0, max as i32) as usize;
            return;
        }
        let mut c = self.cursor;
        for _ in 0..n {
            c = (c as i32 + d).rem_euclid(n as i32) as usize;
            if self.selectable(c) {
                break;
            }
        }
        self.cursor = c;
    }

    /// A key (W3C code) while open. Esc / Z (B) / Backspace back; arrows; Enter / Space / X (A) pick.
    pub fn key(&mut self, code: &str) -> Option<SysAction> {
        match code {
            "Escape" | "KeyZ" | "Backspace" => self.back(),
            "ArrowUp" | "KeyW" => {
                self.step(-1);
                None
            }
            "ArrowDown" | "KeyS" => {
                self.step(1);
                None
            }
            "ArrowLeft" | "ArrowRight" | "KeyA" | "KeyD" => {
                let d = if code == "ArrowLeft" || code == "KeyA" { -1 } else { 1 };
                let c = self.cursor;
                if self.rows().get(c).is_some_and(|r| r.2.starts_with("< ")) {
                    self.activate(c, d)
                } else {
                    None
                }
            }
            "Enter" | "Space" | "KeyX" | "NumpadEnter" => {
                let c = self.cursor;
                self.activate(c, 1)
            }
            _ => None,
        }
    }

    /// The mouse in view pixels: `click` = the left button went down. Hover moves the highlight.
    pub fn pointer(&mut self, x: i32, y: i32, click: bool) -> Option<SysAction> {
        let (ox, oy, k) = self.at;
        if k <= 0 {
            return None;
        }
        let (mx, my) = ((x - ox).div_euclid(k), (y - oy).div_euclid(k));
        let hit = self.hits.iter().find(|h| mx >= h.0[0] && mx < h.0[2] && my >= h.0[1] && my < h.0[3]).cloned()?;
        if !self.selectable(hit.1) {
            return None;
        }
        self.cursor = hit.1;
        if !click {
            return None;
        }
        match hit.2 {
            // a "< value >" row: the left half steps back, the right half on
            Some((v0, v1)) if mx >= v0 => self.activate(hit.1, if mx < (v0 + v1) / 2 { -1 } else { 1 }),
            _ => self.activate(hit.1, 1),
        }
    }

    /// Draw over the view (`w` x `h`): the picture dimmed, the menu in the middle.
    pub fn draw(&mut self, packs: &Packs, fb: &mut [u32], w: usize, h: usize) {
        if !self.open || fb.len() < w * h {
            return;
        }
        let data = self.data.get_or_insert_with(|| crate::battle::load_data(packs).ok()).clone();
        let Some(md) = data.as_ref().and_then(|d| d.menu.clone()) else { return };
        let t = &md.text;
        for p in fb[..w * h].iter_mut() {
            *p = 0xFF00_0000 | ((*p >> 1) & 0x007F_7F7F);
        }
        let mut cv = Canvas::new();
        let rows = self.rows();
        let title = match self.page {
            Page::Main => "PAUSED",
            Page::Settings => "SETTINGS",
            Page::Controls => "CONTROLS",
            Page::Confirm => "QUIT?",
        };
        let (shown, first): (Vec<usize>, usize) = if self.page == Page::Controls {
            // the readable list scrolls; Back stays under it
            let n = rows.len() - 1;
            let first = self.scroll.min(n.saturating_sub(VISIBLE));
            let mut v: Vec<usize> = (first..n.min(first + VISIBLE)).collect();
            v.push(n);
            (v, first)
        } else {
            ((0..rows.len()).collect(), 0)
        };
        let _ = first;
        let wide = self.page != Page::Main;
        let (x0, x1) = if wide { (8, 232) } else { (52, 188) };
        let row_h = 12;
        let note = self.page == Page::Confirm;
        let body_h = shown.len() as i32 * row_h + if note { 24 } else { 0 };
        let y0 = ((160 - (body_h + 30)) / 2).max(2);
        cv.panel(x0, y0, x1, y0 + 22);
        if wide {
            item(&mut cv, t, MenuFont::Bold, title, x0 + 10, y0 + 17, 0, YELLOW, false, 120);
        } else {
            item(&mut cv, t, MenuFont::Bold, title, (x0 + x1) / 2, y0 + 17, 1, YELLOW, false, x1 - x0 - 8);
        }
        let by = y0 + 24;
        cv.panel(x0, by, x1, by + body_h + 8);
        self.hits.clear();
        let mut y = by + 4;
        if note {
            item(&mut cv, t, MenuFont::Small, "Progress since your last save", (x0 + x1) / 2, y + 9, 1, WHITE, false, x1 - x0 - 8);
            item(&mut cv, t, MenuFont::Small, "will be lost.", (x0 + x1) / 2, y + 19, 1, WHITE, false, x1 - x0 - 8);
            y += 24;
        }
        let fm = t.font(MenuFont::Menu);
        let lw = rows.iter().filter(|r| !r.2.is_empty()).map(|r| fm.measure(&units(&r.1))).max().unwrap_or(0);
        let vx = (x0 + 10 + lw + 10).min(x1 - 60);
        for &i in &shown {
            let (it, label, value) = &rows[i];
            let focus = i == self.cursor && *it != Item::Info;
            let ty = y + 10;
            if value.is_empty() {
                // a centred choice
                item(&mut cv, t, MenuFont::Menu, label, (x0 + x1) / 2, ty, 1, if *it == Item::Info { WHITE } else { YELLOW }, focus, x1 - x0 - 20);
                self.hits.push(([x0, y, x1, y + row_h], i, None));
            } else if *it == Item::Info {
                // Controls: what, then its keys
                let mut p = Pen::new(MenuFont::Small, x0 + 8, ty, 0, YELLOW, Some(SHADOW));
                cv.text(t, &mut p, &units(label));
                item(&mut cv, t, MenuFont::Small, value, x1 - 8, ty, 2, WHITE, false, x1 - x0 - 16 - t.font(MenuFont::Small).measure(&units(label)) - 6);
                self.hits.push(([x0, y, x1, y + row_h], i, None));
            } else {
                let changeable = value.starts_with("< ") && value.ends_with(" >");
                let mut p = Pen::new(MenuFont::Menu, x0 + 10, ty, 0, if focus && !changeable { GREEN } else { YELLOW }, Some(SHADOW));
                cv.text(t, &mut p, &units(label));
                let v = if changeable { &value[2..value.len() - 2] } else { value.as_str() };
                let vw = item(&mut cv, t, MenuFont::Menu, v, vx, ty, 0, WHITE, focus && changeable, x1 - vx - 10);
                self.hits.push(([x0, y, x1, y + row_h], i, Some((vx - 8, vx + vw + 8))));
            }
            y += row_h;
        }
        if self.page == Page::Controls && rows.len() - 1 > VISIBLE {
            item(&mut cv, t, MenuFont::Small, "UP / DOWN: more", x1 - 8, y0 + 15, 2, GREY, false, 80);
        }
        let mut mv = wf_core::menu::surface_view(cv);
        mv.overlay = true;
        let mr = self.menu.get_or_insert_with(|| MenuRenderer::load(packs));
        let pic = mr.render(packs, &mv, None, 0);
        // scaled by the largest integer the view takes, centred
        let k = ((w / 240).min(h / 160)).max(1) as i32;
        let (ox, oy) = ((w as i32 - 240 * k) / 2, (h as i32 - 160 * k) / 2);
        self.at = (ox, oy, k);
        for yy in 0..160 * k {
            for xx in 0..240 * k {
                let c = pic[((yy / k) * 240 + xx / k) as usize];
                if c >> 24 == 0 {
                    continue;
                }
                let (px, py) = (ox + xx, oy + yy);
                if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                    fb[py as usize * w + px as usize] = c;
                }
            }
        }
    }
}

/// Controls lines shown at once
const VISIBLE: usize = 8;

/// a 3 x 5 arrow head pointing left (tip at x) / right (tip at x + 2), centred on row y
fn arrow(cv: &mut Canvas, x: i32, y: i32, left: bool) {
    for k in 0..3 {
        let c = if left { x + k } else { x + 2 - k };
        cv.fill(c, y - k, c + 1, y + k + 1, YELLOW);
    }
}

/// text cut to `max_w`; focused: green between yellow arrows; `align` 0 left, 1 centre, 2 right. Its width.
#[allow(clippy::too_many_arguments)]
fn item(cv: &mut Canvas, t: &TextData, font: MenuFont, s: &str, x: i32, y: i32, align: u8, colour: u8, focus: bool, max_w: i32) -> i32 {
    let fm = t.font(font);
    let mut u = units(s);
    if fm.measure(&u) > max_w.max(8) {
        let dots = units("..");
        while !u.is_empty() && fm.measure(&u) + fm.measure(&dots) > max_w.max(8) {
            u.pop();
        }
        u.extend(dots);
    }
    let wd = fm.measure(&u);
    let x0 = match align {
        1 => x - wd / 2,
        2 => x - wd,
        _ => x,
    };
    let mut p = Pen::new(font, x0, y, 0, if focus { GREEN } else { colour }, Some(SHADOW));
    cv.text(t, &mut p, &u);
    if focus {
        let mid = y - if font == MenuFont::Bold { 7 } else { 6 };
        arrow(cv, x0 - 6, mid, true);
        arrow(cv, x0 + wd + 2, mid, false);
    }
    let _ = RED;
    wd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_in_plain_words() {
        assert_eq!(key_name("KeyX"), "X");
        assert_eq!(key_name("ArrowUp"), "Up");
        assert_eq!(key_name("Shift+F6"), "Shift+F6");
        assert_eq!(key_name("Backquote"), "`");
        assert_eq!(key_name("Shift+KeyR"), "Shift+R");
    }

    #[test]
    fn esc_always_backs_out() {
        let mut m = SysMenu::default();
        m.show(SysCtx { restart: true, quit_game: true, unsaved: true, ..Default::default() });
        // into Settings, Esc back to the main page, Esc closes
        m.cursor = 2;
        assert_eq!(m.key("Enter"), None);
        assert_eq!(m.page, Page::Settings);
        assert_eq!(m.key("Escape"), None);
        assert_eq!(m.page, Page::Main);
        assert_eq!(m.key("Escape"), Some(SysAction::Resume));
        assert!(!m.open);
        // quitting with unsaved progress asks first
        m.show(SysCtx { unsaved: true, ..Default::default() });
        let quit = m.rows().iter().position(|r| r.0 == Item::Quit).unwrap();
        m.cursor = quit;
        assert_eq!(m.key("Enter"), None);
        assert_eq!(m.page, Page::Confirm);
        assert_eq!(m.key("ArrowUp"), None);
        assert_eq!(m.key("Enter"), Some(SysAction::QuitToLauncher));
    }
}
