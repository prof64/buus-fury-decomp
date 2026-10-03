//! `wf-import`: seed a wf-core story / field `World` from a real-game emulator state (its work RAM), so a chapter of
//! the campaign can be matched against the real game from any point instead of one sequential playthrough from New
//! Game. Spec and status: `engine/docs/segments.md`.
//!
//! * [`import_story`]: the persistent state (GameData + the 9 character records + script words).
//! * [`import_emulator_state`]: a whole World at an **area-entry cut** (the logic frame in which the leader appears
//!   in a freshly loaded area: the load's frame, or the frame its blocking on-enter script let the arrival finish).
//!   The importer rebuilds the frame the way wf-core plays it: the leader at the end of a fade-out towards the area
//!   (story, player carry-over, RNG, input history, music, banner from RAM), then wf-core's own load (objects,
//!   triggers, map, camera snap), the load's scripts through the caller's [`StoryHost`], the held frames of a
//!   blocking on-enter script. The RNG state at the load's start is not in RAM: it is the RAM's state rewound by
//!   the draws the load made ([`rewind`]), found by building twice. Every imported World is then checked against the
//!   RAM (RNG, leader, camera, every actor's sprite / position / facing / animation); mismatches are `notes`.
//! * [`host::StoryHost`]: the playthrough's per-frame script loop; [`segment`]: the segment harness.

pub mod host;
pub mod ram;
pub mod segment;

pub use host::{HostSnapshot, StoryHost};
pub use ram::GbaRam;

use ram::*;
use std::sync::Arc;
use wf_core::content::GameData;
use wf_core::field::{self, AreaId, FieldConfig, Mode};
use wf_core::rng::Rng;
use wf_core::session::Session;
use wf_core::story::{CharState, Story};
use wf_core::world::{Body, Event, World};

/// what kind of point of the game a RAM snapshot is
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutKind {
    /// a freshly loaded area: the leader exists and the fade-in has not stepped yet
    AreaEntry,
    /// a load whose on-enter script is still blocked (no leader yet): not importable, cut where the leader appears
    LoadPending,
    /// anywhere else (normal play, fades, events)
    MidArea,
}

/// what the importer reads first (cheap; tools / harnesses use it to pick cuts)
#[derive(Clone, Debug)]
pub struct CutInfo {
    pub area: AreaId,
    /// logic frame counter `0x03000F94`
    pub lf: u32,
    /// the leader: (x, y) px (footprint top-left), facing; None = no player object
    pub leader: Option<(i32, i32, u8)>,
    /// main-loop mode vtable (`ram::MODE_*`) and its brightness word (fades)
    pub mode: (u32, i32),
    pub kind: CutKind,
    /// event nesting (`0x03002A5C`)
    pub depth: u32,
}

#[derive(Debug)]
pub enum ImportError {
    /// the RAM is not at a point this importer handles (yet)
    Unsupported(String),
    /// the rebuilt state does not reproduce the RAM's (RNG never matched, leader missing, ...)
    Mismatch(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Unsupported(s) => write!(f, "unsupported cut: {s}"),
            ImportError::Mismatch(s) => write!(f, "import mismatch: {s}"),
        }
    }
}

/// an imported World and what a host needs to continue it
pub struct Imported {
    pub world: World,
    /// the game's held keys of the last logic frame (input manager +2): the first frame's edges
    pub prev_keys: u16,
    /// logic frame counter at the cut (`World.frame` = this)
    pub lf: u32,
    pub kind: CutKind,
    /// logic frames wf-core played inside the import (held frames of a blocking on-enter script)
    pub held_frames: u32,
    /// RNG draws of the rebuilt load (the RAM's state rewound by these is the load's start)
    pub draws: u32,
    /// state the import could not take from RAM or that differs from it after the rebuild (empty = exact)
    pub notes: Vec<String>,
}

// -------------------------------------------------------------------------------------------------------------
// reading

