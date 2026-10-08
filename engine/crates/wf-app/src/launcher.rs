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

mod battle_page;
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
    ("ninja_boss", "Ninja Boss", &[("t80", 1)], "plateau"),
    ("guardian", "Pilaf's Guardian", &[("t63", 1)], "canyon"),
    ("warlord", "Airship Warlord", &[("t0", 1)], "plateau"),
    ("blister", "Blister", &[("t6", 1)], "crater"),
    ("worm", "Worm Head", &[("t142", 1)], "stomach"),
    ("majin_fighters", "4 Majin Fighters", &[("t68", 4)], "crater"),
];

/// Battle mode's arenas (a plain arena fighter, not the story): open floors with nothing to hide behind or get stuck
/// on, picked on their own - no opponent is tied to a story place. Left out: the Southwest Forest and the villages
/// (tree tops / houses over the fighters, playtest #6), the Rocky Plateau's cliffs, the Barren Wasteland's rock
/// pillars (wide beams fizzled on them), HFIL's and the Demon World's obstacles, the crater after the blast (its pit),
/// Yakon's planet, the Stomach's corridor. Mods' own arenas follow these.
pub const BATTLE_ARENAS: &[&str] = &["crater", "canyon", "stadium", "htc", "volcano", "dojo"];
/// the Arena row's first choice
pub const DEFAULT_ARENA: &str = "crater";

/// The Battle page's character picker: (Character, Form, Outfit, fighter id, form record). The chosen row is the exact
/// form record the battle starts in, so transforming up / down follows the game's chain (a halo Super Saiyan 3 Goku
/// drops back to the halo Goku). Built from the form table (`forms/*.json`, sprite folder = gfx id - 9):
/// * a Form is a difference in play - stats, transformations, the Super Saiyan gauge's drain; an Outfit is a look
///   only (same stats, same chain shape). Teen / Mystic / tournament (form 15: the Saiyaman suit without helmet and
///   cape, s307) Gohan are looks of the same base form (no stat bonus; Mystic Gohan has no Super Saiyan form); the
///   Great Saiyaman is a form (+5 STR / POW / END, faster - the Saiyaman skill's transformation).
/// * "no limit": the story's records of a Super Saiyan form whose controller does not drain the gauge
///   (`player.rs` `drains`: 0x03001B78 / 0x03001B80 / 0x03001B68 / 0x03001B6C do, 0x03001B7C / 0x03001B84 do not) -
///   Goku 7 / 8, Gohan 17, Vegeta 30, Gotenks 36 -> 37 -> 38 (and the KO keeps the form: no flag 0x40).
/// * "Unstable Super Saiyan": Trunks 24, the story's unstable Trunks (controller 0x03001B88: a quarter walking speed,
///   turns Super Saiyan by itself after 120 + rand(180) frames into 25, which turns back after 240 + rand(120)) - one
///   entry, starting in 24.
/// * labels fit the page's value column (~27 characters).
/// * not listed: records with the same play as a listed one (Gogeta 44 = 41: only the fusion-dance flag 0x08 / the
///   controller 0x03001B60 differ, neither read in a battle) and records whose sprite lacks the fighting animations
///   (Goku 1 Other World clothes, Gohan 9 / 11 / 12, Trunks 23, the failed fusions Gotenks 32 / 33 and Gogeta 42 / 43
///   have only 2 - 10 of the 20 animations: no ki blast / attacks; Android 18, Krillin, Tien, Yamcha likewise).
/// * Hercule, Videl and Pikkon (the Dojo's Balanced roster): record slot 8 never grows (50 HP, STR 17 at any level -
///   no damage on a Majin Fighter, KO'd in seconds), so their battles take Goku's record at the chosen level
///   ([`Launcher::battle_config`]); their own sprites have every fighting animation.
pub const PICKS: &[(&str, &str, &str, &str, i32)] = &[
    ("Goku", "Base", "Normal", "goku", 0),
    ("Goku", "Base", "Halo (Other World)", "goku", 2),
    ("Goku", "Super Saiyan", "Normal", "goku", 3),
    ("Goku", "Super Saiyan", "Halo (Other World)", "goku", 4),
    ("Goku", "Super Saiyan 3", "Normal", "goku", 5),
    ("Goku", "Super Saiyan 3", "Halo (Other World)", "goku", 6),
    ("Goku", "Super Saiyan 3, no limit", "Normal", "goku", 8),
    ("Goku", "Super Saiyan 3, no limit", "Halo (Other World)", "goku", 7),
    ("Gohan", "Base", "Teen Gohan (school clothes)", "gohan", 10),
    ("Gohan", "Base", "Saiyaman suit, no helmet", "gohan", 15),
    ("Gohan", "Base", "Mystic Gohan (orange gi)", "gohan", 13),
    ("Gohan", "Super Saiyan", "Teen Gohan (school clothes)", "gohan", 16),
    ("Gohan", "Super Saiyan", "Saiyaman suit, no helmet", "gohan", 18),
    ("Gohan", "Super Saiyan, no limit", "Teen Gohan (school clothes)", "gohan", 17),
    ("Gohan", "Great Saiyaman", "Normal", "gohan", 14),
    ("Goten", "Base", "Normal", "goten", 19),
    ("Goten", "Super Saiyan", "Normal", "goten", 20),
    ("Trunks", "Base", "Normal", "trunks", 21),
    ("Trunks", "Super Saiyan", "Normal", "trunks", 22),
    ("Vegeta", "Base", "Normal", "vegeta", 26),
    ("Vegeta", "Base", "Halo (Other World)", "vegeta", 27),
    ("Vegeta", "Super Saiyan", "Normal", "vegeta", 28),
    ("Vegeta", "Super Saiyan", "Halo (Other World)", "vegeta", 29),
    ("Vegeta", "Super Saiyan, no limit", "Normal", "vegeta", 30),
    ("Gotenks", "Base", "Normal", "gotenks", 31),
    ("Gotenks", "Super Saiyan", "Normal", "gotenks", 34),
    ("Gotenks", "Super Saiyan 3", "Normal", "gotenks", 35),
    ("Gotenks", "Base, no-limit transforms", "Normal", "gotenks", 36),
    ("Gotenks", "Super Saiyan, no limit", "Normal", "gotenks", 37),
    ("Gotenks", "Super Saiyan 3, no limit", "Normal", "gotenks", 38),
    ("Vegito", "Base", "Normal", "vegito", 39),
    ("Vegito", "Super Saiyan", "Normal", "vegito", 40),
    ("Gogeta", "Super Saiyan", "Normal", "gogeta", 41),
    ("Hercule", "Base", "Normal", "f47", 47),
    ("Videl", "Base", "Normal", "f52", 52),
    ("Pikkon", "Base", "Normal", "f50", 50),
];

