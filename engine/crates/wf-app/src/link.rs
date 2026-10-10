//! Link play in the app: the title's **Multiplayer -> Dojo / Trading Post** over online play
//! (`re/buu/menu/NOTES_screens.md` "Multiplayer (link cable)", engine/docs/netcode.md "Link play: playing it",
//! engine/docs/menus.md "Link play screens").
//!
//! [`LinkSim`] wraps the title's [`StorySim`]: everything is the game's own title until the player picks Dojo or
//! Trading Post on the Multiplayer page. The game then opens its link box "Waiting for a friend..." (title.rs, vt
//! `0x0804AEBC`); here that box **is** the online lobby: the room of that kind is joined / hosted (the
//! [`LinkConfig::connect`] callback: the native shell's WebRTC room, the tests' in-process network) while the title
//! keeps running under it, so its own B Cancel leaves. Once a friend is in the room the link screens take over:
//! * both: "Select Saved Game" (94) - the local save slots ("No Saved Games", 91, when none can be used);
//! * Dojo: "Choose your character!" (105) from the save's party; the host sets "Character Mode:" (99) Balanced /
//!   In-Game (97 / 98) and "Rounds:" (100) 1 / 3 / 5, the guest sees them and confirms with "Start Fight" (96); the
//!   match is `wf_net::dojo::DojoGame` in a rollback `P2PSession` on the same connection ("Round N" 106.. banners,
//!   "You Win!!" / "You Lose!!" / "Draw!!" 111..113), then "Grab your prize!" / "Go back to training!" (114 / 115);
//!   the prize goes into the winner's local save once, from the **confirmed** inputs only;
//! * Trading Post: "Your Trade:" / "Friend's Trade:" (80 / 81), the save's supplies, unworn equipment and "Zenie"
//!   (82), "How many to trade?" (83), "Propose Trade" / "Accept" / "Decline" (84..86), "You have accepted trade"
//!   (87), "TRADE" "COMPLETE!" (88 / 89) - `wf_net::trade::TradePeer` with the two-phase commit on the slot file
//!   (`wf_app::trade::FileStore`), the peers named by stable link ids (a reconnection resumes a pending trade);
//! * a drop at any point after the friend came: "Disconnected" (93).
//!
//! When the flow ends the title's link box is closed with its own B and the Multiplayer page is back.
//! The screens' layouts are INFERRED (the link pages could not be recorded: no cable in the emulator) - the game's
//! strings, fonts, panel and cursor art, the title picture behind them.
//!
//! Link play is not part of the recordable simulation: while a flow runs the sim reports no state hash / snapshot
//! (replays / rewind skip it); the Dojo match itself is checked by its rollback session's per-frame hashes.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_core::content::GameData;
use wf_core::input::{A, B, DOWN, LEFT, RIGHT, UP};
use wf_core::menu::save::{SaveData, SLOTS};
use wf_core::menu::text::{units, Canvas, Pen, TextData};
use wf_core::menu::link::trade::{TradeEntry, KIND_EQUIP, KIND_SUPPLY, KIND_ZENIE};
use wf_core::menu::link::LinkAction;
use wf_core::menu::MenuData;
use wf_net::dojo::{balanced_roster, fighter_setup, DojoChar, DojoGame, DojoHello, DojoMatch, DojoMode, DojoRules, DojoSave, DojoSetup, Ending, Phase as DPhase};
use wf_net::game::Game;
use wf_net::lobby::{Lobby, LobbySettings, Role, KIND_DOJO, KIND_TRADE};
use wf_net::netsim::{content_id, Clock};
use wf_net::proto::{decode, Msg};
use wf_net::session::{NetConfig, NetEvent, P2PSession};
use wf_net::trade::{Offer, TradeEvent, TradePeer, TradeStore};
use wf_net::transport::{PeerChange, Transport};
use wf_view::{AudioEvent, Buttons, MenuFont, MenuLayer, MenuOp, MenuView, RenderView, Sim, SimEvent};

use crate::story::StorySim;
use crate::trade::FileStore;

/// which link page
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkKind {
    Dojo,
    Trade,
}

/// opens the connection to the room of a kind: (transport, our role in the room)
pub type Connect = Box<dyn FnMut(LinkKind) -> Result<(Box<dyn Transport>, Role), String>>;

pub struct LinkConfig {
    pub connect: Connect,
    /// milliseconds (the lobby / sessions' clock)
    pub clock: Clock,
    /// this player's stable id (persisted by the shell): names the trade peers across reconnections
    pub link_id: String,
    pub name: String,
    /// the native shell's `<data>/saves` (slotN.sav): the Trading Post commits there; the Dojo prize is written there
    /// too (and into the running title's slots, which the shell persists); None = no Trading Post
    pub saves_dir: Option<PathBuf>,
    pub rules: DojoRules,
    pub seed: u32,
    /// a scripted player in place of the shell's buttons (tests, `--link-bot`)
    pub bot: Option<LinkBot>,
    /// after each flow, a JSON report here (`--link-report`): the end ("done" / "complete" / the ending / the
    /// message) and the Dojo match as confirmed (frame, checksum, inputs' hash, ending)
    pub report: Option<PathBuf>,
}

/// what a peer says in its hellos
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct LinkHello {
    pub link_id: String,
    #[serde(default)]
    pub dojo: Option<DojoHello>,
}

const SFX_CURSOR: u8 = 39;
const SFX_MENU: u8 = 40;
const SFX_ACCEPT: u8 = 41;

/// the connection: the transport, the room's lobby, outgoing packets
struct Net {
    t: Box<dyn Transport>,
    lobby: Lobby,
    out: Vec<(String, Vec<u8>)>,
}

impl Net {
    /// one pump: connection changes, the lobby's packets (handled), the rest for the match / the trade
    fn pump(&mut self, now: f64) -> (Vec<PeerChange>, Vec<(String, Vec<u8>)>) {
        let changes = self.t.changes();
        for c in &changes {
            match c {
                PeerChange::Connected(id) => self.lobby.connected(id),
                PeerChange::Disconnected(id) => self.lobby.disconnected(id),
            }
        }
        let Some(me) = self.t.local_id() else { return (changes, Vec::new()) };
        let mut rest = Vec::new();
        for (from, b) in self.t.recv(now) {
            match decode(&b) {
                Some(Msg::Lobby(j)) => {
                    let _ = self.lobby.handle(&me, &from, &j, &mut self.out);
                }
                _ => rest.push((from, b)),
            }
        }
        self.lobby.poll(&me, now, &mut self.out);
        self.flush(now);
        (changes, rest)
    }
    fn flush(&mut self, now: f64) {
        for (to, b) in self.out.drain(..) {
            self.t.send(&to, b, now);
        }
    }
    /// the friend: a connected peer that said hello (its transport id, its hello)
    fn friend(&self) -> Option<(String, LinkHello)> {
        self.lobby.peer_extras().into_iter().find_map(|(id, v)| serde_json::from_value::<LinkHello>(v).ok().map(|h| (id, h)))
    }
    fn set_hello(&mut self, h: &LinkHello) {
        self.lobby.set_extra(serde_json::to_value(h).unwrap_or_default());
    }
}

/// the trade's view of the connection: the friend under its stable link id
#[derive(Default)]
struct Port {
    me: String,
    inbox: Vec<(String, Vec<u8>)>,
    changes: Vec<PeerChange>,
    outbox: Vec<(String, Vec<u8>)>,
}

impl Transport for Port {
    fn local_id(&self) -> Option<String> {
        Some(self.me.clone())
    }
    fn send(&mut self, to: &str, data: Vec<u8>, _now: f64) {
        self.outbox.push((to.to_string(), data));
    }
    fn recv(&mut self, _now: f64) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.inbox)
    }
    fn changes(&mut self) -> Vec<PeerChange> {
        std::mem::take(&mut self.changes)
    }
}

/// one row of "Your Trade:"

struct TradeUi {
    peer: TradePeer,
    store: FileStore,
    port: Port,
    friend_tid: String,
    friend_lid: String,
    accepted: bool,
    friend_accepted: bool,
    /// a status line (the game's strings where it has one)
    note: Vec<u16>,
    /// "Disconnected" with a trade in doubt
    away: Option<bool>,
    /// the committed trade's id (TRADE COMPLETE) and the frames its page has shown
    done: Option<(String, u32)>,
    /// the friend's proposal revision given to the page (`Friend::offer`) and its answer not yet given
    friend_given: Option<u32>,
    friend_answer: Option<bool>,
}

struct Match {
    sess: P2PSession<DojoGame>,
    /// the game advanced with confirmed inputs only (the result / prize source)
    conf: DojoGame,
    /// the confirmed game's checksum after each confirmed frame, and the match's definition (config, setup,
    /// rules) - the record's cross-build replay data (`dojo_replay`: native == wasm per frame)
    hashes: Vec<u64>,
    def: serde_json::Value,
    m: DojoMatch,
    /// the last frame whose sounds were played
    audio_at: i32,
}

