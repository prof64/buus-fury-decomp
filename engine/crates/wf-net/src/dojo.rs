//! Link play: the **Dojo** (title -> Multiplayer -> Dojo; `re/buu/menu/NOTES_screens.md`, engine/docs/netcode.md
//! "Link play: Dojo"): two players pick one of their saves and a character from its party, the host sets "Character
//! Mode:" (Balanced / In-Game) and "Rounds:" (1 / 3 / 5), and they fight a rollback versus match in the Dojo
//! (area 0-90) in rounds; the winner "Grab[s] your prize!" (a banner only - the ROM writes no item), the loser "Go back to
//! training!".
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
//! What comes from the ROM (static disassembly; the emulator has no link cable - engine/docs/netcode.md "Link play:
//! Dojo" has the table with addresses):
//! * MEASURED: the texts (strings 79..139: Select Saved Game 94, Dojo 95, Start Fight 96, Character Mode 97..99,
//!   Rounds 100..103, "Choose your character!" 105, Round 1..5 106..110, You Win!! / You Lose!! / Draw!! 111..113,
//!   Grab your prize! / Go back to training! 114 / 115, Disconnected 93); the fight (0x0802193A, state block
//!   0x03002BFC: +0 / +1 the two players' round wins, +4 the round timer's seconds): the RNG words 0x03004A5C / 60 are
//!   zeroed once when the fight starts (both carts from (0, 0); the generator then runs on through every round); area
//!   0-90 (0x08008E70), the two fighters 50 px left / right of (152, 220); "Round N" = 106 + both players' wins (a
//!   drawn round keeps its number); a round: 180 seconds (0xB4) of 60 updates each; it ends the update a fighter's HP
//!   (record +0x16) is 0 - one down: the other wins the round, both down: Draw!!, the timer at 0 with both standing:
//!   Draw!! (no HP comparison); the result shows 300 updates (0x12C) before the next round; the match runs until a
//!   player has more than rounds / 2 wins (draws do not count: no limit on the number of rounds); "Grab your prize!" /
//!   "Go back to training!" are only the match winner's / loser's banners - the Dojo writes nothing to the save (the
//!   only save write of the link code is the Trading Post's, 0x080231AE). Character Mode: Balanced (the option page's
//!   mode byte 1) = a fixed roster of 24 fighters (strings 116..139: Goku .. Pikkon, list 0x08023120) with the table
//!   0x0872BFCC's records - HP 15000, EP 100, STR 67, POW 67, END 25, SPD 50 (Hercule STR 60 / POW 74), one skill
//!   (7, Energy Blast) - built by 0x08022904 / 0x0801339C; In-Game (mode byte 0) = the save's party (the 12-byte list
//!   at +0x584, names 158+) with each character's own record packed whole (0x08022934 -> 0x080229B0: level, form,
//!   skills and skill levels, max HP / EP as current, STR..SPD, equipment).
//! * INFERRED: how long "Round N" shows before a round (`between_frames` counts the 300-update result display), and
//!   that the host's mode / rounds apply to both (the option page's link exchange is not decoded).

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
}

/// The rule constants - the defaults are the ROM's (module doc); tests shorten them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoRules {
    /// frames per round before the timer ends it in a draw (ROM: 180 s x 60 updates)
    pub round_frames: u32,
    /// frames between rounds (ROM: the result shows 300 updates)
    pub between_frames: u32,
    /// Balanced Mode's HP (ROM: 15000)
    pub balanced_hp: i32,
    /// most rounds before the match ends with equal wins as Draw!! - 0 = no limit (the ROM: only a majority ends it)
    pub max_rounds: u16,
    /// a supply written to the winner's save - None = the ROM (the Dojo gives no item: "Grab your prize!" is text)
    pub prize: Option<usize>,
}

impl Default for DojoRules {
    fn default() -> Self {
        DojoRules { round_frames: 180 * 60, between_frames: 300, balanced_hp: 15000, max_rounds: 0, prize: None }
    }
}

/// The Balanced roster (ROM: strings 116..139, records 0x0872BFCC 28 bytes each: sprite id, HP 15000, EP 100, STR,
/// POW, END, SPD): (name, our fighter, our form, STR, POW, END, SPD); the form is the one whose gfx id is the record's
/// sprite (the lowest when two share it).
pub const BALANCED_ROSTER: [(&str, &str, i32, [i32; 4]); 24] = [
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
];
/// Balanced Mode's EP and its one skill (ROM: 100, skill 7 Energy Blast)
pub const BALANCED_EP: i32 = 100;
pub const BALANCED_SKILLS: [i32; 1] = [7];

