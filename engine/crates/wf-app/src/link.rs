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
use wf_core::input::{A, B, DOWN, L, LEFT, R, RIGHT, UP};
use wf_core::menu::save::{SaveData, SLOTS};
use wf_core::menu::text::{units, Canvas, Pen, TextData};
use wf_core::menu::MenuData;
use wf_net::dojo::{balanced_roster, fighter_setup, DojoChar, DojoGame, DojoHello, DojoMatch, DojoMode, DojoRules, DojoSave, DojoSetup, Ending, Phase as DPhase};
use wf_net::game::Game;
use wf_net::lobby::{Lobby, LobbySettings, Role, KIND_DOJO, KIND_TRADE};
use wf_net::netsim::{content_id, Clock};
use wf_net::proto::{decode, Msg};
use wf_net::session::{NetConfig, NetEvent, P2PSession};
use wf_net::trade::{Offer, TState, TradeEvent, TradePeer, TradeStore};
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
#[derive(Clone, Debug, PartialEq)]
enum Row {
    /// supply index, count in the save
    Supply(u16, u32),
    /// equipment item id, unworn copies
    Equip(i32, u32),
    /// zenie in the save
    Zenie(u32),
}

struct TradeUi {
    peer: TradePeer,
    store: FileStore,
    port: Port,
    friend_tid: String,
    friend_lid: String,
    rows: Vec<Row>,
    counts: Vec<u32>,
    /// the cursor: a row, or the buttons below (rows.len())
    sel: usize,
    /// "How many to trade?" open on `sel`
    picking: bool,
    /// the button under the cursor: 0 Propose Trade, 1 Accept, 2 Decline
    btn: usize,
    accepted: bool,
    friend_accepted: bool,
    /// a status line (the game's strings where it has one)
    note: Vec<u16>,
    /// "Disconnected" with a trade in doubt
    away: Option<bool>,
    /// the committed trade's id (TRADE COMPLETE) and the frames its page has shown
    done: Option<(String, u32)>,
}

struct Match {
    sess: P2PSession<DojoGame>,
    /// the game advanced with confirmed inputs only (the result / prize source)
    conf: DojoGame,
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
    /// "TRADE COMPLETE!": the trade's connection keeps answering (the friend may still have to commit) until the
    /// record is Done or the friend is gone - only then does a key leave (None: nothing left to answer)
    Complete(Option<Box<TradeUi>>),
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

