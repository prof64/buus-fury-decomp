//! Link play: the **Dojo** (title -> Multiplayer -> Dojo; `re/buu/menu/NOTES_screens.md`, engine/docs/netcode.md
//! "Link play: Dojo"): two players pick one of their saves and a character from its party, the host sets "Character
//! Mode:" (Balanced / In-Game) and "Rounds:" (1 / 3 / 5), and they fight a rollback versus match in the Dojo
//! (area 0-90) in rounds; the knockdown of the match drops a prize (coins, perhaps a supply) the winner collects in
//! the 300 updates before the result - "Grab your prize!" -, the loser "Go back to training!".
//!
//! Pieces (all deterministic plain data; the network is the lobby's `dojo` room kind + the rollback session):
//! * [`DojoSave`]: what a peer shows of a local save (its party: fighter, form, level, stats, equipment), built from a
//!   [`SaveData`] - the game's save pick ("Select Saved Game", "No Saved Games").
//! * [`DojoSetup`] / [`DojoHello`]: the host's mode + rounds (in its hello / the `start`'s extra) and each peer's save
//!   summary + pick + the setup it saw (the confirm step: the host starts only when the guest echoes its setup).
//! * [`fighter_setup`]: a pick in a mode -> the battle `FighterSetup`.
//! * [`DojoGame`]: the rounds as one rollback [`Game`]: a PvP battle `Session` per round in the "dojo" arena, the
//!   round's end (a KO, at once, or the round timer: a draw), the pause between rounds, the
//!   score, the match result.
//! * [`DojoMatch`]: a peer's end of the match: the result once its frame is confirmed by every input, an optional prize
//!   written to the local save exactly once, "Disconnected" (no result, no prize) if the link drops first.
//!
//! What comes from the ROM (static disassembly, and recordings of two linked emulators, tools/link/dojo_record.py ->
//! data/link/dojo_*.json, gated by tests/dojo_link.rs - engine/docs/netcode.md "Link play: Dojo" has the table with
//! addresses):
//! * MEASURED: the texts (strings 79..139: Select Saved Game 94, Dojo 95, Start Fight 96, Character Mode 97..99,
//!   Rounds 100..103, "Choose your character!" 105, Round 1..5 106..110, You Win!! / You Lose!! / Draw!! 111..113,
//!   Grab your prize! / Go back to training! 114 / 115, Disconnected 93); the fight (0x0802193A, state block
//!   0x03002BFC: +0 / +1 the two players' round wins, +4 the round timer's seconds): the RNG words 0x03004A5C / 60 are
//!   zeroed once when the fight starts (both carts from (0, 0); the generator then runs on through every round); area
//!   0-90 (0x08008E70), the two fighters 50 px left / right of (152, 220); "Round N" = 106 + both players' wins (a
//!   drawn round keeps its number); a round: 180 seconds (0xB4) of 60 updates each; a knockdown (HP, record +0x16, at
//!   0) decides it 300 updates (0x12C) later - one down: the other wins the round, both down: Draw!! - the timer, the
//!   fighters and the fallen one's regeneration running on; the timer at 0 with both standing: Draw!! (no HP
//!   comparison); then 63 updates of the old fighters, the next round's made in an update of their own, 125 of "Round
//!   N" (`DojoRules`, measured); the match runs until a player has more than rounds / 2 wins (draws do not count: no
//!   limit on the number of rounds). The knockdown that decides the match runs the loot script s498 with the fallen
//!   fighter's level (`player::link_prize`): coins (s500) and at most a first-roll supply (s499), picked up as in the
//!   field (coins by touch once landed, a supply with A and its 61-update hold) - the zenie goes into the game data in
//!   RAM; the only save write of the link code is the Trading Post's (0x080231AE). Character Mode: Balanced (the option page's
//!   mode byte 1) = a fixed roster of 32 fighters (strings 116..147: Goku .. Uub, list 0x08023120) with the table
//!   0x0872BFCC's records - HP 15000, EP 100, STR 67, POW 67, END 25, SPD 50 (Hercule STR 60 / POW 74; Broly and Majin Buu .. Super Buu 3 END 30 / SPD 45), one skill
//!   (7, Energy Blast) - built by 0x08022904 / 0x0801339C; In-Game (mode byte 0) = the save's party (the 12-byte list
//!   at +0x584, names 158+) with each character's own record packed whole (0x08022934 -> 0x080229B0: level, form,
//!   skills and skill levels, max HP / EP as current, STR..SPD, equipment, the selected skill). A fighter as made has
//!   the plain form controller and no form stat bonus whatever its form (`Player::link_plain`) until it transforms.
//! * INFERRED: that the host's mode / rounds apply to both (the option page's link exchange is not decoded; the
//!   recordings set both sides' to the same).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_core::battle::{BattleConfig, FighterSetup, FixedRecord, RngInit};
use wf_core::content::GameData;
use wf_core::menu::save::SaveData;
use wf_core::progression::{record_at_level, GrowthMode};
use wf_core::session::{Session, Snapshot};

use crate::core_game::MatchConfig;
use crate::game::{chain_mix, Game};

/// the Dojo arena (area 0-90; `wf_core::battle::ARENAS`)
pub const ARENA: &str = "dojo";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DojoMode {
    /// "Balanced Mode" (97): fixed stats for everyone
    #[default]
    Balanced,
    /// "In-Game Mode" (98): the save's own character
    InGame,
}

/// The host's choices (it sends them in its hello and the `start`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoSetup {
    pub mode: DojoMode,
    /// 1, 3 or 5
    pub rounds: u8,
}

