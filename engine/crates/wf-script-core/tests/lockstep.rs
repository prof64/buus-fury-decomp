//! Script / cutscene lockstep against the real game (`data/script_lockstep`, recorder
//! `tools/script_lockstep/`): every micro scenario (one native or command class through the game's runner) and
//! every full cutscene (event graphs started by the game) is replayed on a field World built from the
//! recording's initial snapshot, with the same input, frame by frame; per logic frame the actors (position 24.8,
//! facing, animation raw / cel), the camera (22.10), the screen fade, the sounds and the script's own timing
//! (blocked natives resuming, text nodes opening) are compared.
//!
//! Both runtimes are checked: the exact bytecode VM and the decompiled Rhai (the same host underneath); their
//! per-frame World hashes must be identical.
//!
//! `LOCKSTEP_VERBOSE=1` prints every divergence, `LOCKSTEP_ONLY=name` runs one scenario.

use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use wf_core::content::{DirSource, GameData};
use wf_core::field::FieldConfig;
use wf_core::input::InputFrame;
use wf_core::scene::{self, Npc};
use wf_core::session::Session;
use wf_core::story::Story;
use wf_core::world::{Body, EntityId, Event, World, MAX_PLAYERS};
use wf_script::vm::{Globals, Status, Thread};
use wf_script::{ScriptId, ScriptSet};
use wf_script_core::{poll, CoreHost, CoreState, HostState, SharedHost};
use wf_script_rhai::{Runtime, Status as RStatus, UnitRef};

// ---------------------------------------------------------------------------------------------------------------
// fixture

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn content_dir() -> PathBuf {
    if let Ok(d) = std::env::var("WF_FIELD_CONTENT") {
        return PathBuf::from(d);
    }
    // the field-test extraction shared by the test crates; `wf_extract::cache` re-extracts it when the extractor's
    // source hash in its `.complete` marker differs (no stale content after an extractor change)
    let only = ["tables", "sprites", "effects", "maps", "world", "items", "scripts"];
    let shared = repo().join("engine/target/tmp/wf-core-field-content");
    if wf_extract::cache::fresh(&shared, &only) {
        return shared;
    }
    wf_extract::cache::ensure(&shared, &repo().join("roms/buu.gba"), &only)
}

struct Fixture {
    data: Arc<GameData>,
    set: ScriptSet,
    rhai: RefCell<Runtime>,
    host: SharedHost,
    /// micro scenario name -> its inline unit address
    micro: BTreeMap<String, u32>,
    /// exported files whose handler is split into steps
    stepped: std::collections::BTreeSet<String>,
}

/// the micro scenarios' bytecode becomes inline units at fake addresses (so the decompiler exports them too)
const MICRO_BASE: u32 = 0x0F00_0000;

fn scenarios(dir: &str) -> Vec<(String, Value)> {
    let d = repo().join("data/script_lockstep").join(dir);
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(&d) else { return out };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    for p in paths {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        if let Ok(only) = std::env::var("LOCKSTEP_ONLY") {
            if !only.split(',').any(|o| o == name) {
                continue;
            }
        }
        let v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        out.push((name, v));
    }
    out
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap()).collect()
}

struct Wrap(Option<Fixture>);
// SAFETY: the Rhai runtime / Rc host are only touched under FIXTURE_LOCK (one test at a time)
unsafe impl Sync for Wrap {}
unsafe impl Send for Wrap {}
static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fixture() -> Option<&'static Fixture> {
    static W: OnceLock<Wrap> = OnceLock::new();
    W.get_or_init(|| {
        let Some(rom) = wf_rom::testutil::rom() else { return Wrap(None) };
        // the dialogue font: the typewriter's line fitter
        let font = wf_ui::font::Font::load(&rom, "dialog", 0x0806_EE6C);
        wf_script_core::text::set_dialog_font((0..256u16).map(|c| font.advance(c)).collect());
        let data = Arc::new(GameData::load(&DirSource { root: content_dir() }).expect("content"));
        let mut set = ScriptSet::load_all(&rom);
        let mut micro = BTreeMap::new();
        for (k, (name, v)) in scenarios("micro").into_iter().enumerate() {
            let addr = MICRO_BASE + 0x100 * k as u32;
            let code = hex(v["code"].as_str().unwrap());
            set.insert_code(ScriptId::Inline(addr), code).unwrap();
            micro.insert(name, addr);
        }
        let (files, errs) = wf_script::export::export(&rom, &set);
        assert!(errs.is_empty(), "decompile errors: {errs:?}");
        let w = Session::new_field(data.clone(), FieldConfig::default()).world;
        let host = SharedHost(Rc::new(RefCell::new(CoreState { world: w, story: HostState::default() })));
        let rt = Runtime::new(&files, Box::new(host.clone())).expect("compile");
        let stepped = files.iter().filter(|(_, src)| src.contains("fn step_0()")).map(|(p, _)| p.clone()).collect();
        Wrap(Some(Fixture { data, set, rhai: RefCell::new(rt), host, micro, stepped }))
    })
    .0
    .as_ref()
}