    /// one frame. `pressed`: new presses; `held`: the buttons held (the match's input)
    fn step(&mut self, now: f64, clock: &Clock, held: u16, pressed: u16, ev: &mut Vec<SimEvent>) -> Out {
        self.t = self.t.wrapping_add(1);
        if let Some(r) = self.net.lobby.rejected.clone() {
            if !matches!(self.phase, Phase::Message(_) | Phase::Match(_) | Phase::Result(_) | Phase::Complete(_) | Phase::Trade(_)) {
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
        if matches!(self.phase, Phase::Complete(Some(_))) {
            return self.step_complete(now, pressed, ev);
        }
        let _ = self.net.pump(now);
        if self.friend.is_none() {
            self.friend = self.net.friend();
        } else if let Some(f) = self.net.friend() {
            // (its hello changes as it picks: the latest)
            self.friend = Some(f);
        }
        let lost = self.friend_gone() || self.net.t.closed();
        if lost && !matches!(self.phase, Phase::Waiting | Phase::Message(_) | Phase::Result(_) | Phase::Complete(_)) {
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
            Phase::Waiting => {
                if self.friend.is_some() {
                    let sel = (0..SLOTS).find(|&i| self.usable(i));
                    self.set_phase(match sel {
                        Some(sel) => Phase::Saves { sel },
                        None => Phase::NoSaves,
                    });
                }
                Out::None
            }
            Phase::NoSaves | Phase::Message(_) | Phase::Result(_) | Phase::Complete(_) => {
                if pressed & (A | B) != 0 {
                    sfx(ev, SFX_MENU);
                    return Out::Done;
                }
                Out::None
            }
            Phase::Saves { sel } => {
                let old = *sel;
                let mut s = old;
                if pressed & (UP | DOWN) != 0 {
                    let d = if pressed & UP != 0 { SLOTS - 1 } else { 1 };
                    for _ in 0..SLOTS {
                        s = (s + d) % SLOTS;
                        if self.usable(s) {
                            break;
                        }
                    }
                    if s != old {
                        sfx(ev, SFX_CURSOR);
                        if let Phase::Saves { sel } = &mut self.phase {
                            *sel = s;
                        }
                    }
                }
                if pressed & B != 0 {
                    sfx(ev, SFX_MENU);
                    return Out::Done;
                }
                if pressed & A != 0 && self.usable(s) {
                    sfx(ev, SFX_MENU);
                    self.slot = s;
                    match self.kind {
                        LinkKind::Dojo => {
                            self.dsave = self.saves[s].as_ref().and_then(|sd| DojoSave::from_save(s, sd, &self.data));
                            self.set_phase(Phase::Setup { row: if self.host() { 0 } else { 2 } });
                        }
                        LinkKind::Trade => return self.open_trade(now, ev),
                    }
                }
                Out::None
            }
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
        self.set_phase(Phase::Match(Box::new(Match { sess, conf, m: DojoMatch::new(slot as u8), audio_at: 0 })));
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
        let rows = trade_rows(&sd, &story_flags(&self.data));
        let nonce = (self.t as u64) ^ ((self.slot as u64) << 32) ^ (self.net.lobby.settings.secret.len() as u64);
        let peer = TradePeer::new(&self.link_id, &h.link_id, nonce ^ u64::from(self.net.t.local_id().unwrap_or_default().len() as u32), &store);
        let n = rows.len();
        self.set_phase(Phase::Trade(Box::new(TradeUi {
            peer,
            store,
            port: Port { me: self.link_id.clone(), ..Default::default() },
            friend_tid: tid,
            friend_lid: h.link_id,
            rows,
            counts: vec![0; n],
            sel: 0,
            picking: false,
            btn: 0,
            accepted: false,
            friend_accepted: false,
            note: Vec::new(),
            away: None,
            done: None,
        })));
        Out::None
    }

    /// "TRADE COMPLETE!": the trade keeps answering the friend; a key leaves once the record is Done (the friend
    /// committed too), the friend is gone, or after 10 s (the friend then finishes at the next link: the journal)
    fn step_complete(&mut self, now: f64, pressed: u16, ev: &mut Vec<SimEvent>) -> Out {
        let Phase::Complete(Some(u)) = &mut self.phase else { return Out::None };
        trade_io(&mut self.net, u, now);
        for e in u.peer.events() {
            match e {
                TradeEvent::Disconnected { in_doubt } => u.away = Some(in_doubt),
                TradeEvent::Reconnected => u.away = None,
                _ => {}
            }
        }
        let Some((id, shown)) = u.done.as_mut() else { return Out::None };
        *shown += 1;
        let settled = u.store.record(id).is_some_and(|r| r.state == TState::Done);
        let gone = u.away.is_some() || self.net.t.closed();
        if pressed & (A | B) != 0 && (settled || gone || *shown > 600) {
            sfx(ev, SFX_MENU);
            return Out::Done;
        }
        Out::None
    }

    fn step_trade(&mut self, now: f64, pressed: u16, ev: &mut Vec<SimEvent>) -> Out {
        let Phase::Trade(u) = &mut self.phase else { return Out::None };
        trade_io(&mut self.net, u, now);
        let mut done = None;
        for e in u.peer.events() {
            match e {
                TradeEvent::FriendOffer(_) => {
                    // (a new offer from the friend voids both acceptances)
                    u.friend_accepted = false;
                    u.accepted = false;
                }
                TradeEvent::FriendAccepted => u.friend_accepted = true,
                TradeEvent::Declined => {
                    u.friend_accepted = false;
                    u.accepted = false;
                    u.note = units("Your friend declined");
                }
                TradeEvent::Agreed(_) => u.note = units("..."),
                TradeEvent::Complete(id) => {
                    u.done = Some((id, 0));
                    done = Some(true);
                }
                TradeEvent::Aborted(_, why) => {
                    u.accepted = false;
                    u.friend_accepted = false;
                    u.note = units(&why);
                }
                TradeEvent::Disconnected { in_doubt } => u.away = Some(in_doubt),
                TradeEvent::Reconnected => u.away = None,
            }
        }
        if done == Some(true) {
            sfx(ev, SFX_ACCEPT);
            let sd = u.store.save();
            if let Phase::Trade(u) = std::mem::replace(&mut self.phase, Phase::Complete(None)) {
                self.set_phase(Phase::Complete(Some(u)));
            }
            if let Some(sd) = sd {
                self.saves[self.slot] = Some(sd.clone());
                return Out::Slot(self.slot, sd);
            }
            return Out::None;
        }
        // input
        if let Some(in_doubt) = u.away {
            // "Disconnected": with a trade in doubt the save stays locked until the friend is back; any key leaves
            // (the journal finishes it at the next link with this friend)
            let _ = in_doubt;
            if pressed & (A | B) != 0 {
                sfx(ev, SFX_MENU);
                return Out::Done;
            }
            return Out::None;
        }
        let n = u.rows.len();
        let locked = u.accepted;
        if u.picking {
            let k = u.sel;
            let max = match u.rows[k] {
                Row::Supply(_, c) | Row::Equip(_, c) => c,
                Row::Zenie(z) => z,
            };
            let step = if matches!(u.rows[k], Row::Zenie(_)) { if pressed & (L | R) != 0 { 100 } else { 1 } } else { 1 };
            let c = &mut u.counts[k];
            if pressed & (UP | R) != 0 {
                *c = (*c + step).min(max);
                sfx(ev, SFX_CURSOR);
            }
            if pressed & (DOWN | L) != 0 {
                *c = c.saturating_sub(step);
                sfx(ev, SFX_CURSOR);
            }
            if pressed & (A | B) != 0 {
                sfx(ev, SFX_MENU);
                u.picking = false;
            }
            return Out::None;
        }
        if pressed & UP != 0 && u.sel > 0 {
            u.sel -= 1;
            sfx(ev, SFX_CURSOR);
        }
        if pressed & DOWN != 0 && u.sel < n {
            u.sel += 1;
            sfx(ev, SFX_CURSOR);
        }
        if u.sel == n && pressed & (LEFT | RIGHT) != 0 {
            u.btn = if pressed & RIGHT != 0 { (u.btn + 1) % 3 } else { (u.btn + 2) % 3 };
            sfx(ev, SFX_CURSOR);
        }
        if pressed & B != 0 {
            sfx(ev, SFX_MENU);
            if u.accepted {
                u.peer.decline();
                u.accepted = false;
                return Out::None;
            }
            return Out::Done;
        }
        if pressed & A != 0 {
            sfx(ev, SFX_MENU);
            if u.sel < n {
                if !locked {
                    u.picking = true;
                }
            } else {
                match u.btn {
                    0 => {
                        // "Propose Trade": the offer as picked
                        let o = offer_of(&u.rows, &u.counts);
                        u.peer.set_offer(o);
                        u.accepted = false;
                        u.note = Vec::new();
                    }
                    1 => match u.peer.accept(&u.store) {
                        Ok(()) => {
                            u.accepted = true;
                            u.note = Vec::new();
                        }
                        Err(e) => u.note = units(&e),
                    },
                    _ => {
                        u.peer.decline();
                        u.accepted = false;
                    }
                }
            }
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
            Phase::Complete(_) => {
                head(&mut cv, t.s(79));
                centre(&mut cv, 62, t.s(88), MenuFont::Bold, 6);
                centre(&mut cv, 80, t.s(89), MenuFont::Bold, 6);
            }
            Phase::Trade(u) => {
                head(&mut cv, t.s(79));
                line(&mut cv, 16, 36, t.s(80), 2);
                line(&mut cv, 116, 36, t.s(81), 2);
                // your side: rows with the counts picked
                let first = u.sel.saturating_sub(4).min(u.rows.len().saturating_sub(5));
                for (k, (r, &c)) in u.rows.iter().zip(u.counts.iter()).enumerate().skip(first).take(5) {
                    let y = 50 + (k - first) as i32 * 13;
                    let mut s = row_name(d, t, r);
                    s.extend(units(&format!(" {c}")));
                    line(&mut cv, 24, y, &s, if u.sel == k { 6 } else { 2 });
                    if u.sel == k {
                        cursor(&mut cv, 22, y - 4);
                    }
                }
                // a list longer than its five rows scrolls with the cursor: the game's UP / DOWN prompts (the
                // Inventory's long list, button table `0x086B77A4`) beside its first and last row, greyed (frame 2)
                // at either end (INFERRED placement: the link pages were never recorded)
                if u.rows.len() > 5 {
                    let more_up = first > 0;
                    let more_down = first + 5 < u.rows.len();
                    let (_, bw, bh, up) = wf_core::menu::kit::BUTTONS[wf_core::menu::kit::BTN_UP as usize];
                    let (_, _, _, down) = wf_core::menu::kit::BUTTONS[wf_core::menu::kit::BTN_DOWN as usize];
                    cv.image(up, if more_up { 0 } else { 2 }, bw, bh, 98, 44);
                    cv.image(down, if more_down { 0 } else { 2 }, bw, bh, 98, 94);
                }
                // the friend's offer
                if let Some(o) = u.peer.friend_offer() {
                    let mut y = 50;
                    for (&i, &n) in &o.supplies {
                        let mut s = d.inv.name.get(i as usize).cloned().unwrap_or_default();
                        s.extend(units(&format!(" {n}")));
                        line(&mut cv, 120, y, &s, 2);
                        y += 13;
                    }
                    for &id in &o.equipment {
                        line(&mut cv, 120, y, &equip_name(d, id), 2);
                        y += 13;
                    }
                    if o.zenie > 0 {
                        let mut s = t.s(82).to_vec();
                        s.extend(units(&format!(" {}", o.zenie)));
                        line(&mut cv, 120, y, &s, 2);
                    }
                }
                // the buttons
                let by = 122;
                for (k, sid) in [84usize, 85, 86].into_iter().enumerate() {
                    let x = [20, 112, 164][k];
                    line(&mut cv, x, by, t.s(sid), if u.sel == u.rows.len() && u.btn == k { 6 } else { 2 });
                    if u.sel == u.rows.len() && u.btn == k {
                        cursor(&mut cv, x - 2, by - 4);
                    }
                }
                if u.picking {
                    line(&mut cv, 16, 112, t.s(83), 3);
                } else if let Some(in_doubt) = u.away {
                    let mut s = t.s(93).to_vec();
                    if in_doubt {
                        s.extend(units(" - finishing when your friend is back"));
                    }
                    line(&mut cv, 16, 112, &s, 3);
                } else if u.accepted {
                    line(&mut cv, 16, 112, t.s(87), 3);
                } else if !u.note.is_empty() {
                    line(&mut cv, 16, 112, &u.note, 3);
                } else if u.friend_accepted {
                    line(&mut cv, 16, 112, &units("Your friend accepted"), 3);
                }
            }
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

fn equip_name(d: &MenuData, id: i32) -> Vec<u16> {
    d.items.equipment.get(id as usize).map(|e| e.name.clone()).unwrap_or_else(|| units(&format!("#{id}")))
}

fn row_name(d: &MenuData, t: &TextData, r: &Row) -> Vec<u16> {
    match r {
        Row::Supply(i, have) => {
            let mut s = d.inv.name.get(*i as usize).cloned().unwrap_or_default();
            s.extend(units(&format!(" ({have})")));
            s
        }
        Row::Equip(id, have) => {
            let mut s = equip_name(d, *id);
            s.extend(units(&format!(" ({have})")));
            s
        }
        Row::Zenie(z) => {
            let mut s = t.s(82).to_vec();
            s.extend(units(&format!(" ({z})")));
            s
        }
    }
}

/// what the save can trade: supplies (not story items) in the save, unworn equipment, zenie
fn trade_rows(sd: &SaveData, story: &[bool]) -> Vec<Row> {
    let inv = crate::trade::inventory_of(sd, story);
    let mut rows = Vec::new();
    for (i, &n) in inv.supplies.iter().enumerate() {
        if n > 0 && !inv.story.get(i).copied().unwrap_or(false) {
            rows.push(Row::Supply(i as u16, n));
        }
    }
    let mut eq: std::collections::BTreeMap<i32, u32> = Default::default();
    for &(id, worn) in &inv.equipment {
        if !worn {
            *eq.entry(id).or_default() += 1;
        }
    }
    for (id, n) in eq {
        rows.push(Row::Equip(id, n));
    }
    rows.push(Row::Zenie(inv.zenie));
    rows
}

fn offer_of(rows: &[Row], counts: &[u32]) -> Offer {
    let mut o = Offer::default();
    for (r, &c) in rows.iter().zip(counts) {
        if c == 0 {
            continue;
        }
        match r {
            Row::Supply(i, _) => {
                o.supplies.insert(*i, c);
            }
            Row::Equip(id, _) => o.equipment.extend(std::iter::repeat_n(*id, c as usize)),
            Row::Zenie(_) => o.zenie = c,
        }
    }
    o
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
        LinkSim { inner, cfg, flow: None, wait_seen: false, closing: None, prev: 0, steps: 0, last: None, record: None }
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
            Some(Phase::Trade(_)) => "trade",
            Some(Phase::Complete(_)) => "complete",
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
    /// the trade rows' labels (tests / tools)
    pub fn trade_rows(&self) -> Vec<String> {
        match self.flow.as_ref().map(|f| &f.phase) {
            Some(Phase::Trade(u)) => u.rows.iter().map(|r| format!("{r:?}")).collect(),
            _ => Vec::new(),
        }
    }
    /// the cursor's place on the trade screen / the picked counts (tests)
    pub fn trade_cursor(&self) -> Option<(usize, bool, usize, Vec<u32>)> {
        match self.flow.as_ref().map(|f| &f.phase) {
            Some(Phase::Trade(u)) => Some((u.sel, u.picking, u.btn, u.counts.clone())),
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
        let cursor = self.trade_cursor().map(|(sel, picking, btn, counts)| serde_json::json!({ "sel": sel, "picking": picking, "btn": btn, "counts": counts }));
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
        // waiting: the title's own box runs (its B Cancel), the lobby under it
        if matches!(flow.phase, Phase::Waiting) {
            self.inner.step(inputs, events);
            if title_wait(&self.inner.session.world).is_none() {
                self.flow = None;
                self.wait_seen = false;
                self.last = Some("cancelled".into());
                return;
            }
        }
        let flow = self.flow.as_mut().expect("flow");
        let out = flow.step(now, &clock, held, pressed, events);
        if let Some(r) = flow.record.take() {
            self.record = Some(r);
        }
        match out {
            Out::None => {}
            Out::Done => {
                self.last = Some(match &flow.phase {
                    Phase::Message(m) if !m.is_empty() => String::from_utf16_lossy(m),
                    Phase::Message(_) => "disconnected".into(),
                    Phase::Result(e) => format!("{e:?}"),
                    Phase::Complete(_) => "complete".into(),
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
                let Some((sel, picking, btn, counts)) = sim.trade_cursor() else { return 0 };
                let Some((accepted, _, friend)) = sim.trade_state() else { return 0 };
                let n = counts.len();
                let (first, zenie) = (self.first, self.zenie);
                let target = |k: usize| if k == 0 { first } else if k == n - 1 { zenie } else { 0 };
                if let Some(k) = (0..n).find(|&k| counts[k] != target(k)) {
                    if picking && sel == k {
                        return if counts[k] < target(k) { UP } else { DOWN };
                    }
                    if picking {
                        return A;
                    }
                    return if sel == k { A } else if sel < k { DOWN } else { UP };
                }
                if picking {
                    return A;
                }
                if sel < n {
                    return DOWN;
                }
                if !self.proposed {
                    if btn != 0 {
                        return LEFT;
                    }
                    self.proposed = true;
                    return A;
                }
                if self.accept && friend.is_some_and(|o| !o.is_empty()) && !accepted {
                    return if btn != 1 { RIGHT } else { A };
                }
                0
            }
            // the result / TRADE COMPLETE / Disconnected: shown a while, then A
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