/// The Balanced roster as Dojo characters ("Choose your character!" in Balanced Mode: the same 24 for every save).
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
        })
        .collect()
}

/// A pick in a mode -> the fighter the battle builds (module doc: the ROM's two record builders).
/// Balanced: the roster entry of the pick's fighter and form (else the fighter's first roster entry) with its fixed
/// record; In-Game: the save's record as is.
pub fn fighter_setup(c: &DojoChar, mode: DojoMode, rules: &DojoRules, d: &GameData) -> FighterSetup {
    match mode {
        DojoMode::Balanced => {
            let e = BALANCED_ROSTER
                .iter()
                .find(|e| e.1 == c.fighter && e.2 == c.form)
                .or_else(|| BALANCED_ROSTER.iter().find(|e| e.1 == c.fighter))
                .unwrap_or(&BALANCED_ROSTER[0]);
            FighterSetup {
                fighter: e.1.into(),
                form: e.2,
                level: 50,
                auto_spend: "none".into(),
                fixed: Some(FixedRecord { hp: rules.balanced_hp, ep: BALANCED_EP, stats: e.3, skills: Some(BALANCED_SKILLS.to_vec()) }),
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
                fixed: Some(FixedRecord { hp, ep, stats: c.stats, skills: (!c.skills.is_empty()).then(|| c.skills.clone()) }),
                ..Default::default()
            }
        }
    }
}

/// The match both peers build: PvP in the Dojo, no enemies, no XP. The generator starts from (0, 0) as the ROM's
/// fight zeroes it (`seed` is kept for the lobby's API and ignored).
pub fn match_config(p1: FighterSetup, p2: FighterSetup, _seed: u32) -> MatchConfig {
    MatchConfig::pvp(
        BattleConfig { arena: ARENA.into(), players: vec![p1, p2], phases: Vec::new(), rng: RngInit::State(0, 0), xp: false, spawn_near: None },
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
}

/// The whole Dojo match as one rollback game.
pub struct DojoGame {
    data: Arc<GameData>,
    pub cfg: MatchConfig,
    pub setup: DojoSetup,
    pub rules: DojoRules,
    pub st: DojoState,
    pub s: Session,
}

impl DojoGame {
    pub fn new(data: Arc<GameData>, cfg: MatchConfig, setup: DojoSetup, rules: DojoRules) -> DojoGame {
        let s = Self::round_session(&data, &cfg, None);
        DojoGame {
            data,
            cfg,
            setup,
            rules,
            st: DojoState { round: 0, phase: Phase::Between { left: rules.between_frames }, t: 0, wins: [0; 2], results: Vec::new(), over_at: None, frame: 0 },
            s,
        }
    }
    /// a round: the match's fighters afresh; the generator goes on from where the last round left it (`rng`; the ROM
    /// zeroes it once, when the fight starts)
    fn round_session(data: &Arc<GameData>, cfg: &MatchConfig, rng: Option<[u32; 2]>) -> Session {
        let mut c = cfg.clone();
        if let Some([a, b]) = rng {
            c.battle.rng = RngInit::State(a, b);
        }
        c.session(data.clone())
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
    fn round_end(&self) -> Option<RoundResult> {
        let w = &self.s.world;
        let down = |slot: usize| w.players.get(slot).and_then(|&id| w.find(id)).is_none_or(|i| w.pl_ref(i).dead || w.pl_ref(i).rec.hp <= 0);
        match (down(0), down(1)) {
            (false, true) => return Some(RoundResult::Win(0)),
            (true, false) => return Some(RoundResult::Win(1)),
            (true, true) => return Some(RoundResult::Draw),
            _ => {}
        }
        (self.st.t >= self.rules.round_frames).then_some(RoundResult::Draw)
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
            Phase::Over => {}
            Phase::Between { left } => {
                if left <= 1 {
                    self.st.phase = Phase::Fight;
                    self.st.t = 0;
                } else {
                    self.st.phase = Phase::Between { left: left - 1 };
                }
            }
            Phase::Fight => {
                let k = inputs.iter().map(|&v| v & crate::core_game::BUTTONS_MASK).collect::<Vec<_>>();
                self.s.step(&k);
                self.st.t += 1;
                if let Some(r) = self.round_end() {
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
                        let rng = self.s.world.rng.state();
                        self.s = Self::round_session(&self.data, &self.cfg, Some(rng));
                        self.st.phase = Phase::Between { left: self.rules.between_frames };
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
        let h = DojoHello { save: DojoSave::default(), pick: 0, setup: Some(DojoSetup::default()) };
        let j = serde_json::to_value(&h).unwrap();
        assert_eq!(serde_json::from_value::<DojoHello>(j).unwrap(), h);
    }
}
