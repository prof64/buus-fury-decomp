//! The link-cable pages (title -> Multiplayer -> Dojo / Trading Post, after "Waiting for a friend..."): the game's
//! own screens, drawn and stepped as its modes are (re/buu/menu/NOTES_screens.md "Multiplayer"; mode tables
//! `0x08049B2C` Select Game, `0x08049450` .. Dojo, `0x0804A60C` .. Trading Post: trade.rs). One [`Link`] screen holds the page
//! on show; the app's link flow (wf_app::link) steps it with the local keys, takes its [`LinkAction`]s and gives it
//! what the friend's side did ([`Friend`]). Gated pixel for pixel against two linked emulators
//! (re/buu/menu/linkcap.py, scenarios_link.py -> data/menu/link_*.json, tests/menu_parity.rs).
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
    /// the Trading Post's pages (trade.rs: the picker, "How many to trade?", the proposal, the boxes)
    Trade(Box<trade::Trade>),
}

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
        Link { page: LinkPage::Saves(l), trade, actions: Vec::new(), friend: Friend::default(), figs_shown: None }
    }
}

impl Page for Link {
    fn container(&mut self) -> &mut Container {
        match &mut self.page {
            LinkPage::Saves(l) => &mut l.c,
            LinkPage::Trade(t) => t.container(),
        }
    }
    fn container_ref(&self) -> &Container {
        match &self.page {
            LinkPage::Saves(l) => &l.c,
            LinkPage::Trade(t) => t.container_ref(),
        }
    }
    fn update(&mut self, _w: &World) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.tick_figs();
                self.figs_shown = Some(l.figs.clone());
            }
            LinkPage::Trade(_) => {}
        }
    }
    fn before_draw(&mut self, w: &World) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.prepare();
                l.build_figs(w);
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
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.rebuilt = false;
                l.rng_owed = 0;
            }
            LinkPage::Trade(_) => {}
        }
    }
    fn built(&mut self, w: &mut World) {
        match &mut self.page {
            LinkPage::Saves(l) => l.built(w),
            LinkPage::Trade(_) => {}
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
            LinkPage::Trade(_) => Vec::new(),
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        match &mut self.page {
            LinkPage::Saves(_) => {
                let (consumed, act) = super::kit::run_step(
                    self,
                    w,
                    |p| p.container(),
                    |p, w, _src, code| {
                        let LinkPage::Saves(l) = &mut p.page else { return (false, None) };
                        match code {
                            0 => {
                                w.sfx(SFX_PICK, 64);
                                let sel = l.sel.max(0) as usize;
                                if w.saves.slot(sel).is_some() {
                                    p.actions.push(LinkAction::Save(sel));
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
                if let Some(a) = act {
                    return a;
                }
                let _ = consumed;
                Action::None
            }
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
            LinkPage::Trade(_) => {}
        }
        let mut v = super::surface_view(cv);
        v.layers[0].priority = 3;
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
            Link { page: LinkPage::Trade(Box::new(t)), trade: true, actions: Vec::new(), friend: Friend::default(), figs_shown: None }
        }
        _ => return None,
    };
    Some((super::Screen::Link(l), super::title::BOOT, false))
}

/// the Trading Post's picker on show (the app's link flow, after Select Saved Game loaded the save into the story)
pub fn open_trade(w: &mut World) {
    let t = trade::Trade::new(w);
    open(w, Link { page: LinkPage::Trade(Box::new(t)), trade: true, actions: Vec::new(), friend: Friend::default(), figs_shown: None });
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
