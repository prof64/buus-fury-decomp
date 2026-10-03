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
    st.lf_bias = ram.lf() as i64 - st.play_frames as i64;
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
            fusion: u16le(&r, 2) as u16,
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
        f.mode = Mode::FadeOut { k: field::FADE - 1, to, x, y, facing: Some(facing), step: 0 };
    }
    field::attach_host(&mut w, true);
    patch_leader(&mut w, ram, notes);
    w.frame = frame0;
    w.g.fc0 = 0;
    w.g.presses[0].ring = press_ring(ram);
    w.g.presses[0].pressed = 0;
    import_globals(&mut w, ram);
    w.scene.song = song(ram);
    // the HUD: GameData +1 bit 1 (hide_hud 0x30 clears it: the HUD object removes itself and stays away through the
    // loads until a show_hud - a save made on the world map loads without one; dungeon chapters db2 / db3)
    w.scene.hud_hidden = w.story.game_bits & 2 == 0;
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

/// the running event at a cut inside an event: (event, node, params) from the innermost scene object on the IWRAM
/// stack (`0x0801246E`: +0 header, +8 u16 next state, +0x14 param block pointer; SCRIPT.md B4)
pub fn running_event(ram: &GbaRam, runner: &StoryHost) -> Option<(u16, u16, [i32; 4])> {
    let mut a = 0x0300_7000u32;
    while a < 0x0300_8000 - 0x18 {
        let h = ram.r32(a);
        let pp = ram.r32(a + 0x14);
        if let Some(ev) = runner.event_of_header(h).filter(|_| matches!(pp >> 24, 2 | 3) && ram.valid(pp)) {
            let next = ram.r16(a + 8) as u16;
            if let Some(node) = runner.node_before(ev, next) {
                let params = [0, 4, 8, 12].map(|o| ram.s32(pp + o));
                return Some((ev, node, params));
            }
        }
        a += 4;
    }
    None
}

/// a field restored by `return_to_field` inside an event (its objects keep their state from before the scene): the
/// area loaded at the leader, every non-player object replaced by the RAM's actors (enemies: type, 24.8 position,
/// facing, HP, AI state stack; NPCs: sprite, position, facing, animation from the recording's rows), the event
/// blocked in the return's fade-in phase
#[allow(clippy::too_many_arguments)]
fn build_returned(_data: &Arc<GameData>, ram: &GbaRam, runner: &mut StoryHost, ev: (u16, u16, [i32; 4]), k: usize, mut w: World, calls0: u32, notes: &mut Vec<String>) -> Built {
    use wf_script_core::{Pending, Phase};
    let to = w.field.as_ref().map(|f| f.area).unwrap_or_default();
    let (x, y, facing) = (ram.s32(ram.player() + 0x1A0) >> 8, ram.s32(ram.player() + 0x1A4) >> 8, ram.r8(ram.player() + 0x10));
    let list = vec![
        Phase::Fade { kind: wf_core::scene::FadeKind::Black, out: true, frames: 16 },
        Phase::ReturnToField,
        Phase::Fade { kind: wf_core::scene::FadeKind::Black, out: false, frames: 16 },
    ];
    // (the return's fade-in started in the load's frame)
    runner.continue_event(&mut w, ev.0, ev.1, ev.2, k, Some(Pending::Phases { list, k: 2, started: true }));
    w.scene.screen.black = 16;
    w.scene.screen.dim = 256;
    wf_core::scene::start_fade(&mut w, wf_core::scene::FadeKind::Black, false, 16);
    field::load_area(&mut w, to, x, y, Some(facing));
    let evs: Vec<Event> = std::mem::take(&mut w.events);
    let _ = evs;
    // (the players arrive at once: the restored field had them - the 3D map's flyer is made by the arrival)
    field::finish_arrival(&mut w);
    if let Some(f) = w.field.as_mut() {
        f.arrival = None;
        f.mode = Mode::Normal;
    }
    // (the event still holds the field: the arrival does not release it)
    w.scene.depth = 1;
    field::set_busy(&mut w, true);
    field_from_ram(&mut w, ram, notes);
    notes.push(format!("event s{:03} at node {}: return_to_field, {} actions before it dropped; mid-area objects from RAM", ev.0, ev.1, k));
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held: 0, finished: true }
}

