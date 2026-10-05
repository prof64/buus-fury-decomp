//! The launcher: the first screen of the packaged game (native app and web build). Title art from the player's own
//! content (`ui/images/screen_title.png`), the game's `menu` font, one platform-independent state machine both
//! shells drive with W3C key codes, exactly like the lobby / controls / replay overlays.
//!
//! Title → Story (the game's own title screen — New Game with the prologue / Continue / Delete Game on the save
//! slots — or a quick continue / new game from here) · Battle (any fighter / form / level vs any enemy, boss or
//! story preset, any arena, 1-4 local players) · Online (the lobby, [`crate::netmenu`]) · Replays · Mods (turn
//! mods on / off, open or add the mods folder) · Settings (view / zoom, volume, fullscreen, controls).
//!
//! The launcher only decides *what* to start ([`Choice`]); the shell builds the app ([`story_app`],
//! [`App::battle`]) and keeps the [`Settings`] (a JSON file natively, localStorage in the browser).
//! Save slots: the story session's `World.saves` (the game's Save Game screen writes them); shells persist a slot
//! file per slot whenever [`saves_of`] reports a new generation and pass the files back in [`story_app`].
//!
//! Story → "Title screen" hands over to the core's title (`wf_core::menu::title`, [`title_app`]): the session then
//! builds the new world itself (`menu::title::boot`) and the in-game Save screen writes the same slots.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_content::image::{decode_rgba, Rgba};
use wf_content::Packs;
use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::GameData;
use wf_core::field::FieldConfig;
use wf_core::menu::save::Saves;
/// story save slots (the game's flash holds 3)
pub use wf_core::menu::save::SLOTS;
use wf_core::session::Session;
use wf_render::font as tiny;
use wf_render::ui::Font;
use wf_render::ViewMode;

use crate::input::Bindings;
use crate::rebind::{MenuResult, RebindMenu};
use crate::App;

/// Story presets (multi-phase story fights). Mirrors `wf-mcp`'s catalog (`crates/wf-mcp/src/catalog.rs`).
pub const PRESETS: &[(&str, &str, &[(&str, i32)], &str)] = &[
    ("buu_saga", "Majin Buu saga", &[("t28", 1), ("t116", 1), ("t121", 1), ("t60", 1)], "crater"),
    ("majin_buu", "Majin Buu & Babidi (survive)", &[("t26+~t29", 1)], "crater"),
    ("majin_buu2", "Majin Buu, 2nd (survive)", &[("t27", 1)], "crater"),
    ("super_buu", "Super Buu", &[("t116", 1)], "crater2"),
    ("super_buu_waste", "Super Buu: Wasteland", &[("t118", 1), ("t119", 1)], "wasteland"),
    ("kid_buu", "Kid Buu", &[("t60", 1)], "wasteland"),
    ("janemba", "Janemba > Super Janemba", &[("t51", 1), ("t122", 1)], "hfil"),
    ("broly", "Broly", &[("t10", 1)], "volcano"),
    ("babidi_ship", "Pui Pui > Yakon > Dabura", &[("t96", 1), ("t145", 1), ("t18", 1)], "demon"),
    ("yakon", "Yakon", &[("t145", 1)], "yakon"),
    ("dabura", "Dabura", &[("t18", 1)], "demon"),
    ("majin_vegeta", "Majin Vegeta", &[("t70", 1)], "canyon"),
    ("tournament", "Spopovich & Yamu", &[("t111+t147", 1)], "stadium"),
    ("htc", "Time Chamber > Super Buu", &[("t39", 1), ("t130", 1), ("t115", 1)], "htc"),
    ("inside_buu", "Inside Buu gauntlet", &[("t40", 1), ("t35", 1), ("t90", 1), ("t121", 1)], "htc"),
    ("pikkon", "Pikkon", &[("t91", 1)], "stadium"),
    ("totenhotep", "Pharaoh Totenhotep", &[("t129", 1)], "crater"),
    ("ninja_boss", "Ninja Boss", &[("t80", 1)], "forest"),
    ("guardian", "Pilaf's Guardian", &[("t63", 1)], "forest"),
    ("warlord", "Airship Warlord", &[("t0", 1)], "forest"),
    ("blister", "Blister", &[("t6", 1)], "crater"),
    ("worm", "Worm Head", &[("t142", 1)], "crater"),
    ("majin_fighters", "4 Majin Fighters", &[("t68", 4)], "forest"),
];

/// boss enemy types (catalog's `BOSS_TYPES`): listed first
const BOSS_TYPES: &[i32] = &[
    0x00, 0x06, 0x0a, 0x0b, 0x12, 0x1a, 0x1b, 0x1c, 0x23, 0x25, 0x27, 0x28, 0x33, 0x3c, 0x3d, 0x3f, 0x46, 0x50, 0x51, 0x5a, 0x5b, 0x60, 0x64, 0x6f, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
    0x7a, 0x81, 0x82, 0x8e, 0x91, 0x93, 0x2d, 0x31, 0x55, 0x6d, 0x6e, 0x70, 0x94, 0x84, 0x87, 0x3b, 0x26, 0x4, 0x19, 0x1d,
];