enum Phase {
    /// the title's own "Waiting for a friend..." box over the lobby
    Waiting,
    Saves { sel: usize },
    NoSaves,
    /// "Choose your character!" (the save's party, In-Game; the Balanced roster); `confirmed`: picked, waiting for
    /// the friend
    Chars { sel: usize, confirmed: bool },
    /// the Dojo page: rows 0 mode, 1 rounds, 2 Start Fight (the host sets it, the guest sees it)
    Setup { row: usize },
    Match(Box<Match>),
    Result(Ending),
    Trade(Box<TradeUi>),
    /// a message (string) ended the flow; any key goes back
    Message(Vec<u16>),
}

/// what a flow asks the host sim to do
enum Out {
    None,
    /// the flow is over: close the title's link box
    Done,
    /// a save slot changed (prize / trade): the new save
    Slot(usize, SaveData),
}

struct Flow {
    kind: LinkKind,
    data: Arc<GameData>,
    net: Net,
    role: Role,
    phase: Phase,
    /// frames in the phase (cursor blink)
    t: u32,
    saves: Vec<Option<SaveData>>,
    slot: usize,
    dsave: Option<DojoSave>,
    pick: usize,
    setup: DojoSetup,
    friend: Option<(String, LinkHello)>,
    link_id: String,
    rules: DojoRules,
    saves_dir: Option<PathBuf>,
    record: Option<MatchRecord>,
}

/// a finished Dojo match as this peer confirmed it (the tests compare both peers')
#[derive(Clone, Debug, PartialEq)]
pub struct MatchRecord {
    /// the confirmed game's last frame (the first frame of the match's end, or the drop)
    pub frame: i32,
    /// its checksum there
    pub checksum: u64,
    /// the confirmed inputs per slot up to there
    pub inputs: Vec<Vec<u16>>,
    pub ending: Ending,
    /// the confirmed game's checksum after each of those frames (index f = after frame f's inputs)
    pub hashes: Vec<u64>,
    /// the match's definition: {"config", "setup", "rules"} (`dojo_replay` rebuilds the game from it)
    pub def: serde_json::Value,
}

fn sfx(ev: &mut Vec<SimEvent>, id: u8) {
    ev.push(SimEvent::Audio(AudioEvent::PlaySfx { id, volume: 64, pan: 0 }));
}

fn story_flags(d: &GameData) -> Vec<bool> {
    d.menu.as_ref().map(|m| m.inv.story.clone()).unwrap_or_default()
}

fn slot_file(dir: &std::path::Path, i: usize) -> PathBuf {
    dir.join(format!("slot{}.sav", i + 1))
}

impl Flow {
    fn new(kind: LinkKind, data: Arc<GameData>, cfg: &mut LinkConfig, saves: Vec<Option<SaveData>>) -> Result<Flow, String> {
        let (t, role) = (cfg.connect)(kind)?;
        let host = role == Role::Host;
        let hello = LinkHello { link_id: cfg.link_id.clone(), dojo: None };
        let settings = LobbySettings {
            role,
            name: cfg.name.clone(),
            content: content_id(&data),
            want_players: 2,
            auto_start: false,
            // (the guest's Start Fight sets it)
            ready: host,
            secret: format!("{}-{}", cfg.link_id, cfg.seed),
            kind: if kind == LinkKind::Dojo { KIND_DOJO.into() } else { KIND_TRADE.into() },
            extra: serde_json::to_value(&hello).unwrap_or_default(),
            base: wf_net::dojo::base_config(cfg.seed),
            ..Default::default()
        };
        let nonce = (cfg.clock)() as u32 ^ cfg.seed.wrapping_mul(0x9E37_79B9);
        Ok(Flow {
            kind,
            data,
            net: Net { t, lobby: Lobby::new(settings, nonce), out: Vec::new() },
            role,
            phase: Phase::Waiting,
            t: 0,
            saves,
            slot: 0,
            dsave: None,
            pick: 0,
            setup: DojoSetup::default(),
            friend: None,
            link_id: cfg.link_id.clone(),
            rules: cfg.rules,
            saves_dir: cfg.saves_dir.clone(),
            record: None,
        })
    }

    fn set_phase(&mut self, p: Phase) {
        self.phase = p;
        self.t = 0;
    }

    fn host(&self) -> bool {
        self.role == Role::Host
    }

    /// the slots this kind can use
    fn usable(&self, i: usize) -> bool {
        let Some(sd) = self.saves.get(i).and_then(|s| s.as_ref()) else { return false };
        match self.kind {
            LinkKind::Dojo => DojoSave::from_save(i, sd, &self.data).is_some(),
            LinkKind::Trade => self.saves_dir.is_some(),
        }
    }

    fn friend_gone(&self) -> bool {
        self.friend.as_ref().is_some_and(|(id, _)| !self.net.lobby.peer_connected(id))
    }

    /// what the local player did on a wf-core link page (`menu::link`)
    fn link_action(&mut self, a: LinkAction, now: f64, ev: &mut Vec<SimEvent>) -> Out {
        if let Phase::Trade(u) = &mut self.phase {
            match a {
                LinkAction::Propose(e) => {
                    u.accepted = false;
                    u.peer.propose(offer_from(&e));
                }
                LinkAction::CancelPropose | LinkAction::LeaveProposal => {
                    u.accepted = false;
                    u.peer.withdraw();
                }
                LinkAction::Answer(true) => match u.peer.accept(&u.store) {
                    Ok(()) => u.accepted = true,
                    Err(e) => {
                        // (our side cannot take it - a limit: as a decline)
                        u.note = units(&e);
                        u.peer.decline();
                    }
                },
                LinkAction::Answer(false) => {
                    u.accepted = false;
                    u.peer.decline();
                }
                LinkAction::CancelAnswer => {
                    u.accepted = false;
                    u.peer.unaccept();
                }
                // B on the picker: back to Select Saved Game (the page shows it), the trade's connection dropped
                LinkAction::Unpick => {
                    let sel = self.slot;
                    self.set_phase(Phase::Saves { sel });
                }
                // (TRADE COMPLETE! on the page: the commit runs on its own - `TradeEvent::Complete`)
                _ => {}
            }
            return Out::None;
        }
        match (&self.phase, a) {
            (Phase::Saves { .. }, LinkAction::Back) => Out::Done,
            (Phase::Saves { .. }, LinkAction::Save(s)) if self.usable(s) => {
                self.slot = s;
                match self.kind {
                    LinkKind::Dojo => {
                        self.dsave = self.saves[s].as_ref().and_then(|sd| DojoSave::from_save(s, sd, &self.data));
                        self.set_phase(Phase::Setup { row: if self.host() { 0 } else { 2 } });
                        Out::None
                    }
                    LinkKind::Trade => self.open_trade(now, ev),
                }
            }
            _ => Out::None,
        }
    }