/// The fusions' story level: the automatic level never goes below the level of the opponent of their first story
/// fight (the enemy definitions' level bytes) - Gogeta vs Janemba 105, Gotenks vs Super Buu 125, Vegito vs Super Buu
/// with Gohan 142. A fusion's record is 60% of its two members' summed stats, so at a regular enemy's level (a Majin
/// Fighter's 75: max EP 199 - 227) Banshee Blast (200), Dragon Fist (300), Energy Sword / Volleyball (350) could never
/// be paid; the story never has them that weak.
const FUSION_LEVELS: &[(&str, i32)] = &[("gogeta", 105), ("gotenks", 125), ("vegito", 142)];

/// form records the picker does not list -> the listed record a saved setting moves to (same character and look)
const PICK_ALIASES: &[(i32, i32)] = &[(1, 0), (9, 10), (11, 10), (12, 10), (23, 21), (24, 22), (25, 22), (32, 31), (33, 31), (42, 41), (43, 41), (44, 41)];

/// One row of the character picker ([`PICKS`], then fighters a mod adds: each form record its own Form)
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub character: String,
    pub form: String,
    pub outfit: String,
    pub fighter: String,
    pub form_id: i32,
}

/// boss enemy types (catalog's `BOSS_TYPES`): listed first
const BOSS_TYPES: &[i32] = &[
    0x00, 0x06, 0x0a, 0x0b, 0x12, 0x1a, 0x1b, 0x1c, 0x23, 0x25, 0x27, 0x28, 0x33, 0x3c, 0x3d, 0x3f, 0x46, 0x50, 0x51, 0x5a, 0x5b, 0x60, 0x64, 0x6f, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
    0x7a, 0x81, 0x82, 0x8e, 0x91, 0x93, 0x2d, 0x31, 0x55, 0x6d, 0x6e, 0x70, 0x94, 0x84, 0x87, 0x3b, 0x26, 0x4, 0x19, 0x1d,
];

/// the game's own enemy types are 0 ..= 149 (a mod's new types come after)
const LAST_GAME_ENEMY: i32 = 149;