/// an enemy's AI node runtimes from RAM (mid-area imports): the game's runtime tree mirrors the state's definition
/// tree - Sequence (vt 0x0804AAF4): +4 cursor, +8 count, +0xC child runtimes; Selector (0x0804A710): +4 count, +8
/// children; the path followers (ChasePlayer ...: +4 speed, +8 budget, +0xC / +0x10 waypoint, +0x14 valid, +0x18
/// re-plan counter, +0x1C / +0x20 goal); other node kinds keep a fresh runtime (a note says which)
fn import_ai_rt(w: &mut World, ei: usize, lvl: usize, ram: &GbaRam, r: u32, notes: &mut Vec<String>) {
    use wf_core::ai::K;
    let Some(tree) = w.en_ref(ei).ai.stack.get(lvl).map(|s| Arc::clone(&s.tree)) else { return };
    let mut todo = vec![(0usize, r)];
    let mut rts: Vec<(usize, wf_core::ai::NodeRt)> = vec![];
    while let Some((n, r)) = todo.pop() {
        if !ram.valid(r) || n >= tree.nodes.len() {
            continue;
        }
        let node = &tree.nodes[n];
        let mut rt = wf_core::ai::NodeRt::default();
        match node.k {
            K::Sequence => {
                rt.i = ram.s32(r + 4);
                for (k, &c) in node.kids.iter().enumerate() {
                    todo.push((c as usize, ram.r32(r + 0xC + 4 * k as u32)));
                }
            }
            K::Selector => {
                for (k, &c) in node.kids.iter().enumerate() {
                    todo.push((c as usize, ram.r32(r + 8 + 4 * k as u32)));
                }
            }
            K::ChasePlayer | K::ChasePlayerThroughRocks | K::MoveToPoint | K::WanderToRandomPoint => {
                rt.path = wf_core::ai::PathRt {
                    speed: ram.s32(r + 4),
                    budget: ram.s32(r + 8),
                    wx: ram.s32(r + 0xC),
                    wy: ram.s32(r + 0x10),
                    valid: ram.r8(r + 0x14) as i32,
                    counter: ram.r8(r + 0x18) as i32,
                    gx: ram.s32(r + 0x1C),
                    gy: ram.s32(r + 0x20),
                };
            }
            K::Chance | K::GotoState | K::SubState | K::IfPlayerWithin | K::IfPlayerFarther | K::IfPlayerInMeleeRange | K::IfTimeInState | K::IfCooldownOver | K::IfLastKidBuu => {}
            k => notes.push(format!("ai runtime of {k:?} (state {:#x} node {n}) not imported", tree.state)),
        }
        rts.push((n, rt));
    }
    let e = w.en(ei);
    if let Some(st) = e.ai.stack.get_mut(lvl) {
        for (n, rt) in rts {
            if let Some(x) = st.rt.get_mut(n) {
                *x = rt;
            }
        }
    }
}