    /// the wf-core link page this phase shows (None: drawn by the flow itself)
    fn page(&self) -> Option<&'static str> {
        match self.phase {
            Phase::Saves { .. } => Some("saves"),
            Phase::Trade(_) => Some("trade"),
            _ => None,
        }
    }

    /// one frame. `pressed`: new presses; `held`: the buttons held (the match's input)
    fn step(&mut self, now: f64, clock: &Clock, held: u16, pressed: u16, ev: &mut Vec<SimEvent>) -> Out {
        self.t = self.t.wrapping_add(1);
        if let Some(r) = self.net.lobby.rejected.clone() {
            if !matches!(self.phase, Phase::Message(_) | Phase::Match(_) | Phase::Result(_) | Phase::Trade(_)) {
                self.set_phase(Phase::Message(units(&r)));
            }
        }
        // the match and the trade pump the connection themselves (their packets)
        if matches!(self.phase, Phase::Match(_)) {
            return self.step_match(now, clock, held, ev);
        }
        if matches!(self.phase, Phase::Trade(_)) {
            return self.step_trade(now, pressed, ev);
        }
        let _ = self.net.pump(now);
        if self.friend.is_none() {
            self.friend = self.net.friend();
        } else if let Some(f) = self.net.friend() {
            // (its hello changes as it picks: the latest)
            self.friend = Some(f);
        }
        let lost = self.friend_gone() || self.net.t.closed();
        if lost && !matches!(self.phase, Phase::Waiting | Phase::Message(_) | Phase::Result(_)) {
            self.set_phase(Phase::Message(Vec::new()));
            return Out::None;
        }
        // the guest shows the host's setup; a change sends it back to the setup page (its pick void)
        if !self.host() && matches!(self.phase, Phase::Setup { .. } | Phase::Chars { .. }) {
            if let Some(s) = self.host_setup() {
                if s != self.setup {
                    self.setup = s;
                    if matches!(self.phase, Phase::Chars { .. }) {
                        self.net.lobby.set_ready(false);
                        self.send_dojo_hello(None);
                        self.set_phase(Phase::Setup { row: 2 });
                    }
                }
            }
        }
        match &mut self.phase {
            // (the title's box: once the friend is there it counts its 10 updates and ends - `linked`)
            Phase::Waiting => Out::None,
            Phase::NoSaves | Phase::Message(_) | Phase::Result(_) => {
                if pressed & (A | B) != 0 {
                    sfx(ev, SFX_MENU);
                    return Out::Done;
                }
                Out::None
            }
            // (the page itself is wf-core's `menu::link` screen on the title app: `LinkSim` steps it, its actions
            // come in through `link_action`)
            Phase::Saves { .. } => Out::None,
            Phase::Setup { row } => {
                let host = self.role == Role::Host;
                let mut r = *row;
                if host && pressed & (UP | DOWN) != 0 {
                    r = if pressed & UP != 0 { (r + 2) % 3 } else { (r + 1) % 3 };
                    sfx(ev, SFX_CURSOR);
                }
                if host && pressed & (LEFT | RIGHT) != 0 && r < 2 {
                    let right = pressed & RIGHT != 0;
                    if r == 0 {
                        self.setup.mode = if self.setup.mode == DojoMode::Balanced { DojoMode::InGame } else { DojoMode::Balanced };
                    } else {
                        let order = [1u8, 3, 5];
                        let k = order.iter().position(|&x| x == self.setup.rounds).unwrap_or(1);
                        self.setup.rounds = order[if right { (k + 1) % 3 } else { (k + 2) % 3 }];
                    }
                    sfx(ev, SFX_CURSOR);
                    self.send_dojo_hello(Some(self.setup));
                }
                if let Phase::Setup { row } = &mut self.phase {
                    *row = r;
                }
                if pressed & B != 0 {
                    sfx(ev, SFX_MENU);
                    let sel = self.slot;
                    self.set_phase(Phase::Saves { sel });
                    return Out::None;
                }
                // Start Fight: on to the character (the guest once the host's setup shows)
                if pressed & A != 0 && r == 2 && (host || self.host_setup().is_some()) {
                    sfx(ev, SFX_MENU);
                    if host {
                        self.send_dojo_hello(Some(self.setup));
                    }
                    self.set_phase(Phase::Chars { sel: 0, confirmed: false });
                }
                Out::None
            }
            Phase::Chars { sel, confirmed } => {
                let host = self.role == Role::Host;
                let (mut k, mut c) = (*sel, *confirmed);
                let list = self.char_list();
                let n = list.len().max(1);
                if !c {
                    if pressed & UP != 0 {
                        k = (k + n - 1) % n;
                        sfx(ev, SFX_CURSOR);
                    }
                    if pressed & DOWN != 0 {
                        k = (k + 1) % n;
                        sfx(ev, SFX_CURSOR);
                    }
                    if pressed & B != 0 {
                        sfx(ev, SFX_MENU);
                        self.set_phase(Phase::Setup { row: 2 });
                        return Out::None;
                    }
                    if pressed & A != 0 && !list.is_empty() {
                        sfx(ev, SFX_ACCEPT);
                        c = true;
                        self.pick = k;
                        self.net.lobby.set_fighter(fighter_setup(&list[k], self.setup.mode, &self.rules, &self.data));
                        if host {
                            self.net.lobby.set_start_extra(serde_json::to_value(self.setup).unwrap_or_default());
                        } else {
                            self.net.lobby.set_ready(true);
                        }
                        // (the guest's hello with the setup = its confirm of that setup)
                        self.send_dojo_hello(Some(self.setup));
                    }
                } else if pressed & B != 0 {
                    // withdraw the pick
                    sfx(ev, SFX_MENU);
                    c = false;
                    if !host {
                        self.net.lobby.set_ready(false);
                        self.send_dojo_hello(None);
                    }
                }
                if let Phase::Chars { sel, confirmed } = &mut self.phase {
                    *sel = k;
                    *confirmed = c;
                }
                // the host starts once the guest confirmed this very setup
                if host && c && self.friend.as_ref().and_then(|(_, h)| h.dojo.as_ref()).is_some_and(|d| d.setup == Some(self.setup)) {
                    self.net.lobby.request_start();
                }
                if self.net.lobby.start.is_some() {
                    return self.start_match(now, ev);
                }
                Out::None
            }
            Phase::Match(_) | Phase::Trade(_) => Out::None,
        }
    }

    /// the host's setup as its hello shows it (the guest's view)
    fn host_setup(&self) -> Option<DojoSetup> {
        self.friend.as_ref().and_then(|(_, h)| h.dojo.as_ref()).and_then(|d| d.setup)
    }

    /// the characters to choose from: In-Game the save's party, Balanced the game's fixed roster (strings 116..139)
    fn char_list(&self) -> Vec<DojoChar> {
        match self.setup.mode {
            DojoMode::Balanced => balanced_roster(),
            DojoMode::InGame => self.dsave.as_ref().map(|s| s.party.clone()).unwrap_or_default(),
        }
    }

    fn send_dojo_hello(&mut self, setup: Option<DojoSetup>) {
        let d = DojoHello { save: self.dsave.clone().unwrap_or_default(), pick: self.pick, setup };
        let h = LinkHello { link_id: self.link_id.clone(), dojo: Some(d) };
        self.net.set_hello(&h);
    }

    fn start_match(&mut self, now: f64, _ev: &mut Vec<SimEvent>) -> Out {
        let Some(st) = self.net.lobby.start.clone() else { return Out::None };
        let me = self.net.t.local_id().unwrap_or_default();
        let Some(slot) = st.players.iter().position(|s| s.id == me) else {
            self.set_phase(Phase::Message(units("not seated in the match")));
            return Out::None;
        };
        let setup: DojoSetup = serde_json::from_value(st.extra.clone()).unwrap_or(self.setup);
        let game = DojoGame::new(self.data.clone(), st.config.clone(), setup, self.rules);
        let conf = DojoGame::new(self.data.clone(), st.config.clone(), setup, self.rules);
        let ncfg = NetConfig { slots: st.players.len(), input_delay: st.input_delay, max_prediction: st.max_prediction, session: st.session, ..Default::default() };
        let peers = st.players.iter().enumerate().filter(|(_, s)| s.id != me).map(|(k, s)| (s.id.clone(), vec![k])).collect();
        let sess = P2PSession::new(game, ncfg, vec![slot], peers, Vec::new(), st.host == me, now);
        self.setup = setup;
        let def = serde_json::json!({ "config": st.config, "setup": setup, "rules": self.rules });
        self.set_phase(Phase::Match(Box::new(Match { sess, conf, hashes: Vec::new(), def, m: DojoMatch::new(slot as u8), audio_at: 0 })));
        Out::None
    }

    fn step_match(&mut self, now: f64, clock: &Clock, held: u16, ev: &mut Vec<SimEvent>) -> Out {
        let Phase::Match(m) = &mut self.phase else { return Out::None };
        let changes = self.net.t.changes();
        let mut dropped = self.net.t.closed();
        for c in &changes {
            if let PeerChange::Disconnected(id) = c {
                m.sess.peer_left(id);
                self.net.lobby.disconnected(id);
            }
        }
        for (from, b) in self.net.t.recv(now) {
            let Some(msg) = decode(&b) else { continue };
            if let Msg::Lobby(j) = &msg {
                let me = self.net.t.local_id().unwrap_or_default();
                let _ = self.net.lobby.handle(&me, &from, j, &mut self.net.out);
                continue;
            }
            m.sess.count_recv(b.len());
            m.sess.handle(&from, &msg, now);
        }
        let mut out = Vec::new();
        let before = m.sess.frame();
        m.sess.tick(now, &**clock, &[held], &mut out);
        for (to, b) in out {
            self.net.t.send(&to, b, now);
        }
        self.net.flush(now);
        // sounds of newly simulated frames (predicted; a rolled-back frame's sound is not repeated)
        if m.sess.frame() > before && m.sess.frame() > m.audio_at {
            m.audio_at = m.sess.frame();
            wf_core::view::sim_events(&m.sess.game.s.world, ev);
        }
        if m.sess.events.iter().any(|e| matches!(e, NetEvent::Disconnected { .. })) {
            dropped = true;
        }
        // the confirmed game: every input known
        let c = m.sess.confirmed().min(m.sess.frame());
        // (stopping at the end: both peers' confirmed games end on the same frame)
        while m.conf.st.frame < c && m.conf.st.phase != DPhase::Over {
            let f = m.conf.st.frame as usize;
            let inp: Vec<u16> = (0..m.sess.cfg.slots).map(|s| m.sess.known_inputs().get(s).and_then(|v| v.get(f)).copied().unwrap_or(0)).collect();
            m.conf.advance(&inp, false);
            let h = m.conf.checksum(&m.conf.save());
            m.hashes.push(h);
        }
        let Some(mut save) = self.saves.get(self.slot).cloned().flatten() else { return Out::None };
        let before_save = save.clone();
        let ended = m.m.settle(&m.conf, &self.rules, &mut save).cloned();
        let e = match ended {
            Some(e) => Some(e),
            None if dropped => Some(m.m.disconnected().clone()),
            None => None,
        };
        let Some(e) = e else { return Out::None };
        self.record = Some(MatchRecord {
            frame: m.conf.st.frame,
            checksum: m.conf.checksum(&m.conf.save()),
            inputs: m.sess.known_inputs().iter().map(|v| v[..(m.conf.st.frame as usize).min(v.len())].to_vec()).collect(),
            ending: e.clone(),
            hashes: m.hashes.clone(),
            def: m.def.clone(),
        });
        self.set_phase(Phase::Result(e));
        if save != before_save {
            self.saves[self.slot] = Some(save.clone());
            if let Some(dir) = &self.saves_dir {
                let _ = crate::trade::disk_write(&slot_file(dir, self.slot), &crate::trade::slot_bytes(&save));
            }
            return Out::Slot(self.slot, save);
        }
        Out::None
    }

    fn open_trade(&mut self, _now: f64, _ev: &mut Vec<SimEvent>) -> Out {
        let Some(dir) = self.saves_dir.clone() else {
            self.set_phase(Phase::Message(units("no save folder")));
            return Out::None;
        };
        let Some((tid, h)) = self.friend.clone() else { return Out::None };
        let path = slot_file(&dir, self.slot);
        // the slot file is the running title's slot (the shell keeps them equal): write it if it is missing
        if crate::trade::disk_read(&path).is_none() {
            if let Some(sd) = self.saves[self.slot].as_ref() {
                let _ = crate::trade::disk_write(&path, &crate::trade::slot_bytes(sd));
            }
        }
        let store = match FileStore::open(&path, story_flags(&self.data)) {
            Ok(s) => s,
            Err(e) => {
                self.set_phase(Phase::Message(units(&e)));
                return Out::None;
            }
        };
        let Some(sd) = store.save().or_else(|| self.saves[self.slot].clone()) else { return Out::None };
        let nonce = (self.t as u64) ^ ((self.slot as u64) << 32) ^ (self.net.lobby.settings.secret.len() as u64);
        let peer = TradePeer::new(&self.link_id, &h.link_id, nonce ^ u64::from(self.net.t.local_id().unwrap_or_default().len() as u32), &store);
        let _ = sd;
        self.set_phase(Phase::Trade(Box::new(TradeUi {
            peer,
            store,
            port: Port { me: self.link_id.clone(), ..Default::default() },
            friend_tid: tid,
            friend_lid: h.link_id,
            accepted: false,
            friend_accepted: false,
            note: Vec::new(),
            away: None,
            done: None,
            friend_given: None,
            friend_answer: None,
        })));
        Out::None
    }

    /// the picked save as the trade's store has it
    fn trade_save(&self) -> Option<SaveData> {
        match &self.phase {
            Phase::Trade(u) => u.store.save(),
            _ => None,
        }
    }

    /// what the page's friend should know now (taken once): the friend's proposal (its offer as the page's entries)
    /// and its answer
    fn trade_friend(&mut self) -> (Option<Vec<TradeEntry>>, Option<bool>) {
        let Phase::Trade(u) = &mut self.phase else { return (None, None) };
        let mut offer = None;
        if u.peer.friend_proposed() && u.peer.friend_rev() != u.friend_given {
            u.friend_given = u.peer.friend_rev();
            offer = u.peer.friend_offer().map(entries_of);
        }
        (offer, u.friend_answer.take())
    }

    /// TRADE COMPLETE! and the trade settled on both sides (or the friend gone, or 10 s): the flow can end
    fn trade_settled(&self) -> bool {
        let Phase::Trade(u) = &self.phase else { return true };
        let Some((id, shown)) = u.done.as_ref() else { return false };
        u.peer.settled(id, &u.store) || u.away.is_some() || self.net.t.closed() || *shown > 600
    }

    fn step_trade(&mut self, now: f64, pressed: u16, ev: &mut Vec<SimEvent>) -> Out {
        let Phase::Trade(u) = &mut self.phase else { return Out::None };
        trade_io(&mut self.net, u, now);
        let mut done = None;
        for e in u.peer.events() {
            match e {
                // (the page takes the friend's proposal from the peer: `trade_friend`)
                TradeEvent::FriendOffer(_) => {}
                TradeEvent::FriendAccepted => {
                    u.friend_accepted = true;
                    u.friend_answer = Some(true);
                }
                TradeEvent::Declined => {
                    u.friend_accepted = false;
                    u.accepted = false;
                    u.friend_answer = Some(false);
                }
                TradeEvent::Agreed(_) => {}
                TradeEvent::Complete(id) => {
                    u.done = Some((id, 0));
                    done = Some(true);
                }
                // (our side refused the agreed trade - its check failed: as a decline)
                TradeEvent::Aborted(_, why) => {
                    u.note = units(&why);
                    u.friend_answer = Some(false);
                }
                TradeEvent::Disconnected { in_doubt } => u.away = Some(in_doubt),
                TradeEvent::Reconnected => u.away = None,
            }
        }
        if let Some((_, shown)) = u.done.as_mut() {
            *shown += 1;
        }
        if done == Some(true) {
            if let Some(sd) = u.store.save() {
                self.saves[self.slot] = Some(sd.clone());
                return Out::Slot(self.slot, sd);
            }
            return Out::None;
        }
        // "Disconnected" with nothing done: any key leaves (a trade in doubt stays locked until the friend is back;
        // the journal finishes it at the next link with this friend)
        if u.away.is_some() && u.done.is_none() && pressed & (A | B) != 0 {
            sfx(ev, SFX_MENU);
            return Out::Done;
        }
        Out::None
    }

    // ---------------------------------------------------------------------------------------------------------------
    // drawing (INFERRED layouts: the game's strings / fonts / panel / cursor; the title picture behind)

    fn view(&self, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut bg = Canvas::new();
        bg.ops.push(MenuOp::Image { path: "ui/images/screen_title.png".into(), frame: 0, fw: 240, fh: 160, x: 0, y: 0, key0: false });
        let mut v = wf_core::menu::surface_view(bg);
        v.layers[0].priority = 3;
        let blink = self.t & 8 == 0;
        let (w, h) = (224, 144);
        let mut cv = Canvas::new();
        cv.background(12, 4, w - 24, h - 16);
        cv.panel(8, 0, w - 8, h - 8);
        let head = |cv: &mut Canvas, s: &[u16]| {
            let mut p = Pen::new(MenuFont::Bold, w / 2, 20, 1, 3, Some(1));
            cv.text(t, &mut p, s);
        };
        let line = |cv: &mut Canvas, x: i32, y: i32, s: &[u16], col: u8| {
            let mut p = Pen::new(MenuFont::Menu, x, y, 0, col, Some(1));
            cv.text(t, &mut p, s);
        };
        let centre = |cv: &mut Canvas, y: i32, s: &[u16], font: MenuFont, col: u8| {
            let mut p = Pen::new(font, w / 2, y, 1, col, Some(1));
            cv.text(t, &mut p, s);
        };
        let cursor = |cv: &mut Canvas, x: i32, y: i32| {
            if blink {
                cv.image("ui/images/cursor.png", 0, 16, 16, x - 16, y - 8);
            }
        };
        match &self.phase {
            Phase::Waiting => {}
            Phase::NoSaves => {
                head(&mut cv, t.s(94));
                centre(&mut cv, 70, t.s(91), MenuFont::Menu, 2);
            }
            Phase::Message(m) => {
                head(&mut cv, t.s(if self.kind == LinkKind::Dojo { 95 } else { 79 }));
                centre(&mut cv, 64, t.s(93), MenuFont::Bold, 6);
                if !m.is_empty() {
                    centre(&mut cv, 84, m, MenuFont::Menu, 2);
                }
            }
            Phase::Saves { sel } => {
                head(&mut cv, t.s(94));
                centre(&mut cv, 40, t.s(92), MenuFont::Menu, 2);
                for i in 0..SLOTS {
                    let y = 62 + i as i32 * 18;
                    let s = match self.saves.get(i).and_then(|s| s.as_ref()) {
                        Some(sd) => units(&crate::launcher::slot_summary(&crate::trade::slot_bytes(sd), &self.data).unwrap_or_default()),
                        None => units("-"),
                    };
                    let col = if i == *sel { 6 } else if self.usable(i) { 2 } else { 0x75 };
                    line(&mut cv, 40, y, &s, col);
                    if i == *sel {
                        cursor(&mut cv, 36, y - 4);
                    }
                }
            }
            Phase::Chars { sel, confirmed } => {
                head(&mut cv, t.s(95));
                centre(&mut cv, 36, t.s(105), MenuFont::Menu, 2);
                let list = self.char_list();
                let balanced = self.setup.mode == DojoMode::Balanced;
                // (6 rows at a time)
                let first = sel.saturating_sub(5).min(list.len().saturating_sub(6));
                for (i, c) in list.iter().enumerate().skip(first).take(6) {
                    let y = 54 + (i - first) as i32 * 12;
                    let mut name = if balanced { t.s(116 + i).to_vec() } else { char_name(t, c) };
                    if !balanced {
                        name.extend(units(&format!("  Lv{}", c.level)));
                    }
                    line(&mut cv, 56, y, &name, if i == *sel { 6 } else { 2 });
                    if i == *sel && !confirmed {
                        cursor(&mut cv, 52, y - 4);
                    }
                }
                if *confirmed {
                    centre(&mut cv, 128, t.s(90), MenuFont::Menu, 2);
                }
            }
            Phase::Setup { row } => {
                head(&mut cv, t.s(95));
                centre(&mut cv, 36, t.s(104), MenuFont::Menu, 2);
                let mode = t.s(if self.setup.mode == DojoMode::Balanced { 97 } else { 98 });
                let rounds = t.s(match self.setup.rounds {
                    1 => 101,
                    5 => 103,
                    _ => 102,
                });
                let rows: [(&[u16], &[u16]); 2] = [(t.s(99), mode), (t.s(100), rounds)];
                for (i, (l, val)) in rows.iter().enumerate() {
                    let y = 60 + i as i32 * 18;
                    line(&mut cv, 36, y, l, 2);
                    line(&mut cv, 130, y, val, if *row == i { 6 } else { 2 });
                    if *row == i && self.host() {
                        cursor(&mut cv, 32, y - 4);
                    }
                }
                let y = 104;
                let ready = self.host() || self.host_setup().is_some();
                centre(&mut cv, y, t.s(96), MenuFont::Bold, if !ready { 0x75 } else if *row == 2 { 6 } else { 2 });
                if *row == 2 {
                    cursor(&mut cv, 72, y - 4);
                }
            }
            Phase::Result(e) => {
                head(&mut cv, t.s(95));
                let (a, b): (usize, Option<usize>) = match e {
                    Ending::Won { .. } => (111, Some(114)),
                    Ending::Lost => (112, Some(115)),
                    Ending::Draw => (113, None),
                    Ending::Disconnected => (93, None),
                };
                centre(&mut cv, 64, t.s(a), MenuFont::Bold, 6);
                if let Some(b) = b {
                    centre(&mut cv, 86, t.s(b), MenuFont::Menu, 2);
                }
            }
            // (the Trading Post's pages are wf-core's link screen: `LinkSim.page`)
            Phase::Trade(_) => {}
            Phase::Match(_) => {}
        }
        v.layers.push(MenuLayer { w: w as u16, h: h as u16, x: 8, y: 8, priority: 0, ops: cv.ops });
        v
    }

    /// the match's banner over the battle ("Round N" between rounds, the result once over)
    fn match_overlay(&self, d: &MenuData) -> Option<MenuView> {
        let Phase::Match(m) = &self.phase else { return None };
        let t = &d.text;
        let g = &m.sess.game;
        let s: &[u16] = match g.st.phase {
            DPhase::Between { .. } => t.s(106 + (g.st.round as usize).min(4)),
            DPhase::Over => match g.result() {
                Some(wf_net::dojo::DojoResult::Win(x)) if x as usize == m.m.slot as usize => t.s(111),
                Some(wf_net::dojo::DojoResult::Win(_)) => t.s(112),
                _ => t.s(113),
            },
            DPhase::Fight => return None,
        };
        let mut cv = Canvas::new();
        let mut p = Pen::new(MenuFont::Bold, 120, 80, 1, 6, Some(1));
        cv.text(t, &mut p, s);
        let mut v = wf_core::menu::surface_view(cv);
        v.overlay = true;
        Some(v)
    }
}

