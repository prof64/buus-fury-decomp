//! The segment harness: a real-game recording that starts at an emulator state (`data/segments/*.json`, recorder
//! `tools/segments/segment.py`) replayed on wf-core from the imported start state with the same input; compared per
//! complete logic frame exactly like the story playthrough (area, every actor's position / facing / animation,
//! camera, fade, RNG, sounds, text boxes opening, zenie). The verdict is the first frame where anything differs.
//!
//! Env (diagnosis): `SEGMENT_VERBOSE=N` prints the first N divergences, `SEGMENT_TRACE=a..b` the host / scene state.

use crate::{import_emulator_state, GbaRam, Imported, StoryHost};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use wf_core::content::GameData;
use wf_core::scene;
use wf_core::world::{Body, EntityId, Event, World};

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| v.as_u64().map(|u| u as i64).unwrap_or(0))
}

/// read a segment / playthrough file (one JSON document)
pub fn load(path: &Path) -> Result<Value, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", path.display()))
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
}

#[derive(Default, Debug)]
pub struct Report {
    /// video frames in the recording
    pub frames: usize,
    pub logic: usize,
    /// video frames matched continuously from the start (the verdict)
    pub compared: usize,
    pub first: Option<(usize, String)>,
    pub counts: BTreeMap<&'static str, usize>,
    /// actor comparisons made (sanity: the verdict is not vacuous)
    pub actors: usize,
    /// the import's notes (state not taken from RAM / differing after the rebuild)
    pub notes: Vec<String>,
    pub errors: Vec<String>,
    /// held frames / RNG draws of the import's rebuild
    pub import: (u32, u32),
    /// scripts started (logic frame, id)
    pub started: Vec<(usize, String)>,
    shown: usize,
    /// `SEGMENT_RECS`: the per-character comparison (`recs.rs`), or no game rows for this segment
    pub recs: Option<crate::recs::Tally>,
    pub recs_missing: bool,
    /// `SEGMENT_WORLD`: the world-state comparison (`world.rs`: story flags, game bits, timers, script words)
    pub world: Option<crate::world::Tally>,
    /// `SEGMENT_AREA`: the area-local comparison (`area.rs`: trigger list + resume nodes, walk-grid changes)
    pub area: Option<crate::area::Tally>,
}

impl Report {
    fn bad(&mut self, k: usize, what: &'static str, msg: String) {
        *self.counts.entry(what).or_default() += 1;
        let limit: usize = std::env::var("SEGMENT_VERBOSE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        if self.shown < limit {
            eprintln!("    f{k} {what}: {msg}");
            self.shown += 1;
        }
        if self.first.is_none() {
            self.first = Some((k, format!("{what}: {msg}")));
        }
    }
    /// every frame matched
    pub fn exact(&self) -> bool {
        self.first.is_none() && self.compared == self.frames
    }
}