/// the live walk / enemy grids from RAM (mid-area imports)
fn import_grids(w: &mut World, ram: &GbaRam) {
    let g = wf_core::content::GRID;
    let read = |off: u32| -> Option<Vec<u8>> {
        let b = ram.r32(CAM + off);
        if !ram.valid(b) {
            return None;
        }
        let bytes = ram.read(b, 32 * g.min(256));
        let mut out = vec![0u8; g * g];
        for ty in 0..g.min(256) {
            for tx in 0..g.min(256) {
                out[ty * g + tx] = (bytes[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1;
            }
        }
        Some(out)
    };
    let (walk, enemy) = (read(0xA8), read(0xB0));
    if let Some(c) = w.col.as_mut() {
        if let Some(e) = enemy {
            c.enemy = e;
        }
        if let Some(wk) = walk {
            if wk != c.map.walk {
                c.walk = Some(wk);
            }
        }
    }
}

/// the field's live state from RAM (mid-area imports: a field restored after a scene, or any frame): the actors by
/// match and patch, the leader's 24.8 position / animation / script pose, the grids, the stagger, the 3-D map's
/// flyer and camera, the 2-D camera
fn field_from_ram(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    import_objects(w, ram, notes);
    let rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    patch_leader(w, ram, notes);
    // the walk and enemy grids as they stand (map object +0xA8 / +0xB0: bitmaps, 32 bytes a row, bit = free)
    import_grids(w, ram);
    // (building the enemies entered their states: the path stagger as the RAM has it)
    w.g.path_stagger = (ram.r8(0x0300_32E5) & 7) as i32;
    // the leader as it stands: its 24.8 position (the load put it on the pixel) and animation (an idle's tick runs
    // on through the scene)
    if let Some(pi) = w.player_idx(0) {
        let po = ram.player();
        let p = w.pl(pi);
        p.a.x = ram.s32(po + 0x1A0);
        p.a.y = ram.s32(po + 0x1A4);
        p.a.facing = ram.r8(po + 0x10);
    }
    if let (Some(&(_, raw, cel, tick)), Some(pi)) = (rows.get(&ram.player()), w.player_idx(0)) {
        // a script's frame-range controller (0x0804DE58: +4 pass, +8 ticks a cel, +0xC passes, +0x10 loop start,
        // +0x14 last cel) runs the leader as a scene puppet
        let st = ram.r32(ram.player() + 0x14);
        if ram.valid(st) && ram.r32(st) == 0x0804_DE58 {
            let mode = wf_core::scene::AnimMode::Frames { each: ram.s32(st + 8), loops: ram.s32(st + 0xC), loop_start: ram.s32(st + 0x10), last: ram.s32(st + 0x14), pass: ram.s32(st + 4) };
            let aid = wf_core::player::ANIM_RAW.iter().position(|&r| r == raw).unwrap_or(0);
            wf_core::player::script_pose(w, pi, aid, cel);
            let id = w.ents[pi].id;
            let anim = wf_core::scene::ActAnim { raw, frame: cel, tick, mode, ..wf_core::scene::ActAnim::of(aid as i32) };
            w.scene.puppets.retain(|p| p.id != id);
            w.scene.puppets.push(wf_core::scene::Puppet { id, anim });
        }
        let p = w.pl(pi);
        p.ap.raw = raw;
        p.ap.frame = cel;
        p.ap.tick = tick;
    }
    // the 3D map's flyer: altitude +0x1A8, heading +0x1AC, the flight state (+4 reversing, +8 speed, +0xC turn), its
    // animation, the 3D camera (map object +0 x, +4 y, +8 height, +0xC heading, +0x10 pitch)
    if wf_core::fieldworld::flying(w) {
        let po = ram.player();
        let st = ram.r32(po + 0x14);
        let row = rows.get(&po).copied();
        if let Some(fl) = w.field.as_mut().and_then(|f| f.flight.as_mut()) {
            fl.z = ram.s32(po + 0x1A8);
            fl.h = ram.s32(po + 0x1AC);
            if ram.valid(st) {
                fl.rev = ram.r32(st + 4) != 0;
                fl.spd = ram.s32(st + 8);
                fl.turn = ram.s32(st + 0xC);
            }
            fl.tick = ram.r16(po + 0x12) as i32;
            fl.frame = ram.r8(po + 0x11) as i32;
            if let Some((_, raw, _, _)) = row {
                fl.raw = raw;
            }
            fl.cam = [ram.s32(CAM), ram.s32(CAM + 4), ram.s32(CAM + 8), ram.s32(CAM + 0xC), ram.s32(CAM + 0x10)];
            // (the camera object caches the altitude of its last pitch: the pitch stays until z changes)
            fl.zc = Some(fl.z);
        }
    }
    // the camera as the field left it (`return_to_field` puts the saved one back)
    w.scene.cam.mode = wf_core::scene::CamMode::Player;
    wf_core::battle::set_camera(w, 0, ram.s32(CAM), ram.s32(CAM + 4));
    // (a camera that is not where following the leader keeps it was held by a script before the scene: fixed)
    {
        let mut probe = w.clone();
        field::world_mode_cameras(&mut probe);
        if (probe.cameras[0].x, probe.cameras[0].y) != (w.cameras[0].x, w.cameras[0].y) {
            w.scene.cam.mode = wf_core::scene::CamMode::Fixed;
        }
    }
}

/// the field's actors from RAM, by **match and patch** (mid-area imports): every RAM actor (enemy / NPC object) is
/// matched to an entity the area's own load made - same enemy type / NPC sprite, the nearest unclaimed one - and
/// that entity is patched to the RAM's state (it keeps what the load attached: patrol runners, placements, talk
/// events); RAM actors without a match are built; entities no RAM actor claims go. The actors end in the RAM's
/// world-list order (the update order).
fn import_objects(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    let players = w.players.clone();
    // our candidates: (entity id, kind key) - enemies by type, NPCs by sprite id
    let key_of = |w: &World, i: usize| -> Option<(u8, i32)> {
        match &w.ents[i].body {
            Body::Enemy(e) => Some((0, e.ty)),
            Body::Npc(n) => Some((1, n.sprite_id)),
            _ => None,
        }
    };
    let mut free: Vec<(wf_core::world::EntityId, (u8, i32))> =
        (0..w.ents.len()).filter(|&i| !w.ents[i].removed && !players.contains(&w.ents[i].id)).filter_map(|i| key_of(w, i).map(|k| (w.ents[i].id, k))).collect();
    let mut order: Vec<wf_core::world::EntityId> = vec![];
    // the world list is updated last to first: RAM order = our entity order
    for o in ram.objects() {
        let vt = ram.r32(o);
        let (ox, oy, of) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4), ram.r8(o + 0x10));
        let key = match vt {
            VT_ENEMY => (0u8, ((ram.r32(o + 0x224).wrapping_sub(0x0871_1274)) / 28) as i32),
            VT_NPC | VT_NPC_TALK => match rows.get(&o) {
                Some(&(sprite, ..)) => (1u8, sprite),
                None => continue,
            },
            _ => continue,
        };
        // the nearest unclaimed entity of the same kind
        let pick = free
            .iter()
            .enumerate()
            .filter(|(_, c)| c.1 == key)
            .filter_map(|(k, c)| w.find(c.0).and_then(|i| w.actor(i)).map(|a| (k, ((a.x - ox) as i64).abs() + ((a.y - oy) as i64).abs())))
            .min_by_key(|x| x.1)
            .map(|x| x.0);
        let id = match (pick, key.0) {
            (Some(k), _) => free.remove(k).0,
            (None, 0) => {
                let states = ai_states(ram, o);
                let Some(ei) = wf_core::battle::spawn_enemy_at(w, key.1, ox >> 8, oy >> 8, 0, states.first().copied(), true) else {
                    notes.push(format!("enemy type {} at ({ox}, {oy}) not built", key.1));
                    continue;
                };
                w.ents[ei].id
            }
            (None, _) => {
                let n = wf_core::scene::Npc::new(&w.data, key.1, ox >> 8, oy >> 8, of);
                w.add(Body::Npc(Box::new(n)))
            }
        };
        let Some(i) = w.find(id) else { continue };
        order.push(id);
        match key.0 {
            0 => patch_enemy(w, i, ram, o, rows.get(&o).copied(), notes),
            _ => {
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.a.x = ox;
                    n.a.y = oy;
                    n.a.facing = of;
                    if let Some(&(_, raw, cel, tick)) = rows.get(&o) {
                        n.anim.raw = raw;
                        n.anim.frame = cel;
                        n.anim.tick = tick;
                    }
                    // the hold state 0x03001B00 (`set_pose`): frozen on its cel
                    if ram.r32(o + 0x14) == 0x0300_1B00 {
                        n.anim.mode = wf_core::scene::AnimMode::Pose;
                    }
                }
            }
        }
    }
    // the load's objects the field no longer has
    for (id, _) in free {
        if let Some(i) = w.find(id) {
            if matches!(w.ents[i].body, Body::Enemy(_)) {
                wf_core::enemy::unmark_self(w, i);
            }
            w.ents[i].removed = true;
        }
    }
    w.ents.retain(|e| !e.removed);
    let gone: Vec<wf_core::world::EntityId> = w.enemies.iter().copied().filter(|&id| w.find(id).is_none()).collect();
    w.enemies.retain(|id| !gone.contains(id));
    if let Some(f) = w.field.as_mut() {
        f.placed.retain(|p| !gone.contains(&p.0));
        f.patrols.retain(|p| !gone.contains(&p.entity));
    }
    // the actors in the RAM's order, after the other objects
    let mut actors = vec![];
    for id in &order {
        if let Some(i) = w.ents.iter().position(|e| e.id == *id) {
            actors.push(w.ents.remove(i));
        }
    }
    w.ents.extend(actors);
}