impl Default for DojoSetup {
    fn default() -> Self {
        DojoSetup { mode: DojoMode::Balanced, rounds: 3 }
    }
}

impl DojoSetup {
    pub fn valid(&self) -> bool {
        matches!(self.rounds, 1 | 3 | 5)
    }
}

/// One character a save offers ("Choose your character!").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoChar {
    pub name: String,
    /// record character index
    pub character: i32,
    /// battle fighter id (`wf_core::battle::FIGHTERS`)
    pub fighter: String,
    pub form: i32,
    pub level: i32,
    /// STR, POW, END, SPD
    pub stats: [i32; 4],
    pub equip: [i32; 4],
    /// the save record's max HP / EP and known skills (In-Game Mode fights with the record as is)
    #[serde(default)]
    pub max_hp: i32,
    #[serde(default)]
    pub max_ep: i32,
    #[serde(default)]
    pub skills: Vec<i32>,
    /// the selected skill and each skill's chosen level (In-Game Mode: the record's)
    #[serde(default)]
    pub sel: usize,
    #[serde(default)]
    pub skill_levels: Vec<i32>,
}

/// What a peer shows of one of its saves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DojoSave {
    pub slot: usize,
    pub party: Vec<DojoChar>,
}

impl DojoSave {
    /// the save's party as Dojo characters (None: no party member the battle can play - "No Saved Games")
    pub fn from_save(slot: usize, s: &SaveData, d: &GameData) -> Option<DojoSave> {
        let mut party = Vec::new();
        for (ci, c) in s.story.chars.iter().enumerate() {
            if !c.in_party() {
                continue;
            }
            let Some(f) = d.fighters.iter().find(|f| f.character == ci as i32 && f.fusion.is_none()) else { continue };
            let form = if f.forms.contains(&c.form) { c.form } else { f.default_form };
            // the shown name: the game's roster string for this fighter + form (strings 116..139: "SS Goku" ..), else
            // the form's name ("Goku"), else the fighter's - never the bare fighter id (playtest 26: "goku Lv90")
            let name = BALANCED_ROSTER
                .iter()
                .find(|r| r.1 == f.id && r.2 == form)
                .map(|r| r.0.to_string())
                .or_else(|| d.form(form).map(|x| x.name.clone()).filter(|n| !n.is_empty()))
                .unwrap_or_else(|| f.name.clone());
            party.push(DojoChar {
                name,
                character: ci as i32,
                fighter: f.id.clone(),
                form,
                level: c.rec.level,
                stats: [c.rec.str_, c.rec.pow, c.rec.end, c.rec.spd],
                equip: c.equip,
                max_hp: c.rec.max_hp,
                max_ep: c.rec.max_ep,
                skills: c.skills.clone(),
                sel: c.sel.max(0) as usize,
                skill_levels: (0..c.skills.len()).map(|k| c.skill_cur(k)).collect(),
            });
        }
        (!party.is_empty()).then_some(DojoSave { slot, party })
    }
}

/// A peer's payload in the `dojo` room's hellos.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DojoHello {
    pub save: DojoSave,
    /// index into `save.party`
    pub pick: usize,
    /// host: its choices; guest: the host's choices it saw (the confirm step)
    pub setup: Option<DojoSetup>,
    /// the character is picked (A on "Choose your character!"), and the pick as the game's page counts it (Balanced:
    /// the roster index; In-Game: the character id) - what the friend's page takes
    #[serde(default)]
    pub picked: bool,
    #[serde(default)]
    pub page_pick: i32,
    /// the matches played on this link so far: a pick counts for the match it was made for (after a match both are
    /// back on "Choose your character!" - the friend's last hello still shows its pick of the match before)
    #[serde(default)]
    pub played: u32,
}

/// The rule constants - the defaults are the ROM's (module doc); tests shorten them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoRules {
    /// frames per round before the timer ends it in a draw (ROM: 180 s x 60 updates)
    pub round_frames: u32,
    /// a round's result to the next round's fighters: the fighters go on without keys (63 updates, measured: the link
    /// recording dojo_b3)
    pub between_frames: u32,
    /// a round's start: its fighters stand while "Round N" shows, before the timer runs and the keys count (their idle
    /// animations and EP regeneration go on) - 125 updates from their creation, measured from the link recording
    /// (tools/link/dojo_record.py, data/link/dojo_fight1: made at frame 1,521, the timer at 180 from 1,646)
    #[serde(default = "d_start_frames")]
    pub start_frames: u32,
    /// Balanced Mode's HP (ROM: 15000)
    pub balanced_hp: i32,
    /// most rounds before the match ends with equal wins as Draw!! - 0 = no limit (the ROM: only a majority ends it)
    pub max_rounds: u16,
    /// a supply written to the winner's save - None = the ROM (the Dojo gives no item: "Grab your prize!" is text)
    pub prize: Option<usize>,
}

fn d_start_frames() -> u32 {
    125
}

impl Default for DojoRules {
    fn default() -> Self {
        DojoRules { round_frames: 180 * 60, between_frames: 63, start_frames: d_start_frames(), balanced_hp: 15000, max_rounds: 0, prize: None }
    }
}

/// where the fighters stand when a round starts (24.8; both facing down): measured from the link recording
pub const SPAWN: [(i32, i32); 2] = [(102 << 8, 220 << 8), (202 << 8, 220 << 8)];

