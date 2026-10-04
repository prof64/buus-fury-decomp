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
    /// the events' original step bytecode ((event, node) -> (ROM code address when stored uncompressed, code); the
    /// content's `scripts/bytecode.json`, `set_step_code`): the importer maps a running interpreter's PC to the
    /// natives the node has run
    pub step_code: StepCode,
    /// a text box the import opened at its node's start while the game's object is not made yet (the cut stands
    /// inside that iteration: real run 007, whole-run cut 756,613 - s003's node 2 after its first node's box): the
    /// portrait flag of `vram::text_open`; its tiles are taken after the allocator is rebuilt from RAM (the importer)
    pub text_open_due: Option<bool>,
}

/// the cause of an event the import continues: a fusion component's level-up hook (s003 cut while
/// `Story::levelup_char` is set - the importer's `levelup_fusion`) re-fuses when it ends; anything else has none
fn imported_cause(w: &World, event: u16) -> Option<Cause> {
    let entity = w.player_idx(0).map(|i| w.ents[i].id)?;
    (event == 3 && w.story.levelup_char.is_some()).then_some(Cause::LevelUp { entity })
}

type StepCode = BTreeMap<(u16, u16), (Option<u32>, Vec<u8>)>;
static STEP_CODE: std::sync::OnceLock<StepCode> = std::sync::OnceLock::new();
/// the inline scripts' bytecode by ROM address: (script id, code) - a script running at a cut outside any event
static INLINE_CODE: std::sync::OnceLock<BTreeMap<u32, (String, Vec<u8>)>> = std::sync::OnceLock::new();

/// the inline script whose code holds ROM address `pc`: (id, its address, code)
pub fn inline_at(pc: u32) -> Option<(String, u32, Vec<u8>)> {
    let m = INLINE_CODE.get()?;
    let (&a, (id, code)) = m.range(..=pc).next_back()?;
    (pc <= a + code.len() as u32).then(|| (id.clone(), a, code.clone()))
}

/// the events' step bytecode from the content's `scripts/bytecode.json`, for every host made after (see
/// `StoryHost::step_code`; the segments harness calls it once)
pub fn set_step_code(json: &[u8]) {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(json) else { return };
    let Some(evs) = v["events"].as_object() else { return };
    let hex = |s: &str| -> Vec<u8> { (0..s.len() / 2).filter_map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).ok()).collect() };
    let mut m = StepCode::new();
    for (name, e) in evs {
        let Some(ev) = name.strip_prefix('s').and_then(|x| x.parse::<u16>().ok()) else { continue };
        for n in e["nodes"].as_array().into_iter().flatten() {
            let (Some(node), Some(code)) = (n["node"].as_u64(), n["code"]["code"].as_str()) else { continue };
            let addr = n["code"]["code_addr"].as_str().and_then(|a| u32::from_str_radix(a.trim_start_matches("0x"), 16).ok());
            m.insert((ev, node as u16), (addr, hex(code)));
        }
    }
    let _ = STEP_CODE.set(m);
    let mut im = BTreeMap::new();
    for (id, e) in v["inline"].as_object().into_iter().flatten() {
        let (Some(addr), Some(code)) = (e["code_addr"].as_str().and_then(|a| u32::from_str_radix(a.trim_start_matches("0x"), 16).ok()), e["code"].as_str()) else { continue };
        im.insert(addr, (id.clone(), hex(code)));
    }
    let _ = INLINE_CODE.set(im);
}

impl StoryHost {