// ---------------------------------------------------------------------------------------------------------------
// world from the recording's snapshot

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| v.as_u64().map(|u| u as i64).unwrap_or(0))
}

fn rd16(b: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([b[o], b[o + 1]]) as i32
}
fn rd32(b: &[u8], o: usize) -> i64 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as i64
}

/// GameData 0x03001068 + the character records 0x03001560 -> Story
fn story_from(d: &GameData, init: &Value) -> Story {
    let mut st = Story::new_game(d);
    let gd = hex(init["gd"].as_str().unwrap_or(""));
    let recs = hex(init["recs"].as_str().unwrap_or(""));
    if gd.len() >= 0x500 {
        for k in 0..st.flags.len() {
            st.flags[k] = gd[0x55 + k];
        }
        for k in 0..st.supplies.len().min(0x53) {
            st.supplies[k] = gd[2 + k];
        }
        st.zenie = rd32(&gd, 0x4F4) as u32;
        st.current = gd[0] as i32;
    }
    for c in 0..9usize {
        if recs.len() < 0x68 * (c + 1) {
            break;
        }
        let r = &recs[0x68 * c..0x68 * (c + 1)];
        let cs = &mut st.chars[c];
        cs.flags = r[0];
        cs.form = r[0x14] as i32;
        cs.rec.hp = rd16(r, 0x16);
        cs.rec.max_hp = rd16(r, 0x18);
        cs.rec.ep = rd16(r, 0x1A);
        cs.rec.max_ep = rd16(r, 0x1C);
        cs.rec.xp = rd32(r, 0x24);
        cs.rec.str_ = r[0x28] as i32;
        cs.rec.pow = r[0x29] as i32;
        cs.rec.end = r[0x2A] as i32;
        cs.rec.spd = r[0x2B] as i32;
        cs.rec.points = rd16(r, 0x2C);
        cs.rec.level = wf_core::progression::level_of(cs.rec.xp, &d.xp_table);
        let n = (r[0x0C] as usize).min(16);
        cs.skills = (0..n).map(|k| r[0x0D + k] as i32).collect();
        cs.skill_levels = (0..n).map(|k| r[0x2F + 2 * k] as i32).collect();
        for k in 0..4 {
            cs.equip[k] = (rd32(r, 0x3C + 4 * k) & 0x0FFF_FFFF) as i32;
        }
    }
    for (k, g) in init["g"].as_array().into_iter().flatten().enumerate().take(4) {
        st.g[k] = i(g) as i32;
    }
    for (k, l) in init["l"].as_array().into_iter().flatten().enumerate().take(4) {
        st.l[k] = i(l) as i32;
    }
    st
}

/// actor row [obj#, sprite, x, y, z, dir, raw, frame, tick]
struct Row<'a>(&'a Value);
impl Row<'_> {
    fn k(&self) -> usize {
        i(&self.0[0]) as usize
    }
    fn sprite(&self) -> i32 {
        i(&self.0[1]) as i32
    }
    fn x(&self) -> i32 {
        i(&self.0[2]) as i32
    }
    fn y(&self) -> i32 {
        i(&self.0[3]) as i32
    }
    fn dir(&self) -> u8 {
        i(&self.0[5]) as u8
    }
    fn raw(&self) -> i32 {
        i(&self.0[6]) as i32
    }
    fn frame(&self) -> i32 {
        i(&self.0[7]) as i32
    }
    fn tick(&self) -> i32 {
        self.0.get(8).map(i).unwrap_or(0) as i32
    }
}

struct Built {
    world: World,
    /// recording obj# -> entity
    map: BTreeMap<usize, EntityId>,
}