fn char_name(t: &TextData, c: &DojoChar) -> Vec<u16> {
    // the game's character list (strings 116..139) where the name is there
    let n = units(&c.name);
    (116..=139).map(|i| t.s(i)).find(|s| *s == n.as_slice()).map(|s| s.to_vec()).unwrap_or(n)
}




/// the page's offer entries as the trade's offer
fn offer_from(e: &[TradeEntry]) -> Offer {
    let mut o = Offer::default();
    for x in e {
        match x.kind {
            KIND_EQUIP => o.equipment.extend(std::iter::repeat_n(x.id as i32, x.amount.max(1) as usize)),
            KIND_SUPPLY => *o.supplies.entry(x.id as u16).or_default() += x.amount,
            _ => o.zenie += x.amount,
        }
    }
    o
}

/// the trade's offer as the page's entries (equipment one entry per copy, the supplies by index, zenie last)
fn entries_of(o: &Offer) -> Vec<TradeEntry> {
    let mut v: Vec<TradeEntry> = o.equipment.iter().map(|&id| TradeEntry { amount: 1, kind: KIND_EQUIP, id: id as u8 }).collect();
    v.extend(o.supplies.iter().filter(|(_, &n)| n > 0).map(|(&i, &n)| TradeEntry { amount: n, kind: KIND_SUPPLY, id: i as u8 }));
    if o.zenie > 0 {
        v.push(TradeEntry { amount: o.zenie, kind: KIND_ZENIE, id: 0 });
    }
    v
}