/// an enemy's AI state stack (obj+0x238: +4 depth, +8 {runtime, def} per level; def = 0x0870F514 + 8 * state)
fn ai_states(ram: &GbaRam, o: u32) -> Vec<i32> {
    let st = o + 0x238;
    let n = ram.r32(st + 4).min(8);
    (0..n).map(|i| (ram.r32(st + 12 + 8 * i).wrapping_sub(0x0870_F514) / 8) as i32).collect()
}

/// an enemy entity set to a RAM enemy object's state
fn patch_enemy(w: &mut World, ei: usize, ram: &GbaRam, o: u32, row: Option<(i32, i32, i32, i32)>, notes: &mut Vec<String>) {
    let states = ai_states(ram, o);
    // the state stack as the RAM has it (re-entered: their runtimes come next)
    if let Some(&first) = states.first() {
        wf_core::nodes::reset(w, ei, first as u16);
        for &s in states.iter().skip(1) {
            wf_core::nodes::push(w, ei, s as u16);
        }
    }
    for lvl in 0..states.len() {
        let r = ram.r32(o + 0x238 + 8 + 8 * lvl as u32);
        import_ai_rt(w, ei, lvl, ram, r, notes);
    }
    let d = Arc::clone(&w.data);
    let e = w.en(ei);
    e.a.x = ram.s32(o + 0x1A0);
    e.a.y = ram.s32(o + 0x1A4);
    e.a.facing = ram.r8(o + 0x10);
    e.hp1024 = ram.s32(o + 0x28C) as i64;
    // the AI's timers on the frame counter (the World's clock = the logic counter here): state entry +0x210,
    // MarkTime +0x1E8, the counter +0x1E4
    e.state_stamp = ram.s32(o + 0x210) as i64;
    e.mark1 = ram.s32(o + 0x1E8) as i64;
    e.counter = ram.s32(o + 0x1E4) as i64;
    if let Some((sprite, raw, cel, tick)) = row {
        // (the sprite the object shows: the world map's circles pick theirs by the place's state)
        if e.a.sprite != Some(sprite - wf_core::scene::SPRITE_BASE) {
            e.a.set_sprite(&d, Some(sprite - wf_core::scene::SPRITE_BASE));
        }
        e.ea.pos.raw = raw;
        e.ea.pos.frame = cel;
        e.ea.pos.tick = tick;
    }
    wf_core::enemy::sync_mark(w, ei);
    import_patrol(w, ei, ram, o);
}

