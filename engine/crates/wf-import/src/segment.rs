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
    let menu_screen = wf_core::menu::active(w) && !matches!(w.menu.phase, wf_core::menu::Phase::FadeOut { .. });
    let present: Vec<usize> = fr["actors"].as_array().into_iter().flatten().map(|a| Row(a).k()).collect();
    if !menu_screen {
        map.retain(|k, _| present.contains(k));
    }
    for a in fr["actors"].as_array().into_iter().flatten().filter(|_| !menu_screen) {
        let r = Row(a);
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
    for (ei, e) in w.ents.iter().enumerate().filter(|(_, e)| !e.removed && !menu_page) {
        let is_actor = matches!(e.body, Body::Npc(_) | Body::Player(_) | Body::Enemy(_));
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
    let ram = GbaRam::from_json(&doc["init"]["ram"])?;
    let mut host = StoryHost::new(data, files)?;
    let imp = import_emulator_state(data, &ram, &mut host).map_err(|e| e.to_string())?;
    host.prev_held = imp.prev_keys;
    Ok((imp, host))
}

/// replay a segment from its imported start state: the first divergence and the matched span
pub fn replay(data: &Arc<GameData>, files: &BTreeMap<String, String>, doc: &Value) -> Report {
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
    let mut lf_prev = i(&doc["init"]["lf"]);
    let mut presented = host.hs.present.len();
    for (k, fr) in frames.iter().enumerate() {
        let keys = i(&fr["keys"]) as u16;
        let lf = i(&fr["lf"]);
        let logic = lf != lf_prev;
        lf_prev = lf;
        if logic {
            rep.logic += 1;
            // SEGMENT_TRACE with SEGMENT_RNG=1: every RNG call's source location inside the traced frames
            let rng_trace = trace.is_some_and(|(a, b)| (a..=b).contains(&k)) && std::env::var("SEGMENT_RNG").is_ok();
            if rng_trace {
                eprintln!("  rng calls of t{k}:");
                wf_core::rng::TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            host.step(&mut sess, keys);
            wf_core::rng::TRACE.store(false, std::sync::atomic::Ordering::Relaxed);
        } else if wf_core::menu::active(&sess.world) && std::env::var("SEG_MENU_LOGIC").is_err() {
            // a menu screen runs per video frame (menu::step models the game's redraw lag itself)
            host.step(&mut sess, keys);
        } else {
            sess.world.events.clear();
        }
        if let Some((a, b)) = trace {
            if (a..=b).contains(&k) {
                let w = &sess.world;
                let p = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y, w.pl_ref(pi).a.facing, w.pl_ref(pi).gs.name()));
                if std::env::var("SEGMENT_NPC").is_ok() {
                    let npcs: Vec<String> = (0..w.ents.len()).filter(|&i| !w.ents[i].removed).filter_map(|i| match &w.ents[i].body {
                        Body::Npc(n) => Some(format!("{}:{}/{}", n.sprite_id, n.anim.raw, n.anim.tick)),
                        _ => None,
                    }).collect();
                    let game: Vec<String> = fr["actors"].as_array().into_iter().flatten().map(|a| format!("{}:{}/{}", a[1], a[6], a[8])).collect();
                    eprintln!("  t{k} npcs {} | game {}", npcs.join(" "), game.join(" "));
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
                            "({},{} {:x?} s{:?} hp{} anim{:?})",
                            e.a.x >> 8,
                            e.a.y >> 8,
                            e.ai.stack.iter().map(|s| s.state).collect::<Vec<_>>(),
                            e.a.sprite,
                            e.hp1024 >> 10,
                            (e.ea.pos.raw, e.ea.pos.frame, e.ea.pos.tick)
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
        let complete = frames.get(k + 1).is_none_or(|n| i(&n["lf"]) != lf);
        if !complete {
            continue;
        }
        compare(w, fr, &game_snd[k], &mut map, k, &mut rep);
        if rep.first.is_none() {
            rep.compared = k + 1;
        }
    }
    rep.errors = host.errors.clone();
    rep.started = host.started.clone();
    if std::env::var("SEGMENT_STARTED").is_ok() {
        eprintln!("  scripts started: {:?}", rep.started);
    }
    rep
}
