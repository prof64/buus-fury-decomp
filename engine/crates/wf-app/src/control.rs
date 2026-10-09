//! Replays, rewind, save states and training tools around an [`App`] — shared by the native and web shells.
//!
//! Every live session is recorded (`wf_replay::Recorder`: config + inputs + hash checkpoints) and handed to the
//! host to persist ([`Request::SaveReplay`], periodically and when it ends). A [`Timeline`] of periodic
//! snapshots makes any frame reachable: holding the rewind key steps back through it, seeking restores the
//! nearest snapshot and re-simulates (steps cost ~1 µs). Live input after a rewind truncates the recording there
//! (a new branch of the same fight). Playback runs a replay's inputs with pause / frame advance / slow motion /
//! fast forward / seek, verifies every hash checkpoint, and can be taken over at any frame. Save states are a
//! snapshot plus the recording up to it. A ghost is a second simulation running another replay in lockstep,
//! drawn semi-transparent. Overlays (training boxes, AI labels, damage log, input display, status) are drawn
//! over the rendered view and never touch the simulation.
//!
//! The host feeds key events ([`Control::key`]), wall time ([`Control::advance`]) and the current wall clock
//! (`now_unix`, metadata only), renders through [`Control::render`] and services [`Control::take_requests`]
//! (file system on desktop, OPFS / IndexedDB on the web).
//!
//! Two kinds of sim record: the battle (`CoreSim`, header `sim` [`SIM_KIND`], config = the `BattleConfig`) and story
//! mode (`StorySim`, header `sim` [`STORY_KIND`], config = its `StorySim::start`: the title with the slot files, a
//! slot's game or a new game at an area, + the seed). Ghosts and the training overlay are battle-only.

use std::collections::VecDeque;

use wf_core::battle::{BattleConfig, Outcome};
use wf_core::session::Session;
use wf_core::world::{Body, Event, World};
use wf_render::Fit;
use wf_replay::{Branch, Checker, ContentId, Divergence, Header, Recorder, Replay, SaveState, StateHeader, Timeline, TimelineConfig, VerifyReport};
use wf_view::{AudioEvent, RenderView, Sim, SpriteEffect, GBA_FPS};

use crate::battle::{self, CoreSim, SIM_KIND};
use crate::hotkeys::{Action, Hotkeys, Mods};
use crate::overlay::{self, Text};
use crate::story::{StorySim, STORY_KIND};
use crate::App;

/// Playback / game speeds (index 3 = normal).
pub const SPEEDS: [f64; 7] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
const NORMAL: usize = 3;
const SEEK_FRAMES: u64 = 299; // 5 s

/// Things only the host can do (storage, its own UI).
#[derive(Debug)]
pub enum Request {
    /// Create or overwrite recording `id` (`<id>.wfr`); `header` for listings.
    SaveReplay { id: String, bytes: Vec<u8>, header: Header },
    SaveState { slot: u8, bytes: Vec<u8> },
    /// Read slot `slot` and call [`Control::load_state`] (or report it empty).
    LoadState { slot: u8 },
    /// Show the replay browser.
    OpenReplays,
    /// The battle's end screen chose "Change fighter": back to the launcher's Battle page (the recording is saved
    /// first, [`Control::finalize`]).
    BattleSelect,
    /// A replay's menu chose "Exit" / "Back": back to the launcher ([`Control::take_exit_replay`]).
    ExitReplay,
}

/// A replay's own menu (playback: the player's buttons drive nothing else): Start / B / Esc during playback pause it
/// with "Resume" / "Exit to launcher"; at the replay's end it opens by itself with "Watch again" / "Back". UP / DOWN
/// pick, A / START confirm, B is the way back (resume / exit). Playtest: "I watched a replay and spammed, nothing
/// happened, I could not get out once it was over" - the end only said "B takes over, Home restarts" in a toast.
#[derive(Clone, Debug)]
struct PlayMenu {
    finished: bool,
    sel: u8,
    /// host frames since it opened (the buttons do nothing for the first few: a press that opened it, mashing)
    age: u32,
    /// host frames since player 1 last held a button: a pick needs a short pause before its press, so mashing
    /// leaves the menu up (in sight) instead of flipping it shut / open or picking blindly
    quiet: u32,
}

/// host frames a replay menu ignores the buttons once open
const PLAY_MENU_LOCK: u32 = 12;
/// host frames with no button held before a replay menu takes a pick
const PLAY_MENU_QUIET: u32 = 8;

/// The battle's end (live play only): some frames after the outcome the game's "You Win!!" / "You Lose!!" (the link
/// Dojo's strings 111 / 112, its big title font) over the arena and its choice box "Retry" / "Change fighter" (the
/// dialogue box with the choice cursor). The world runs on without the player's buttons meanwhile (recorded as
/// such); UP / DOWN pick, A / START confirm. Retry rebuilds the same battle (a new recording); Change fighter asks
/// the host for the launcher's Battle page ([`Request::BattleSelect`]).
#[derive(Clone, Debug)]
struct EndScreen {
    outcome: Outcome,
    /// the frame the outcome came
    at: u64,
    sel: u8,
    /// player 1's buttons last step (edges)
    prev: u16,
    /// steps since player 1 last held a button (a confirm needs a pause before it: not the fight's mashing)
    quiet: u32,
}

/// frames between the outcome and the end screen (the last enemy's death / the KO's fall play out)
const END_DELAY: u64 = 90;
/// frames the end screen ignores the buttons once up (a fight's button mashing does not pick a choice)
const END_LOCK: u64 = 60;
/// a confirm counts only after this many frames with no button held (mashing A through the end picks nothing; a
/// deliberate press after a short pause does)
const END_QUIET: u32 = 12;
/// the choices of the end screen
const END_CHOICES: &str = "- Retry\n- Change fighter";

struct Playback {
    id: String,
    replay: Replay,
    checker: Checker,
}

/// Another replay simulated in lockstep with the main one and drawn over it.
pub struct Ghost {
    pub id: String,
    sim: CoreSim,
    replay: Replay,
    timeline: Timeline,
}

impl Ghost {
    fn sync(&mut self, frame: u64) {
        if self.sim.frame() != frame {
            self.timeline.seek(&mut self.sim, &self.replay.inputs, frame);
        }
    }
}