thread_local! {
    static INFO: std::cell::RefCell<Option<serde_json::Value>> = const { std::cell::RefCell::new(None) };
}

/// switch the per-step state snapshot of [`link_info`] on / off (off by default: no cost)
pub fn set_link_info(on: bool) {
    INFO.with(|i| *i.borrow_mut() = if on { Some(serde_json::Value::Null) } else { None });
}

/// the last [`LinkSim`]'s state after its last step (JSON: phase, title page / wait, setup, trade cursor / state,
/// the last flow's end, the last Dojo match's record) - the browser build's test hook; Null while switched off
/// Replays a Dojo match from a link report's `match.replay` ({def, inputs, hashes}: either build's) on THIS build:
/// the game rebuilt from the definition, advanced with the recorded confirmed inputs; returns this build's checksum
/// after each frame and the first frame whose checksum differs from the recorded one (None = identical every frame).
/// The native `examples/dojo_replay` and the browser's `wf_dojo_replay` call it: native == wasm on the same inputs.
pub fn dojo_replay(data: Arc<GameData>, replay: &serde_json::Value) -> Result<(Vec<u64>, Option<usize>), String> {
    let def = &replay["def"];
    let cfg = serde_json::from_value(def["config"].clone()).map_err(|e| format!("config: {e}"))?;
    let setup: DojoSetup = serde_json::from_value(def["setup"].clone()).map_err(|e| format!("setup: {e}"))?;
    let rules: DojoRules = serde_json::from_value(def["rules"].clone()).map_err(|e| format!("rules: {e}"))?;
    let inputs: Vec<Vec<u16>> = serde_json::from_value(replay["inputs"].clone()).map_err(|e| format!("inputs: {e}"))?;
    let want: Vec<u64> = replay["hashes"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().and_then(|s| u64::from_str_radix(s, 16).ok())).collect())
        .unwrap_or_default();
    let mut g = DojoGame::new(data, cfg, setup, rules);
    let n = inputs.iter().map(|v| v.len()).max().unwrap_or(0);
    let mut out = Vec::with_capacity(n);
    for f in 0..n {
        let inp: Vec<u16> = inputs.iter().map(|v| v.get(f).copied().unwrap_or(0)).collect();
        g.advance(&inp, false);
        out.push(g.checksum(&g.save()));
    }
    let first = (0..out.len().max(want.len())).find(|&f| out.get(f) != want.get(f));
    Ok((out, first))
}

pub fn link_info() -> serde_json::Value {
    INFO.with(|i| i.borrow().clone().unwrap_or(serde_json::Value::Null))
}

