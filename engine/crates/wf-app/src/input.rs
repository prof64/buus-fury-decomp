//! Keyboard / gamepad -> GBA buttons. Key names are W3C `KeyboardEvent.code` strings (`ArrowUp`, `KeyX`,
//! `Enter`...); winit's `KeyCode` debug names are the same strings, so one bindings file serves both hosts.
//! Gamepads use the W3C "standard" mapping indices (0 = bottom face button ... 12-15 = d-pad).

use serde::{Deserialize, Serialize};
use wf_view::buttons as b;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Bindings {
    /// key code -> button name (`A`, `B`, `Select`, `Start`, `Right`, `Left`, `Up`, `Down`, `R`, `L`)
    pub keys: Vec<(String, String)>,
    /// standard-mapping gamepad button index -> button name
    pub pad: Vec<(u32, String)>,
    /// left-stick dead zone for the d-pad (0..1)
    pub stick_threshold: f32,
}

impl Default for Bindings {
    /// mGBA's defaults: arrows, X = A, Z = B, A = L, S = R, Enter = Start, Backspace = Select.
    fn default() -> Self {
        let k = |c: &str, n: &str| (c.to_string(), n.to_string());
        let p = |i: u32, n: &str| (i, n.to_string());
        Bindings {
            keys: vec![
                k("ArrowUp", "Up"),
                k("ArrowDown", "Down"),
                k("ArrowLeft", "Left"),
                k("ArrowRight", "Right"),
                k("KeyX", "A"),
                k("KeyZ", "B"),
                k("KeyA", "L"),
                k("KeyS", "R"),
                k("Enter", "Start"),
                k("Backspace", "Select"),
            ],
            pad: vec![
                p(0, "B"),
                p(1, "A"),
                p(2, "B"),
                p(3, "A"),
                p(4, "L"),
                p(5, "R"),
                p(6, "L"),
                p(7, "R"),
                p(8, "Select"),
                p(9, "Start"),
                p(12, "Up"),
                p(13, "Down"),
                p(14, "Left"),
                p(15, "Right"),
            ],
            stick_threshold: 0.5,
        }
    }
}

pub fn button_bit(name: &str) -> u16 {
    b::NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| 1 << i).unwrap_or(0)
}

impl Bindings {
    pub fn from_json(s: &str) -> Option<Bindings> {
        serde_json::from_str(s).ok()
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn key(&self, code: &str) -> u16 {
        self.keys.iter().filter(|(c, _)| c == code).map(|(_, n)| button_bit(n)).fold(0, |a, x| a | x)
    }

    /// Rebind: `button` now comes from `code` only (other keys bound to that button are removed; `code`'s
    /// other bindings stay unless `exclusive`).
    pub fn rebind_key(&mut self, button: &str, code: &str) {
        self.keys.retain(|(c, n)| !n.eq_ignore_ascii_case(button) && c != code);
        self.keys.push((code.to_string(), button.to_string()));
    }

    /// Buttons from a gamepad: pressed standard buttons (bit i = button i) and the left stick.
    pub fn pad(&self, pressed: u32, axis_x: f32, axis_y: f32) -> u16 {
        let mut out = 0;
        for (i, n) in &self.pad {
            if *i < 32 && pressed & (1 << i) != 0 {
                out |= button_bit(n);
            }
        }
        let t = self.stick_threshold;
        if axis_x < -t {
            out |= b::LEFT;
        }
        if axis_x > t {
            out |= b::RIGHT;
        }
        if axis_y < -t {
            out |= b::UP;
        }
        if axis_y > t {
            out |= b::DOWN;
        }
        out
    }
}

/// Held keys -> buttons for one local player.
#[derive(Default, Clone, Debug)]
pub struct KeyState {
    held: Vec<String>,
    /// keys pressed since the last step (a tap shorter than a frame still reaches the game once)
    tapped: Vec<String>,
}

impl KeyState {
    pub fn set(&mut self, code: &str, down: bool) {
        self.held.retain(|c| c != code);
        if down {
            self.held.push(code.to_string());
            if !self.tapped.iter().any(|c| c == code) {
                self.tapped.push(code.to_string());
            }
        }
    }
    pub fn clear(&mut self) {
        self.held.clear();
        self.tapped.clear();
    }
    /// Call after a step consumed the inputs.
    pub fn consumed(&mut self) {
        self.tapped.clear();
    }
    pub fn buttons(&self, bind: &Bindings) -> u16 {
        let mut v = self.held.iter().chain(&self.tapped).fold(0, |a, c| a | bind.key(c));
        // the GBA d-pad cannot report opposite directions
        if v & b::LEFT != 0 && v & b::RIGHT != 0 {
            v &= !(b::LEFT | b::RIGHT);
        }
        if v & b::UP != 0 && v & b::DOWN != 0 {
            v &= !(b::UP | b::DOWN);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_rebind() {
        let mut bi = Bindings::default();
        assert_eq!(bi.key("KeyX"), b::A);
        assert_eq!(bi.key("Backspace"), b::SELECT);
        bi.rebind_key("A", "KeyK");
        assert_eq!(bi.key("KeyX"), 0);
        assert_eq!(bi.key("KeyK"), b::A);
        let s = bi.to_json();
        assert_eq!(Bindings::from_json(&s).unwrap(), bi);
        let mut k = KeyState::default();
        k.set("ArrowLeft", true);
        k.set("ArrowRight", true);
        assert_eq!(k.buttons(&bi), 0);
    }
}
