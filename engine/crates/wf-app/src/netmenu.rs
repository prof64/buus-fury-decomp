//! The in-game online lobby (native and browser shells; drawn into a framebuffer with the game's `menu` font):
//! host a room (mode, players, enemy, pause rule, input delay) or join / watch one by its invite code, pick fighter
//! and form, toggle ready, start (host), rejoin the last match, leave. Platform independent: hosts feed it key codes
//! (W3C `code` names) and typed text, draw it, and act on the [`MenuAction`]s it returns (open the lobby on their
//! transport, pass ready / fighter / start to it). The room screen reads the live [`NetStatus`] (roster, invite).

use serde::{Deserialize, Serialize};
use wf_content::Packs;
use wf_net::netsim::NetStatus;
use wf_render::font as tiny;
use wf_render::ui::Font;

/// (enemy id, name, arena) the host can pick (the enemies' game names; open arenas - the forest's tree tops hid the
/// fighters, playtest #6)
pub const ENEMIES: &[(&str, &str, &str)] = &[
    ("t68", "Majin Fighters", "crater"),
    ("t60", "Kid Buu", "wasteland"),
    ("t51", "Janemba", "crater"),
    ("t26+~t29", "Majin Buu & Babidi", "crater"),
    ("t129", "Pharoah Totenhotep", "crater"),
    ("t145", "Yakon", "yakon"),
    ("t10", "Broly", "volcano"),
    ("t18", "Dabura", "demon"),
];
const MODES: [&str; 3] = ["Co-op vs enemies", "PvP", "PvP + enemies"];

/// What the player picked last time (hosts persist it: a file natively, localStorage in the browser).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OnlinePrefs {
    pub name: String,
    pub fighter: String,
    pub form: i32,
    /// signaling server, `ws://` or `wss://`
    pub signal: String,
    /// 0 co-op, 1 PvP, 2 PvP + enemies
    pub mode: u8,
    pub players: u8,
    pub enemy: String,
    pub count: u32,
    /// "local" | "shared"
    pub pause: String,
    pub delay: u32,
    /// the last invite code typed
    pub code: String,
}

impl Default for OnlinePrefs {
    fn default() -> Self {
        OnlinePrefs {
            name: String::new(),
            fighter: "goku".into(),
            form: 2,
            signal: "ws://127.0.0.1:3536".into(),
            mode: 0,
            players: 2,
            enemy: "t60".into(),
            count: 1,
            pause: "local".into(),
            delay: 2,
            code: String::new(),
        }
    }
}