    /// Import: event `event` stopped inside node `node`'s code: the handler runs with its first `drop` natives
    /// dropped unexecuted (the game ran them before the cut; instant ones are queued so none runs twice), the rest
    /// run as usual (the host blocks on the first that waits)
    /// (`done`: the actions planned before the waiting one - their parameter writes are already in `params`)
    pub fn continue_mid_node(&mut self, w: &mut World, event: u16, node: u16, params: [i32; 4], drop: usize, done: usize) {
        let mut g = self.rt.globals();
        g.g = w.story.g;
        self.rt.set_globals(g);
        self.hs.in_unit = false;
        self.rt.start_event_at(event, node, params);
        self.started.push((self.frame, format!("s{event:03}@{node}")));
        self.cause = imported_cause(w, event);
        self.rt.with_thread(|t| {
            t.skip = drop;
            t.queue_instants = true;
            t.params_done = Some(done);
        });
        let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
        self.rt.with_thread(|t| {
            t.queue_instants = false;
            t.params_done = None;
        });
        self.finish(w, st);
    }
    /// Import: an inline script (an area / map-event script, no event graph) stopped in its code at a cut: it runs
    /// again from its start with its first `drop` actions dropped (the game ran them), the rest as usual
    pub fn continue_inline(&mut self, w: &mut World, id: &str, drop: usize) {
        let mut g = self.rt.globals();
        g.g = w.story.g;
        self.rt.set_globals(g);
        let steps = self.stepped.contains(&format!("scripts/inline/{id}.rhai"));
        self.hs.in_unit = true;
        self.rt.start_unit(UnitRef::new(id.to_string(), "", steps), [0; 4]);
        self.started.push((self.frame, id.to_string()));
        self.cause = None;
        self.rt.with_thread(|t| {
            t.skip = drop;
            t.queue_instants = true;
        });
        let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
        self.rt.with_thread(|t| t.queue_instants = false);
        self.finish(w, st);
        // (a map trigger's inline script that blocks holds the field like an event, `finish` for `Cause::Trigger`:
        // the landing script's player stands through its pan, wait and jump)
        if self.running && self.outer.is_empty() {
            wf_core::field::hold_nested(w);
        }
    }
    /// Import: an event stopped inside a node of an event it runs (`run_event`, a scene depth of 2): the outer node
    /// starts again with its first `drop_outer` actions dropped, its `run_event` runs (the inner event started before
    /// the cut: its scene start does not wait the frame a fresh one does), then the inner event's first `drop_inner`
    /// actions are dropped too. The parameters are the ones at the outer node's `run_event` (it writes the inner's).
    pub fn continue_mid_nested(&mut self, w: &mut World, outer: (u16, u16, [i32; 4]), drop_outer: usize, drop_inner: usize) {
        let mut g = self.rt.globals();
        g.g = w.story.g;
        self.rt.set_globals(g);
        self.hs.in_unit = false;
        self.rt.start_event_at(outer.0, outer.1, outer.2);
        self.started.push((self.frame, format!("s{:03}@{}", outer.0, outer.1)));
        self.cause = imported_cause(w, outer.0);
        self.rt.with_thread(|t| {
            t.skip = drop_outer + drop_inner;
            t.queue_instants = true;
            t.through_events = true;
        });
        let mut st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
        for _ in 0..4 {
            let at_enter = self.rt.with_thread(|t| matches!(t.blocked, Some(wf_script_rhai::Act::Scene { .. })));
            if !at_enter || st.is_err() {
                break;
            }
            self.hs.pending = None;
            st = self.with_host(w, |rt| rt.resume(None).map_err(|e| e.to_string()));
        }
        self.rt.with_thread(|t| {
            t.queue_instants = false;
            t.through_events = false;
        });
        self.finish(w, st);
    }
    /// `files` = the exported scripts (`wf_script::export::export(rom, &ScriptSet::load_all(rom))`)
    pub fn new(data: &Arc<GameData>, files: &BTreeMap<String, String>) -> Result<StoryHost, String> {
        let spare = Session::new_field(data.clone(), FieldConfig::default()).world;
        let shared = SharedHost(Rc::new(RefCell::new(CoreState { world: spare, story: HostState::default() })));
        let rt = Runtime::new(files, Box::new(shared.clone()))?;
        let stepped = files.iter().filter(|(_, src)| src.contains("fn step_0()")).map(|(p, _)| p.clone()).collect();
        let hs = HostState { nest_area_enter: true, ..HostState::default() };
        Ok(StoryHost { rt, shared, hs, queue: VecDeque::new(), running: false, cause: None, outer: vec![], stepped, errors: vec![], started: vec![], prev_held: 0, frame: 0, step_code: STEP_CODE.get().cloned().unwrap_or_default(), text_open_due: None })
    }