pub const VIEWS: &[(&str, &str)] = &[
    ("pc:0", "PC wide (auto)"),
    ("pc:1", "PC wide 1x"),
    ("pc:2", "PC wide 2x"),
    ("pc:3", "PC wide 3x"),
    ("pc:4", "PC wide 4x"),
    ("pc:5", "PC wide 5x"),
    ("pc:6", "PC wide 6x"),
    ("gba:0", "Classic 240x160"),
];

/// `"pc:N"` / `"gba:N"` -> the renderer's view mode
pub fn view_mode(s: &str) -> ViewMode {
    let (k, z) = s.split_once(':').unwrap_or((s, "0"));
    let z: u32 = z.parse().unwrap_or(0);
    if k == "gba" {
        ViewMode::Gba { scale: z }
    } else {
        ViewMode::Pc { zoom: z }
    }
}

/// The battle the player set up last.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BattlePrefs {
    /// false = one enemy type, true = a story preset
    pub preset_mode: bool,
    pub fighter: String,
    /// None = the fighter's default form
    pub form: Option<i32>,
    pub level: i32,
    pub enemy: String,
    pub count: u32,
    /// "" = automatic (the preset's / the enemy's usual arena)
    pub arena: String,
    pub preset: String,
    pub players: usize,
}

impl Default for BattlePrefs {
    fn default() -> Self {
        BattlePrefs {
            preset_mode: false,
            fighter: "goku".into(),
            form: None,
            level: 60,
            enemy: "t60".into(),
            count: 1,
            arena: String::new(),
            preset: "kid_buu".into(),
            players: 1,
        }
    }
}

/// Launcher settings (persisted by the shell).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// `VIEWS` key
    pub view: String,
    /// 0..=100 percent
    pub volume: u32,
    pub fullscreen: bool,
    /// mod folder names that do not load
    pub disabled_mods: Vec<String>,
    pub battle: BattlePrefs,
    /// story mode: hold the picture for the GBA's measured load lengths ("original pacing"; presentation only)
    #[serde(default)]
    pub original_pacing: bool,
    /// "PC wide (auto)": false = steady (at most one zoom step above the window's base), true = max (the map fills
    /// the window; zooms up to 6x on small maps)
    #[serde(default)]
    pub auto_zoom_max: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { view: "pc:0".into(), volume: 100, fullscreen: false, disabled_mods: Vec::new(), battle: BattlePrefs::default(), original_pacing: false, auto_zoom_max: false }
    }
}

impl Settings {
    pub fn from_json(s: &str) -> Option<Settings> {
        serde_json::from_str(s).ok()
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    pub fn mode(&self) -> ViewMode {
        view_mode(&self.view)
    }
    pub fn gain(&self) -> f32 {
        self.volume.min(100) as f32 / 100.0
    }
}

/// What the shell should do after a key.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    None,
    /// story mode: a new game (None) or continue from a slot
    Story { slot: Option<usize> },
    /// story mode on the game's own title screen (New Game / Continue / Delete Game there)
    Title,
    Battle(BattleConfig),
    /// the online lobby
    Online,
    Replays,
    OpenModsFolder,
    /// web: pick a mod folder to add
    AddMod,
    Quit,
    /// settings changed (save them; apply fullscreen)
    Settings,
    /// bindings changed (save them)
    Bindings,
}

impl Choice {
    /// `{"kind": ...}` for the browser shell
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Choice::None => json!({"kind": "none"}),
            Choice::Story { slot } => json!({"kind": "story", "slot": slot}),
            Choice::Title => json!({"kind": "title"}),
            Choice::Battle(c) => json!({"kind": "battle", "config": c}),
            Choice::Online => json!({"kind": "online"}),
            Choice::Replays => json!({"kind": "replays"}),
            Choice::OpenModsFolder => json!({"kind": "open_mods"}),
            Choice::AddMod => json!({"kind": "add_mod"}),
            Choice::Quit => json!({"kind": "quit"}),
            Choice::Settings => json!({"kind": "settings"}),
            Choice::Bindings => json!({"kind": "bindings"}),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Story,
    Battle,
    Mods,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Story,
    Battle,
    Online,
    Replays,
    Mods,
    Settings,
    Quit,
    TitleScreen,
    NewGame,
    Slot(usize),
    Kind,
    Fighter,
    Form,
    Level,
    Enemy,
    Count,
    Preset,
    Arena,
    Players,
    Fight,
    Mod(usize),
    ModsFolder,
    View,
    Volume,
    Fullscreen,
    /// story mode's "original pacing" (holds the picture for the GBA's load lengths; presentation only)
    Pacing,
    /// "Auto zoom: steady / max" (`Settings::auto_zoom_max`)
    AutoZoom,
    Controls,
    Back,
}