impl OnlinePrefs {
    pub fn arena(&self) -> &'static str {
        ENEMIES.iter().find(|e| e.0 == self.enemy).map(|e| e.2).unwrap_or("crater")
    }
    pub fn pvp(&self) -> bool {
        self.mode != 0
    }
    pub fn pvp_enemies(&self) -> bool {
        self.mode == 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Host,
    /// enter a code (to join, or to watch)
    Code { watch: bool },
    /// the room / waiting for the match
    Room,
    /// in a running online match (Esc): the match keeps running behind it
    InMatch,
    Message,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuAction {
    None,
    /// open a room with `prefs`
    Host,
    Join { code: String, spectate: bool },
    Rejoin,
    SetReady(bool),
    SetFighter { fighter: String, form: i32 },
    Start,
    /// leave the room / the match
    Leave,
    /// close the menu (back to the game)
    Close,
    /// prefs changed (save them)
    Saved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    HostRoom,
    JoinRoom,
    Watch,
    Rejoin,
    Name,
    Fighter,
    Form,
    Server,
    Close,
    Mode,
    Players,
    Enemy,
    Pause,
    Delay,
    Create,
    Back,
    Code,
    Go,
    Ready,
    StartMatch,
    Leave,
    Resume,
    LeaveMatch,
    Ok,
}

pub struct OnlineMenu {
    pub open: bool,
    pub screen: Screen,
    cursor: usize,
    pub prefs: OnlinePrefs,
    /// a rejoin ticket exists (offer "Rejoin last match")
    pub has_ticket: bool,
    /// editing this text row (typed text goes into `buf`)
    editing: Option<Row>,
    buf: String,
    pub ready: bool,
    /// we host the room on screen
    pub hosting: bool,
    pub spectating: bool,
    pub message: String,
    font: Option<Option<Font>>,
    /// a blink counter for the text cursor
    t: u32,
}

impl Default for OnlineMenu {
    fn default() -> Self {
        OnlineMenu { open: false, screen: Screen::Main, cursor: 0, prefs: OnlinePrefs::default(), has_ticket: false, editing: None, buf: String::new(), ready: false, hosting: false, spectating: false, message: String::new(), font: None, t: 0 }
    }
}

/// fighters added by content mods (`battle/fighters/*.json`), set when the shells load game data ([`set_roster`])
static ROSTER: std::sync::Mutex<Vec<(String, Vec<i32>)>> = std::sync::Mutex::new(Vec::new());

/// the lobby's fighter list: the game's fighters, then the content's own (mods) - call after loading game data
pub fn set_roster(d: &wf_net::wf_core::content::GameData) {
    let builtin: Vec<&str> = wf_net::wf_core::battle::FIGHTERS.iter().map(|f| f.0).collect();
    let extra = d.fighters.iter().filter(|f| !builtin.contains(&f.id.as_str())).map(|f| (f.id.clone(), f.forms.clone())).collect();
    if let Ok(mut r) = ROSTER.lock() {
        *r = extra;
    }
}

fn fighters() -> Vec<(String, Vec<i32>)> {
    let mut v: Vec<(String, Vec<i32>)> = wf_net::wf_core::battle::FIGHTERS.iter().filter(|f| !f.0.starts_with('f')).map(|f| (f.0.to_string(), f.3.to_vec())).collect();
    if let Ok(r) = ROSTER.lock() {
        v.extend(r.iter().cloned());
    }
    v
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => String::new(),
    }
}

impl OnlineMenu {
    pub fn show(&mut self, screen: Screen) {
        self.open = true;
        self.screen = screen;
        self.cursor = 0;
        self.editing = None;
    }
    /// an error / notice screen
    pub fn say(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.show(Screen::Message);
    }
    /// the room opened (hosting or joining)
    pub fn enter_room(&mut self, hosting: bool, spectating: bool) {
        self.hosting = hosting;
        self.spectating = spectating;
        self.ready = hosting;
        self.show(Screen::Room);
    }
    pub fn editing(&self) -> bool {
        self.editing.is_some()
    }

    fn rows(&self) -> Vec<Row> {
        use Row::*;
        match self.screen {
            Screen::Main => {
                let mut v = vec![HostRoom, JoinRoom, Watch];
                if self.has_ticket {
                    v.push(Rejoin);
                }
                v.extend([Name, Fighter, Form, Server, Close]);
                v
            }
            Screen::Host => vec![Mode, Players, Enemy, Pause, Delay, Create, Back],
            Screen::Code { .. } => vec![Code, Go, Back],
            Screen::Room => {
                let mut v = Vec::new();
                if !self.hosting && !self.spectating {
                    v.push(Ready);
                }
                if !self.spectating {
                    v.extend([Fighter, Form]);
                }
                if self.hosting {
                    v.push(StartMatch);
                }
                v.push(Leave);
                v
            }
            Screen::InMatch => vec![Resume, LeaveMatch],
            Screen::Message => vec![Ok],
        }
    }

    fn fighter_idx(&self) -> usize {
        fighters().iter().position(|f| f.0 == self.prefs.fighter).unwrap_or(0)
    }

