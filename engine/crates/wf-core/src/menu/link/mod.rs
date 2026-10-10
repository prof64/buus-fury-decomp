//! The link-cable pages (title -> Multiplayer -> Dojo / Trading Post, after "Waiting for a friend..."): the game's
//! own screens, drawn and stepped as its modes are (re/buu/menu/NOTES_screens.md "Multiplayer"; mode tables
//! `0x08049B2C` Select Game, `0x0804A3BC` .. Dojo (dojo.rs, chars.rs), `0x0804A60C` .. Trading Post: trade.rs). One [`Link`] screen holds the page
//! on show; the app's link flow (wf_app::link) steps it with the local keys, takes its [`LinkAction`]s and gives it
//! what the friend's side did ([`Friend`]). Gated pixel for pixel against two linked emulators
//! (re/buu/menu/linkcap.py, scenarios_link.py -> data/menu/link_*.json, tests/menu_parity.rs).
//!
//! **The Dojo flow** (dojo.rs, chars.rs): Select Saved Game -> A: the host ([`Link::host`], player 1) goes on to the
//! setup page ([`LinkPage::Setup`]); the guest waits in "Waiting for a friend..." over its list ([`Link::wait`]). The
//! host's START FIGHT runs the same box over the setup page; both boxes end once the friend has answered
//! ([`Friend::answered`]: the guest has picked its save and taken the host's setup) and both sides go on to the
//! character pick ([`LinkPage::Chars`], the roster by the host's Character Mode: [`Friend::balanced`] on the guest's
//! side). A there runs the pick's wait box until the friend's pick arrives ([`Friend::picked`]); then
//! [`LinkAction::Fight`] (the fight, another module). B: setup -> Select Saved Game ([`LinkAction::SetupBack`]); a wait
//! box -> its page ([`LinkAction::WaitCancel`]); the character pick -> the setup page (host) / Select Saved Game
//! (guest) ([`LinkAction::CharsBack`]).
//!
//! **For the app** (`wf_app::link`): make the screen with [`Link::saves`] (`host` = player 1), step the menu with the
//! local keys, drain [`Link::actions`] after each step (`take_actions`), and report the friend's side with
//! [`friend_answered`], [`friend_picked`], [`friend_setup`]: each is what the game's link message sets, so the pages
//! end their boxes on the same steps as the game.
//!
//! **Trading Post (for the app's link flow).** Load the picked save into the story, then [`open_trade`] (the picker
//! is built from the story). Take the actions each step: `Propose(offer)` - send our offer; `CancelPropose` - withdraw
//! it; `Answer(accept)` - send ACCEPT / DECLINE; `CancelAnswer` - withdraw the answer; `LeaveProposal` - we left the
//! proposal (B); `Complete { mine, friend }` - both accepted: apply and save now (TRADE COMPLETE! shows, then the
//! screen becomes the title's Multiplayer page by itself); `Unpick` - B on the picker, Select Saved Game shows. Give
//! the friend's moves as they arrive: `Friend::offer = Some(its offer)` when it proposed, `Friend::answer =
//! Some(accept)` when it answered (the boxes take them; a decline on either side brings both back to the picker).

pub mod trade;

use super::kit::{Container, Widget};
use super::save::GameList;
use super::{Action, MenuData, Page};
use crate::world::World;
use serde::{Deserialize, Serialize};
use wf_view::MenuView;