/// the trade's traffic for one frame: the connection's packets for the friend into the trade peer (the friend under
/// its link id - a reconnection, a new transport id with the same link id, takes over), the peer's tick, its packets
/// out
fn trade_io(net: &mut Net, u: &mut TradeUi, now: f64) {
    let (changes, rest) = net.pump(now);
    // the friend's connection: the one with its link id that is talking now (a restarted friend comes back on a
    // new connection while its dead one may still be listed - a network that died tells nobody; the first listed
    // would keep the dead one)
    let ours = |tid: &str| net.lobby.peer_extras().into_iter().any(|(id, v)| id == tid && serde_json::from_value::<LinkHello>(v).is_ok_and(|h| h.link_id == u.friend_lid));
    let talking = rest.iter().map(|(from, _)| from.clone()).find(|from| *from != u.friend_tid && ours(from));
    let friend = talking.or_else(|| net.friend().filter(|(_, h)| h.link_id == u.friend_lid).map(|(tid, _)| tid).filter(|tid| *tid != u.friend_tid && !net.lobby.peer_connected(&u.friend_tid)));
    if let Some(tid) = friend {
        u.friend_tid = tid;
        u.port.changes.push(PeerChange::Connected(u.friend_lid.clone()));
    }
    for c in changes {
        if let PeerChange::Disconnected(id) = c {
            if id == u.friend_tid {
                u.port.changes.push(PeerChange::Disconnected(u.friend_lid.clone()));
            }
        }
    }
    for (from, b) in rest {
        if from == u.friend_tid {
            u.port.inbox.push((u.friend_lid.clone(), b));
        }
    }
    let n_in = u.port.inbox.len();
    u.peer.tick(now, &mut u.port, &mut u.store);
    let n_out = u.port.outbox.len();
    for (to, b) in std::mem::take(&mut u.port.outbox) {
        if to == u.friend_lid {
            net.t.send(&u.friend_tid, b, now);
        }
    }
    if std::env::var_os("WF_LINK_DEBUG").is_some() && (n_in > 0 || n_out > 0) {
        let recs: Vec<_> = u.store.records().iter().map(|r| (r.id[..6].to_string(), r.state)).collect();
        eprintln!("trade {now:.0} in {n_in} out {n_out} friend {} {} away {:?} recs {recs:?}", u.friend_tid, u.friend_lid, u.away);
    }
}

/// The title with link play: see the module doc.
pub struct LinkSim {
    pub inner: StorySim,
    cfg: LinkConfig,
    flow: Option<Flow>,
    /// the title's link box seen open (a flow starts once per opening)
    wait_seen: bool,
    /// closing the title's link box after a flow: frames of B fed to the title
    closing: Option<u32>,
    prev: u16,
    steps: u64,
    /// the last flow's end (tests, logs): "done" / "cancelled" / the message
    pub last: Option<String>,
    /// the last Dojo match
    pub record: Option<MatchRecord>,
    /// a wf-core link page is open on the title app (`inner`): the title's state from before, restored when the
    /// flow ends, and the page open
    page: Option<(Box<crate::story::StoryState>, &'static str)>,
}

/// the title's link box: its kind (2 Dojo, 1 Trading Post) while open
pub fn title_wait(w: &wf_core::world::World) -> Option<u8> {
    w.menu.stack.iter().rev().find_map(|s| match s {
        wf_core::menu::Screen::Title(t) => Some(t.wait),
        _ => None,
    })?
}

impl LinkSim {
    pub fn new(inner: StorySim, cfg: LinkConfig) -> LinkSim {
        LinkSim { inner, cfg, flow: None, wait_seen: false, closing: None, prev: 0, steps: 0, last: None, record: None, page: None }
    }

    /// what the link part shows now ("title", "waiting", "saves", "chars", "setup", "match", "result", "trade",
    /// "complete", "message", "nosaves", "closing")
    pub fn phase(&self) -> &'static str {
        if self.closing.is_some() {
            return "closing";
        }
        match self.flow.as_ref().map(|f| &f.phase) {
            None => "title",
            Some(Phase::Waiting) => "waiting",
            Some(Phase::Saves { .. }) => "saves",
            Some(Phase::NoSaves) => "nosaves",
            Some(Phase::Chars { .. }) => "chars",
            Some(Phase::Setup { .. }) => "setup",
            Some(Phase::Match(_)) => "match",
            Some(Phase::Result(_)) => "result",
            // (TRADE COMPLETE! shown, the trade settling)
            Some(Phase::Trade(u)) if u.done.is_some() => "complete",
            Some(Phase::Trade(_)) => "trade",
            Some(Phase::Message(_)) => "message",
        }
    }
    /// the Dojo match's rollback session (tests: hashes, inputs, confirmed frames)
    pub fn dojo(&self) -> Option<&P2PSession<DojoGame>> {
        match self.flow.as_ref().map(|f| &f.phase) {
            Some(Phase::Match(m)) => Some(&m.sess),
            _ => None,
        }
    }
    /// the Dojo's ending once decided
    pub fn ending(&self) -> Option<&Ending> {
        match self.flow.as_ref().map(|f| &f.phase) {
            Some(Phase::Result(e)) => Some(e),
            _ => None,
        }
    }
    /// the trade screen's state (tests): (accepted, friend accepted, friend's offer, rows' counts)
    pub fn trade_state(&self) -> Option<(bool, bool, Option<Offer>)> {
        match self.flow.as_ref().map(|f| &f.phase) {
            Some(Phase::Trade(u)) => Some((u.accepted, u.friend_accepted, u.peer.friend_offer().cloned())),
            _ => None,
        }
    }
    /// the Trading Post's page (the wf-core link screen on the title app) while it shows
    pub fn trade_page(&self) -> Option<&wf_core::menu::link::trade::Trade> {
        match self.inner.session.world.menu.stack.last() {
            Some(wf_core::menu::Screen::Link(l)) => match &l.page {
                wf_core::menu::link::LinkPage::Trade(t) => Some(t),
                _ => None,
            },
            _ => None,
        }
    }
    /// the Dojo setup / character pages (tests): (the setup page's row - MAX on the character page, picked, the
    /// setup shown)
    pub fn setup_state(&self) -> Option<(usize, bool, DojoSetup)> {
        match self.flow.as_ref().map(|f| (&f.phase, f.setup)) {
            Some((Phase::Setup { row }, s)) => Some((*row, false, s)),
            Some((Phase::Chars { confirmed, .. }, s)) => Some((usize::MAX, *confirmed, s)),
            _ => None,
        }
    }
    /// the trade screen's "Disconnected" (Some(in doubt): the friend is gone; a trade in doubt keeps the save locked)
    pub fn trade_away(&self) -> Option<bool> {
        match self.flow.as_ref().map(|f| &f.phase) {
            // (a trade completed is not in doubt any more)
            Some(Phase::Trade(u)) if u.done.is_some() => None,
            Some(Phase::Trade(u)) => u.away,
            _ => None,
        }
    }
    /// the scripted player (tests: its memory, its choices)
    pub fn bot(&self) -> Option<&LinkBot> {
        self.cfg.bot.as_ref()
    }
    pub fn bot_mut(&mut self) -> Option<&mut LinkBot> {
        self.cfg.bot.as_mut()
    }
    /// the report of the last flow (as `--link-report` writes it)
    pub fn report_json(&self) -> serde_json::Value {
        let m = self.record.as_ref().map(|r| {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for s in &r.inputs {
                for &b in s {
                    for x in b.to_le_bytes() {
                        h ^= x as u64;
                        h = h.wrapping_mul(0x0100_0000_01b3);
                    }
                }
                h ^= 0xff;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            serde_json::json!({
                "frame": r.frame,
                "checksum": format!("{:016x}", r.checksum),
                "inputs": r.inputs.iter().map(|s| s.len()).collect::<Vec<_>>(),
                "inputs_hash": format!("{h:016x}"),
                "ending": format!("{:?}", r.ending),
                // the cross-build replay data (`dojo_replay`): the definition, every confirmed input, the per-frame
                // checksums this build computed
                "replay": {
                    "def": r.def,
                    "inputs": r.inputs,
                    "hashes": r.hashes.iter().map(|x| format!("{x:016x}")).collect::<Vec<_>>(),
                },
            })
        });
        serde_json::json!({ "last": self.last, "match": m, "steps": self.steps })
    }
    /// leave the link at once (the shell quits / a test drops a peer): the connection goes with the flow
    pub fn drop_link(&mut self) {
        self.flow = None;
    }

    fn data(&self) -> Arc<GameData> {
        self.inner.session.world.data.clone()
    }
}

impl LinkSim {
    fn publish_info(&self) {
        if INFO.with(|i| i.borrow().is_none()) {
            return;
        }
        let w = &self.inner.session.world;
        let title = w.menu.stack.iter().rev().find_map(|s| match s {
            wf_core::menu::Screen::Title(t) => Some(serde_json::json!({ "page": format!("{:?}", t.page), "wait": t.wait })),
            _ => None,
        });
        let setup = self.setup_state().map(|(row, picked, s)| {
            serde_json::json!({ "row": if row == usize::MAX { -1 } else { row as i64 }, "picked": picked, "mode": format!("{:?}", s.mode), "rounds": s.rounds })
        });
        let cursor = self.trade_page().map(|t| serde_json::json!({ "side": t.pick.side, "row": t.pick.first + t.pick.row, "mine": t.pick.mine.len(), "proposal": t.prop.is_some(), "box": t.boxed.is_some() }));
        let trade = self.trade_state().map(|(acc, facc, friend)| {
            serde_json::json!({ "accepted": acc, "friend_accepted": facc, "friend_offer": friend.map(|o| !o.is_empty()) })
        });
        let record = self.record.as_ref().map(|r| serde_json::json!({ "frame": r.frame, "checksum": format!("{:016x}", r.checksum), "ending": format!("{:?}", r.ending) }));
        let v = serde_json::json!({
            "phase": self.phase(), "title": title, "setup": setup, "cursor": cursor, "trade": trade,
            "last": self.last, "record": record, "steps": self.steps, "report": self.report_json(),
        });
        INFO.with(|i| {
            if let Some(x) = i.borrow_mut().as_mut() {
                *x = v;
            }
        });
    }
}

impl Sim for LinkSim {
    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>) {
        self.step_inner(inputs, events);
        self.publish_info();
    }
    fn view(&self, out: &mut RenderView) {
        self.view_inner(out)
    }
    fn frame(&self) -> u64 {
        self.steps
    }
    fn state_hash(&self) -> u64 {
        self.state_hash_inner()
    }
    fn snapshot(&self) -> Option<Vec<u8>> {
        self.snapshot_inner()
    }
    fn restore(&mut self, s: &[u8]) -> bool {
        self.restore_inner(s)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        // (the shells' save-slot persistence reads the title's StorySim)
        self.inner.as_any()
    }
    fn paused_input(&self) -> Option<Buttons> {
        // a link flow cannot wait for a local menu
        self.flow.as_ref().map(|_| 0)
    }
}