/// a patrol's runner (enemy +0x22C current step object): the world map's 3-D move (`cutscene_move` step, vt
/// 0x0804A800: +4 s16 target x / y, +8 s16 z / u16 speed, +0xC / +0x10 / +0x14 velocity, +0x18 frames left); the
/// step's index is the record whose target words are the object's; the height obj+0x1A8, heading +0x1AC
fn import_patrol(w: &mut World, ei: usize, ram: &GbaRam, o: u32) {
    let id = w.ents[ei].id;
    let st = ram.r32(o + 0x22C);
    let Some(f) = w.field.as_mut() else { return };
    let Some(p) = f.patrols.iter_mut().find(|p| p.entity == id) else { return };
    p.z = ram.s32(o + 0x1A8);
    p.h = ram.s32(o + 0x1AC);
    if !ram.valid(st) || ram.r32(st) != 0x0804_A800 {
        return;
    }
    let (w0, w1) = (ram.r32(st + 4), ram.r32(st + 8));
    if let Some(k) = p.steps.iter().position(|s| s.func == 0x0803_9F09 && s.words.first() == Some(&w0) && s.words.get(1) == Some(&w1)) {
        p.idx = k as i32;
    }
    p.cur = Some(wf_core::fieldnpc::StepRt::Move3 {
        tx: (w0 & 0xFFFF) as i16 as i32,
        ty: (w0 >> 16) as i16 as i32,
        tz: (w1 & 0xFFFF) as i16 as i32,
        n: ram.s32(st + 0x18),
        vx: ram.s32(st + 0xC),
        vy: ram.s32(st + 0x10),
        vz: ram.s32(st + 0x14),
        h: ram.s32(o + 0x1AC),
    });
}

/// **a cut at any frame with no event running** (mid-area): the area loaded at the leader, the players arrived, the
/// field's live state from RAM (`field_from_ram`); objects the import does not model yet are listed in notes
fn build_mid(data: &Arc<GameData>, ram: &GbaRam, info: &CutInfo, runner: &mut StoryHost, rng0: [u32; 2], frame0: u32, notes: &mut Vec<String>) -> Built {
    let story = import_story(data, ram);
    let (x, y, facing) = info.leader.unwrap_or((120, 120, 0));
    let to = info.area;
    let cfg = FieldConfig { world: to.world, area: to.area, variant: to.variant, pos: Some((x, y)), facing, chars: vec![story.current], story: Some(story.clone()), rng: wf_core::battle::RngInit::State(rng0[0], rng0[1]), xp: true, arrive: false };
    let mut w = field::create(data.clone(), &cfg);
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
        f.mode = Mode::Normal;
    }
    field::attach_host(&mut w, true);
    patch_leader(&mut w, ram, notes);
    w.frame = frame0;
    w.g.fc0 = 0;
    w.g.presses[0].ring = press_ring(ram);
    w.g.presses[0].pressed = 0;
    import_globals(&mut w, ram);
    w.scene.song = song(ram);
    w.scene.hud_hidden = w.story.game_bits & 2 == 0;
    w.scene.banner = None;
    w.scene.banner_last = -1;
    w.rng = Rng::new(rng0[0], rng0[1]);
    w.events.clear();
    let calls0 = w.rng.calls;
    field::load_area(&mut w, to, x, y, Some(facing));
    // (the load's on-enter scripts ran long ago: their effects are in RAM)
    w.events.clear();
    field::finish_arrival(&mut w);
    if let Some(f) = w.field.as_mut() {
        f.arrival = None;
        f.mode = Mode::Normal;
    }
    field_from_ram(&mut w, ram, notes);
    // what the RAM has that the import does not rebuild yet
    let known = [VT_PLAYER, VT_ENEMY, VT_NPC, VT_NPC_TALK, VT_BANNER];
    let other: std::collections::BTreeSet<u32> = ram.objects().into_iter().map(|o| ram.r32(o)).filter(|v| !known.contains(v)).collect();
    if !other.is_empty() {
        notes.push(format!("mid-area: objects of classes {:x?} are not imported", other));
    }
    let _ = runner;
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held: 0, finished: true }
}