/// a round's fighters as the game makes them (link recording dojo_fight1, exact from the generator's zero): each in
/// slot order draws rand(255) (unused here) and its idle tick rand(255), stands at `SPAWN` facing down
pub fn place_fighters(s: &mut Session) {
    let ids = s.world.players.clone();
    for (slot, id) in ids.into_iter().enumerate().take(2) {
        let Some(i) = s.world.find(id) else { continue };
        s.world.rng.rand(255);
        let t = s.world.rng.rand(255);
        let p = s.world.pl(i);
        p.a.x = SPAWN[slot].0;
        p.a.y = SPAWN[slot].1;
        p.a.facing = 0;
        p.ap.tick = t;
    }
}

/// The Balanced roster (ROM: strings 116..147, records 0x0872BFCC 28 bytes each: sprite id, HP 15000, EP 100, STR,
/// POW, END, SPD): (name, our fighter, our form, STR, POW, END, SPD); the form is the one whose gfx id is the record's
/// sprite (the lowest when two share it).
pub const BALANCED_ROSTER: [(&str, &str, i32, [i32; 4]); 32] = [
    ("Goku", "goku", 0, [67, 67, 25, 50]),
    ("SS Goku", "goku", 3, [67, 67, 25, 50]),
    ("SS3 Goku", "goku", 5, [67, 67, 25, 50]),
    ("Teen Gohan", "gohan", 10, [67, 67, 25, 50]),
    ("SS Teen Gohan", "gohan", 16, [67, 67, 25, 50]),
    ("Mystic Gohan", "gohan", 13, [67, 67, 25, 50]),
    ("Great Saiyaman", "gohan", 14, [67, 67, 25, 50]),
    ("Gohan", "gohan", 15, [67, 67, 25, 50]),
    ("SS Gohan", "gohan", 18, [67, 67, 25, 50]),
    ("Trunks", "trunks", 21, [67, 67, 25, 50]),
    ("SS Trunks", "trunks", 22, [67, 67, 25, 50]),
    ("Vegeta", "vegeta", 26, [67, 67, 25, 50]),
    ("SS Vegeta", "vegeta", 28, [67, 67, 25, 50]),
    ("Goten", "goten", 19, [67, 67, 25, 50]),
    ("SS Goten", "goten", 20, [67, 67, 25, 50]),
    ("Gotenks", "gotenks", 31, [67, 67, 25, 50]),
    ("SS Gotenks", "gotenks", 34, [67, 67, 25, 50]),
    ("SS3 Gotenks", "gotenks", 35, [67, 67, 25, 50]),
    ("Vegito", "vegito", 39, [67, 67, 25, 50]),
    ("SS Vegito", "vegito", 40, [67, 67, 25, 50]),
    ("SS Gogeta", "gogeta", 41, [67, 67, 25, 50]),
    ("Hercule", "f47", 47, [60, 74, 25, 50]),
    ("Videl", "f52", 52, [67, 67, 25, 50]),
    ("Pikkon", "f50", 50, [67, 67, 25, 50]),
    // (no form has their sprite: the plain form 45 - what the game gives every Balanced fighter - with the record's
    // sprite, `BALANCED_SPRITES`)
    ("Piccolo", "f45", 45, [67, 67, 25, 50]),
    ("Broly", "f45", 45, [67, 67, 30, 45]),
    ("Majin Buu", "f45", 45, [67, 67, 30, 45]),
    ("Super Buu", "f45", 45, [67, 67, 30, 45]),
    ("Super Buu 2", "f45", 45, [67, 67, 30, 45]),
    ("Super Buu 3", "f45", 45, [67, 67, 30, 45]),
    ("Kid Buu", "f45", 45, [67, 67, 25, 50]),
    ("Uub", "f45", 45, [67, 67, 25, 50]),
];
/// the roster records' sprite ids (ROM) of the entries from 24 on (Piccolo .. Uub)
pub const BALANCED_SPRITES: [i32; 8] = [222, 38, 43, 49, 51, 52, 47, 273];
/// Balanced Mode's EP and its one skill (ROM: 100, skill 7 Energy Blast)
pub const BALANCED_EP: i32 = 100;
pub const BALANCED_SKILLS: [i32; 1] = [7];

/// The Balanced roster as Dojo characters ("Choose your character!" in Balanced Mode: the same 32 for every save).
pub fn balanced_roster() -> Vec<DojoChar> {
    BALANCED_ROSTER
        .iter()
        .map(|&(name, fighter, form, stats)| DojoChar {
            name: name.into(),
            character: -1,
            fighter: fighter.into(),
            form,
            level: 50,
            stats,
            equip: [0, 44, 77, 112],
            max_hp: 0,
            max_ep: 0,
            skills: Vec::new(),
            sel: 0,
            skill_levels: Vec::new(),
        })
        .collect()
}