/// A mod folder the shell found.
#[derive(Clone, Debug, PartialEq)]
pub struct ModEntry {
    pub name: String,
    pub enabled: bool,
}

pub struct Launcher {
    pub screen: Screen,
    cursor: usize,
    pub settings: Settings,
    /// slot summaries (None = empty)
    pub slots: Vec<Option<String>>,
    pub mods: Vec<ModEntry>,
    /// native shells: Quit, Fullscreen, "Open mods folder"; web: "Add mod folder"
    pub native: bool,
    pub bindings: Bindings,
    pub rebind: RebindMenu,
    /// a line under the menu (errors from the shell)
    pub message: String,
    data: Option<Arc<GameData>>,
    /// (fighter id, name, forms, default form)
    fighters: Vec<(String, String, Vec<i32>, i32)>,
    /// (id, name)
    enemies: Vec<(String, String)>,
    /// (id, name)
    arenas: Vec<(String, String)>,
    font: Option<Font>,
    bg: Option<Rgba>,
}

fn wrap(i: i32, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        i.rem_euclid(n as i32) as usize
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => String::new(),
    }
}

impl Launcher {
    pub fn new(packs: &Packs, settings: Settings, native: bool) -> Launcher {
        let data = crate::battle::game_data(packs).ok();
        let mut fighters = Vec::new();
        let mut enemies = Vec::new();
        let mut arenas = Vec::new();
        if let Some(d) = &data {
            for f in d.fighters.iter().filter(|f| !(f.id.starts_with('f') && f.id[1..].chars().all(|c| c.is_ascii_digit()))) {
                let name = if f.name == f.id { d.chars.iter().find(|c| c.index == f.character).map(|c| c.name.clone()).unwrap_or_else(|| cap(&f.id)) } else { f.name.clone() };
                let name = if f.fusion.is_some() && f.name == f.id { cap(&f.id) } else { name };
                fighters.push((f.id.clone(), name, f.forms.clone(), f.default_form));
            }
            let mut es: Vec<(bool, String, String)> = d
                .enemies
                .iter()
                .filter(|e| (e.hp > 0 || e.ai != 0) && !e.name.is_empty())
                .map(|e| (!BOSS_TYPES.contains(&e.ty), e.name.clone(), format!("t{}", e.ty)))
                .collect();
            es.sort();
            enemies = es.into_iter().map(|(_, n, id)| (id, n)).collect();
            arenas = d.arenas.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
        }
        let bg = packs.read("ui/images/screen_title.png").and_then(|b| decode_rgba(&b).ok());
        Launcher {
            screen: Screen::Main,
            cursor: 0,
            settings,
            slots: vec![None; SLOTS],
            mods: Vec::new(),
            native,
            bindings: Bindings::default(),
            rebind: RebindMenu::default(),
            message: String::new(),
            data,
            fighters,
            enemies,
            arenas,
            font: Font::load(packs, "menu").ok(),
            bg,
        }
    }

    /// Slot files (bytes as written from [`saves_of`]; empty = no game) -> the Story page's summaries.
    pub fn set_slots(&mut self, files: &[Vec<u8>]) {
        self.slots = (0..SLOTS).map(|i| files.get(i).and_then(|b| self.data.as_ref().and_then(|d| slot_summary(b, d)))).collect();
    }

    pub fn show(&mut self, s: Screen) {
        self.screen = s;
        self.cursor = 0;
    }

    fn rows(&self) -> Vec<Row> {
        match self.screen {
            Screen::Main => {
                let mut v = vec![Row::Story, Row::Battle, Row::Online, Row::Replays, Row::Mods, Row::Settings];
                if self.native {
                    v.push(Row::Quit);
                }
                v
            }
            Screen::Story => {
                let mut v = vec![Row::TitleScreen];
                v.extend((0..SLOTS).map(Row::Slot));
                v.extend([Row::NewGame, Row::Back]);
                v
            }
            Screen::Battle => {
                let b = &self.settings.battle;
                let mut v = vec![Row::Kind, Row::Fighter, Row::Form, Row::Level];
                if b.preset_mode {
                    v.push(Row::Preset);
                } else {
                    v.extend([Row::Enemy, Row::Count]);
                }
                v.extend([Row::Arena, Row::Players, Row::Fight, Row::Back]);
                v
            }
            Screen::Mods => {
                let mut v: Vec<Row> = (0..self.mods.len()).map(Row::Mod).collect();
                v.push(Row::ModsFolder);
                v.push(Row::Back);
                v
            }
            Screen::Settings => {
                let mut v = vec![Row::View, Row::AutoZoom, Row::Volume, Row::Pacing];
                if self.native {
                    v.push(Row::Fullscreen);
                }
                v.extend([Row::Controls, Row::Back]);
                v
            }
        }
    }