pub struct Control {
    /// false = the sim cannot snapshot (demo world): everything here is inert.
    pub enabled: bool,
    pub content: ContentId,
    pub hotkeys: Hotkeys,
    /// Wall clock (Unix seconds) for recording metadata; set by the host (wasm has no clock).
    pub now_unix: u64,
    rec: Option<Recorder>,
    /// Id of the live recording (file stem).
    pub rec_id: String,
    play: Option<Playback>,
    pub timeline: Timeline,
    pub speed: usize,
    pub rewinding: bool,
    pub ff: bool,
    step_req: u32,
    pub show_inputs: bool,
    pub training: bool,
    ghost: Option<Ghost>,
    ghost_off: Option<Ghost>,
    pub damage_log: VecDeque<String>,
    toast: Option<(String, u32, f64)>,
    /// the host started link play (`--link` / `?link=`): the title's "Waiting for a friend..." box is the lobby.
    /// Without it the box can only wait - the overlay then says how to set link play up (playtest 27)
    pub link_configured: bool,
    requests: Vec<Request>,
    acc: f64,
    rew_acc: f64,
    /// Inputs of the last step (input display).
    pub last_inputs: Vec<u16>,
    text: Text,
    since_save: f64,
    saved_frames: u64,
    seq: u32,
    outcome: Option<Outcome>,
    /// Seconds between automatic saves of the live recording.
    pub autosave_secs: f64,
    /// the battle's end screen ([`EndScreen`])
    end: Option<EndScreen>,
    /// show the end screen when a live battle ends (the shells turn it on; off: the fight just runs on, as tools
    /// and tests drive it)
    pub end_screen: bool,
    /// "Change fighter" was chosen (native shell: [`Control::take_battle_select`])
    battle_select: bool,
    /// a replay's menu ([`PlayMenu`]) and player 1's buttons last host frame (its edges)
    play_menu: Option<PlayMenu>,
    play_prev: u16,
    /// a replay's menu chose to leave (native shell: [`Control::take_exit_replay`])
    exit_replay: bool,
    /// the PC system menu (Esc in every mode, [`crate::sysmenu`]) and what the shell has to carry out from it
    pub sys: crate::sysmenu::SysMenu,
    sys_actions: VecDeque<crate::sysmenu::SysAction>,
    sys_was_paused: bool,
    /// the shell's facts for the menu: can it quit itself, its volume, full screen (None: no switch), a key screen
    pub sys_quit_game: bool,
    pub sys_volume: u32,
    pub sys_fullscreen: Option<bool>,
    pub sys_rebind: bool,
    /// "Esc: Menu" shown at a game's start (seconds left)
    hint_t: f64,
    /// story: the save generation last seen and the frame it changed (progress since then is unsaved)
    save_mark: Option<(u32, u64)>,
}

/// seconds the "Esc: Menu" hint stays (the last one fades)
const HINT_SECS: f64 = 5.0;
/// frames of play after the last save that count as progress worth a "quit without saving?" question
const UNSAVED_FRAMES: u64 = 600;

fn fmt_time(frames: u64) -> String {
    let s = frames as f64 / GBA_FPS;
    format!("{}:{:04.1}", (s / 60.0) as u64, s % 60.0)
}

fn ent_name(w: &World, id: u32) -> String {
    match w.find(id).map(|i| &w.ents[i].body) {
        Some(Body::Player(p)) => format!("P{}", p.slot + 1),
        Some(Body::Enemy(e)) => w.data.enemy(e.ty).map(|t| t.name.clone()).unwrap_or_else(|| format!("t{}", e.ty)),
        Some(Body::Shot(s)) => ent_name(w, s.owner),
        Some(_) => format!("#{id}"),
        None => format!("#{id}"),
    }
}

/// A replay file in any supported form (binary `.wfr`, its JSON, the agent server's JSON).
pub fn parse_replay(bytes: &[u8]) -> Result<Replay, String> {
    let mut r = Replay::from_any(bytes).map_err(|e| e.to_string())?;
    if r.header.sim.is_empty() {
        r.header.sim = SIM_KIND.into();
    }
    Ok(r)
}

/// The live sim's recordable start: (header `sim` kind, config, seed, title). None = it cannot be recorded.
fn start_of(app: &App) -> Option<(&'static str, serde_json::Value, u64, String)> {
    if let Some(s) = CoreSim::of(app.sim.as_ref()) {
        return Some((SIM_KIND, serde_json::to_value(&s.config).ok()?, battle::seed(&s.config), battle::title(&s.config)));
    }
    let st = app.sim.as_any()?.downcast_ref::<StorySim>()?;
    let c = st.start.clone()?;
    let seed = c["seed"].as_u64().unwrap_or(0);
    let title = StorySim::start_title(&c);
    Some((STORY_KIND, c, seed, title))
}

fn outcome_name(o: Outcome) -> &'static str {
    match o {
        Outcome::Running => "",
        Outcome::Won => "won",
        Outcome::Lost => "lost",
    }
}

impl Control {
    pub fn new(app: &mut App, mut hotkeys: Hotkeys, now_unix: u64) -> Control {
        hotkeys.migrate();
        let enabled = start_of(app).is_some() && app.sim.snapshot().is_some();
        let mut c = Control {
            enabled,
            content: battle::content_id(&app.packs),
            timeline: Timeline::new(TimelineConfig::with_rewind_seconds(hotkeys.rewind_seconds)),
            hotkeys,
            now_unix,
            rec: None,
            rec_id: String::new(),
            play: None,
            speed: NORMAL,
            rewinding: false,
            ff: false,
            step_req: 0,
            show_inputs: false,
            training: false,
            ghost: None,
            ghost_off: None,
            damage_log: VecDeque::new(),
            toast: None,
            link_configured: false,
            requests: Vec::new(),
            acc: 0.0,
            rew_acc: 0.0,
            last_inputs: vec![0; app.sim.players()],
            text: Text::default(),
            since_save: 0.0,
            saved_frames: 0,
            seq: 0,
            outcome: None,
            autosave_secs: 30.0,
            end: None,
            end_screen: false,
            battle_select: false,
            play_menu: None,
            play_prev: 0,
            exit_replay: false,
            sys: Default::default(),
            sys_actions: VecDeque::new(),
            sys_was_paused: false,
            sys_quit_game: false,
            sys_volume: 100,
            sys_fullscreen: None,
            sys_rebind: false,
            hint_t: HINT_SECS,
            save_mark: None,
        };
        if enabled {
            c.timeline.capture(app.sim.as_ref());
            c.start_recording(app, None);
        }
        c
    }

    fn session(app: &App) -> Option<&Session> {
        CoreSim::of(app.sim.as_ref())
    }

    fn new_id(&mut self) -> String {
        self.seq += 1;
        format!("{:010}-{:02}", self.now_unix, self.seq % 100)
    }

    fn header(&self, app: &App, branch: Option<Branch>) -> Option<Header> {
        let (kind, config, seed, title) = start_of(app)?;
        Some(Header {
            sim: kind.into(),
            content: self.content.clone(),
            config,
            seed,
            players: app.sim.players() as u8,
            hash_interval: wf_replay::DEFAULT_HASH_INTERVAL,
            title,
            created: self.now_unix,
            branch,
            ..Default::default()
        })
    }

    /// New recording of the live sim from its current frame.
    fn start_recording(&mut self, app: &App, branch: Option<Branch>) {
        let Some(h) = self.header(app, branch) else { return };
        let f = app.sim.frame();
        let start = if f == 0 { None } else { app.sim.snapshot() };
        self.rec = Some(Recorder::new(h, f, app.sim.state_hash(), start));
        self.rec_id = self.new_id();
        self.saved_frames = 0;
        self.outcome = Self::session(app).map(|s| s.outcome);
    }

