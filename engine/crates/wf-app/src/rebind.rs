//! In-game controls screen (desktop shell; the web shell has its own HTML panel): pick a GBA button, press the
//! new key. Platform independent: hosts feed it key codes (W3C `code` names) and draw it over the frame.
//!
//! Navigation: Up / Down (arrow keys, always — so a bad binding can never lock you out), Enter = rebind the
//! selected button / run the selected action, Escape = cancel a pending rebind or close.

use wf_content::Packs;
use wf_render::font as tiny;
use wf_render::ui::Font;

use crate::input::Bindings;

pub const BUTTONS: [&str; 10] = ["Up", "Down", "Left", "Right", "A", "B", "L", "R", "Start", "Select"];
const ROWS: usize = BUTTONS.len() + 2; // + Reset, Done

#[derive(Default)]
pub struct RebindMenu {
    pub open: bool,
    pub cursor: usize,
    /// waiting for the key for `BUTTONS[cursor]`
    pub waiting: bool,
    font: Option<Option<Font>>,
}

/// What the host should do after a key.
#[derive(Debug, PartialEq, Eq)]
pub enum MenuResult {
    None,
    /// bindings changed: save them
    Changed,
    Closed,
}

pub fn key_label(code: &str) -> String {
    match code {
        "ArrowUp" => "Up arrow".into(),
        "ArrowDown" => "Down arrow".into(),
        "ArrowLeft" => "Left arrow".into(),
        "ArrowRight" => "Right arrow".into(),
        c => c.trim_start_matches("Key").trim_start_matches("Digit").to_string(),
    }
}

impl RebindMenu {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.waiting = false;
    }

    /// A key went down while the menu is open.
    pub fn key(&mut self, code: &str, b: &mut Bindings) -> MenuResult {
        if self.waiting {
            self.waiting = false;
            if code == "Escape" {
                return MenuResult::None;
            }
            b.rebind_key(BUTTONS[self.cursor], code);
            self.cursor = (self.cursor + 1).min(ROWS - 1);
            return MenuResult::Changed;
        }
        match code {
            "ArrowUp" => self.cursor = (self.cursor + ROWS - 1) % ROWS,
            "ArrowDown" => self.cursor = (self.cursor + 1) % ROWS,
            "Escape" => {
                self.open = false;
                return MenuResult::Closed;
            }
            "Enter" | "Space" | "NumpadEnter" => {
                if self.cursor < BUTTONS.len() {
                    self.waiting = true;
                } else if self.cursor == BUTTONS.len() {
                    *b = Bindings::default();
                    return MenuResult::Changed;
                } else {
                    self.open = false;
                    return MenuResult::Closed;
                }
            }
            _ => {}
        }
        MenuResult::None
    }

    /// Draw over an ARGB view framebuffer (centred 240x160 panel), with the game's `menu` font when available.
    pub fn draw(&mut self, packs: &Packs, fb: &mut [u32], w: usize, h: usize, b: &Bindings) {
        if !self.open {
            return;
        }
        if self.font.is_none() {
            self.font = Some(Font::load(packs, "menu").ok());
        }
        // dim the game
        for p in fb.iter_mut() {
            *p = 0xFF00_0000 | ((*p >> 2) & 0x003F_3F3F);
        }
        let (ox, oy) = ((w as i32 - 240) / 2, (h as i32 - 160) / 2);
        tiny::fill_rect(fb, w, h, ox + 8, oy + 4, 224, 152, 0xFF10_1820);
        tiny::fill_rect(fb, w, h, ox + 8, oy + 4, 224, 1, 0xFFF0_A030);
        tiny::fill_rect(fb, w, h, ox + 8, oy + 155, 224, 1, 0xFFF0_A030);
        let font = self.font.as_ref().and_then(|f| f.as_ref());
        let text = |fb: &mut [u32], s: &str, x: i32, y: i32, c: u32| match font {
            Some(f) => {
                f.draw_argb(fb, w, h, s, x, y + 9, c);
            }
            None => tiny::draw_text(fb, w, h, x, y, s, c),
        };
        text(fb, "CONTROLS", ox + 16, oy + 8, 0xFFF0_A030);
        for row in 0..ROWS {
            let y = oy + 22 + row as i32 * 11;
            let sel = row == self.cursor;
            if sel {
                tiny::fill_rect(fb, w, h, ox + 12, y - 1, 216, 11, 0xFF2E_3A48);
            }
            let col = if sel { 0xFFFF_FFFF } else { 0xFFB8_B8B8 };
            if row < BUTTONS.len() {
                let name = BUTTONS[row];
                text(fb, name, ox + 18, y, col);
                let keys: Vec<String> = b.keys.iter().filter(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(k, _)| key_label(k)).collect();
                let v = if sel && self.waiting { "press a key...".to_string() } else if keys.is_empty() { "-".into() } else { keys.join(", ") };
                text(fb, &v, ox + 90, y, if sel && self.waiting { 0xFFF0_A030 } else { col });
            } else if row == BUTTONS.len() {
                text(fb, "Reset to defaults", ox + 18, y, col);
            } else {
                text(fb, "Done", ox + 18, y, col);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebind_flow() {
        let mut m = RebindMenu { open: true, ..Default::default() };
        let mut b = Bindings::default();
        m.cursor = 4; // A
        assert_eq!(m.key("Enter", &mut b), MenuResult::None);
        assert!(m.waiting);
        assert_eq!(m.key("KeyK", &mut b), MenuResult::Changed);
        assert_eq!(b.key("KeyK"), wf_view::buttons::A);
        assert_eq!(b.key("KeyX"), 0);
        assert_eq!(m.cursor, 5);
        m.cursor = BUTTONS.len();
        assert_eq!(m.key("Enter", &mut b), MenuResult::Changed);
        assert_eq!(b, Bindings::default());
        assert_eq!(m.key("Escape", &mut b), MenuResult::Closed);
        assert!(!m.open);
    }
}
