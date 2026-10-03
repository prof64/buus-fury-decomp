//! Story playthrough lockstep: the opening of the campaign as the real game plays it (recorder
//! `tools/story_playthrough/record.py`, data `data/story_playthrough/*.json`: new game -> the prologue -> Snakeway
//! -> ...) replayed with the same input on a wf-core field session driven like the app's story mode
//! (`wf-app/src/story.rs` StorySim): one Rhai runtime on the shared host, the field's `FieldEvent::Run` queue,
//! blocked scripts resumed between `Session::step_world` and `Session::step_post`, `Cause::Ai` ->
//! `scene::ai_script_done`. Only the prologue scene #4 is started by the harness (the game's new-game hook); every
//! later script starts from the field itself.
//!
//! Compared per complete logic frame, with no state copied from the recording after frame 0 (the RNG runs free):
//! area, the player (position 24.8, facing), every actor (position, facing, animation raw / cel), camera, RNG,
//! sounds, text boxes opening. The verdict is the first frame where anything differs: everything before it is the
//! continuous matched span (`MIN_MATCHED`).
//!
//! `PLAYTHROUGH_VERBOSE=N` prints the first N divergences, `PLAYTHROUGH_RNG=a..b` prints every RNG call (caller) in
//! those frames, `PLAYTHROUGH_RESYNC=1` restarts the RNG from the
//! recording every logic frame (diagnosis only), `PLAYTHROUGH_TRACE=from..to` prints the host / scene state.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use wf_core::content::{DirSource, GameData};
use wf_core::field::FieldConfig;
use wf_core::input::InputFrame;
use wf_core::scene::{self, Npc};
use wf_core::session::Session;
use wf_core::story::Story;
use wf_core::world::{Body, EntityId, Event, World, MAX_PLAYERS};

/// frames of `opening.json` that must match continuously (the verdict)
const MIN_MATCHED: usize = 41423; // the whole recording: New Game -> ... -> Instant Transmission -> the Other World Tournament (Pikkon) -> Earth 2-1

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

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap()).collect()
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| v.as_u64().map(|u| u as i64).unwrap_or(0))
}

fn rd16(b: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([b[o], b[o + 1]]) as i32
}
fn rd32(b: &[u8], o: usize) -> i64 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as i64
}

/// GameData 0x03001068 + the character records 0x03001560 -> Story (as tests/lockstep.rs)
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

/// the World at frame 0 (the new-game hook starting the prologue): the snapshot's story and objects
fn build_world(data: &Arc<GameData>, doc: &Value) -> (World, BTreeMap<usize, EntityId>) {
    let init = &doc["init"];
    let objs = doc["objs"].as_array().unwrap();
    let story = story_from(data, init);
    let area = init["area"].as_array().cloned().unwrap_or_default();
    let (aw, aa, av) = (i(&area[0]) as i32, i(&area[1]) as i32, i(&area[2]) as i32);
    let rows: Vec<Row> = init["actors"].as_array().unwrap().iter().map(Row).collect();
    let player_row = rows.iter().find(|r| objs[r.k()]["player"].as_bool() == Some(true));
    let (px, py, pdir) = player_row.map(|r| (r.x() >> 8, r.y() >> 8, r.dir())).unwrap_or((120, 120, 0));
    let rng = init["rng"].as_array().map(|r| wf_core::battle::RngInit::State(i(&r[0]) as u32, i(&r[1]) as u32)).unwrap_or(wf_core::battle::RngInit::Seed(1));
    let cfg = FieldConfig { world: aw, area: aa, variant: av, pos: Some((px, py)), facing: pdir, chars: vec![story.current], story: Some(story), rng, xp: true, arrive: false };
    let mut w = Session::new_field(data.clone(), cfg).world;
    let pid = w.players[0];
    w.ents.retain(|e| e.id == pid);
    if let Some(fl) = w.field.as_mut() {
        fl.placed.clear();
    }
    w.enemies.clear();
    let mut map = BTreeMap::new();
    let mut order: Vec<EntityId> = vec![];
    for r in &rows {
        if Some(r.k()) == player_row.map(|p| p.k()) {
            let i = w.player_idx(0).unwrap();
            let p = w.pl(i);
            p.a.x = r.x();
            p.a.y = r.y();
            p.a.facing = r.dir();
            map.insert(r.k(), pid);
            order.push(pid);
            continue;
        }
        let mut n = Npc::new(data, r.sprite(), 0, 0, r.dir());
        n.a.x = r.x();
        n.a.y = r.y();
        n.anim = scene::ActAnim { raw: r.raw().max(1), frame: r.frame(), tick: r.tick(), ..Default::default() };
        let id = w.add(Body::Npc(Box::new(n)));
        map.insert(r.k(), id);
        order.push(id);
    }
    let mut ents = std::mem::take(&mut w.ents);
    ents.sort_by_key(|e| order.iter().position(|&o| o == e.id).unwrap_or(usize::MAX));
    w.ents = ents;
    let cam = init["cam"].as_array().unwrap();
    wf_core::battle::set_camera(&mut w, 0, i(&cam[0]) as i32, i(&cam[1]) as i32);
    w.cameras[0].shown = w.cameras[0].rect;
    w.scene.song = init["song"].as_i64().unwrap_or(-1) as i32;
    if let Some(r) = init["rng"].as_array() {
        w.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
    }
    wf_core::field::attach_host(&mut w, true);
    // a new game has no player object until the prologue warps to Snakeway
    if player_row.is_none() {
        scene::send_offstage(&mut w);
    }
    w.events.clear();
    (w, map)
}