    pub fn is_playback(&self) -> bool {
        self.play.is_some()
    }
    pub fn playback_id(&self) -> Option<&str> {
        self.play.as_ref().map(|p| p.id.as_str())
    }
    pub fn ghost_id(&self) -> Option<&str> {
        self.ghost.as_ref().map(|g| g.id.as_str())
    }
    pub fn divergence(&self) -> Option<Divergence> {
        self.play.as_ref().and_then(|p| p.checker.divergence)
    }

    fn log_end(&self) -> u64 {
        match (&self.play, &self.rec) {
            (Some(p), _) => p.replay.end_frame(),
            (None, Some(r)) => r.replay.end_frame(),
            _ => 0,
        }
    }
    fn log_start(&self) -> u64 {
        match (&self.play, &self.rec) {
            (Some(p), _) => p.replay.start_frame(),
            (None, Some(r)) => r.replay.start_frame(),
            _ => 0,
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), overlay::WHITE, 2.5));
    }
    fn warn(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), overlay::BAD, 5.0));
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    // ------------------------------------------------------------------------------------------- stepping

    /// Exactly one frame (headless tools, tests): live inputs, or the replay's in playback.
    pub fn step(&mut self, app: &mut App, live: &[u16]) -> bool {
        self.enabled && self.step_once(app, live, true)
    }

    /// The end screen chose "Change fighter" (cleared by the call): the host goes back to the launcher's Battle page.
    /// Start opens the system menu: a local battle (no Start menu of its own) outside its end screen and replays
    fn start_opens_menu(&self, app: &App) -> bool {
        CoreSim::of(app.sim.as_ref()).is_some() && self.play.is_none() && self.end.is_none()
    }

    /// Open the system menu (Esc): what this game allows; the game pauses under it (an online match cannot)
    pub fn open_sys(&mut self, app: &mut App) {
        let story = app.sim.as_any().and_then(|a| a.downcast_ref::<crate::story::StorySim>());
        let online = wf_net::netsim::current_status().is_some() || app.sim.as_any().is_some_and(|a| a.is::<crate::link::LinkSim>());
        let unsaved = story.is_some() && self.save_mark.is_some_and(|(_, f)| app.sim.frame().saturating_sub(f) > UNSAVED_FRAMES);
        let ctx = crate::sysmenu::SysCtx {
            restart: CoreSim::of(app.sim.as_ref()).is_some() && self.play.is_none() && self.enabled,
            quit_game: self.sys_quit_game,
            unsaved,
            online,
            classic: matches!(app.mode, wf_render::ViewMode::Gba { .. }),
            volume: self.sys_volume,
            fullscreen: self.sys_fullscreen,
            rebind: self.sys_rebind,
        };
        self.sys.hotkeys = self.hotkeys.clone();
        self.sys.show(ctx);
        self.sys_was_paused = app.paused;
        if !online {
            app.paused = true;
        }
        self.acc = 0.0;
        self.hint_t = 0.0;
        app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_ACCEPT as u8, volume: 64, pan: 0 });
    }

    /// carry out a system menu choice: resume / restart here, the rest is the shell's ([`Control::take_sys_action`])
    fn sys_act(&mut self, app: &mut App, a: crate::sysmenu::SysAction) {
        use crate::sysmenu::SysAction as S;
        app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_CURSOR as u8, volume: 64, pan: 0 });
        if !self.sys.open {
            app.paused = self.sys_was_paused;
            self.acc = 0.0;
        }
        match a {
            S::Resume => {}
            S::Restart => {
                self.retry(app);
                app.paused = false;
            }
            S::ToggleView => {
                app.mode = match app.mode {
                    wf_render::ViewMode::Pc { .. } => wf_render::ViewMode::Gba { scale: 0 },
                    wf_render::ViewMode::Gba { .. } => wf_render::ViewMode::Pc { zoom: 0 },
                };
                self.sys_actions.push_back(a);
            }
            S::Volume(v) => {
                self.sys_volume = v;
                self.sys_actions.push_back(a);
            }
            S::ToggleFullscreen => {
                self.sys_fullscreen = self.sys_fullscreen.map(|f| !f);
                self.sys_actions.push_back(a);
            }
            _ => self.sys_actions.push_back(a),
        }
    }

    /// What the system menu left for the shell: quit to the launcher / the game, view, volume, full screen, keys.
    pub fn take_sys_action(&mut self) -> Option<crate::sysmenu::SysAction> {
        self.sys_actions.pop_front()
    }

    /// The mouse in view pixels (`click`: the left button went down) - true while the system menu has it.
    pub fn sys_pointer(&mut self, app: &mut App, x: i32, y: i32, click: bool) -> bool {
        if !self.sys.open {
            return false;
        }
        if let Some(a) = self.sys.pointer(x, y, click) {
            self.sys_act(app, a);
        }
        true
    }

    /// story: the save generation and the frame it changed (the system menu's "progress since the last save")
    fn mark_saves(&mut self, app: &App) {
        if let Some(gen) = app.sim.as_any().and_then(|a| a.downcast_ref::<crate::story::StorySim>()).map(|s| s.session.world.saves.generation) {
            if self.save_mark.is_none_or(|m| m.0 != gen) {
                self.save_mark = Some((gen, app.sim.frame()));
            }
        }
    }

    /// The hint's clock where no [`Control::advance`] runs (headless stepping).
    pub fn tick_hint(&mut self, dt: f64) {
        if !self.sys.open {
            self.hint_t = (self.hint_t - dt).max(0.0);
        }
    }

    /// Show the "Esc: Menu" hint again (a shell starting a game in place).
    pub fn show_hint(&mut self) {
        self.hint_t = HINT_SECS;
    }

    /// A replay's menu chose to leave (the shell goes back to the launcher).
    pub fn take_exit_replay(&mut self) -> bool {
        std::mem::take(&mut self.exit_replay)
    }

    /// A replay is playing (its menu takes the player's buttons).
    pub fn playing(&self) -> bool {
        self.play.is_some()
    }

    /// Open a replay's pause menu (Esc in the shells), or close it as "Resume" when open and not at the end.
    pub fn toggle_play_menu(&mut self, app: &mut App) {
        if self.play.is_none() {
            return;
        }
        match &self.play_menu {
            Some(m) if !m.finished => {
                self.play_menu = None;
                app.paused = false;
            }
            Some(_) => {}
            None => {
                self.play_menu = Some(PlayMenu { finished: false, sel: 0, age: 0, quiet: 0 });
                app.paused = true;
            }
        }
    }

    /// The replay menu's buttons (player 1's edges), every host frame of a playback.
    fn play_menu_input(&mut self, app: &mut App, live: &[u16]) {
        use wf_core::input::{A, B, DOWN, START, UP};
        let held = live.first().copied().unwrap_or(0);
        let pressed = held & !self.play_prev;
        self.play_prev = held;
        let Some(m) = &mut self.play_menu else {
            if pressed & (START | B) != 0 {
                self.play_menu = Some(PlayMenu { finished: false, sel: 0, age: 0, quiet: 0 });
                app.paused = true;
                app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_ACCEPT as u8, volume: 64, pan: 0 });
            }
            return;
        };
        m.age = m.age.saturating_add(1);
        let quiet = m.quiet;
        m.quiet = if held == 0 { m.quiet.saturating_add(1) } else { 0 };
        if m.age < PLAY_MENU_LOCK {
            return;
        }
        if pressed & (UP | DOWN) != 0 {
            m.sel ^= 1;
            app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_CURSOR as u8, volume: 64, pan: 0 });
        }
        // B: the way back (resume a paused replay, leave a finished one)
        let pick = if quiet < PLAY_MENU_QUIET {
            None
        } else if pressed & (A | START) != 0 {
            Some(m.sel)
        } else if pressed & B != 0 {
            Some(if m.finished { 1 } else { 0 })
        } else {
            None
        };
        let Some(sel) = pick else { return };
        let finished = m.finished;
        app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_ACCEPT as u8, volume: 64, pan: 0 });
        self.play_menu = None;
        match (finished, sel) {
            // Resume
            (false, 0) => app.paused = false,
            // Watch again: from the start
            (true, 0) => {
                let t = self.log_start();
                self.seek(app, t);
                app.paused = false;
            }
            // Exit to launcher / Back
            _ => {
                self.exit_replay = true;
                self.requests.push(Request::ExitReplay);
            }
        }
    }

    /// the system menu over the picture, or the "Esc: Menu" hint at a game's start (fading out)
    fn draw_sys(&mut self, app: &mut App, w: usize, h: usize) {
        if self.sys.open {
            self.sys.draw(&app.packs, &mut app.renderer.fb, w, h);
            return;
        }
        if self.hint_t <= 0.0 || app.renderer.fb.len() < w * h {
            return;
        }
        self.text.ensure(&app.packs);
        let t = &self.text;
        let s = "Esc: Menu";
        let (tw, lh) = (t.width(s) + 8, t.line_h() + 4);
        let (x, y) = (4, h as i32 - lh - 4);
        // the last second fades: the plate drawn on a copy, mixed back by what is left
        let a = (self.hint_t.min(1.0) * 256.0) as u32;
        let fb = &mut app.renderer.fb;
        let rows: Vec<(usize, Vec<u32>)> = (y.max(0)..(y + lh + 2).min(h as i32))
            .map(|yy| {
                let i = yy as usize * w + x.max(0) as usize;
                (i, fb[i..(i + (tw + 2) as usize).min(yy as usize * w + w)].to_vec())
            })
            .collect();
        t.plate(fb, w, h, x, y, s, overlay::WHITE);
        if a < 256 {
            for (i, old) in rows {
                for (k, &o) in old.iter().enumerate() {
                    let n = fb[i + k];
                    let mix = |sh: u32| ((((n >> sh) & 255) * a + ((o >> sh) & 255) * (256 - a)) >> 8) << sh;
                    fb[i + k] = 0xFF00_0000 | mix(16) | mix(8) | mix(0);
                }
            }
        }
    }

    /// the replay menu over the picture: its title in the big font, the choice box with its cursor
    fn play_menu_view(&self, app: &mut App) {
        let Some(m) = &self.play_menu else { return };
        let Some(id) = self.play.as_ref().map(|p| p.id.clone()) else { return };
        let _ = id;
        let title = if m.finished { "Replay finished" } else { "Replay paused" };
        app.view.title = vec![wf_view::TitleView { text: wf_core::menu::text::units(title), x: 120, y: 44 }];
        app.view.dialog = Some(wf_view::DialogView {
            text: if m.finished { "- Watch again\n- Back".into() } else { "- Resume\n- Exit to launcher".into() },
            style: 1,
            page: 0,
            position: wf_view::DialogPos::Bottom,
            portrait: None,
            portrait_left: false,
            cursor: Some(m.sel),
        });
        app.view.dialog_anim = Some(wf_view::DialogAnim { scale: 0, typed: None, cursor_draws: Some(0) });
    }

    pub fn take_battle_select(&mut self) -> bool {
        std::mem::take(&mut self.battle_select)
    }

    /// whether the end screen is up (its choice box takes player 1's buttons)
    fn end_shown(&self, frame: u64) -> bool {
        self.play.is_none() && self.end.as_ref().is_some_and(|e| frame >= e.at + END_DELAY)
    }

    /// The end screen's input (player 1's button edges) and the buttons the world gets meanwhile (none). Some(choice)
    /// once A / START picks one.
    fn end_input(&mut self, app: &mut App, live: &mut Vec<u16>) -> Option<u8> {
        let f = app.sim.frame();
        let shown = self.end_shown(f);
        let e = self.end.as_mut()?;
        let held = live.first().copied().unwrap_or(0);
        let pressed = held & !e.prev;
        e.prev = held;
        let quiet = e.quiet;
        e.quiet = if held == 0 { e.quiet.saturating_add(1) } else { 0 };
        if !shown {
            return None;
        }
        live.iter_mut().for_each(|b| *b = 0);
        if f < e.at + END_DELAY + END_LOCK {
            return None;
        }
        use wf_core::input::{A, DOWN, START, UP};
        if pressed & (UP | DOWN) != 0 {
            e.sel ^= 1;
            app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_CURSOR as u8, volume: 64, pan: 0 });
        }
        if pressed & (A | START) != 0 && quiet >= END_QUIET {
            app.audio_out.push(AudioEvent::PlaySfx { id: wf_core::menu::SFX_ACCEPT as u8, volume: 64, pan: 0 });
            return Some(e.sel);
        }
        None
    }

    /// Retry: the same battle again from its start (the finished one's recording is saved, a new one starts)
    fn retry(&mut self, app: &mut App) {
        let Some((kind, config, _, _)) = start_of(app) else { return };
        let Ok(sim) = self.build_sim(app, kind, &config) else { return };
        self.finalize(app);
        self.install(app, sim);
        self.timeline.clear();
        self.timeline.capture(app.sim.as_ref());
        self.end = None;
        self.start_recording(app, None);
        app.paused = false;
        app.sim.view(&mut app.view);
    }

    /// One simulation step (live inputs or the replay's). False at the end of a replay.
    fn step_once(&mut self, app: &mut App, live: &[u16], audio: bool) -> bool {
        let mut live = live.to_vec();
        if self.play.is_none() {
            match self.end_input(app, &mut live) {
                Some(0) => {
                    self.retry(app);
                    return true;
                }
                Some(_) => {
                    self.battle_select = true;
                    self.requests.push(Request::BattleSelect);
                }
                None => {}
            }
        }
        let live = &live[..];
        let f = app.sim.frame();
        let inputs: Vec<u16> = match &self.play {
            Some(p) => match p.replay.inputs.get(f) {
                Some(x) => x.to_vec(),
                None => {
                    // the end: the replay's menu opens by itself ("Watch again" / "Back")
                    if !self.play_menu.as_ref().is_some_and(|m| m.finished) {
                        self.play_menu = Some(PlayMenu { finished: true, sel: 0, age: 0, quiet: 0 });
                    }
                    app.paused = true;
                    return false;
                }
            },
            None => {
                if let Some(r) = &mut self.rec {
                    if r.frame() != f {
                        // live input after a rewind / seek: the recording continues from here
                        r.truncate(f);
                        self.timeline.invalidate_after(f);
                    }
                }
                live.to_vec()
            }
        };
        for (i, v) in app.inputs.iter_mut().enumerate() {
            *v = inputs.get(i).copied().unwrap_or(0);
        }
        self.mark_saves(app);
        let a0 = app.audio_out.len();
        app.step();
        if !audio {
            // fast: keep music changes, drop sound effects
            let keep: Vec<AudioEvent> = app.audio_out.drain(a0..).filter(|e| matches!(e, AudioEvent::PlaySong(_) | AudioEvent::StopSong)).collect();
            app.audio_out.extend(keep);
        }
        let nf = app.sim.frame();
        self.last_inputs = app.inputs.clone();
        let sim = app.sim.as_ref();
        let mut desync = None;
        match &mut self.play {
            Some(p) => {
                if let Some(d) = p.checker.check(&p.replay, nf, || sim.state_hash()) {
                    if p.checker.divergence == Some(d) {
                        desync = Some(d);
                    }
                }
            }
            None => {
                if let Some(r) = &mut self.rec {
                    r.record(&app.inputs, nf, || sim.state_hash());
                }
            }
        }
        if let Some(d) = desync {
            self.toast = Some((format!("DESYNC: {d}"), overlay::BAD, 6.0));
        }
        self.timeline.on_frame(sim);
        if let Some(s) = CoreSim::of(sim) {
            for e in &s.world.events {
                if let Event::Damage { target, attacker, dmg, crit } = e {
                    let line = format!("{} {}>{} {:.1}{}", nf, ent_name(&s.world, *attacker), ent_name(&s.world, *target), *dmg as f64 / 1024.0, if *crit { " CRIT" } else { "" });
                    self.damage_log.push_back(line);
                    if self.damage_log.len() > 8 {
                        self.damage_log.pop_front();
                    }
                }
            }
            if self.play.is_none() && self.outcome != Some(s.outcome) {
                self.outcome = Some(s.outcome);
                if s.outcome != Outcome::Running {
                    self.autosave(app);
                }
            }
            // the end screen follows the outcome (a rewind back into the fight takes it away again)
            if s.outcome == Outcome::Running {
                self.end = None;
            } else if self.end_screen && self.play.is_none() && self.end.is_none() {
                self.end = Some(EndScreen { outcome: s.outcome, at: nf, sel: 0, prev: self.last_inputs.first().copied().unwrap_or(0), quiet: 0 });
            }
        }
        if let Some(g) = &mut self.ghost {
            g.sync(nf);
        }
        true
    }

    /// Advance by `dt` seconds of wall time with the live players' buttons. Returns the steps run.
    pub fn advance(&mut self, app: &mut App, dt: f64, live: &[u16]) -> u32 {
        let dt = dt.clamp(0.0, 0.25);
        if let Some(t) = &mut self.toast {
            t.2 -= dt;
            if t.2 <= 0.0 {
                self.toast = None;
            }
        }
        if !self.sys.open {
            self.hint_t = (self.hint_t - dt).max(0.0);
        }
        self.mark_saves(app);
        if !self.enabled {
            return app.advance(dt, &|| 0.0);
        }
        if self.play.is_some() {
            self.play_menu_input(app, live);
        } else {
            self.play_menu = None;
        }
        self.since_save += dt;
        if self.since_save >= self.autosave_secs {
            self.since_save = 0.0;
            self.autosave(app);
        }
        let period = 1.0 / app.pacing.fps;
        if self.rewinding {
            self.rew_acc += dt * if self.ff { self.hotkeys.fast_forward } else { 1.0 };
            let n = (self.rew_acc / period) as u64;
            self.rew_acc -= n as f64 * period;
            let cur = app.sim.frame();
            let target = cur.saturating_sub(n).max(self.log_start());
            if target < cur {
                self.seek(app, target);
            }
            return 0;
        }
        if self.step_req > 0 {
            self.step_req -= 1;
            return self.step_once(app, live, true) as u32;
        }
        if app.paused {
            return 0;
        }
        let speed = SPEEDS[self.speed] * if self.ff { self.hotkeys.fast_forward } else { 1.0 };
        self.acc += dt * speed;
        let max = (app.pacing.max_steps as f64 * speed.max(1.0)).ceil() as u32;
        let mut n = 0;
        while self.acc >= period && n < max {
            if !self.step_once(app, live, speed <= 2.0) {
                self.acc = 0.0;
                break;
            }
            self.acc -= period;
            n += 1;
        }
        if n == max {
            self.acc = self.acc.min(period);
        }
        n
    }

    /// Jump to `target` (clamped to the recording / replay). Sound effects stop; the music keeps playing.
    pub fn seek(&mut self, app: &mut App, target: u64) -> u64 {
        if !self.enabled {
            return app.sim.frame();
        }
        let log = match (&self.play, &self.rec) {
            (Some(p), _) => &p.replay.inputs,
            (None, Some(r)) => &r.replay.inputs,
            _ => return app.sim.frame(),
        };
        let f = self.timeline.seek(app.sim.as_mut(), log, target.max(log.start));
        app.audio_out.push(AudioEvent::StopAllSfx);
        if let Some(g) = &mut self.ghost {
            g.sync(f);
        }
        f
    }

    // ------------------------------------------------------------------------------------------- actions

    /// A key event. `game_bound`: the key drives a GBA button. Returns true when it was a hotkey (the host
    /// then does not pass it to the game).
    pub fn key(&mut self, app: &mut App, code: &str, m: Mods, down: bool, repeat: bool, game_bound: bool) -> bool {
        // the system menu takes every key while open (releases go on to the shell's held keys)
        if self.sys.open {
            if down {
                if let Some(a) = self.sys.key(code) {
                    self.sys_act(app, a);
                }
            }
            return down;
        }
        // Esc opens it in every mode; Enter (Start) where the game has no Start menu of its own (a battle)
        if down && !repeat && !m.any() && (code == "Escape" || (code == "Enter" && self.start_opens_menu(app))) {
            self.open_sys(app);
            return true;
        }
        if !self.enabled {
            return false;
        }
        if !down {
            if let Some(a) = self.hotkeys.lookup_release(code) {
                self.hold(app, a, false);
                return true;
            }
            return false;
        }
        let Some(a) = self.hotkeys.lookup(code, m, game_bound) else { return false };
        if a.is_hold() {
            if !repeat {
                self.hold(app, a, true);
            }
        } else if !repeat || matches!(a, Action::FrameAdvance | Action::SeekBack | Action::SeekForward) {
            self.act(app, a);
        }
        true
    }

    pub fn hold(&mut self, app: &mut App, a: Action, on: bool) {
        match a {
            Action::Rewind => {
                if on && !self.rewinding {
                    app.audio_out.push(AudioEvent::StopAllSfx);
                    self.rew_acc = 0.0;
                }
                self.rewinding = on;
            }
            Action::FastForward => self.ff = on,
            _ => {}
        }
    }

    /// Run an action (hotkey or a host UI button).
    pub fn act(&mut self, app: &mut App, a: Action) {
        if !self.enabled {
            return;
        }
        match a {
            Action::Rewind | Action::FastForward => {
                let on = !(if a == Action::Rewind { self.rewinding } else { self.ff });
                self.hold(app, a, on);
            }
            Action::Pause => {
                app.paused = !app.paused;
                self.acc = 0.0;
            }
            Action::FrameAdvance => {
                app.paused = true;
                self.step_req += 1;
            }
            Action::Slower => {
                self.speed = self.speed.saturating_sub(1);
                self.toast(format!("Speed x{}", SPEEDS[self.speed]));
            }
            Action::Faster => {
                self.speed = (self.speed + 1).min(SPEEDS.len() - 1);
                self.toast(format!("Speed x{}", SPEEDS[self.speed]));
            }
            Action::SeekBack => {
                let t = app.sim.frame().saturating_sub(SEEK_FRAMES);
                self.seek(app, t);
            }
            Action::SeekForward => {
                let t = app.sim.frame() + SEEK_FRAMES;
                self.seek(app, t);
            }
            Action::Restart => {
                let t = self.log_start();
                self.seek(app, t);
            }
            Action::SaveState(n) => self.save_state(app, n),
            Action::LoadState(n) => self.requests.push(Request::LoadState { slot: n }),
            Action::Replays => self.requests.push(Request::OpenReplays),
            Action::TakeOver => self.take_over(app),
            Action::InputDisplay => self.show_inputs = !self.show_inputs,
            Action::Training => {
                self.training = !self.training;
                self.toast(if self.training { "Training overlay on" } else { "Training overlay off" });
            }
            Action::Ghost => {
                if self.ghost.is_some() {
                    self.ghost_off = self.ghost.take();
                    self.toast("Ghost off");
                } else if let Some(mut g) = self.ghost_off.take() {
                    g.sync(app.sim.frame());
                    self.ghost = Some(g);
                    self.toast("Ghost on");
                } else {
                    self.toast("Pick a ghost in the replay browser");
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------- recordings

    /// The live recording as a file (`None` in playback or without a sim that records).
    pub fn current_replay(&self, app: &App) -> Option<(String, Replay)> {
        let r = self.rec.as_ref()?;
        let mut r = r.clone();
        if app.sim.frame() == r.frame() {
            r.finish(app.sim.state_hash());
        }
        if let Some(s) = Self::session(app) {
            r.replay.header.outcome = outcome_name(s.outcome).into();
        }
        Some((self.rec_id.clone(), r.replay))
    }

    fn autosave(&mut self, app: &App) {
        let Some((id, r)) = self.current_replay(app) else { return };
        if r.inputs.len() == 0 || r.inputs.len() == self.saved_frames {
            return;
        }
        self.saved_frames = r.inputs.len();
        self.requests.push(Request::SaveReplay { id, bytes: r.to_bytes(), header: r.header.clone() });
    }

    /// Persist the live recording now (host shutdown, before switching sessions).
    pub fn finalize(&mut self, app: &App) {
        self.autosave(app);
    }

    /// A fresh sim of a header's kind and config (battle or story).
    fn build_sim(&self, app: &App, kind: &str, config: &serde_json::Value) -> Result<Box<dyn Sim>, String> {
        match kind {
            STORY_KIND => Ok(Box::new(StorySim::from_start(&app.packs, config)?)),
            SIM_KIND => Ok(Box::new(self.build_core(app, config)?)),
            k => Err(format!("cannot rebuild a \"{k}\" simulation")),
        }
    }

    fn build_core(&self, app: &App, config: &serde_json::Value) -> Result<CoreSim, String> {
        let cfg: BattleConfig = serde_json::from_value(config.clone()).map_err(|e| format!("config: {e}"))?;
        let data = match Self::session(app) {
            Some(s) => s.data.clone(),
            None => battle::game_data(&app.packs)?,
        };
        let mut s = Session::new(data, cfg);
        // online PvP recordings (wf-net) carry the rule set next to the battle config
        if config["pvp"].as_bool() == Some(true) {
            wf_core::battle::set_pvp(&mut s.world, true);
        }
        Ok(CoreSim(s))
    }

    /// Replace the app's sim (new config): stop sounds, re-render, music if not starting at frame 0.
    fn install(&mut self, app: &mut App, sim: Box<dyn Sim>) {
        self.hint_t = HINT_SECS;
        self.save_mark = None;
        app.sim = sim;
        app.inputs = vec![0; app.sim.players()];
        self.last_inputs = app.inputs.clone();
        app.audio_out.push(AudioEvent::StopAllSfx);
        app.audio_out.push(AudioEvent::StopSong);
        self.damage_log.clear();
        self.acc = 0.0;
    }

    fn music(app: &mut App) {
        if app.sim.frame() == 0 {
            return; // the first step starts it
        }
        let story = app.sim.as_any().and_then(|a| a.downcast_ref::<StorySim>()).map(|s| s.session.world.scene.song).filter(|&s| s > 0).map(|s| s as u16);
        let song = story.or_else(|| Self::session(app).and_then(|s| s.world.data.areas.get(&s.world.arena.map).and_then(|a| a.1)));
        if let Some(id) = song {
            app.audio_out.push(AudioEvent::PlaySong(id));
        }
    }

    /// Snapshot of the replay's start (frame 0 from its config, or its start state) so seeks before the
    /// current frame work after loading mid-recording.
    fn seed_start(&mut self, app: &App, r: &Replay) {
        if let Ok(mut s) = self.build_sim(app, &r.header.sim, &r.header.config) {
            if r.start_state.as_ref().is_none_or(|st| s.restore(st)) {
                self.timeline.capture(s.as_ref());
            }
        }
    }

    /// Play a replay file (`id` = its name, for branches / listings).
    pub fn play_replay(&mut self, app: &mut App, id: &str, bytes: &[u8]) -> Result<(), String> {
        let r = parse_replay(bytes)?;
        if r.header.sim != SIM_KIND && r.header.sim != STORY_KIND {
            return Err(format!("not a battle or story replay ({})", r.header.sim));
        }
        if let Some(m) = r.header.content.mods_mismatch(&self.content) {
            return Err(format!("This replay was {m}"));
        }
        let mut sim = self.build_sim(app, &r.header.sim, &r.header.config)?;
        if let Some(s) = &r.start_state {
            if !sim.restore(s) {
                return Err("the replay's start state does not fit this engine".into());
            }
        }
        self.finalize(app);
        self.install(app, sim);
        self.timeline.clear();
        self.timeline.capture(app.sim.as_ref());
        let mut checker = Checker::default();
        let sim = app.sim.as_ref();
        checker.check(&r, sim.frame(), || sim.state_hash());
        let diff = r.header.content.diff(&self.content);
        self.play = Some(Playback { id: id.to_string(), replay: r, checker });
        self.play_menu = None;
        self.rec = None;
        app.paused = false;
        self.speed = NORMAL;
        Self::music(app);
        app.sim.view(&mut app.view);
        if let Some(g) = &mut self.ghost {
            g.sync(app.sim.frame());
        }
        if let Some(d) = self.divergence() {
            self.warn(format!("Start state differs: {d}"));
        } else if !diff.is_empty() {
            self.warn(format!("Recorded with other content: {}", diff.join("; ")));
        } else {
            self.toast(format!("Playing {id} - Start / B: menu"));
        }
        Ok(())
    }

    /// Stop playback / take over the replay at the current frame: inputs before it are the replay's, the
    /// player's from here (a new recording that names the replay as its parent).
    pub fn take_over(&mut self, app: &mut App) {
        let Some(p) = self.play.take() else { return };
        self.play_menu = None;
        let f = app.sim.frame();
        let mut r = p.replay;
        r.inputs.truncate(f);
        r.checkpoints.retain(|c| c.0 <= f);
        r.header.branch = Some(Branch { parent: p.id.clone(), frame: f });
        r.header.created = self.now_unix;
        r.header.outcome.clear();
        self.timeline.invalidate_after(f);
        self.rec = Some(Recorder::resume(r));
        self.rec_id = self.new_id();
        self.saved_frames = 0;
        self.outcome = Self::session(app).map(|s| s.outcome);
        app.paused = false;
        self.speed = NORMAL;
        self.toast(format!("Taking over {} at {}", p.id, fmt_time(f)));
    }

    /// Save the current state into `slot` (the host stores the request's bytes).
    pub fn save_state(&mut self, app: &mut App, slot: u8) {
        let Some(st) = self.make_state(app, slot) else { return };
        self.requests.push(Request::SaveState { slot, bytes: st.to_bytes() });
        self.toast(format!("Saved state {slot} ({})", fmt_time(app.sim.frame())));
    }

    pub fn make_state(&self, app: &App, slot: u8) -> Option<SaveState> {
        let (kind, config, _, title) = start_of(app)?;
        let snapshot = app.sim.snapshot()?;
        let f = app.sim.frame();
        let mut replay = match (&self.play, &self.rec) {
            (Some(p), _) => p.replay.clone(),
            (None, Some(r)) => r.replay.clone(),
            _ => return None,
        };
        replay.inputs.truncate(f);
        replay.checkpoints.retain(|c| c.0 <= f);
        Some(SaveState {
            header: StateHeader {
                sim: kind.into(),
                content: self.content.clone(),
                config,
                frame: f,
                hash: format!("{:016x}", app.sim.state_hash()),
                created: self.now_unix,
                slot: slot as u32,
                title,
            },
            snapshot,
            replay: (replay.end_frame() == f).then_some(replay),
        })
    }

    /// Load a save state (any session / config; across app restarts). Recording continues from it.
    pub fn load_state(&mut self, app: &mut App, bytes: &[u8]) -> Result<(), String> {
        let st = SaveState::from_bytes(bytes).map_err(|e| e.to_string())?;
        if st.header.sim != SIM_KIND && st.header.sim != STORY_KIND {
            return Err(format!("not a battle or story save state ({})", st.header.sim));
        }
        if let Some(m) = st.header.content.mods_mismatch(&self.content) {
            return Err(format!("This save state was {m}"));
        }
        let same = start_of(app).is_some_and(|(k, c, _, _)| k == st.header.sim && c == st.header.config);
        let mut fresh = None;
        if !same {
            fresh = Some(self.build_sim(app, &st.header.sim, &st.header.config)?);
        }
        self.finalize(app);
        if let Some(sim) = fresh {
            self.install(app, sim);
        }
        if !app.sim.restore(&st.snapshot) {
            return Err("the save state does not fit this engine build".into());
        }
        app.audio_out.push(AudioEvent::StopAllSfx);
        if !same {
            Self::music(app);
        }
        let f = app.sim.frame();
        let hash_ok = format!("{:016x}", app.sim.state_hash()) == st.header.hash;
        self.play = None;
        self.timeline.clear();
        let branch = Some(Branch { parent: format!("state {}", st.header.slot), frame: f });
        match st.replay {
            Some(mut r) if r.end_frame() == f => {
                self.seed_start(app, &r);
                r.header.branch = branch;
                r.header.created = self.now_unix;
                r.header.outcome.clear();
                self.rec = Some(Recorder::resume(r));
                self.rec_id = self.new_id();
                self.saved_frames = 0;
                self.outcome = Self::session(app).map(|s| s.outcome);
            }
            _ => self.start_recording(app, branch),
        }
        self.timeline.capture(app.sim.as_ref());
        app.sim.view(&mut app.view);
        if let Some(g) = &mut self.ghost {
            g.sync(f);
        }
        let diff = st.header.content.diff(&self.content);
        if !hash_ok {
            self.warn("State loaded, but its hash differs (other engine build?)");
        } else if !diff.is_empty() {
            self.warn(format!("State from other content: {}", diff.join("; ")));
        } else {
            self.toast(format!("Loaded state {} ({})", st.header.slot, fmt_time(f)));
        }
        Ok(())
    }

    /// Run another replay as a ghost next to this one (same frame number), drawn semi-transparent.
    pub fn set_ghost(&mut self, app: &App, id: &str, bytes: &[u8]) -> Result<(), String> {
        let r = parse_replay(bytes)?;
        if r.header.sim != SIM_KIND {
            return Err(format!("ghosts are battle replays (this is {})", r.header.sim));
        }
        if let Some(m) = r.header.content.mods_mismatch(&self.content) {
            return Err(format!("This replay was {m}"));
        }
        let mut sim = self.build_core(app, &r.header.config)?;
        if let Some(s) = &r.start_state {
            if !sim.restore(s) {
                return Err("the ghost's start state does not fit this engine".into());
            }
        }
        let mut timeline = Timeline::new(TimelineConfig { interval: 60, window: 600, ..Default::default() });
        timeline.capture(&sim);
        let other_arena = Self::session(app).is_some_and(|s| s.config.arena != sim.0.config.arena);
        let mut g = Ghost { id: id.to_string(), sim, replay: r, timeline };
        g.sync(app.sim.frame());
        self.ghost = Some(g);
        self.ghost_off = None;
        if other_arena {
            self.warn("Ghost is from another arena");
        } else {
            self.toast(format!("Ghost: {id}"));
        }
        Ok(())
    }

    pub fn clear_ghost(&mut self) {
        self.ghost = None;
        self.ghost_off = None;
    }

    /// Headless verification of a replay file against this build and content.
    pub fn verify(&self, app: &App, bytes: &[u8]) -> Result<VerifyReport, String> {
        let r = parse_replay(bytes)?;
        let mut sim = self.build_sim(app, &r.header.sim, &r.header.config)?;
        Ok(wf_replay::verify(&r, sim.as_mut()))
    }

    // ------------------------------------------------------------------------------------------- drawing

    /// Render local player `player` (with the ghost) and draw the tool overlays.
    pub fn render(&mut self, app: &mut App, player: usize, win_w: u32, win_h: u32) -> Fit {
        app.sim.view(&mut app.view);
        self.end_view(app);
        self.play_menu_view(app);
        if let Some(g) = &self.ghost {
            let mut gv = RenderView::default();
            g.sim.view(&mut gv);
            let ids = &g.sim.0.world.players;
            let mut sprites: Vec<_> = gv
                .sprites
                .into_iter()
                .filter(|s| ids.contains(&s.entity) && s.sheet.starts_with("sprites/"))
                .map(|mut s| {
                    s.effect = SpriteEffect::Blend { eva: 7, evb: 9 };
                    s
                })
                .collect();
            sprites.append(&mut app.view.sprites);
            app.view.sprites = sprites;
        }
        let f = app.render(player, win_w, win_h);
        self.draw_overlays(app, player, f.view_w as usize, f.view_h as usize);
        f
    }

    /// the end screen over the arena: the result in the game's big title font, the choice box with its cursor
    fn end_view(&self, app: &mut App) {
        if !self.end_shown(app.sim.frame()) {
            return;
        }
        let Some(e) = &self.end else { return };
        let Some(md) = CoreSim::of(app.sim.as_ref()).and_then(|s| s.data.menu.clone()) else { return };
        let won = e.outcome == Outcome::Won;
        // (clear of player 1: the title and the choice box go to the screen half the player is not in - at the
        // bottom of the screen the box covered Goku, playtest 40)
        let low = CoreSim::of(app.sim.as_ref())
            .and_then(|s| s.world.player_idx(0).map(|i| s.world.pl_ref(i).a.py()))
            .zip(app.view.cameras.first().map(|c| c.y))
            .is_some_and(|(py, cy)| py - cy > 88);
        app.view.title = vec![wf_view::TitleView { text: md.text.s(if won { 111 } else { 112 }).to_vec(), x: 120, y: if low { 88 } else { 44 } }];
        app.view.dialog = Some(wf_view::DialogView {
            text: END_CHOICES.into(),
            style: 1,
            page: 0,
            position: if low { wf_view::DialogPos::Top } else { wf_view::DialogPos::Bottom },
            portrait: None,
            portrait_left: false,
            cursor: Some(e.sel),
        });
        // fully typed, full size, the cursor shown
        app.view.dialog_anim = Some(wf_view::DialogAnim { scale: 0, typed: None, cursor_draws: Some(0) });
    }

    fn draw_overlays(&mut self, app: &mut App, player: usize, w: usize, h: usize) {
        if self.enabled {
            self.draw_tools(app, player, w, h);
        }
        // the system menu over everything (the tools' status lines too)
        self.draw_sys(app, w, h);
    }

    fn draw_tools(&mut self, app: &mut App, player: usize, w: usize, h: usize) {
        self.text.ensure(&app.packs);
        let t = &self.text;
        let lh = t.line_h();
        // (the picture's own origin: the PC view is clamped to the map, `Renderer::view_origin_in`)
        let (vx, vy, _, _) = app.renderer.origin;
        let _ = player;
        let fb = &mut app.renderer.fb;
        if fb.len() < w * h {
            return;
        }
        if self.training {
            if let Some(s) = CoreSim::of(app.sim.as_ref()) {
                overlay::training(t, s, fb, w, h, vx, vy);
            }
            let mut y = 30;
            for l in &self.damage_log {
                t.plate(fb, w, h, 8, y, l, overlay::WHITE);
                y += lh + 1;
            }
        }
        if self.show_inputs {
            overlay::input_display(t, &self.last_inputs, fb, w, h);
        }
        // status line, top right
        let frame = app.sim.frame();
        let mut parts: Vec<String> = Vec::new();
        if let Some(p) = &self.play {
            parts.push(format!("REPLAY {} / {}", fmt_time(frame - p.replay.start_frame()), fmt_time(p.replay.inputs.len())));
        } else if self.training || app.paused || self.rewinding {
            parts.push(format!("REC {}", fmt_time(frame)));
        }
        if self.rewinding {
            parts.push("<< REWIND".into());
        } else if app.paused {
            parts.push("PAUSED".into());
        }
        let speed = SPEEDS[self.speed] * if self.ff { self.hotkeys.fast_forward } else { 1.0 };
        if speed != 1.0 && !self.rewinding {
            parts.push(format!("x{speed}"));
        }
        if let Some(g) = &self.ghost {
            parts.push(format!("GHOST {}", if g.sim.frame() == frame { "" } else { "(ended)" }).trim_end().to_string());
        }
        if self.training || !parts.is_empty() {
            parts.push(format!("F{frame}"));
        }
        if !parts.is_empty() {
            let s = parts.join("  ");
            let tw = t.width(&s);
            t.plate(fb, w, h, w as i32 - tw - 8, 6, &s, overlay::ACCENT);
        }
        let mut right_y = if parts.is_empty() { 6 } else { 6 + lh + 2 };
        if let Some(d) = self.play.as_ref().and_then(|p| p.checker.divergence) {
            let s = format!("DESYNC {d}");
            let tw = t.width(&s);
            t.plate(fb, w, h, w as i32 - tw - 8, 6 + lh + 2, &s, overlay::BAD);
            right_y = 6 + 2 * (lh + 2);
        }
        // the toast goes top right, under the status lines: the bottom centre (where it was) is where the game's
        // own text boxes / choices are (playtest 31: "Classic 240x160 (F1)" over the end screen's "Retry"); with a
        // text box at the top it goes to the bottom right instead
        if let Some((m, c, _)) = &self.toast {
            let tw = t.width(m);
            let top_box = app.view.dialog.as_ref().is_some_and(|d| matches!(d.position, wf_view::DialogPos::Top));
            let y = if top_box { h as i32 - lh - 6 } else { right_y };
            t.plate(fb, w, h, w as i32 - tw - 8, y, m, *c);
        }
        // the title's link box open with no link set up: it can only wait (the game's own box, kept as it is) - say
        // how to play with a friend, in player terms (playtest 27: command-line flags a player cannot use, over the
        // title's menu item and the copyright line) - at the top of the picture, clear of both
        if !self.link_configured {
            let story = app.sim.as_any().and_then(|a| a.downcast_ref::<crate::story::StorySim>());
            if story.is_some_and(|s| crate::link::title_wait(s.world()).is_some()) {
                let lines = ["No friend connected.", "To play together: Launcher > Story > Link play,", "both players on the same room number."];
                let mut y = 4;
                for (i, l) in lines.iter().enumerate() {
                    let tw = t.width(l);
                    t.plate(fb, w, h, ((w as i32 - tw) / 2).max(0), y, l, if i == 0 { overlay::ACCENT } else { overlay::WHITE });
                    y += lh + 1;
                }
            }
        }
    }

    /// One-line JSON status for host UIs.
    pub fn status_json(&self, app: &App) -> String {
        serde_json::json!({
            "enabled": self.enabled,
            "mode": if self.play.is_some() { "playback" } else { "live" },
            "frame": app.sim.frame(),
            "start": self.log_start(),
            "end": self.log_end(),
            "paused": app.paused,
            "speed": SPEEDS[self.speed],
            "rewinding": self.rewinding,
            "ff": self.ff,
            "recording": self.rec_id,
            "replay": self.playback_id(),
            "ghost": self.ghost_id(),
            "training": self.training,
            "inputs": self.show_inputs,
            "desync": self.divergence().map(|d| d.to_string()),
            "snapshots": self.timeline.len(),
            "snapshot_bytes": self.timeline.bytes(),
        })
        .to_string()
    }
}
