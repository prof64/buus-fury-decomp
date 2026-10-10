//! The link-cable pages (title -> Multiplayer -> Dojo / Trading Post, after "Waiting for a friend..."): the game's
//! own screens, drawn and stepped as its modes are (re/buu/menu/NOTES_screens.md "Multiplayer"; mode tables
//! `0x08049B2C` Select Game, `0x08049B5C` .. Dojo, `0x08049450` .. Trading Post). One [`Link`] screen holds the page
//! on show; the app's link flow (wf_app::link) steps it with the local keys, takes its [`LinkAction`]s and gives it
//! what the friend's side did ([`Friend`]). Gated pixel for pixel against two linked emulators
//! (re/buu/menu/linkcap.py, scenarios_link.py -> data/menu/link_*.json, tests/menu_parity.rs).

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
}

/// what the friend's side has done, as far as the pages show it
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Friend {
    /// the friend has picked its save
    pub saved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinkPage {
    /// Select Saved Game (vt `0x08049B2C`: the game list `0x08005460(0x5E "Select Saved Game", 0xD, arrows)`)
    Saves(GameList),
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
        Link { page: LinkPage::Saves(l), trade, actions: Vec::new(), friend: Friend::default(), figs_shown: None }
    }
}

impl Page for Link {
    fn container(&mut self) -> &mut Container {
        match &mut self.page {
            LinkPage::Saves(l) => &mut l.c,
        }
    }
    fn container_ref(&self) -> &Container {
        match &self.page {
            LinkPage::Saves(l) => &l.c,
        }
    }
    fn update(&mut self, _w: &World) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.tick_figs();
                self.figs_shown = Some(l.figs.clone());
            }
        }
    }
    fn before_draw(&mut self, w: &World) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.prepare();
                l.build_figs(w);
            }
        }
    }
    fn built_imported(&mut self) {
        match &mut self.page {
            LinkPage::Saves(l) => {
                l.rebuilt = false;
                l.rng_owed = 0;
            }
        }
    }
    fn built(&mut self, w: &mut World) {
        match &mut self.page {
            LinkPage::Saves(l) => l.built(w),
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
        }
    }
    fn step(&mut self, w: &mut World) -> Action {
        match &mut self.page {
            LinkPage::Saves(_) => {
                let (consumed, act) = super::kit::run_step(
                    self,
                    w,
                    |p| match &mut p.page {
                        LinkPage::Saves(l) => &mut l.c,
                    },
                    |p, w, _src, code| {
                        let LinkPage::Saves(l) = &mut p.page;
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
        }
    }
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let t = &d.text;
        let mut cv = super::text::Canvas::new();
        match &self.page {
            LinkPage::Saves(l) => l.draw(t, &mut cv),
        }
        let mut v = super::surface_view(cv);
        v.layers[0].priority = 3;
        v
    }
    fn redraw_cost(&self) -> u8 {
        super::title::SELECT_FRAMES
    }
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
        _ => return None,
    };
    Some((super::Screen::Link(l), super::title::BOOT, false))
}