/// what the local player did on a page (taken by the app's link flow)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkAction {
    /// Select Saved Game: A on a slot with a game
    Save(usize),
    /// B: back out of the link
    Back,
    /// the setup page's START FIGHT: the host's setup (Character Mode Balanced, rounds 1 / 3 / 5), the wait box is up
    StartFight { balanced: bool, rounds: u8 },
    /// B on the setup page: back to Select Saved Game (the save pick undone)
    SetupBack,
    /// B in a wait box: back to the page under it (the save pick / START FIGHT / the character pick undone)
    WaitCancel,
    /// the wait box ended: the friend answered (the setup / save exchange: on to the character pick)
    Chars,
    /// the character pick's A: the roster index (Balanced: [`chars::ROSTER`]; In-Game: the save's character id)
    Pick(i32),
    /// B on the character pick: back to the setup page (host) / Select Saved Game (guest)
    CharsBack,
    /// both picked: the fight starts (the character pick's box ended with 2)
    Fight,
    /// Trading Post, the picker's R "Propose Trade": our offer is on the table ("Waiting for a friend..." until
    /// [`Friend::offer`])
    Propose(Vec<trade::TradeEntry>),
    /// B in the box after Propose Trade: the proposal withdrawn (back to the picker)
    CancelPropose,
    /// the proposal's A: ACCEPT (true) / DECLINE ("Waiting for a friend..." until [`Friend::answer`])
    Answer(bool),
    /// B in the box after ACCEPT / DECLINE: the answer withdrawn (back to the proposal)
    CancelAnswer,
    /// B on the proposal: back to the picker (the offers stay as they are)
    LeaveProposal,
    /// both accepted: TRADE COMPLETE! shows now - apply the trade to the save (ours given, the friend's taken) and
    /// write it (the game does both here); after 61 updates the page goes back to the title's Multiplayer page
    Complete { mine: Vec<trade::TradeEntry>, friend: Vec<trade::TradeEntry> },
    /// B on the Trading Post's picker: back to Select Saved Game (the page shows it)
    Unpick,
}