/// A pick in a mode -> the fighter the battle builds (module doc: the ROM's two record builders).
/// Balanced: the roster entry of the pick's fighter and form (else the fighter's first roster entry) with its fixed
/// record; In-Game: the save's record as is.
pub fn fighter_setup(c: &DojoChar, mode: DojoMode, rules: &DojoRules, d: &GameData) -> FighterSetup {
    match mode {
        DojoMode::Balanced => {
            let k = BALANCED_ROSTER
                .iter()
                .position(|e| e.1 == c.fighter && e.2 == c.form && e.0 == c.name)
                .or_else(|| BALANCED_ROSTER.iter().position(|e| e.1 == c.fighter && e.2 == c.form))
                .or_else(|| BALANCED_ROSTER.iter().position(|e| e.1 == c.fighter))
                .unwrap_or(0);
            let e = &BALANCED_ROSTER[k];
            FighterSetup {
                sprite: k.checked_sub(24).and_then(|j| BALANCED_SPRITES.get(j).copied()),
                fighter: e.1.into(),
                form: e.2,
                level: 50,
                auto_spend: "none".into(),
                fixed: Some(FixedRecord { hp: rules.balanced_hp, ep: BALANCED_EP, stats: e.3, skills: Some(BALANCED_SKILLS.to_vec()), level: 50, ..Default::default() }),
                ..Default::default()
            }
        }
        DojoMode::InGame => {
            // (a summary without HP - an older peer: the level's average record)
            let base = d.chars.iter().find(|x| x.index == c.character).map(|ch| record_at_level(ch, c.level, &d.xp_table, GrowthMode::Average, 0));
            let (hp, ep) = match base {
                _ if c.max_hp > 0 => (c.max_hp, c.max_ep),
                Some(b) => (b.max_hp, b.max_ep),
                None => (100, 100),
            };
            FighterSetup {
                fighter: c.fighter.clone(),
                form: c.form,
                level: c.level,
                auto_spend: "none".into(),
                equip: c.equip,
                fixed: Some(FixedRecord {
                    hp,
                    ep,
                    stats: c.stats,
                    skills: (!c.skills.is_empty()).then(|| c.skills.clone()),
                    sel: c.sel,
                    skill_levels: c.skill_levels.clone(),
                    level: c.level,
                }),
                ..Default::default()
            }
        }
    }
}

/// The match both peers build: PvP in the Dojo, no enemies, no XP. The generator starts from (0, 0) as the ROM's
/// fight zeroes it (`seed` is kept for the lobby's API and ignored).
pub fn match_config(p1: FighterSetup, p2: FighterSetup, _seed: u32) -> MatchConfig {
    MatchConfig::pvp(
        BattleConfig { arena: ARENA.into(), players: vec![p1, p2], phases: Vec::new(), rng: RngInit::State(0, 0), xp: false, spawn_near: None, arena_rules: false, even_level: None },
        false,
    )
}

/// The host's `LobbySettings.base` for a `dojo` room (its players are replaced by the seated peers' fighters).
pub fn base_config(seed: u32) -> MatchConfig {
    match_config(FighterSetup::default(), FighterSetup::default(), seed)
}

/// How a round ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RoundResult {
    /// slot 0 / 1 won
    Win(u8),
    Draw,
}

/// The match result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum DojoResult {
    Win(u8),
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Phase {
    /// "Round N" up: the round starts in `left` frames
    Between { left: u32 },
    Fight,
    Over,
}

/// The rounds' bookkeeping (part of the rollback state).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct DojoState {
    /// 0-based
    pub round: u8,
    pub phase: Phase,
    /// frames into the current round
    pub t: u32,
    pub wins: [u8; 2],
    pub results: Vec<RoundResult>,
    /// the frame index (state index) the match ended
    pub over_at: Option<i32>,
    /// state index (frames simulated)
    pub frame: i32,
    /// updates since a fighter went down this round (0: none): the round's result comes `KO_FRAMES` after it (the
    /// timer runs on)
    #[serde(default)]
    pub down_for: u32,
    /// the prize picked up so far (zenie: the coins the match-deciding knockdown dropped, `wf_core::player::link_prize`)
    #[serde(default)]
    pub zenie: i32,
    /// the round's end banner's place in the world list (the next entity id when it was made: the draw order)
    #[serde(default)]
    pub banner_place: Option<u32>,
}



/// a knockdown to the round's result (link recording dojo_fight2: HP 0 at frame 6,757, the win at 7,056)
pub const KO_FRAMES: u32 = 300;

/// The whole Dojo match as one rollback game.
pub struct DojoGame {
    data: Arc<GameData>,
    pub cfg: MatchConfig,
    pub setup: DojoSetup,
    pub rules: DojoRules,
    pub st: DojoState,
    pub s: Session,
    /// the OBJs each side shows: its draw pass before the last update (the presenter's one-frame lag) in that side's
    /// draw order (`wf_core::view::render_view_link`) - not rollback state, like the session's presenter
    shown: [Option<Vec<wf_view::SpriteDraw>>; 2],
    /// the damage numbers' shake pixels on each side (its own camera's, from the same build as `shown`)
    num_shake: [(i32, i32); 2],
    /// the link is lost (`link_lost`): the updates since - the link error's script runs over the fight (its
    /// `hide_hud`, its 16-step fade-out), the fighters going on without keys; not rollback state
    lost: Option<i32>,
    /// ... and each side's BG camera as the picture had it when the link was lost (it stays there under the script)
    lost_cams: [(i32, i32); 2],
}