fn sounds(w: &World) -> Vec<String> {
    let mut v: Vec<String> = w
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Sfx { id, vol } => Some(format!("play {id} {vol}")),
            Event::SfxStop { id } | Event::SfxStopMatch { id } => Some(format!("stop {id}")),
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

/// one complete logic frame: the World against the recording's row
pub fn compare(w: &World, fr: &Value, es: &[String], map: &mut BTreeMap<usize, EntityId>, k: usize, rep: &mut Report) {
    // the title screen: our world is a placeholder field under it, the game has none (its area id is stale)
    if wf_core::menu::title::on_title(w) {
        return;
    }
    let a = &fr["area"];
    if let Some(f) = w.field.as_ref() {
        let ours = (f.area.world as i64, f.area.area as i64, f.area.variant as i64);
        let game = (i(&a[0]), i(&a[1]), i(&a[2]));
        if ours != game && game != (0, 0, 0) {
            rep.bad(k, "area", format!("{ours:?} vs game {game:?}"));
        }
    }
    // actors (the player included). A recording object that is gone frees its entity (area loads re-create the
    // player object: a new obj# for the same entity)
    // a menu screen's own objects replace the game's object world (the party menu's character figures: segment sb
    // 21-14); the world's actors are compared again when it closes
    // (a script-opened screen's held black - fade at its end - has its object world already: Switch Character builds
    // its figures in the fade's last step, real route shipr_172 lf 356,699)
    let menu_screen = wf_core::menu::active(w) && !matches!(w.menu.phase, wf_core::menu::Phase::FadeOut { level } if level < 256);
    let present: Vec<usize> = fr["actors"].as_array().into_iter().flatten().map(|a| Row(a).k()).collect();
    if !menu_screen {
        map.retain(|k, _| present.contains(k));
    }
    for a in fr["actors"].as_array().into_iter().flatten().filter(|_| !menu_screen) {
        let r = Row(a);
        // not an actor: a damage number (vt 0x0804C904) in freed actor memory - the recorder reads a stale sprite id
        // at the actor's offset; no animation (raw -1), position (0, 0) (dungeon segment db4 15-31)
        if r.raw() < 0 && r.x() == 0 && r.y() == 0 {
            continue;
        }
        // nor is an actor's freed memory during a script warp's load (the old player object, still in the
        // recorder's list for the load's lag frames: a ROM pointer as x, a facing > 7; db1 9-1 -> the world map)
        if r.dir() > 7 {
            continue;
        }
        let id = match map.get(&r.k()).copied().filter(|&id| w.find(id).is_some_and(|i| !w.ents[i].removed)) {
            Some(id) => Some(id),
            None => {
                let used: Vec<EntityId> = map.values().copied().collect();
                let e = (0..w.ents.len()).find(|&i| !w.ents[i].removed && !used.contains(&w.ents[i].id) && scene::sprite_id(w, i) == Some(r.sprite())).map(|i| w.ents[i].id);
                if let Some(e) = e {
                    map.insert(r.k(), e);
                }
                e
            }
        };
        let Some(i) = id.and_then(|id| w.find(id)).filter(|&i| !w.ents[i].removed) else {
            // the entity at that position (a sprite mismatch) helps the diagnosis
            let there = (0..w.ents.len()).find(|&i| !w.ents[i].removed && w.actor(i).is_some_and(|a| a.x == r.x() && a.y == r.y()));
            let there = there.map(|i| format!(" (ours there: sprite {:?})", scene::sprite_id(w, i))).unwrap_or_default();
            rep.bad(k, "missing", format!("obj #{} sprite {} has no entity{there}", r.k(), r.sprite()));
            continue;
        };
        let ac = w.actor(i).unwrap();
        rep.actors += 1;
        if ac.x != r.x() || ac.y != r.y() {
            rep.bad(k, "pos", format!("#{} sprite {}: ({}, {}) vs game ({}, {})", r.k(), r.sprite(), ac.x as f64 / 256.0, ac.y as f64 / 256.0, r.x() as f64 / 256.0, r.y() as f64 / 256.0));
        }
        if ac.facing != r.dir() {
            rep.bad(k, "dir", format!("#{} sprite {}: {} vs game {}", r.k(), r.sprite(), ac.facing, r.dir()));
        }
        if let Some((raw, frame)) = scene::shown_anim(w, i) {
            let ours = ac.sprite.and_then(|s| w.data.sprite(s)).and_then(|sp| sp.anim_raw(raw)).map(|a| a.canon_raw.min(raw)).unwrap_or(-1);
            if r.raw() >= 0 && (ours != r.raw() || (ours >= 0 && frame != r.frame())) {
                let tick = match &w.ents[i].body {
                    Body::Npc(n) => format!(" tick {}", n.anim.tick),
                    _ => String::new(),
                };
                rep.bad(k, "anim", format!("#{} sprite {}: raw {} cel {}{} vs game raw {} cel {} tick {}", r.k(), r.sprite(), ours, frame, tick, r.raw(), r.frame(), a[8]));
            }
        }
    }
    let shown: Vec<EntityId> = fr["actors"].as_array().into_iter().flatten().filter_map(|a| map.get(&Row(a).k()).copied()).collect();
    // a pause-menu page replaces the object world in the game (saved and restored around it): no actors to match
    let menu_page = menu_screen || wf_core::menu::active(w) && fr["actors"].as_array().is_none_or(|a| a.is_empty());
    for (ei, e) in w.ents.iter().enumerate().filter(|(_, e)| !e.removed && !menu_page && !wf_core::vram::suspended(w, e.id)) {
        // (a player Majin Buu ate is out of the game's object world until he spits it out)
        let is_actor = match &e.body {
            Body::Player(p) => !p.gone,
            Body::Npc(_) | Body::Enemy(_) => true,
            _ => false,
        };
        if is_actor && !shown.contains(&e.id) {
            rep.bad(k, "extra", format!("entity {} sprite {:?} ({}) not in the game", e.id, scene::sprite_id(w, ei), if matches!(e.body, Body::Npc(_)) { "npc" } else if matches!(e.body, Body::Player(_)) { "player" } else { "enemy" }));
            break;
        }
    }
    let cam = fr["cam"].as_array().unwrap();
    // the 3D world map: the game's camera object is the map's 3D camera (x, y 22.10: wf-core `Flight.cam`)
    let (cx, cy) = match w.field.as_ref().and_then(|f| f.flight.as_ref()) {
        Some(fl) => (fl.cam[0], fl.cam[1]),
        None => (w.cameras[0].x, w.cameras[0].y),
    };
    if cx != i(&cam[0]) as i32 || cy != i(&cam[1]) as i32 {
        rep.bad(k, "cam", format!("({}, {}) vs game ({}, {})", cx as f64 / 1024.0, cy as f64 / 1024.0, i(&cam[0]) as f64 / 1024.0, i(&cam[1]) as f64 / 1024.0));
    }
    let io = fr["io"].as_array().unwrap();
    let game_black = i(&io[5]) == 0;
    let field_black = w.field.as_ref().is_some_and(|f| {
        let b = match f.mode {
            wf_core::field::Mode::FadeOut { k, step, .. } => 256 - wf_core::field::fade_out_level(k, step),
            wf_core::field::Mode::FadeIn { k } => 256 - wf_core::field::fade_in_level(w, k),
            wf_core::field::Mode::Normal => 256,
        };
        (31 * b.clamp(0, 256)) >> 8 == 0
    });
    if game_black != (scene::palette_black(w) || field_black || wf_core::menu::screen_black(w)) {
        rep.bad(k, "fade", format!("black {} vs game {}", !game_black, game_black));
    }
    if let Some(r) = fr["rng"].as_array() {
        let g = [i(&r[0]) as u32, i(&r[1]) as u32];
        if w.rng.state() != g {
            rep.bad(k, "rng", format!("{:08x?} vs game {:08x?}", w.rng.state(), g));
        }
    }
    let os = sounds(w);
    if es != os.as_slice() {
        rep.bad(k, "snd", format!("{os:?} vs game {es:?}"));
    }
    if (w.story.zenie as i64) != i(&fr["zenie"]) {
        rep.bad(k, "zenie", format!("{} vs game {}", w.story.zenie, i(&fr["zenie"])));
    }
    // the leader's HP (the row's `hp` = the current character's record HP): the app soak found a 1-HP drift no
    // segment saw (§11). Only while the leader exists and the game shows one (not in scenes / menus).
    // (a fusion component's level-up hook: the row's record is the component's, `Story::levelup_char`)
    if let (Some(pi), Some(gh)) = (w.player_idx(0), fr.get("hp").and_then(|v| v.as_i64())) {
        let hp = match w.story.levelup_char {
            Some(c) => w.story.chars.get(c as usize).map(|x| x.rec.hp).unwrap_or(0),
            None => w.pl_ref(pi).rec.hp,
        };
        if !fr["p"].is_null() && !wf_core::menu::active(w) && hp as i64 != gh {
            rep.bad(k, "hp", format!("{} vs game {}", hp, gh));
        }
    }
}

/// the recording's objects at the cut -> the imported World's entities (same sprite, same 24.8 position)
pub fn initial_map(w: &World, init: &Value) -> BTreeMap<usize, EntityId> {
    let mut map = BTreeMap::new();
    for a in init["actors"].as_array().into_iter().flatten() {
        let r = Row(a);
        let used: Vec<EntityId> = map.values().copied().collect();
        let hit = (0..w.ents.len()).find(|&k| {
            !w.ents[k].removed && !used.contains(&w.ents[k].id) && scene::sprite_id(w, k) == Some(r.sprite()) && w.actor(k).is_some_and(|ac| ac.x == r.x() && ac.y == r.y())
        });
        if let Some(k) = hit {
            map.insert(r.k(), w.ents[k].id);
        }
    }
    map
}

/// the game's sounds per logic frame (a lag frame's sounds belong to the logic frame it continues)
fn game_sounds(doc: &Value) -> Vec<Vec<String>> {
    let frames = doc["frames"].as_array().unwrap();
    let mut out: Vec<Vec<String>> = vec![vec![]; frames.len()];
    let mut last = 0usize;
    let mut prev_lf = i(&doc["init"]["lf"]);
    for (k, fr) in frames.iter().enumerate() {
        let lf = i(&fr["lf"]);
        let logic = lf != prev_lf;
        prev_lf = lf;
        let to = if !logic { last } else { k };
        out[to].extend(emu_sounds(fr));
        out[to].sort();
        if logic {
            last = k;
        }
    }
    out
}

fn range_env(name: &str) -> Option<(usize, usize)> {
    let v = std::env::var(name).ok()?;
    let (a, b) = v.split_once("..")?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// import the segment's start state; Err = the import failed
pub fn start(data: &Arc<GameData>, files: &BTreeMap<String, String>, doc: &Value) -> Result<(Imported, StoryHost), String> {
    let mut ram = GbaRam::from_json(&doc["init"]["ram"])?;
    // the display at the cut: the first recorded row's probe (the screen does not change before its logic frame)
    ram.screen_black = doc["frames"].get(0).and_then(|f| f["io"].get(5)).and_then(|v| v.as_i64()).map(|v| v == 0);
    // the actors' sprite ids / animations at the cut (rows [obj#, sprite, x, y, z, dir, raw, cel, tick])
    if let (Some(objs), Some(rows)) = (doc["init"]["objs"].as_array(), doc["init"]["actors"].as_array()) {
        for r in rows {
            let k = i(&r[0]) as usize;
            let ptr = objs.get(k).and_then(|o| o["ptr"].as_str()).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());
            if let Some(ptr) = ptr {
                ram.actor_rows.push((ptr, i(&r[1]) as i32, i(&r[6]) as i32, i(&r[7]) as i32, i(&r[8]) as i32));
            }
        }
    }
    let mut host = StoryHost::new(data, files)?;
    let mut imp = import_emulator_state(data, &ram, &mut host).map_err(|e| e.to_string())?;
    host.prev_held = imp.prev_keys;
    // the sound driver (wf-core sndsim.rs): its RAM, the play position and the cycles to the next DMA interrupt
    // (`init.snd_phase`, tools/segments/snd_phase.py; without it the redraw timing is approximate)
    if data.sound.is_some() {
        let u32_ = |a: u32| u32::from_le_bytes(ram.read(a, 4).try_into().unwrap_or([0; 4]));
        let phase = doc["init"]["snd_phase"].as_u64().map(|p| p as u32);
        let dram = [ram.read(0x0300_38A4, 0x18 + 15 * 0x60), ram.read(0x0300_3898 + 0x5C4, 16 * 0x40)].concat();
        wf_core::sndsim::import(&mut imp.world, &dram, u32_(0x0300_389C), u32_(0x0300_38A0), phase);
    }
    Ok((imp, host))
}

/// replay a segment from its imported start state: the first divergence and the matched span
pub fn replay(data: &Arc<GameData>, files: &BTreeMap<String, String>, doc: &Value) -> Report {
    replay_with(data, files, doc, &mut |_| {})
}

/// a replay observer (`replay_with`): `Before(k, world)` right before the harness steps video frame k (a logic or menu
/// step; none on a lag frame), `After(k, world)` after every video frame (the picture comparison, wf-app example
/// `pixels`: the renderer's presenter keeps the previous step's OBJ build)
pub enum Frame<'a> {
    Before(usize, &'a wf_core::world::World),
    After(usize, &'a wf_core::world::World),
}

pub fn replay_with(data: &Arc<GameData>, files: &BTreeMap<String, String>, doc: &Value, hook: &mut dyn FnMut(Frame)) -> Report {
    let frames = doc["frames"].as_array().cloned().unwrap_or_default();
    let mut rep = Report { frames: frames.len(), ..Default::default() };
    let (imp, mut host) = match start(data, files, doc) {
        Ok(v) => v,
        Err(e) => {
            rep.first = Some((0, format!("import: {e}")));
            return rep;
        }
    };
    rep.notes = imp.notes.clone();
    rep.import = (imp.held_frames, imp.draws);
    let mut map = initial_map(&imp.world, &doc["init"]);
    let mut sess = StoryHost::session(data, imp.world);
    let game_snd = game_sounds(doc);
    let trace = range_env("SEGMENT_TRACE");
    // recordings with `vram` rows check the object tile allocator too (SEGMENT_VRAM=0 turns it off)
    let vram_check = std::env::var("SEGMENT_VRAM").map(|v| v != "0").unwrap_or(true);
    let mut game_vram = doc["init"].get("vram").cloned().unwrap_or(Value::Null);
    // SEGMENT_LOADEST: the map rebuild's model parts at the imported arrival (area-load length fit)
    if std::env::var_os("SEGMENT_LOADEST").is_some() {
        let w = &sess.world;
        if let (Some((c, extra)), Some(f)) = (wf_core::sndsim::reload_parts(w), w.field.as_ref()) {
            let (mut en, mut npc, mut oth, mut dec_all, mut sprites) = (0, 0, 0, 0i64, std::collections::BTreeSet::new());
            for (k, e) in w.ents.iter().enumerate() {
                if e.removed {
                    continue;
                }
                match &e.body {
                    Body::Enemy(_) => en += 1,
                    Body::Npc(_) => npc += 1,
                    Body::Player(_) => {}
                    _ => oth += 1,
                }
                if w.actor(k).is_some() {
                    dec_all += scene::cel_dec(w, k) as i64;
                    if let Some(sp) = scene::sprite_id(w, k) {
                        sprites.insert(sp);
                    }
                }
            }
            let grids = w.data.field.as_ref().and_then(|d| d.maps.get(&f.map)?.reload.as_ref().map(|r| r.grids)).unwrap_or(0);
            eprintln!("load-est {} map {} c {} extra {} grids {grids} cam {:?} enemies {en} npcs {npc} other {oth} dec_all {dec_all} sprites {} signs {} banner {}", doc["name"].as_str().unwrap_or("?"), f.map, c, extra, (w.cameras[0].rect.x0, w.cameras[0].rect.y0), sprites.len(), f.signs, w.scene.banner.as_ref().map(|b| b.1 as i64).unwrap_or(-1));
        }
    }
    let mut lf_prev = i(&doc["init"]["lf"]);
    let mut presented = host.hs.present.len();
    let mut lf_drift = 0i64;
    // SEGMENT_SIMLAG=1: the sim makes the load lag itself (`Options::sim_lag`); the recording's lag frames are only
    // replayed where the sim has none (the rest of the unmodelled long iterations). Counts: the sim's lag frames, the
    // ones that disagree with the recording, the recording's lag frames still replayed
    let sim_lag = std::env::var_os("SEGMENT_SIMLAG").is_some();
    sess.world.opts.sim_lag = sim_lag;
    // the app's pacing (`Options::no_lag`, segments.md: the lag interface): menus step once per recorded logic frame
    // like the field, the recording's other rows are lag frames (the redraws' and the close's black frames);
    // SEGMENT_MENULAG=1 keeps the old per-video-frame menus with the redraw / close lag model
    let no_lag = !sim_lag && std::env::var_os("SEGMENT_MENULAG").is_none();
    sess.world.opts.no_lag = no_lag;
    sess.world.g.load_pending = false;
    let (mut own_lag, mut lag_bad, mut replayed) = (0usize, 0usize, 0usize);
    let mut prev_sim = false;
    // (replayed lag: the area loads counted before the last logic frame; the camera's frame of a load iteration - the
    // count of its lag frames still to come, `Session::lag_step`'s `lag_cam`)
    let field_loads = |w: &wf_core::world::World| w.field.as_ref().map(|f| f.loads);
    let mut loads_before = field_loads(&sess.world);
    let mut load_cam: Option<Option<(usize, (i32, i32))>> = None;
    // SEGMENT_RECS: the game's per-character state rows (tools/segments/recs.py)
    // (a gate with "recs_exact": its records are checked always - wf-import tests/segments.rs fails it on a difference)
    let recs_rows = (std::env::var_os("SEGMENT_RECS").is_some() || doc["recs_exact"].as_bool() == Some(true)).then(|| crate::recs::load(doc)).flatten();
    let (mut recs_i, mut recs_tally) = (0usize, crate::recs::Tally::default());
    // SEGMENT_WORLD: the game's world-state rows (tools/segments/world.py; a gate with "world_exact" always)
    let world_rows = (std::env::var_os("SEGMENT_WORLD").is_some() || doc["world_exact"].as_bool() == Some(true)).then(|| crate::world::load(doc)).flatten();
    let (mut world_i, mut world_tally) = (0usize, crate::world::Tally::default());
    // SEGMENT_AREA: the game's area-local rows (tools/segments/area.py; a gate with "area_exact" always)
    let area_rows = (std::env::var_os("SEGMENT_AREA").is_some() || doc["area_exact"].as_bool() == Some(true)).then(|| crate::area::load(doc)).flatten();
    let (mut area_i, mut area_tally) = (0usize, crate::area::Tally::default());
    // (the game's changes are against its grid before row 0's frame: ours as imported)
    let area_g0 = area_rows.as_ref().and_then(|_| crate::area::walk_grid(&sess.world));
    let mut was_leaving = false;
    for (k, fr) in frames.iter().enumerate() {
        let keys = i(&fr["keys"]) as u16;
        let lf = i(&fr["lf"]);
        let logic = lf != lf_prev;
        lf_prev = lf;
        if logic {
            loads_before = field_loads(&sess.world);
            load_cam = None;
        }
        let kind = if sess.world.g.load_pending { "load" } else { "iter" };
        let total = sess.world.g.lag + 1;
        let starting = sim_lag && !prev_sim && !wf_core::menu::active(&sess.world) && (sess.world.g.load_pending || sess.world.g.lag > 0);
        let is_sim = sim_lag && !wf_core::menu::active(&sess.world) && (sess.world.g.lag > 0 || sess.world.g.load_pending) && sess.lag_step();
        prev_sim = is_sim;
        if is_sim {
            if starting {
                // the recording's lag run from here
                let mut n = 0;
                let mut q = lf_prev;
                for f2 in frames[k..].iter() {
                    if i(&f2["lf"]) != q {
                        break;
                    }
                    q = i(&f2["lf"]);
                    n += 1;
                }
                let map = sess.world.field.as_ref().map(|f| f.map.clone()).unwrap_or_default();
                let sim_n = if kind == "load" { sess.world.g.lag + 1 } else { total - 1 };
                eprintln!("simlag-{kind} f{k} {map} sim {sim_n} game {n}");
            }
            own_lag += 1;
            if logic {
                lag_bad += 1;
                rep.bad(k, "lag", "sim lag frame vs game logic".into());
            }
        } else if sim_lag && logic && sess.world.g.lag == 0 && false {
        } else if logic {
            rep.logic += 1;
            // SEGMENT_TRACE with SEGMENT_RNG=1: every RNG call's source location inside the traced frames
            let rng_trace = trace.is_some_and(|(a, b)| (a..=b).contains(&k)) && std::env::var("SEGMENT_RNG").is_ok();
            if rng_trace {
                eprintln!("  rng calls of t{k}:");
                wf_core::rng::TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            hook(Frame::Before(k, &sess.world));
            host.step(&mut sess, keys);
            wf_core::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
        } else if !no_lag
            && wf_core::menu::active(&sess.world)
            && !(matches!(sess.world.menu.phase, wf_core::menu::Phase::FadeOut { .. }) && !sess.world.menu.scripted)
            // (a power gauge that ended inside the last world frame: the lag after it is the script's - sb 21-25
            // f61646, 9 lag frames after the Spirit Bomb's gauge filled - and the screen's last step is the next
            // world frame's)
            && !(wf_core::menu::ended_in_world(&sess.world) && wf_core::menu::minigame::world_runs(&sess.world))
        {
            // a menu screen runs per video frame (menu::step models the game's redraw lag itself); the pause menu's
            // opening fade-out is still the field's loop - the logic counter runs, and a lag frame there is a frame
            // without an update (the cameras keep easing through the fade: dungeon segments db3 10-14, db4 15-17 /
            // 15-29); a script-opened menu (the save point's Switch Characters) fades inside its own loop
            hook(Frame::Before(k, &sess.world));
            host.step(&mut sess, keys);
        } else {
            sess.world.events.clear();
            // the game's lag frame: the sound hardware's frame passes without the main loop's update
            let last = frames.get(k + 1).is_some_and(|n| i(&n["lf"]) != lf);
            // an area load's iteration (the last logic frame loaded an area): its world restore ends with a camera
            // update, shown in the iteration's last frame (a frame earlier when the last sound update ran over a
            // VBlank) - `Session::lag_step`'s rule for the sim's own lag (1-5 f11,575: the arrival camera's shake)
            // (the game's camera makes ONE step in the load's iteration: none here when ours already stepped after the
            // arrival - Diablo Desert 11-13 f1,640, a cutscene's fade-in warp whose arrival frame ran the camera update -
            // so the rule only applies to a camera still at the position it had at the iteration's first lag frame)
            // (and only for an event's warp_fade - a scene running, 1-5's s464; an inline trigger script's warp_fade
            // steps no camera in the iteration: 11-13 f1,640, depth 0)
            let cam_now = (sess.world.cameras[0].x, sess.world.cameras[0].y);
            let cam_at = *load_cam.get_or_insert_with(|| {
                let now = field_loads(&sess.world);
                (now.is_some() && now != loads_before).then_some((sess.world.sound.wrap as usize, cam_now))
            });
            if let Some((c, at)) = cam_at {
                let left = frames[k + 1..].iter().take_while(|n| i(&n["lf"]) == lf).count();
                if std::env::var_os("T_CAMDBG").is_some() {
                    eprintln!("  t-lc f{k} left {left} c {c} cam {:?} at {:?} script_fade_in {:?} mode {:?}", cam_now, at, sess.world.field.as_ref().map(|f| f.script_fade_in), sess.world.field.as_ref().map(|f| f.mode.clone()));
                }
                let snapped = cam_now == at && std::env::var_os("T_LCALL").is_none();
                if left == c && !snapped && false {
                }
                if left == c && cam_now == at && sess.world.field.as_ref().is_some_and(|f| f.script_fade_in && matches!(f.mode, wf_core::field::Mode::FadeIn { .. })) && sess.world.scene.depth > 0 {
                    sess.world.tick_shakes();
                    wf_core::field::world_mode_cameras(&mut sess.world);
                }
            }
            // SEGMENT_LAGCENSUS: each replayed lag run at its first frame - its length and what the sim was doing
            let run_start = k < 2 || i(&frames[k - 1]["lf"]) != i(&frames[k - 2]["lf"]);
            if std::env::var_os("SEGMENT_LAGCENSUS").is_some() && run_start {
                let n = frames[k..].iter().take_while(|n| i(&n["lf"]) == lf).count();
                let w = &sess.world;
                let prev_steps = if k > 0 { frames[k - 1]["steps"].to_string() } else { String::new() };
                eprintln!(
                    "lagrun {} f{k} len {n} pending {:?} mode {:?} depth {} dialog {} steps {}",
                    doc["name"].as_str().unwrap_or("?"),
                    host.hs.pending,
                    w.field.as_ref().map(|f| f.mode.clone()),
                    w.scene.depth,
                    w.scene.dialog.is_some(),
                    prev_steps.chars().take(160).collect::<String>()
                );
                if let Some(d) = w.scene.dialog.as_ref() {
                    eprintln!(
                        "lagdlg {} f{k} portrait {} kind {} pos {:?} left {} fades {} pages {} chars {} first {} options {} text {:?}",
                        doc["name"].as_str().unwrap_or("?"),
                        d.portrait,
                        d.kind,
                        d.pos,
                        d.portrait_left,
                        d.fades,
                        d.pages,
                        d.text.chars().count(),
                        d.typing.first().copied().unwrap_or(0),
                        d.options,
                        d.text.chars().take(40).collect::<String>()
                    );
                }
            }
            wf_core::sndsim::lag_frame_ex(&mut sess.world, last);
            replayed += 1;
        }
        // a menu close's black frames (the world restore's long iteration) are alignment, not logic: the app does not
        // recreate the hardware's load time, so the close takes the recording's length - the video frames up to the
        // next logic frame - as an area load's replayed lag does (`Menu.tr.leave`; the camera's restore step in the
        // frame the game shows it, `Menu.leave_cam`)
        let leaving = matches!(sess.world.menu.phase, wf_core::menu::Phase::Leave { .. }) && sess.world.menu.tr.fade_in != 0;
        // (under `no_lag` the close's black frames are the recording's lag rows themselves: nothing to set)
        if !no_lag && leaving && !was_leaving && sess.world.menu.reload_close && sess.world.menu.phase == (wf_core::menu::Phase::Leave { t: 0 }) && k + 1 < frames.len() {
            // the frames after the exit stub's logic frame with one logic counter value: the iteration's video frames
            let lf1 = i(&frames[k + 1]["lf"]);
            let run = frames[k + 1..].iter().take_while(|n| i(&n["lf"]) == lf1).count();
            // the end of the recording inside the close: its length unknown - the model's stays
            if k + 1 + run < frames.len() {
                let (model, model_cam) = (sess.world.menu.tr.leave, sess.world.menu.leave_cam);
                let leave = run as u16 + 1;
                // the restore's camera step: the black frame where the recorded camera first moves (Leave step t at
                // video frame k + 1 + t steps the camera when t + 2 + leave_cam == leave); a camera that does not move
                // keeps the model's frame
                let cam0 = &frames[k]["cam"];
                let cam = frames[k + 1..k + 1 + run].iter().position(|n| n["cam"] != *cam0).map(|j| (leave as i64 - 2 - j as i64).clamp(0, 3) as u16).unwrap_or(model_cam);
                sess.world.menu.tr.leave = leave;
                sess.world.menu.leave_cam = cam;
                if std::env::var_os("SEGMENT_CLOSEDBG").is_some() {
                    eprintln!("close f{k}: recorded {leave} (cam {cam}) model {model} (cam {model_cam})");
                }
            }
        }
        was_leaving = leaving;
        if let Some((a, b)) = trace {
            if (a..=b).contains(&k) {
                let w = &sess.world;
                let p = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y, w.pl_ref(pi).a.facing, w.pl_ref(pi).gs.name()));
                if std::env::var("SEGMENT_HOST").is_ok() {
                    eprintln!("  t{k} host {} screen {:?} {} {}", host.thread_summary(), w.scene.screen.fade, w.scene.screen.black, w.scene.screen.dim);
                }
                if std::env::var("SEGMENT_TIMERS").is_ok() {
                    eprintln!("  t{k} timers {:?}", host.hs.timers);
                }
                if std::env::var("SEGMENT_ORDER").is_ok() {
                    let v: Vec<String> = w.ents.iter().filter(|e| !e.removed).map(|e| format!("{}:{}", e.id, w.find(e.id).and_then(|i| w.actor(i)).and_then(|a| a.sprite).unwrap_or(-1))).collect();
                    eprintln!("  t{k} order {}", v.join(" "));
                    if std::env::var("SEGMENT_ORDER").as_deref() == Ok("2") {
                        let kinds: Vec<String> = w.ents.iter().filter(|e| !e.removed).map(|e| format!("{}:{:?}", e.id, e.body).chars().take(90).collect()).collect();
                        eprintln!("  t{k} kinds {}", kinds.join(" | "));
                    }
                }
                if std::env::var("SEGMENT_FADEDBG").is_ok() {
                    eprintln!("  t{k} fadedbg {:?} dim {} black {}", w.scene.screen.fade, w.scene.screen.dim, w.scene.screen.black);
                }
                if std::env::var("SEGMENT_AIDBG").is_ok() {
                    for i in 0..w.ents.len() {
                        if let Body::Enemy(e) = &w.ents[i].body {
                            for se in &e.ai.stack {
                                if std::env::var("SEGMENT_AITREE").is_ok() {
                                    eprintln!("  t{k} tree en{i} st{} {:?}", se.state, se.tree.nodes.iter().map(|n| (n.name.clone(), n.kids.clone(), format!("{:?}", n.p))).collect::<Vec<_>>());
                                }
                                eprintln!("  t{k} ai en{i} st{} {}", se.state, { let kids = &se.tree.nodes[0].kids; let ci = se.rt[0].i as usize; let ks: Vec<String> = kids.iter().enumerate().map(|(n, &kk)| format!("{}{}", if n == ci { "*" } else { "" }, se.tree.nodes[kk as usize].name)).collect(); let kk = kids.get(ci).copied().unwrap_or(0) as usize; let r = &se.rt[kk]; format!("{} | cur {} f{} i{} d{:?} p{:?}", ks.join(","), se.tree.nodes[kk].name, r.f, r.i, &r.d[..4], se.tree.nodes[kk].p) });
                            }
                        }
                    }
                }
                if std::env::var("SEGMENT_NPC").is_ok() {
                    let npcs: Vec<String> = (0..w.ents.len()).filter(|&i| !w.ents[i].removed).filter_map(|i| match &w.ents[i].body {
                        Body::Npc(n) => Some(format!("{}:{}/{}", n.sprite_id, n.anim.raw, n.anim.tick)),
                        _ => None,
                    }).collect();
                    let game: Vec<String> = fr["actors"].as_array().into_iter().flatten().map(|a| format!("{}:{}/{}", a[1], a[6], a[8])).collect();
                    eprintln!("  t{k} npcs {} | game {}", npcs.join(" "), game.join(" "));
                }
                // SEGMENT_ENTS: every live entity's kind (shots / fx with their debug form)
                if std::env::var("SEGMENT_ENTS").is_ok() {
                    let es: Vec<String> = (0..w.ents.len()).filter(|&i| !w.ents[i].removed).map(|i| match &w.ents[i].body {
                        Body::Player(_) => format!("{}:player", w.ents[i].id),
                        Body::Enemy(e) => format!("{}:enemy{}@({},{})", w.ents[i].id, e.ty, e.a.x >> 8, e.a.y >> 8),
                        Body::Shot(s) => format!("{}:shot {:?}", w.ents[i].id, s).chars().take(160).collect(),
                        Body::Fx(f) => format!("{}:fx {:?}", w.ents[i].id, f).chars().take(80).collect(),
                        Body::Npc(n) => format!("{}:npc{}", w.ents[i].id, n.sprite_id),
                        _ => format!("{}:other", w.ents[i].id),
                    }).collect();
                    eprintln!("  t{k} ents {}", es.join(" | "));
                }
                if std::env::var("SEGMENT_FORM").is_ok() {
                    if let Some(pi) = w.player_idx(0) {
                        let pl = w.pl_ref(pi);
                        eprintln!("  t{k} form {} walk {} spd {} ss_gauge {} ss_acc {} ss3 {} cooldown {} regen_timer {} ep_acc {} ep {} gs {}", pl.form, pl.walk, pl.eff.spd, pl.ss_gauge, pl.ss_acc, pl.ss3_gauge, pl.ss_cooldown, pl.regen_timer, pl.ep_acc, pl.rec.ep, pl.gs.name());
                    }
                }
                if std::env::var("SEGMENT_PUPPETS").is_ok() {
                    let pp: Vec<String> = w.scene.puppets.iter().map(|p| format!("{}:{}/{}/{}", p.id, p.anim.raw, p.anim.frame, p.anim.tick)).collect();
                    let tr: Vec<String> = w.scene.tracks.iter().map(|t| format!("{:?}:{}{}{}", t.owner, t.queue.len(), if t.started { "s" } else { "" }, if t.holding { "h" } else { "" })).collect();
                    eprintln!("  t{k} puppets {} | wait {:?} done {} batch {} tracks {}", pp.join(" "), w.scene.director.wait.is_some(), w.scene.director.done, w.scene.director.batch, tr.join(" "));
                }
                if wf_core::menu::active(w) {
                    eprintln!("  t{k} menu {:?} lag {} ticks {} play {} game lf {} snd {}", w.menu.phase, w.menu.lag, w.menu.ticks, w.story.play_frames, lf, fr["snd"]);
                }
                eprintln!(
                    "  t{k} wf {} logic {logic} keys {keys:x} run {} depth {} busy {:?} mode {:?} pending {:?} p {:?} game p {} rng {:08x?} game {}",
                    w.frame, host.running(),
                    w.scene.depth,
                    w.field.as_ref().map(|f| f.busy),
                    w.field.as_ref().map(|f| f.mode.clone()),
                    host.hs.pending,
                    p,
                    fr["p"],
                    w.rng.state(),
                    fr["rng"],
                );
                // SEGMENT_ORDER=1: the entity list order (index: kind)
                if std::env::var("SEGMENT_ORDER").is_ok() {
                    let o: Vec<String> = w.ents.iter().enumerate().filter(|(_, e)| !e.removed).map(|(k, e)| format!("{k}:{}", match &e.body {
                        wf_core::world::Body::Player(_) => "P".to_string(),
                        wf_core::world::Body::Enemy(en) => format!("E{:x}", en.ty),
                        wf_core::world::Body::Npc(_) => "N".to_string(),
                        _ => "o".to_string(),
                    })).collect();
                    eprintln!("      order {}", o.join(" "));
                }
                // SEGMENT_GRID=tx0,ty0,tx1,ty1: the enemy-layer grid in those tiles ('#' blocked)
                if let Some(v) = std::env::var("SEGMENT_GRID").ok() {
                    let t: Vec<i32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
                    if let (Some(c), [tx0, ty0, tx1, ty1]) = (w.col.as_ref(), t.as_slice()) {
                        for ty in *ty0..*ty1 {
                            let row: String = (*tx0..*tx1).map(|tx| if c.tile(wf_core::world::Layer::Enemy, tx, ty) { '.' } else { '#' }).collect();
                            eprintln!("      grid {ty} {row}");
                        }
                    }
                }
                // the enemies: position px, AI state stack (top last), sprite
                let ens: Vec<String> = (0..w.ents.len())
                    .filter(|&i| !w.ents[i].removed)
                    .filter_map(|i| match &w.ents[i].body {
                        wf_core::world::Body::Enemy(e) => Some(format!(
                            "({},{} {:x?} s{:?} hp{} anim{:?} m{:?} t{})",
                            e.a.x >> 8,
                            e.a.y >> 8,
                            e.ai.stack.iter().map(|s| s.state).collect::<Vec<_>>(),
                            e.a.sprite,
                            e.hp1024 >> 10,
                            (e.ea.pos.raw, e.ea.pos.frame, e.ea.pos.tick),
                            e.marked,
                            e.a.anim_t
                        )),
                        wf_core::world::Body::Shot(sh) => Some(format!("shot({},{} t{} dir{} {:?})", sh.x >> 8, sh.y >> 8, sh.t, sh.dir, std::mem::discriminant(&sh.kind))),
                        wf_core::world::Body::Fx(_) => Some("fx".into()),
                        wf_core::world::Body::Breakable(b) => Some(format!("brk(box{:?} hp{} shake{})", b.bx, b.hp, b.shake)),
                        wf_core::world::Body::Pickup(p) => Some(format!("pickup({},{} z{} t{} box{:?})", p.x >> 8, p.y >> 8, p.z, p.t, p.bx)),
                        _ => None,
                    })
                    .collect();
                let pst = w.player_idx(0).map(|pi| (w.pl_ref(pi).ap.raw, w.pl_ref(pi).ap.frame, w.pl_ref(pi).ap.tick, w.pl_ref(pi).a.stats.str_));
                eprintln!("      player anim (raw, frame, tick, str) {:?} held {:?}", pst, w.field.as_ref().map(|f| (f.held, f.held_event, f.resume_held, f.event_ended, f.busy)));
                if std::env::var("SEGMENT_TRACKS").is_ok() {
                    eprintln!("      tracks {:?} wait {:?} done {}", w.scene.tracks.iter().map(|t| (t.owner, t.queue.len(), t.started, t.holding, t.queue.first().map(|c| format!("{c:?}").chars().take(40).collect::<String>()))).collect::<Vec<_>>(), w.scene.director.wait, w.scene.director.done);
                }
                eprintln!("      loot {:?} enemies {}", w.field.as_ref().map(|f| f.loot), ens.join(" "));
            }
        }
        let w = &sess.world;
        let game_text = fr["steps"].as_array().into_iter().flatten().any(|s| s[0].as_str() == Some("step") && matches!(i(&s[2]), 1 | 2 | 3 | 5 | 6));
        let ours_text = host.hs.present[presented.min(host.hs.present.len())..].iter().any(|p| !matches!(p, wf_script_core::Present::ItemGet { .. }));
        presented = host.hs.present.len();
        if game_text != ours_text {
            rep.bad(k, "text", format!("game opens a text box: {game_text}, ours: {ours_text}"));
        }
        if fr.get("vram").is_some() {
            game_vram = fr["vram"].clone();
        }
        hook(Frame::After(k, &sess.world));
        let complete = frames.get(k + 1).is_none_or(|n| i(&n["lf"]) != lf);
        // SEGMENT_LF=1: where `World.frame` leaves the game's logic counter (0x03000F94)
        if logic && std::env::var("SEGMENT_LF").is_ok() {
            let d = lf - w.g.clock(w.frame);
            if d != lf_drift {
                eprintln!("  lf drift {lf_drift} -> {d} at f{k}: frozen {} field-frozen {} menu {} mode {:?} pending {:?}", scene::frozen(w), wf_core::field::frozen(w), wf_core::menu::active(w), w.field.as_ref().map(|f| f.mode.clone()), host.hs.pending);
                lf_drift = d;
            }
        }
        if !complete {
            continue;
        }
        compare(w, fr, &game_snd[k], &mut map, k, &mut rep);
        // the object tile allocator vs the game's (recordings with `vram` rows)
        if vram_check && !game_vram.is_null() {
            if let Some(v) = w.field.as_ref().and_then(|f| f.vram.as_ref()) {
                let ours = vram_json(&v.a);
                if ours != game_vram {
                    let d = vram_diff(&ours, &game_vram);
                    if std::env::var("VRAM_DBG").is_ok() {
                        eprintln!("    f{k} vram owners {:?}", v.owners);
                    }
                    rep.bad(k, "vram", d);
                }
            }
        }
        if rep.first.is_none() {
            rep.compared = k + 1;
            // SEGMENT_RECS (report only): the characters' state at the rows where the game's changed, and the last
            // row - while the segment still matches (`recs.rs`)
            if let Some(rows) = recs_rows.as_ref() {
                while recs_i + 1 < rows.len() && rows[recs_i + 1].0 <= k {
                    recs_i += 1;
                }
                if rows.get(recs_i).is_some_and(|r| r.0 <= k) && (rows[recs_i].0 == k || k + 1 == frames.len()) {
                    recs_tally.check(k, &sess.world, &rows[recs_i].1);
                }
            }
            // SEGMENT_AREA (report only): the area's own lists at the rows where the game's changed, and the last row
            // (the walk grid's changes relative to ours after row 0, as the game's)
            if let Some(rows) = area_rows.as_ref() {
                while area_i + 1 < rows.len() && rows[area_i + 1].0 <= k {
                    area_i += 1;
                }
                if rows.get(area_i).is_some_and(|r| r.0 <= k) && rows[area_i].0 == k {
                    // (change rows only: the segment's last row is often inside the next load, the game's list torn down)
                    area_tally.check(k, &sess.world, area_g0.as_deref(), &rows[area_i].1);
                }
            }
            // SEGMENT_WORLD (report only): the world state at the rows where the game's changed, and the last row
            if let Some(rows) = world_rows.as_ref() {
                while world_i + 1 < rows.len() && rows[world_i + 1].0 <= k {
                    world_i += 1;
                }
                if rows.get(world_i).is_some_and(|r| r.0 <= k) && (rows[world_i].0 == k || k + 1 == frames.len()) {
                    world_tally.check(k, &sess.world, &host.hs, host.running(), rows[world_i].0 != k, &rows[world_i].1);
                }
            }
        }
    }
    if world_rows.is_some() {
        rep.world = Some(world_tally);
    }
    if area_rows.is_some() {
        rep.area = Some(area_tally);
    }
    if let Some(_) = recs_rows.as_ref() {
        rep.recs = Some(recs_tally);
    } else if std::env::var_os("SEGMENT_RECS").is_some() || doc["recs_exact"].as_bool() == Some(true) {
        rep.recs_missing = true;
    }
    if sim_lag {
        eprintln!("simlag {}: sim lag frames {own_lag} disagreeing {lag_bad} replayed from the recording {replayed}", doc["name"].as_str().unwrap_or("?"));
    }
    rep.errors = host.errors.clone();
    rep.started = host.started.clone();
    if std::env::var("SEGMENT_STARTED").is_ok() {
        eprintln!("  scripts started: {:?}", rep.started);
    }
    rep
}

/// the allocator in the recording's row form (tools/segments/segment.py `vram_state`)
fn vram_json(a: &wf_core::vram::ObjVram) -> Value {
    let mut v = vec![serde_json::json!(a.free_head), serde_json::json!(a.spare_head)];
    for b in &a.e {
        if b.used == 0xFF {
            v.push(serde_json::json!([255, b.hi, b.lo]));
        } else {
            v.push(serde_json::json!([b.used, b.start, b.size, b.hi, b.lo, b.fnext, b.fprev]));
        }
    }
    Value::Array(v)
}

/// the used blocks (start, size) of both, for a divergence message
fn vram_diff(ours: &Value, game: &Value) -> String {
    let used = |v: &Value| -> Vec<(i64, i64)> {
        let mut u: Vec<(i64, i64)> = v
            .as_array()
            .into_iter()
            .flatten()
            .skip(2)
            .filter_map(|b| b.as_array())
            .filter(|b| b.len() == 7 && b[0].as_i64() == Some(1))
            .map(|b| (i(&b[1]), i(&b[2])))
            .collect();
        u.sort();
        u
    };
    let (a, b) = (used(ours), used(game));
    if a == b {
        let (x, y) = (ours.as_array().unwrap(), game.as_array().unwrap());
        let d: Vec<String> = (0..x.len().min(y.len())).filter(|&k| x[k] != y[k]).take(4).map(|k| format!("[{}] ours {} game {}", k as i64 - 2, x[k], y[k])).collect();
        return format!("free / spare lists differ (same used blocks): {}", d.join(", "));
    }
    let only_a: Vec<_> = a.iter().filter(|x| !b.contains(x)).collect();
    let only_b: Vec<_> = b.iter().filter(|x| !a.contains(x)).collect();
    format!("used blocks (start, size) ours only {only_a:?}, game only {only_b:?}")
}
