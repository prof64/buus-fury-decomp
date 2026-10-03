//! The story host: the per-frame script loop of the story playthrough (`wf-script-core/tests/playthrough.rs`,
//! wf-app `StorySim`) as a library. One Rhai runtime on the shared host, the field's `FieldEvent::Run` queue.
//!
//! Host order per logic frame: `Session::step_begin` -> run the scripts it queued (drop scripts, kill-XP level-up,
//! defeat scenes; a script started there takes its first wait from the next frame) -> `step_update` -> resume the
//! blocked script -> start the update's scripts -> `step_post` -> start the trigger check's scripts. `Cause::Ai` ->
//! `scene::ai_script_done`, `Cause::AreaEnter` -> `field::finish_arrival` when those scripts end.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use wf_core::content::GameData;
use wf_core::field::{Cause, FieldConfig, FieldEvent};
use wf_core::fieldcontent::Callback;
use wf_core::input::InputFrame;
use wf_core::scene;
use wf_core::session::Session;
use wf_core::world::{Event, World, MAX_PLAYERS};
use wf_script_core::{poll, CoreState, HostState, SharedHost};
use wf_script_rhai::{Runtime, Status, UnitRef};

#[derive(Clone)]
struct Job {
    run: Callback,
    cause: Cause,
}

/// a blocked script suspended while a nested one runs (an area load's on-enter script inside a script's warp)
#[derive(Clone)]
struct Outer {
    thread: wf_script_rhai::ThreadState,
    pending: Option<wf_script_core::Pending>,
    from_wait: bool,
    cause: Option<Cause>,
}

pub struct StoryHost {
    rt: Runtime,
    shared: SharedHost,
    /// the running script's host state (pending wait, presentation log `present`, ...)
    pub hs: HostState,
    queue: VecDeque<Job>,
    running: bool,
    cause: Option<Cause>,
    /// scripts suspended under the running one (innermost last)
    outer: Vec<Outer>,
    stepped: BTreeSet<String>,
    /// script errors (compile / run)
    pub errors: Vec<String>,
    /// (logic frame index, script id) of every script started
    pub started: Vec<(usize, String)>,
    /// held keys of the previous logic frame (input edges)
    pub prev_held: u16,
    /// logic frames stepped
    pub frame: usize,
}

impl StoryHost {
    /// `files` = the exported scripts (`wf_script::export::export(rom, &ScriptSet::load_all(rom))`)
    pub fn new(data: &Arc<GameData>, files: &BTreeMap<String, String>) -> Result<StoryHost, String> {
        let spare = Session::new_field(data.clone(), FieldConfig::default()).world;
        let shared = SharedHost(Rc::new(RefCell::new(CoreState { world: spare, story: HostState::default() })));
        let rt = Runtime::new(files, Box::new(shared.clone()))?;
        let stepped = files.iter().filter(|(_, src)| src.contains("fn step_0()")).map(|(p, _)| p.clone()).collect();
        let hs = HostState { nest_area_enter: true, ..HostState::default() };
        Ok(StoryHost { rt, shared, hs, queue: VecDeque::new(), running: false, cause: None, outer: vec![], stepped, errors: vec![], started: vec![], prev_held: 0, frame: 0 })
    }