impl DojoGame {
    pub fn new(data: Arc<GameData>, cfg: MatchConfig, setup: DojoSetup, rules: DojoRules) -> DojoGame {
        let s = Self::round_session(&data, &cfg, None, setup.rounds, [0; 2]);
        DojoGame {
            data,
            cfg,
            setup,
            rules,
            st: DojoState { round: 0, phase: Phase::Between { left: rules.start_frames.max(1) }, t: 0, wins: [0; 2], results: Vec::new(), over_at: None, frame: 0, down_for: 0, zenie: 0, banner_place: None },
            s,
            shown: [None, None],
            num_shake: [(0, 0); 2],
            lost: None,
            lost_cams: [(0, 0); 2],
        }
    }
    /// the draw pass's sprites on both sides, before an update
    fn draw_pass(&mut self) {
        let mut num_shake = [(0, 0); 2];
        self.shown = [0u8, 1].map(|s| {
            let mut v = wf_view::RenderView::default();
            wf_core::view::render_view_link(&self.s.world, &self.link_draw(s), &mut v);
            num_shake[s as usize] = v.damage_numbers.first().map_or((0, 0), |n| n.shake);
            Some(v.sprites)
        });
        self.num_shake = num_shake;
    }
    /// The link is lost: from here the game's link error script (s001) runs over the fight - `hide_hud` (the HUD and
    /// the friend's bar slide out), `fade_out(16)` - while the fighters go on without keys (two linked emulators, the
    /// guest reset mid-fight: wf-app tests/dojo_pixels.rs `dojo_lost*`). [`DojoGame::lost_step`] is one such update
    pub fn link_lost(&mut self) {
        if self.lost.is_none() {
            self.lost = Some(0);
            for s in 0..2 {
                let c = self.s.world.cameras[s].rect;
                self.lost_cams[s] = (c.x0, c.y0);
            }
        }
    }
    /// one update under the link error (the app calls it once a frame until the script's scene loads). The first is
    /// the update the host waited in: nothing moves, the picture is the last update's. From the second - the
    /// script's iterations - the fighters stay where they are and take no keys, but their animations run on (an
    /// attack ends in the idle stance) and the other objects go on (a blast flies, a damage number rises); the
    /// cameras ease on (the OBJs follow them, the BG stays where it was), and from the third the HUD and the
    /// friend's bar slide out and the picture fades, 16 a step (two linked emulators, the guest reset mid-fight:
    /// wf-app tests/dojo_pixels.rs `dojo_lost*`, the fighters' ticks in their rows)
    pub fn lost_step(&mut self) {
        let Some(j) = self.lost else { return };
        self.lost = Some(j + 1);
        self.s.presenter.before_step(&self.s.world);
        self.draw_pass();
        if j == 0 {
            return;
        }
        // (the fighters held - `World::link_held` -, the shots, the numbers, the cameras going on)
        self.s.world.link_held = true;
        self.s.step(&[0, 0]);
    }
    /// the fade-out is done (the screen is black: the script's scene follows)
    pub fn lost_faded(&self) -> bool {
        self.lost.is_some_and(|j| j >= 18)
    }

    /// that side's world list beyond the props and fighters (their sort keys on our scale: layer 0x38 - the game's,
    /// depth the game's - 4): "Round N" (a title text: layer 0, depth 0) while it shows; from the fight's start the
    /// HUD and the boss bar (layer 16, depth 0) and the round timer (layer 0, depth 0x7FFFFFFF); the round's end
    /// banner (a title text) from its making
    fn link_draw(&self, slot: u8) -> wf_core::view::LinkDraw {
        let (made, result) = (self.since_made().map(|k| k as i32), self.since_result().map(|j| j as i32));
        let mut objs = vec![];
        if made.is_some_and(|k| (65..125).contains(&k)) {
            objs.push((0, (0x38, -4)));
        }
        let hud = matches!((made, result), (Some(k), _) if k >= 126) || result.is_some_and(|j| 9 - j >= -16);
        let bar = matches!((made, result), (Some(k), _) if k >= 126) || result.is_some_and(|j| 9 - j >= -8);
        if hud {
            objs.push((0, (0x38 - 16, -4)));
        }
        if bar {
            objs.push((0, (0x38 - 16, -4)));
        }
        if matches!(self.st.phase, Phase::Fight) && made.is_some_and(|k| k >= 126) {
            objs.push((0, (0x38, i32::MAX - 4)));
        }
        if let (Some(place), true) = (self.st.banner_place, self.banner_shown()) {
            objs.push((place, (0x38, -4)));
        }
        wf_core::view::LinkDraw { local: slot as usize, objs }
    }
    /// the round's end banner is up (0x08021D56: two updates into a knockdown, or the time up, to the result)
    fn banner_shown(&self) -> bool {
        match (self.st.phase, self.since_result()) {
            (Phase::Fight, _) => self.st.down_for >= 2 || (self.st.down_for == 0 && self.st.t >= self.rules.round_frames),
            (_, Some(0)) => true,
            _ => false,
        }
    }
    /// a round: the match's fighters afresh; the generator goes on from where the last round left it (`rng`; the ROM
    /// zeroes it once, when the fight starts)
    fn round_session(data: &Arc<GameData>, cfg: &MatchConfig, rng: Option<[u32; 2]>, rounds: u8, wins: [u8; 2]) -> Session {
        let mut c = cfg.clone();
        if let Some([a, b]) = rng {
            c.battle.rng = RngInit::State(a, b);
        }
        let mut s = c.session(data.clone());
        s.world.opts.link = true;
        s.world.opts.link_rounds = rounds;
        s.world.opts.link_wins = wins;
        // (a link fighter's stats have no form bonus)
        for i in 0..s.world.ents.len() {
            if s.world.players.contains(&s.world.ents[i].id) {
                wf_core::player::link_made(&mut s.world, i);
            }
        }
        place_fighters(&mut s);
        // the round's area load (0x08008E70 at (152, 220)) snaps each side's camera as EnterArea does: (x - 112,
        // y - 76) - it then eases to its fighter through the fade-in (the BG scroll from (40, 144), link recordings)
        for slot in 0..2 {
            wf_core::battle::set_camera(&mut s.world, slot, (152 - 112) << 10, (220 - 76) << 10);
        }
        s
    }
    /// the match result once over
    pub fn result(&self) -> Option<DojoResult> {
        if self.st.phase != Phase::Over {
            return None;
        }
        let need = self.setup.rounds / 2 + 1;
        Some(if self.st.wins[0] >= need || self.st.wins[0] > self.st.wins[1] {
            DojoResult::Win(0)
        } else if self.st.wins[1] >= need || self.st.wins[1] > self.st.wins[0] {
            DojoResult::Win(1)
        } else {
            DojoResult::Draw
        })
    }
    /// the round's end, if it ended this update (ROM 0x08021CC0): a fighter's HP at 0 - one down: the other wins,
    /// both: Draw!!; the timer at 0 with both standing: Draw!!
    /// the prize's next drop picked up (`PRIZE_PICKUP` after the knockdown, one a frame)
    fn prize_pickup(&mut self) {
        self.st.zenie = self.s.world.story.zenie as i32;
    }
    fn downs(&self) -> (bool, bool) {
        let w = &self.s.world;
        let down = |slot: usize| w.players.get(slot).and_then(|&id| w.find(id)).is_none_or(|i| w.pl_ref(i).dead || w.pl_ref(i).rec.hp <= 0);
        (down(0), down(1))
    }
    /// the round's result, if it comes this update: `KO_FRAMES` after a knockdown - who is down then (one: the other
    /// wins, both: Draw!!); the timer at 0 with both standing: Draw!!
    fn round_end(&self) -> Option<RoundResult> {
        if self.st.down_for >= KO_FRAMES {
            return match self.downs() {
                (false, true) => Some(RoundResult::Win(0)),
                (true, false) => Some(RoundResult::Win(1)),
                _ => Some(RoundResult::Draw),
            };
        }
        (self.st.down_for == 0 && self.st.t >= self.rules.round_frames).then_some(RoundResult::Draw)
    }
}