    fn fighter_idx(&self) -> usize {
        self.fighters.iter().position(|f| f.0 == self.settings.battle.fighter).unwrap_or(0)
    }

    fn form_name(&self, id: i32) -> String {
        let Some(f) = self.data.as_ref().and_then(|d| d.form(id)) else { return format!("form {id}") };
        if f.ss3 {
            format!("SS3 {}", f.name)
        } else if f.ss {
            format!("SS {}", f.name)
        } else {
            f.name.clone()
        }
    }

    fn preset_idx(&self) -> usize {
        PRESETS.iter().position(|p| p.0 == self.settings.battle.preset).unwrap_or(0)
    }

    fn auto_arena(&self) -> String {
        let b = &self.settings.battle;
        if b.preset_mode {
            return PRESETS[self.preset_idx()].3.to_string();
        }
        PRESETS
            .iter()
            .find(|p| p.2.len() == 1 && p.2[0].0.split('+').next() == Some(b.enemy.as_str()))
            .map(|p| p.3.to_string())
            .or_else(|| crate::netmenu::ENEMIES.iter().find(|e| e.0 == b.enemy).map(|e| e.2.to_string()))
            .unwrap_or_else(|| "forest".into())
    }

    /// The battle the Battle page describes.
    pub fn battle_config(&self) -> BattleConfig {
        let b = &self.settings.battle;
        let (fid, forms, def) = self.fighters.get(self.fighter_idx()).map(|f| (f.0.clone(), f.2.clone(), f.3)).unwrap_or(("goku".into(), vec![2], 2));
        let form = b.form.filter(|f| forms.contains(f)).unwrap_or(def);
        let f = FighterSetup { fighter: fid, form, level: b.level.clamp(50, 200), ..Default::default() };
        let phases = if b.preset_mode {
            PRESETS[self.preset_idx()].2.iter().map(|(e, n)| Phase { enemy: e.to_string(), count: *n }).collect()
        } else {
            vec![Phase { enemy: b.enemy.clone(), count: b.count.clamp(1, 8) as i32 }]
        };
        let arena = if b.arena.is_empty() { self.auto_arena() } else { b.arena.clone() };
        BattleConfig { arena, players: vec![f; b.players.clamp(1, 4)], phases, rng: RngInit::Seed(1), xp: true }
    }

    fn change(&mut self, row: Row, d: i32) -> Choice {
        let b = &mut self.settings.battle;
        match row {
            Row::Kind => b.preset_mode = !b.preset_mode,
            Row::Fighter => {
                let k = self.fighters.iter().position(|f| f.0 == b.fighter).unwrap_or(0) as i32;
                if let Some(f) = self.fighters.get(wrap(k + d, self.fighters.len())) {
                    b.fighter = f.0.clone();
                    b.form = None;
                }
            }
            Row::Form => {
                let Some(f) = self.fighters.iter().find(|f| f.0 == b.fighter) else { return Choice::None };
                let cur = b.form.filter(|x| f.2.contains(x)).unwrap_or(f.3);
                let k = f.2.iter().position(|&x| x == cur).unwrap_or(0) as i32;
                b.form = f.2.get(wrap(k + d, f.2.len())).copied();
            }
            Row::Level => b.level = (b.level + d * 5).clamp(50, 200),
            Row::Enemy => {
                let k = self.enemies.iter().position(|e| e.0 == b.enemy).unwrap_or(0) as i32;
                if let Some(e) = self.enemies.get(wrap(k + d, self.enemies.len())) {
                    b.enemy = e.0.clone();
                }
            }
            Row::Count => b.count = (b.count as i32 + d).clamp(1, 8) as u32,
            Row::Preset => {
                let k = PRESETS.iter().position(|p| p.0 == b.preset).unwrap_or(0) as i32;
                b.preset = PRESETS[wrap(k + d, PRESETS.len())].0.into();
            }
            Row::Arena => {
                // "" (auto) then every arena
                let k = if b.arena.is_empty() { 0 } else { self.arenas.iter().position(|a| a.0 == b.arena).map(|i| i as i32 + 1).unwrap_or(0) };
                let n = wrap(k + d.signum(), self.arenas.len() + 1);
                b.arena = if n == 0 { String::new() } else { self.arenas[n - 1].0.clone() };
            }
            Row::Players => b.players = (b.players as i32 + d.signum()).clamp(1, 4) as usize,
            Row::Mod(i) => {
                if let Some(m) = self.mods.get_mut(i) {
                    m.enabled = !m.enabled;
                    self.settings.disabled_mods.retain(|n| n != &m.name);
                    if !m.enabled {
                        self.settings.disabled_mods.push(m.name.clone());
                    }
                }
            }
            Row::View => {
                let k = VIEWS.iter().position(|v| v.0 == self.settings.view).unwrap_or(0) as i32;
                self.settings.view = VIEWS[wrap(k + d.signum(), VIEWS.len())].0.into();
            }
            Row::Volume => self.settings.volume = (self.settings.volume as i32 + d.signum() * 10).clamp(0, 100) as u32,
            Row::Fullscreen => self.settings.fullscreen = !self.settings.fullscreen,
            Row::Pacing => self.settings.original_pacing = !self.settings.original_pacing,
            Row::AutoZoom => self.settings.auto_zoom_max = !self.settings.auto_zoom_max,
            _ => return Choice::None,
        }
        Choice::Settings
    }