impl LinkSim {
    fn step_inner(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>) {
        self.steps += 1;
        // a scripted player plays in place of the shell's buttons
        let bot_keys = self.cfg.bot.take().map(|mut b| {
            let k = b.input(self);
            self.cfg.bot = Some(b);
            k
        });
        let own;
        let inputs = match bot_keys {
            Some(k) => {
                own = [k];
                &own[..]
            }
            None => inputs,
        };
        let held = inputs.first().copied().unwrap_or(0);
        let pressed = held & !self.prev;
        self.prev = held;
        // closing the title's link box with its own B (press, release)
        if let Some(k) = self.closing {
            let b = if k < 2 { B } else { 0 };
            self.inner.step(&[b], events);
            let open = title_wait(&self.inner.session.world).is_some();
            self.closing = if open && k < 60 { Some(k + 1) } else { None };
            if !open {
                self.wait_seen = false;
            }
            return;
        }
        let Some(flow) = self.flow.as_mut() else {
            self.inner.step(inputs, events);
            match title_wait(&self.inner.session.world) {
                Some(kind) if !self.wait_seen => {
                    self.wait_seen = true;
                    let k = if kind == 2 { LinkKind::Dojo } else { LinkKind::Trade };
                    let saves = (0..SLOTS).map(|i| self.inner.session.world.saves.slot(i).cloned()).collect();
                    let data = self.data();
                    match Flow::new(k, data, &mut self.cfg, saves) {
                        Ok(f) => self.flow = Some(f),
                        Err(e) => {
                            eprintln!("link: {e}");
                            self.last = Some(e);
                        }
                    }
                }
                None => self.wait_seen = false,
                _ => {}
            }
            return;
        };
        let now = (self.cfg.clock)();
        let clock = self.cfg.clock.clone();
        // waiting: the title's own box runs (its B Cancel), the lobby under it; the friend there = the link up: the box
        // counts its updates and ends, and the link's Select Saved Game is built in that iteration (menu_parity
        // link_connect)
        if matches!(flow.phase, Phase::Waiting) {
            let up = flow.friend.is_some();
            if let Some(wf_core::menu::Screen::Title(t)) = self.inner.session.world.menu.stack.last_mut() {
                if up && t.wait.is_some() && t.link_up.is_none() {
                    t.link_up = Some(0);
                }
            }
            self.inner.step(inputs, events);
            let linked = match self.inner.session.world.menu.stack.last_mut() {
                Some(wf_core::menu::Screen::Title(t)) => t.linked.take(),
                _ => None,
            };
            if linked.is_some() {
                let saved = Box::new(self.inner.save_mem());
                let flow = self.flow.as_mut().expect("flow");
                let sel = (0..SLOTS).find(|&i| flow.usable(i));
                match sel {
                    Some(sel) => {
                        flow.set_phase(Phase::Saves { sel });
                        wf_core::menu::link::open_saves(&mut self.inner.session.world, flow.kind == LinkKind::Trade);
                        self.page = Some((saved, "saves"));
                    }
                    None => flow.set_phase(Phase::NoSaves),
                }
                return;
            }
            if title_wait(&self.inner.session.world).is_none() {
                self.flow = None;
                self.wait_seen = false;
                self.last = Some("cancelled".into());
                return;
            }
        }
        let flow = self.flow.as_mut().expect("flow");
        let mut out = flow.step(now, &clock, held, pressed, events);
        // the phase's page: wf-core's link screen on the title app, stepped with the keys; its actions go to the flow
        if matches!(out, Out::None) {
            let want = flow.page();
            if want != self.page.as_ref().map(|p| p.1) {
                match want {
                    Some(name) => {
                        let saved = self.page.take().map(|p| p.0).unwrap_or_else(|| Box::new(self.inner.save_mem()));
                        let w = &mut self.inner.session.world;
                        let trade = flow.kind == LinkKind::Trade;
                        let shown = match w.menu.stack.last() {
                            Some(wf_core::menu::Screen::Link(l)) => match l.page {
                                wf_core::menu::link::LinkPage::Saves(_) => Some("saves"),
                                wf_core::menu::link::LinkPage::Trade(_) => Some("trade"),
                            },
                            _ => None,
                        };
                        // (a page that went on by itself - the picker's B shows Select Saved Game - is kept)
                        if shown != Some(name) {
                            match name {
                                // the Trading Post's picker is built from the picked save (the story it loads)
                                "trade" => {
                                    if let Some(sd) = flow.trade_save() {
                                        w.story = sd.story;
                                    }
                                    wf_core::menu::link::open_trade(w);
                                }
                                _ => {
                                    let l = wf_core::menu::link::Link::saves(w, trade);
                                    wf_core::menu::link::open(w, l);
                                }
                            }
                        }
                        self.page = Some((saved, name));
                    }
                    None => {
                        if let Some((saved, _)) = self.page.take() {
                            self.inner.load_mem(&saved);
                        }
                    }
                }
            }
            if self.page.is_some() {
                // the friend's moves the page shows (its proposal, its answer)
                let (offer, answer) = flow.trade_friend();
                if let Some(wf_core::menu::Screen::Link(l)) = self.inner.session.world.menu.stack.last_mut() {
                    if offer.is_some() {
                        l.friend.offer = offer;
                    }
                    if answer.is_some() {
                        l.friend.answer = answer;
                    }
                }
                self.inner.step(inputs, events);
                let acts = match self.inner.session.world.menu.stack.last_mut() {
                    Some(wf_core::menu::Screen::Link(l)) => std::mem::take(&mut l.actions),
                    _ => Vec::new(),
                };
                let flow = self.flow.as_mut().expect("flow");
                for a in acts {
                    out = flow.link_action(a, now, events);
                    if !matches!(out, Out::None) {
                        break;
                    }
                }
                // TRADE COMPLETE! went back to the title's Multiplayer page by itself: the flow ends once the trade has
                // settled on both sides (its connection kept answering meanwhile) - the title stays as it is now
                let on_title = matches!(self.inner.session.world.menu.stack.last(), Some(wf_core::menu::Screen::Title(_)));
                if on_title && matches!(flow.phase, Phase::Trade(_)) && flow.trade_settled() {
                    self.flow = None;
                    self.page = None;
                    self.wait_seen = false;
                    self.last = Some("complete".into());
                    return;
                }
            }
        }
        let flow = self.flow.as_mut().expect("flow");
        if let Some(r) = flow.record.take() {
            self.record = Some(r);
        }
        match out {
            Out::None => {}
            Out::Done => {
                if let Some((saved, _)) = self.page.take() {
                    self.inner.load_mem(&saved);
                }
                self.last = Some(match &flow.phase {
                    Phase::Message(m) if !m.is_empty() => String::from_utf16_lossy(m),
                    Phase::Message(_) => "disconnected".into(),
                    Phase::Result(e) => format!("{e:?}"),
                    _ => "left".into(),
                });
                self.flow = None;
                self.closing = Some(0);
                if let Some(p) = &self.cfg.report {
                    let _ = std::fs::write(p, serde_json::to_vec_pretty(&self.report_json()).unwrap_or_default());
                }
            }
            Out::Slot(i, sd) => {
                let sv = &mut self.inner.session.world.saves;
                sv.set(i, Some(sd));
                sv.generation = sv.generation.wrapping_add(1);
            }
        }
    }