fn build_world(f: &Fixture, doc: &Value) -> Built {
    let init = &doc["init"];
    let objs = doc["objs"].as_array().unwrap();
    let story = story_from(&f.data, init);
    let area = init["area"].as_array().cloned().unwrap_or_default();
    let (aw, aa, av) = (i(&area[0]) as i32, i(&area[1]) as i32, i(&area[2]) as i32);
    let rows: Vec<Row> = init["actors"].as_array().unwrap().iter().map(Row).collect();
    let player_row = rows.iter().find(|r| objs[r.k()]["player"].as_bool() == Some(true));
    let (px, py, pdir) = player_row.map(|r| (r.x() >> 8, r.y() >> 8, r.dir())).unwrap_or((120, 120, 0));
    let rng = init["rng"].as_array().map(|r| wf_core::battle::RngInit::State(i(&r[0]) as u32, i(&r[1]) as u32)).unwrap_or(wf_core::battle::RngInit::Seed(1));
    let cfg = FieldConfig { world: aw, area: aa, variant: av, pos: Some((px, py)), facing: pdir, chars: vec![story.current], story: Some(story), rng, xp: true, arrive: false };
    let mut w = Session::new_field(f.data.clone(), cfg).world;
    // the recording's objects replace what the area load placed
    let pid = w.players[0];
    for e in w.ents.iter_mut() {
        if e.id != pid {
            e.removed = true;
        }
    }
    w.ents.retain(|e| !e.removed);
    if let Some(fl) = w.field.as_mut() {
        fl.placed.clear();
    }
    w.enemies.clear();
    let mut map = BTreeMap::new();
    // object list order: the recording lists objects in world-list order (the game updates last to first)
    let mut order: Vec<EntityId> = vec![];
    for r in &rows {
        if Some(r.k()) == player_row.map(|p| p.k()) {
            let i = w.player_idx(0).unwrap();
            let p = w.pl(i);
            p.a.x = r.x();
            p.a.y = r.y();
            p.a.facing = r.dir();
            // the idle state's animation where the recording found it (its tick started at a random value)
            if r.raw() == 1 {
                p.ap.raw = 1;
                p.ap.frame = r.frame();
                p.ap.tick = r.tick();
            }
            map.insert(r.k(), pid);
            order.push(pid);
            continue;
        }
        let mut n = Npc::new(&f.data, r.sprite(), 0, 0, r.dir());
        n.a.x = r.x();
        n.a.y = r.y();
        n.anim = scene::ActAnim { raw: r.raw().max(1), frame: r.frame(), tick: r.tick(), ..Default::default() };
        let id = w.add(Body::Npc(Box::new(n)));
        map.insert(r.k(), id);
        order.push(id);
    }
    // put the entities in the recording's list order
    let mut ents = std::mem::take(&mut w.ents);
    ents.sort_by_key(|e| order.iter().position(|&o| o == e.id).unwrap_or(usize::MAX));
    w.ents = ents;
    let cam = init["cam"].as_array().unwrap();
    wf_core::battle::set_camera(&mut w, 0, i(&cam[0]) as i32, i(&cam[1]) as i32);
    w.cameras[0].shown = w.cameras[0].rect;
    w.scene.song = init["song"].as_i64().unwrap_or(-1) as i32;
    // the area load drew from the RNG: restart it where the recording starts
    if let Some(r) = init["rng"].as_array() {
        w.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
    }
    Built { world: w, map }
}

// ---------------------------------------------------------------------------------------------------------------
// drivers: the bytecode VM and the Rhai runtime behind the same host

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rt {
    Vm,
    Rhai,
}

enum Start {
    Unit(u32),
    Event(u16, [i32; 4]),
}

struct Driver<'a> {
    rt: Rt,
    f: &'a Fixture,
    threads: Vec<Thread>,
    globals: Globals,
    running: bool,
    rhai_started: bool,
}