/// what the friend's side has done, as far as the pages show it
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Friend {
    /// the friend has picked its save
    pub saved: bool,
    /// the link flag `0x03002F54` as this side sees it: the friend's answer the save / setup wait box waits for (the
    /// host's: the guest has the setup; the guest's: the host's setup arrived); the box clears it when it ends
    #[serde(default)]
    pub answered: bool,
    /// the host's Character Mode as the guest got it (the host's own page knows it): Balanced (else In-Game)
    #[serde(default)]
    pub balanced: Option<bool>,
    /// the friend's character pick arrived (`[0x03002FB0 + 4]`, its index at `[0x03002FB0]`)
    #[serde(default)]
    pub picked: Option<i32>,
    /// Trading Post: the friend's proposal (its offer) - set when it arrives; the box after our Propose Trade takes it
    /// (then the proposal page shows; one that came first is taken as soon as we propose)
    #[serde(default)]
    pub offer: Option<Vec<trade::TradeEntry>>,
    /// Trading Post: the friend's answer to the proposal (true ACCEPT) - set when it arrives; the box after our
    /// ACCEPT / DECLINE takes it: both accepted = TRADE COMPLETE!, else back to the picker
    #[serde(default)]
    pub answer: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkPage {
    /// Select Saved Game (vt `0x08049B2C`: the game list `0x08005460(0x5E "Select Saved Game", 0xD, arrows)`)
    Saves(GameList),
    /// the Dojo's setup page (host; vt `0x0804A3BC`)
    Setup(dojo::Setup),
    /// the Dojo's character pick (vt `0x0804CAA8`)
    Chars(chars::Chars),
    /// the Trading Post's pages (trade.rs: the picker, "How many to trade?", the proposal, the boxes)
    Trade(Box<trade::Trade>),
}

pub mod chars;
pub mod dojo;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Link {
    pub page: LinkPage,
    /// Trading Post (mode +0xB38 = 1) or Dojo
    pub trade: bool,
    pub actions: Vec<LinkAction>,
    pub friend: Friend,
    /// the list's figures as the last update left them: OAM is written by the update, so a redraw that spans
    /// frames shows the new surface under the old figures (menu_parity link_saves f26)
    #[serde(default)]
    pub figs_shown: Option<Vec<super::save::Fig>>,
    /// this side is the host (player 1, the setup page's owner)
    #[serde(default)]
    pub host: bool,
    /// "Waiting for a friend..." nested over the page (dojo.rs)
    #[serde(default)]
    pub wait: Option<dojo::Wait>,
    /// the wait box ended with B: its exit stub iteration, the page not updated yet
    #[serde(default)]
    pub stub: bool,
    /// a Select Saved Game made by a Dojo page's B (`0x08005460` in the event): its walker's animation runs on the
    /// tick its idle enter drew (menu_parity link_setup_back: the walker's +0x12 = that draw)
    #[serde(default)]
    pub walk_from_idle: bool,
    #[serde(default)]
    pub walk_jump: bool,
    /// the save slot this side picked (its party is the In-Game roster)
    #[serde(default)]
    pub slot: usize,
}

/// the pages' sounds: the list's A (0x08020054: sound 40), the cursor (39)
const SFX_PICK: i32 = 40;
/// a figure's animation step from the game object's tick at the capture (+0x12)
const FIG_T_OFF: i32 = -1;

impl Link {
    pub fn saves(w: &World, trade: bool) -> Link {
        let mut l = GameList::new(w, 0x5E, 0xD, true);
        l.corners = false;
        // (the Trading Post's list has no walker: 0x080202C8 with +0x2F8 = 0)
        l.walker = !trade;
        Link::new(LinkPage::Saves(l), trade)
    }
    /// a page with nothing pending
    pub fn new(page: LinkPage, trade: bool) -> Link {
        Link { page, trade, actions: Vec::new(), friend: Friend::default(), figs_shown: None, host: false, wait: None, stub: false, walk_from_idle: false, walk_jump: false, slot: 0 }
    }
    /// the actions since the last call (the app's link flow)
    pub fn take_actions(&mut self) -> Vec<LinkAction> {
        std::mem::take(&mut self.actions)
    }
}

/// the open link screen (the menu's top screen)
fn link_mut(w: &mut World) -> Option<&mut Link> {
    match w.menu.stack.last_mut() {
        Some(super::Screen::Link(l)) => Some(l),
        _ => None,
    }
}

/// the friend's answer to the save / setup wait box arrived (the link flag `0x03002F54` set)
pub fn friend_answered(w: &mut World) {
    if let Some(l) = link_mut(w) {
        l.friend.answered = true;
    }
}

/// the friend's character pick arrived (the guest's side also takes the host's mode with the setup:
/// [`friend_setup`])
pub fn friend_picked(w: &mut World, index: i32) {
    if let Some(l) = link_mut(w) {
        l.friend.picked = Some(index);
    }
}

/// the save list's figures' sort depths as a capture shows them (object +4, never set by the game: their OAM order;
/// harnesses only)
pub fn figure_depths(w: &mut World, d: &[i32]) {
    if let Some(Link { page: LinkPage::Saves(g), .. }) = link_mut(w) {
        g.depths = Some(d.to_vec());
    }
}

/// the host's setup as the guest received it (its character pick's roster: Balanced or the save's party)
pub fn friend_setup(w: &mut World, balanced: bool) {
    if let Some(l) = link_mut(w) {
        l.friend.balanced = Some(balanced);
    }
}

impl Link {
    fn page_container(&mut self) -> &mut Container {
        match &mut self.page {
            LinkPage::Saves(l) => &mut l.c,
            LinkPage::Setup(s) => &mut s.c,
            LinkPage::Chars(c) => &mut c.c,
            LinkPage::Trade(t) => t.container(),
        }
    }

    /// the wait box's step and its end (dojo.rs)
    fn step_wait(&mut self, w: &mut World) -> Action {
        let Some(mut wt) = self.wait.take() else { return Action::None };
        let flag = match wt.kind {
            dojo::WaitKind::Pick => self.friend.picked.is_some(),
            dojo::WaitKind::Answer => self.friend.answered,
        };
        let end = wt.step(w, flag);
        let t = std::sync::Arc::clone(&w.data);
        let t = &t.menu.as_ref().expect("menu content").text;
        match end {
            None => self.wait = Some(wt),
            Some(dojo::WaitEnd::Cancel) => {
                self.actions.push(LinkAction::WaitCancel);
                match &mut self.page {
                    // (the page's picture under the box keeps the A prompt pressed until its redraw next iteration)
                    LinkPage::Setup(s) => {
                        s.unpress = true;
                        s.c.drawn = false;
                    }
                    LinkPage::Chars(c) => c.wait_cancelled(),
                    LinkPage::Trade(_) => {}
                    LinkPage::Saves(l) => {
                        l.c.drawn = false;
                        // (the box's exit: the list's next update does not come before its following iteration -
                        // menu_parity link_guest_wait, the walker's cel)
                        self.stub = true;
                    }
                }
            }
            Some(dojo::WaitEnd::Done) => match wt.kind {
                dojo::WaitKind::Answer => {
                    // `0x08040182` clears the flag; the character pick replaces the page
                    self.friend.answered = false;
                    if let LinkPage::Saves(l) = &mut self.page {
                        l.figures_gone(w);
                    }
                    let balanced = match &self.page {
                        LinkPage::Setup(s) => s.balanced,
                        _ => self.friend.balanced.unwrap_or(true),
                    };
                    self.page = LinkPage::Chars(chars::Chars::new(w, t, balanced, self.slot));
                    self.figs_shown = None;
                    self.actions.push(LinkAction::Chars);
                }
                dojo::WaitKind::Pick => {
                    self.actions.push(LinkAction::Fight);
                    if let LinkPage::Chars(c) = &mut self.page {
                        c.fight = true;
                        c.c.drawn = false;
                    }
                }
            },
        }
        Action::None
    }

    fn step_saves(&mut self, w: &mut World) -> Action {
        let (_, act) = super::kit::run_step(
            self,
            w,
            |p| p.page_container(),
            |p, w, _src, code| {
                let LinkPage::Saves(l) = &mut p.page else { return (false, None) };
                match code {
                    0 => {
                        w.sfx(SFX_PICK, 64);
                        let sel = l.sel.max(0) as usize;
                        if w.saves.slot(sel).is_some() {
                            p.actions.push(LinkAction::Save(sel));
                            p.slot = sel;
                            if !p.trade {
                                dojo::save_picked(p, w);
                            }
                        }
                        (true, None)
                    }
                    1 => {
                        w.sfx(super::SFX_MENU, 64);
                        p.actions.push(LinkAction::Back);
                        (true, None)
                    }
                    2 | 3 => {
                        let moved = if code == 2 { l.sel != 0 } else { l.sel < 2 };
                        if moved {
                            w.sfx(super::SFX_CURSOR, 64);
                        }
                        // (0x08005CBA / 0x08005D0A, the list's base events: the figures are made again)
                        l.figs_key = None;
                        l.event(code);
                        // (the A prompt greys on an empty game: nothing to pick)
                        let ex = w.saves.slot(l.sel.max(0) as usize).is_some();
                        if let Some(Widget::Button(b)) = l.c.kids.get_mut(0) {
                            b.enabled = ex;
                        }
                        (true, None)
                    }
                    _ => (false, None),
                }
            },
        );
        act.unwrap_or(Action::None)
    }

    fn step_setup(&mut self, w: &mut World) -> Action {
        if let LinkPage::Setup(s) = &mut self.page {
            if std::mem::take(&mut s.unpress) {
                s.c.focus = None;
                s.c.drawn = false;
            }
        }
        let (_, act) = super::kit::run_step(
            self,
            w,
            |p| p.page_container(),
            |p, w, _src, code| {
                let LinkPage::Setup(s) = &mut p.page else { return (false, None) };
                match code {
                    0 => {
                        if s.choose() {
                            // START FIGHT: the wait box (the page's picture under it keeps the A prompt pressed)
                            s.c.focus = Some(s.a_btn);
                            p.actions.push(LinkAction::StartFight { balanced: s.balanced, rounds: s.rounds() });
                            let t = std::sync::Arc::clone(&w.data);
                            p.wait = Some(dojo::Wait::new(&t.menu.as_ref().expect("menu content").text, dojo::WaitKind::Answer));
                        }
                        (true, None)
                    }
                    1 => {
                        // (no sound) a new Select Saved Game
                        p.actions.push(LinkAction::SetupBack);
                        let mut l = Link::saves(w, p.trade);
                        l.host = p.host;
                        l.friend = p.friend.clone();
                        l.slot = p.slot;
                        l.walk_from_idle = true;
                        *p = l;
                        (true, None)
                    }
                    _ => (false, None),
                }
            },
        );
        act.unwrap_or(Action::None)
    }

    fn step_chars(&mut self, w: &mut World) -> Action {
        if let LinkPage::Chars(c) = &mut self.page {
            // the fade to the fight (a nested mode: no input)
            if c.fight {
                c.fade = (c.fade + 16).min(256);
                c.c.drawn = false;
                return Action::None;
            }
            c.after_cancel();
            c.tick_objects(w);
        }
        let (_, act) = super::kit::run_step(
            self,
            w,
            |p| p.page_container(),
            |p, w, _src, code| {
                let LinkPage::Chars(c) = &mut p.page else { return (false, None) };
                match c.event(w, code) {
                    Some(chars::CharsEv::Pick(i)) => {
                        p.actions.push(LinkAction::Pick(i));
                        let t = std::sync::Arc::clone(&w.data);
                        p.wait = Some(dojo::Wait::new(&t.menu.as_ref().expect("menu content").text, dojo::WaitKind::Pick));
                        (true, None)
                    }
                    Some(chars::CharsEv::Back) => {
                        p.actions.push(LinkAction::CharsBack);
                        c.figures_gone(w);
                        if p.host {
                            let t = std::sync::Arc::clone(&w.data);
                            p.page = LinkPage::Setup(dojo::Setup::new(&t.menu.as_ref().expect("menu content").text));
                        } else {
                            let mut l = Link::saves(w, p.trade);
                            l.host = p.host;
                            l.friend = p.friend.clone();
                            l.walk_from_idle = true;
                            *p = l;
                        }
                        (true, None)
                    }
                    Some(chars::CharsEv::Moved) => (true, None),
                    None => (false, None),
                }
            },
        );
        act.unwrap_or(Action::None)
    }
}

impl Page for Link {
    fn container(&mut self) -> &mut Container {
        if self.wait.is_some() {
            return &mut self.wait.as_mut().expect("wait").c;
        }
        self.page_container()
    }
    fn container_ref(&self) -> &Container {
        if let Some(wt) = &self.wait {
            return &wt.c;
        }
        match &self.page {
            LinkPage::Saves(l) => &l.c,
            LinkPage::Setup(s) => &s.c,
            LinkPage::Chars(c) => &c.c,
            LinkPage::Trade(t) => t.container_ref(),
        }
    }
    fn update(&mut self, w: &World) {
        // (a nested wait box: the page under it is not updated - its figures stand still)
        if self.wait.is_some() {
            return;
        }
        if std::mem::take(&mut self.stub) {
            return;
        }
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.tick_figs();
                self.figs_shown = Some(l.figs.clone());
                // (a list made by a Dojo page's B: its walker shows its first cel once, then runs on the tick its idle
                // enter drew - the object's cel field is computed by the first tick, under the controller's rule: a cel
                // past the walk's last starts it over; menu_parity link_setup_back, link_chars_ingame)
                if std::mem::take(&mut self.walk_jump) {
                    for f in l.figs.iter_mut().filter(|f| !f.idle) {
                        let n = w
                            .data
                            .sprite(f.sprite)
                            .and_then(|sp| sp.anim_raw(super::save::FIG_ANIM))
                            .map(|a| a.steps(super::save::FIG_DIR).len() as i32)
                            .unwrap_or(1);
                        f.t = if (f.idle_t + 1) >> 3 >= n { -1 } else { f.idle_t };
                    }
                }
            }
            LinkPage::Setup(_) | LinkPage::Trade(_) => {}
            LinkPage::Chars(c) => c.update(w),
        }
    }
    fn before_draw(&mut self, w: &World) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.prepare();
                l.build_figs(w);
            }
            LinkPage::Setup(_) => {}
            LinkPage::Chars(c) => {
                if self.wait.is_none() {
                    c.before_draw(w)
                }
            }
            LinkPage::Trade(t) => t.before_draw(),
        }
    }
    fn after_draw(&mut self, _w: &World) {
        if let LinkPage::Trade(t) = &mut self.page {
            t.after_draw();
        }
    }
    fn built_imported(&mut self) {
        if let LinkPage::Saves(l) = &mut self.page {
            l.rebuilt = false;
            l.rng_owed = 0;
        }
    }
    fn built(&mut self, w: &mut World) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                let fresh = l.rebuilt;
                l.built(w);
                if fresh && std::mem::take(&mut self.walk_from_idle) {
                    self.walk_jump = true;
                }
            }
            LinkPage::Setup(_) | LinkPage::Trade(_) => {}
            LinkPage::Chars(c) => c.built(w),
        }
    }
    fn objs(&self, w: &World) -> Vec<wf_view::MenuObj> {
        match &self.page {
            LinkPage::Saves(l) => match &self.figs_shown {
                Some(f) if *f != l.figs => {
                    let mut g = l.clone();
                    g.figs = f.clone();
                    g.fig_objs(w)
                }
                _ => l.fig_objs(w),
            },
            LinkPage::Setup(_) | LinkPage::Trade(_) => Vec::new(),
            LinkPage::Chars(c) => c.objs(w),
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        if self.wait.is_some() {
            return self.step_wait(w);
        }
        match &self.page {
            LinkPage::Saves(_) => self.step_saves(w),
            LinkPage::Setup(_) => self.step_setup(w),
            LinkPage::Chars(_) => self.step_chars(w),
            LinkPage::Trade(_) => trade::step(self, w),
        }
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        if let LinkPage::Trade(t) = &self.page {
            return t.view(d);
        }
        let t = &d.text;
        let mut cv = super::text::Canvas::new();
        match &self.page {
            LinkPage::Saves(l) => l.draw(t, &mut cv),
            LinkPage::Setup(s) => s.draw(t, &mut cv),
            LinkPage::Chars(c) => c.draw(t, &mut cv),
            LinkPage::Trade(_) => {}
        }
        let mut v = super::surface_view(cv);
        v.layers[0].priority = 3;
        if let LinkPage::Chars(c) = &self.page {
            if c.fade > 0 {
                v.fade = Some(wf_view::PaletteFade { level: c.fade, target: 0 });
            }
        }
        if let Some(wt) = &self.wait {
            v.layers.push(wt.layer(t));
        } else if let LinkPage::Chars(c) = &self.page {
            if let Some(l) = c.frame_layer() {
                v.layers.push(l);
            }
        }
        v
    }
    fn redraw_cost(&self) -> u8 {
        super::title::SELECT_FRAMES
    }
}