    /// A key went down (W3C `code`). Arrows / Enter / Escape, plus the game's default A (X) and B (Z) buttons.
    pub fn key(&mut self, code: &str) -> Choice {
        if self.rebind.open {
            let r = self.rebind.key(code, &mut self.bindings);
            return if r == MenuResult::Changed { Choice::Bindings } else { Choice::None };
        }
        let rows = self.rows();
        self.cursor = self.cursor.min(rows.len().saturating_sub(1));
        let row = rows[self.cursor];
        let step = |big: bool| if big { 10 } else { 1 };
        match code {
            "ArrowUp" => self.cursor = wrap(self.cursor as i32 - 1, rows.len()),
            "ArrowDown" => self.cursor = wrap(self.cursor as i32 + 1, rows.len()),
            "ArrowLeft" | "PageUp" => return self.change(row, -step(code == "PageUp")),
            "ArrowRight" | "PageDown" => return self.change(row, step(code == "PageDown")),
            "Escape" | "KeyZ" | "Backspace" => {
                if self.screen != Screen::Main {
                    let back = match self.screen {
                        Screen::Story => Row::Story,
                        Screen::Battle => Row::Battle,
                        Screen::Mods => Row::Mods,
                        _ => Row::Settings,
                    };
                    self.show(Screen::Main);
                    self.cursor = self.rows().iter().position(|&r| r == back).unwrap_or(0);
                }
            }
            "Enter" | "NumpadEnter" | "Space" | "KeyX" => return self.activate(row),
            _ => {}
        }
        Choice::None
    }

    fn activate(&mut self, row: Row) -> Choice {
        self.message.clear();
        match row {
            Row::Story => self.show(Screen::Story),
            Row::Battle => {
                self.show(Screen::Battle);
                self.cursor = self.rows().iter().position(|&r| r == Row::Fight).unwrap_or(0);
            }
            Row::Online => return Choice::Online,
            Row::Replays => return Choice::Replays,
            Row::Mods => self.show(Screen::Mods),
            Row::Settings => self.show(Screen::Settings),
            Row::Quit => return Choice::Quit,
            Row::NewGame => return Choice::Story { slot: None },
            Row::TitleScreen => return Choice::Title,
            Row::Slot(i) => {
                if self.slots.get(i).is_some_and(|s| s.is_some()) {
                    return Choice::Story { slot: Some(i) };
                }
                self.message = "That slot is empty.".into();
            }
            Row::Fight => return Choice::Battle(self.battle_config()),
            Row::Mod(_) | Row::Kind | Row::Fullscreen | Row::Pacing | Row::AutoZoom => return self.change(row, 1),
            Row::ModsFolder => return if self.native { Choice::OpenModsFolder } else { Choice::AddMod },
            Row::Controls => {
                self.rebind.toggle();
                self.rebind.cursor = 0;
            }
            Row::Back => {
                return self.key("Escape");
            }
            _ => {}
        }
        Choice::None
    }