/// the 3D world map (an area whose load makes the flyer)
fn is_3d(_data: &GameData, to: AreaId) -> bool {
    (to.world, to.area) == (0, 50)
}

/// one build of an area entry INSIDE an event (a script's warp): the leader before the load, the event held, the
/// load as warp 0x77 makes it (the players go idle, EnterArea re-creates them facing down, the on-enter scripts run
/// nested), then the event continues from its node after the warp (`StoryHost::continue_event`)
fn build_event_entry(data: &Arc<GameData>, ram: &GbaRam, info: &CutInfo, runner: &mut StoryHost, ev: (u16, u16, [i32; 4]), rng0: [u32; 2], frame0: u32, notes: &mut Vec<String>) -> Built {
    let story = import_story(data, ram);
    let (x, y, facing) = info.leader.unwrap_or((120, 120, 0));
    let to = info.area;
    let cfg = FieldConfig { world: to.world, area: to.area, variant: to.variant, pos: Some((x, y)), facing, chars: vec![story.current], story: Some(story.clone()), rng: wf_core::battle::RngInit::State(rng0[0], rng0[1]), xp: true, arrive: false };
    let mut w = field::create(data.clone(), &cfg);
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
        f.mode = Mode::Normal;
    }
    field::attach_host(&mut w, true);
    patch_leader(&mut w, ram, notes);
    w.frame = frame0;
    w.g.fc0 = 0;
    w.g.presses[0].ring = press_ring(ram);
    w.g.presses[0].pressed = 0;
    import_globals(&mut w, ram);
    w.scene.song = song(ram);
    w.scene.hud_hidden = w.story.game_bits & 2 == 0;
    w.scene.banner = None;
    w.scene.banner_last = -1;
    w.rng = Rng::new(rng0[0], rng0[1]);
    w.events.clear();
    let calls0 = w.rng.calls;
    // the event holds the field (a load inside it keeps it held)
    w.scene.depth = 1;
    field::set_busy(&mut w, true);
    let areat = (to.world, to.area, to.variant);
    // which load: the node's own warp / warp_fade into this area, or (none in the node) the one before a `jump`
    let probe = runner.probe_event(&mut w, ev.0, ev.1, ev.2, areat);
    // a return_to_field (0x75) in the node: the field the scene left, restored - a mid-area state (`build_returned`)
    if probe.is_none() {
        if let Some((k, _)) = runner.probe_event_for(&mut w, ev.0, ev.1, ev.2, &|n, _| n == 0x75) {
            return build_returned(data, ram, runner, ev, k, w, calls0, notes);
        }
    }
    let fading = match probe {
        Some((_, 0x76)) => true,
        Some(_) => false,
        None if ev.1 == 0 && !is_3d(data, to) => false,
        None => {
            notes.push(format!("unsupported: event s{:03} node {}: no warp into {:?} in the node (scene load / return_to_field / an earlier scene's warp_fade)", ev.0, ev.1, areat));
            let draws = w.rng.calls.wrapping_sub(calls0);
            return Built { world: w, draws, held: 0, finished: false };
        }
    };
    // a script's fade_out before its warp: the screen stays black through the load (a warp_fade's darkness is the
    // field's own fade)
    if ram.screen_black == Some(true) && !fading {
        w.scene.screen.black = 16;
        w.scene.screen.dim = 256;
    }
    // the event stands at its load action, blocked on it (warp_fade: until the fade-in ends; warp: until the
    // arrival) - positioned before the load: the node's queries read the world before it (the leader exists)
    {
        use wf_script_core::Pending;
        match probe {
            Some((k, 0x76)) => runner.continue_event(&mut w, ev.0, ev.1, ev.2, k, Some(Pending::Transition)),
            Some((k, _)) => runner.continue_event(&mut w, ev.0, ev.1, ev.2, k, Some(Pending::Arrival)),
            None => {}
        }
    }
    let evs: Vec<Event>;
    if fading {
        // warp_fade 0x76: the field's fade-out ends in this frame's post step with the load (no idle draw of its own:
        // the call made it), its on-enter script nested; the script waits for the fade-in (`Pending::Transition`)
        if let Some(f) = w.field.as_mut() {
            f.mode = Mode::FadeOut { k: field::fade_len(4) - 1, to, x, y, facing: Some(0), step: 4 };
            f.nest_fade = true;
        }
        let mut sess = Session { data: data.clone(), world: w, config: Default::default(), prev: [0; wf_core::world::MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() };
        evs = sess.step_post().to_vec();
        w = sess.world;
        w.events.clear();
    } else {
        // warp 0x77 (wf-script-core natives): the players go idle, the script camera ends, EnterArea
        w.scene.cam.mode = wf_core::scene::CamMode::Player;
        w.scene.cam.scroll = None;
        for s in 0..w.players.len() {
            if let Some(i) = w.player_idx(s) {
                wf_core::player::set_idle(&mut w, i);
            }
        }
        if let Some(f) = w.field.as_mut() {
            f.nest_load = true;
        }
        field::load_area(&mut w, to, x, y, Some(0));
        if let Some(f) = w.field.as_mut() {
            f.nest_load = false;
        }
        evs = std::mem::take(&mut w.events);
    }
    // (the event blocked on its load action - placed before the load, below: the load's scripts nest over it)
    match probe {
        Some(_) => runner.nest_over(&mut w, &evs),
        None => {
            runner.run_nested(&mut w, &evs);
            runner.continue_event(&mut w, ev.0, ev.1, ev.2, 0, None);
        }
    }
    let skipped = probe.map(|p| p.0).unwrap_or(0);
    // a nested on-enter script that blocks holds the load: wf-core plays the held frames (no input) until the leader
    // arrives (as an exit arrival's, `build_area_entry`)
    let mut held = 0;
    let arriving = |w: &World| w.field.as_ref().is_some_and(|f| f.arrival.is_some());
    if arriving(&w) {
        let mut sess = Session { data: data.clone(), world: w, config: Default::default(), prev: [0; wf_core::world::MAX_PLAYERS], outcome: wf_core::battle::Outcome::Running, hook: None, presenter: Default::default() };
        while arriving(&sess.world) && held < 600 {
            held += 1;
            runner.step(&mut sess, 0);
        }
        w = sess.world;
    }
    // the load frame's end: the cameras (a modal scroll the continuation started takes its step; the script sees it
    // done in the next frame - the game's PC is still in camera_scroll_to at the cut: opening 1-10, s068)
    field::world_mode_cameras(&mut w);
    notes.retain(|n| !n.starts_with("event "));
    notes.push(format!("event s{:03} continues at node {} ({} actions before the cut dropped)", ev.0, ev.1, skipped));
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held, finished: true }
}

