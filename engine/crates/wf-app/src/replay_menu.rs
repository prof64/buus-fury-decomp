//! In-game replay browser (desktop shell; the web shell has an HTML panel). The host lists the files, the menu
//! picks one and returns a command; the host does the file work and calls [`crate::control::Control`].

use wf_content::Packs;
use wf_replay::Header;

use crate::overlay::{self, Text};

#[derive(Clone, Debug)]
pub struct ReplayEntry {
    pub id: String,
    pub header: Header,
    /// kept entries are never pruned
    pub kept: bool,
    pub size: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplayCmd {
    None,
    Play(String),
    Ghost(String),
    Verify(String),
    Keep(String),
    Delete(String),
    Export(String),
    /// save the live recording now under a chosen name
    ExportCurrent,
    Import,
    Close,
}

#[derive(Default)]
pub struct ReplayMenu {
    pub open: bool,
    pub cursor: usize,
    pub entries: Vec<ReplayEntry>,
    /// result line (verification, errors)
    pub info: String,
    text: Text,
}

/// `YYYY-MM-DD HH:MM` (UTC) from Unix seconds, without a date library.
pub fn date(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // civil from days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, secs / 60 % 60)
}

pub fn summary(e: &ReplayEntry) -> String {
    let h = &e.header;
    let secs = h.frames as f64 / wf_view::GBA_FPS;
    let mut s = format!("{} {} {}:{:02}", date(h.created), h.title, (secs / 60.0) as u64, secs as u64 % 60);
    if !h.outcome.is_empty() {
        s.push(' ');
        s.push_str(&h.outcome);
    }
    if h.branch.is_some() {
        s.push_str(" (branch)");
    }
    if e.kept {
        s.push_str(" *");
    }
    s
}

impl ReplayMenu {
    pub fn show(&mut self, entries: Vec<ReplayEntry>) {
        self.entries = entries;
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
        self.open = true;
    }

    pub fn key(&mut self, code: &str) -> ReplayCmd {
        let n = self.entries.len();
        let id = self.entries.get(self.cursor).map(|e| e.id.clone());
        let on = |f: fn(String) -> ReplayCmd| id.clone().map(f).unwrap_or(ReplayCmd::None);
        match code {
            "ArrowUp" => self.cursor = self.cursor.saturating_sub(1),
            "ArrowDown" => self.cursor = (self.cursor + 1).min(n.saturating_sub(1)),
            "PageUp" => self.cursor = self.cursor.saturating_sub(10),
            "PageDown" => self.cursor = (self.cursor + 10).min(n.saturating_sub(1)),
            "Enter" | "NumpadEnter" => {
                let c = on(ReplayCmd::Play);
                if c != ReplayCmd::None {
                    self.open = false;
                }
                return c;
            }
            "KeyG" => {
                let c = on(ReplayCmd::Ghost);
                if c != ReplayCmd::None {
                    self.open = false;
                }
                return c;
            }
            "KeyV" => return on(ReplayCmd::Verify),
            "KeyK" => return on(ReplayCmd::Keep),
            "Delete" | "KeyD" => return on(ReplayCmd::Delete),
            "KeyE" => return on(ReplayCmd::Export),
            "KeyC" => return ReplayCmd::ExportCurrent,
            "KeyO" => return ReplayCmd::Import,
            "Escape" | "F10" => {
                self.open = false;
                return ReplayCmd::Close;
            }
            _ => {}
        }
        ReplayCmd::None
    }

    /// Draw over a `w` x `h` ARGB view.
    pub fn draw(&mut self, packs: &Packs, fb: &mut [u32], w: usize, h: usize) {
        if !self.open || fb.len() < w * h {
            return;
        }
        self.text.ensure(packs);
        let t = &self.text;
        let lh = t.line_h() + 1;
        overlay::dim_rect(fb, w, h, 0, 0, w as i32, h as i32);
        let (x0, y0) = (12, 8);
        t.draw(fb, w, h, x0, y0, &format!("REPLAYS ({})", self.entries.len()), overlay::ACCENT);
        let rows = ((h as i32 - y0 - 4 * lh - 8) / lh).max(1) as usize;
        let first = self.cursor.saturating_sub(rows - 1);
        if self.entries.is_empty() {
            t.draw(fb, w, h, x0, y0 + 2 * lh, "No recordings yet: every fight is recorded automatically.", 0xFFB8_B8B8);
        }
        for (k, e) in self.entries.iter().enumerate().skip(first).take(rows) {
            let y = y0 + lh + 4 + (k - first) as i32 * lh;
            let sel = k == self.cursor;
            if sel {
                wf_render::font::fill_rect(fb, w, h, x0 - 4, y - 1, w as i32 - 2 * x0 + 8, lh, 0xFF2E_3A48);
            }
            t.draw(fb, w, h, x0, y, &summary(e), if sel { overlay::WHITE } else { 0xFFB8_B8B8 });
        }
        let fy = h as i32 - 3 * lh - 4;
        if !self.info.is_empty() {
            t.draw(fb, w, h, x0, fy, &self.info, overlay::ACCENT);
        }
        t.draw(fb, w, h, x0, fy + lh, "Enter play  G ghost  V verify  K keep  D delete", 0xFFB8_B8B8);
        t.draw(fb, w, h, x0, fy + 2 * lh, "E export  C save current  O import  Esc close", 0xFFB8_B8B8);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates() {
        assert_eq!(super::date(0), "1970-01-01 00:00");
        assert_eq!(super::date(1_790_000_000), "2026-09-21 14:13");
    }
}