    fn label(&self, row: Row) -> (String, String) {
        let b = &self.settings.battle;
        let t = |s: &str| s.to_string();
        match row {
            Row::Story => (t("Story"), String::new()),
            Row::Battle => (t("Battle"), String::new()),
            Row::Online => (t("Online"), String::new()),
            Row::Replays => (t("Replays"), String::new()),
            Row::Mods => (t("Mods"), String::new()),
            Row::Settings => (t("Settings"), String::new()),
            Row::Quit => (t("Quit"), String::new()),
            Row::TitleScreen => (t("Title screen"), t("New Game / Continue")),
            Row::NewGame => (t("Quick start"), t("Snakeway, no prologue")),
            Row::Slot(i) => (format!("Continue {}", i + 1), self.slots.get(i).cloned().flatten().unwrap_or_else(|| t("- empty -"))),
            Row::Kind => (t("Fight"), if b.preset_mode { t("< Story preset >") } else { t("< Any enemy >") }),
            Row::Fighter => (t("Fighter"), format!("< {} >", self.fighters.get(self.fighter_idx()).map(|f| f.1.as_str()).unwrap_or("?"))),
            Row::Form => {
                let f = self.fighters.get(self.fighter_idx());
                let id = f.map(|f| b.form.filter(|x| f.2.contains(x)).unwrap_or(f.3)).unwrap_or(0);
                (t("Form"), format!("< {} >", self.form_name(id)))
            }
            Row::Level => (t("Level"), format!("< {} >", b.level.clamp(50, 200))),
            Row::Enemy => (t("Enemy"), format!("< {} >", self.enemies.iter().find(|e| e.0 == b.enemy).map(|e| e.1.as_str()).unwrap_or(b.enemy.as_str()))),
            Row::Count => (t("How many"), format!("< {} >", b.count)),
            Row::Preset => (t("Preset"), format!("< {} >", PRESETS[self.preset_idx()].1)),
            Row::Arena => {
                let id = if b.arena.is_empty() { self.auto_arena() } else { b.arena.clone() };
                let name = self.arenas.iter().find(|a| a.0 == id).map(|a| a.1.clone()).unwrap_or(id);
                (t("Arena"), if b.arena.is_empty() { format!("< Auto: {name} >") } else { format!("< {name} >") })
            }
            Row::Players => (t("Players"), format!("< {} >", b.players)),
            Row::Fight => (t("START"), String::new()),
            Row::Mod(i) => {
                let m = &self.mods[i];
                (m.name.clone(), if m.enabled { t("< on >") } else { t("< off >") })
            }
            Row::ModsFolder => (if self.native { t("Open mods folder") } else { t("Add mod folder...") }, String::new()),
            Row::View => (t("View"), format!("< {} >", VIEWS.iter().find(|v| v.0 == self.settings.view).map(|v| v.1).unwrap_or("?"))),
            Row::Volume => (t("Volume"), format!("< {}% >", self.settings.volume)),
            Row::Fullscreen => (t("Fullscreen"), if self.settings.fullscreen { t("< on >") } else { t("< off >") }),
            Row::Pacing => (t("Original pacing"), if self.settings.original_pacing { t("< on >") } else { t("< off >") }),
            Row::AutoZoom => (t("Auto zoom"), if self.settings.auto_zoom_max { t("< max >") } else { t("< steady >") }),
            Row::Controls => (t("Controls..."), String::new()),
            Row::Back => (t("Back"), String::new()),
        }
    }

