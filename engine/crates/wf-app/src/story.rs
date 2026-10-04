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

use std::collections::BTreeMap;
use std::sync::Arc;

use wf_content::Packs;
use wf_core::content::GameData;
use wf_core::field::FieldConfig;
use wf_core::input::InputFrame;
use wf_core::session::Session;
use wf_core::world::{World, MAX_PLAYERS};
use wf_view::{Buttons, RenderView, Sim, SimEvent};

/// the GBA's load lengths per destination area (display frames without a logic frame; measured)
const LOAD_LAG: &str = include_str!("load_lag.json");

pub struct StorySim {
    pub session: Session,
    /// the script host: the same glue the segment harness replays the real game with (`wf_import::StoryHost`:
    /// nested on-enter scripts inside a script's warp, defeat-callback holds, the countdown ...)
    host: wf_import::StoryHost,
    prev: [u16; MAX_PLAYERS],
    /// scripts that failed to compile / run (shown by the shells' logs)
    pub errors: Vec<String>,
    /// audio of the initial load (song), emitted with the first step
    init_events: Vec<SimEvent>,
    /// the screen's one-frame lags (wf-core view::Presenter; the core's view does banner, fades, tile timers, the
    /// greyed icon, the dialogue animation)
    presenter: wf_core::view::Presenter,
    /// steps taken (`Sim::frame`: replays, rewind and rollback index their input logs by it - `World.frame` counts
    /// object passes and stands still in menus, holds and loads)
    steps: u64,
}

/// [`StorySim::save_mem`]'s state
#[derive(Clone)]
pub struct StoryState {
    pub session: wf_core::session::Snapshot,
    pub host: wf_import::HostSnapshot,
    pub prev: [u16; MAX_PLAYERS],
    pub steps: u64,
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
        let mut session = Session::new_field(Arc::clone(&data), cfg);
        // the app recreates no hardware slowdown: one step per game logic iteration (`Options::no_lag`)
        session.world.opts.no_lag = true;
        Self::with_session(packs, session)
    }

    /// The in-process snapshot (rollback netcode, rewind): the session, the script host's between-frames state, the
    /// input edges and the step count - `Sim::snapshot` is its byte form.
    pub fn save_mem(&self) -> StoryState {
        StoryState { session: self.session.snapshot(), host: self.host.snapshot(), prev: self.prev, steps: self.steps }
    }
    pub fn load_mem(&mut self, s: &StoryState) {
        self.session.restore(&s.session);
        self.host.restore(&s.host);
        self.prev = s.prev;
        self.steps = s.steps;
        self.init_events.clear();
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
        wf_core::field::attach_host(&mut session.world, true);
        let mut host = wf_import::StoryHost::new(&session.data, &files)?;
        let mut init_events = Vec::new();
        wf_core::view::sim_events(&session.world, &mut init_events);
        // the first area's load scripts (gates, doors, bridges) before frame 0
        host.start(&mut session);
        let errors = host.errors.clone();
        Ok(StorySim { session, host, prev: [0; MAX_PLAYERS], errors, init_events, presenter: Default::default(), steps: 0 })
    }

    /// Story mode from a real-game checkpoint: the RAM of an area-entry cut imported ONCE (`wf_import`), then the
    /// app plays on with its own host and carries everything itself (the soak's chapter starts: ship, dungeons).
    /// A checkpoint start from a segment / recording document (its `init.ram` + `init.snd_phase`): the segment
    /// harness's own start (`wf_import::segment::start`: the area entry and the sound driver's state), then the app.
    pub fn from_segment(packs: &Packs, doc: &serde_json::Value) -> Result<StorySim, String> {
        let data: Arc<GameData> = crate::battle::load_data(packs)?;
        if let Ok(f) = wf_render::ui::Font::load(packs, "dialog") {
            wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
        }
        let files: BTreeMap<String, String> = packs
            .list()
            .into_iter()
            .filter(|p| p.starts_with("scripts/") && p.ends_with(".rhai"))
            .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
            .collect();
        step_code(packs);
        let (imp, host) = wf_import::segment::start(&data, &files, doc)?;
        let mut session = wf_import::StoryHost::session(&data, imp.world);
        wf_core::field::attach_host(&mut session.world, true);
        let errors = host.errors.clone();
        let mut prev = [0; MAX_PLAYERS];
        prev[0] = imp.prev_keys;
        Ok(StorySim { session, host, prev, errors, init_events: Vec::new(), presenter: Default::default(), steps: 0 })
    }

    pub fn from_import(packs: &Packs, ram: &wf_import::GbaRam) -> Result<StorySim, String> {
        let data: Arc<GameData> = crate::battle::load_data(packs)?;
        if let Ok(f) = wf_render::ui::Font::load(packs, "dialog") {
            wf_script_core::text::set_dialog_font((0..256u16).map(|c| f.advance(c)).collect());
        }
        let files: BTreeMap<String, String> = packs
            .list()
            .into_iter()
            .filter(|p| p.starts_with("scripts/") && p.ends_with(".rhai"))
            .filter_map(|p| packs.read(&p).and_then(|b| String::from_utf8(b).ok()).map(|s| (p, s)))
            .collect();
        step_code(packs);
        let mut host = wf_import::StoryHost::new(&data, &files)?;
        let imp = wf_import::import_emulator_state(&data, ram, &mut host).map_err(|e| format!("{e:?}"))?;
        host.prev_held = imp.prev_keys;
        let mut session = wf_import::StoryHost::session(&data, imp.world);
        wf_core::field::attach_host(&mut session.world, true);
        let errors = host.errors.clone();
        let mut prev = [0; MAX_PLAYERS];
        prev[0] = imp.prev_keys;
        Ok(StorySim { session, host, prev, errors, init_events: Vec::new(), presenter: Default::default(), steps: 0 })
    }

    pub fn world(&self) -> &World {
        &self.session.world
    }

    /// the script host's state (timers, pending wait; harnesses)
    pub fn host(&self) -> &wf_import::StoryHost {
        &self.host
    }
}

