//! Link play: the **Dojo** (title -> Multiplayer -> Dojo; `re/buu/menu/NOTES_screens.md`, engine/docs/netcode.md
//! "Link play: Dojo"): two players pick one of their saves and a character from its party, the host sets "Character
//! Mode:" (Balanced / In-Game) and "Rounds:" (1 / 3 / 5), and they fight a rollback versus match in the Dojo
//! (area 0-90) in rounds; the winner "Grab[s] your prize!" into their own local save, the loser "Go back to
//! training!".
//!
//! Pieces (all deterministic plain data; the network is the lobby's `dojo` room kind + the rollback session):
//! * [`DojoSave`]: what a peer shows of a local save (its party: fighter, form, level, stats, equipment), built from a
//!   [`SaveData`] - the game's save pick ("Select Saved Game", "No Saved Games").
//! * [`DojoSetup`] / [`DojoHello`]: the host's mode + rounds (in its hello / the `start`'s extra) and each peer's save
//!   summary + pick + the setup it saw (the confirm step: the host starts only when the guest echoes its setup).
//! * [`fighter_setup`]: a pick in a mode -> the battle `FighterSetup`.
//! * [`DojoGame`]: the rounds as one rollback [`Game`]: a PvP battle `Session` per round in the "dojo" arena, the
//!   round's end (a KO - the PvP rule's last fighter standing - or the round timer), the pause between rounds, the
//!   score, the match result.
//! * [`DojoMatch`]: a peer's end of the match: the result once its frame is confirmed by every input, the prize
//!   written to the local save exactly once, "Disconnected" (no result, no prize) if the link drops first.
//!
//! What comes from the ROM and what is inferred (no link cable in the emulator; static reading only):
//! * MEASURED (strings 79..139 and the Dojo page's mode table 0x0804BC04, enter 0x0801ED34): the flow and its texts
//!   - Select Saved Game (94), Dojo (95), Start Fight (96), Character Mode: Balanced / In-Game (97..99), Rounds: 1 / 3
//!   / 5 (100..103), "Choose your character!" (105), Round 1..5 (106..110), You Win!! / You Lose!! / Draw!!
//!   (111..113), Grab your prize! / Go back to training! (114 / 115), Disconnected (93); the Dojo map is area 0-90
//!   (unnamed in the area table, the walled room the arena uses).
//! * INFERRED (constants of this module, each a `DojoRules` field): the round timer (99 s), the pause between rounds,
//!   best-of-N (a player with a majority of N wins at once; a drawn round scores for no one; equal wins after N rounds
//!   = Draw!!), a timed-out round's winner (the higher HP fraction), Balanced = every fighter at level 100 with
//!   average growth and points spent evenly, In-Game = the save's level + its stat points (approximated: the stats
//!   above the level's average growth), the character list (each party member in its current form), the prize (one
//!   supply, default Senzu Bean 21), and that the host's mode / rounds apply to both.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_core::battle::{BattleConfig, FighterSetup, Outcome, RngInit};
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
            party.push(DojoChar {
                name: f.name.clone(),
                character: ci as i32,
                fighter: f.id.clone(),
                form,
                level: c.rec.level,
                stats: [c.rec.str_, c.rec.pow, c.rec.end, c.rec.spd],
                equip: c.equip,
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

/// INFERRED rule constants (module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DojoRules {
    /// frames per round before the timer decides
    pub round_frames: u32,
    /// frames between rounds ("Round N" shown)
    pub between_frames: u32,
    /// Balanced Mode's level
    pub balanced_level: i32,
    /// the prize: one of this supply
    pub prize_supply: usize,
}

impl Default for DojoRules {
    fn default() -> Self {
        DojoRules { round_frames: 99 * 60, between_frames: 180, balanced_level: 100, prize_supply: 21 }
    }
}

/// A pick in a mode -> the fighter the battle builds.
pub fn fighter_setup(c: &DojoChar, mode: DojoMode, rules: &DojoRules, d: &GameData) -> FighterSetup {
    match mode {
        DojoMode::Balanced => FighterSetup {
            fighter: c.fighter.clone(),
            form: c.form,
            level: rules.balanced_level,
            growth: "average".into(),
            auto_spend: "even".into(),
            ..Default::default()
        },
        DojoMode::InGame => {
            // the save's stats above what the level's average growth gives = its spent points (approximation)
            let base = d.chars.iter().find(|x| x.index == c.character).map(|ch| record_at_level(ch, c.level, &d.xp_table, GrowthMode::Average, 0));
            let bonus = match base {
                Some(b) => [c.stats[0] - b.str_, c.stats[1] - b.pow, c.stats[2] - b.end, c.stats[3] - b.spd].map(|v| v.max(0)),
                None => [0; 4],
            };
            FighterSetup {
                fighter: c.fighter.clone(),
                form: c.form,
                level: c.level,
                growth: "average".into(),
                auto_spend: "none".into(),
                bonus,
                equip: c.equip,
                ..Default::default()
            }
        }
    }
}

/// The match both peers build: PvP in the Dojo, no enemies, no XP.
pub fn match_config(p1: FighterSetup, p2: FighterSetup, seed: u32) -> MatchConfig {
    MatchConfig::pvp(
        BattleConfig { arena: ARENA.into(), players: vec![p1, p2], phases: Vec::new(), rng: RngInit::Seed(seed), xp: false },
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
        let s = Self::round_session(&data, &cfg, 0);
        DojoGame {
            data,
            cfg,
            setup,
            rules,
            st: DojoState { round: 0, phase: Phase::Between { left: rules.between_frames }, t: 0, wins: [0; 2], results: Vec::new(), over_at: None, frame: 0 },
            s,
        }
    }
    /// each round: the same match with its own seed (round k = seed + k)
    fn round_session(data: &Arc<GameData>, cfg: &MatchConfig, round: u8) -> Session {
        let mut c = cfg.clone();
        if let RngInit::Seed(x) = c.battle.rng {
            c.battle.rng = RngInit::Seed(x.wrapping_add(round as u32));
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
    fn hp_frac(&self, slot: usize) -> (i64, i64) {
        let w = &self.s.world;
        let Some(i) = w.players.get(slot).and_then(|&id| w.find(id)) else { return (0, 1) };
        let p = w.pl_ref(i);
        if p.dead {
            return (0, 1);
        }
        (p.rec.hp.max(0) as i64, p.rec.max_hp.max(1) as i64)
    }
    /// the round's end, if it ended this frame
    fn round_end(&self) -> Option<RoundResult> {
        let w = &self.s.world;
        let dead = |slot: usize| w.players.get(slot).and_then(|&id| w.find(id)).is_none_or(|i| w.pl_ref(i).dead);
        match wf_core::battle::outcome(w) {
            // all down
            Outcome::Lost => return Some(RoundResult::Draw),
            // the PvP rule's last fighter standing (200 frames after the KO)
            Outcome::Won => {
                return Some(match (dead(0), dead(1)) {
                    (false, true) => RoundResult::Win(0),
                    (true, false) => RoundResult::Win(1),
                    _ => RoundResult::Draw,
                })
            }
            Outcome::Running => {}
        }
        if self.st.t >= self.rules.round_frames {
            let (a, am) = self.hp_frac(0);
            let (b, bm) = self.hp_frac(1);
            let (l, r) = (a * bm, b * am);
            return Some(if l > r {
                RoundResult::Win(0)
            } else if r > l {
                RoundResult::Win(1)
            } else {
                RoundResult::Draw
            });
        }
        None
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
                    if self.st.wins.iter().any(|&w| w >= need) || self.st.results.len() >= self.setup.rounds as usize {
                        self.st.phase = Phase::Over;
                        self.st.over_at = Some(self.st.frame);
                    } else {
                        self.st.round += 1;
                        self.s = Self::round_session(&self.data, &self.cfg, self.st.round);
                        self.st.phase = Phase::Between { left: self.rules.between_frames };
                    }
                }
            }
        }
    }
    fn checksum(&self, s: &Self::State) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.0.hash(&mut h);
        chain_mix(h.finish(), s.1.world.state_hash())
    }
}

/// A peer's end of a match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ending {
    /// "You Win!!" + "Grab your prize!" (the prize was written to our save)
    Won { prize_supply: usize },
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
                let sup = &mut save.story.supplies;
                if sup.len() <= rules.prize_supply {
                    sup.resize(rules.prize_supply + 1, 0);
                }
                sup[rules.prize_supply] = sup[rules.prize_supply].saturating_add(1);
                Ending::Won { prize_supply: rules.prize_supply }
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