    fn cycle(&mut self, row: Row, d: i32) -> MenuAction {
        let p = &mut self.prefs;
        let wrap = |v: i32, n: i32| ((v % n) + n) % n;
        match row {
            Row::Fighter => {
                let fs = fighters();
                let i = wrap(self.fighter_idx() as i32 + d, fs.len() as i32) as usize;
                let p = &mut self.prefs;
                p.fighter = fs[i].0.clone();
                p.form = fs[i].1[0];
                return self.fighter_changed();
            }
            Row::Form => {
                let fs = fighters();
                let forms = &fs[self.fighter_idx()].1;
                let k = forms.iter().position(|&f| f == self.prefs.form).unwrap_or(0) as i32;
                self.prefs.form = forms[wrap(k + d, forms.len() as i32) as usize];
                return self.fighter_changed();
            }
            Row::Mode => p.mode = wrap(p.mode as i32 + d, 3) as u8,
            Row::Players => p.players = (wrap(p.players as i32 - 2 + d, 3) + 2) as u8,
            Row::Enemy => {
                let k = ENEMIES.iter().position(|e| e.0 == p.enemy).unwrap_or(0) as i32;
                p.enemy = ENEMIES[wrap(k + d, ENEMIES.len() as i32) as usize].0.into();
            }
            Row::Pause => p.pause = if p.pause == "shared" { "local".into() } else { "shared".into() },
            Row::Delay => p.delay = wrap(p.delay as i32 + d, 9) as u32,
            Row::Ready => {
                self.ready = !self.ready;
                return MenuAction::SetReady(self.ready);
            }
            _ => return MenuAction::None,
        }
        MenuAction::Saved
    }

    fn fighter_changed(&self) -> MenuAction {
        if self.screen == Screen::Room {
            MenuAction::SetFighter { fighter: self.prefs.fighter.clone(), form: self.prefs.form }
        } else {
            MenuAction::Saved
        }
    }

    /// Typed text (the host's text input event; letters, digits, punctuation).
    pub fn text(&mut self, s: &str) {
        let Some(row) = self.editing else { return };
        for c in s.chars() {
            if c.is_control() {
                continue;
            }
            let ok = match row {
                Row::Code => c.is_ascii_alphanumeric() || c == '-',
                Row::Name => c.is_ascii_graphic() || c == ' ',
                _ => c.is_ascii_graphic(),
            };
            if ok && self.buf.len() < 120 {
                self.buf.push(if row == Row::Code { c.to_ascii_uppercase() } else { c });
            }
        }
    }

    /// A key went down while the menu is open.
    pub fn key(&mut self, code: &str) -> MenuAction {
        if let Some(row) = self.editing {
            match code {
                "Backspace" => {
                    self.buf.pop();
                }
                "Escape" => self.editing = None,
                "Enter" | "NumpadEnter" => {
                    self.editing = None;
                    let v = self.buf.trim().to_string();
                    match row {
                        Row::Name => self.prefs.name = v,
                        Row::Server => self.prefs.signal = v,
                        Row::Code => {
                            self.prefs.code = v;
                            return self.go();
                        }
                        _ => {}
                    }
                    return MenuAction::Saved;
                }
                _ => {}
            }
            return MenuAction::None;
        }
        let rows = self.rows();
        self.cursor = self.cursor.min(rows.len().saturating_sub(1));
        let row = rows[self.cursor];
        match code {
            "ArrowUp" | "KeyW" => self.cursor = (self.cursor + rows.len() - 1) % rows.len(),
            "ArrowDown" | "KeyS" => self.cursor = (self.cursor + 1) % rows.len(),
            "ArrowLeft" | "KeyA" => return self.cycle(row, -1),
            "ArrowRight" | "KeyD" => return self.cycle(row, 1),
            "Escape" | "Backspace" => {
                return match self.screen {
                    Screen::Main => {
                        self.open = false;
                        MenuAction::Close
                    }
                    Screen::Host | Screen::Code { .. } | Screen::Message => {
                        self.show(Screen::Main);
                        MenuAction::None
                    }
                    Screen::Room => {
                        self.show(Screen::Main);
                        MenuAction::Leave
                    }
                    Screen::InMatch => {
                        self.open = false;
                        MenuAction::Close
                    }
                };
            }
            "Enter" | "NumpadEnter" | "Space" | "KeyX" => return self.activate(row),
            _ => {}
        }
        MenuAction::None
    }