    /// Draw into an ARGB framebuffer `w` x `h` (the 240 x 160 screen in the middle).
    pub fn draw(&mut self, packs: &Packs, fb: &mut [u32], w: usize, h: usize) {
        const ACCENT: u32 = 0xFFF0_A030;
        let (ox, oy) = ((w as i32 - 240) / 2, (h as i32 - 160) / 2);
        fb.iter_mut().for_each(|p| *p = 0xFF00_0000);
        let main = self.screen == Screen::Main && !self.rebind.open;
        if let Some(bg) = &self.bg {
            for y in 0..bg.h.min(160) as i32 {
                for x in 0..bg.w.min(240) as i32 {
                    let (px, py) = (ox + x, oy + y);
                    if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        let c = bg.px[(y as u32 * bg.w + x as u32) as usize];
                        fb[py as usize * w + px as usize] = if main { 0xFF00_0000 | ((c >> 1) & 0x007F_7F7F) } else { 0xFF00_0000 | ((c >> 2) & 0x003F_3F3F) };
                    }
                }
            }
        }
        let font = self.font.as_ref();
        let text = |fb: &mut [u32], s: &str, x: i32, y: i32, c: u32| match font {
            Some(f) => {
                // the game's menu text: a (+1, +1) shadow
                f.draw_argb(fb, w, h, s, x + 1, y + 10, 0xFF00_0000);
                f.draw_argb(fb, w, h, s, x, y + 9, c);
            }
            None => tiny::draw_text(fb, w, h, x, y, s, c),
        };
        let width = |s: &str| match font {
            Some(f) => f.width(s),
            None => tiny::text_width(s),
        };
        if self.rebind.open {
            let b = self.bindings.clone();
            self.rebind.draw(packs, fb, w, h, &b);
            return;
        }
        let rows = self.rows();
        self.cursor = self.cursor.min(rows.len().saturating_sub(1));
        if main {
            // a compact column under the logo, like the game's START / OPTIONS
            let n = rows.len() as i32;
            let (pw, ph) = (92, n * 10 + 4);
            let (px, py) = (ox + 74, oy + 145 - ph);
            tiny::fill_rect(fb, w, h, px, py, pw, ph, 0xC010_1820);
            for p in [py, py + ph - 1] {
                tiny::fill_rect(fb, w, h, px, p, pw, 1, ACCENT);
            }
            for (k, &row) in rows.iter().enumerate() {
                let (l, _) = self.label(row);
                let y = py + 2 + k as i32 * 10;
                let sel = k == self.cursor;
                if sel {
                    tiny::fill_rect(fb, w, h, px + 3, y - 1, pw - 6, 10, 0xFF2E_3A48);
                }
                text(fb, &l, px + (pw - width(&l)) / 2, y, if sel { 0xFFFF_FFFF } else { 0xFFB8_B8B8 });
            }
            if !self.message.is_empty() {
                tiny::draw_text(fb, w, h, ox + 4, oy + 150, &self.message, 0xFFFF_7A6B);
            }
            return;
        }
        tiny::fill_rect(fb, w, h, ox + 4, oy + 2, 232, 156, 0xE010_1820);
        tiny::fill_rect(fb, w, h, ox + 4, oy + 2, 232, 1, ACCENT);
        tiny::fill_rect(fb, w, h, ox + 4, oy + 157, 232, 1, ACCENT);
        let title = match self.screen {
            Screen::Main => "",
            Screen::Story => "STORY",
            Screen::Battle => "BATTLE",
            Screen::Mods => "MODS",
            Screen::Settings => "SETTINGS",
        };
        text(fb, title, ox + 10, oy + 5, ACCENT);
        let mut y = oy + 19;
        let note = match self.screen {
            Screen::Mods if self.mods.is_empty() => "No mods yet. A mod is a folder with a pack.json.",
            Screen::Mods => "Changes apply to the next game you start.",
            Screen::Story => "Save at the game's save points.",
            _ => "",
        };
        if !note.is_empty() {
            tiny::draw_text(fb, w, h, ox + 10, y, note, 0xFF9A_988F);
            y += 11;
        }
        // scroll long lists (mods)
        let visible = ((oy + 140 - y) / 11).max(1) as usize;
        let first = self.cursor.saturating_sub(visible - 1);
        for (k, &row) in rows.iter().enumerate().skip(first).take(visible) {
            let sel = k == self.cursor;
            if sel {
                tiny::fill_rect(fb, w, h, ox + 8, y - 1, 224, 11, 0xFF2E_3A48);
            }
            let col = if sel { 0xFFFF_FFFF } else { 0xFFB8_B8B8 };
            let (l, v) = self.label(row);
            let col = if row == Row::Fight { ACCENT } else { col };
            text(fb, &l, ox + 14, y, col);
            if !v.is_empty() {
                let mut v = v;
                while width(&v) > 150 && v.chars().count() > 6 {
                    // keep "< ... >": drop characters before the closing bracket
                    let cut = v.char_indices().rev().nth(2).map(|(i, _)| i).unwrap_or(0);
                    v.remove(cut);
                }
                text(fb, &v, ox + 78, y, col);
            }
            y += 11;
        }
        if !self.message.is_empty() {
            tiny::draw_text(fb, w, h, ox + 10, oy + 137, &self.message, 0xFFFF_7A6B);
        }
        tiny::draw_text(fb, w, h, ox + 10, oy + 147, "ENTER ok  <> change  ESC back", 0xFF6A_6870);
    }
}

/// "Goku Lv 12  W-A  1:23" for a slot file (None = empty / unreadable)
pub fn slot_summary(bytes: &[u8], d: &GameData) -> Option<String> {
    let mut s = Saves::default();
    s.set_slot_bytes(0, bytes);
    let sd = s.slot(0)?;
    let ch = sd.story.chars.get(sd.story.current.max(0) as usize);
    let name = d.chars.iter().find(|c| c.index == sd.story.current).map(|c| c.name.clone()).unwrap_or_default();
    let lv = ch.map(|c| c.rec.level).unwrap_or(1);
    let mins = sd.story.play_frames / (60 * 60);
    Some(format!("{name} Lv{lv}  {}-{}  {}:{:02}", sd.world, sd.area, mins / 60, mins % 60))
}

/// A story app: a new game (`slot` None) or the game saved in `slots[slot]`. `slots` = the slot files (empty = no
/// game); they are loaded into the session so the in-game Save screen lists and overwrites them.
pub fn story_app(packs: Packs, slots: &[Vec<u8>], slot: Option<usize>, seed: u32) -> Result<App, String> {
    let s = story_sim(&packs, slots, slot, seed)?;
    Ok(App::new(packs, Box::new(s)))
}