impl<'a> Driver<'a> {
    fn start(&mut self, w: &mut World, hs: &mut HostState, s: &Start) -> Result<(), String> {
        self.globals.g = w.story.g;
        match self.rt {
            Rt::Vm => {
                let t = match s {
                    Start::Unit(a) => Thread::new(ScriptId::Inline(*a), [0; 4]),
                    Start::Event(e, p) => Thread::new_event(&self.f.set, *e, *p).map_err(|e| format!("{e:?}"))?,
                };
                self.threads.push(t);
                let t = self.threads.last_mut().unwrap();
                let st = t.run(&self.f.set, &mut self.globals, &mut CoreHost { w, s: hs }).map_err(|e| format!("{e:?}"))?;
                self.after(w, st == Status::Finished);
            }
            Rt::Rhai => {
                let mut rt = self.f.rhai.borrow_mut();
                rt.set_globals(self.globals.clone());
                match s {
                    Start::Unit(a) => {
                        let file = format!("inline_0x{a:08X}");
                        let steps = self.f.stepped.contains(&format!("scripts/inline/{file}.rhai"));
                        rt.start_unit(UnitRef::new(file, "", steps), [0; 4]);
                    }
                    Start::Event(e, p) => rt.start_event(*e, *p),
                }
                self.rhai_started = true;
                let st = with_host(self.f, w, hs, || rt.run());
                drop(rt);
                let st = st.map_err(|e| e.to_string())?;
                self.after(w, st == RStatus::Finished);
            }
        }
        self.running = true;
        Ok(())
    }
    fn after(&mut self, _w: &mut World, finished: bool) {
        if finished {
            self.running = false;
        }
    }
    fn resume(&mut self, w: &mut World, hs: &mut HostState, v: Option<i32>) -> Result<(), String> {
        match self.rt {
            Rt::Vm => {
                let t = self.threads.last_mut().unwrap();
                let st = t.resume(v, &self.f.set, &mut self.globals, &mut CoreHost { w, s: hs }).map_err(|e| format!("{e:?}"))?;
                self.after(w, st == Status::Finished);
            }
            Rt::Rhai => {
                let mut rt = self.f.rhai.borrow_mut();
                let st = with_host(self.f, w, hs, || rt.resume(v));
                drop(rt);
                let st = st.map_err(|e| e.to_string())?;
                self.after(w, st == RStatus::Finished);
            }
        }
        Ok(())
    }
}

/// run `op` with the shared host holding (w, hs), then move them back
fn with_host<T>(f: &Fixture, w: &mut World, hs: &mut HostState, op: impl FnOnce() -> T) -> T {
    {
        let mut st = f.host.0.borrow_mut();
        std::mem::swap(&mut st.world, w);
        std::mem::swap(&mut st.story, hs);
    }
    let r = op();
    let mut st = f.host.0.borrow_mut();
    std::mem::swap(&mut st.world, w);
    std::mem::swap(&mut st.story, hs);
    r
}

// ---------------------------------------------------------------------------------------------------------------
// replay + compare

#[derive(Default, Debug, Clone)]
struct Report {
    frames: usize,
    logic: usize,
    pos: usize,
    dir: usize,
    anim: usize,
    cam: usize,
    fade: usize,
    snd: usize,
    resume: usize,
    text: usize,
    missing: usize,
    bg_pos: usize,
    bg_dir: usize,
    bg_missing: usize,
    /// text / choice nodes: (frame the game visited the step, frame ours opened)
    first: Option<String>,
    first_bg: Option<String>,
    hashes: Vec<u64>,
    errors: Vec<String>,
}

impl Report {
    fn bad(&self) -> usize {
        self.pos + self.dir + self.anim + self.cam + self.fade + self.snd + self.resume + self.text + self.missing
    }
    fn note_bg(&mut self, frame: usize, what: &str, msg: String) {
        if std::env::var("LOCKSTEP_VERBOSE").map(|v| v == "2").unwrap_or(false) {
            eprintln!("    f{frame} {what}: {msg}");
        }
        if self.first_bg.is_none() {
            self.first_bg = Some(format!("f{frame} {what}: {msg}"));
        }
    }
    fn note(&mut self, frame: usize, what: &str, msg: String) {
        if std::env::var("LOCKSTEP_VERBOSE").is_ok() {
            eprintln!("    f{frame} {what}: {msg}");
        }
        if self.first.is_none() {
            self.first = Some(format!("f{frame} {what}: {msg}"));
        }
    }
}

fn sounds(w: &World) -> Vec<String> {
    let mut v: Vec<String> = w
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Sfx { id, vol } => Some(format!("play {id} {vol}")),
            Event::SfxStop { id } => Some(format!("stop {id}")),
            Event::SfxStopAll => Some("stopall".into()),
            Event::Song { id } => Some(format!("song {id}")),
            _ => None,
        })
        .collect();
    v.sort();
    v
}

fn emu_sounds(f: &Value) -> Vec<String> {
    let mut v: Vec<String> = f["snd"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let a = s.as_array()?;
            Some(match a[0].as_str()? {
                "play" => format!("play {} {}", i(&a[1]), i(&a[2])),
                "stop" | "stopmatch" => format!("stop {}", i(&a[1])),
                "stopall" => "stopall".into(),
                "song" => format!("song {}", i(&a[1])),
                o => o.to_string(),
            })
        })
        .collect();
    v.sort();
    v
}