    fn view_inner(&self, out: &mut RenderView) {
        match self.flow.as_ref() {
            Some(_) if self.page.is_some() => self.inner.view(out),
            Some(f) if !matches!(f.phase, Phase::Waiting) => {
                let Some(d) = self.inner.session.world.data.menu.clone() else { return self.inner.view(out) };
                if let Phase::Match(m) = &f.phase {
                    m.sess.game.s.view(out);
                    out.menu = f.match_overlay(&d);
                    return;
                }
                *out = RenderView::default();
                out.frame = self.steps;
                out.menu = Some(f.view(&d));
            }
            _ => self.inner.view(out),
        }
    }
    fn state_hash_inner(&self) -> u64 {
        if self.flow.is_some() || self.closing.is_some() {
            0
        } else {
            self.inner.state_hash()
        }
    }
    fn snapshot_inner(&self) -> Option<Vec<u8>> {
        if self.flow.is_some() || self.closing.is_some() {
            None
        } else {
            self.inner.snapshot()
        }
    }
    fn restore_inner(&mut self, s: &[u8]) -> bool {
        if self.flow.is_some() {
            return false;
        }
        self.inner.restore(s)
    }
}

/// The title app with link play (the native shell's `--link`): [`crate::launcher::title_sim`] wrapped in [`LinkSim`].
pub fn link_title_app(packs: wf_content::Packs, slots: &[Vec<u8>], seed: u32, cfg: LinkConfig) -> Result<crate::App, String> {
    let s = crate::launcher::title_sim(&packs, slots, seed)?;
    Ok(crate::App::new(packs, Box::new(LinkSim::new(s, cfg))))
}

// -------------------------------------------------------------------------------------------------------------------
// the scripted player (tests, the native shell's `--link-bot`)

/// A closed-loop scripted player for the link pages: from the title (START, Multiplayer, Dojo / Trading Post) through
/// the room, the save list, the Dojo's setup / characters / match or the trade screen (an offer, Propose, Accept),
/// the result / TRADE COMPLETE / Disconnected pages (A after a while), back to the title. Its buttons are a pure
/// function of what the screens show and its own memory, so two scripted apps meet whatever the network does
/// (tests/link.rs, `buus-fury --link-bot dojo|trade`, tools/link/twoproc.sh). Set on [`LinkConfig::bot`], the
/// [`LinkSim`] plays it in place of the shell's buttons.
#[derive(Clone, Debug)]
pub struct LinkBot {
    pub kind: LinkKind,
    pub host: bool,
    /// the host's Character Mode / Rounds to set (the guest waits until it sees them)
    pub mode: DojoMode,
    pub rounds: u8,
    /// the offer: this many of the trade list's first row, this much zenie
    pub first: u32,
    pub zenie: u32,
    /// the trade: accept the friend's offer (else only propose)
    pub accept: bool,
    /// the proposal answered (ACCEPT pressed)
    answered: bool,
    /// back at the title after a flow: go into the page again (a test's friend coming back to finish a trade)
    pub repeat: bool,
    downs: u32,
    proposed: bool,
    in_phase: u32,
    last_page: Option<wf_core::menu::title::TPage>,
    last_phase: &'static str,
    /// a flow started (the title afterwards = done)
    pub started: bool,
    k: u64,
}

impl LinkBot {
    pub fn new(kind: LinkKind, host: bool) -> LinkBot {
        LinkBot {
            kind,
            host,
            mode: DojoMode::InGame,
            rounds: 1,
            first: 2,
            zenie: 30,
            accept: true,
            repeat: false,
            downs: 0,
            proposed: false,
            answered: false,
            in_phase: 0,
            last_page: None,
            last_phase: "",
            started: false,
            k: 0,
        }
    }

    /// the flow ran and the title is back
    pub fn done(&self, sim: &LinkSim) -> bool {
        self.started && sim.phase() == "title"
    }

    /// this frame's buttons: presses on every 4th frame (held 2), the choice from what is shown
    pub fn input(&mut self, sim: &LinkSim) -> u16 {
        self.k += 1;
        let ph = sim.phase();
        if self.repeat && self.started && ph == "title" {
            self.started = false;
            self.proposed = false;
            self.last_page = None;
        }
        if ph != self.last_phase {
            self.last_phase = ph;
            self.in_phase = 0;
            self.downs = 0;
        }
        self.in_phase += 1;
        if ph == "match" {
            // P1 (the host) walks in and punches, the guest stands still
            return if self.host { RIGHT | if self.k % 12 < 2 { A } else { 0 } } else { 0 };
        }
        if self.k % 4 >= 2 {
            return 0;
        }
        let want = self.decide(sim, ph);
        if want & (DOWN | UP | LEFT | RIGHT) != 0 {
            self.downs += 1;
        }
        want
    }

    fn decide(&mut self, sim: &LinkSim, ph: &str) -> u16 {
        use wf_core::input::START;
        use wf_core::menu::title::TPage;
        match ph {
            "title" => {
                if self.started {
                    return 0;
                }
                let w = &sim.inner.session.world;
                let t = w.menu.stack.iter().rev().find_map(|s| match s {
                    wf_core::menu::Screen::Title(t) => Some(t),
                    _ => None,
                });
                let Some(t) = t else { return START };
                if self.last_page != Some(t.page) {
                    self.last_page = Some(t.page);
                    self.downs = 0;
                }
                // (the pages' cursor is the selected control's id: Single 0 / Multiplayer 1, Dojo 0 / Trading Post 1;
                // a page keeps it when the flow comes back)
                match t.page {
                    TPage::Title => START,
                    TPage::Mode => {
                        if t.c.sel != 1 {
                            DOWN
                        } else {
                            A
                        }
                    }
                    TPage::Multi => {
                        let want = if self.kind == LinkKind::Trade { 1 } else { 0 };
                        if t.wait.is_some() {
                            0
                        } else if t.c.sel != want {
                            DOWN
                        } else {
                            A
                        }
                    }
                    _ => 0,
                }
            }
            "waiting" => {
                self.started = true;
                0
            }
            "saves" => A,
            "chars" => {
                let picked = sim.setup_state().map(|s| s.1).unwrap_or(true);
                if picked {
                    0
                } else {
                    A
                }
            }
            "setup" => {
                let Some((row, _, s)) = sim.setup_state() else { return 0 };
                if self.host {
                    // the mode, the rounds, then Start Fight
                    if row == 0 && s.mode != self.mode {
                        RIGHT
                    } else if row == 0 {
                        DOWN
                    } else if row == 1 && s.rounds != self.rounds {
                        // (1 / 3 / 5: LEFT down, RIGHT up)
                        if s.rounds > self.rounds {
                            LEFT
                        } else {
                            RIGHT
                        }
                    } else if row == 1 {
                        DOWN
                    } else {
                        A
                    }
                } else if s.rounds == self.rounds && s.mode == self.mode {
                    // the guest goes on once the host's setup shows
                    A
                } else {
                    0
                }
            }
            "trade" => {
                // "Disconnected": a trade in doubt (the save locked) waits for the friend - a key would leave; with
                // nothing in doubt, A leaves after a while
                match sim.trade_away() {
                    Some(true) => return 0,
                    Some(false) => return if self.in_phase > 40 { A } else { 0 },
                    None => {}
                }
                use wf_core::menu::link::trade::TBox;
                let Some(t) = sim.trade_page() else { return 0 };
                // "How many to trade?": up / down to the count wanted, A
                if let Some(TBox::Count(b)) = &t.boxed {
                    let want = if b.e.kind == KIND_ZENIE { self.zenie } else { self.first };
                    return if b.count < want {
                        RIGHT
                    } else if b.count > want {
                        LEFT
                    } else {
                        A
                    };
                }
                if t.boxed.is_some() {
                    return 0;
                }
                // the proposal: ACCEPT (the cursor's first place) once
                if t.prop.is_some() {
                    if self.accept && !self.answered {
                        self.answered = true;
                        return A;
                    }
                    return 0;
                }
                // the picker: the list's first entry (`first` of it) and zenie onto "Your Trade:", then Propose
                let p = &t.pick;
                let mut want: Vec<usize> = Vec::new();
                if self.first > 0 && !p.list.is_empty() {
                    want.push(0);
                }
                if self.zenie > 0 {
                    if let Some(k) = p.list.iter().position(|e| e.kind == KIND_ZENIE) {
                        want.push(k);
                    }
                }
                let have = |k: usize| p.mine.iter().any(|m| m.kind == p.list[k].kind && m.id == p.list[k].id);
                if let Some(&k) = want.iter().find(|&&k| k < p.list.len() && !have(k)) {
                    if p.side != 0 {
                        return LEFT;
                    }
                    let cur = (p.first + p.row).max(0) as usize;
                    return if cur < k {
                        DOWN
                    } else if cur > k {
                        UP
                    } else {
                        A
                    };
                }
                if !self.proposed {
                    self.proposed = true;
                    return wf_core::input::R;
                }
                0
            }
            // TRADE COMPLETE!: the page goes back to the title's Multiplayer page by itself (a key there would open
            // the link box again)
            "complete" if self.kind == LinkKind::Trade => 0,
            // the result / Disconnected: shown a while, then A
            "result" | "complete" | "message" | "nosaves" => {
                if self.in_phase > 40 {
                    A
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
}