    /// a session around an imported / built world (no hook: the host runs the queue itself)
    pub fn session(data: &Arc<GameData>, world: World) -> Session {
        Session { data: data.clone(), world, config: Default::default(), prev: [0; MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() }
    }

    /// a script is blocked (waiting for frames / input / a fade ...)
    pub fn running(&self) -> bool {
        self.running
    }
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    fn with_host<T>(&mut self, w: &mut World, op: impl FnOnce(&mut Runtime) -> T) -> T {
        {
            let mut st = self.shared.0.borrow_mut();
            std::mem::swap(&mut st.world, w);
            std::mem::swap(&mut st.story, &mut self.hs);
        }
        let r = op(&mut self.rt);
        let mut st = self.shared.0.borrow_mut();
        std::mem::swap(&mut st.world, w);
        std::mem::swap(&mut st.story, &mut self.hs);
        r
    }

    /// queue the `Run` events of `evs`
    pub fn collect(&mut self, evs: &[Event]) {
        for e in evs {
            if let Event::Field(FieldEvent::Run { run, cause, .. }) = e {
                self.queue.push_back(Job { run: run.clone(), cause: cause.clone() });
            }
        }
    }

    fn finish(&mut self, w: &mut World, st: Result<Status, String>) {
        if !matches!(st, Ok(Status::Blocked)) {
            match self.cause.take() {
                Some(Cause::Ai { entity }) => scene::ai_script_done(w, entity),
                // the load waits for its on-enter script
                Some(Cause::AreaEnter) => wf_core::field::finish_arrival(w),
                _ => {}
            }
        }
        match st {
            Ok(Status::Blocked) => self.running = true,
            Ok(Status::Finished) => {
                self.running = false;
                w.story.g = self.rt.globals().g;
                if self.outer.is_empty() {
                    wf_core::field::set_busy(w, false);
                }
            }
            Err(e) => {
                self.errors.push(e);
                self.running = false;
                if self.outer.is_empty() {
                    wf_core::field::set_busy(w, false);
                }
            }
        }
        // a nested script ended: the suspended one is the running (blocked) script again
        if !self.running {
            if let Some(o) = self.outer.pop() {
                self.rt.restore(o.thread);
                self.hs.pending = o.pending;
                self.hs.from_wait = o.from_wait;
                self.cause = o.cause;
                self.running = true;
            }
        }
    }

    /// The game runs an area load's on-enter script inside EnterArea, nested in the script whose `warp` loads the
    /// area (Other World travel s511 / s512, which no event holds; `Pending::Arrival`). A script call that left such
    /// a load (its on-enter `Run` among the events since `from`) runs that script now on top of it; when it ends
    /// the suspended script resumes at once (the game's warp returns in the same frame).
    fn nest_loads(&mut self, w: &mut World, from: usize) {
        if !self.running || !matches!(self.hs.pending, Some(wf_script_core::Pending::Arrival)) {
            return;
        }
        let k = (from..w.events.len()).find(|&k| matches!(&w.events[k], Event::Field(FieldEvent::Run { cause: Cause::AreaEnter, .. })));
        let Some(k) = k else { return };
        let Event::Field(FieldEvent::Run { run, .. }) = w.events.remove(k) else { return };
        let o = Outer { thread: self.rt.state(), pending: self.hs.pending.take(), from_wait: std::mem::take(&mut self.hs.from_wait), cause: self.cause.take() };
        self.outer.push(o);
        self.running = false;
        self.queue.push_front(Job { run, cause: Cause::AreaEnter });
        self.run_queue(w);
    }

    /// after a nested script ended: the suspended script continues now if its wait is over
    fn resume_outer(&mut self, w: &mut World) {
        if self.running && matches!(self.hs.pending, Some(wf_script_core::Pending::Arrival)) {
            self.resume_blocked(w);
        }
    }

    /// start event graph `ev` now (the game's new-game hook starts the prologue #4 like this)
    pub fn start_event(&mut self, w: &mut World, ev: u16) {
        let mut g = self.rt.globals();
        g.g = w.story.g;
        self.rt.set_globals(g);
        self.rt.start_event(ev, [0; 4]);
        let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
        self.finish(w, st);
    }

    /// run queued scripts until one blocks or the queue is empty
    pub fn run_queue(&mut self, w: &mut World) {
        let k = self.frame;
        while !self.running {
            let Some(job) = self.queue.pop_front() else { break };
            match &job.cause {
                Cause::Talk { npc } => self.hs.talk_target = Some(*npc),
                Cause::Break { x, y, .. } | Cause::Defeat { x, y, .. } => self.hs.loot_pos = (*x, *y),
                _ => {}
            }
            let mut g = self.rt.globals();
            g.g = w.story.g;
            self.rt.set_globals(g);
            self.cause = Some(job.cause.clone());
            match &job.run {
                Callback::Script(id) => {
                    let path = format!("scripts/inline/{id}.rhai");
                    let steps = self.stepped.contains(&path);
                    self.started.push((k, id.clone()));
                    self.rt.start_unit(UnitRef::new(id.clone(), "", steps), [0; 4]);
                }
                Callback::Event(id) => match self.rt.event_id(id) {
                    Some(n) => {
                        self.started.push((k, format!("s{n:03}")));
                        self.rt.start_event(n, [0; 4])
                    }
                    None => {
                        self.errors.push(format!("unknown event id {id}"));
                        continue;
                    }
                },
            }
            let from = w.events.len();
            let depth = self.outer.len();
            let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
            self.finish(w, st);
            self.nest_loads(w, from);
            if self.outer.len() < depth {
                self.resume_outer(w);
            }
        }
    }

    fn resume_blocked(&mut self, w: &mut World) {
        if !self.running {
            return;
        }
        let mut hs = std::mem::take(&mut self.hs);
        let r = poll(w, &mut hs);
        self.hs = hs;
        if let Some(v) = r {
            let from = w.events.len();
            let depth = self.outer.len();
            let st = self.with_host(w, |rt| rt.resume(v).map_err(|e| e.to_string()));
            self.finish(w, st);
            self.nest_loads(w, from);
            if self.outer.len() < depth {
                self.resume_outer(w);
            }
            self.run_queue(w);
        }
    }

    /// one logic frame with the leader's held keys (the playthrough's host order)
    pub fn step(&mut self, sess: &mut Session, keys: u16) {
        self.frame += 1;
        let mut inputs = [InputFrame::default(); MAX_PLAYERS];
        inputs[0] = InputFrame::from_held(keys, self.prev_held);
        self.prev_held = keys;
        // the frame's start (deferred calls: drop scripts, kill XP): their scripts run before the objects' update
        sess.step_begin(inputs);
        let pre: Vec<Event> = sess.world.events.clone();
        self.collect(&pre);
        let before = self.started.len();
        self.run_queue(&mut sess.world);
        let fresh = self.started.len() > before;
        let n0 = sess.world.events.len();
        sess.step_update();
        // the running script resumes from its wait (not one this frame's start began: its first wait counts from the
        // next frame), then the scripts the update started (NPC talk)
        let evs: Vec<Event> = sess.world.events[n0..].to_vec();
        if !fresh {
            self.resume_blocked(&mut sess.world);
        }
        self.collect(&evs);
        self.run_queue(&mut sess.world);
        // the trigger check (post step) starts its event in this frame too
        let n = sess.world.events.len();
        let post: Vec<Event> = sess.step_post()[n..].to_vec();
        self.collect(&post);
        self.run_queue(&mut sess.world);
    }
}

/// a [`StoryHost`]'s between-frames state (the script thread, globals, host state, queue)
#[derive(Clone)]
pub struct HostSnapshot {
    thread: wf_script_rhai::ThreadState,
    globals: wf_script::vm::Globals,
    hs: HostState,
    queue: VecDeque<Job>,
    running: bool,
    cause: Option<Cause>,
    outer: Vec<Outer>,
    errors: usize,
    started: usize,
    prev_held: u16,
    frame: usize,
}

impl StoryHost {
    /// run the `Run` events of `evs` (in order) on `w`; false = a script is still blocked / queued afterwards
    pub fn run_events(&mut self, w: &mut World, evs: &[Event]) -> bool {
        self.collect(evs);
        self.run_queue(w);
        !self.running && self.queue.is_empty()
    }
    pub fn snapshot(&self) -> HostSnapshot {
        HostSnapshot {
            thread: self.rt.state(),
            globals: self.rt.globals(),
            hs: self.hs.clone(),
            queue: self.queue.clone(),
            running: self.running,
            cause: self.cause.clone(),
            outer: self.outer.clone(),
            errors: self.errors.len(),
            started: self.started.len(),
            prev_held: self.prev_held,
            frame: self.frame,
        }
    }
    pub fn restore(&mut self, s: &HostSnapshot) {
        self.rt.restore(s.thread.clone());
        self.rt.set_globals(s.globals.clone());
        self.hs = s.hs.clone();
        self.queue = s.queue.clone();
        self.running = s.running;
        self.cause = s.cause.clone();
        self.outer = s.outer.clone();
        self.errors.truncate(s.errors);
        self.started.truncate(s.started);
        self.prev_held = s.prev_held;
        self.frame = s.frame;
    }
}