/// Game enemy types the Battle page does not offer as an opponent (playtest #10, each played headless through
/// `examples/opponents.rs`): not fighters - the dummy (0x54), the floor trigger, the inert helpers (0x22 / 0x72 /
/// 0x7E), the unused unnamed fighters (0x1E, 0x87) and worm part (0x90): an instant "You Win!!" or nothing to fight;
/// another enemy's part or summon - Janemba's arms / upper body and his summoned hand (invisible, never acts), the
/// Worm Body, Super Buu's kamikaze ghost and his fragment, Kid Buu's split copy; story actors that never fight - the
/// three Babidis, Vegeta (Gravity Chamber 5 HP, Barren Wasteland 1 HP), Goten's tag game, the Majin Buus (survive
/// fights: the story presets have them); the same fighter twice - the "(variant)" records (Broly's phase 2, Ghost,
/// Grand Mechanoid, Mechanoid, Nosferatu, Phantom, Poltergeist, Power Mechandoid, Vampire, Vlad), the disguised
/// Ninja / Shinobi / Shadow Warrior, Ninja Boss (variant), Spopovich's and Yamu's tournament copies, Super Buu
/// (script-created / 0 XP)
/// Game enemies that make no fair arena fight (Battle mode is a plain arena fighter; `examples/arena_audit.rs`, each
/// played with the arena rules): Super Buu with Gohan absorbed (0x77 / 0x78: no state of his takes damage - he glides
/// between the story's waypoints); Blister's Enzyme (latches on for ~1 HP hits, never KOs) and Buu's Blood Cell (drifts
/// away, never attacks) - hazards of Buu's insides, not fighters; the armoured vehicles and keep-away shooters that
/// fire from ~144 px - off the 240 px screen most of the fight - Juggernaut (punches take ~0.5% even with the even
/// match), Tank, War Machine, Hessian, Marauder; the Mini-Yakon (hops in place, never attacks - it stays in Yakon's
/// fight, his own summon)
const ARENA_UNFIT: &[i32] = &[0x77, 0x78, 24, 7, 58, 127, 140, 46, 71, 146];

const NOT_OPPONENTS: &[i32] = &[
    84, 141, 34, 114, 126, 30, 135, 144, 52, 53, 55, 54, 143, 13, 14, 61, 4, 25, 29, 136, 137, 38, 26, 27, 28, 11, 31, 42, 73,
    83, 87, 92, 95, 134, 139, 79, 105, 102, 81, 109, 110, 111, 147, 117, 120, 57,
];

/// The opponent names the Battle page shows for game types whose catalogue name carries a note
const OPPONENT_NAMES: &[(i32, &str)] = &[
    (112, "Spopovich"),
    (148, "Yamu"),
    (116, "Super Buu"),
    (115, "Super Buu (Time Chamber)"),
    (118, "Super Buu (Gotenks absorbed)"),
    (119, "Super Buu (Gohan absorbed)"),
    (121, "Super Buu (inside Buu)"),
    (39, "Goten (Time Chamber)"),
    (59, "Tournament Fighter"),
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
    /// settings saved when Battle mode had story presets: true = `preset` was chosen (moved to its enemy on load)
    pub preset_mode: bool,
    pub fighter: String,
    /// None = the fighter's default form
    pub form: Option<i32>,
    /// the level chosen on the Level row (used when `level_auto` is off)
    pub level: i32,
    /// the fighter's level follows the opponents: the highest level byte of the enemies' ROM definitions (the
    /// story's level for them - Kid Buu 150; run 1 fought him with Goku at 158). On by default (also for settings
    /// saved before it existed)
    pub level_auto: bool,
    pub enemy: String,
    pub count: u32,
    /// one of the Battle mode arenas ([`BATTLE_ARENAS`])
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
            level_auto: true,
            enemy: "t60".into(),
            count: 1,
            arena: DEFAULT_ARENA.into(),
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
    /// the view default became "Classic 240x160": settings saved before it (false) move to it once
    /// ([`Settings::migrate`]); F1 in a game switches Classic <-> PC wide
    #[serde(default)]
    pub classic_default: bool,
    /// link play from the title (Multiplayer -> Dojo / Trading Post with a friend online): 0 off, 1 host a room,
    /// 2 join one; the room number both players pick (`LINK<number>` on the link server)
    #[serde(default)]
    pub link_mode: u8,
    #[serde(default)]
    pub link_room: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            view: "gba:0".into(),
            volume: 100,
            fullscreen: false,
            disabled_mods: Vec::new(),
            battle: BattlePrefs::default(),
            original_pacing: false,
            auto_zoom_max: false,
            classic_default: true,
            link_mode: 0,
            link_room: 1,
        }
    }
}