/// the frame the recorded script work ends: the runner's return (injected) or the last outermost scene exit
fn scene_end(doc: &Value) -> usize {
    if let Some(d) = doc["done"].as_i64() {
        return d as usize;
    }
    let mut depth = 0i32;
    let mut end = usize::MAX;
    for (k, fr) in doc["frames"].as_array().unwrap().iter().enumerate() {
        for s in fr["steps"].as_array().into_iter().flatten() {
            match s[0].as_str() {
                Some("run") => depth += 1,
                Some("exit") => {
                    depth = (depth - 1).max(0);
                    if depth == 0 {
                        end = k;
                    }
                }
                _ => {}
            }
        }
    }
    end
}

/// top-level scene starts of a natural recording: (frame, event) for every run at depth 0
fn top_runs(doc: &Value) -> Vec<(usize, u16)> {
    let mut out = vec![];
    let mut depth = 0i32;
    for (k, fr) in doc["frames"].as_array().unwrap().iter().enumerate() {
        for s in fr["steps"].as_array().into_iter().flatten() {
            match s[0].as_str() {
                Some("run") => {
                    if depth == 0 {
                        out.push((k, i(&s[1]) as u16));
                    }
                    depth += 1;
                }
                Some("exit") => depth = (depth - 1).max(0),
                _ => {}
            }
        }
    }
    out
}