impl DojoGame {
    /// updates since this round's fighters were made (the fade-in, "Round N", the HUD's slide in)
    fn since_made(&self) -> Option<u32> {
        match self.st.phase {
            Phase::Between { left } if left > self.rules.start_frames => None,
            Phase::Over => None,
            _ => Some(self.s.world.frame),
        }
    }
    /// updates since the round's result (the fade-out, the HUD's slide out), while its fighters are still shown
    fn since_result(&self) -> Option<u32> {
        match self.st.phase {
            Phase::Between { left } if left > self.rules.start_frames => Some(self.rules.between_frames + 1 + self.rules.start_frames - left),
            Phase::Over => Some((self.st.frame - self.st.over_at.unwrap_or(self.st.frame)) as u32),
            _ => None,
        }
    }
    /// The match as `slot`'s screen shows it - the link fight's picture on that side's GBA (the fight 0x0802193A
    /// and its modes; engine/docs/netcode.md "Link play: Dojo", the screen): the local fighter's camera and HUD,
    /// the opponent's HP as the boss bar (`0x080195A8(0, remote, 0, 100)`), the round timer's seconds, "Round N"
    /// and the knockdown's banner (title texts), the fades. Rendered as player 0 (`Renderer::render(.., 0, ..)`).
    pub fn view(&self, slot: u8, out: &mut wf_view::RenderView) {
        use wf_view::Sim;
        self.s.view(out);
        // (the game's picture: the fighters under the arena's foreground, no see-through)
        out.see_through.clear();
        let s = (slot as usize).min(1);
        // the BG scroll: the camera before the update, as the story's battles - but the round's start (the fade-in
        // mode 0x080495B4 and "Round N") shows the update's own
        let cam = &self.s.world.cameras[s];
        let c = if self.since_made().is_some_and(|k| k < 126) { cam.rect } else { cam.shown };
        out.cameras = vec![wf_view::CameraView { x: c.x0, y: c.y0 }];
        // (under the link error's script: the camera of the picture the link was lost on)
        let c = if self.lost.is_some_and(|j| j >= 1) {
            out.cameras = vec![wf_view::CameraView { x: self.lost_cams[s].0, y: self.lost_cams[s].1 }];
            cam.rect
        } else {
            c
        };
        let hud = out.hud.get(s).cloned();
        out.hud = hud.into_iter().collect();
        // the OBJs in this side's draw order
        if let Some(sp) = &self.shown[s] {
            out.sprites = sp.clone();
        }
        for n in out.damage_numbers.iter_mut() {
            n.shake = self.num_shake[s];
        }
        // under the link error's script the BG stays where the picture the link was lost on had it; the OBJs are
        // placed by the camera as it goes on
        if self.lost.is_some_and(|j| j >= 1) {
            let (dx, dy) = (self.lost_cams[s].0 - c.x0, self.lost_cams[s].1 - c.y0);
            for sp in out.sprites.iter_mut() {
                sp.x += dx;
                sp.y += dy;
            }
            for n in out.damage_numbers.iter_mut() {
                n.x += dx;
                n.y += dy;
            }
        }
        // the HUD's bars as the frame leaves them (drawn from the fighters' records in the draw pass: an Energy
        // Blast's EP in its own frame - link recording dojo_px1)
        {
            let w = &self.s.world;
            if let (Some(h), Some(p)) = (out.hud.first_mut(), w.players.get(s).and_then(|&id| w.find(id)).map(|i| w.pl_ref(i))) {
                h.hp = p.hp();
                h.ep = p.ep();
            }
        }
        out.boss_bars.clear();
        out.title.clear();
        out.timer = None;
        let made = self.since_made().map(|k| k as i32);
        let result = self.since_result().map(|j| j as i32);
        // the fades (0x080495B4 / 0x080496CC, 4 a frame): in from the fighters' creation, out from the result
        let level = match (made, result) {
            (Some(k), _) => 256 - (4 * k).min(256),
            (_, Some(j)) => (4 * j).min(256),
            _ => 0,
        };
        if level > 0 {
            out.palette_fade = Some(wf_view::PaletteFade { level: level as u16, target: 0 });
        }
        // (the link error's fade: the palette toward black, 16 a step)
        if let Some(j) = self.lost.map(|j| j - 2).filter(|&j| j > 0) {
            out.palette_fade = Some(wf_view::PaletteFade { level: (16 * j).clamp(0, 256) as u16, target: 0 });
        }
        // the HUD and the boss bar: in from update 126 (y -16 / -8, +1 an update, to 8), out after the result
        // (from 8, -1 an update, gone above -16 / -8)
        let lost = self.lost;
        let y = |lo: i32| -> Option<i32> {
            // (the script's `hide_hud`: up a pixel an update, from its second iteration)
            if let Some(j) = lost.map(|j| (j - 2).max(0)) {
                return (8 - j >= lo).then_some(8 - j);
            }
            match (made, result) {
                (Some(k), _) if k >= 126 => Some((lo + k - 126).min(8)),
                (_, Some(j)) if 9 - j >= lo => Some((9 - j).min(8)),
                _ => None,
            }
        };
        match (y(-16), out.hud.first_mut()) {
            (Some(hy), Some(h)) => {
                h.y_off = hy - 8;
                h.hidden = false;
            }
            (None, Some(h)) => h.hidden = true,
            _ => {}
        }
        let w = &self.s.world;
        if let Some(by) = y(-8) {
            let other = w.players.get(1 - s).and_then(|&id| w.find(id)).map(|i| w.pl_ref(i));
            // the remote fighter's fill (vfunc +0x64 0x0804F898: HP * 53 / max HP)
            let fill = other.map(|p| if p.rec.max_hp > 0 { p.rec.hp.max(0) * 53 / p.rec.max_hp } else { 0 }).unwrap_or(0);
            out.boss_bars.push(wf_view::BossBarView { y: by, fill: fill.clamp(0, 53) as u8 });
        }
        let Some(md) = w.data.menu.clone() else { return };
        let t = &md.text;
        // "Round N" (106 + both players' wins) at (120, 80) while its mode runs (60 updates after the fade-in)
        if made.is_some_and(|k| (65..125).contains(&k)) {
            let n = 106 + (self.st.wins[0] as usize + self.st.wins[1] as usize).min(4);
            out.title.push(wf_view::TitleView { text: t.s(n).to_vec(), x: 120, y: 80 });
        }
        // the round's end at (120, 40) (0x08021D56, from this side: one down - You Win!! / You Lose!!, the match's
        // last - Grab your prize! / Go back to training!; both down or the time up - Draw!!)
        if self.banner_shown() {
            let (d0, d1) = self.downs();
            let need = self.setup.rounds as i32 / 2 + 1;
            let (me, them) = (self.st.wins[s] as i32, self.st.wins[1 - s] as i32);
            let (mine, theirs) = if s == 0 { (d0, d1) } else { (d1, d0) };
            let last = result == Some(0);
            let id = match (mine, theirs) {
                (true, false) => if (if last { them } else { them + 1 }) >= need { 115 } else { 112 },
                (false, true) => if (if last { me } else { me + 1 }) >= need { 114 } else { 111 },
                _ => 113,
            };
            out.title.push(wf_view::TitleView { text: t.s(id).to_vec(), x: 120, y: 40 });
        }
        // the round timer (object 0x0802E732): "%d" of the seconds, centred at 8 on a 32x16 OBJ at (120, 12), the
        // menu font in colour 2 with shadow 1, priority 0 - from the fight's second update to the result's (it is
        // deleted in that update, its OAM entry shown once more)
        if (matches!(self.st.phase, Phase::Fight) && made.is_some_and(|k| k >= 127)) || result == Some(0) {
            let secs = 180 - (self.st.t.min(self.rules.round_frames) / 60) as i32;
            let mut cv = wf_core::menu::text::Canvas::new();
            let mut pen = wf_core::menu::text::Pen::new(wf_view::MenuFont::Menu, 8, 7, 1, 2, Some(1));
            pen.clip = [0, 0, 32, 16];
            let digits: Vec<u16> = secs.max(0).to_string().bytes().map(|b| b as u16).collect();
            cv.text(t, &mut pen, &digits);
            let mut v = wf_core::menu::surface_view(cv);
            v.layers[0].w = 32;
            v.layers[0].h = 16;
            v.layers[0].x = 120;
            v.layers[0].y = 12;
            v.overlay = true;
            out.menu = Some(v);
        }
    }
}