/// the link's first page as the title's Multiplayer handler builds it (`0x0801E84A`: the box ended with the link up,
/// Select Saved Game made in that iteration - its figures with it: menu_parity link_connect)
pub fn open_saves(w: &mut World, trade: bool) {
    let mut l = Link::saves(w, trade);
    if let LinkPage::Saves(g) = &mut l.page {
        g.prepare();
        g.build_figs(w);
        // (its cursor counts from that iteration too)
        for k in g.c.kids.iter_mut() {
            if let Widget::Cursor(c) = k {
                c.t = 1;
            }
        }
    }
    open(w, l);
}

/// show a page (the app's link flow, over the title): it replaces the menu on show
pub fn open(w: &mut World, l: Link) {
    super::open(w, super::Screen::Link(l), super::title::BOOT, false);
}

/// a harness opening a page from JSON (`menu::open_from_json`): `{"kind": "link_saves", "trade": false, "sel": 0}` -
/// the page on show (no transition: the capture starts on it)
pub fn open_json(w: &World, v: &serde_json::Value) -> Option<(super::Screen, super::Transition, bool)> {
    let trade = v["trade"].as_bool().unwrap_or(false);
    let l = match v["kind"].as_str()? {
        "link_saves" => {
            let mut l = Link::saves(w, trade);
            if let (LinkPage::Saves(g), Some(s)) = (&mut l.page, v["sel"].as_i64()) {
                g.sel = s as i32;
            }
            // (the page has been running: its figures are made and alive, the walker at the game's tick)
            if let LinkPage::Saves(g) = &mut l.page {
                g.prepare();
                g.build_figs(w);
                g.rebuilt = false;
                g.rng_owed = 0;
                let ts: Vec<i64> = v["fig_t"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
                for (k, f) in g.figs.iter_mut().enumerate() {
                    f.live = true;
                    if let Some(&t) = ts.get(k) {
                        f.t = t as i32 + FIG_T_OFF;
                        f.idle_t = t as i32 + FIG_T_OFF;
                    }
                }
            }
            if let (LinkPage::Saves(g), Some(ct)) = (&mut l.page, v["cursor_t"].as_u64()) {
                for k in g.c.kids.iter_mut() {
                    if let Widget::Cursor(c) = k {
                        c.t = ct as u32;
                    }
                }
            }
            l
        }
        // the Dojo's setup page: `{"kind": "link_setup", "sel": 9, "balanced": true, "rounds": 3, "cursor_t": n}`
        "link_setup" => {
            let t = &w.data.menu.as_ref()?.text;
            let mut s = dojo::Setup::new(t);
            s.set(
                v["sel"].as_i64().unwrap_or(9) as i32,
                v["balanced"].as_bool().unwrap_or(true),
                v["rounds"].as_i64().unwrap_or(3) as i32,
            );
            if let Some(ct) = v["cursor_t"].as_u64() {
                for k in s.c.kids.iter_mut() {
                    if let Widget::Cursor(c) = k {
                        c.t = ct as u32;
                    }
                }
            }
            let mut l = Link::saves(w, trade);
            l.page = LinkPage::Setup(s);
            l
        }
        // the character pick: `{"kind": "link_chars", "balanced": true, "sel": 0, "figs": [[walk, frame, tick] |
        // null; 5]}` (the figures as the game has them)
        "link_chars" => {
            let t = &w.data.menu.as_ref()?.text;
            let mut c = chars::Chars::new(w, t, v["balanced"].as_bool().unwrap_or(true), v["slot"].as_u64().unwrap_or(0) as usize);
            let figs: Vec<Option<(bool, i32, i32)>> = v["figs"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|f| f.as_array().map(|f| (f[0].as_i64() == Some(1), f[1].as_i64().unwrap_or(0) as i32, f[2].as_i64().unwrap_or(0) as i32)))
                        .collect()
                })
                .unwrap_or_default();
            c.import(w, v["sel"].as_i64().unwrap_or(0) as i32, &figs);
            let mut l = Link::saves(w, trade);
            l.page = LinkPage::Chars(c);
            l.friend.balanced = v["balanced"].as_bool();
            l
        }
        // the Trading Post's picker as it starts (built from the running game's data); `cursor_t` its blink
        "link_trade" => {
            let mut t = trade::Trade::new(w);
            t.pick.prepare();
            t.pick.arrows();
            if let Some(ct) = v["cursor_t"].as_u64() {
                for k in t.pick.c.kids.iter_mut() {
                    if let Widget::Cursor(c) = k {
                        c.t = ct as u32;
                    }
                }
            }
            Link::new(LinkPage::Trade(Box::new(t)), true)
        }
        _ => return None,
    };
    let mut l = l;
    l.host = v["host"].as_bool().unwrap_or(false);
    Some((super::Screen::Link(l), super::title::BOOT, false))
}