/// Import a real-game state (work RAM) as a wf-core World ready to continue from the next logic frame.
///
/// Supported: area-entry cuts ([`CutKind::AreaEntry`]); other points return `ImportError::Unsupported`. `runner` runs
/// the load's scripts and plays the held frames of a blocking on-enter script; keep using the same host for the
/// replay (its running script / queue are part of the state).
pub fn import_emulator_state(data: &Arc<GameData>, ram: &GbaRam, runner: &mut StoryHost) -> Result<Imported, ImportError> {
    let info = inspect(ram);
    // an area entry inside a running event (a script's warp): the event's position from the scene objects
    let in_event = (info.kind == CutKind::MidArea && info.depth > 0 && info.leader.is_some()).then(|| running_event(ram, runner)).flatten();
    match info.kind {
        CutKind::AreaEntry => {}
        CutKind::MidArea if in_event.is_some() => {}
        // (any frame with nothing running: the mid-area import)
        CutKind::MidArea if info.depth == 0 && info.leader.is_some() => {}
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
        let b = match in_event {
            Some(ev) => build_event_entry(data, ram, &info, runner, ev, guess, frame0, &mut n),
            None if info.kind == CutKind::MidArea => build_mid(data, ram, &info, runner, guess, frame0, &mut n),
            None => build_area_entry(data, ram, &info, runner, guess, frame0, 600, &mut n),
        };
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
    if let Some(n) = notes.iter().find(|n| n.starts_with("unsupported: ")) {
        return Err(ImportError::Unsupported(n.trim_start_matches("unsupported: ").to_string()));
    }
    if b.world.rng.state() != target {
        return Err(ImportError::Mismatch(format!("RNG {:08x?} after {} draws vs RAM {:08x?}", b.world.rng.state(), b.draws, target)));
    }
    let mut world = b.world;
    if !b.finished {
        notes.push("the load's scripts are still running at the cut (host state carries over)".into());
    }
    check(&mut world, ram, &mut notes);
    import_vram(&mut world, ram);
    // a banner object of an earlier load still showing: its countdown (obj +0x10) -> our age
    {
        world.scene.banner_made = u32::MAX;
        if let Some(o) = ram.objects().into_iter().find(|&o| ram.r32(o) == VT_BANNER) {
            let rem = ram.r16(o + 0x10) as u32;
            let text = data.field.as_ref().and_then(|fd| fd.area(info.area.world, info.area.area)).map(|a| a.banner.clone()).unwrap_or_default();
            world.scene.banner = Some((text, 239u32.saturating_sub(rem)));
        }
    }
    // an event run by the countdown's hook (#9 from the timer update 0x0800425C: its return address 0x08004289 on
    // the stack)
    runner.hs.timer_event = (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0800_4289);
    // the script countdown (GD+0x4D4, running = GD+1 bit 2): the host's timer (12-x: the Airship's self destruct)
    {
        let gd = ram.read(GD, 0x4D8);
        runner.hs.timers.countdown = u32le(&gd, 0x4D4) as i32;
        runner.hs.timers.countdown_on = gd[1] & 4 != 0;
    }
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


/// the object tile allocator (`0x03001C4C`, vram.rs) as the RAM holds it; the actors' blocks (obj +0xC) belong to
/// their entities (matched by position like `check`), the shadows' to their shared graphics (`0x03002B88` + 4k:
/// reference count, handle +2); other blocks stay foreign
fn import_vram(w: &mut World, ram: &GbaRam) {
    use wf_core::vram::{ObjVram, Owner, VramState, ENTRIES, UNITS};
    const A: u32 = 0x0300_1C4C;
    let mut a = ObjVram::fresh();
    a.free_head = ram.r8(A);
    a.spare_head = ram.r8(A + 1);
    for i in 0..ENTRIES {
        let b = A + 0x694 + 10 * i as u32;
        a.e[i] = wf_core::vram::Block { used: ram.r8(b), start: ram.r16(b + 2), size: ram.r16(b + 4), hi: ram.r8(b + 6), lo: ram.r8(b + 7), fnext: ram.r8(b + 8), fprev: ram.r8(b + 9) };
    }
    for u in 0..UNITS as u32 {
        a.idx[u as usize] = ram.r8(A + 0xC00 + u);
    }
    let mut owners: Vec<(u16, Owner)> = vec![];
    let mut used_ents: Vec<wf_core::world::EntityId> = vec![];
    let mut signs = 0u32;
    for o in ram.objects() {
        let vt = ram.r32(o);
        if vt == 0x0804_D1E4 {
            let h = ram.r16(o + 0x16);
            if h != 0xFFFF {
                owners.push((h, Owner::Props));
            }
            continue;
        }
        if vt == VT_HUD || vt == VT_BANNER || vt == 0x0804_C070 || vt == 0x0804_D878 {
            let h = ram.r16(o + 0xC);
            if h != 0xFFFF {
                signs += (vt == 0x0804_D878) as u32;
                owners.push((h, if vt == VT_HUD { Owner::Hud } else if vt == VT_BANNER { Owner::Banner } else if vt == 0x0804_D878 { Owner::Sign(signs - 1) } else { Owner::StatMark }));
            }
            continue;
        }
        if !matches!(vt, VT_PLAYER | VT_ENEMY | VT_NPC | VT_NPC_TALK | 0x0804_E1B4 | 0x0804_AC20) {
            continue;
        }
        let h = ram.r16(o + 0xC);
        if h == 0xFFFF {
            continue;
        }
        let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && w.actor(k).is_some_and(|ac| ac.x == x && ac.y == y));
        if let Some(k) = ent {
            used_ents.push(w.ents[k].id);
            owners.push((h, Owner::Ent(w.ents[k].id)));
        }
    }
    let mut shared_rc = [0u8; 5];
    for (k, rc) in shared_rc.iter_mut().enumerate() {
        // the shadows' records, then the halo's (`0x03001BA8`)
        let p = if k == wf_core::vram::HALO { 0x0300_1BA8 } else { 0x0300_2B88 + 4 * k as u32 };
        *rc = ram.r8(p);
        if *rc > 0 {
            owners.push((ram.r16(p + 2), Owner::Shared(k as u8)));
        }
    }
    for b in &a.e {
        if b.used == 1 && !owners.iter().any(|o| o.0 == b.start << 1) {
            owners.push((b.start << 1, Owner::Foreign));
        }
    }
    // the purgeable non-actor objects (made by the load after the players: at the list's end)
    let stamp = w.next_id;
    let objs: Vec<(Owner, u32, i32)> = owners
        .iter()
        .filter(|o| matches!(o.1, Owner::Hud | Owner::Banner | Owner::BossBar | Owner::StatMark | Owner::Sign(_) | Owner::Map3d | Owner::Timer))
        .map(|o| (o.1, stamp, a.e[a.idx[(o.0 >> 1) as usize & 511] as usize].size as i32 * 64))
        .collect();
    if let Some(f) = w.field.as_mut() {
        f.vram = Some(VramState { a, owners, shared_rc, suspended: vec![], hud_out: vec![], popups: vec![], keyed: vec![], stashed: None, delayed: vec![], objs, susp: vec![], shadow_set: vec![], popup_at: vec![], pass_at: 0 });
    }
}
