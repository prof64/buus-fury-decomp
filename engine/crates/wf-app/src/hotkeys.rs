//! Emulator-style hotkeys (rewind, fast forward, pause / frame advance, save state slots, replays, training
//! overlays), rebindable, same key names as [`crate::input::Bindings`] (W3C `code`) plus modifiers:
//! `"Shift+F6"`, `"Ctrl+KeyR"`, `"Backquote"`.
//!
//! Defaults avoid mGBA's game keys (arrows, X, Z, A, S, Enter, Backspace) and follow mGBA where it has an
//! equivalent: hold ` (Backquote) to rewind, hold Tab to fast forward. Save state slots are F6..F9 (load) and
//! Shift+F6..F9 (save): mGBA's plain = load / Shift = save convention, moved past F1..F5 which this app already
//! uses (view, zoom, stats, training, controls). A key that is bound to a GBA button is never a hotkey without a
//! modifier (the game binding wins).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// hold
    Rewind,
    /// hold
    FastForward,
    Pause,
    FrameAdvance,
    Slower,
    Faster,
    SeekBack,
    SeekForward,
    Restart,
    SaveState(u8),
    LoadState(u8),
    Replays,
    /// take over a replay at the current frame (branch)
    TakeOver,
    InputDisplay,
    Training,
    Ghost,
}

pub const SLOTS: u8 = 4;

