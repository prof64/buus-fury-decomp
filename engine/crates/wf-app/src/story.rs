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

use crate::battle::PacksSource;

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
    banner: BannerTracker,
    shown: ShownCamera,
    /// the OBJ layer shows the previous frame's build (OAM is copied at the next VBlank; verified per frame
    /// against the emulator in field_parity): the sprites as they were before this step
    prev_obj: Option<ObjLayer>,
    /// show the previous frame's camera (off: wf-core's field camera already matches the screen, field_parity)
    pub lag_camera: bool,
}

/// `areas.json` key `"W-A"` -> (world, area)
pub fn parse_area(s: &str) -> Option<(i32, i32)> {
    let (w, a) = s.split_once('-')?;
    Some((w.trim().parse().ok()?, a.trim().parse().ok()?))
}

impl StorySim {
    /// A new game at `area` (`"1-1"` = Snakeway, where the game's play starts), or `story` to continue.
    pub fn new(packs: &Packs, area: (i32, i32), chars: Vec<i32>, seed: u32) -> Result<StorySim, String> {
        let data: Arc<GameData> = Arc::new(GameData::load(&PacksSource(packs)).map_err(|e| e.to_string())?);
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
        self.banner.cur = None;
        self.banner.remember_current(&self.session.world);
        self.session.world.scene.banner = None;
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
            banner: BannerTracker::new(packs),
            shown: ShownCamera::default(),
            lag_camera: false,
            prev_obj: None,
        };
        // the first area's load scripts (gates, doors, bridges) before frame 0
        let evs: Vec<Event> = s.session.world.events.clone();
        s.banner.step(&evs);
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
            if let Some(Cause::Ai { entity }) = self.cause.take() {
                wf_core::scene::ai_script_done(&mut self.session.world, entity);
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

impl StorySim {
    /// the state's own picture (before the OBJ-layer lag)
    fn view_now(&self, out: &mut RenderView) {
        wf_core::view::render_view(&self.session.world, out);
        patch_view(&self.session.world, out);
        self.banner.apply(out);
    }
}

/// Everything the game draws as OBJs (sprites, HUD, area banner, dialogue box). The picture shows the previous
/// frame's OBJ build (shadow OAM / OBJ tiles are copied at the next VBlank) over the current frame's BG scroll and
/// palette — verified frame by frame against the emulator (field_parity).
#[derive(Clone, Debug, Default)]
pub struct ObjLayer {
    pub sprites: Vec<wf_view::SpriteDraw>,
    pub hud: Vec<wf_view::HudView>,
    pub banner: Option<wf_view::BannerView>,
    pub dialog: Option<wf_view::DialogView>,
    pub dialog_anim: Option<wf_view::DialogAnim>,
}

impl ObjLayer {
    pub fn of(v: &RenderView) -> ObjLayer {
        ObjLayer { sprites: v.sprites.clone(), hud: v.hud.clone(), banner: v.banner.clone(), dialog: v.dialog.clone(), dialog_anim: v.dialog_anim }
    }
    pub fn apply(&self, out: &mut RenderView) {
        out.sprites = self.sprites.clone();
        out.hud = self.hud.clone();
        out.banner = self.banner.clone();
        out.dialog = self.dialog.clone();
        out.dialog_anim = self.dialog_anim;
    }
}

impl Sim for StorySim {
    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>) {
        events.append(&mut self.init_events);
        self.shown.before_step(&self.session.world);
        let mut pre = RenderView::default();
        self.view_now(&mut pre);
        self.prev_obj = Some(ObjLayer::of(&pre));
        let mut frames = [InputFrame::default(); MAX_PLAYERS];
        for (k, f) in frames.iter_mut().enumerate() {
            let h = inputs.get(k).copied().unwrap_or(0);
            *f = InputFrame::from_held(h, self.prev[k]);
            self.prev[k] = h;
        }
        self.session.step_world(frames);
        // the blocked script resumes when its wait is over, then the scripts this frame's update started (NPC talk)
        // start; the trigger check's events start right after step_post (an event's first node runs a frame later:
        // the story playthrough lockstep, wf-script-core tests/playthrough.rs)
        let evs: Vec<Event> = self.session.world.events.clone();
        if self.running {
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
        let all = self.session.world.events.clone();
        self.banner.step(&all);
        wf_core::view::sim_events(&self.session.world, events);
    }

    fn view(&self, out: &mut RenderView) {
        self.view_now(out);
        if self.lag_camera {
            self.shown.apply(out);
        }
        if let Some(p) = &self.prev_obj {
            p.apply(out);
        }
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

/// Frontend-side completions of wf-core's view (until the core's `render_view` provides them): the area
/// transition palette fade (field mode FadeOut / FadeIn: brightness 256 -> 0 by 8 per frame, `0x08006D98` /
/// `0x08006DD6`).
/// The dialogue box animation from the core's dialog state (open zoom over the 7 `DIALOG_OPEN` frames, close zoom,
/// typewriter), measured against the emulator's OAM (affine PA = 2048 / scale) and screen.
pub fn dialog_anim(w: &World) -> Option<wf_view::DialogAnim> {
    use wf_core::scene::DialogPhase as P;
    let d = w.scene.dialog.as_ref()?;
    let (o, c) = (DIALOG_OPEN_OFF.load(std::sync::atomic::Ordering::Relaxed), DIALOG_CLOSE_OFF.load(std::sync::atomic::Ordering::Relaxed));
    let (scale, typed) = match d.phase {
        P::Opening => ((d.t + o).clamp(1, 8), Some(0)),
        P::Shown => (8, Some(d.typed.max(0) as u32)),
        P::Closing => (8 - (d.t + c), None),
        P::Closed => return None,
    };
    if scale < 1 {
        return Some(wf_view::DialogAnim { scale: 0xFF, typed });
    }
    Some(wf_view::DialogAnim { scale: scale.min(8) as u8, typed })
}

/// calibration of [`dialog_anim`] (field_parity)
pub static DIALOG_OPEN_OFF: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);
pub static DIALOG_CLOSE_OFF: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub fn patch_view(w: &World, out: &mut RenderView) {
    if out.dialog_anim.is_none() {
        out.dialog_anim = dialog_anim(w);
    }
    if out.palette_fade.is_none() {
        if let Some(f) = w.field.as_ref() {
            let level = match f.mode {
                wf_core::field::Mode::FadeOut { k, .. } => (8 * k).clamp(0, 256),
                wf_core::field::Mode::FadeIn { k } => (256 - 8 * k).clamp(0, 256),
                wf_core::field::Mode::Normal => 0,
            };
            if level > 0 {
                out.palette_fade = Some(wf_view::PaletteFade { level: level as u16, target: 0 });
            }
        }
    }
    // areas without combat (towns, offices) show the skill icon greyed out (emulator HUD)
    if w.field.as_ref().is_some_and(|f| !f.combat) {
        for h in out.hud.iter_mut() {
            h.skill_alt = true;
        }
    }
    // tile animations run on the map's own timers, which start at the area load
    if let (Some(f), Some(m)) = (w.field.as_ref(), out.map.as_mut()) {
        m.anim_frame = w.frame.saturating_sub(f.entered).wrapping_add(ANIM_PHASE);
    }
}

/// The picture shows the previous frame's camera (the game writes BG scroll and builds OAM before its camera
/// update; verified against the emulator): remember the cameras before a step, show them after it.
#[derive(Clone, Debug, Default)]
pub struct ShownCamera(Vec<wf_view::CameraView>);

impl ShownCamera {
    /// call before stepping
    pub fn before_step(&mut self, w: &World) {
        let mut v = RenderView::default();
        wf_core::view::render_view(w, &mut v);
        self.0 = v.cameras;
    }
    pub fn apply(&self, out: &mut RenderView) {
        for (c, s) in out.cameras.iter_mut().zip(&self.0) {
            *c = *s;
        }
    }
}

/// frames between the core's load frame and the map animation timers' start (measured, field_parity)
pub const ANIM_PHASE: u32 = 0;

/// The area-name banner (docs/frontend.md §7) kept by the frontend until wf-core's view provides it: started when
/// an area load finishes with a name id different from the last one shown (not 0; the world map only records it),
/// `t` = frames since the first frame after the load, gone after t = 238.
#[derive(Clone, Debug, Default)]
pub struct BannerTracker {
    names: std::collections::HashMap<String, (i64, String)>,
    last_id: i64,
    cur: Option<(String, u32)>,
    loaded: bool,
    /// added to `t` when drawing (calibration)
    pub t_offset: i32,
}

impl BannerTracker {
    pub fn new(packs: &Packs) -> BannerTracker {
        let mut names = std::collections::HashMap::new();
        if let Ok(j) = packs.value("areas.json") {
            for a in j["areas"].as_array().into_iter().flatten() {
                if let Some(k) = a["key"].as_str() {
                    names.insert(k.to_string(), (a["name_string_id"].as_i64().unwrap_or(0), a["name"].as_str().unwrap_or("").to_string()));
                }
            }
        }
        BannerTracker { names, ..Default::default() }
    }
    /// call after every step with the step's events
    pub fn step(&mut self, evs: &[Event]) {
        if let Some((_, t)) = self.cur.as_mut() {
            *t += 1;
            if *t > 238 {
                self.cur = None;
            }
        }
        for e in evs {
            if let Event::Field(FieldEvent::AreaEnter { area, .. }) = e {
                // the load deletes every object, the banner too
                self.cur = None;
                let key = format!("{}-{}", area.world, area.area);
                let (id, name) = self.names.get(&key).cloned().unwrap_or((0, String::new()));
                if key == "0-50" {
                    self.last_id = id;
                } else if id != 0 && id != self.last_id {
                    self.last_id = id;
                    if std::env::var("DEBUG_BANNER").is_ok() {
                        eprintln!("banner start {key} id {id} last {}", self.last_id);
                    }
                    self.cur = Some((name, 0));
                    self.loaded = true;
                }
            }
        }
    }
    /// the current area's name counts as already shown (a session that starts mid-area)
    pub fn remember_current(&mut self, w: &World) {
        if let Some(f) = w.field.as_ref() {
            let key = format!("{}-{}", f.area.world, f.area.area);
            self.last_id = self.names.get(&key).map(|x| x.0).unwrap_or(0);
        }
    }
    pub fn apply(&self, out: &mut RenderView) {
        if out.banner.is_none() {
            if let Some((text, t)) = &self.cur {
                let t = (*t as i64 + self.t_offset as i64).max(0) as u32;
                if t <= 238 {
                    out.banner = Some(wf_view::BannerView { text: text.clone(), t });
                }
            }
        }
    }
}

/// `App` running story mode.
pub fn app(packs: Packs, area: (i32, i32), chars: Vec<i32>, seed: u32) -> Result<crate::App, String> {
    let s = StorySim::new(&packs, area, chars, seed)?;
    Ok(crate::App::new(packs, Box::new(s)))
}
