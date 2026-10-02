//! Story mode: a `wf-core` field session (areas, exits, NPCs, pickups, the world map) with the game's scripts run
//! by the Rhai runtime (`wf-script-rhai`, decompiled scripts from the content pack's `scripts/`) on the script host
//! (`wf-script-core`). Implements `wf_view::Sim`, so the shells treat it like the battle.
//!
//! Script timing follows the host contract (docs/script-host.md): the frame's objects + cutscene systems run
//! (`Session::step_world`), then scripts started this frame run and a blocked script resumes when its pending
//! action completes (`poll`), then `Session::step_post`. Scripts started by an area load (on-enter / on-leave)
//! and by triggers / talk / pickups / defeats arrive as `FieldEvent::Run` events; they are run in order, one at a
//! time (the game's event runner is single-threaded). Difference from the game: a load's scripts run right after
//! the load instead of inside it (the core's synchronous hook must be `Send`; the Rhai runtime is not), which only
//! matters for RNG draws inside those scripts.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use wf_content::Packs;
use wf_core::content::GameData;
use wf_core::field::{Cause, FieldConfig, FieldEvent};
use wf_core::fieldcontent::Callback;
use wf_core::input::InputFrame;
use wf_core::session::Session;
use wf_core::world::{Event, World, MAX_PLAYERS};
use wf_script::vm::Globals;
use wf_script_core::{poll, CoreState, HostState, SharedHost};
use wf_script_rhai::{Runtime, Status, UnitRef};
use wf_view::{Buttons, RenderView, Sim, SimEvent};


/// A script waiting to run.
#[derive(Clone, Debug)]
struct Job {
    run: Callback,
    cause: Cause,
}

pub struct StorySim {
    pub session: Session,
    rt: Runtime,
    host: SharedHost,
    hs: HostState,
    globals: Globals,
    stepped: std::collections::BTreeSet<String>,
    queue: VecDeque<Job>,
    running: bool,
    /// why the running script started (an enemy AI node waits for its script's end)
    cause: Option<Cause>,
    prev: [u16; MAX_PLAYERS],
    /// scripts that failed to compile / run (shown by the shells' logs)
    pub errors: Vec<String>,
    /// audio of the initial load (song), emitted with the first step
    init_events: Vec<SimEvent>,
    /// the screen's one-frame lags (wf-core view::Presenter; the core's view does banner, fades, tile timers, the
    /// greyed icon, the dialogue animation)
    presenter: wf_core::view::Presenter,
}

/// `areas.json` key `"W-A"` -> (world, area)
pub fn parse_area(s: &str) -> Option<(i32, i32)> {
    let (w, a) = s.split_once('-')?;
    Some((w.trim().parse().ok()?, a.trim().parse().ok()?))
}

impl StorySim {
    /// A new game at `area` (`"1-1"` = Snakeway, where the game's play starts), or `story` to continue.
    pub fn new(packs: &Packs, area: (i32, i32), chars: Vec<i32>, seed: u32) -> Result<StorySim, String> {
        let data: Arc<GameData> = crate::battle::load_data(packs)?;
        if data.field.is_none() {
            return Err("this content has no world/ data (re-extract with the current extractor)".into());
        }
        // the dialogue font drives the typewriter / line fitter
        if let Ok(f) = wf_render::ui::Font::load(packs, "dialog") {
            wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
        }
        let files: BTreeMap<String, String> = packs
            .list()
            .into_iter()
            .filter(|p| p.starts_with("scripts/") && p.ends_with(".rhai"))
            .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
            .collect();
        if files.is_empty() {
            return Err("this content has no scripts/ (re-extract with the current extractor)".into());
        }
        let cfg = FieldConfig { world: area.0, area: area.1, chars, rng: wf_core::battle::RngInit::Seed(seed), ..Default::default() };
        let session = Session::new_field(Arc::clone(&data), cfg);
        Self::with_session(packs, session)
    }

    /// The session started mid-area (harnesses): no area banner for the current area.
    pub fn skip_area_banner(&mut self) {
        self.session.world.scene.banner = None;
        self.presenter.reset();
    }

    /// Story mode around an existing field session (tests / harnesses that build the session themselves).
    pub fn with_session(packs: &Packs, mut session: Session) -> Result<StorySim, String> {
        if let Ok(f) = wf_render::ui::Font::load(packs, "dialog") {
            wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
        }
        let files: BTreeMap<String, String> = packs
            .list()
            .into_iter()
            .filter(|p| p.starts_with("scripts/") && p.ends_with(".rhai"))
            .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
            .collect();
        if files.is_empty() {
            return Err("this content has no scripts/ (re-extract with the current extractor)".into());
        }
        let stepped = files.iter().filter(|(_, src)| src.contains("fn step_0()")).map(|(p, _)| p.clone()).collect();
        wf_core::field::attach_host(&mut session.world, true);
        let spare = session.world.clone();
        let host = SharedHost(std::rc::Rc::new(std::cell::RefCell::new(CoreState { world: spare, story: HostState::default() })));
        let rt = Runtime::new(&files, Box::new(host.clone()))?;
        let mut s = StorySim {
            session,
            rt,
            host,
            hs: HostState::default(),
            globals: Globals::default(),
            stepped,
            queue: VecDeque::new(),
            running: false,
            cause: None,
            prev: [0; MAX_PLAYERS],
            errors: Vec::new(),
            init_events: Vec::new(),
            presenter: Default::default(),
        };
        // the first area's load scripts (gates, doors, bridges) before frame 0
        let evs: Vec<Event> = s.session.world.events.clone();
        wf_core::view::sim_events(&s.session.world, &mut s.init_events);
        s.collect(&evs);
        s.run_queue();
        Ok(s)
    }