    fn go(&mut self) -> MenuAction {
        let watch = matches!(self.screen, Screen::Code { watch: true });
        if self.prefs.code.is_empty() {
            self.editing = Some(Row::Code);
            self.buf.clear();
            return MenuAction::None;
        }
        MenuAction::Join { code: self.prefs.code.clone(), spectate: watch }
    }

    fn activate(&mut self, row: Row) -> MenuAction {
        match row {
            Row::HostRoom => self.show(Screen::Host),
            Row::JoinRoom => {
                self.show(Screen::Code { watch: false });
                self.editing = Some(Row::Code);
                self.buf = self.prefs.code.clone();
            }
            Row::Watch => {
                self.show(Screen::Code { watch: true });
                self.editing = Some(Row::Code);
                self.buf = self.prefs.code.clone();
            }
            Row::Rejoin => return MenuAction::Rejoin,
            Row::Name | Row::Server | Row::Code => {
                self.editing = Some(row);
                self.buf = match row {
                    Row::Name => self.prefs.name.clone(),
                    Row::Server => self.prefs.signal.clone(),
                    _ => self.prefs.code.clone(),
                };
            }
            Row::Fighter | Row::Form | Row::Mode | Row::Players | Row::Enemy | Row::Pause | Row::Delay | Row::Ready => return self.cycle(row, 1),
            Row::Close | Row::Resume => {
                self.open = false;
                return MenuAction::Close;
            }
            Row::Create => return MenuAction::Host,
            Row::Back => self.show(Screen::Main),
            Row::Go => return self.go(),
            Row::StartMatch => return MenuAction::Start,
            Row::Leave | Row::LeaveMatch => {
                self.show(Screen::Main);
                return MenuAction::Leave;
            }
            Row::Ok => self.show(Screen::Main),
        }
        MenuAction::None
    }

    fn label(&self, row: Row) -> (String, String) {
        let p = &self.prefs;
        let fs = fighters();
        let fi = self.fighter_idx();
        let forms = &fs[fi].1;
        let fk = forms.iter().position(|&f| f == p.form).unwrap_or(0);
        let t = |s: &str| s.to_string();
        match row {
            Row::HostRoom => (t("Host a room"), String::new()),
            Row::JoinRoom => (t("Join a room"), String::new()),
            Row::Watch => (t("Watch a room"), String::new()),
            Row::Rejoin => (t("Rejoin last match"), String::new()),
            Row::Name => (t("Name"), if p.name.is_empty() { t("(random)") } else { p.name.clone() }),
            Row::Fighter => (t("Fighter"), format!("< {} >", cap(&fs[fi].0))),
            Row::Form => (t("Form"), format!("< {} ({}/{}) >", p.form, fk + 1, forms.len())),
            Row::Server => (t("Server"), p.signal.clone()),
            Row::Close => (t("Close"), String::new()),
            Row::Mode => (t("Mode"), format!("< {} >", MODES[p.mode.min(2) as usize])),
            Row::Players => (t("Players"), format!("< {} >", p.players)),
            Row::Enemy => (t("Enemy"), if p.mode == 1 { t("(none in PvP)") } else { format!("< {} >", ENEMIES.iter().find(|e| e.0 == p.enemy).map(|e| e.1).unwrap_or("?")) }),
            Row::Pause => (t("Menu pauses"), if p.pause == "shared" { t("< everyone >") } else { t("< only me >") }),
            Row::Delay => (t("Input delay"), format!("< {} frames >", p.delay)),
            Row::Create => (t("Create room"), String::new()),
            Row::Back => (t("Back"), String::new()),
            Row::Code => (t("Code"), p.code.clone()),
            Row::Go => (if matches!(self.screen, Screen::Code { watch: true }) { t("Watch") } else { t("Join") }, String::new()),
            Row::Ready => (t("Ready"), if self.ready { t("< yes >") } else { t("< no >") }),
            Row::StartMatch => (t("Start match"), String::new()),
            Row::Leave => (t("Leave room"), String::new()),
            Row::Resume => (t("Back to the match"), String::new()),
            Row::LeaveMatch => (t("Leave the match"), String::new()),
            Row::Ok => (t("OK"), String::new()),
        }
    }