fn replay(f: &Fixture, name: &str, doc: &Value, rt: Rt) -> Report {
    let mut rep = Report::default();
    let Built { world, mut map } = build_world(f, doc);
    let mut scripted = Scripted { initial: world.ents.iter().map(|e| e.id).collect(), ..Default::default() };
    let mut sess = Session { data: f.data.clone(), world, config: Default::default(), prev: [0; MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() };
    // the area load's song / sounds belong to the frame before the recording
    sess.world.events.clear();
    let mut hs = HostState::default();
    let mut drv = Driver { rt, f, threads: vec![], globals: Globals { g: sess.world.story.g }, running: false, rhai_started: false };
    let frames = doc["frames"].as_array().unwrap();
    let starts: Vec<(usize, Start)> = match doc["kind"].as_str() {
        Some("code") => vec![(0, Start::Unit(f.micro[name]))],
        Some("event") => vec![(0, Start::Event(i(&doc["event"]) as u16, [0; 4]))],
        _ => top_runs(doc).into_iter().map(|(k, e)| (k, Start::Event(e, [0; 4]))).collect(),
    };
    let objs = doc["objs"].as_array().unwrap();
    let injected = matches!(doc["kind"].as_str(), Some("code") | Some("event"));
    // the game's sounds per logic frame: a logic frame that ran long (lag video frames) plays its sounds in a later
    // video frame, up to the one where the next logic frame starts
    let mut game_snd: Vec<Vec<String>> = vec![vec![]; frames.len()];
    {
        let mut last = 0usize;
        let mut prev_lf = i(&doc["init"]["lf"]);
        let mut prev_lag = false;
        for (k, fr) in frames.iter().enumerate() {
            let lf = i(&fr["lf"]);
            let logic = k == 0 || lf != prev_lf;
            prev_lf = lf;
            let snd = emu_sounds(fr);
            let to = if !logic || prev_lag { last } else { k };
            game_snd[to].extend(snd);
            game_snd[to].sort();
            if logic {
                last = k;
            }
            prev_lag = !logic;
        }
    }
    let end = scene_end(doc).saturating_add(1);
    let mut prev_held = 0u16;
    let mut lf_prev = i(&doc["init"]["lf"]);
    let done = doc["done"].as_i64();
    let mut resumed_at: Option<usize> = None;
    let mut presented = 0usize;
    for (k, fr) in frames.iter().enumerate() {
        rep.frames += 1;
        let keys = i(&fr["keys"]) as u16;
        let lf = i(&fr["lf"]);
        let logic = k == 0 || lf != lf_prev || fr.get("lf").is_none();
        lf_prev = lf;
        let starting: Vec<&Start> = starts.iter().filter(|(s, _)| *s == k).map(|(_, s)| s).collect();
        let menu_frame = k > 0 && !logic && wf_core::menu::active(&sess.world);
        if k == 0 {
            let w = &mut sess.world;
            // the call frame: the script starts before this frame's world update (it blocks in a nested loop)
            for s in &starting {
                if let Err(e) = drv.start(w, &mut hs, s) {
                    rep.errors.push(format!("f{k} start: {e}"));
                }
            }
            // an injected call that returns at once: the stub then runs the frame's world update
            if injected && !drv.running {
                sess.world.step_entities();
            }
            // the wait mode's first frame updates the camera (scroll steps, follow, the player camera)
            scene::camera_update(&mut sess.world);
        } else if logic || wf_core::menu::active(&sess.world) {
            // a menu screen runs per video frame (menu::step models the game's redraw lag itself; the game's logic
            // frame counter does not advance while it runs)
            if logic {
                rep.logic += 1;
            }
            let mut inputs = [InputFrame::default(); MAX_PLAYERS];
            inputs[0] = InputFrame::from_held(keys, prev_held);
            prev_held = keys;
            // the game's RNG where its previous frame left it: draws of objects this replay does not simulate (enemy
            // AI, dying enemies' effects) do not shift the scene's own draws (idle ticks, punch sounds)
            if let Some(r) = frames[k - 1]["rng"].as_array() {
                sess.world.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
            }
            // the frame: objects and cutscene systems, then the scripts resume, then the camera
            sess.step_world(inputs);
            let w = &mut sess.world;
            if drv.running {
                if let Some(v) = poll(w, &mut hs) {
                    resumed_at = Some(k);
                    if let Err(e) = drv.resume(w, &mut hs, v) {
                        rep.errors.push(format!("f{k} resume: {e}"));
                        drv.running = false;
                    }
                    // the injected runner returned inside the stub, which then runs the world update (objects) once more
                    if injected && !drv.running {
                        sess.world.step_entities();
                    }
                }
            }
            // scenes the game starts in this frame (on-defeat callbacks, hooks): after the running one's turn
            for s in &starting {
                if let Err(e) = drv.start(&mut sess.world, &mut hs, s) {
                    rep.errors.push(format!("f{k} start: {e}"));
                }
            }
            sess.step_post();
        }
        // the script's own end (micro: `done` = the frame the runner returned)
        if let (Some(d), Some("code")) = (done, doc["kind"].as_str()) {
            if k as i64 == d && drv.running {
                rep.resume += 1;
                rep.note(k, "timing", format!("script still blocked when the game's runner returned ({:?})", hs.pending));
            }
            if (k as i64) < d && !drv.running && drv.threads.len() + drv.rhai_started as usize > 0 {
                rep.resume += 1;
                rep.note(k, "timing", format!("script finished at {k} < game {d}"));
            }
        }
        let _ = resumed_at;
        if std::env::var("LOCKSTEP_TRACE").is_ok() {
            let steps: Vec<String> = fr["steps"].as_array().into_iter().flatten().map(|s| s.to_string()).collect();
            eprintln!("  t{k} logic {logic} keys {keys:x} running {} depth {} pending {:?} dialog {:?} cam {:?} {:?} tracks {:?} steps {:?}", drv.running, sess.world.scene.depth, hs.pending, sess.world.scene.dialog.as_ref().map(|d| (d.kind, d.phase, d.page, d.pages, d.cursor, d.t)), sess.world.scene.cam.mode, (sess.world.cameras[0].x, sess.world.cameras[0].y, sess.world.field.as_ref().map(|f| f.camera_max)), sess.world.scene.tracks.iter().map(|t| (t.owner, t.queue.len(), t.started, t.holding)).collect::<Vec<_>>(), steps);
        }
        if !logic && !wf_core::menu::active(&sess.world) && !menu_frame {
            // a lag frame: the game ran no logic, so nothing sounded (our events are the last step's)
            sess.world.events.clear();
        }
        let w = &sess.world;
        scripted.update(w);
        // text nodes: the game's step visit vs the frame our box opened
        let game_text = fr["steps"].as_array().into_iter().flatten().any(|s| s[0].as_str() == Some("step") && matches!(i(&s[2]), 1 | 2 | 3 | 5 | 6));
        let ours_text = hs.present[presented.min(hs.present.len())..].iter().any(|p| !matches!(p, wf_script_core::Present::ItemGet { .. }));
        presented = hs.present.len();
        if game_text || ours_text {
            if game_text != ours_text {
                rep.text += 1;
                rep.note(k, "text", format!("game opens a text box: {game_text}, ours: {ours_text}"));
            }
        }
        // after the scenes ended the world plays on (enemy AI, the fight): outside the scene systems
        if k > end {
            break;
        }
        // a logic frame that runs long (a load, a long redraw) is complete only in its last video frame
        let complete = frames.get(k + 1).is_none_or(|n| i(&n["lf"]) != lf);
        if !complete {
            continue;
        }
        compare(w, fr, &game_snd[k], objs, &mut map, k, &mut rep, &scripted);
        rep.hashes.push(w.state_hash() ^ hash_host(&hs));
        if !drv.running && done.is_some_and(|d| k as i64 > d) && doc["kind"].as_str() == Some("code") {
            // the tail after the runner returned: the player resumes its own update in the game
            break;
        }
    }
    rep
}

fn hash_host(h: &HostState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut s = wf_core::world::StableHasher::default();
    h.hash(&mut s);
    s.finish()
}


/// entities the scene drives (commands, puppets, spawned by scripts) — compared strictly; the rest of the
/// recording's objects (map NPCs, enemies the scene does not touch) are background: compared by position / facing
/// only and reported apart
#[derive(Default)]
struct Scripted {
    ids: std::collections::BTreeSet<EntityId>,
    initial: std::collections::BTreeSet<EntityId>,
}

impl Scripted {
    fn update(&mut self, w: &World) {
        // actors whose position / facing / animation a command sets (removals, sounds, flashes, shakes do not)
        for t in &w.scene.tracks {
            if let scene::Owner::Entity(id) = t.owner {
                let drives = t.queue.iter().any(|c| {
                    !matches!(c, scene::Cmd::Remove | scene::Cmd::Sfx { .. } | scene::Cmd::Puff { .. } | scene::Cmd::HitFlash | scene::Cmd::Shake { .. } | scene::Cmd::ScreenFlash { .. } | scene::Cmd::Wait { .. })
                });
                if drives {
                    self.ids.insert(id);
                }
            }
        }
        for p in &w.scene.puppets {
            self.ids.insert(p.id);
        }
        for e in &w.ents {
            // actors the scripts spawned (map NPCs the field loads carry a behaviour / placement tag)
            if matches!(&e.body, Body::Npc(n) if n.behaviour.is_none() && n.tag.is_none()) && !self.initial.contains(&e.id) {
                self.ids.insert(e.id);
            }
        }
        if let Some(&p) = w.players.first() {
            self.ids.insert(p);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compare(w: &World, fr: &Value, es: &[String], objs: &[Value], map: &mut BTreeMap<usize, EntityId>, k: usize, rep: &mut Report, scripted: &Scripted) {
    for a in fr["actors"].as_array().into_iter().flatten() {
        let r = Row(a);
        let id = match map.get(&r.k()) {
            Some(&id) => Some(id),
            None => {
                // a new object: the first unmapped entity with that sprite id
                let used: Vec<EntityId> = map.values().copied().collect();
                let e = (0..w.ents.len()).find(|&i| !w.ents[i].removed && !used.contains(&w.ents[i].id) && scene::sprite_id(w, i) == Some(r.sprite())).map(|i| w.ents[i].id);
                if let Some(e) = e {
                    map.insert(r.k(), e);
                }
                e
            }
        };
        let Some(i) = id.and_then(|id| w.find(id)).filter(|&i| !w.ents[i].removed) else {
            rep.bg_missing += 1;
            rep.note_bg(k, "missing", format!("obj #{} sprite {} ({}) has no entity", r.k(), r.sprite(), objs[r.k()]["vt"]));
            continue;
        };
        let strict = scripted.ids.contains(&w.ents[i].id);
        let ac = w.actor(i).unwrap();
        if ac.x != r.x() || ac.y != r.y() {
            let m = format!("#{} sprite {}: ({}, {}) vs game ({}, {})", r.k(), r.sprite(), ac.x as f64 / 256.0, ac.y as f64 / 256.0, r.x() as f64 / 256.0, r.y() as f64 / 256.0);
            if strict {
                rep.pos += 1;
                rep.note(k, "pos", m);
            } else {
                rep.bg_pos += 1;
                rep.note_bg(k, "bg pos", m);
            }
        }
        if ac.facing != r.dir() {
            let m = format!("#{} sprite {}: {} vs game {}", r.k(), r.sprite(), ac.facing, r.dir());
            if strict {
                rep.dir += 1;
                rep.note(k, "dir", m);
            } else {
                rep.bg_dir += 1;
                rep.note_bg(k, "bg dir", m);
            }
        }
        // animation (scripted actors): compared where the game's anim pointer is a real anim of the sprite
        if strict {
            if let Some((raw, frame)) = scene::shown_anim(w, i) {
                // the recording names an anim by the first table slot holding the same pointer (content `canon_raw`)
                let ours = ac.sprite.and_then(|s| w.data.sprite(s)).and_then(|sp| sp.anim_raw(raw)).map(|a| a.canon_raw.min(raw)).unwrap_or(-1);
                if r.raw() >= 0 && (ours != r.raw() || (ours >= 0 && frame != r.frame())) {
                    rep.anim += 1;
                    rep.note(k, "anim", format!("#{} sprite {}: raw {} cel {} vs game raw {} cel {}", r.k(), r.sprite(), ours, frame, r.raw(), r.frame()));
                }
            }
        }
    }
    let cam = fr["cam"].as_array().unwrap();
    let c = &w.cameras[0];
    if c.x != i(&cam[0]) as i32 || c.y != i(&cam[1]) as i32 {
        rep.cam += 1;
        rep.note(k, "cam", format!("({}, {}) vs game ({}, {})", c.x as f64 / 1024.0, c.y as f64 / 1024.0, i(&cam[0]) as f64 / 1024.0, i(&cam[1]) as f64 / 1024.0));
    }
    // screen: a fully black palette in the game <-> level 16 here
    let io = fr["io"].as_array().unwrap();
    let pal = i(&io[5]);
    let game_black = pal == 0;
    // a menu screen's black (its enter / leave iterations: menu::view's black page)
    let menu_black = wf_core::menu::view(w).is_some_and(|v| v.backdrop == 0 && v.palette == [(0u16, 0u32)]);
    let ours_black = scene::palette_black(w) || menu_black;
    if game_black != ours_black {
        rep.fade += 1;
        rep.note(k, "fade", format!("black {} (lvl {} mosaic {}) vs game palette sum {pal:#x}", ours_black, w.scene.screen.black, w.scene.screen.mosaic));
    }
    if std::env::var("LOCKSTEP_RNG").is_ok() {
        if let Some(r) = fr["rng"].as_array() {
            let g = [i(&r[0]) as u32, i(&r[1]) as u32];
            if w.rng.state() != g {
                eprintln!("    f{k} rng {:08x?} vs game {:08x?}", w.rng.state(), g);
            }
        }
    }
    let os = sounds(w);
    if es != os.as_slice() {
        rep.snd += 1;
        rep.note(k, "snd", format!("{os:?} vs game {es:?}"));
    }
}

// ---------------------------------------------------------------------------------------------------------------

fn run_dir(dir: &str, strict: bool) {
    let _g = FIXTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(f) = fixture() else { return };
    let all = scenarios(dir);
    let mut lines = vec![];
    let mut failed = vec![];
    for (name, doc) in &all {
        let vm = replay(f, name, doc, Rt::Vm);
        let rh = replay(f, name, doc, Rt::Rhai);
        let same = vm.hashes == rh.hashes;
        if !same {
            let k = vm.hashes.iter().zip(rh.hashes.iter()).position(|(a, b)| a != b).unwrap_or(vm.hashes.len().min(rh.hashes.len()));
            eprintln!("  {name}: vm and rhai diverge at frame {k} (vm {} frames, rhai {}) rhai first: {:?} errs {:?}", vm.hashes.len(), rh.hashes.len(), rh.first, rh.errors);
        }
        let line = format!(
            "{name:24} frames {:5} logic {:5} | pos {} dir {} anim {} cam {} fade {} snd {} timing {} text {} | bg pos {} dir {} missing {} | vm==rhai {} {}{}",
            vm.frames,
            vm.logic,
            vm.pos,
            vm.dir,
            vm.anim,
            vm.cam,
            vm.fade,
            vm.snd,
            vm.resume,
            vm.text,
            vm.bg_pos,
            vm.bg_dir,
            vm.bg_missing,
            same,
            vm.first.clone().unwrap_or_default(),
            if vm.errors.is_empty() { String::new() } else { format!(" ERR {:?}", vm.errors) },
        );
        eprintln!("{line}");
        lines.push(line);
        if !same || !vm.errors.is_empty() || (strict && vm.bad() > 0) {
            failed.push(name.clone());
        }
    }
    eprintln!("{dir}: {} scenarios, {} failing: {failed:?}", all.len(), failed.len());
    assert!(failed.is_empty(), "failing: {failed:?}");
}

#[test]
fn micro_scenarios_match_the_game() {
    run_dir("micro", true);
}

#[test]
fn full_cutscenes_match_the_game() {
    // strict since every full cutscene matches (2026-10-02); background objects (bg_*) stay informational
    run_dir("", true);
}