/// the Dojo's character pick on show: where the link is after a fight (link recording dojo_px3: the fight's fade-out,
/// then "Choose your character!" again, the same Character Mode) - the app's link flow
pub fn open_chars(w: &mut World, balanced: bool, slot: usize, host: bool) {
    let Some(md) = w.data.menu.clone() else { return };
    let c = chars::Chars::new(w, &md.text, balanced, slot);
    let mut l = Link::new(LinkPage::Chars(c), false);
    l.friend.balanced = Some(balanced);
    l.slot = slot;
    l.host = host;
    open(w, l);
}

/// the Trading Post's picker on show (the app's link flow, after Select Saved Game loaded the save into the story)
pub fn open_trade(w: &mut World) {
    let t = trade::Trade::new(w);
    open(w, Link::new(LinkPage::Trade(Box::new(t)), true));
}

/// a harness giving the page what the friend's side did (menu_parity's link scenarios: a trace row's `link`,
/// `{"offer": [[amount, kind, id], ..]}` / `{"answer": 1 accept | 2 decline}`) - as the app sets [`Friend`]
pub fn harness(w: &mut World, v: &serde_json::Value) {
    let Some(super::Screen::Link(l)) = w.menu.stack.last_mut() else { return };
    if let Some(o) = v["offer"].as_array() {
        let e = |x: &serde_json::Value| trade::TradeEntry { amount: x[0].as_u64().unwrap_or(0) as u32, kind: x[1].as_u64().unwrap_or(0) as u8, id: x[2].as_u64().unwrap_or(0) as u8 };
        l.friend.offer = Some(o.iter().map(e).collect());
    }
    if let Some(a) = v["answer"].as_i64() {
        l.friend.answer = Some(a == 1);
    }
}