    /// Draw over an ARGB framebuffer `w` x `h` (a 240 x 160 panel in the middle; dims what is under it).
    pub fn draw(&mut self, packs: &Packs, fb: &mut [u32], w: usize, h: usize, status: Option<&NetStatus>) {
        if !self.open {
            return;
        }
        self.t = self.t.wrapping_add(1);
        if self.font.is_none() {
            self.font = Some(Font::load(packs, "menu").ok());
        }
        for p in fb.iter_mut() {
            *p = 0xFF00_0000 | ((*p >> 2) & 0x003F_3F3F);
        }
        let (ox, oy) = ((w as i32 - 240) / 2, (h as i32 - 160) / 2);
        const ACCENT: u32 = 0xFFF0_A030;
        tiny::fill_rect(fb, w, h, ox + 4, oy + 2, 232, 156, 0xFF10_1820);
        tiny::fill_rect(fb, w, h, ox + 4, oy + 2, 232, 1, ACCENT);
        tiny::fill_rect(fb, w, h, ox + 4, oy + 157, 232, 1, ACCENT);
        let font = self.font.as_ref().and_then(|f| f.as_ref());
        let text = |fb: &mut [u32], s: &str, x: i32, y: i32, c: u32| match font {
            Some(f) => {
                f.draw_argb(fb, w, h, s, x, y + 9, c);
            }
            None => tiny::draw_text(fb, w, h, x, y, s, c),
        };
        let width = |s: &str| match font {
            Some(f) => f.width(s),
            None => tiny::text_width(s),
        };
        let title = match self.screen {
            Screen::Main => "PLAY ONLINE".to_string(),
            Screen::Host => "HOST A ROOM".to_string(),
            Screen::Code { watch: false } => "JOIN A ROOM".to_string(),
            Screen::Code { watch: true } => "WATCH A ROOM".to_string(),
            Screen::Room => {
                let inv = status.map(|s| s.invite.clone()).filter(|s| !s.is_empty()).or(status.map(|s| s.room.clone())).unwrap_or_default();
                format!("ROOM {inv}")
            }
            Screen::InMatch => "ONLINE MATCH".to_string(),
            Screen::Message => "ONLINE".to_string(),
        };
        text(fb, &title, ox + 10, oy + 5, ACCENT);
        let mut y = oy + 19;
        // the room: who is there
        if self.screen == Screen::Room {
            let st = status.cloned().unwrap_or_default();
            let line = match st.phase.as_str() {
                "connecting" | "" => "connecting to the server...".to_string(),
                "lobby" if self.hosting => format!("share the code - {} of {} players", st.players_present.max(1), st.want_players),
                "lobby" => "waiting for the host to start".to_string(),
                "starting" | "running" => "starting...".to_string(),
                "reconnecting" => st.rejoin.clone().unwrap_or_else(|| "rejoining...".into()),
                "error" => st.error.clone().unwrap_or_default(),
                "rejected" => st.outcome.clone(),
                p => p.to_string(),
            };
            tiny::draw_text(fb, w, h, ox + 10, y, &line, if matches!(st.phase.as_str(), "error" | "rejected") { 0xFFFF_7A6B } else { 0xFF9A_988F });
            y += 10;
            for e in st.lobby.iter().take(5) {
                let seat = match (e.seat, e.role.as_str()) {
                    (_, "spectator") => "--".to_string(),
                    (Some(s), _) => format!("P{}", s + 1),
                    (None, _) => "-".to_string(),
                };
                let c = if e.me { 0xFFFF_FFFF } else { 0xFFB8_B8B8 };
                text(fb, &seat, ox + 10, y, ACCENT);
                text(fb, &e.name.chars().take(12).collect::<String>(), ox + 40, y, c);
                if e.role == "spectator" {
                    text(fb, "watching", ox + 120, y, 0xFF9A_988F);
                } else {
                    text(fb, &format!("{} {}", cap(&e.fighter), e.form), ox + 120, y, c);
                    let r = if e.role == "host" { "HOST" } else if e.ready { "READY" } else { "..." };
                    text(fb, r, ox + 196, y, if e.ready || e.role == "host" { 0xFF7A_E07A } else { 0xFF9A_988F });
                }
                y += 11;
            }
            y = y.max(oy + 19 + 10 + 11 * 2) + 3;
            tiny::fill_rect(fb, w, h, ox + 8, y - 2, 224, 1, 0xFF2E_3A48);
        }
        if self.screen == Screen::Message || (self.screen == Screen::InMatch && !self.message.is_empty()) {
            // wrap the message at the panel width
            let mut line = String::new();
            for word in self.message.split(' ') {
                if width(&format!("{line} {word}")) > 215 && !line.is_empty() {
                    text(fb, &line, ox + 10, y, 0xFFEC_EBE7);
                    y += 11;
                    line.clear();
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            text(fb, &line, ox + 10, y, 0xFFEC_EBE7);
            y += 15;
        }
        let rows = self.rows();
        self.cursor = self.cursor.min(rows.len().saturating_sub(1));
        for (k, &row) in rows.iter().enumerate() {
            if y > oy + 140 {
                break;
            }
            let sel = k == self.cursor;
            if sel {
                tiny::fill_rect(fb, w, h, ox + 8, y - 1, 224, 11, 0xFF2E_3A48);
            }
            let col = if sel { 0xFFFF_FFFF } else { 0xFFB8_B8B8 };
            let (l, mut v) = self.label(row);
            if self.editing == Some(row) {
                v = format!("{}{}", self.buf, if self.t / 20 % 2 == 0 { "_" } else { " " });
            }
            text(fb, &l, ox + 14, y, col);
            if !v.is_empty() {
                // long values (server URLs) show their end
                let mut v = v;
                while width(&v) > 132 && v.len() > 4 {
                    v.remove(0);
                }
                text(fb, &v, ox + 96, y, if self.editing == Some(row) { ACCENT } else { col });
            }
            y += 11;
        }
        let hint = if self.editing.is_some() { "type  ENTER ok  ESC cancel" } else { "ENTER ok  <> change  ESC back" };
        tiny::draw_text(fb, w, h, ox + 10, oy + 147, hint, 0xFF6A_6870);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_and_join_flow() {
        let mut m = OnlineMenu::default();
        m.show(Screen::Main);
        assert_eq!(m.key("Enter"), MenuAction::None);
        assert_eq!(m.screen, Screen::Host);
        assert_eq!(m.key("ArrowRight"), MenuAction::Saved); // mode -> PvP
        assert!(m.prefs.pvp());
        for _ in 0..5 {
            m.key("ArrowDown");
        }
        assert_eq!(m.key("Enter"), MenuAction::Host);
        m.enter_room(true, false);
        assert_eq!(m.key("ArrowRight"), MenuAction::SetFighter { fighter: "gohan".into(), form: 10 });
        m.show(Screen::Main);
        m.key("ArrowDown");
        assert_eq!(m.key("Enter"), MenuAction::None);
        assert!(m.editing());
        m.text("kqzta-7f3k2q");
        assert_eq!(m.key("Enter"), MenuAction::Join { code: "KQZTA-7F3K2Q".into(), spectate: false });
        m.enter_room(false, false);
        assert_eq!(m.key("Enter"), MenuAction::SetReady(true));
        assert_eq!(m.key("Escape"), MenuAction::Leave);
        assert_eq!(m.screen, Screen::Main);
    }
}