impl Settings {
    pub fn from_json(s: &str) -> Option<Settings> {
        let mut st: Settings = serde_json::from_str(s).ok()?;
        st.migrate();
        Some(st)
    }
    /// settings saved before the Classic default: the view moves to Classic once
    pub fn migrate(&mut self) {
        if !self.classic_default {
            self.view = "gba:0".into();
            self.classic_default = true;
        }
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
    /// the character picker: Character (`Fighter`), Form, Outfit (looks; shown when the form has several)
    Fighter,
    Form,
    Outfit,
    Level,
    Enemy,
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
    /// Story: link play off / host / join, and the room
    LinkMode,
    LinkRoom,
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
    /// the character picker's rows ([`PICKS`] present in the content, then mod fighters)
    picks: Vec<Pick>,
    /// fighter id -> its default form record (a saved setting with no form)
    default_forms: Vec<(String, i32)>,
    /// (id, name)
    enemies: Vec<(String, String)>,
    /// enemy type -> its definition's level byte (`enemies/*.json` `level_byte`)
    enemy_levels: std::collections::BTreeMap<i32, i32>,
    /// (id, name)
    arenas: Vec<(String, String)>,
    font: Option<Font>,
    bg: Option<Rgba>,
    /// the Battle page's animation clock: seconds the shell reported ([`Launcher::advance`]) and the frame it makes
    clock: f64,
    tick: u32,
    /// the Battle page's renderer and caches
    page: battle_page::PageState,
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

/// The skills a battle gives a fighter in form `form`: the full moveset - the character's skills plus every
/// transformation the fighter has, whatever the level (Battle mode is an arena, not the story: no story-era lists).
/// Goku and Gotenks always have Super Saiyan 3 (22; the story teaches it at Majin Buu / Super Buu). The Great Saiyaman
/// is a form of its own: no Super Saiyan above it (the SS skill would do nothing) and no Saiyaman skill (18, the
/// story's suit toggle). Fusion Dance (12) does nothing in a battle: left out.
/// None = the fighter's own list as it is (mods, extra characters).
fn arena_skills(d: &GameData, fighter: &str, form: i32) -> Option<Vec<i32>> {
    let fd = d.fighter(fighter)?;
    if fd.skills.is_some() {
        return None;
    }
    let ch = d.chars.iter().find(|c| c.index == fd.character)?;
    let mut v: Vec<i32> = ch.skills.iter().copied().filter(|&s| s != 12 && s != 18).collect();
    match fighter {
        "goku" | "gotenks" if !v.contains(&22) => v.push(22),
        "gohan" if form == 14 => v.retain(|&s| s != 20),
        _ => {}
    }
    Some(v)
}

/// a mod fighter's form record as a Form label: Base / Super Saiyan / Super Saiyan 3 by its flags
fn form_label(d: &GameData, id: i32) -> String {
    match d.form(id) {
        Some(f) if f.ss3 => "Super Saiyan 3".into(),
        Some(f) if f.ss => "Super Saiyan".into(),
        Some(_) => "Base".into(),
        None => format!("form {id}"),
    }
}

impl Launcher {
    pub fn new(packs: &Packs, mut settings: Settings, native: bool) -> Launcher {
        settings.migrate();
        let data = crate::battle::game_data(packs).ok();
        let mut picks = Vec::new();
        let mut default_forms = Vec::new();
        let mut enemies = Vec::new();
        let mut arenas = Vec::new();
        if let Some(d) = &data {
            // the game's fighters: the curated rows whose fighter and form record the content has
            for &(c, f, o, fid, id) in PICKS {
                if d.fighter(fid).is_some_and(|fd| fd.forms.contains(&id)) && d.form(id).is_some() {
                    picks.push(Pick { character: c.into(), form: f.into(), outfit: o.into(), fighter: fid.into(), form_id: id });
                }
            }
            // fighters a mod adds: every form record a Form of its own
            let builtin: Vec<&str> = wf_core::battle::FIGHTERS.iter().map(|f| f.0).collect();
            for f in d.fighters.iter() {
                default_forms.push((f.id.clone(), f.default_form));
                if builtin.contains(&f.id.as_str()) {
                    continue;
                }
                let name = if f.name == f.id { d.chars.iter().find(|c| c.index == f.character).map(|c| c.name.clone()).unwrap_or_else(|| cap(&f.id)) } else { f.name.clone() };
                let mut seen: Vec<String> = Vec::new();
                for &id in &f.forms {
                    let mut label = form_label(d, id);
                    if seen.contains(&label) {
                        label = format!("{label} ({id})");
                    }
                    seen.push(label.clone());
                    picks.push(Pick { character: name.clone(), form: label, outfit: "Normal".into(), fighter: f.id.clone(), form_id: id });
                }
            }
            let mut es: Vec<(bool, String, String)> = d
                .enemies
                .iter()
                .filter(|e| (e.hp > 0 || e.ai != 0) && !e.name.is_empty())
                .filter(|e| e.ty > LAST_GAME_ENEMY || !(NOT_OPPONENTS.contains(&e.ty) || ARENA_UNFIT.contains(&e.ty)))
                .map(|e| {
                    let name = OPPONENT_NAMES.iter().find(|n| n.0 == e.ty && e.ty <= LAST_GAME_ENEMY).map(|n| n.1.to_string()).unwrap_or_else(|| e.name.clone());
                    (!BOSS_TYPES.contains(&e.ty), name, format!("t{}", e.ty))
                })
                .collect();
            es.sort();
            enemies = es.into_iter().map(|(_, n, id)| (id, n)).collect();
            // the open arenas, then a mod's own
            arenas = BATTLE_ARENAS
                .iter()
                .filter_map(|id| d.arenas.iter().find(|a| a.id == *id))
                .chain(d.arenas.iter().filter(|a| !wf_core::battle::ARENAS.iter().any(|g| g.0 == a.id)))
                .map(|a| (a.id.clone(), a.name.clone()))
                .collect();
        }
        let bg = packs.read("ui/images/screen_title.png").and_then(|b| decode_rgba(&b).ok());
        let enemy_levels = packs
            .list()
            .into_iter()
            .filter(|p| p.starts_with("enemies/") && p.ends_with(".json"))
            .filter_map(|p| packs.read(&p).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()))
            .filter_map(|v| Some((v["type"].as_i64()? as i32, v["level_byte"].as_i64()? as i32)))
            .collect();
        // Battle mode is one opponent in an open arena: settings saved with a story preset keep its opponent when it
        // was one enemy, an arena no longer offered (or the old "automatic" one) moves to the default, one enemy
        {
            let b = &mut settings.battle;
            if b.preset_mode {
                b.preset_mode = false;
                if let Some(&[(e, 1)]) = PRESETS.iter().find(|p| p.0 == b.preset).map(|p| p.2) {
                    if !e.contains('+') {
                        b.enemy = e.into();
                    }
                }
            }
            if !arenas.iter().any(|a| a.0 == b.arena) {
                b.arena = DEFAULT_ARENA.into();
            }
            b.count = 1;
        }
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
            picks,
            default_forms,
            enemies,
            enemy_levels,
            arenas,
            font: Font::load(packs, "menu").ok(),
            bg,
            clock: 0.0,
            tick: 0,
            page: Default::default(),
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
                v.push(Row::NewGame);
                // link play: the title's Multiplayer pages with a friend (both pick the same room)
                v.push(Row::LinkMode);
                if self.settings.link_mode != 0 {
                    v.push(Row::LinkRoom);
                }
                v.push(Row::Back);
                v
            }
            Screen::Battle => {
                // the character select (`battle_page`), in its reading order: the fighter, the opponent (one fighter),
                // the arena, the players; A on any row starts it, B goes back
                let mut v = vec![Row::Fighter, Row::Form];
                if self.outfits().len() > 1 {
                    v.push(Row::Outfit);
                }
                v.extend([Row::Level, Row::Enemy, Row::Arena, Row::Players]);
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

    /// Time for the Battle page's animated previews: the shells call this once per displayed frame with the seconds
    /// since the last one; the idle animations run at the game's 60 frames a second whatever the display rate.
    pub fn advance(&mut self, seconds: f64) {
        self.clock += seconds.clamp(0.0, 0.25);
        self.tick = (self.clock * wf_view::GBA_FPS) as u32;
    }

    /// The character picker's rows (Character, Form, Outfit -> fighter, form record)
    pub fn picks(&self) -> &[Pick] {
        &self.picks
    }

    /// the picker row the saved battle setting names: its fighter and form record (no form = the fighter's default;
    /// a record the picker does not list = its [`PICK_ALIASES`] replacement); else the fighter's first row, else the
    /// first row
    pub fn pick_idx(&self) -> usize {
        let b = &self.settings.battle;
        // no form chosen: the character's first row (base form, normal look - Goku's record default is his Other World
        // halo look, form 2, which the page showed first: playtest #18), a mod fighter's own default form
        let form = b
            .form
            .or_else(|| PICKS.iter().find(|r| r.3 == b.fighter).map(|r| r.4))
            .or_else(|| self.default_forms.iter().find(|f| f.0 == b.fighter).map(|f| f.1));
        let form = form.map(|id| PICK_ALIASES.iter().find(|a| a.0 == id).map(|a| a.1).unwrap_or(id));
        self.picks
            .iter()
            .position(|p| p.fighter == b.fighter && Some(p.form_id) == form)
            .or_else(|| self.picks.iter().position(|p| p.fighter == b.fighter))
            .unwrap_or(0)
    }

    /// distinct values of a picker column among `rows`, in order
    fn distinct<'a>(rows: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
        let mut v: Vec<&str> = Vec::new();
        for r in rows {
            if !v.contains(&r) {
                v.push(r);
            }
        }
        v
    }

    /// the Outfit row is shown when the chosen character has more than one look for the chosen form
    fn outfits(&self) -> Vec<&str> {
        let Some(cur) = self.picks.get(self.pick_idx()) else { return Vec::new() };
        Self::distinct(self.picks.iter().filter(|p| p.character == cur.character && p.form == cur.form).map(|p| p.outfit.as_str()))
    }

    /// step the picker: column 0 Character, 1 Form, 2 Outfit by `d`; the other columns keep their value where the
    /// new choice has it (Goku's Super Saiyan -> Vegeta's Super Saiyan; a halo stays a halo), else the first
    fn step_pick(&mut self, col: usize, d: i32) -> Choice {
        let Some(cur) = self.picks.get(self.pick_idx()).cloned() else { return Choice::None };
        let pick = |c: &str, f: &str, o: &str, picks: &[Pick]| -> Option<Pick> {
            let rows: Vec<&Pick> = picks.iter().filter(|p| p.character == c).collect();
            let form = rows.iter().find(|p| p.form == f).map(|p| p.form.clone()).or_else(|| rows.first().map(|p| p.form.clone()))?;
            let rows: Vec<&&Pick> = rows.iter().filter(|p| p.form == form).collect();
            rows.iter().find(|p| p.outfit == o).or_else(|| rows.first()).map(|p| (**p).clone())
        };
        let next = match col {
            0 => {
                let cs = Self::distinct(self.picks.iter().map(|p| p.character.as_str()));
                let k = cs.iter().position(|c| *c == cur.character).unwrap_or(0) as i32;
                let c = cs[wrap(k + d, cs.len())].to_string();
                pick(&c, &cur.form, &cur.outfit, &self.picks)
            }
            1 => {
                let fs = Self::distinct(self.picks.iter().filter(|p| p.character == cur.character).map(|p| p.form.as_str()));
                let k = fs.iter().position(|f| *f == cur.form).unwrap_or(0) as i32;
                let f = fs[wrap(k + d, fs.len())].to_string();
                pick(&cur.character, &f, &cur.outfit, &self.picks)
            }
            _ => {
                let os = self.outfits();
                let k = os.iter().position(|o| *o == cur.outfit).unwrap_or(0) as i32;
                let o = os.get(wrap(k + d, os.len())).map(|o| o.to_string()).unwrap_or_default();
                pick(&cur.character, &cur.form, &o, &self.picks)
            }
        };
        if let Some(p) = next {
            self.settings.battle.fighter = p.fighter;
            self.settings.battle.form = Some(p.form_id);
        }
        Choice::None
    }

    /// the Battle page's opponents (enemy id `tN`, name), in the page's order after the story presets
    pub fn enemy_list(&self) -> &[(String, String)] {
        &self.enemies
    }

    /// the arena the battle uses (a saved one no longer offered: the default)
    fn arena_id(&self) -> String {
        let b = &self.settings.battle;
        if self.arenas.iter().any(|a| a.0 == b.arena) {
            b.arena.clone()
        } else {
            DEFAULT_ARENA.into()
        }
    }

    /// The battle the Battle page describes.
    /// The Level row's default, an even match: the opponent's level (its ROM definition's level byte - Majin Buu
    /// 100-110, Super Buu 125-147, Kid Buu 150), 50 .. 200
    pub fn auto_level(&self) -> i32 {
        let b = &self.settings.battle;
        b.enemy
            .trim_start_matches('t')
            .parse::<i32>()
            .ok()
            .and_then(|t| self.enemy_levels.get(&t).copied())
            .unwrap_or(60)
            .max(self.picks.get(self.pick_idx()).and_then(|p| FUSION_LEVELS.iter().find(|f| f.0 == p.fighter)).map(|f| f.1).unwrap_or(50))
            .clamp(50, 200)
    }

    /// the fighter's level the battle uses (the Level row)
    pub fn battle_level(&self) -> i32 {
        let b = &self.settings.battle;
        if b.level_auto {
            self.auto_level()
        } else {
            b.level.clamp(50, 200)
        }
    }

    pub fn battle_config(&self) -> BattleConfig {
        let b = &self.settings.battle;
        // the picker's row: the exact form record (its transformations follow the game's chain from it)
        let (fid, form) = self.picks.get(self.pick_idx()).map(|p| (p.fighter.clone(), p.form_id)).unwrap_or(("goku".into(), 2));
        let level = self.battle_level();
        // the full moveset at full strength: every skill at its top level (only the five main characters' two
        // upgradable skills have levels), the level's stat points spent evenly, full EP, and an SS / SS3 form with its
        // gauge full (a form reached by transforming always is)
        let skills = self.data.as_ref().and_then(|d| arena_skills(d, &fid, form));
        let mut f = FighterSetup { fighter: fid, form, level, auto_spend: "even".into(), skill_level: 2, full_gauge: true, skills, engage: true, ..Default::default() };
        // Hercule, Videl, Pikkon (record slot 8): the slot never grows in the game (50 HP, STR 17 at every level), so
        // the battle takes Goku's record at this level - HP, EP, stats with the points spent evenly - and keeps their
        // own skill list (Energy Blast)
        if let Some(d) = self.data.as_ref().filter(|d| d.fighter(&f.fighter).is_some_and(|fd| fd.character == 8 && fd.fusion.is_none())) {
            let (r, _, _) = wf_core::battle::build_record(d, &FighterSetup { fighter: "goku".into(), level, auto_spend: "even".into(), ..Default::default() });
            f.fixed = Some(wf_core::battle::FixedRecord { hp: r.max_hp, ep: r.max_ep, stats: [r.str_, r.pow, r.end, r.spd], skills: None });
        }
        // one opponent, ended by a KO; the arena rules (`Options::arena`) over the game's
        let phases = vec![Phase { enemy: b.enemy.clone(), count: 1 }];
        let arena = self.arena_id();
        // (the opponent starts in view: 80 px in front of the player)
        BattleConfig { arena, players: vec![f; b.players.clamp(1, 4)], phases, rng: RngInit::Seed(1), xp: true, spawn_near: Some(80), arena_rules: true, even_level: Some(self.auto_level()) }
    }

    fn change(&mut self, row: Row, d: i32) -> Choice {
        // the character picker: Character / Form / Outfit
        match row {
            Row::Fighter => return self.step_pick(0, d),
            Row::Form => return self.step_pick(1, d),
            Row::Outfit => return self.step_pick(2, d),
            _ => {}
        }
        // the Level row: Auto (the enemies' level) and 50 .. 200 by 5 in a ring - from Auto the steps start at the
        // automatic level; past 200 / below 50 it is Auto again
        let auto = self.auto_level();
        let b = &mut self.settings.battle;
        if row == Row::Level {
            let cur = if b.level_auto { auto } else { b.level.clamp(50, 200) };
            let next = cur + d.signum() * 5;
            if !b.level_auto && !(50..=200).contains(&next) {
                b.level_auto = true;
            } else {
                b.level_auto = false;
                b.level = next.clamp(50, 200);
            }
            return Choice::None;
        }
        match row {
            // (the picker's rows and the level: above)
            Row::Fighter | Row::Form | Row::Outfit | Row::Level => {}
            // the opponent: one fighter
            Row::Enemy => {
                let k = self.enemies.iter().position(|e| e.0 == b.enemy).unwrap_or(0);
                if let Some(e) = self.enemies.get(wrap(k as i32 + d, self.enemies.len())) {
                    b.enemy = e.0.clone();
                }
            }
            Row::Arena => {
                let k = self.arenas.iter().position(|a| a.0 == b.arena).unwrap_or(0);
                if let Some(a) = self.arenas.get(wrap(k as i32 + d.signum(), self.arenas.len())) {
                    b.arena = a.0.clone();
                }
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
            Row::LinkMode => self.settings.link_mode = wrap(self.settings.link_mode as i32 + d.signum(), 3) as u8,
            Row::LinkRoom => self.settings.link_room = wrap(self.settings.link_room as i32 + d, 10_000) as u32,
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
            // the character select: A on any of its rows starts the fight
            Row::Fighter | Row::Form | Row::Outfit | Row::Level | Row::Enemy | Row::Arena | Row::Players
                if self.screen == Screen::Battle =>
            {
                return Choice::Battle(self.battle_config())
            }
            Row::Mod(_) | Row::Fullscreen | Row::Pacing | Row::AutoZoom => return self.change(row, 1),
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
            Row::TitleScreen if self.settings.link_mode != 0 => (t("Title screen"), t("Multiplayer with a friend")),
            Row::TitleScreen => (t("Title screen"), t("New Game / Continue")),
            Row::LinkMode => (
                t("Link play"),
                format!("< {} >", ["Off", "Host a room", "Join a room"][self.settings.link_mode.min(2) as usize]),
            ),
            Row::LinkRoom => (t("Room"), format!("< {:04} >", self.settings.link_room)),
            Row::NewGame => (t("Quick start"), t("Snakeway, no prologue")),
            Row::Slot(i) => (format!("Continue {}", i + 1), self.slots.get(i).cloned().flatten().unwrap_or_else(|| t("- empty -"))),
            Row::Fighter => (t("Character"), format!("< {} >", self.picks.get(self.pick_idx()).map(|p| p.character.as_str()).unwrap_or("?"))),
            Row::Form => (t("Form"), format!("< {} >", self.picks.get(self.pick_idx()).map(|p| p.form.as_str()).unwrap_or("?"))),
            Row::Outfit => (t("Outfit"), format!("< {} >", self.picks.get(self.pick_idx()).map(|p| p.outfit.as_str()).unwrap_or("?"))),
            Row::Level if b.level_auto => (t("Level"), format!("< Even: {} >", self.auto_level())),
            Row::Level => (t("Level"), format!("< {} >", b.level.clamp(50, 200))),
            Row::Enemy => (t("Enemy"), format!("< {} >", self.enemies.iter().find(|e| e.0 == b.enemy).map(|e| e.1.as_str()).unwrap_or(b.enemy.as_str()))),
            Row::Arena => {
                let id = self.arena_id();
                let name = self.arenas.iter().find(|a| a.0 == id).map(|a| a.1.clone()).unwrap_or(id);
                (t("Arena"), format!("< {name} >"))
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
        // the Battle page: a character select in the game's menu style (the plain list without the game's menu data)
        if self.screen == Screen::Battle && !self.rebind.open {
            let n = self.rows().len();
            self.cursor = self.cursor.min(n.saturating_sub(1));
            if battle_page::draw(self, packs, fb, w, h) {
                return;
            }
        }
        // Story / Settings / Mods: the game's menu style, like the Battle page
        if !main && self.screen != Screen::Battle && !self.rebind.open {
            let rows = self.rows();
            self.cursor = self.cursor.min(rows.len().saturating_sub(1));
            let (title, note) = match self.screen {
                Screen::Story => ("Story", "Save at the save points"),
                Screen::Mods if self.mods.is_empty() => ("Mods", "No mods yet"),
                Screen::Mods => ("Mods", "Applies to the next game"),
                Screen::Settings => ("Settings", ""),
                _ => ("", ""),
            };
            let list: Vec<battle_page::ListRow> = rows
                .iter()
                .map(|&r| {
                    let (label, value) = self.label(r);
                    battle_page::ListRow { label, value, accent: r == Row::Fight }
                })
                .collect();
            let (cursor, message) = (self.cursor, self.message.clone());
            if battle_page::list_page(self, packs, fb, w, h, title, note, &list, cursor, &message) {
                return;
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
        // the value column starts after the screen's widest label ("Original pacing" ran into its value at the old
        // fixed x 78), and the values are cut to the room left of the panel's right edge
        let lw = rows.iter().filter(|&&r| !self.label(r).1.is_empty()).map(|&r| width(&self.label(r).0)).max().unwrap_or(0);
        let vx = (14 + lw + 8).max(78);
        let room = (232 - vx - 4).max(40);
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
                while width(&v) > room && v.chars().count() > 6 {
                    // keep "< ... >": drop characters before the closing bracket
                    let cut = v.char_indices().rev().nth(2).map(|(i, _)| i).unwrap_or(0);
                    v.remove(cut);
                }
                text(fb, &v, ox + vx, y, col);
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
    // the area as the in-game Select Game names it (its banner text), the code only for an area without one
    // (playtest #21: "1-10" read as a code)
    let area = d
        .field
        .as_ref()
        .and_then(|f| f.areas.get(&(sd.world, sd.area)))
        .and_then(|a| Some(a.banner.trim()).filter(|b| !b.is_empty()).map(str::to_string).or_else(|| a.name.clone()))
        .unwrap_or_else(|| format!("{}-{}", sd.world, sd.area));
    // (the play time before the area: a long area name is what a narrow line cuts - it lost the time, playtest 21)
    Some(format!("{name} Lv{lv} {}:{:02} {area}", mins / 60, mins % 60))
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