impl Action {
    pub fn is_hold(self) -> bool {
        matches!(self, Action::Rewind | Action::FastForward)
    }
    pub fn name(self) -> String {
        match self {
            Action::Rewind => "rewind".into(),
            Action::FastForward => "fast_forward".into(),
            Action::Pause => "pause".into(),
            Action::FrameAdvance => "frame_advance".into(),
            Action::Slower => "slower".into(),
            Action::Faster => "faster".into(),
            Action::SeekBack => "seek_back".into(),
            Action::SeekForward => "seek_forward".into(),
            Action::Restart => "restart".into(),
            Action::SaveState(n) => format!("save_state_{n}"),
            Action::LoadState(n) => format!("load_state_{n}"),
            Action::Replays => "replays".into(),
            Action::TakeOver => "take_over".into(),
            Action::InputDisplay => "input_display".into(),
            Action::Training => "training".into(),
            Action::Ghost => "ghost".into(),
        }
    }
    pub fn parse(s: &str) -> Option<Action> {
        if let Some(n) = s.strip_prefix("save_state_") {
            return n.parse().ok().filter(|n| (1..=SLOTS).contains(n)).map(Action::SaveState);
        }
        if let Some(n) = s.strip_prefix("load_state_") {
            return n.parse().ok().filter(|n| (1..=SLOTS).contains(n)).map(Action::LoadState);
        }
        Action::all().into_iter().find(|a| a.name() == s)
    }
    pub fn all() -> Vec<Action> {
        let mut v = vec![
            Action::Rewind,
            Action::FastForward,
            Action::Pause,
            Action::FrameAdvance,
            Action::Slower,
            Action::Faster,
            Action::SeekBack,
            Action::SeekForward,
            Action::Restart,
        ];
        v.extend((1..=SLOTS).map(Action::SaveState));
        v.extend((1..=SLOTS).map(Action::LoadState));
        v.extend([Action::Replays, Action::TakeOver, Action::InputDisplay, Action::Training, Action::Ghost]);
        v
    }
    /// Label for settings screens.
    pub fn label(self) -> String {
        match self {
            Action::Rewind => "Rewind (hold)".into(),
            Action::FastForward => "Fast forward (hold)".into(),
            Action::Pause => "Pause / resume".into(),
            Action::FrameAdvance => "Frame advance".into(),
            Action::Slower => "Slower".into(),
            Action::Faster => "Faster".into(),
            Action::SeekBack => "Seek -5 s".into(),
            Action::SeekForward => "Seek +5 s".into(),
            Action::Restart => "Back to start".into(),
            Action::SaveState(n) => format!("Save state {n}"),
            Action::LoadState(n) => format!("Load state {n}"),
            Action::Replays => "Replays".into(),
            Action::TakeOver => "Take over replay".into(),
            Action::InputDisplay => "Input display".into(),
            Action::Training => "Training overlay".into(),
            Action::Ghost => "Ghost on / off".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    /// Ctrl or Cmd
    pub ctrl: bool,
    pub alt: bool,
}

impl Mods {
    pub fn any(self) -> bool {
        self.shift || self.ctrl || self.alt
    }
    /// bit 0 shift, 1 ctrl/meta, 2 alt (wasm boundary)
    pub fn from_bits(b: u32) -> Mods {
        Mods { shift: b & 1 != 0, ctrl: b & 2 != 0, alt: b & 4 != 0 }
    }
}

/// `"Shift+F6"` -> (mods, code)
pub fn parse_combo(s: &str) -> (Mods, String) {
    let mut m = Mods::default();
    let mut code = String::new();
    for part in s.split('+') {
        match part {
            "Shift" => m.shift = true,
            "Ctrl" | "Meta" | "Cmd" => m.ctrl = true,
            "Alt" => m.alt = true,
            c => code = c.to_string(),
        }
    }
    (m, code)
}

pub fn combo(m: Mods, code: &str) -> String {
    let mut s = String::new();
    if m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    s.push_str(code);
    s
}

/// Modifier keys themselves (never bound alone).
pub fn is_modifier(code: &str) -> bool {
    matches!(code, "ShiftLeft" | "ShiftRight" | "ControlLeft" | "ControlRight" | "AltLeft" | "AltRight" | "MetaLeft" | "MetaRight" | "SuperLeft" | "SuperRight")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hotkeys {
    /// (combo, action name)
    pub keys: Vec<(String, String)>,
    /// Rewind window (seconds of play kept as dense snapshots).
    #[serde(default = "d_rewind")]
    pub rewind_seconds: f64,
    /// Fast-forward multiplier while the key is held.
    #[serde(default = "d_ff")]
    pub fast_forward: f64,
    /// Automatic recordings kept (oldest pruned; kept / starred ones never).
    #[serde(default = "d_keep")]
    pub keep_replays: usize,
}
fn d_rewind() -> f64 {
    60.0
}
fn d_ff() -> f64 {
    4.0
}
fn d_keep() -> usize {
    50
}

impl Default for Hotkeys {
    fn default() -> Self {
        let mut keys: Vec<(String, String)> = [
            ("Backquote", "rewind"),
            ("Tab", "fast_forward"),
            ("KeyP", "pause"),
            ("KeyN", "frame_advance"),
            ("Minus", "slower"),
            ("Equal", "faster"),
            ("BracketLeft", "seek_back"),
            ("BracketRight", "seek_forward"),
            // (no Home key on a Mac laptop)
            ("Shift+KeyR", "restart"),
            ("F10", "replays"),
            ("KeyB", "take_over"),
            ("KeyI", "input_display"),
            ("F4", "training"),
            ("KeyG", "ghost"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        for n in 1..=SLOTS {
            keys.push((format!("F{}", 5 + n), format!("load_state_{n}")));
            keys.push((format!("Shift+F{}", 5 + n), format!("save_state_{n}")));
        }
        Hotkeys { keys, rewind_seconds: d_rewind(), fast_forward: d_ff(), keep_replays: d_keep() }
    }
}

impl Hotkeys {
    pub fn from_json(s: &str) -> Option<Hotkeys> {
        serde_json::from_str(s).ok()
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    /// The action for a key press. `game_bound`: the key drives a GBA button (then only modified combos match).
    pub fn lookup(&self, code: &str, m: Mods, game_bound: bool) -> Option<Action> {
        if game_bound && !m.any() {
            return None;
        }
        self.keys.iter().find(|(c, _)| parse_combo(c) == (m, code.to_string())).and_then(|(_, a)| Action::parse(a))
    }
    /// The hold action a released key ends (modifiers ignored: the modifier may be released first).
    pub fn lookup_release(&self, code: &str) -> Option<Action> {
        self.keys.iter().filter(|(c, _)| parse_combo(c).1 == code).filter_map(|(_, a)| Action::parse(a)).find(|a| a.is_hold())
    }
    pub fn keys_for(&self, a: Action) -> Vec<String> {
        let n = a.name();
        self.keys.iter().filter(|(_, x)| *x == n).map(|(c, _)| c.clone()).collect()
    }
    /// Keys saved before a default changed: Home (restart) - a key Mac laptops lack - becomes Shift+R.
    pub fn migrate(&mut self) {
        let taken = self.keys.iter().any(|(c, _)| c == "Shift+KeyR");
        for (c, a) in self.keys.iter_mut() {
            if c == "Home" && a == "restart" && !taken {
                *c = "Shift+KeyR".into();
            }
        }
    }

    /// `action` now comes from `combo` only (the combo is taken from any other action).
    pub fn rebind(&mut self, action: Action, combo: &str) {
        let n = action.name();
        self.keys.retain(|(c, a)| *a != n && c != combo);
        self.keys.push((combo.to_string(), n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults() {
        let h = Hotkeys::default();
        let none = Mods::default();
        let shift = Mods { shift: true, ..Default::default() };
        assert_eq!(h.lookup("Backquote", none, false), Some(Action::Rewind));
        assert_eq!(h.lookup("F6", none, false), Some(Action::LoadState(1)));
        assert_eq!(h.lookup("F9", shift, false), Some(Action::SaveState(4)));
        assert_eq!(h.lookup("KeyP", none, true), None);
        assert_eq!(h.lookup_release("Tab"), Some(Action::FastForward));
        // no default hotkey is an mGBA game key
        let game = crate::input::Bindings::default();
        for (c, _) in &h.keys {
            let (m, code) = parse_combo(c);
            assert!(m.any() || game.key(&code) == 0, "{c} is a game key");
        }
        let mut h2 = h.clone();
        h2.rebind(Action::Rewind, "KeyR");
        assert_eq!(h2.lookup("KeyR", none, false), Some(Action::Rewind));
        assert_eq!(h2.lookup("Backquote", none, false), None);
        assert_eq!(Hotkeys::from_json(&h2.to_json()).unwrap(), h2);
        for a in Action::all() {
            assert_eq!(Action::parse(&a.name()), Some(a));
        }
    }
}