    fn collect(&mut self, evs: &[Event]) {
        for e in evs {
            if let Event::Field(FieldEvent::Run { run, cause, .. }) = e {
                self.queue.push_back(Job { run: run.clone(), cause: cause.clone() });
            }
        }
    }

    /// run `op` with the shared host holding the session's world and the host state, then move them back
    fn with_host<T>(&mut self, op: impl FnOnce(&mut Runtime) -> T) -> T {
        {
            let mut st = self.host.0.borrow_mut();
            std::mem::swap(&mut st.world, &mut self.session.world);
            std::mem::swap(&mut st.story, &mut self.hs);
        }
        let r = op(&mut self.rt);
        let mut st = self.host.0.borrow_mut();
        std::mem::swap(&mut st.world, &mut self.session.world);
        std::mem::swap(&mut st.story, &mut self.hs);
        r
    }

    fn finish(&mut self, st: Result<Status, String>) {
        if !matches!(st, Ok(Status::Blocked)) {
            match self.cause.take() {
                Some(Cause::Ai { entity }) => wf_core::scene::ai_script_done(&mut self.session.world, entity),
                // the load waits for its on-enter script (field::finish_arrival)
                Some(Cause::AreaEnter) => wf_core::field::finish_arrival(&mut self.session.world),
                _ => {}
            }
        }
        match st {
            Ok(Status::Blocked) => self.running = true,
            Ok(Status::Finished) => {
                self.running = false;
                self.globals = self.rt.globals();
                self.session.world.story.g = self.globals.g;
                // a blocking event releases the field when it ends
                wf_core::field::set_busy(&mut self.session.world, false);
            }
            Err(e) => {
                self.errors.push(e);
                self.running = false;
                wf_core::field::set_busy(&mut self.session.world, false);
            }
        }
    }

    /// Start queued scripts while none is blocked.
    fn run_queue(&mut self) {
        while !self.running {
            let Some(job) = self.queue.pop_front() else { break };
            match &job.cause {
                Cause::Talk { npc } => self.hs.talk_target = Some(*npc),
                Cause::Break { x, y, .. } | Cause::Defeat { x, y, .. } => self.hs.loot_pos = (*x, *y),
                _ => {}
            }
            self.globals.g = self.session.world.story.g;
            self.rt.set_globals(self.globals.clone());
            self.cause = Some(job.cause.clone());
            match &job.run {
                Callback::Script(id) => {
                    let path = format!("scripts/inline/{id}.rhai");
                    let steps = self.stepped.contains(&path);
                    self.rt.start_unit(UnitRef::new(id.clone(), "", steps), [0; 4]);
                }
                Callback::Event(id) => match self.rt.event_id(id).ok_or(()) {
                    Ok(n) => self.rt.start_event(n, [0; 4]),
                    Err(_) => {
                        self.errors.push(format!("unknown event id {id}"));
                        continue;
                    }
                },
            }
            let st = self.with_host(|rt| rt.run().map_err(|e| e.to_string()));
            self.finish(st);
        }
    }

    pub fn world(&self) -> &World {
        &self.session.world
    }
}

impl Sim for StorySim {
    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>) {
        events.append(&mut self.init_events);
        self.presenter.before_step(&self.session.world);
        let mut frames = [InputFrame::default(); MAX_PLAYERS];
        for (k, f) in frames.iter_mut().enumerate() {
            let h = inputs.get(k).copied().unwrap_or(0);
            *f = InputFrame::from_held(h, self.prev[k]);
            self.prev[k] = h;
        }
        // the frame's start (deferred calls: drop scripts, kill XP, defeat scenes) runs its scripts before the
        // objects' update; a script started there takes its first wait from the next frame
        self.session.step_begin(frames);
        let pre: Vec<Event> = self.session.world.events.clone();
        let was_running = self.running;
        self.collect(&pre);
        self.run_queue();
        let fresh = !was_running && self.running;
        let n0 = self.session.world.events.len();
        self.session.step_update();
        // the blocked script resumes when its wait is over, then the scripts this frame's update started (NPC talk)
        // start; the trigger check's events start right after step_post (an event's first node runs a frame later:
        // the story playthrough lockstep, wf-script-core tests/playthrough.rs)
        let evs: Vec<Event> = self.session.world.events[n0..].to_vec();
        if self.running && !fresh {
            let mut hs = std::mem::take(&mut self.hs);
            let r = poll(&mut self.session.world, &mut hs);
            self.hs = hs;
            if let Some(v) = r {
                let st = self.with_host(|rt| rt.resume(v).map_err(|e| e.to_string()));
                self.finish(st);
                self.run_queue();
            }
        }
        self.collect(&evs);
        self.run_queue();
        let k = self.session.world.events.len();
        let post: Vec<Event> = self.session.step_post()[k..].to_vec();
        self.collect(&post);
        self.run_queue();
        wf_core::view::sim_events(&self.session.world, events);
    }

    fn view(&self, out: &mut RenderView) {
        wf_core::view::render_view(&self.session.world, out);
        self.presenter.apply(out);
    }
    fn frame(&self) -> u64 {
        self.session.world.frame as u64
    }
    fn state_hash(&self) -> u64 {
        self.session.world.state_hash()
    }
    fn players(&self) -> usize {
        self.session.world.players.len().max(1)
    }
}

/// `App` running story mode.
pub fn app(packs: Packs, area: (i32, i32), chars: Vec<i32>, seed: u32) -> Result<crate::App, String> {
    let s = StorySim::new(&packs, area, chars, seed)?;
    Ok(crate::App::new(packs, Box::new(s)))
}