/// [`story_app`]'s sim (its start recorded in `StorySim::start`: replays and save states rebuild it)
pub fn story_sim(packs: &Packs, slots: &[Vec<u8>], slot: Option<usize>, seed: u32) -> Result<crate::story::StorySim, String> {
    if slot.is_none() {
        // Quick start: the title's own New Game (GameData reset, prologue #4 -> #38) played unseen up to Goku's
        // control on Snake Way - the real game's state there (Goku: Energy Blast, Kamehameha, Instant Transmission;
        // #38 forgets Super Saiyan; zenie 100), not a fresh new-game record dropped into 1-1
        let mut s = title_sim(packs, slots, seed)?;
        let first_free = (0..SLOTS).find(|&i| s.session.world.saves.slot(i).is_none()).unwrap_or(0) as i32;
        s.session.world.saves.pending = Some(wf_core::menu::title::Boot::NewGame { slot: first_free });
        s.skip_prologue()?;
        s.session.world.saves.generation = 0;
        s.start = Some(serde_json::json!({"kind": "slots", "slots": crate::story::hex_slots(slots), "slot": slot, "seed": seed}));
        return Ok(s);
    }
    let data = crate::battle::load_data(packs)?;
    if data.field.is_none() {
        return Err("this content has no world/ data (re-extract with the current extractor)".into());
    }
    let mut saves = Saves::default();
    for (i, b) in slots.iter().enumerate().take(SLOTS) {
        saves.set_slot_bytes(i, b);
    }
    let mut cfg = FieldConfig { world: 1, area: 1, rng: RngInit::Seed(seed), ..Default::default() };
    if let Some(i) = slot {
        let sd = saves.slot(i).ok_or_else(|| format!("save slot {} is empty or unreadable", i + 1))?.clone();
        cfg = FieldConfig { world: sd.world, area: sd.area, variant: sd.variant, pos: Some((sd.x, sd.y)), story: Some(sd.story), ..cfg };
    }
    let mut session = Session::new_field(data, cfg);
    // the app recreates no hardware slowdown: one step per game logic iteration (`Options::no_lag`)
    session.world.opts.no_lag = true;
    session.world.menu.settings = options_of(slots);
    saves.current = slot.unwrap_or_else(|| (0..SLOTS).find(|&i| saves.slot(i).is_none()).unwrap_or(0)) as i32;
    saves.generation = 0;
    session.world.saves = saves;
    let mut s = crate::story::StorySim::with_session(packs, session)?;
    s.start = Some(serde_json::json!({"kind": "slots", "slots": crate::story::hex_slots(slots), "slot": slot, "seed": seed}));
    Ok(s)
}

/// Story mode on the game's title screen (START → Single Player → Select Game: New Game with the prologue, Continue,
/// Delete Game) with the slot files loaded; the world under the title is a placeholder the title's boot replaces.
pub fn title_app(packs: Packs, slots: &[Vec<u8>], seed: u32) -> Result<App, String> {
    let s = title_sim(&packs, slots, seed)?;
    Ok(App::new(packs, Box::new(s)))
}

/// [`title_app`]'s sim (its start recorded in `StorySim::start`)
pub fn title_sim(packs: &Packs, slots: &[Vec<u8>], seed: u32) -> Result<crate::story::StorySim, String> {
    let data = crate::battle::load_data(packs)?;
    if data.field.is_none() {
        return Err("this content has no world/ data (re-extract with the current extractor)".into());
    }
    let mut saves = Saves::default();
    for (i, b) in slots.iter().enumerate().take(SLOTS) {
        saves.set_slot_bytes(i, b);
    }
    let cfg = FieldConfig { world: 1, area: 1, rng: RngInit::Seed(seed), ..Default::default() };
    let mut session = Session::new_field(data, cfg);
    session.world.opts.no_lag = true;
    session.world.menu.settings = options_of(slots);
    session.world.saves = saves;
    // before the sim takes the creation events: the title's song replaces the placeholder area's
    wf_core::menu::title::open_title(&mut session.world);
    let mut s = crate::story::StorySim::with_session(packs, session)?;
    s.skip_area_banner();
    s.start = Some(serde_json::json!({"kind": "title", "slots": crate::story::hex_slots(slots), "seed": seed}));
    Ok(s)
}

/// A running story app's save slots: (generation, the slot files). None = not story mode. After the [`SLOTS`]
/// games, one more file: the options block (text speed, volumes - the cartridge's flash record 0, shared by every
/// game; `Settings::to_flash`).
pub fn saves_of(app: &App) -> Option<(u32, Vec<Vec<u8>>)> {
    let s = app.sim.as_any()?.downcast_ref::<crate::story::StorySim>()?;
    let w = &s.session.world;
    let sv = &w.saves;
    let mut files: Vec<Vec<u8>> = (0..SLOTS).map(|i| sv.slot_bytes(i)).collect();
    files.push(w.menu.settings.to_flash());
    Some((sv.generation, files))
}

/// the options a story app starts with: the block after the slot files ([`saves_of`]), else the game's defaults
fn options_of(files: &[Vec<u8>]) -> wf_core::menu::options::Settings {
    files.get(SLOTS).and_then(|b| wf_core::menu::options::Settings::from_flash(b)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_views() {
        let s = Settings::default();
        assert_eq!(Settings::from_json(&s.to_json()), Some(s));
        assert_eq!(view_mode("gba:0"), ViewMode::Gba { scale: 0 });
        assert_eq!(view_mode("pc:3"), ViewMode::Pc { zoom: 3 });
        assert_eq!(Settings::from_json("{}").unwrap().volume, 100);
    }
}
