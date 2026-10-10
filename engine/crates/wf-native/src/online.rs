//! The native shell's online play: the in-game lobby menu (`wf_app::netmenu`, F12 / `--online`, or straight into a
//! room with `--net-host` / `--net-join` / `--net-spectate` / `--net-rejoin`), the non-blocking lobby on a WebRTC
//! transport, and swapping the app's simulation for the match (and back to a local battle when leaving).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use wf_app::battle::BattleConfig;
use wf_app::control::Control;
use wf_app::netmenu::{MenuAction, OnlineMenu, OnlinePrefs, Screen};
use wf_app::App;
use wf_net::cli::{NetArgs, NetMode};
use wf_net::netsim::{current_status, NetLobby, NetSim};
use wf_net::wf_core::content::GameData;

pub struct Online {
    pub menu: OnlineMenu,
    pub lobby: Option<NetLobby>,
    /// the command line's net options (signal, key, ICE, record / report dirs, ticket path)
    pub args: NetArgs,
    data: Option<Arc<GameData>>,
    prefs_path: PathBuf,
    ticket_path: PathBuf,
    /// the local battle to go back to
    pub local: BattleConfig,
    pub in_match: bool,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Online {
    pub fn new(mut args: NetArgs, data_dir: &Path, local: BattleConfig, signal_given: bool) -> Online {
        let prefs_path = data_dir.join("online.json");
        let ticket_path = data_dir.join("net-rejoin.json");
        args.record_dir.get_or_insert_with(|| data_dir.join("replays").display().to_string());
        args.ticket_path.get_or_insert_with(|| ticket_path.display().to_string());
        let mut prefs: OnlinePrefs = std::fs::read_to_string(&prefs_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        if signal_given || prefs.signal.is_empty() {
            prefs.signal = args.signal.clone();
        }
        if let Some(f) = local.players.first() {
            if prefs.fighter.is_empty() {
                prefs.fighter = f.fighter.clone();
                prefs.form = f.form;
            }
        }
        let mut menu = OnlineMenu::default();
        menu.prefs = prefs;
        menu.has_ticket = ticket_path.exists();
        Online { menu, lobby: None, args, data: None, prefs_path, ticket_path, local, in_match: false }
    }

    fn save_prefs(&self) {
        let _ = std::fs::write(&self.prefs_path, serde_json::to_string_pretty(&self.menu.prefs).unwrap_or_default());
    }

    fn data(&mut self, app: &App) -> Result<Arc<GameData>, String> {
        if self.data.is_none() {
            self.data = Some(wf_app::battle::game_data(&app.packs)?);
        }
        Ok(self.data.clone().unwrap())
    }

    /// F12 / Esc in an online match.
    pub fn toggle(&mut self, app: &mut App) {
        if self.menu.open {
            self.menu.open = false;
        } else {
            self.menu.has_ticket = self.ticket_path.exists();
            self.menu.show(if self.in_match { Screen::InMatch } else if self.lobby.is_some() { Screen::Room } else { Screen::Main });
            if self.in_match {
                self.menu.message = "The match keeps running while this menu is open.".into();
            }
        }
        self.sync_pause(app);
    }

    fn sync_pause(&self, app: &mut App) {
        // a local battle pauses behind the menu; an online match cannot (Sim::paused_input keeps it running)
        app.paused = self.menu.open;
    }

    /// The command line asked for a room (`--net-host` etc.): open it right away.
    pub fn start_cli(&mut self, app: &mut App) {
        let args = self.args.clone();
        let hosting = args.mode == NetMode::Host;
        let spectating = matches!(args.mode, NetMode::Spectate(_));
        self.open_room(app, args, hosting, spectating, true);
    }

    /// The battle a host proposes / the fighter a joiner brings, from the menu's choices.
    fn battle(&self) -> BattleConfig {
        let p = &self.menu.prefs;
        let mut b = wf_app::battle::config(p.arena(), &p.enemy, p.count.max(1), &p.fighter, Some(p.form), None, 1, (now_unix() % 100_000) as u32 + 1);
        b.xp = false;
        b
    }

    fn open_room(&mut self, app: &mut App, args: NetArgs, hosting: bool, spectating: bool, from_cli: bool) {
        let data = match self.data(app) {
            Ok(d) => d,
            Err(e) => return self.menu.say(e),
        };
        let battle = if from_cli { self.local.clone() } else { self.battle() };
        let clock = wf_net::cli::clock();
        match wf_net::cli::open_lobby(&args, &data, &battle, clock) {
            Ok((mut lobby, invite)) => {
                if !from_cli {
                    // the menu's host starts with the Start row; its joiners say when they are ready
                    lobby.lobby.settings.auto_start = false;
                    lobby.lobby.settings.ready = hosting;
                }
                eprintln!("wf-net: room {invite}");
                self.lobby = Some(lobby);
                self.menu.enter_room(hosting, spectating);
                self.menu.ready = lobby_ready(self.lobby.as_ref());
                self.sync_pause(app);
            }
            Err(e) => self.menu.say(e),
        }
    }

    fn menu_args(&self, mode: NetMode) -> NetArgs {
        let p = &self.menu.prefs;
        NetArgs {
            mode,
            signal: p.signal.clone(),
            players: p.players as usize,
            pvp: p.pvp(),
            pvp_enemies: p.pvp_enemies(),
            delay: p.delay,
            name: p.name.clone(),
            pause: p.pause.clone(),
            ..self.args.clone()
        }
    }

    fn act(&mut self, a: MenuAction, app: &mut App, ctl: &mut Control) {
        match a {
            MenuAction::None => {}
            MenuAction::Saved => self.save_prefs(),
            MenuAction::Close => {}
            MenuAction::Host => {
                self.save_prefs();
                let args = self.menu_args(NetMode::Host);
                self.open_room(app, args, true, false, false);
            }
            MenuAction::Join { code, spectate } => {
                self.save_prefs();
                let args = self.menu_args(if spectate { NetMode::Spectate(code) } else { NetMode::Join(code) });
                self.open_room(app, args, false, spectate, false);
            }
            MenuAction::Rejoin => {
                let mut args = self.menu_args(NetMode::Rejoin(String::new()));
                args.ticket_path = Some(self.ticket_path.display().to_string());
                self.open_room(app, args, false, false, true);
            }
            MenuAction::SetReady(r) => {
                if let Some(l) = &mut self.lobby {
                    l.lobby.set_ready(r);
                }
            }
            MenuAction::SetFighter { fighter, form } => {
                self.save_prefs();
                if let Some(l) = &mut self.lobby {
                    let mut f = l.lobby.settings.fighter.clone();
                    f.fighter = fighter;
                    f.form = form;
                    l.lobby.set_fighter(f);
                }
            }
            MenuAction::Start => {
                if let Some(l) = &mut self.lobby {
                    l.lobby.request_start();
                }
            }
            MenuAction::Leave => {
                self.lobby = None;
                if self.in_match {
                    self.leave_match(app, ctl);
                }
                wf_net::netsim::clear_current();
            }
        }
        self.sync_pause(app);
    }

    /// Leave the room / match (the system menu's "Leave match" / quitting): the peers are told when the match's
    /// session goes (`NetSim`'s drop: its "bye"), as the lobby menu's Leave does.
    pub fn leave(&mut self, app: &mut App, ctl: &mut Control) {
        self.lobby = None;
        if self.in_match {
            self.leave_match(app, ctl);
        }
        wf_net::netsim::clear_current();
        self.menu.open = false;
    }

    fn leave_match(&mut self, app: &mut App, ctl: &mut Control) {
        self.in_match = false;
        match wf_app::battle::session(&app.packs, self.local.clone()) {
            Ok(s) => app.set_sim(Box::new(wf_app::battle::CoreSim(s))),
            Err(e) => eprintln!("back to the local battle: {e}"),
        }
        *ctl = Control::new(app, ctl.hotkeys.clone(), now_unix());
    }

    /// A key while the menu is open (true = consumed).
    pub fn key(&mut self, code: &str, app: &mut App, ctl: &mut Control) -> bool {
        if !self.menu.open {
            return false;
        }
        let a = self.menu.key(code);
        self.act(a, app, ctl);
        true
    }

    pub fn text(&mut self, s: &str) {
        if self.menu.open && self.menu.editing() {
            self.menu.text(s);
        }
    }

    /// Every frame: pump the lobby; when the match starts, it becomes the app's simulation.
    pub fn frame(&mut self, app: &mut App, ctl: &mut Control) {
        let Some(l) = &mut self.lobby else { return };
        if !l.poll() {
            self.menu.ready = lobby_ready(self.lobby.as_ref());
            return;
        }
        let l = self.lobby.take().unwrap();
        let data = match self.data(app) {
            Ok(d) => d,
            Err(e) => return self.menu.say(e),
        };
        match wf_net::cli::finish_lobby(&self.args, l, data) {
            Ok(mut sim) => {
                sim.content = wf_app::battle::content_id(&app.packs);
                let slot = sim.local_slot();
                eprintln!("wf-net: match {} as {}", if sim.rejoined() { "rejoined" } else { "started" }, slot.map(|k| format!("P{}", k + 1)).unwrap_or("spectator".into()));
                ctl.finalize(app);
                app.set_sim(Box::new(sim) as Box<dyn wf_view::Sim>);
                *ctl = Control::new(app, ctl.hotkeys.clone(), now_unix());
                self.in_match = true;
                self.menu.open = false;
                self.menu.has_ticket = self.ticket_path.exists();
                self.sync_pause(app);
            }
            Err(e) => self.menu.say(e),
        }
    }

    /// The online match ended / the app's sim is not an online one anymore.
    pub fn check_match(&mut self, app: &App) {
        if self.in_match && app.sim.as_any().and_then(|a| a.downcast_ref::<NetSim>()).is_none() {
            self.in_match = false;
        }
    }

    pub fn draw(&mut self, app: &mut App, fb_w: usize, fb_h: usize) {
        if !self.menu.open {
            return;
        }
        let st = current_status().map(|s| s.borrow().clone());
        let App { packs, renderer, .. } = app;
        self.menu.draw(packs, &mut renderer.fb, fb_w, fb_h, st.as_ref());
    }
}

fn lobby_ready(l: Option<&NetLobby>) -> bool {
    l.is_some_and(|l| l.lobby.settings.ready)
}

/// Headless screenshots of the lobby menu (`--show-online main|host|room`): the room with a sample roster.
pub fn preview(which: &str, packs: &wf_app::wf_content::Packs, fb: &mut [u32], w: usize, h: usize) {
    use wf_net::lobby::LobbyEntry;
    let mut m = OnlineMenu::default();
    m.has_ticket = true;
    let mut st = wf_net::netsim::NetStatus::default();
    match which {
        "host" => m.show(Screen::Host),
        "room" => {
            m.enter_room(true, false);
            st.phase = "lobby".into();
            st.invite = "KQZTA-7F3K2Q".into();
            st.players_present = 3;
            st.want_players = 4;
            let e = |name: &str, role: &str, f: &str, form: i32, ready: bool, me: bool, seat: Option<usize>| LobbyEntry { id: name.into(), name: name.into(), role: role.into(), fighter: f.into(), form, ready, me, seat };
            st.lobby = vec![e("Prof", "host", "goku", 2, true, true, Some(0)), e("Ozzy", "player", "vegeta", 26, true, false, Some(1)), e("Kai", "player", "gohan", 10, false, false, Some(2)), e("Mo", "spectator", "goku", 2, true, false, None)];
        }
        "code" => {
            m.show(Screen::Code { watch: false });
            m.key("Enter");
            m.text("KQZTA-7F3");
        }
        _ => m.show(Screen::Main),
    }
    m.draw(packs, fb, w, h, Some(&st));
}
