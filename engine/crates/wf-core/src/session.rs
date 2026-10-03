//! The headless session: the one API every consumer (frontend adapter, agents, tests, netcode) drives.
//!
//! `reset(config)` builds a battle, `step(inputs)` advances one frame with every player's held buttons and returns the
//! frame's events, `observe(player)` gives a structured observation, `snapshot` / `restore` copy the whole state
//! (a `World` clone: plain data + one Arc to the static game data), `hash` is the per-frame state hash.

use crate::battle::{self, BattleConfig, Outcome};
use crate::content::GameData;
use crate::input::InputFrame;
use crate::observe::{observe, Observation};
use crate::world::{Event, World, MAX_PLAYERS};
use std::sync::Arc;

#[derive(Clone)]
pub struct Snapshot {
    pub world: World,
    pub prev: [u16; MAX_PLAYERS],
    pub outcome: Outcome,
}

pub struct Session {
    pub data: Arc<GameData>,
    pub world: World,
    pub config: BattleConfig,
    /// held buttons of the previous frame per player (edges)
    pub prev: [u16; MAX_PLAYERS],
    pub outcome: Outcome,
    /// field: the script host's synchronous hook. Called for every `FieldEvent::Run` right after the phase of the
    /// frame that produced it (deferred calls at the start of the world update, triggers / defeats after it), so a
    /// script's RNG draws and natives (drops, flags, warps) land where the game runs them. None = scripts only leave as
    /// events (the host runs them after the step).
    pub hook: Option<FieldHook>,
    /// the screen's one-frame lags (field sessions through `Sim`; `view::Presenter`)
    pub presenter: crate::view::Presenter,
}

pub type FieldHook = Box<dyn FnMut(&mut World, &crate::field::FieldEvent) + Send>;