    /// a session around an imported / built world (no hook: the host runs the queue itself)
    pub fn session(data: &Arc<GameData>, world: World) -> Session {
        Session { data: data.clone(), world, config: Default::default(), prev: [0; MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() }
    }

    /// the running thread's frames, queue and blocked action (debugging: SEGMENT_HOST)
    pub fn thread_summary(&self) -> String {
        let t = self.rt.state();
        format!("blocked {:?} pending {:?} outer {} queue {}", t.blocked, self.hs.pending, self.outer.len(), t.queue.len())
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
        // a trigger's / talking NPC's event ended: its end node's value goes to the starter's resume slot unless a
        // warp / refresh_map_triggers / return to title set the no-save word (`0x08012546`)
        if matches!(st, Ok(Status::Finished)) && !self.hs.in_unit && !self.hs.no_resume_save {
            if let (Some(c @ (Cause::Trigger { .. } | Cause::Talk { .. })), Some(v)) = (self.cause.as_ref(), self.rt.with_thread(|t| t.event_result)) {
                wf_core::field::set_resume_node(w, c, v);
            }
        }
        let trigger = matches!(self.cause, Some(Cause::Trigger { .. }));
        if !matches!(st, Ok(Status::Blocked)) {
            match self.cause.take() {
                Some(Cause::Ai { entity }) => scene::ai_script_done(w, entity),
                // the load waits for its on-enter script
                Some(Cause::AreaEnter) => wf_core::field::finish_arrival(w),
                // a fusion component's level-up hook: the fusion is current again, re-fused
                // (the second component's XP comes now: its own hook runs next, in this frame)
                Some(Cause::LevelUp { .. }) => {
                    let from = w.events.len();
                    // (a plain level-up: the routine's new form object; a fusion component's hook re-fuses below)
                    if w.story.levelup_char.is_none() {
                        wf_core::player::levelup_hook_done(w);
                    }
                    wf_core::player::fusion_levelup_done(w);
                    let evs: Vec<Event> = w.events[from..].to_vec();
                    // (inside the XP routine's call: before the jobs already queued - the kill's on-defeat callback)
                    let n = self.queue.len();
                    self.collect(&evs);
                    let added: Vec<Job> = self.queue.drain(n..).collect();
                    for j in added.into_iter().rev() {
                        self.queue.push_front(j);
                    }
                }
                // an on-defeat callback that ran from the death effect and ended without starting an event (13-45's
                // Samurai once flag 156 is set): nothing held the frame, its update runs (not after the drop script
                // queued before the callback: that one must not release the hold)
                Some(Cause::Defeat { .. }) if matches!(st, Ok(Status::Finished)) && self.outer.is_empty() && !self.queue.iter().any(|j| matches!(j.cause, Cause::Defeat { .. })) => {
                    wf_core::field::release_unstarted_hold(w)
                }
                _ => {}
            }
        }
        match st {
            Ok(Status::Blocked) => {
                // a map trigger's inline script that blocks (prop_hide / prop_show at Capsule Corp's doors) holds the
                // field until it ends, like an event: the objects' think pauses, animations tick (4-20: the player's
                // walk stops 2 frames)
                if matches!(self.cause, Some(Cause::Trigger { .. })) && self.outer.is_empty() {
                    wf_core::field::hold_nested(w);
                }
                // an on-defeat inline script that blocks runs in the VM's nested loop like any callback: it holds the
                // field when no event of this frame does yet (the level-up popup #3 that ran before it has ended:
                // 12-28's Warlord - the other enemies do not update in the popup's exit frame)
                if matches!(self.cause, Some(Cause::Defeat { .. })) && self.outer.is_empty() && !w.field.as_ref().is_some_and(|f| f.busy) {
                    wf_core::field::set_busy(w, true);
                }
                self.running = true
            }
            Ok(Status::Finished) => {
                self.running = false;
                w.story.g = self.rt.globals().g;
                if self.outer.is_empty() {
                    // (a map trigger's inline script ends in its own call, not after the scripts: no form tick after
                    // it - 7-2 rE2__089 f1,492, s495's message in the trigger's run_scene)
                    if trigger {
                        wf_core::player::no_tick_after_scripts(w);
                    }
                    wf_core::field::set_busy(w, false);
                    // the actors a script outside any event commanded go back to their own updates when it ends
                    // (no end_event releases them: real route 1-5 -> 0-51 -> 18-1, the arrival's inline script
                    // play_anim'ed the Ghost and left it a puppet for good - its AI never ran, the game's draws its
                    // Chance 120 frames later)
                    if w.scene.depth == 0 && w.scene.ai_wait.is_empty() {
                        wf_core::scene::release_puppets(w);
                    }
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
        if !self.running || !matches!(self.hs.pending, Some(wf_script_core::Pending::Arrival | wf_script_core::Pending::Transition)) {
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

    /// the event of a scene header address (`// Event N (header 0xADDR)`), for imports
    pub fn event_of_header(&self, header: u32) -> Option<u16> {
        self.rt.library().headers.get(&header).copied()
    }

    /// the graph node that is running when a scene object's next state is `next` (a code node leading there)
    pub fn node_before(&self, event: u16, next: u16) -> Option<u16> {
        let g = self.rt.library().graphs.get(&event)?;
        let nx = |n: &wf_script_rhai::GNode| match n {
            wf_script_rhai::GNode::Code { next, .. } | wf_script_rhai::GNode::Say { next, .. } | wf_script_rhai::GNode::SayArgs { next, .. } | wf_script_rhai::GNode::Choice { next, .. } => Some(*next),
            _ => None,
        };
        g.iter().find(|(_, n)| matches!(n, wf_script_rhai::GNode::Code { .. }) && nx(n) == Some(next)).or_else(|| g.iter().find(|(_, n)| nx(n) == Some(next))).map(|(k, _)| *k)
    }

    /// Import: where event `event`'s node `node` stands relative to the load into `area`: its handler runs with
    /// every queued action dropped unexecuted (queries answer from `w`) until a warp (0x77) / warp_fade (0x76) into
    /// `area` is queued: Some((actions before it, native)); None = the node has no such load (it began after it: the
    /// first node of a scene a `jump` entered). The thread is left as it was.
    pub fn probe_event(&mut self, w: &mut World, event: u16, node: u16, params: [i32; 4], area: (i32, i32, i32)) -> Option<(usize, u8)> {
        self.probe_event_for(w, event, node, params, &|n, args| {
            matches!(n, 0x76 | 0x77) && args.len() >= 3 && (args[0], args[1]) == (area.0, area.1) && (args[2] == area.2 || args[2] == 0)
        })
    }

    /// `probe_event` for the first queued action `is_load(native, args)` accepts
    pub fn probe_event_for(&mut self, w: &mut World, event: u16, node: u16, params: [i32; 4], is_load: &dyn Fn(u8, &[i32]) -> bool) -> Option<(usize, u8)> {
        self.probe_queue(w, event, node, params, false, is_load)
    }

    /// `probe_event_for` with every instant native queued too (the order `continue_mid_node` drops in)
    pub fn probe_queued(&mut self, w: &mut World, event: u16, node: u16, params: [i32; 4], is_load: &dyn Fn(u8, &[i32]) -> bool) -> Option<(usize, u8)> {
        self.probe_queue(w, event, node, params, true, is_load)
    }

    fn probe_queue(&mut self, w: &mut World, event: u16, node: u16, params: [i32; 4], instants: bool, is_load: &dyn Fn(u8, &[i32]) -> bool) -> Option<(usize, u8)> {
        let saved = self.rt.state();
        // (the probe's queries must not move the generator: the placement run replays them; immediate natives with
        // effects - `save_field_on_next_scene` arms the host - ran before the cut in the game: their state is RAM's)
        let rng = w.rng.clone();
        let w0 = w.clone();
        let (hs0, story0) = (self.hs.clone(), w.story.clone());
        let mut g = self.rt.globals();
        g.g = w.story.g;
        self.rt.set_globals(g);
        self.rt.start_event_at(event, node, params);
        let mut done = 0usize;
        let mut out = None;
        for _ in 0..4096 {
            self.rt.with_thread(|t| {
                t.hold_queue = true;
                t.queue_instants = instants;
            });
            let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
            let (found, n, held) = self.rt.with_thread(|t| {
                let f = t.queue.iter().enumerate().find_map(|(k, a)| match a {
                    wf_script_rhai::Act::Native { native, args, .. } if is_load(*native, args) => Some((k, *native)),
                    _ => None,
                });
                (f, t.queue.len(), t.blocked.is_none() && !t.queue.is_empty())
            });
            if let Some((k, n)) = found {
                out = Some((done + k, n));
                break;
            }
            // (the node only goes past its queue by running through it; a frame-spanning wait the probe cannot pass)
            // (the node's handler is the top unit: once it is gone the next node would start - stop there)
            let in_node = self.rt.with_thread(|t| t.frames.iter().any(|f| matches!(f, wf_script_rhai::Frame::Unit { .. })));
            if st.is_err() || !held || !in_node {
                break;
            }
            done += n;
            self.rt.with_thread(|t| {
                t.hold_queue = false;
                t.skip = n;
            });
        }
        self.rt.restore(saved);
        // (the world as it was: an instant native at the handler's front ran during the probe - `begin_parallel`
        // left a batch open, `hide_hud` hid the HUD - mid_10000, s459 node 0)
        *w = w0;
        w.rng = rng;
        self.hs = hs0;
        w.story = story0;
        out
    }

    /// Import: continue event `event` from graph node `node` after the load into `area` (see `probe_event`): `skip`
    /// actions dropped (the game ran them), then the node runs on until its first wait; `blocked` = the load action
    /// itself is the one waiting (a warp_fade's: `Pending::Transition`, its fade-in still to run)
    pub fn continue_event(&mut self, w: &mut World, event: u16, node: u16, params: [i32; 4], skip: usize, blocked: Option<wf_script_core::Pending>) {
        let mut g = self.rt.globals();
        g.g = w.story.g;
        self.rt.set_globals(g);
        self.hs.in_unit = false;
        self.rt.start_event_at(event, node, params);
        self.started.push((self.frame, format!("s{event:03}@{node}")));
        self.cause = imported_cause(w, event);
        let (hs0, story0, batch0) = (self.hs.clone(), w.story.clone(), w.scene.director.batch);
        if let Some(pending) = blocked {
            // run the handler(s) up to the load action without executing anything, then block on it
            let mut left = skip;
            for _ in 0..4096 {
                self.rt.with_thread(|t| t.hold_queue = true);
                let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
                if st.is_err() {
                    break;
                }
                let n = self.rt.with_thread(|t| t.queue.len());
                if n > left {
                    self.rt.with_thread(|t| {
                        t.hold_queue = false;
                        for _ in 0..left {
                            t.queue.pop_front();
                        }
                        t.blocked = t.queue.pop_front();
                    });
                    // (the immediate natives the placement replayed ran before the cut: the host and the story as
                    // they were - RAM's)
                    self.hs = hs0;
                    w.story = story0;
                    // (a `begin_parallel` among them ran its batch open; its `end_parallel` was dropped with the other
                    // actions the game ran: the batch is closed in RAM - r062_001_20-6 +1,137, s473 node 12's step 0)
                    w.scene.director.batch = batch0;
                    self.hs.pending = Some(pending);
                    self.running = true;
                    return;
                }
                left -= n;
                self.rt.with_thread(|t| {
                    t.hold_queue = false;
                    t.skip = n;
                });
            }
            return;
        }
        // (actions in later handler steps: dropped as the steps queue them)
        self.rt.with_thread(|t| t.skip = skip);
        let st = self.with_host(w, |rt| rt.run().map_err(|e| e.to_string()));
        self.finish(w, st);
    }

    /// a load's scripts run nested over the blocked script that made the load (import: `nest_loads` for an imported
    /// warp); a warp's `Pending::Arrival` returns when they end (at once without any)
    pub fn nest_over(&mut self, w: &mut World, evs: &[Event]) {
        let runs: Vec<Job> = evs.iter().filter_map(|e| match e {
            Event::Field(FieldEvent::Run { run, cause, .. }) => Some(Job { run: run.clone(), cause: cause.clone() }),
            _ => None,
        }).collect();
        if !runs.is_empty() {
            let o = Outer { thread: self.rt.state(), pending: self.hs.pending.take(), from_wait: std::mem::take(&mut self.hs.from_wait), cause: self.cause.take() };
            self.outer.push(o);
            self.running = false;
            for j in runs.into_iter().rev() {
                self.queue.push_front(j);
            }
            self.run_queue(w);
        } else {
            self.resume_outer(w);
        }
    }

    /// run a load's scripts nested inside a script (import of a warp inside an event): the field stays held when they
    /// end, as under the warp that made the load
    pub fn run_nested(&mut self, w: &mut World, evs: &[Event]) {
        self.outer.push(Outer { thread: self.rt.state(), pending: None, from_wait: false, cause: None });
        self.collect(evs);
        self.run_queue(w);
        // (still running nested: it stays on top of the placeholder; ended: the placeholder came back - drop it)
        if self.outer.is_empty() {
            self.running = false;
        } else if !self.running {
            self.outer.pop();
        }
    }

    /// the blocked script goes on if its wait is already over (the rest of an imported frame)
    pub fn settle(&mut self, w: &mut World) {
        self.resume_blocked(w);
    }

    /// run queued scripts until one blocks or the queue is empty
    pub fn run_queue(&mut self, w: &mut World) {
        let k = self.frame;
        while !self.running {
            let Some(job) = self.queue.pop_front() else { break };
            self.hs.talk_event = matches!(job.cause, Cause::Talk { .. });
            // (a death, with its loot block - not a world-map marker's TriggerSelf)
            self.hs.defeat_job = matches!(job.cause, Cause::Defeat { loot: Some(_), .. });
            match &job.cause {
                Cause::Talk { npc } => self.hs.talk_target = Some(*npc),
                Cause::Break { x, y, .. } | Cause::Defeat { x, y, .. } => self.hs.loot_pos = (*x, *y),
                _ => {}
            }
            wf_core::field::start_cause(w, &job.cause);
            let mut g = self.rt.globals();
            g.g = w.story.g;
            self.rt.set_globals(g);
            self.cause = Some(job.cause.clone());
            match &job.run {
                Callback::Script(id) => {
                    let path = format!("scripts/inline/{id}.rhai");
                    let steps = self.stepped.contains(&path);
                    self.started.push((k, id.clone()));
                    self.hs.in_unit = true;
                    self.rt.start_unit(UnitRef::new(id.clone(), "", steps), [0; 4]);
                }
                Callback::Event(id) => match self.rt.event_id(id) {
                    Some(n) => {
                        self.started.push((k, format!("s{n:03}")));
                        {
                        self.hs.in_unit = false;
                        // an outermost event's start clears the no-save word (0x080124B0); a trigger / talking NPC
                        // starts its event at the exit value of its last run (its resume slot, wf-core `resume_node`)
                        if self.outer.is_empty() {
                            self.hs.no_resume_save = false;
                        }
                        if matches!(job.cause, Cause::Ai { .. }) && self.outer.is_empty() {
                            wf_core::player::ai_event_started(w);
                        }
                        let node = wf_core::field::resume_node(w, &job.cause);
                        // an AI's RunScript hands the event the enemy's +0x214 block as its parameters
                        let params = match &job.cause {
                            Cause::Ai { entity } => [w.enemy_by_id(*entity).map(|(_, e)| e.p214).unwrap_or(0), 0, 0, 0],
                            _ => [0; 4],
                        };
                        self.rt.start_event_from(n, node, params, 0)
                    }
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
            // (only the callback's first run, the frame its enemy dies: a prop command after a wait is a plain one)
            self.hs.defeat_job = false;
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
        // a lag frame the sim makes itself (`Options::sim_lag`): no input is read, no logic runs
        if sess.lag_step() {
            return;
        }
        let mut inputs = [InputFrame::default(); MAX_PLAYERS];
        inputs[0] = InputFrame::from_held(keys, self.prev_held);
        self.prev_held = keys;
        self.step_inputs(sess, inputs);
    }

    /// the world's creation events (the first area's load scripts: gates, doors, bridges) before frame 0
    pub fn start(&mut self, sess: &mut Session) {
        let evs: Vec<Event> = sess.world.events.clone();
        self.collect(&evs);
        self.run_queue(&mut sess.world);
    }

    /// one logic frame with every player's input frame (the app's story mode: wf-app `StorySim`)
    pub fn step_inputs(&mut self, sess: &mut Session, inputs: [InputFrame; MAX_PLAYERS]) {
        if sess.lag_step() {
            return;
        }
        self.frame += 1;
        // the frame's start (deferred calls: drop scripts, kill XP): their scripts run before the objects' update
        sess.step_begin(inputs);
        let pre: Vec<Event> = sess.world.events.clone();
        self.collect(&pre);
        let before = self.started.len();
        self.run_queue(&mut sess.world);
        // (the title's New Game: the prologue hook #4 starts in the New Game iteration, whose lag frames come before
        // this first field frame, so its scene enter is over now and its 64-frame fade (mode 0x0804BBA4) runs from
        // this frame - the whole run from the reset: f854 .. f917, the load at f918; as the playthrough harness. The
        // Select picture under it shows each level a frame later: menu::view)
        let new_game = pre.iter().any(|e| matches!(e, Event::Field(FieldEvent::Run { run: Callback::Event(id), cause: Cause::Menu, .. }) if id == "s004"));
        let fresh = self.started.len() > before && !new_game;
        let n0 = sess.world.events.len();
        sess.step_update();
        // the countdown (GD+0x4D4) counts the frames whose object pass sees the field free; our frame order (update,
        // then the script's resume, then the post step's trigger check) is settled after them below
        let cd0 = self.hs.timers.countdown;
        let free_at_tick = wf_script_core::countdown_counts(&sess.world);
        // (an event the countdown's hook started runs inside the countdown object's update: its frame is counted)
        let hook_event = self.hs.timer_event;
        wf_script_core::tick_timers(&mut sess.world, &mut self.hs);
        // the running script resumes from its wait (not one this frame's start began: its first wait counts from the
        // next frame), then the scripts the update started (NPC talk)
        let evs: Vec<Event> = sess.world.events[n0..].to_vec();
        if !fresh {
            self.resume_blocked(&mut sess.world);
        }
        self.collect(&evs);
        self.run_queue(&mut sess.world);
        // (an event that ended now - at the top of the game's frame - leaves the field free for this frame's count:
        // Airship db5_023 f569, the level-up popup's last box)
        let ticked_late = !free_at_tick && !hook_event && self.hs.timers.countdown_on && self.hs.timers.countdown > 1 && wf_script_core::countdown_counts(&sess.world);
        if ticked_late {
            self.hs.timers.countdown -= 1;
        }
        let cd1 = self.hs.timers.countdown;
        let free_mid = wf_script_core::countdown_counts(&sess.world);
        // the trigger check (post step) starts its event in this frame too
        let n = sess.world.events.len();
        sess.step_post();
        // a warp_fade's load (the field's fade mode, post step): its on-enter script runs nested now
        self.nest_loads(&mut sess.world, n);
        let post: Vec<Event> = sess.world.events[n.min(sess.world.events.len())..].to_vec();
        self.collect(&post);
        self.run_queue(&mut sess.world);
        // ... and not in the frame the trigger check holds the field - a script it starts (Airship db5_022 f51, the
        // lift's inline trigger) or an exit's walk-out (db5_028 f282): the countdown's object sees the hold
        if free_mid && !wf_script_core::countdown_counts(&sess.world) && cd1 < cd0 && cd1 > 0 {
            self.hs.timers.countdown = cd0;
        }
        // the game runs a frame's scripts before its objects: a HUD a script showed gets its update (the
        // stat-points mark) in the same frame, after the script's own objects (segments sb 20-2 f1574, 20-5 f5212)
        wf_core::vram::stat_mark(&mut sess.world);
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