impl Game for DojoGame {
    type State = (DojoState, Snapshot);
    fn save(&self) -> Self::State {
        (self.st.clone(), self.s.snapshot())
    }
    fn load(&mut self, s: &Self::State) {
        self.st = s.0.clone();
        self.s.restore(&s.1);
    }
    fn advance(&mut self, inputs: &[u16], _first: bool) {
        self.st.frame += 1;
        match self.st.phase {
            // the match's result: its fighters go on without keys under the fade-out, as between rounds, then the
            // screen is black (the link's menu follows)
            Phase::Over => {
                if self.since_result().is_some_and(|j| j < self.rules.between_frames + 1) {
                    self.s.presenter.before_step(&self.s.world);
                    self.draw_pass();
                    self.s.step(&[0, 0]);
                }
            }
            Phase::Between { left } => {
                // after a round's result its fighters go on for `between_frames` without keys, then the next round's
                // are made (an update of its own) and stand / idle for `start_frames` (link recording dojo_b3: the
                // result at 9,932, the old fighters to 9,995, the new ones made at 9,996, the timer at 180 from 10,121)
                if left == self.rules.start_frames + 1 {
                    self.st.banner_place = None;
                    let rng = self.s.world.rng.state();
                    self.s = Self::round_session(&self.data, &self.cfg, Some(rng), self.setup.rounds, self.st.wins);
                } else {
                    self.s.presenter.before_step(&self.s.world);
                    self.draw_pass();
                    self.s.step(&[0, 0]);
                }
                if left <= 1 {
                    self.st.phase = Phase::Fight;
                    self.st.t = 0;
                } else {
                    self.st.phase = Phase::Between { left: left - 1 };
                }
            }
            Phase::Fight => {
                let k = inputs.iter().map(|&v| v & crate::core_game::BUTTONS_MASK).collect::<Vec<_>>();
                let held = self.s.world.link_get.is_some();
                self.s.presenter.before_step(&self.s.world);
                self.draw_pass();
                self.s.step(&k);
                self.prize_pickup();
                // (a supply's item-get holds the round's clocks for the 61 frames after the pick-up's - link
                // recording dojo_c9: that second of the timer 121 frames, the round's result 61 frames later)
                if !held {
                    self.st.t += 1;
                    if self.st.down_for > 0 || self.downs() != (false, false) {
                        self.st.down_for += 1;
                    }
                }
                if self.st.banner_place.is_none() && self.banner_shown() {
                    self.st.banner_place = Some(self.s.world.next_id);
                }
                if let Some(r) = self.round_end() {
                    wf_core::player::link_round_over(&mut self.s.world);
                    self.st.down_for = 0;
                    self.st.results.push(r);
                    if let RoundResult::Win(x) = r {
                        self.st.wins[x as usize] += 1;
                    }
                    let need = self.setup.rounds / 2 + 1;
                    let capped = self.rules.max_rounds > 0 && self.st.results.len() >= self.rules.max_rounds as usize;
                    if self.st.wins.iter().any(|&w| w >= need) || capped {
                        self.st.phase = Phase::Over;
                        self.st.over_at = Some(self.st.frame);
                    } else {
                        self.st.round = self.st.round.saturating_add(1);
                        self.st.phase = Phase::Between { left: self.rules.between_frames + 1 + self.rules.start_frames };
                    }
                }
            }
        }
    }
    fn checksum(&self, s: &Self::State) -> u64 {
        use std::hash::{Hash, Hasher};
        // the platform-independent hasher (usize lengths as u64): a browser (wasm32) and a native peer in one room
        // must compute the same checksum - DefaultHasher writes a Vec's length as a native usize (4 bytes on
        // wasm32, 8 natively): native vs browser peers saw the same inputs and states but different checksums
        let mut h = wf_core::world::StableHasher::default();
        s.0.hash(&mut h);
        chain_mix(h.finish(), s.1.world.state_hash())
    }
}