/// the content's event / inline-script bytecode for the importer (`wf_import::host::set_step_code`, as the segment
/// harness's setup): a cut inside a running event node or an area's inline script continues from the action the
/// game's interpreter is at (without it the import drops the script - soak m_pg_g111: the world map landing's
/// inline script 0x0807364C holding the leader 67 frames)
fn step_code(packs: &Packs) {
    if let Some(b) = packs.read("scripts/bytecode.json") {
        wf_import::host::set_step_code(&b);
    }
}

impl Sim for StorySim {
    fn lag_hint(&self) -> u32 {
        // an area load in the last step: the GBA's load length into that area (measured on the real route,
        // load_lag.json via tools/db57/load_lag.py); 0 otherwise
        let Some(f) = self.session.world.field.as_ref() else { return 0 };
        if f.entered != self.session.world.frame {
            return 0;
        }
        let key = format!("{}-{}", f.area.world, f.area.area);
        let d: serde_json::Value = serde_json::from_str(LOAD_LAG).unwrap_or_default();
        d["areas"][&key].as_u64().or_else(|| d["default"].as_u64()).unwrap_or(0) as u32
    }

    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>) {
        events.append(&mut self.init_events);
        self.presenter.before_step(&self.session.world);
        let mut frames = [InputFrame::default(); MAX_PLAYERS];
        for (k, f) in frames.iter_mut().enumerate() {
            let h = inputs.get(k).copied().unwrap_or(0);
            *f = InputFrame::from_held(h, self.prev[k]);
            self.prev[k] = h;
        }
        // one logic frame in the playthrough's host order (wf-import host.rs `step_inputs`)
        let seen = self.host.errors.len();
        // the title's New Game / Continue (and a game over's return to the title) replace the world: the new field
        // is this host's too (without the mark the core does not hold the field for talk / defeat / level-up events:
        // Yemma's office NPCs kept wandering in the frame s048 started)
        wf_core::field::attach_host(&mut self.session.world, true);
        self.host.step_inputs(&mut self.session, frames);
        self.errors.extend(self.host.errors[seen.min(self.host.errors.len())..].iter().cloned());
        wf_core::view::sim_events(&self.session.world, events);
        self.steps += 1;
    }

    fn view(&self, out: &mut RenderView) {
        wf_core::view::render_view(&self.session.world, out);
        self.presenter.apply(out);
    }
    fn frame(&self) -> u64 {
        self.steps
    }
    fn state_hash(&self) -> u64 {
        self.session.world.state_hash()
    }
    /// save states / rewind / rollback for story mode: the session (world, its input edges, outcome), the script
    /// host's between-frames state (the running thread, globals, host state, queue, suspended scripts) and this sim's
    /// input edges. "WFS1", u32 session length, the session bytes, u32 host length, the host (JSON), 4 x u16 prev, u64
    /// steps.
    /// The presenter's one-frame view lag is not kept: the first frame after a restore shows the restored state.
    fn snapshot(&self) -> Option<Vec<u8>> {
        let mut v = b"WFS1".to_vec();
        let s = self.session.snapshot_bytes();
        v.extend((s.len() as u32).to_le_bytes());
        v.extend(s);
        let h = serde_json::to_vec(&self.host.snapshot()).ok()?;
        v.extend((h.len() as u32).to_le_bytes());
        v.extend(h);
        for p in self.prev {
            v.extend(p.to_le_bytes());
        }
        v.extend(self.steps.to_le_bytes());
        Some(v)
    }
    fn restore(&mut self, b: &[u8]) -> bool {
        let rd = |o: usize| -> Option<usize> { Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize) };
        if b.get(..4) != Some(b"WFS1".as_slice()) {
            return false;
        }
        let Some(sn) = rd(4) else { return false };
        let Some(sb) = b.get(8..8 + sn) else { return false };
        let Some(hn) = rd(8 + sn) else { return false };
        let Some(hb) = b.get(12 + sn..12 + sn + hn) else { return false };
        let Ok(hs) = serde_json::from_slice::<wf_import::host::HostSnapshot>(hb) else { return false };
        let o = 12 + sn + hn;
        if b.len() < o + 2 * MAX_PLAYERS || !self.session.restore_bytes(sb) {
            return false;
        }
        self.host.restore(&hs);
        for k in 0..MAX_PLAYERS {
            self.prev[k] = u16::from_le_bytes([b[o + 2 * k], b[o + 2 * k + 1]]);
        }
        self.init_events.clear();
        self.presenter.reset();
        if let Some(s) = b.get(o + 2 * MAX_PLAYERS..o + 2 * MAX_PLAYERS + 8) {
            self.steps = u64::from_le_bytes(s.try_into().unwrap_or([0; 8]));
        }
        true
    }
    fn players(&self) -> usize {
        self.session.world.players.len().max(1)
    }
    /// the launcher reads the save slots (`launcher::saves_of`)
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

/// `App` running story mode.
pub fn app(packs: Packs, area: (i32, i32), chars: Vec<i32>, seed: u32) -> Result<crate::App, String> {
    let s = StorySim::new(&packs, area, chars, seed)?;
    Ok(crate::App::new(packs, Box::new(s)))
}