fn s16(b: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([b[o], b[o + 1]]) as i32
}
fn u16le(b: &[u8], o: usize) -> i32 {
    u16::from_le_bytes([b[o], b[o + 1]]) as i32
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub fn inspect(ram: &GbaRam) -> CutInfo {
    let area = AreaId { world: ram.r8(WS) as i32, area: ram.r8(WS + 1) as i32, variant: ram.r8(WS + 2) as i32 };
    let p = ram.player();
    let leader = (p != 0).then(|| (ram.s32(p + 0x1A0) >> 8, ram.s32(p + 0x1A4) >> 8, ram.r8(p + 0x10)));
    let m = ram.r32(MODE);
    let vt = ram.mode_vt();
    let bright = if ram.valid(m) { ram.s32(m + 4) } else { 0 };
    let depth = ram.r32(DEPTH);
    let kind = if vt == MODE_FADE_IN && bright == 0 && leader.is_some() && autowalk(ram, p) {
        CutKind::AreaEntry
    } else if leader.is_none() && depth == 0 {
        CutKind::LoadPending
    } else {
        CutKind::MidArea
    };
    CutInfo { area, lf: ram.lf(), leader, mode: (vt, bright), kind, depth }
}

fn autowalk(ram: &GbaRam, p: u32) -> bool {
    let st = ram.r32(p + 0x14);
    ram.valid(st) && ram.r32(st) == ST_AUTOWALK
}

/// the persistent state: GameData `0x03001068` (character, game bits, supplies, flags, equipment, zenie, play time),
/// the 9 character records `0x03001560`, script words L / G, `get_variant_byte`
pub fn import_story(data: &GameData, ram: &GbaRam) -> Story {
    let gd = ram.read(GD, 0x8A8);
    let mut st = Story::new_game(data);
    st.current = gd[0] as i32;
    st.game_bits = gd[1] as u32;
    for k in 0..st.supplies.len().min(83) {
        st.supplies[k] = gd[2 + k];
    }
    for k in 0..st.flags.len().min(0x8F) {
        st.flags[k] = gd[0x55 + k];
    }
    // the remake's owned list holds unequipped copies (equipped ones are the characters' slots): marked entries out
    let n = (u32le(&gd, 0xE4) as usize).min(250);
    st.equipment = (0..n).map(|k| u32le(&gd, 0xE8 + 4 * k)).filter(|v| v >> 28 == 0).map(|v| (v & 0x0FFF_FFFF) as i32).collect();
    st.zenie = u32le(&gd, 0x4F4);
    st.play_frames = (ram.lf() as i64 + u32le(&gd, 0x4D0) as i32 as i64).max(0) as u64;
    for c in 0..9usize.min(st.chars.len()) {
        let r = ram.read(REC0 + 0x68 * c as u32, 0x68);
        let mut rec = st.chars[c].rec.clone();
        rec.hp = s16(&r, 0x16);
        rec.max_hp = s16(&r, 0x18);
        rec.ep = s16(&r, 0x1A);
        rec.max_ep = s16(&r, 0x1C);
        rec.xp = u32le(&r, 0x24) as i64;
        rec.level = wf_core::progression::level_of(rec.xp, &data.xp_table);
        rec.str_ = r[0x28] as i32;
        rec.pow = r[0x29] as i32;
        rec.end = r[0x2A] as i32;
        rec.spd = r[0x2B] as i32;
        rec.points = u16le(&r, 0x2C);
        let n = (r[0x0C] as usize).min(16);
        st.chars[c] = CharState {
            rec,
            form: r[0x14] as i32,
            skills: (0..n).map(|k| r[0x0D + k] as i32).collect(),
            skill_levels: (0..n).map(|k| r[0x2F + 2 * k] as i32).collect(),
            sel: r[0x15] as i32,
            equip: [0, 1, 2, 3].map(|k| (u32le(&r, 0x3C + 4 * k) & 0x0FFF_FFFF) as i32),
            ss_cooldown: u32le(&r, 4) as i32,
            flags: r[0],
            skill_cur: (0..n).map(|k| r[0x2E + 2 * k] as i32).collect(),
        };
    }
    for k in 0..4u32 {
        st.l[k as usize] = ram.s32(WORDS_L + 4 * k);
        st.g[k as usize] = ram.s32(WORDS_G + 4 * k);
    }
    st.variant_byte = ram.r8(VARIANT_BYTE) as i32;
    st
}

/// the song the driver plays (-1 = none)
pub fn song(ram: &GbaRam) -> i32 {
    if ram.r8(SONG_PLAYER + 5) != 1 {
        return -1;
    }
    let h = ram.r32(SONG_PLAYER + 0xC);
    if (SONG_TABLE..SONG_TABLE + 53 * 20).contains(&h) {
        ((h - SONG_TABLE) / 20) as i32
    } else {
        -1
    }
}

/// the input object's press ring (+8 keys u16[64], +0x88 frame stamps u32[64]) in stamp order
fn press_ring(ram: &GbaRam) -> std::collections::VecDeque<(u16, i64)> {
    let mut v: Vec<(u16, i64)> = (0..64u32).map(|k| (ram.r16(INPUT + 8 + 2 * k), ram.r32(INPUT + 0x88 + 4 * k) as i64)).filter(|e| e.0 != 0).collect();
    v.sort_by_key(|e| e.1);
    v.into_iter().collect()
}

/// the IWRAM singletons the core keeps in `World.g` (they survive area loads): the path followers' re-plan phase,
/// candy, Buu pacified, the energy sphere, Babidi's waypoint, the teleport chain, the mummy wrap slot
fn import_globals(w: &mut World, ram: &GbaRam) {
    w.g.path_stagger = (ram.r8(0x0300_32E5) & 7) as i32;
    w.g.candy = ram.s32(0x0300_2FE4).max(0);
    w.g.buu_pacified = ram.s32(0x0300_302C);
    w.g.sphere_state = ram.r8(0x0300_32E4) as i32;
    w.g.waypoint = ram.s32(0x0300_32EC);
    w.g.tele_chain = ram.s32(0x0300_3228);
    w.g.wrap_slot = ram.s32(0x0300_30BC);
}

/// the RNG state `n` calls before `s` (the generator 0x0803FAAC is invertible)
pub fn rewind(s: [u32; 2], n: u32) -> [u32; 2] {
    let (mut a, mut b) = (s[0], s[1]);
    for _ in 0..n {
        // a' = (a + C1) ^ b ; b' = (rol(b, 3) + C2) ^ a'
        let rb = (b ^ a).wrapping_sub(0x0632_d80f);
        let ob = rb.rotate_right(3);
        let oa = (a ^ ob).wrapping_sub(0x02cb_bc71);
        a = oa;
        b = ob;
    }
    [a, b]
}

// -------------------------------------------------------------------------------------------------------------
// building

/// the leader's carry-over the load keeps (renew_player): gauges, cooldown, the form controller's accumulators, the
/// timed buff, the hittable flag
fn patch_leader(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let Some(pi) = w.player_idx(0) else { return };
    let p = ram.player();
    let ci = ram.r8(GD) as u32;
    let rec = REC0 + 0x68 * ci;
    let gd = ram.read(GD, 0x8A8);
    let form = w.pl_ref(pi).form;
    let (ss, ss3, ctl_addr) = w.data.form(form).map(|f| (f.ss, f.ss3, f.controller)).unwrap_or((false, false, 0));
    let story_form = ctl_addr == 0x3001b88 || ctl_addr == 0x3001b8c;
    let ctl = if p != 0 { ram.r32(p + 0x22C) } else { 0 };
    // (the form's controller object: the player's +0x22C, a heap object; content's `controller` names its type)
    let _ = (form, &notes);
    let pl = w.pl(pi);
    pl.ss_gauge = ram.r16(rec + 0x1E) as i32;
    pl.ss3_gauge = (ram.r16(rec + 0x20) as i64) << 16;
    pl.ss_cooldown = ram.r32(rec + 4) as i32;
    if ram.valid(ctl) {
        let acc = ram.r32(ctl + 0xC);
        if story_form {
            pl.story_cnt = acc as i32;
        } else {
            pl.regen_timer = (acc & 0xffff) as i32;
        }
        if ctl_addr == 0x3001b64 {
            pl.fus_cnt = ram.r32(ctl + 0x10) as i32;
        }
        if ss || ss3 {
            pl.ss_acc = (ram.r32(ctl + 0x14) & 0xffff) as i32;
        }
    }
    let holy = u32le(&gd, 0x4DC) as i32;
    pl.holy_left = holy.max(0);
    if holy > 0 {
        pl.boost = [0, 1, 2, 3].map(|k| u32le(&gd, 0x4E0 + 4 * k) as i32);
    }
    if p != 0 {
        pl.a.hittable = ram.r8(p + 0x1CC) != 0;
    }
    let mut pl = w.pl_ref(pi).clone();
    pl.refresh_stats(w);
    *w.pl(pi) = pl;
}

struct Built {
    world: World,
    draws: u32,
    held: u32,
    finished: bool,
}

/// one build of the area-entry frame from the load's starting RNG `rng0`
fn build_area_entry(data: &Arc<GameData>, ram: &GbaRam, info: &CutInfo, runner: &mut StoryHost, rng0: [u32; 2], frame0: u32, held_max: u32, notes: &mut Vec<String>) -> Built {
    let story = import_story(data, ram);
    let (x, y, facing) = info.leader.unwrap_or((120, 120, 0));
    let to = info.area;
    let cfg = FieldConfig { world: to.world, area: to.area, variant: to.variant, pos: Some((x, y)), facing, chars: vec![story.current], story: Some(story.clone()), rng: wf_core::battle::RngInit::State(rng0[0], rng0[1]), xp: true, arrive: false };
    let mut w = field::create(data.clone(), &cfg);
    // back to the last fade-out frame before the load: only the players remain (the load deletes the rest)
    let players = w.players.clone();
    w.ents.retain(|e| players.contains(&e.id));
    w.enemies.clear();
    w.story = story;
    if let Some(f) = w.field.as_mut() {
        f.placed.clear();
        f.placed_loot.clear();
        f.triggers.clear();
        f.patrols.clear();
        f.deferred.clear();
        f.loads = 0;
        f.mode = Mode::FadeOut { k: field::FADE - 1, to, x, y, facing: Some(facing) };
    }
    field::attach_host(&mut w, true);
    patch_leader(&mut w, ram, notes);
    w.frame = frame0;
    w.g.fc0 = 0;
    w.g.presses[0].ring = press_ring(ram);
    w.g.presses[0].pressed = 0;
    import_globals(&mut w, ram);
    w.scene.song = song(ram);
    // the banner: shown by this load when the RAM has a banner object (its name id then differed from the last one)
    let banner_obj = ram.objects().iter().any(|&o| ram.r32(o) == VT_BANNER);
    let name = data.field.as_ref().and_then(|fd| fd.area(to.world, to.area)).map(|a| a.name_id).unwrap_or(0);
    w.scene.banner = None;
    w.scene.banner_last = if banner_obj { -1 } else { name };
    w.rng = Rng::new(rng0[0], rng0[1]);
    w.events.clear();
    let calls0 = w.rng.calls;
    // the load frame's end: the fade-out's last step loads the area (step_post), then its scripts
    let mut sess = Session { data: data.clone(), world: w, config: Default::default(), prev: [0; wf_core::world::MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() };
    let evs: Vec<Event> = sess.step_post().to_vec();
    let mut finished = runner.run_events(&mut sess.world, &evs);
    // a blocking on-enter script holds the load: wf-core plays the held frames (no input) until the leader arrives
    let mut held = 0;
    let arriving = |w: &World| w.field.as_ref().is_some_and(|f| f.arrival.is_some());
    while arriving(&sess.world) && held < held_max {
        held += 1;
        runner.step(&mut sess, 0);
        finished = !arriving(&sess.world);
    }
    let w = sess.world;
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held, finished: finished || held > 0 }
}

/// Import a real-game state (work RAM) as a wf-core World ready to continue from the next logic frame.
///
/// Supported: area-entry cuts ([`CutKind::AreaEntry`]); other points return `ImportError::Unsupported`. `runner` runs
/// the load's scripts and plays the held frames of a blocking on-enter script; keep using the same host for the
/// replay (its running script / queue are part of the state).
pub fn import_emulator_state(data: &Arc<GameData>, ram: &GbaRam, runner: &mut StoryHost) -> Result<Imported, ImportError> {
    let info = inspect(ram);
    match info.kind {
        CutKind::AreaEntry => {}
        CutKind::LoadPending => return Err(ImportError::Unsupported(format!("area {:?}: the load's on-enter script is still blocked (no leader); cut at the frame the leader appears", info.area))),
        CutKind::MidArea => return Err(ImportError::Unsupported(format!("area {:?}: mid-area (mode {:#x} brightness {}, depth {}); only area-entry cuts are importable so far", info.area, info.mode.0, info.mode.1, info.depth))),
    }
    let target = ram.rng();
    // pass 1 counts the load's draws and held frames from a guess; then rebuild from the rewound state until stable
    // (a build is a pure function of (RAM, starting RNG), so the runner's script state must start clean)
    let mut guess = target;
    let mut last: Option<Built> = None;
    let mut notes = vec![];
    for _ in 0..4 {
        let mut n = vec![];
        let probe = runner.snapshot();
        let frame0 = info.lf.wrapping_sub(last.as_ref().map(|b| b.held).unwrap_or(0));
        let b = build_area_entry(data, ram, &info, runner, guess, frame0, 600, &mut n);
        let ok = b.world.rng.state() == target;
        let next = rewind(target, b.draws);
        notes = n;
        if ok && b.world.frame == info.lf {
            last = Some(b);
            break;
        }
        runner.restore(&probe);
        guess = next;
        last = Some(b);
    }
    let b = last.unwrap();
    if b.world.rng.state() != target {
        return Err(ImportError::Mismatch(format!("RNG {:08x?} after {} draws vs RAM {:08x?}", b.world.rng.state(), b.draws, target)));
    }
    let mut world = b.world;
    if !b.finished {
        notes.push("the load's scripts are still running at the cut (host state carries over)".into());
    }
    check(&mut world, ram, &mut notes);
    Ok(Imported { world, prev_keys: ram.r16(INPUT + 2) & 0x3FF, lf: info.lf, kind: info.kind, held_frames: b.held, draws: b.draws, notes })
}

/// the rebuilt World against the RAM: leader, camera, actors (by sprite, nearest first); differences become notes
fn check(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let p = ram.player();
    if let Some(pi) = w.player_idx(0) {
        let a = &w.pl_ref(pi).a;
        if p != 0 && (a.x != ram.s32(p + 0x1A0) || a.y != ram.s32(p + 0x1A4) || a.facing != ram.r8(p + 0x10)) {
            notes.push(format!("leader ({}, {}, {}) vs RAM ({}, {}, {})", a.x, a.y, a.facing, ram.s32(p + 0x1A0), ram.s32(p + 0x1A4), ram.r8(p + 0x10)));
        }
    } else {
        notes.push("no leader entity".into());
    }
    let cam = (ram.s32(CAM), ram.s32(CAM + 4));
    if (w.cameras[0].x, w.cameras[0].y) != cam {
        notes.push(format!("camera ({}, {}) vs RAM {cam:?}", w.cameras[0].x, w.cameras[0].y));
    }
    // actors: every RAM object with a sprite def vs the World's actors
    let ours: Vec<(i32, i32, i32, u8)> = w
        .ents
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.removed && matches!(e.body, Body::Npc(_) | Body::Player(_) | Body::Enemy(_)))
        .filter_map(|(i, _)| {
            let a = w.actor(i)?;
            Some((wf_core::scene::sprite_id(w, i).unwrap_or(-1), a.x, a.y, a.facing))
        })
        .collect();
    let theirs: Vec<(u32, i32, i32, u8)> = ram
        .objects()
        .into_iter()
        .filter(|&o| matches!(ram.r32(o), VT_PLAYER | VT_ENEMY | VT_NPC | VT_NPC_TALK))
        .map(|o| (o, ram.s32(o + 0x1A0), ram.s32(o + 0x1A4), ram.r8(o + 0x10)))
        .collect();
    if ours.len() != theirs.len() {
        notes.push(format!("{} actors vs {} in RAM", ours.len(), theirs.len()));
    }
    let mut used = vec![false; ours.len()];
    for (o, x, y, f) in theirs {
        match (0..ours.len()).find(|&k| !used[k] && ours[k].1 == x && ours[k].2 == y) {
            Some(k) => {
                used[k] = true;
                if ours[k].3 != f {
                    notes.push(format!("actor at ({x}, {y}) facing {} vs RAM {f}", ours[k].3));
                }
            }
            None => notes.push(format!("RAM actor {o:#x} vt {:#x} at ({}, {}) has no entity there", ram.r32(o), x as f64 / 256.0, y as f64 / 256.0)),
        }
    }
}