// ---------------------------------------------------------------------------------------------------------------
// the story host (wf-app StorySim's loop)

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

#[derive(Default)]
struct Report {
    logic: usize,
    compared: usize,
    first: Option<(usize, String)>,
    counts: BTreeMap<&'static str, usize>,
    shown: usize,
    /// actor comparisons made (sanity: the verdict is not vacuous)
    actors: usize,
}

impl Report {
    fn bad(&mut self, k: usize, what: &'static str, msg: String) {
        *self.counts.entry(what).or_default() += 1;
        let limit: usize = std::env::var("PLAYTHROUGH_VERBOSE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        if self.shown < limit {
            eprintln!("    f{k} {what}: {msg}");
            self.shown += 1;
        }
        if self.first.is_none() {
            self.first = Some((k, format!("{what}: {msg}")));
        }
    }
}

fn compare(w: &World, fr: &Value, es: &[String], map: &mut BTreeMap<usize, EntityId>, k: usize, rep: &mut Report) {
    // area
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
    let present: Vec<usize> = fr["actors"].as_array().into_iter().flatten().map(|a| Row(a).k()).collect();
    map.retain(|k, _| present.contains(k));
    for a in fr["actors"].as_array().into_iter().flatten() {
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
            rep.bad(k, "missing", format!("obj #{} sprite {} has no entity", r.k(), r.sprite()));
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
                rep.bad(k, "anim", format!("#{} sprite {}: raw {} cel {} vs game raw {} cel {}", r.k(), r.sprite(), ours, frame, r.raw(), r.frame()));
            }
        }
    }
    // entities with a sprite the game does not show (extra objects)
    let shown: Vec<EntityId> = fr["actors"].as_array().into_iter().flatten().filter_map(|a| map.get(&Row(a).k()).copied()).collect();
    for (ei, e) in w.ents.iter().enumerate().filter(|(_, e)| !e.removed) {
        let is_actor = matches!(e.body, Body::Npc(_) | Body::Player(_) | Body::Enemy(_));
        if is_actor && !shown.contains(&e.id) {
            rep.bad(k, "extra", format!("entity {} sprite {:?} ({}) not in the game", e.id, scene::sprite_id(w, ei), if matches!(e.body, Body::Npc(_)) { "npc" } else if matches!(e.body, Body::Player(_)) { "player" } else { "enemy" }));
            break;
        }
    }
    let cam = fr["cam"].as_array().unwrap();
    let c = &w.cameras[0];
    if c.x != i(&cam[0]) as i32 || c.y != i(&cam[1]) as i32 {
        rep.bad(k, "cam", format!("({}, {}) vs game ({}, {})", c.x as f64 / 1024.0, c.y as f64 / 1024.0, i(&cam[0]) as f64 / 1024.0, i(&cam[1]) as f64 / 1024.0));
    }
    let io = fr["io"].as_array().unwrap();
    let game_black = i(&io[5]) == 0;
    // the field's own transition fade (brightness 256 -> 0 / 0 -> 256 by 8 per logic frame; RenderView does not
    // carry it yet) or a cutscene fade
    let field_black = w.field.as_ref().is_some_and(|f| {
        let b = match f.mode {
            wf_core::field::Mode::FadeOut { k, .. } => 256 - 8 * k,
            wf_core::field::Mode::FadeIn { k } => 8 * k,
            wf_core::field::Mode::Normal => 256,
        };
        (31 * b.clamp(0, 256)) >> 8 == 0
    });
    if game_black != (scene::palette_black(w) || field_black) {
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

fn trace_range() -> Option<(usize, usize)> {
    let v = std::env::var("PLAYTHROUGH_TRACE").ok()?;
    let (a, b) = v.split_once("..")?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn replay(data: &Arc<GameData>, files: &BTreeMap<String, String>, doc: &Value) -> Report {
    let mut rep = Report::default();
    let (world, mut map) = build_world(data, doc);
    let mut sess = Session { data: data.clone(), world, config: Default::default(), prev: [0; MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() };
    // the shared script host (wf-import): the same loop the segment harness and the app's story mode run
    let mut host = wf_import::StoryHost::new(data, files).expect("compile");
    let frames = doc["frames"].as_array().unwrap();
    // the game's sounds per logic frame (a long logic frame plays its sounds in a later video frame)
    let mut game_snd: Vec<Vec<String>> = vec![vec![]; frames.len()];
    {
        let mut last = 0usize;
        let mut prev_lf = i(&doc["init"]["lf"]);
        let mut prev_lag = false;
        for (k, fr) in frames.iter().enumerate() {
            let lf = i(&fr["lf"]);
            let logic = k == 0 || lf != prev_lf;
            prev_lf = lf;
            // a non-logic (lag) video frame's sounds belong to the logic frame it continues; the next logic frame
            // keeps its own (a coin collected right after a lag frame: frame 21,363)
            let _ = prev_lag;
            let to = if !logic { last } else { k };
            game_snd[to].extend(emu_sounds(fr));
            game_snd[to].sort();
            if logic {
                last = k;
            }
            prev_lag = !logic;
        }
    }
    let resync = std::env::var("PLAYTHROUGH_RESYNC").is_ok();
    let trace = trace_range();
    let mut prev_held = 0u16;
    let mut lf_prev = i(&doc["init"]["lf"]);
    let mut presented = 0usize;
    let rng_trace: Option<(usize, usize)> = std::env::var("PLAYTHROUGH_RNG").ok().and_then(|v| {
        let (a, b) = v.split_once("..")?;
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    for (k, fr) in frames.iter().enumerate() {
        if let Some((a, b)) = rng_trace {
            let on = (a..=b).contains(&k);
            if on {
                eprintln!("  -- frame {k} rng calls:");
            }
            wf_core::rng::TRACE.store(on, std::sync::atomic::Ordering::Relaxed);
        }
        let keys = i(&fr["keys"]) as u16;
        let lf = i(&fr["lf"]);
        let logic = k == 0 || lf != lf_prev;
        lf_prev = lf;
        if k == 0 {
            // the new-game hook: the prologue scene starts before this frame's update
            let ev = i(&doc["event"]) as u16;
            host.start_event(&mut sess.world, ev);
            scene::camera_update(&mut sess.world);
        } else if logic {
            rep.logic += 1;
            let mut inputs = [InputFrame::default(); MAX_PLAYERS];
            inputs[0] = InputFrame::from_held(keys, prev_held);
            prev_held = keys;
            if resync {
                if let Some(r) = frames[k - 1]["rng"].as_array() {
                    sess.world.rng.set_state([i(&r[0]) as u32, i(&r[1]) as u32]);
                }
            }
            // one logic frame in the shared host's order (wf_import::StoryHost::step_inputs)
            host.step_inputs(&mut sess, inputs);
        }
        if let Some((a, b)) = trace {
            if (a..=b).contains(&k) {
                let w = &sess.world;
                let steps: Vec<String> = fr["steps"].as_array().into_iter().flatten().map(|s| s.to_string()).collect();
                let p = w.player_idx(0).map(|pi| (w.pl_ref(pi).a.x, w.pl_ref(pi).a.y, w.pl_ref(pi).a.facing, w.pl_ref(pi).gs.name(), (w.pl_ref(pi).ap.raw, w.pl_ref(pi).ap.frame, w.pl_ref(pi).ap.tick)));
                let picks: Vec<(i32, i32, i32, i32, i32, bool, [i32; 4])> = w.ents.iter().filter_map(|e| match &e.body {
                    Body::Pickup(pk) => Some((pk.x, pk.y, pk.vx, pk.vy, pk.z, pk.landed, pk.coin_rect())),
                    _ => None,
                }).collect();
                let ens: Vec<String> = w.ents.iter().filter_map(|e| match &e.body {
                    Body::Enemy(en) => Some(format!("{}:{:?} f{} st{:?} ({},{}) anim {:?} mv {} mark {:?}", e.id, en.a.sprite, en.a.facing, en.ai.stack.iter().map(|s| format!("{:x}", s.state)).collect::<Vec<_>>(), en.a.x, en.a.y, en.ea, en.moving, en.marked)),
                    _ => None,
                }).collect();
                if !ens.is_empty() {
                    eprintln!("    enemies {ens:?}");
                }
                // PLAYTHROUGH_NPC=sprite: that NPC's animation (raw, frame, tick) against the game's actor row
                if let Some(sp) = std::env::var("PLAYTHROUGH_NPC").ok().and_then(|v| v.parse::<i32>().ok()) {
                    for e in w.ents.iter().filter(|e| !e.removed) {
                        if let Body::Npc(n) = &e.body {
                            if n.sprite_id == sp {
                                let g: Vec<&Value> = fr["actors"].as_array().into_iter().flatten().filter(|a| a[1].as_i64() == Some(sp as i64)).collect();
                                eprintln!("    npc {sp} ours raw {} frame {} tick {} | game {:?}", n.anim.raw, n.anim.frame, n.anim.tick, g.first().map(|a| (a[6].clone(), a[7].clone(), a[8].clone())));
                            }
                        }
                    }
                }
                // PLAYTHROUGH_ENTS=1: every live entity with an actor (kind, sprite, position, foot)
                if std::env::var("PLAYTHROUGH_ENTS").is_ok() {
                    if let Some(c) = w.col.as_ref() {
                        if let Some(g) = c.walk.as_ref() {
                            let d: Vec<(usize, usize, u8)> = (0..g.len()).filter(|&k| g[k] != c.map.walk[k]).map(|k| ((k % 256) * 8, (k / 256) * 8, g[k])).collect();
                            eprintln!("    walk grid changes (x, y, v): {d:?}");
                        }
                    }
                    for (i, e) in w.ents.iter().enumerate() {
                        if let Some(a) = w.actor(i).filter(|_| !e.removed) {
                            eprintln!("    ent {} {:?} sprite {:?} ({}, {}) foot {}x{} solid {:?}", e.id, std::mem::discriminant(&e.body), a.sprite, a.x as f64 / 256.0, a.y as f64 / 256.0, a.foot_w, a.foot_h, a.body_box());
                        }
                    }
                }
                eprintln!("    field mode {:?} arrival {:?}", w.field.as_ref().map(|f| f.mode.clone()), w.field.as_ref().map(|f| f.arrival.is_some()));
                if let Some(pi) = w.player_idx(0) {
                    let pl = w.pl_ref(pi);
                    eprintln!("    skills {:?} levels {:?} sel {}", pl.skills, pl.skill_level, pl.sel);
                }
                if !w.g.shakes.is_empty() {
                    eprintln!("    shakes {:?} cam {:?}", w.g.shakes, (w.cameras[0].x, w.cameras[0].y));
                }
                if !picks.is_empty() {
                    eprintln!("    loot {:?}", w.field.as_ref().map(|f| f.loot));
                    eprintln!("    pickups {picks:?}");
                }
                eprintln!(
                    "  t{k} logic {logic} keys {keys:x} run {} depth {} dialog {:?} busy {:?} pending {:?} p {:?} game p {} rng {:08x?} game {} steps {:?}",
                    host.running(),
                    w.scene.depth,
                    w.scene.dialog.as_ref().map(|d| (d.kind, d.phase, d.page, d.pages, d.t)),
                    w.field.as_ref().map(|f| f.busy),
                    host.hs.pending,
                    p,
                    fr["p"],
                    w.rng.state(),
                    fr["rng"],
                    steps
                );
            }
        }
        if !logic {
            sess.world.events.clear();
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
    for e in &host.errors {
        eprintln!("  script error: {e}");
    }
    eprintln!("  scripts started by the field: {:?}", host.started);
    rep
}

#[test]
fn story_playthrough_matches_the_game() {
    let p = repo().join("data/story_playthrough/opening.json");
    let Ok(bytes) = std::fs::read(&p) else { return };
    let Some(rom) = wf_rom::testutil::rom() else { return };
    let font = wf_ui::font::Font::load(&rom, "dialog", 0x0806_EE6C);
    wf_script_core::text::set_dialog_font((0..256u16).map(|c| font.advance(c)).collect());
    let data = Arc::new(GameData::load(&DirSource { root: content_dir() }).expect("content"));
    let set = wf_script::ScriptSet::load_all(&rom);
    let (files, errs) = wf_script::export::export(&rom, &set);
    assert!(errs.is_empty(), "decompile errors: {errs:?}");
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let rep = replay(&data, &files, &doc);
    let n = doc["frames"].as_array().unwrap().len();
    eprintln!(
        "playthrough: {} video frames ({} logic), continuous match up to frame {} | first divergence {:?} | counts {:?} | {} actor checks",
        n, rep.logic, rep.compared, rep.first, rep.counts, rep.actors
    );
    assert!(rep.compared >= MIN_MATCHED.min(n), "matched {} < {MIN_MATCHED}: {:?}", rep.compared, rep.first);
}