/// A peer's end of a match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ending {
    /// "You Win!!" + "Grab your prize!" (`prize`: a supply written to our save - None in the ROM's rules)
    Won { prize: Option<usize> },
    /// "You Lose!!" + "Go back to training!"
    Lost,
    /// "Draw!!"
    Draw,
    /// "Disconnected": the link dropped before the result was confirmed - no result, no prize
    Disconnected,
}

/// A peer's side of the match: its slot, its save, the ending (once).
#[derive(Clone, Debug)]
pub struct DojoMatch {
    pub slot: u8,
    pub ending: Option<Ending>,
}

impl DojoMatch {
    pub fn new(slot: u8) -> DojoMatch {
        DojoMatch { slot, ending: None }
    }
    /// The link dropped: "Disconnected" unless the match already ended.
    pub fn disconnected(&mut self) -> &Ending {
        self.ending.get_or_insert(Ending::Disconnected)
    }
    /// Called every frame with the game's **confirmed** state (every input of every frame up to it known): the
    /// first time it shows the match over, the ending is decided and the winner's prize written into `save` -
    /// exactly once (later calls, rollbacks and re-deliveries change nothing).
    pub fn settle(&mut self, confirmed: &DojoGame, rules: &DojoRules, save: &mut SaveData) -> Option<&Ending> {
        if self.ending.is_some() {
            return self.ending.as_ref();
        }
        let r = confirmed.result()?;
        let e = match r {
            DojoResult::Draw => Ending::Draw,
            DojoResult::Win(x) if x == self.slot => {
                if let Some(p) = rules.prize {
                    let sup = &mut save.story.supplies;
                    if sup.len() <= p {
                        sup.resize(p + 1, 0);
                    }
                    sup[p] = sup[p].saturating_add(1);
                }
                Ending::Won { prize: rules.prize }
            }
            DojoResult::Win(_) => Ending::Lost,
        };
        self.ending = Some(e);
        self.ending.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setups() {
        assert!(DojoSetup { mode: DojoMode::InGame, rounds: 5 }.valid());
        assert!(!DojoSetup { mode: DojoMode::InGame, rounds: 2 }.valid());
        let h = DojoHello { save: DojoSave::default(), pick: 0, setup: Some(DojoSetup::default()), ..Default::default() };
        let j = serde_json::to_value(&h).unwrap();
        assert_eq!(serde_json::from_value::<DojoHello>(j).unwrap(), h);
    }
}