impl Session {
    pub fn new(data: Arc<GameData>, config: BattleConfig) -> Session {
        let mut world = battle::create(Arc::clone(&data), &config);
        crate::scene::note_events(&mut world);
        Session { data, world, config, prev: [0; MAX_PLAYERS], outcome: Outcome::Running, hook: None, presenter: Default::default() }
    }
    /// a field session (the overworld game): new game or saved story, start area
    pub fn new_field(data: Arc<GameData>, config: crate::field::FieldConfig) -> Session {
        let mut world = crate::field::create(Arc::clone(&data), &config);
        crate::scene::note_events(&mut world);
        Session { data, world, config: BattleConfig::default(), prev: [0; MAX_PLAYERS], outcome: Outcome::Running, hook: None, presenter: Default::default() }
    }
    /// a field session with the script host's hook attached from the start: the first area's load scripts (on-enter:
    /// gates / doors / bridges through open_area / block_area, 48 area variants) run before frame 0
    pub fn new_field_hooked(data: Arc<GameData>, config: crate::field::FieldConfig, hook: FieldHook) -> Session {
        let mut s = Session::new_field(data, config);
        s.hook = Some(hook);
        Self::run_hook(&mut s.hook, &mut s.world, 0);
        crate::scene::note_events(&mut s.world);
        s
    }
    pub fn reset(&mut self, config: BattleConfig) {
        *self = Session::new(Arc::clone(&self.data), config);
    }
    /// one frame with every player's held buttons (missing players = 0)
    pub fn step(&mut self, held: &[u16]) -> &[Event] {
        let mut inputs = [InputFrame::default(); MAX_PLAYERS];
        for (s, inp) in inputs.iter_mut().enumerate() {
            let h = held.get(s).copied().unwrap_or(0);
            *inp = InputFrame::from_held(h, self.prev[s]);
            self.prev[s] = h;
        }
        self.step_frames(inputs)
    }
    /// one frame with explicit input frames (edges given)
    pub fn step_frames(&mut self, inputs: [InputFrame; MAX_PLAYERS]) -> &[Event] {
        self.step_world(inputs);
        self.step_post()
    }
    /// the first part of a frame: input, the objects' update, the cutscene systems (screen, dialog, command tracks).
    /// A script host resumes its blocked scripts after this and before [`Session::step_post`] (the game resumes a
    /// script from its wait mode before the frame's camera update; `wf-script-core` docs).
    pub fn step_world(&mut self, inputs: [InputFrame; MAX_PLAYERS]) {
        self.step_begin(inputs);
        self.step_update();
    }
    /// the frame's start: input, the cutscene input mask, the field's deferred calls (breakables, the death effects'
    /// drop scripts and kill XP). A script host runs the `FieldEvent::Run`s this leaves before [`Session::step_update`]
    /// (the game runs them inside these calls, before the objects' update).
    pub fn step_begin(&mut self, inputs: [InputFrame; MAX_PLAYERS]) {
        let w = &mut self.world;
        w.events.clear();
        w.g.shake_ended = false;
        w.input = inputs;
        // `return_to_title` (0x8E) last frame: the session restarts on the title screen (menu/title.rs)
        if w.scene.requests.iter().any(|r| matches!(r, crate::scene::Request::ReturnToTitle)) {
            *w = crate::menu::title::title_world(&self.data, w);
            w.input = inputs;
            self.presenter.reset();
            return;
        }
        // a menu screen runs instead of the world (menu/mod.rs): nothing else updates
        if crate::menu::active(w) {
            crate::menu::step(w);
            // the title's New Game / Continue: a new field world (menu/title.rs); its load events stay for the host
            if let Some(nw) = crate::menu::title::boot(&self.data, w) {
                *w = nw;
                self.presenter.reset();
                return;
            }
            if !std::mem::take(&mut w.menu.resume) {
                w.menu.closed_this_frame = !crate::menu::active(w);
                // scripts a menu starts (item use) run where the game runs them: right after its step
                Self::run_hook(&mut self.hook, &mut self.world, 0);
                return;
            }
        }
        crate::scene::pre_step(w);
        if w.field.is_some() {
            crate::field::pre_step(w);
            Self::run_hook(&mut self.hook, &mut self.world, 0);
        }
    }
    /// the objects' update and the cutscene systems (second part of [`Session::step_world`])
    pub fn step_update(&mut self) {
        let w = &mut self.world;
        if std::mem::take(&mut w.menu.closed_this_frame) {
            return;
        }
        if crate::menu::active(w) {
            if w.field.is_some() && crate::menu::minigame::world_runs(w) {
                // the players get no input: the mode reads the keys (menu_parity mg_power: no punches)
                w.input = [InputFrame::default(); MAX_PLAYERS];
                w.step_entities();
            } else if w.field.is_some() && matches!(w.menu.phase, crate::menu::Phase::FadeOut { .. }) {
                // a screen opening over the field: its fade-out leaves the objects still, the cameras keep easing
                // (segment sb 21-14: the save point's party menu, the follow camera reaches its target under it)
                crate::field::world_mode_cameras(w);
            }
            return;
        }
        if let Some(f) = w.field.as_mut() {
            f.pre_update = false;
        }
        if w.field.is_some() {
            if !crate::field::frozen(w) && !crate::scene::frozen(w) {
                battle::pre_step(w);
                w.step_entities();
            }
            // the leader's KO timer: the death / Lazarus Crystal hook (menu/gameover.rs)
            crate::menu::gameover::tick(w);
            crate::scene::tick(w);
            return;
        }
        let w = &mut self.world;
        if !crate::scene::frozen(w) {
            battle::pre_step(w);
            w.step_entities();
        }
        crate::scene::tick(w);
    }
    /// the rest of the frame: cameras and the field / battle rules after the update
    pub fn step_post(&mut self) -> &[Event] {
        let w = &mut self.world;
        if crate::menu::active(w) && !crate::menu::opened_this_frame(w) {
            if w.field.is_some() && crate::menu::minigame::world_runs(w) {
                crate::field::world_mode_cameras(w);
            }
            crate::menu::post(w);
            return &self.world.events;
        }
        if w.field.is_some() {
            let k = w.events.len();
            crate::field::post_step(w);
            Self::run_hook(&mut self.hook, &mut self.world, k);
            // START: the pause menu (the field main loop checks it after the objects, triggers and scripts)
            crate::menu::check_pause(&mut self.world);
            crate::menu::mapscreen::check_select(&mut self.world);
            // the frame's music events keep `Scene.song` current inside the state (byte snapshots carry no events)
            crate::scene::note_events(&mut self.world);
            return &self.world.events;
        }
        battle::post_step(w);
        if self.outcome == Outcome::Running {
            self.outcome = battle::outcome(w);
        }
        crate::scene::note_events(&mut self.world);
        &self.world.events
    }
    /// call the hook for the Run events from index `from` on (events the hook itself adds are not re-dispatched)
    fn run_hook(hook: &mut Option<FieldHook>, w: &mut World, from: usize) {
        let Some(h) = hook.as_mut() else { return };
        let n = w.events.len();
        for k in from..n {
            if let Event::Field(fe @ crate::field::FieldEvent::Run { .. }) = &w.events[k] {
                let fe = fe.clone();
                h(w, &fe);
            }
        }
    }
    pub fn observe(&self, player: usize) -> Observation {
        observe(&self.world, player)
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { world: self.world.clone(), prev: self.prev, outcome: self.outcome }
    }
    pub fn restore(&mut self, s: &Snapshot) {
        self.world = s.world.clone();
        self.prev = s.prev;
        self.outcome = s.outcome;
    }
    pub fn hash(&self) -> u64 {
        self.world.state_hash()
    }
    pub fn frame(&self) -> u32 {
        self.world.frame
    }
}
