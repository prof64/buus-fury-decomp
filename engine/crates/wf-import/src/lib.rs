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
pub mod area;
pub mod recs;
pub mod world;
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

/// A fusion component's level-up hook (#3) in flight: the game made the component current (GameData +0) while the
/// player object still shows the fusion (`Story::levelup_char`; Gotenks 6-9 tile +6,000: current Trunks (3), the
/// player Gotenks' sprite). The player's sprite def (+0x58) names the fusion: it is current, the component is
/// `levelup_char`. Called before the records are read (the fusion's own record follows).
fn levelup_fusion(data: &GameData, ram: &GbaRam, st: &mut Story) {
    let p = ram.player();
    if !ram.valid(p) {
        return;
    }
    let def = ram.r32(p + 0x58);
    let comp = st.current;
    let shown = |c: usize| -> bool {
        let form = ram.r8(REC0 + 0x68 * c as u32 + 0x14) as i32;
        data.form(form).and_then(|f| data.sprite(f.gfx - 9)).is_some_and(|s| s.def_addr == def)
    };
    if comp < 0 || shown(comp as usize) {
        return;
    }
    for f in 0..9usize {
        let fus = ram.r16(REC0 + 0x68 * f as u32 + 2);
        // (+0x02: bits 0-3 char A, 4-7 char B, 8-15 the fusion's level byte - `Story::fusion`; the old byte split
        // matched Trunks only through the level byte: run 2 cut 1,374,599, Goten's hook ran unrecognised)
        let parts = [(fus & 0xF) as i32, ((fus >> 4) & 0xF) as i32];
        if fus != 0 && parts.contains(&comp) && shown(f) {
            st.levelup_char = Some(comp);
            st.current = f as i32;
            // the first component's hook: the second's XP comes after it (`fusion_xp_left`) - the XP routine
            // 0x08013914 is suspended in its first recursive call (return 0x08013963 on the stack; that call's push
            // {r3-r7, lr} saved the caller's r5 = the XP 12 bytes below the return: run 2 cut 1,374,598, Goten's
            // hook, Trunks' 52,900 XP and its level rolls next)
            if comp == parts[0] {
                if let Some(a) = (0x0300_7000u32..0x0300_8000).step_by(4).find(|&a| ram.r32(a) == 0x0801_3963) {
                    st.fusion_xp_left = Some((parts[1], ram.r32(a - 0xC) as i64));
                }
            }
            return;
        }
    }
}

/// the persistent state: GameData `0x03001068` (character, game bits, supplies, flags, equipment, zenie, play time),
/// the 9 character records `0x03001560`, script words L / G, `get_variant_byte`
pub fn import_story(data: &GameData, ram: &GbaRam) -> Story {
    let gd = ram.read(GD, 0x8A8);
    let mut st = Story::new_game(data);
    st.current = gd[0] as i32;
    levelup_fusion(data, ram, &mut st);
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
            fusion_time: u32le(&r, 8) as i32,
            ss_gauge: u16le(&r, 0x1E) as i32,
            ss3_gauge: u16le(&r, 0x20) as i32,
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
    // the HUD object's (vt 0x08049FEC) last low-HP beep: +0x10, the counter 0x03000F94 when it sounded (update
    // 0x0800BA4A: again once more than 60 later) - run 2 row 140,866 (0-91, Gohan low on HP: the app beeped from its
    // own phase, 5 iterations late)
    if let Some(h) = ram.objects().into_iter().find(|&o| ram.r32(o) == 0x0804_9FEC) {
        if let Some(f) = w.field.as_mut() {
            f.hud_beep = ram.r32(h + 0x10);
        }
    }
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
    let cfg = FieldConfig { world: to.world, area: to.area, variant: to.variant, pos: Some((x, y)), facing, chars: vec![story.current], story: Some(story.clone()), rng: wf_core::battle::RngInit::State(rng0[0], rng0[1]), xp: true, arrive: false, slow_fade_in: false };
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
    let mut fresh = None;
    while a < 0x0300_8000 - 0x18 {
        let h = ram.r32(a);
        let pp = ram.r32(a + 0x14);
        if let Some(ev) = runner.event_of_header(h).filter(|_| matches!(pp >> 24, 2 | 3) && ram.valid(pp)) {
            let next = ram.r16(a + 8) as u16;
            if let Some(node) = runner.node_before(ev, next) {
                let params = [0, 4, 8, 12].map(|o| ram.s32(pp + o));
                return Some((ev, node, params));
            }
            // (a state word that names no node - a `steps` node's: the node whose code holds a live interpreter's PC)
            if let Some(node) = node_of_pc(ram, runner, ev) {
                let params = [0, 4, 8, 12].map(|o| ram.s32(pp + o));
                return Some((ev, node, params));
            }
            // (a scene object that has not stepped - next state 0 - under the modal scene mode: the event a trigger
            // / talk started in the cut's frame, its first node still to run - when no event runs it (a nested
            // run_event's not stepped yet is its outer's: whole-run cuts 612,541 / 620,017); run 2 cut 49,356, s100)
            if next == 0 && fresh_scene(ram, a) && fresh.is_none() {
                fresh = Some((ev, 0, [0, 4, 8, 12].map(|o| ram.s32(pp + o))));
            }
        }
        a += 4;
    }
    fresh
}

/// the main mode is the modal scene (vt 0x08049D18) of the scene object at `obj` (its +4) and that object's next
/// state is 0: the event started in this frame, its first node has not run
fn fresh_scene(ram: &GbaRam, obj: u32) -> bool {
    ram.mode_vt() == MODE_SCENE && ram.r32(ram.r32(MODE) + 4) == obj && ram.r16(obj + 8) == 0
}

/// the event running `ev` with `run_event` (the next event object up the stack, its interpreter standing just
/// after a `run_event(ev, ..)`): (event, node, params, its actions before that `run_event`)
fn outer_event(ram: &GbaRam, runner: &StoryHost, ev: (u16, u16, [i32; 4])) -> Option<((u16, u16, [i32; 4]), usize)> {
    let mut a = 0x0300_7000u32;
    let mut seen_inner = false;
    while a < 0x0300_8000 - 0x18 {
        let h = ram.r32(a);
        let pp = ram.r32(a + 0x14);
        if let Some(e) = runner.event_of_header(h).filter(|_| matches!(pp >> 24, 2 | 3) && ram.valid(pp)) {
            if !seen_inner {
                seen_inner = e == ev.0;
            } else if e != ev.0 {
                let params = [0, 4, 8, 12].map(|o| ram.s32(pp + o));
                let node = runner.node_before(e, ram.r16(a + 8) as u16).or_else(|| node_of_pc(ram, runner, e))?;
                let ran = natives_run(ram, runner, (e, node, params))?;
                let (n, args) = ran.last()?;
                let runs = (wf_script::natives::RUN_EVENT..=wf_script::natives::RUN_EVENT_P0_P3).contains(n);
                return (runs && args.as_ref().is_none_or(|x| x.first() == Some(&(ev.0 as i32)))).then(|| ((e, node, params), ran.len() - 1));
            }
        }
        a += 4;
    }
    None
}

/// event `ev`'s node whose code holds the PC of an interpreter on the context stack (`natives_run`'s search)
fn node_of_pc(ram: &GbaRam, runner: &StoryHost, ev: u16) -> Option<u16> {
    let nodes: Vec<u16> = runner.step_code.keys().filter(|k| k.0 == ev).map(|k| k.1).collect();
    nodes.into_iter().find(|&n| natives_run(ram, runner, (ev, n, [0; 4])).is_some())
}

/// a script running at a cut whose scene depth reads 0 (a `run_script` scene, real route Kami's Lookout s512): only
/// when an interpreter's PC is inside one of its nodes
fn running_event_live(ram: &GbaRam, runner: &StoryHost) -> Option<(u16, u16, [i32; 4])> {
    let ev = running_event(ram, runner)?;
    natives_run(ram, runner, ev).is_some().then_some(ev)
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
    // (the field the scene left was entered before it: its name is the last banner shown - the return shows none;
    // Kid Buu chapter cuts rkb_003 .. 012 at 19-3, the game's screen has no banner)
    if let Some(name) = _data.field.as_ref().and_then(|fd| fd.area(to.world, to.area)).map(|a| a.name_id) {
        w.scene.banner_last = name;
    }
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

/// (a HandAmbush runtime past its first update, its count set from the animation later)
const HAND_STARTED: i32 = -1000;

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
        // a condition (base vt 0x0804BCA8, update 0x080342AE): +4 its child's runtime, +8 the child entered (`on`;
        // whole-run cuts 808,709 / 808,776 / 809,325: a MeleeAttack under IfPlayerInMeleeRange entered again in ours
        // - a second hit)
        if wf_core::nodes::is_cond(node.k) {
            rt.on = ram.r32(r + 8) != 0;
            if let Some(&c) = node.kids.first() {
                todo.push((c as usize, ram.r32(r + 4)));
            }
        }
        match node.k {
            K::Sequence => {
                rt.i = ram.s32(r + 4);
                for (k, &c) in node.kids.iter().enumerate() {
                    todo.push((c as usize, ram.r32(r + 0xC + 4 * k as u32)));
                }
            }
            // ParallelAll / ParallelAny (vt 0x0804BDA8: +4 children, +8.. their runtimes) as the Selector
            K::Selector | K::ParallelAll | K::ParallelAny => {
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
            // KnockbackSlide (0x0804A138): +4 / +8 velocity, +0xC the frames parameter (16, constant), +0x10 the frames
            // left of a blocked slide (counts down from 16 - run 2 cut 1,928,000, 19-3: Kid Buu blocking, 9 left, he
            // stops 8 moves later; ours took the 16); its animation is the one showing
            K::KnockbackSlide => {
                rt.d[0] = ram.s32(r + 4);
                rt.d[1] = ram.s32(r + 8);
                rt.d[2] = ram.s32(r + 0x10);
                rt.d[3] = -1;
            }
            // HoverDrift (vt 0x0804CB80): +4 / +8 the drift velocity (ours: the enemy's vx / vy)
            K::HoverDrift => {
                let (vx, vy) = (ram.s32(r + 4), ram.s32(r + 8));
                let e = w.en(ei);
                e.vx = vx;
                e.vy = vy;
            }
            // GlideToWaypoint (vt 0x0804B678): +4 / +8 the velocity, +0xC the moves left of its 60 (ours counts up)
            K::GlideToWaypoint => {
                rt.d[0] = ram.s32(r + 4);
                rt.d[1] = ram.s32(r + 8);
                rt.f = 60 - ram.s32(r + 0xC);
            }
            // Wait (vt 0x080497D4, update 0x08018BF0): +4 the frames left - set to the node's count at the enter,
            // one off each update, finished at 0 (ours counts the updates up: frames - left; real run 20-6 tile
            // +7,000, Vegeta's Wait(60) with 27 left)
            K::Wait => rt.f = (node.p.g(wf_core::ai::P::Frames) as i32 - ram.s32(r + 4)).max(0),
            // HeavyPunch (a static runtime 0x030032F8; the controller 0x030032F4 counts in the object's tick): set
            // from the animation after the rows (`patch_enemy`)
            K::HeavyPunch => {}
            // FlameBreath (the controller 0x03002FA8 counts in the object's tick, as HeavyPunch): `patch_enemy`
            K::FlameBreath => {}
            // NinjaStarThrow (the controller 0x03003074 counts in the object's tick): `patch_enemy`
            K::NinjaStarThrow => {}
            // EnergyBarrage (Pikkon's controller 0x030030E0 as the object's state: obj+0x12 the tick): `patch_enemy`
            K::EnergyBarrage => {}
            // BackStep (enter 0x08031388, update 0x080313EA): +4 / +8 the hop step, +0x10 the counter (48 at the enter,
            // one off each update, finished at 0; ours counts the updates up)
            K::BackStep => {
                rt.d[0] = ram.s32(r + 4);
                rt.d[1] = ram.s32(r + 8);
                rt.f = 48 - ram.s32(r + 0x10);
            }
            // SpawnEnzymes (vt 0x0804AD18): +4 the frames to the next spawn (one off each update, a spawn at 0, then 120 -
            // ours: the enemy's `enz_timer`); the spawn count is the enemy's +0x204 (`patch_enemy`) (exit-fade sweep
            // 21-6 / 21-8: both Blisters 4 frames from a spawn at the cut, ours 120)
            K::SpawnEnzymes => w.en(ei).enz_timer = ram.s32(r + 4),
            // HandAmbush (vt 0x0804C174): +4 set by its first update (the side draw and the jump to the player done):
            // the frame count is then the strike animation's (`patch_enemy`, after the rows)
            K::HandAmbush if ram.s32(r + 4) != 0 => rt.f = HAND_STARTED,
            K::PopState | K::PushState => {}
            K::Chance | K::GotoState | K::SubState | K::IfPlayerWithin | K::IfPlayerFarther | K::IfPlayerInMeleeRange | K::IfTimeInState | K::IfCooldownOver | K::IfLastKidBuu => {}
            // ThrowBigBall: a singleton runtime (0x030032E8); its frame count comes from the ball (`import_super_ball`)
            K::ThrowBigBall | K::StandStill | K::FacePlayer => {}
            // MechTransform: no runtime words - its update waits for the imported animation (raw 20) to end
            K::MechTransform => {}
            // MeleeAttack (update 0x0802A276 = IsAnim(0)), IncCounter: no runtime words
            K::MeleeAttack | K::IncCounter => {}
            // GhostAnimA / B: no runtime words (the object's state decides - `patch_enemy`)
            K::GhostAnimA | K::GhostAnimB => {}
            // the worm's ring / links / mailbox (`import_worms`; WormSpawnBody keeps nothing) - and the contact's counter
            // (`enemy::contact_hit_foot`): the head's at +4, a body piece's at +0x14 (run 2 row 1,769,316, 21-9)
            K::WormHeadMove | K::WormBodyFollow => {
                w.en(ei).contact_cd = ram.s32(r + if node.k == K::WormHeadMove { 4 } else { 0x14 }).max(0);
            }
            K::WormSpawnBody => {}
            // IfButtonMashed (factory 0x080363FA, vt 0x0804A6D0): +0xC the logic counter at the last counted press (now -
            // gap - 1 at the create), +0x10 the presses counted, +0x14 / +0x18 the parameters - run 2 row 1,640,163,
            // 21-x: an enzyme latched on the leader lets go at the 9th press; ours counted from 0 at the import and held
            // on, the punch then missed it (two ComputeDamage in the game, one in ours: the punch's sound pair swapped)
            K::IfButtonMashed => {
                rt.d[3] = ram.s32(r + 0x10);
                rt.d[4] = ram.s32(r + 0xC);
            }
            // DriftContact (create 0x0802C69C, update 0x0802C6EC; vt 0x0804B640): +4 / +8 the drift step (1/256 px),
            // +0xC done, +0x10 the frames left of a contact's back-off (16) - run 2 cuts 1,633,618 / 1,692,733, 21-x
            // Blood Cells: ours drew a fresh step at the import
            K::DriftContact => {
                rt.d[0] = ram.s32(r + 4);
                rt.d[1] = ram.s32(r + 8);
                rt.d[2] = ram.s32(r + 0x10);
            }
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
            // (equal to the map's: the live grid has no marks - also none of the load's; tour_kami_htc: the talk
            // NPC's footprint marked by our load stood under the player, the game's grid has it free)
            c.walk = if wk != c.map.walk { Some(wk) } else { None };
        }
    }
}

/// the field's live state from RAM (mid-area imports: a field restored after a scene, or any frame): the actors by
/// match and patch, the leader's 24.8 position / animation / script pose, the grids, the stagger, the 3-D map's
/// flyer and camera, the 2-D camera
/// the pause pages' main-loop mode vtables: Status, Inventory, the Inventory's Use dialog
const MODE_STATUS: u32 = 0x0804_AC88;
const MODE_INVENTORY: u32 = 0x0804_9D48;
const MODE_USE: u32 = 0x0804_ACB4;
/// the Equip page (slots, +0x114 the slot), a slot's equipment list (+0x114 the slot, +0x138 top, +0x13C selection)
/// and the list's action dialog (nested; +0x10C its selection) - run 3 cuts 46,263 / 46,660
const MODE_EQUIP: u32 = 0x0804_9588;
const MODE_EQUIP_LIST: u32 = 0x0804_A8F4;
const MODE_EQUIP_ACT: u32 = 0x0804_AFCC;
/// the mode switch after the pause menu's fade-out (vt 0x080493C8, +4 0x0804956C; whole run 968,262): the world is
/// saved, this iteration builds and draws the Status page
const MODE_SWITCH: u32 = 0x0804_93C8;

/// the pause page running at the cut: (its mode vtable, its object)
fn pause_page(ram: &GbaRam) -> Option<(u32, u32)> {
    let m = ram.r32(MODE);
    let vt = ram.mode_vt();
    let switch = vt == MODE_SWITCH && ram.r32(m + 4) == 0x0804_956C;
    ([MODE_STATUS, MODE_INVENTORY, MODE_USE, MODE_EQUIP, MODE_EQUIP_LIST, MODE_EQUIP_ACT].contains(&vt) || switch).then_some((vt, m))
}

/// the page the cut is in (`pause_page`), opened over the imported field: Status (`+0x130` allocating, `+0x134`
/// the values when it started, `+0x144` the values being allocated, `+0x154` the blink counter), Inventory (`+0x11C`
/// tab, `+0x120` scroll top, `+0x124` selection) with its Use dialog when that mode runs (the Inventory then in
/// the mode stack's saved modes, `0x03000F9C`..)
fn open_pause_page(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let Some((vt, m)) = pause_page(ram) else { return };
    if w.data.menu.is_none() {
        return;
    }
    if vt == MODE_SWITCH {
        // the fade-out's last step to come (ours: the step that reaches black builds and draws the page)
        let page = wf_core::menu::status::Status::new(w);
        wf_core::menu::open(w, wf_core::menu::Screen::Status(page), wf_core::menu::PAUSE, false);
        w.menu.phase = wf_core::menu::Phase::FadeOut { level: 240 };
        w.menu.held = ram.r16(0x0300_3300 + 2);
        // (the world save freed the field's sprite blocks already)
        wf_core::vram::world_hide(w);
        notes.push("the pause menu's mode switch (the Status page next): the field from the saved world".into());
        return;
    }
    let screen = if vt == MODE_STATUS {
        let mut s = wf_core::menu::status::Status::new(w);
        s.alloc = ram.r32(m + 0x130) != 0;
        s.blink = ram.r32(m + 0x154);
        for k in 0..4 {
            s.orig[k] = ram.s32(m + 0x134 + 4 * k as u32);
            s.vals[k] = ram.s32(m + 0x144 + 4 * k as u32);
        }
        // the party tabs (+0x12C, vt 0x08049C68): +0x34 first shown, +0x38 end, +0x3C current, +0x40 marked, +0x48 x
        let tb = ram.r32(m + 0x12C);
        if ram.valid(tb) {
            if let Some(wf_core::menu::kit::Widget::Tabs(t)) = s.c.kids.get_mut(s.tabs) {
                t.first = ram.s32(tb + 0x34);
                t.end = ram.s32(tb + 0x38);
                t.cur = ram.s32(tb + 0x3C);
                t.mark = ram.s32(tb + 0x40);
                t.x0 = ram.s32(tb + 0x48);
            }
        }
        wf_core::menu::Screen::Status(s)
    } else if [MODE_EQUIP, MODE_EQUIP_LIST, MODE_EQUIP_ACT].contains(&vt) {
        let list = if vt == MODE_EQUIP {
            None
        } else {
            let depth = ram.r32(MODE + 8).min(8);
            (0..depth).map(|k| ram.r32(MODE + 0xC + 4 * k)).chain([m]).find(|&o| ram.valid(o) && ram.r32(o) == MODE_EQUIP_LIST)
        };
        let e = match list {
            Some(l) => {
                let dialog = (vt == MODE_EQUIP_ACT).then(|| ram.s32(m + 0x10C));
                wf_core::menu::equip::Equip::imported(w, ram.s32(l + 0x114), 0, Some((ram.s32(l + 0x138), ram.s32(l + 0x13C), dialog)))
            }
            None => wf_core::menu::equip::Equip::imported(w, ram.s32(m + 0x114), ram.s32(m + 0x10C), None),
        };
        wf_core::menu::Screen::Equip(e)
    } else {
        let inv = if vt == MODE_INVENTORY {
            Some(m)
        } else {
            let depth = ram.r32(MODE + 8).min(8);
            (0..depth).map(|k| ram.r32(MODE + 0xC + 4 * k)).find(|&o| ram.valid(o) && ram.r32(o) == MODE_INVENTORY)
        };
        let Some(inv) = inv else { return };
        let p = wf_core::menu::inventory::Inventory::imported(w, ram.r32(inv + 0x11C) as u8, ram.s32(inv + 0x120), ram.s32(inv + 0x124), vt == MODE_USE);
        wf_core::menu::Screen::Inventory(p)
    };
    wf_core::menu::open_imported(w, screen);
    w.menu.held = ram.r16(0x0300_3300 + 2);
    notes.push(format!("a pause page (mode {vt:#x}): the field from the saved world"));
}

fn field_from_ram(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    import_objects(w, ram, notes);
    import_spawned_triggers(w, ram, notes);
    import_trigger_resume(w, ram);
    // (shots an enemy's node had made - a beam in flight - are newer than every actor: the list's end)
    let (shots, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut w.ents).into_iter().partition(|e| matches!(e.body, Body::Shot(_)));
    w.ents = rest;
    w.ents.extend(shots);
    import_effects(w, ram);
    let rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    patch_leader(w, ram, notes);
    // the walk and enemy grids as they stand (map object +0xA8 / +0xB0: bitmaps, 32 bytes a row, bit = free)
    import_grids(w, ram);
    // (building the enemies entered their states: the path stagger, the glide's waypoint index ... as the RAM has
    // them - real run 20-6 tile +7,000: the imported glide's enter had stepped the index)
    import_globals(w, ram);
    // the BG sphere effect (`show_energy_sphere` 0xF4; the effect slot 0x03001990 -> object vt 0x080493B4: +0xA the
    // script's 4th argument, +0xB radius, +0x10 / +0x14 centre; drawn by 0x0803AA58 with radius - 3 jitter draws)
    w.scene.visuals.retain(|v| v.kind != wf_core::scene::VisualKind::EnergySphere);
    let sp = ram.r32(0x0300_1990);
    if ram.valid(sp) && ram.r32(sp) == 0x0804_93B4 {
        let v = wf_core::scene::Visual { kind: wf_core::scene::VisualKind::EnergySphere, x: ram.s32(sp + 0x10), y: ram.s32(sp + 0x14), owner: ram.r8(sp + 0xB) as u32, t: ram.r8(sp + 0xA) as i32, life: -1, vid: 0 };
        w.scene.visuals.push(v);
    }
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
    import_death_effects(w, ram, notes);
    import_breakables(w, ram, notes);
    import_player_blasts(w, ram);
    import_rocks(w, ram);
    import_coins(w, ram);
    import_emotes(w, ram);
    // the camera as the field left it (`return_to_field` puts the saved one back)
    w.scene.cam.mode = wf_core::scene::CamMode::Player;
    wf_core::battle::set_camera(w, 0, ram.s32(CAM), ram.s32(CAM + 4));
    // the camera's controller: the mode's camera slot 0x03002BE0 - vt 0x0804D300 follows the actor at +4 (the
    // leader: the player camera, easing toward it), vt 0x0804BE08 holds it still (a script's camera). Another
    // object there (a screen shake) hides it: then a camera not where following the leader keeps it was held by a
    // script before the scene (fixed); with no event running nothing holds it
    let slot = ram.r32(0x0300_2BE0);
    // a screen shake running there (vt 0x0804A720: +4 intensity, +5 period, +6 count, +8 frames left, +0xC / +0x10
    // the offset): it draws two RNG values each period (r033_002_6-1 +7,000: every 2 frames, 0x080079F2). Not under
    // an event: the mid-code import takes the running shake with its command (r033_002 +15,000 / +19,000)
    if w.scene.depth == 0 {
        import_field_shake(w, ram);
    }
    match ram.valid(slot).then(|| ram.r32(slot)) {
        Some(0x0804_D300) => {
            let a = ram.r32(slot + 4);
            if a == ram.player() {
                w.scene.cam.mode = wf_core::scene::CamMode::Player;
            } else {
                let (x, y) = (ram.s32(a + 0x1A0), ram.s32(a + 0x1A4));
                if let Some(k) = (0..w.ents.len()).find(|&k| !w.ents[k].removed && w.actor(k).is_some_and(|ac| ac.x == x && ac.y == y)) {
                    w.scene.cam.mode = wf_core::scene::CamMode::Follow(w.ents[k].id);
                } else {
                    notes.push(format!("camera follows RAM actor {a:#x}: no entity there"));
                }
            }
        }
        Some(0x0804_BE08) => w.scene.cam.mode = wf_core::scene::CamMode::Fixed,
        _ if w.scene.depth > 0 => {
            let mut probe = w.clone();
            field::world_mode_cameras(&mut probe);
            if (probe.cameras[0].x, probe.cameras[0].y) != (w.cameras[0].x, w.cameras[0].y) {
                w.scene.cam.mode = wf_core::scene::CamMode::Fixed;
            }
        }
        _ => {}
    }
}

/// a screen shake running in the mode's camera slot (vt 0x0804A720: +4 intensity, +5 period, +6 count, +8 frames
/// left, +0xC / +0x10 the offset) - the field's, not a script command's
fn import_field_shake(w: &mut World, ram: &GbaRam) {
    let slot = ram.r32(0x0300_2BE0);
    if ram.valid(slot) && ram.r32(slot) == 0x0804_A720 && ram.s32(slot + 8) > 0 {
        w.g.shakes = vec![wf_core::world::Shake { intensity: ram.r8(slot + 4) as i32, period: ram.r8(slot + 5) as i32, cnt: ram.r8(slot + 6) as i32, left: ram.s32(slot + 8) }];
        w.g.shake_off = (ram.s32(slot + 0xC), ram.s32(slot + 0x10));
    }
}

/// the field's actors from RAM, by **match and patch** (mid-area imports): every RAM actor (enemy / NPC object) is
/// matched to an entity the area's own load made - same enemy type / NPC sprite, the nearest unclaimed one - and
/// that entity is patched to the RAM's state (it keeps what the load attached: patrol runners, placements, talk
/// events); RAM actors without a match are built; entities no RAM actor claims go. The actors end in the RAM's
/// world-list order (the update order).
/// event triggers a script added (`spawn_trigger_force` 0x8A: an area trigger the load did not make, its condition
/// false) that stand in the RAM's trigger list (`TRIGS`, event trigger vt 0x08049BD4: +4 rect): ours made again by
/// rect (run 2 cut 49,356: s099's spawn_trigger_force(4) at 1-17, s100's trigger)
fn import_spawned_triggers(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    use wf_core::fieldcontent::{ObjKind, TriggerKind};
    let n = ram.r32(TRIGS).min(256);
    let Some(area) = w.field.as_ref().map(|f| f.area) else { return };
    let defs = w.data.field.as_ref().and_then(|fd| fd.area(area.world, area.area)).map(|a| a.objects.clone()).unwrap_or_default();
    for k in 0..n {
        let o = ram.r32(TRIGS + 4 + 4 * k);
        if !ram.valid(o) || ram.r32(o) != 0x0804_9BD4 {
            continue;
        }
        let r = (ram.s16(o + 4), ram.s16(o + 6), ram.s16(o + 8), ram.s16(o + 0xA));
        if w.field.as_ref().is_some_and(|f| f.triggers.iter().any(|t| (t.rect.x0, t.rect.y0, t.rect.x1, t.rect.y1) == r)) {
            continue;
        }
        let def = defs.iter().find(|d| matches!(&d.kind, ObjKind::Trigger { rect, kind: TriggerKind::Event(_), .. } if (rect.x0, rect.y0, rect.x1, rect.y1) == r));
        if let Some(d) = def {
            field::spawn_map_object(w, d.at, true);
            notes.push(format!("mid-area: a script-spawned event trigger {:?} {r:?}", d.at));
        }
    }
}

/// the event triggers' resume nodes (trigger list `TRIGS`, event trigger vt 0x08049BD4: +4 rect s16 x0 y0 x1 y1,
/// +0x10 the exit value of the event's last run), matched to the load's triggers by rect
fn import_trigger_resume(w: &mut World, ram: &GbaRam) {
    let n = ram.r32(TRIGS).min(256);
    // the load's triggers the RAM's list no longer has (a touch-once trigger fired and removed itself - real run
    // 007 Pilaf's room, whole-run cut 756,163: the room's world-spanning touch-once s364 had run before the cut):
    // dropped, as many as ours exceed the RAM's (an Instant Transmission's wrapped trigger spans the world in RAM:
    // the counts match then and nothing goes)
    let rects: Vec<(i32, i32, i32, i32)> = (0..n).map(|k| ram.r32(TRIGS + 4 + 4 * k)).filter(|&o| ram.valid(o)).map(|o| (ram.s16(o + 4), ram.s16(o + 6), ram.s16(o + 8), ram.s16(o + 0xA))).collect();
    if let Some(f) = w.field.as_mut().filter(|f| f.triggers.len() > rects.len() && rects.len() == n as usize) {
        let mut excess = f.triggers.len() - rects.len();
        let mut left = rects.clone();
        let mut keep = vec![true; f.triggers.len()];
        for (k, t) in f.triggers.iter().enumerate() {
            let r = (t.rect.x0, t.rect.y0, t.rect.x1, t.rect.y1);
            match left.iter().position(|&x| x == r) {
                Some(p) => {
                    left.remove(p);
                }
                None if excess > 0 => {
                    keep[k] = false;
                    excess -= 1;
                }
                None => {}
            }
        }
        let mut it = keep.into_iter();
        f.triggers.retain(|_| it.next().unwrap());
    }
    for k in 0..n {
        let o = ram.r32(TRIGS + 4 + 4 * k);
        if !ram.valid(o) || ram.r32(o) != 0x0804_9BD4 {
            continue;
        }
        let (x0, y0, x1, y1) = (ram.s16(o + 4), ram.s16(o + 6), ram.s16(o + 8), ram.s16(o + 0xA));
        let v = ram.r16(o + 0x10);
        if let Some(t) = w.field.as_mut().and_then(|f| f.triggers.iter_mut().find(|t| (t.rect.x0, t.rect.y0, t.rect.x1, t.rect.y1) == (x0, y0, x1, y1))) {
            t.resume = v;
        }
    }
    // a touch-once trigger that fired is gone from the list until the area loads again: ours (the load's, its spawn
    // condition read before the event's flag) goes too (whole-run cut 887,156: s437's trigger, flag 108 set in the
    // cut's next frame - ours fired s437 again)
    let rects: Vec<(i32, i32, i32, i32)> = (0..n).map(|k| ram.r32(TRIGS + 4 + 4 * k)).filter(|&o| ram.valid(o)).map(|o| (ram.s16(o + 4) as i32, ram.s16(o + 6) as i32, ram.s16(o + 8) as i32, ram.s16(o + 0xA) as i32)).collect();
    if let Some(f) = w.field.as_mut() {
        f.triggers.retain(|t| t.act != wf_core::fieldcontent::Activation::TouchOnce || rects.contains(&(t.rect.x0 as i32, t.rect.y0 as i32, t.rect.x1 as i32, t.rect.y1 as i32)));
    }
}

/// an object's actor row the recording does not have (an object out of the live list): sprite by its def (+0x58), the
/// animation raw = the def's entry holding the current frame record (+0x54), cel +0x11, tick +0x12 - as the recorder's
/// `capture.actors`
fn actor_row_of(data: &wf_core::content::GameData, ram: &GbaRam, o: u32) -> Option<(u32, i32, i32, i32, i32)> {
    let d = ram.r32(o + 0x58);
    let (&sprite, _) = data.sprites.iter().find(|(_, sp)| sp.def_addr == d)?;
    // (the def's animation table is ROM, which the import does not have: the animation whose steps hold the
    // object's current frame record, +0x54 - the lowest raw sharing it, as the recorder names a struct)
    let rec = ram.r32(o + 0x54);
    // (+0x54 is not always a record - an NPC idle in the shared idle state 0x03001B0C showed another word: raw 1,
    // the idle's - run 1 cut 1,629,943)
    let idle = if ram.r32(o + 0x14) == 0x0300_1B0C { 1 } else { -1 };
    let raw = data.sprites.get(&sprite).and_then(|sp| sp.anims.iter().find(|an| an.dirs.iter().any(|st| st.iter().any(|x| x.rom == rec))).map(|an| an.canon_raw)).unwrap_or(idle);
    Some((o, sprite + wf_core::content::GFX_OFFSET, raw, ram.r8(o + 0x11) as i32, ram.r16(o + 0x12) as i32))
}

fn import_objects(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let mut rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    // the objects a purge suspended under a text box (out of the world list - the recording has no rows for them):
    // built like the list's, then suspended (`import_vram`) - run 1 cut 195,432, 7-6: three crowd NPCs back when the
    // box closes
    let susp = ram.suspended_objects();
    for &o in &susp {
        if let Some(r) = actor_row_of(&w.data, ram, o).filter(|_| !rows.contains_key(&o)) {
            rows.insert(o, (r.1, r.2, r.3, r.4));
        }
    }
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
    // (RAM enemy object -> our entity: the worms' links, `import_worms`)
    let mut enemy_of: Vec<(u32, wf_core::world::EntityId)> = vec![];
    // the world list is updated last to first: RAM order = our entity order
    // (the leader's object takes its place among the actors too: real run 0-21 tile r033_002+25000 - Majin Buu is
    // before the player in the list, so the player updates first and Buu's IfPlayerWithin2 sees this frame's step)
    let leader_obj = ram.r32(0x0300_1C34);
    for o in ram.objects().into_iter().chain(susp) {
        if o == leader_obj {
            if let Some(&pid) = players.first() {
                order.push(pid);
            }
            continue;
        }
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
                // the area's own NPC of that sprite (its behaviour; the load did not spawn it - its condition), the
                // nearest; else a bare one
                let def = w.field.as_ref().map(|f| f.area).and_then(|a| w.data.field.as_ref().and_then(|fd| fd.area(a.world, a.area)).map(|ad| ad.objects.clone())).unwrap_or_default().into_iter().filter(|d| matches!(&d.kind, wf_core::fieldcontent::ObjKind::Npc { sprite, behaviour, .. } if *sprite == key.1 && !behaviour.is_empty())).min_by_key(|d| match &d.kind {
                    wf_core::fieldcontent::ObjKind::Npc { x, y, .. } => ((*x - (ox >> 8)).abs() + (*y - (oy >> 8)).abs()) as i64,
                    _ => i64::MAX,
                });
                let rng = w.rng.clone();
                let made = def.and_then(|d| wf_core::fieldnpc::spawn(w, &d));
                w.rng = rng;
                match made {
                    Some(id) => id,
                    None => {
                        let n = wf_core::scene::Npc::new(&w.data, key.1, ox >> 8, oy >> 8, of);
                        w.add(Body::Npc(Box::new(n)))
                    }
                }
            }
        };
        let Some(i) = w.find(id) else { continue };
        order.push(id);
        match key.0 {
            0 => {
                enemy_of.push((o, id));
                patch_enemy(w, i, ram, o, rows.get(&o).copied(), notes);
                place_enemy(w, id, key.1, ox, oy, rows.get(&o).map(|r| r.0), ram.r32(o + 0x228));
            }
            _ => {
                let mut pup = None;
                if let Body::Npc(n) = &mut w.ents[i].body {
                    n.a.x = ox;
                    n.a.y = oy;
                    n.a.facing = of;
                    if let Some(&(_, raw, cel, tick)) = rows.get(&o) {
                        n.anim.raw = raw;
                        n.anim.frame = cel;
                        n.anim.tick = tick;
                        // a map NPC's behaviour keeps its own animation state (fieldnpc.rs: SetAnim 0 idle / 1 walk)
                        if let Some(b) = n.behaviour.as_mut() {
                            (b.raw, b.frame, b.tick) = (raw, cel, tick);
                            b.anim = if raw == 1 { 0 } else { 1 };
                        }
                    }
                    // the hold state 0x03001B00 (`set_pose`): frozen on its cel
                    let held = ram.r32(o + 0x14) == 0x0300_1B00;
                    if held {
                        n.anim.mode = wf_core::scene::AnimMode::Pose;
                    }
                    // ... and, inside an event, an actor the script holds: its own behaviour waits for the event's
                    // end (r033_002_6-1 +17,000: s502's fusion pose, sprite 123 frozen on cel 10)
                    if held && n.behaviour.is_some() {
                        pup = Some(n.anim.clone());
                    }
                    // a map NPC's talk resume node (+0x1BC: the exit value of its last talk)
                    if vt == VT_NPC && n.talk.is_some() {
                        n.resume = ram.r16(o + 0x1BC);
                    }
                }
                if let Some(anim) = pup.filter(|_| w.scene.depth > 0 && !w.scene.puppets.iter().any(|p| p.id == id)) {
                    w.scene.puppets.push(wf_core::scene::Puppet { id, anim });
                }
                import_npc_step(w, i, ram, o, rows.get(&o).copied());
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
    import_worms(w, ram, &enemy_of);
    // an Enzyme's parent Blister (obj+0x1F4): its death counts the Blister's +0x204 down (`K::LatchRelease`)
    for &(o, id) in &enemy_of {
        let po = ram.r32(o + 0x1F4);
        let Some(&(_, pid)) = enemy_of.iter().find(|e| e.0 == po && po != 0) else { continue };
        let blister = w.find(pid).is_some_and(|pi| matches!(&w.ents[pi].body, Body::Enemy(e) if e.ai.stack.iter().any(|st| st.tree.nodes.iter().any(|n| n.k == wf_core::ai::K::SpawnEnzymes))));
        if blister {
            if let Some(i) = w.find(id) {
                if let Body::Enemy(e) = &mut w.ents[i].body {
                    e.parent = pid;
                }
            }
        }
    }
    // ... and the Blister's child list in its order (first child +0x1F0, next sibling +0x1F8: the oldest first; its
    // SelfDestruct death takes the first one with it)
    for &(o, id) in &enemy_of {
        let mut c = ram.r32(o + 0x1F0);
        let blister = w.find(id).is_some_and(|i| matches!(&w.ents[i].body, Body::Enemy(e) if e.ai.stack.iter().any(|st| st.tree.nodes.iter().any(|n| n.k == wf_core::ai::K::SpawnEnzymes))));
        if c == 0 || !blister {
            continue;
        }
        let mut kids = vec![];
        while c != 0 && kids.len() < 64 {
            if let Some(&(_, cid)) = enemy_of.iter().find(|e| e.0 == c) {
                kids.push(cid);
            }
            c = ram.r32(c + 0x1F8);
        }
        if let Some(i) = w.find(id) {
            if let Body::Enemy(e) = &mut w.ents[i].body {
                e.children = kids;
            }
        }
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

/// the worms (21-x: a head and its body segments, WormHeadMove / WormBodyFollow): the chain is the objects' next
/// pointer (obj+0x1F0) - the head sends each move's direction to its next one's mailbox (obj+0x1E4 = dir | 0x80,
/// 0x0802D598) and a body, on a command, takes the oldest of its 12-slot ring (the node's runtime +4.., index
/// +0x10), stores the new one there and passes the old on (0x0802D6A2) - our `cmds` (oldest first, 12, plus the
/// pending command); the head's direction is its facing (obj+0x10). Run 1 cut 1,368,272 (21-11's saved room): ours
/// had no links after the import, the segments stood still (a body 1 px off at f201)
fn import_worms(w: &mut World, ram: &GbaRam, enemy_of: &[(u32, wf_core::world::EntityId)]) {
    use wf_core::ai::K;
    let has = |w: &World, i: usize, k: K| w.en_ref(i).ai.stack.iter().any(|st| st.tree.nodes.iter().any(|n| n.k == k));
    for &(o, id) in enemy_of {
        let Some(i) = w.find(id) else { continue };
        let (head, body) = (has(w, i, K::WormHeadMove), has(w, i, K::WormBodyFollow));
        if !head && !body {
            continue;
        }
        let facing = ram.r8(o + 0x10) & 3;
        w.en(i).worm_dir = facing;
        // a hit's guard (obj+0x204, ShieldHit 15, counted down by the piece's update) and its hittable flag (+0x1CC)
        w.en(i).worm_guard = ram.s32(o + 0x204);
        w.en(i).a.hittable = ram.r8(o + 0x1CC) != 0;
        // a head off the enemy grid (obj+0x291: split off a chain, not marked yet) - not a free-moving "detached"
        // enemy (`worm_head_step` marks it first)
        if ram.r8(o + 0x291) != 0 {
            // (a body piece is off the grid too: ours keeps it unmarked by its prev link; it may become a head)
            w.en(i).detached = false;
            if head {
                w.en(i).worm_remark = true;
            }
            wf_core::enemy::unmark_self(w, i);
        }
        // (the load's own chain links go: a tail's stale next fed the segment before it - cut 1,368,272 f214)
        let nx = ram.r32(o + 0x1F0);
        let nid = enemy_of.iter().find(|e| e.0 == nx).map(|e| e.1).unwrap_or(0);
        w.en(i).next = nid;
        if !enemy_of.iter().any(|e| ram.r32(e.0 + 0x1F0) == o) {
            w.en(i).prev = 0;
        }
        if let Some(ni) = w.find(nid).filter(|_| nid != 0) {
            w.en(ni).prev = id;
        }
        if body {
            // the node's runtime (the state level running WormBodyFollow)
            let lvl = w.en_ref(i).ai.stack.iter().position(|st| st.tree.nodes.first().is_some_and(|n| n.k == K::WormBodyFollow));
            let r = lvl.map(|l| ram.r32(o + 0x238 + 8 + 8 * l as u32)).filter(|&r| ram.valid(r));
            if let Some(r) = r {
                let idx = ram.s32(r + 0x10).rem_euclid(12) as u32;
                let mut cmds: Vec<u8> = (0..12).map(|j| ram.r8(r + 4 + (idx + j) % 12) & 3).collect();
                let mail = ram.r32(o + 0x1E4);
                if mail & 0x80 != 0 {
                    cmds.push((mail & 3) as u8);
                }
                let e = w.en(i);
                e.cmds = cmds;
                // (the mailbox word is the AI counter's slot for other enemies; for a worm it is the command)
                e.counter = 0;
            }
        }
    }
}

/// an imported enemy the field does not track as a placement yet (built bare by the mid-area import) becomes one: the
/// area's nearest unclaimed def of its type (its on-defeat callback), else a run-time enemy `ObjRef::List(0xff, type)`
/// - so its death runs the type's drop script and the callback like the load's (real run 4-1 tile r002_006+11,000:
/// a bandit knocked down at f32 is gone at f49; the game's drop script draws 4 x rand / 6 x chance at f57)
/// (`sprite`: the RAM actor's sprite - placements of one type at one spot differ by it and by their callbacks: real
/// run 7-6, the tournament's two Trunks placements (list 1 #2 sprite 123 -> s300, #3 sprite 124 -> s301, both made by
/// scripts at (344, 600)); the nearest of the matching sprite first)
fn place_enemy(w: &mut World, id: wf_core::world::EntityId, ty: i32, ox: i32, oy: i32, sprite: Option<i32>, def_ptr: u32) {
    let Some(f) = w.field.as_ref() else { return };
    if f.placed.iter().any(|p| p.0 == id) {
        return;
    }
    let area = f.area;
    let claimed: Vec<wf_core::fieldcontent::ObjRef> = f.placed.iter().map(|p| p.1).collect();
    let defs = w.data.field.as_ref().and_then(|fd| fd.area(area.world, area.area)).map(|a| a.objects.clone()).unwrap_or_default();
    // (the enemy object's +0x228 points 8 bytes before its def record: the def itself when unclaimed - run 2 cut
    // 1,928,000, 19-3: four script-spawned Kid Buu defs at one spot, the live one list 1 #2 with on-defeat s460)
    let by_ptr = defs.iter().find(|d| !claimed.contains(&d.at) && d.rom == def_ptr.wrapping_add(8) && matches!(d.kind, wf_core::fieldcontent::ObjKind::Enemy { .. })).map(|d| d.at);
    let at = by_ptr.or_else(|| defs
        .iter()
        .filter(|d| !claimed.contains(&d.at))
        .filter_map(|d| match &d.kind {
            wf_core::fieldcontent::ObjKind::Enemy { ty: t, x, y, sprite: sp, .. } if *t == ty => {
                Some((d.at, (sprite.is_some_and(|s| s != *sp), ((*x - (ox >> 8)).abs() + (*y - (oy >> 8)).abs()) as i64)))
            }
            _ => None,
        })
        .min_by_key(|x| x.1)
        .map(|x| x.0))
        .unwrap_or(wf_core::fieldcontent::ObjRef::List(0xff, ty as u16));
    if let Some(f) = w.field.as_mut() {
        f.placed.push((id, at, ox >> 8, oy >> 8));
    }
}

/// an enemy's AI state stack (obj+0x238: +4 depth, +8 {runtime, def} per level; def = 0x0870F514 + 8 * state)
fn ai_states(ram: &GbaRam, o: u32) -> Vec<i32> {
    let st = o + 0x238;
    let n = ram.r32(st + 4).min(8);
    (0..n).map(|i| (ram.r32(st + 12 + 8 * i).wrapping_sub(0x0870_F514) / 8) as i32).collect()
}

/// an in-event import (the event's world rebuilt by replaying it from its start): the world map's patrol airships
/// (enemies running a 3-D move step, vt 0x0804A800 at obj+0x22C) are where the replay's load put them, not where
/// they have flown since - each RAM patrol patches the nearest of our enemies of its type (real run 0-50 tiles
/// r002_006 +9,000 / +17,000, s515's arrival: "RAM actor ... has no entity there" for all ten)
/// the build's enemies run their animation from the load (an idle's start tick is the build's rand(255)); the RAM
/// rows have the game's: each enemy at a RAM enemy object's exact position showing the same raw takes its cel and
/// tick (the world map's airships, rkb_001 0-50: idle ticks 6 apart)
fn enemy_anims_from_rows(w: &mut World, ram: &GbaRam) {
    let rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    let mut used: Vec<wf_core::world::EntityId> = vec![];
    for o in ram.objects() {
        if ram.r32(o) != VT_ENEMY {
            continue;
        }
        let Some(&(_, raw, cel, tick)) = rows.get(&o) else { continue };
        let (ox, oy) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        let pick = (0..w.ents.len()).find(|&i| {
            !w.ents[i].removed && !used.contains(&w.ents[i].id) && matches!(&w.ents[i].body, Body::Enemy(e) if e.a.x == ox && e.a.y == oy && e.ea.pos.raw == raw)
        });
        if let Some(i) = pick {
            used.push(w.ents[i].id);
            let e = w.en(i);
            e.ea.pos.frame = cel;
            e.ea.pos.tick = tick;
        }
    }
}

fn patch_patrols(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    let mut used: Vec<wf_core::world::EntityId> = vec![];
    for o in ram.objects() {
        if ram.r32(o) != VT_ENEMY || ram.r32(ram.r32(o + 0x22C)) != 0x0804_A800 {
            continue;
        }
        let ty = ((ram.r32(o + 0x224).wrapping_sub(0x0871_1274)) / 28) as i32;
        let (ox, oy) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        let pick = (0..w.ents.len())
            .filter(|&i| !w.ents[i].removed && !used.contains(&w.ents[i].id))
            .filter(|&i| matches!(&w.ents[i].body, Body::Enemy(e) if e.ty == ty))
            .filter(|&i| w.field.as_ref().is_some_and(|f| f.patrols.iter().any(|p| p.entity == w.ents[i].id)))
            .min_by_key(|&i| {
                let a = w.actor(i).unwrap();
                ((a.x - ox) as i64).abs() + ((a.y - oy) as i64).abs()
            });
        match pick {
            Some(i) => {
                used.push(w.ents[i].id);
                patch_enemy(w, i, ram, o, rows.get(&o).copied(), notes);
            }
            None => notes.push(format!("patrol {o:#x} type {ty} has no entity to patch")),
        }
    }
}

/// an enemy entity set to a RAM enemy object's state
fn patch_enemy(w: &mut World, ei: usize, ram: &GbaRam, o: u32, row: Option<(i32, i32, i32, i32)>, notes: &mut Vec<String>) {
    let states = ai_states(ram, o);
    // the state stack as the RAM has it (re-entered: their runtimes come next); the build's own states go without
    // their exits (nothing of them ran in the game: a wander's exit would idle and turn the enemy)
    if let Some(&first) = states.first() {
        // (the re-entry is bookkeeping, not the game's frame: what the enters do to the players - a MeleeAttack's
        // strike on enter - is undone; whole-run cuts 808,709 / 808,776 / 809,325: the leader lost 28 HP twice)
        let players: Vec<(usize, wf_core::world::Entity)> = (0..w.ents.len()).filter(|&k| w.players.contains(&w.ents[k].id)).map(|k| (k, w.ents[k].clone())).collect();
        let (stats, nev) = (w.stats.clone(), w.events.len());
        let n_ents = w.ents.len();
        let e = w.en(ei);
        let old: Vec<_> = e.ai.stack.drain(..).collect();
        e.ai.orphans.extend(old);
        wf_core::nodes::reset(w, ei, first as u16);
        for &s in states.iter().skip(1) {
            wf_core::nodes::push(w, ei, s as u16);
        }
        for (k, ent) in players {
            w.ents[k] = ent;
        }
        // (and what the enters spawned - the RAM's objects are imported on their own: run 1 cut 1,368,272, 21-11's
        // worm head in WormSpawnBody spawned its five body segments again)
        for k in n_ents..w.ents.len() {
            let id = w.ents[k].id;
            wf_core::vram::release(w, wf_core::vram::Owner::Ent(id));
            w.ents[k].removed = true;
        }
        w.stats = stats;
        w.events.truncate(nev);
    }
    for lvl in 0..states.len() {
        let r = ram.r32(o + 0x238 + 8 + 8 * lvl as u32);
        import_ai_rt(w, ei, lvl, ram, r, notes);
    }
    // off the enemy grid (obj+0x291, `0x0800F924`: a SpawnPuff / DriftContact / bat enter set it; it never marks
    // again - whole-run cut 1,121,918, HFIL's wandering souls walk through a wall row)
    if ram.r8(o + 0x291) != 0 && !w.en_ref(ei).detached {
        wf_core::enemy::unmark_self(w, ei);
        w.en(ei).detached = true;
    }
    // the Blister's spawn count (obj+0x204, +1 a spawn: SpawnEnzymes stops at 8)
    if w.en_ref(ei).ai.stack.iter().any(|st| st.tree.nodes.iter().any(|n| n.k == wf_core::ai::K::SpawnEnzymes)) {
        w.en(ei).spawned = ram.s32(o + 0x204);
    }
    import_enemy_beam(w, ei, ram, o, notes);
    import_super_ball(w, ei, ram, o, notes);
    import_bombs(w, ei, ram, o);
    import_flames(w, ei, ram, o);
    import_waves(w, ei, ram, o);
    import_shells(w, ei, ram, o);
    import_wraps(w, ei, ram, o);
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
    // off the enemy grid (obj+0x291 = 1, `0x0800F924`: a bat-form Nosferatu, a Blood Cell, a puff): it fits anywhere
    // and marks no tiles - the imported grid has none of its own (exit-fade sweep 15-23: a bat's chase re-planned
    // into the walls it flies over, ours wall-followed)
    e.detached = ram.r8(o + 0x291) != 0;
    if e.detached {
        e.marked = None;
    }

    if let Some((sprite, raw, cel, tick)) = row {
        // (the sprite the object shows: the world map's circles pick theirs by the place's state)
        if e.a.sprite != Some(sprite - wf_core::scene::SPRITE_BASE) {
            e.a.set_sprite(&d, Some(sprite - wf_core::scene::SPRITE_BASE));
            // a swapped form (MechTransform / the bat): the next swap starts from the one shown (whole-run cut
            // 809,790: a Mechanoid already in its 188 form turns back to 187 at f23 - ours swapped 187 -> 188)
            if d.enemy(e.ty).and_then(|t| t.gfx) != Some(sprite) {
                e.gfx_now = sprite;
            }
        }
        e.set_raw(&d, raw);
        e.ea.pos.raw = raw;
        e.ea.pos.frame = cel;
        e.ea.pos.tick = tick;
        // the animation's rule from its state object (obj+0x14) when it is a shared one (IWRAM singletons: idle
        // 0x03001B0C, loops, one-shots): a node the import re-entered may have set another (mid_7000: a
        // knockback's one-shot over the idle the RAM shows)
        let st = format!("0x{:08X}", ram.r32(o + 0x14));
        if let Some(&r) = d.anim.generic.get(&st) {
            e.ea.rule = r;
            e.ea.done = false;
        } else if ram.valid(ram.r32(o + 0x14)) && wf_core::eanim::KEEP_VTABLES.contains(&format!("0x{:08X}", ram.r32(ram.r32(o + 0x14))).as_str()) {
            // a keep state (vt 0x0804985C / 0x0804A214: the pose stays, nothing ticks - run 2 cut 58,350, s103's
            // return_to_field: the defeated fighter's raw 18 frozen at tick 360; ours ended the pose and idled, a draw)
            e.ea.rule = d.anim.keep;
            e.ea.done = false;
        }
        // the walk state (the IWRAM singleton 0x03001B58, vt 0x0804A1D0 - SetAnim(1)) is our "walk" animation even
        // where the sprite's walk shows the idle's raw (the Enzymes' raw 1: `set_raw` picked the idle) - run 2 cut
        // 1,719,016, 21-x: a level-up's stop message re-idles the walkers (0x0800E050, a rand(255) each), ours took
        // them for idle
        if ram.r32(o + 0x14) == 0x0300_1B58 {
            if let Some(k) = e.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.anims.iter().position(|a| a.name == "walk")) {
                e.a.anim = Some(k as u16);
            }
        }
    }
    // Mini Janemba's roll controller (the singleton 0x03003248 as the object's state, obj+0x14): its counter is the
    // animation tick the RAM row shows; the MiniRollAttack node reads it (run 2 rows 1,318,750 .. 1,319,519: a roll
    // whose counter is far past its 0x100 end - the node never ends; the re-entered node restarted it from 0)
    if ram.r32(o + 0x14) == 0x0300_3248 {
        let (tick, facing) = (w.en_ref(ei).ea.pos.tick, ram.r8(o + 0x10) & 3);
        let e = w.en(ei);
        e.roll_ctl = tick & 0xffff;
        e.a.facing = facing;
    } else if w.en_ref(ei).roll_ctl >= 0 {
        w.en(ei).roll_ctl = -1;
    }
    // a HeavyPunch in progress: its frame count is the controller's tick (obj+0x12), its end 48 + 8 * the punch
    // animation's frames as its enter computes (real run 4-1 whole-run cut 621,339: the punch landed a frame before
    // the cut, tick 49 - the import's fresh node ended at once and idled, a draw the game does not make)
    let ctl_obj = ram.r32(o + 0x14);
    let mut barrage = None;
    let mut taunt = false;
    {
        let tick = w.en_ref(ei).ea.pos.tick;
        let raw_now = w.en_ref(ei).ea.pos.raw;
        let field = w.field.is_some();
        let e = w.en_ref(ei);
        let nf = e.a.anim_raw(&d).and_then(|raw| e.a.sprite.and_then(|s| d.sprite(s)).and_then(|s| s.anim_raw(raw))).map(|a| a.steps(e.a.facing).len() as i32);
        let anim_seq = e.anim_seq as i32;
        for st in w.en(ei).ai.stack.iter_mut() {
            for (n, rt) in st.rt.iter_mut().enumerate() {
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::HeavyPunch) {
                    rt.f = tick;
                    rt.d[0] = 48 + 8 * nf.unwrap_or(2);
                    // (its controller as the object's state: the punch counts on through an event, `nodes::is_ctl`)
                    if field && ctl_obj == 0x0300_32F4 {
                        barrage = Some((st.serial, n as u16));
                    }
                }
                // a FlameBreath in progress: its frame count is the controller's tick too (whole-run cut 787,315, Dragon
                // Ball 2's desert: tick 41, a flame a frame)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::FlameBreath) {
                    rt.f = tick;
                }
                // a TauntAnim in progress (the rock thrower's controller, a heap object vt 0x0804E030 as the enemy's
                // state, counting in the object's tick: the rock at 8, SetAnim(0) at 0x20): the frame count is the tick
                // (run 3 cuts 155,835 .. 158,217, Earth 2-90: tick 31 -> the idle's rand(255) at f0; tick 7 -> the rock's
                // two rand(64) at f0 - a fresh count did neither)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::TauntAnim) && ram.valid(ctl_obj) && ram.r32(ctl_obj) == 0x0804_E030 {
                    rt.f = tick;
                    taunt = true;
                }
                // a HandAmbush past its first update (exit-fade sweep 18-10: the hand already at the player; ours drew
                // its side again): the count follows the strike animation (set on that first update, tick 0)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::HandAmbush) && rt.f == HAND_STARTED {
                    rt.f = tick + 1;
                }
                // a SoldierThrow in progress (the controller 0x03002FCC, as FlameBreath): its frame count is the tick,
                // its state the throw's raw animation (whole-run cut 497,351: Babidi's ship soldier mid-throw)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::SoldierThrow) {
                    rt.f = tick;
                    rt.d[2] = raw_now;
                }
                // a NinjaStarThrow in progress (its controller 0x03003074 as the object's state): the count is the tick
                // (tour desert_nj37: tick 1 at the cut - the star and sound 61 at f7; a fresh count threw 8 late)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::NinjaStarThrow) && ctl_obj == 0x0300_3074 {
                    rt.f = tick;
                    rt.d[6] = anim_seq;
                }
                // the same for a GunShot / RifleShot / MummyWrapShot in progress (controllers 0x03002FC4 / 0x03002FD8 /
                // 0x030030A8; run 3 cut 1,286,554, fat Buu: a gunman's tick 10 - the fresh node's state did not match
                // the animation's, it ended at once and his idle's draw came 2 frames early)
                if st.tree.nodes.get(n).is_some_and(|x| matches!(x.k, wf_core::ai::K::GunShot | wf_core::ai::K::RifleShot | wf_core::ai::K::MummyWrapShot | wf_core::ai::K::MummyWrapBurst))
                    && matches!(ctl_obj, 0x0300_2FC4 | 0x0300_2FD8 | 0x0300_30A8)
                {
                    rt.f = tick;
                    rt.d[6] = anim_seq;
                }
                // a Janemba Hand's HandAmbush in progress (runtime vt 0x0804C174: +4 entered): placed and striking, its
                // window counted on the strike animation's tick (field: ours counts updates, the first one places;
                // whole-run cut 1,121,864, HFIL 18-10: tick 36 - a fresh node jumped the hand to the leader again)
                if field && st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::HandAmbush) && raw_now >= 0 && tick > 0 {
                    rt.f = tick + 1;
                }
                // GhostAnimA / B in progress (no runtime words; the one-shot state's tick): ours counts its updates
                // (at least 16 before the end test) - from the animation's tick (whole-run cut 1,121,864, HFIL 18-10: a
                // soul's appear at tick 29, the paired graphics 9 frames later)
                if st.tree.nodes.get(n).is_some_and(|x| matches!(x.k, wf_core::ai::K::GhostAnimA | wf_core::ai::K::GhostAnimB)) && tick > 0 {
                    rt.f = tick;
                }
                // Pikkon's EnergyBarrage in progress (its controller 0x030030E0 as the object's state, obj+0x14): the
                // count is the controller's tick, and the controller is the enemy's - a pushed hit state replaces it
                // (nodes.rs `push`; run 2 cut 56,305: the punch at 56,339 stops its sound 63)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::EnergyBarrage) && ctl_obj == 0x0300_30E0 && (1..48).contains(&tick) {
                    rt.f = tick;
                    barrage = Some((st.serial, n as u16));
                }
                // Kid Buu's split animation in progress (no runtime words; ours ends at 56 updates): the count is the
                // one-shot's tick (run 2 cut 1,927,900, 19-3: tick 26 - a fresh count split him 26 frames late)
                if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::KidBuuSplitAnim) && tick > 0 {
                    rt.f = tick;
                }
            }
        }
    }
    if let Some(c) = barrage {
        w.en(ei).ctl = c;
    }
    // (and its animation's rule, as its enter set it: the one-shot that ends at the controller's 0x20 - the import's
    // re-entry did not enter the node)
    if taunt {
        let once = d.anim.setanim[&4];
        let e = w.en(ei);
        e.ea.rule = once;
        e.ea.done = false;
    }
    // GhostAnimA / B (update 0x0802DFD8): its end test is `0x0800EE9E` - the object back in the normal state (obj+0x14
    // == 0x03001BA4); ours waits for the one-shot's end - over already when RAM shows the normal state (a cut inside
    // a cutscene map at 14-17, s415: the ghost swapped its graphics in the field's first update after the event)
    let in_ghost = w.en_ref(ei).ai.stack.last().is_some_and(|s| s.tree.nodes.iter().any(|x| matches!(x.k, wf_core::ai::K::GhostAnimA | wf_core::ai::K::GhostAnimB)));
    if in_ghost && ram.r32(o + 0x14) == 0x0300_1BA4 {
        w.en(ei).anim_ended = true;
    }
        // (a knockback's animation index: the one now showing)
    let cur = w.en_ref(ei).a.anim.map(|a| a as i32 + 1).unwrap_or(0);
    for st in w.en(ei).ai.stack.iter_mut() {
        for (n, rt) in st.rt.iter_mut().enumerate() {
            if st.tree.nodes.get(n).is_some_and(|x| x.k == wf_core::ai::K::KnockbackSlide) && rt.d[3] == -1 {
                rt.d[3] = cur;
            }
        }
    }
    wf_core::enemy::sync_mark(w, ei);
    import_patrol(w, ei, ram, o);
    // the +0x214 word an AI RunScript hands its event (Super Buu's waypoint index; after the state rebuild, whose
    // glide enter stepped it)
    w.en(ei).p214 = ram.s32(o + 0x214);
    // LatchDrain's controller (enemy +0x14 -> vt 0x0804C314; its think 0x080298A2: position = the player's + (+8, +0xC)):
    // the kept offset, not the one a re-entry measures (run 2 cuts 1,640,133 / 1,665,790 / 1,679,592 / 1,682,927:
    // inside Buu's cells latched on the leader moved by the leader's step at f0)
    let st = ram.r32(o + 0x14);
    if ram.valid(st) && ram.r32(st) == 0x0804_C314 {
        w.en(ei).hook = wf_core::enemy::Hook::Latch { ox: ram.s32(st + 8), oy: ram.s32(st + 0xC) };
    }
}

/// this enemy's bombs in flight or on the ground (BombThrow; class 0x0804A2E8, MEASURED whole-run cut 621,369: +0x1C
/// the thrower, +0x10 direction, +0x20 / +0x24 position 24.8, +0x48 the counter (bursts at 71), +0x4C / +0x50
/// velocity): rebuilt as the shot (West City 4-1: a bomb lying beside the player bursts 26 frames after the cut -
/// the import had dropped it, the player was not hurt)
fn import_bombs(w: &mut World, ei: usize, ram: &GbaRam, o: u32) {
    let me = w.ents[ei].id;
    for b in ram.ewram_with(0x0804_A2E8).into_iter().filter(|&b| ram.r32(b + 0x1C) == o) {
        let t = ram.s32(b + 0x48);
        let mut s = wf_core::shots::bomb(me, ram.r8(b + 0x10) & 3, ram.s32(b + 0x20), ram.s32(b + 0x24), 0, 0);
        s.t = t as i64;
        s.kind = wf_core::shots::ShotKind::Bomb { vx: ram.s32(b + 0x4C), vy: ram.s32(b + 0x50), landed: if t >= 40 { 40 } else { -1 } };
        wf_core::shots::add(w, s);
    }
}

/// Kid Buu's beam in flight at the cut (the KidBuuKamehameha node; the beam object vt 0x0804AEF8, owner +0x50:
/// +0x10 fired, +0x11 head stopped, +0x14 next hit-effect frame, +0x18 ticks since it was made, +0x20 / +0x24 origin
/// px, +0x38 length, +0x3C direction): the shot rebuilt with those numbers, the node's frame count = the beam's
/// ticks (it is made in the node's enter)
fn import_enemy_beam(w: &mut World, ei: usize, ram: &GbaRam, o: u32, notes: &mut Vec<String>) {
    use wf_core::ai::K;
    // every beam object of this enemy (+0x10: 0 charging - made in the node's enter, its direction (+0x3C) not set
    // yet: the enemy's facing -, 1 fired, 2 released and retracting): a released one goes on retracting on its own
    // (run 2 cut 1,927,800, 19-3: Kid Buu's beam retracts after its node ended, its destructor's stop 32 / sound 33
    // 81 frames in; it was left a foreign block), the node's own is the newest unreleased one (run 2 cut 1,927,640: a
    // fresh charging beam beside a retracting one)
    let bs: Vec<u32> = ram.ewram_with(0x0804_AEF8).into_iter().filter(|&b| ram.r32(b + 0x50) == o).collect();
    if bs.is_empty() {
        return;
    }
    let spots: Vec<(usize, usize)> = w.en_ref(ei).ai.stack.iter().enumerate().flat_map(|(l, s)| s.tree.nodes.iter().enumerate().filter(|(_, n)| n.k == K::KidBuuKamehameha).map(move |(n, _)| (l, n))).collect();
    let me = w.ents[ei].id;
    // (the RAM's facing: this runs before the enemy's own is patched)
    let facing = ram.r8(o + 0x10) & 3;
    let node_beam = bs.iter().copied().filter(|&b| ram.r8(b + 0x10) != 2).min_by_key(|&b| ram.s32(b + 0x18));
    for &b in &bs {
        let state = ram.r8(b + 0x10);
        let own = Some(b) == node_beam;
        if own && spots.is_empty() {
            notes.push("a beam object of this enemy outside a KidBuuKamehameha node: not imported".into());
            continue;
        }
        let dir = if ram.r8(b + 0x3C) < 4 { ram.r8(b + 0x3C) } else { facing };
        // (a charging beam's object stands at the charge position; ours is made at the fire position, as the node's
        // enter: the enemy's position + the table per facing (nodes.rs KidBuuKamehameha) - the game moves it there at
        // the fire: run 2 cut 1,927,640, (253, 381) charging, (229, 385) fired)
        let (x, y) = if state == 0 {
            const OFF: [(i32, i32); 4] = [(16, 20), (16, -4), (-12, 2), (44, 2)];
            let a = &w.en_ref(ei).a;
            let (ox, oy) = OFF[dir as usize & 3];
            (a.px() + ox, a.py() + oy)
        } else {
            (ram.s32(b + 0x20), ram.s32(b + 0x24))
        };
        let vt = ram.s32(b + 0x18);
        let id = wf_core::shots::add(w, wf_core::shots::kame_beam_enemy(me, dir, x, y, ram.s32(b + 0x14) as i64, wf_core::damage::atk(5), 41));
        if let Some(k) = w.find(id) {
            let s = w.shot(k);
            s.t = vt as i64;
            if let wf_core::shots::ShotKind::Kame(kb) = &mut s.kind {
                kb.len = ram.s32(b + 0x38);
                kb.vt = vt;
                kb.stopped = ram.r8(b + 0x11) != 0;
                kb.next = ram.s32(b + 0x14) as i64;
                kb.charging = state == 0;
                kb.done = state == 2;
            }
        }
        if own {
            let (lvl, n) = spots[0];
            let e = w.en(ei);
            e.beam = id;
            if let Some(r) = e.ai.stack.get_mut(lvl).and_then(|s| s.rt.get_mut(n)) {
                r.d[0] = id as i32;
                r.f = vt;
            }
        }
    }
}

/// Super Buu's balls at the cut (the ThrowBigBall node; builder 0x08021280, vt 0x0804A33C in the world list, owner
/// +0x1C: +0x20 / +0x24 24.8 position, +0x40 ticks, +0x44 thrown, +0x48 burst, +0x4C / +0x50 the throw's step):
/// * charging: the shot with those ticks (ours counts one behind: the shot is first updated the frame after its
///   enter), the node's frame count = the ball's ticks (it is made in the node's enter);
/// * thrown: its start the position at the throw (tick 0x3D) = now - step x ticks since, its target start + 43 steps;
/// * bursting: the ball's own blast animation (6 records x 4 ticks, +0x40 the ticks since the impact) - ours the
///   effect the impact made. Their blocks are the ball's (`import_vram`).
/// this enemy's flames in flight (vt 0x0804BFCC, ctor 0x08024768, update 0x080247C0: +0x10 facing, +0x1C owner,
/// +0x20 / +0x24 24.8 position, +0x40 the hit-once flag, +0x48 / +0x4C velocity, +0x50 ticks; no tile block) -> our
/// flame shots, oldest first as the list has them (whole-run cuts 787,315 / 787,345: FlameBreath mid-breath)
fn import_flames(w: &mut World, ei: usize, ram: &GbaRam, o: u32) {
    let me = w.ents[ei].id;
    for b in ram.objects().into_iter().filter(|&b| ram.r32(b) == 0x0804_BFCC && ram.r32(b + 0x1C) == o) {
        let kind = wf_core::shots::ShotKind::Flame { vx: ram.s32(b + 0x48), vy: ram.s32(b + 0x4C), done: ram.s32(b + 0x40) != 0 };
        let mut s = wf_core::shots::Shot::new(me, ram.r8(b + 0x10) & 3, 0, ram.s32(b + 0x20), ram.s32(b + 0x24), kind);
        s.t = ram.s32(b + 0x50) as i64;
        wf_core::shots::add(w, s);
    }
}

/// the enemy's energy waves in flight (vt 0x08049C98, made by SoldierThrow / turrets / cyborgs; +0x4C owner, +0x10
/// facing, +0x14 ticks, +0x1C the life count, +0x20 / +0x24 the tail in whole px, +0x40 the length, +0x44 the mode -
/// 1 = retracting; MEASURED run 2 cut 544,963: a Majin Soldier's wave retracting 8 -> 4 -> gone) -> our wave shot (no
/// constructor sound); its tile block goes with it (`import_vram`)
fn import_waves(w: &mut World, ei: usize, ram: &GbaRam, o: u32) {
    let me = w.ents[ei].id;
    for b in ram.objects().into_iter().filter(|&b| ram.r32(b) == 0x0804_9C98 && ram.r32(b + 0x4C) == o) {
        // (+0x44: 0 growing, 1 pulling back after a wall / its life, 2 pulling back after a hit - both retract 4 a
        // frame from the tail, ours is mode 1: tour castle_ship_42, a wave in mode 2 from 0x20 gone at f8)
        let mode = if ram.s32(b + 0x44) != 0 { 1 } else { 0 };
        let kind = wf_core::shots::ShotKind::EnergyWave { len: ram.s32(b + 0x40), mode, life: ram.s32(b + 0x1C) };
        let mut s = wf_core::shots::Shot::new(me, ram.r8(b + 0x10) & 3, 0, ram.s32(b + 0x20) << 8, ram.s32(b + 0x24) << 8, kind);
        s.t = ram.s32(b + 0x14) as i64;
        wf_core::shots::add(w, s);
    }
}

/// this mummy's bandages (vt 0x0804A830, builder 0x0802D79C / update 0x0802D940: +0x1C the mummy, +0x20 / +0x24 24.8
/// position, +0x40 / +0x44 velocity, +0x48 the flight tick, +0x50 stuck, +0x54 the stick delay, +0x58 / +0x5C a stuck
/// one's offset - player minus bandage): our bandage with those numbers - a stuck one follows the player and slows its
/// walk / dash 48 each (`player::wrap_stats`, reapplied by every bandage update; tour kais_volc25b: one stuck at the
/// tour's start, ours walked at full speed)
fn import_wraps(w: &mut World, ei: usize, ram: &GbaRam, o: u32) {
    let me = w.ents[ei].id;
    let mut stuck = 0;
    for b in ram.objects().into_iter().filter(|&b| ram.r32(b) == 0x0804_A830 && ram.r32(b + 0x1C) == o) {
        let (vx, vy) = (ram.s32(b + 0x40), ram.s32(b + 0x44));
        let diag = vx != 0 && vy != 0;
        let dir = match (vx.signum(), vy.signum()) { (0, 1) | (-1, 1) => 0, (0, _) | (1, -1) => 1, (-1, _) => 2, _ => 3 };
        let mut s = wf_core::shots::mummy_wrap(me, dir, ram.s32(b + 0x20), ram.s32(b + 0x24), diag);
        if let wf_core::shots::ShotKind::Wrap(m) = &mut s.kind {
            m.vx = vx;
            m.vy = vy;
            m.tick = ram.s32(b + 0x48);
            m.stuck = ram.s32(b + 0x50) != 0;
            m.delay = ram.s32(b + 0x54);
            m.off_x = ram.s32(b + 0x58);
            m.off_y = ram.s32(b + 0x5C);
            stuck += m.stuck as i32;
        }
        w.add(Body::Shot(Box::new(s)));
    }
    // (the walk / dash already slowed by them: the player's stats as the bandages' last update left them)
    if stuck > 0 {
        if let Some(pi) = w.player_idx(0) {
            wf_core::player::wrap_stats(w, pi, stuck);
        }
    }
}

/// this tank's shells in flight (vt 0x0804A368, made by 0x0801C900: +0x1C the tank, +0x10 the direction, +0x20 / +0x24
/// 24.8 position, +0x40 / +0x44 velocity, +0x48 ticks - the ki blast class's layout): our shell with those numbers
/// (run 3 cut 112,165, 3-14: a shell 3 ticks out puffs its trail in the cut's first frame - its 1-unit puff)
fn import_shells(w: &mut World, ei: usize, ram: &GbaRam, o: u32) {
    let me = w.ents[ei].id;
    for b in ram.objects().into_iter().filter(|&b| ram.r32(b) == 0x0804_A368 && ram.r32(b + 0x1C) == o) {
        let s = wf_core::enemy::shell_in_flight(me, ram.r8(b + 0x10) & 3, ram.s32(b + 0x20), ram.s32(b + 0x24), ram.s32(b + 0x40), ram.s32(b + 0x44), ram.s32(b + 0x48) as i64);
        w.add(Body::Shot(Box::new(s)));
    }
}

fn import_super_ball(w: &mut World, ei: usize, ram: &GbaRam, o: u32, notes: &mut Vec<String>) {
    use wf_core::ai::K;
    const THROW: i32 = 0x3d;
    let me = w.ents[ei].id;
    for b in ram.objects().into_iter().filter(|&b| ram.r32(b) == 0x0804_A33C && ram.r32(b + 0x1C) == o) {
        let (x, y, t, thrown) = (ram.s32(b + 0x20), ram.s32(b + 0x24), ram.s32(b + 0x40), ram.r8(b + 0x44) != 0);
        if ram.r8(b + 0x48) != 0 {
            let f = wf_core::fx::Fx::Anim { id: 0x0872_a5dc, x: x >> 8, y: y >> 8, rec: 9 + t / 4, left: 6 - t / 4, rate: 0x4000, acc: (t % 4) << 14 };
            w.add(Body::Fx(Box::new(f)));
            continue;
        }
        let id = wf_core::shots::add(w, wf_core::shots::super_ball(me));
        if let Some(k) = w.find(id) {
            let s = w.shot(k);
            s.x = x;
            s.y = y;
            if thrown {
                let (vx, vy) = (ram.s32(b + 0x4C), ram.s32(b + 0x50));
                let since = t - THROW;
                let (sx, sy) = (x - vx * since, y - vy * since);
                let (tx, ty) = (((sx + vx * 43 + 128) >> 8) << 8, ((sy + vy * 43 + 128) >> 8) << 8);
                s.kind = wf_core::shots::ShotKind::SuperBall { phase: 1, sx, sy, tx, ty };
                s.t = since as i64;
            } else {
                s.t = t as i64;
            }
        }
        let spots: Vec<(usize, usize)> = w.en_ref(ei).ai.stack.iter().enumerate().flat_map(|(l, s)| s.tree.nodes.iter().enumerate().filter(|(_, n)| n.k == K::ThrowBigBall).map(move |(n, _)| (l, n))).collect();
        match spots.first() {
            Some(&(lvl, n)) => {
                if let Some(r) = w.en(ei).ai.stack.get_mut(lvl).and_then(|s| s.rt.get_mut(n)) {
                    r.f = t;
                }
            }
            None if !thrown => notes.push("a charging ball of this enemy outside a ThrowBigBall node".into()),
            None => {}
        }
    }
}

/// a map NPC's behaviour step from RAM (talk-NPC object: +0x1B0 the current step object): a wander (vt 0x0804D848:
/// +8 frames left, +0x18 / +0x1C 24.8 velocity, +0x20 / +0x24 target px) -> the behaviour's wander step, created as
/// the runner would, with that progress; the animation as the row shows it (the step's create sets one)
fn import_npc_step(w: &mut World, i: usize, ram: &GbaRam, o: u32, row: Option<(i32, i32, i32, i32)>) {
    let st = ram.r32(o + 0x1B0);
    // (and the walk-grid wander, vt 0x0804D4E4 / update 0x0800A2A8: the same words, plus the footprint it left at
    // +0x28 and the one it walks to at +0x38, px - real run 4-1, West City's walkers: the r009_006 every-N tiles)
    // (the plain talk wander 0x0800A75B's object, vt 0x0804B974: the same words and footprints - MEASURED on the
    // whole-run cuts 885,660 .. 886,110, Nataday Village's walkers)
    let svt = if ram.valid(st) { ram.r32(st) } else { 0 };
    // a walk to a point (step 0x0800A1AF, object vt 0x0804B964: +4 / +8 24.8 velocity, +0xC / +0xE s16 target px,
    // +0x10 u16 frames left - MEASURED on whole-run cut 48,892, East District 3-3's two walkers): that step with the
    // same target, the RAM's velocity and count (a fresh create recomputes them from where the walker is now)
    // a wait (step 0x0800A223, object vt 0x0804D4D4: +4 the count left; the NPC's step index is obj +0x1B4) - whole-run
    // cut 59,619, Pilaf Castle 4-20's guard idle 93 frames before its walk
    if svt == 0x0804_D4D4 {
        let idx = ram.r8(o + 0x1B4) as usize;
        let Body::Npc(n) = &w.ents[i].body else { return };
        if !n.behaviour.as_ref().is_some_and(|b| b.steps.get(idx).is_some_and(|s| s.func == 0x0800_A223)) {
            return;
        }
        let rng = w.rng.clone();
        wf_core::fieldnpc::import_step(w, i, idx);
        w.rng = rng;
        let Body::Npc(n) = &mut w.ents[i].body else { return };
        // (the step's create turns the NPC its def's way; the RAM's facing stands - run-2 cut 32,138: s073's Olibu,
        // walked up by node 0 and waiting, facing 1 in RAM, the wait's create made it 2)
        n.a.facing = ram.r8(o + 0x10);
        if let Some(b) = n.behaviour.as_mut() {
            if let Some(wf_core::fieldnpc::StepRt::Wait { count }) = b.cur.as_mut() {
                *count = ram.s32(st + 4);
            }
            if let Some((_, raw, cel, tick)) = row {
                (b.raw, b.frame, b.tick) = (raw, cel, tick);
                b.anim = if raw == 1 { 0 } else { 1 };
            }
        }
        return;
    }
    // a face-the-player step (0x0800A8E9 idle / 0x0800A90F walk pose, object vt 0x0804B338: +4 the count left, one off
    // an update; the NPC's step index obj +0x1B4) - run 3 cut 30,383, 1-16's villager 19 frames into its 50: a fresh
    // create re-entered its idle (a rand(255) the game does not draw)
    if svt == 0x0804_B338 {
        let idx = ram.r8(o + 0x1B4) as usize;
        let Body::Npc(n) = &w.ents[i].body else { return };
        if !n.behaviour.as_ref().is_some_and(|b| b.steps.get(idx).is_some_and(|s| matches!(s.func, 0x0800_A8E9 | 0x0800_A90F))) {
            return;
        }
        let rng = w.rng.clone();
        wf_core::fieldnpc::import_step(w, i, idx);
        w.rng = rng;
        let Body::Npc(n) = &mut w.ents[i].body else { return };
        n.a.facing = ram.r8(o + 0x10);
        if let Some(b) = n.behaviour.as_mut() {
            if let Some(wf_core::fieldnpc::StepRt::FacePlayer { count }) = b.cur.as_mut() {
                *count = ram.s32(st + 4);
            }
            if let Some((_, raw, cel, tick)) = row {
                (b.raw, b.frame, b.tick) = (raw, cel, tick);
                b.anim = if raw == 1 { 0 } else { 1 };
            }
        }
        return;
    }
    if svt == 0x0804_B964 {
        let (tx, ty) = (ram.r16(st + 0xC) as i16 as i32, ram.r16(st + 0xE) as i16 as i32);
        let Body::Npc(n) = &w.ents[i].body else { return };
        let Some(idx) = n.behaviour.as_ref().and_then(|b| b.steps.iter().position(|s| s.func == 0x0800_A1AF && matches!(s.op, wf_core::fieldcontent::StepOp::WalkTo { x, y, .. } if x == tx && y == ty))) else { return };
        let rng = w.rng.clone();
        wf_core::fieldnpc::import_step(w, i, idx);
        w.rng = rng;
        let Body::Npc(n) = &mut w.ents[i].body else { return };
        if let Some(b) = n.behaviour.as_mut() {
            if let Some(wf_core::fieldnpc::StepRt::WalkTo { n: cnt, vx, vy, .. }) = b.cur.as_mut() {
                *cnt = ram.r16(st + 0x10) as i32;
                *vx = ram.s32(st + 4);
                *vy = ram.s32(st + 8);
            }
            if let Some((_, raw, cel, tick)) = row {
                (b.raw, b.frame, b.tick) = (raw, cel, tick);
                b.anim = if raw == 1 { 0 } else { 1 };
            }
        }
        return;
    }
    if svt != 0x0804_D848 && svt != 0x0804_D4E4 && svt != 0x0804_B974 {
        return;
    }
    let Body::Npc(n) = &w.ents[i].body else { return };
    let Some(idx) = n.behaviour.as_ref().and_then(|b| b.steps.iter().position(|s| matches!(s.func, 0x0800_A75B | 0x0800_A7A1 | 0x0800_A7DF | 0x0800_A811))) else { return };
    let rng = w.rng.clone();
    wf_core::fieldnpc::import_step(w, i, idx);
    w.rng = rng;
    // (a sprite whose walk (raw 2) is its idle's records - the souls, sprite 240: the row names the struct by its first
    // slot, raw 1 - walks in raw 2 while the wander moves: kami_yemma f0, a soul 6 ticks into its walk)
    let walk_aliased = match &w.ents[i].body { Body::Npc(n) => n.a.sprite, _ => None }.and_then(|s| w.data.sprite(s)).and_then(|sp| sp.anim_raw(2)).is_some_and(|a| a.canon_raw == 1);
    let moving = ram.s32(st + 8) > 0;
    let Body::Npc(n) = &mut w.ents[i].body else { return };
    let facing = n.a.facing;
    if let Some(b) = n.behaviour.as_mut() {
        if let Some(wf_core::fieldnpc::StepRt::Wander(s)) = b.cur.as_mut() {
            s.left = ram.s32(st + 8);
            s.vx = ram.s32(st + 0x18);
            s.vy = ram.s32(st + 0x1C);
            s.tx = ram.s32(st + 0x20);
            s.ty = ram.s32(st + 0x24);
            s.dir = facing;
            if svt == 0x0804_D4E4 || svt == 0x0804_B974 {
                s.old = [0, 4, 8, 12].map(|k| ram.s32(st + 0x28 + k));
                s.new = [0, 4, 8, 12].map(|k| ram.s32(st + 0x38 + k));
            }
        }
        if let Some((_, raw, cel, tick)) = row {
            let raw = if raw == 1 && walk_aliased && moving { 2 } else { raw };
            (b.raw, b.frame, b.tick) = (raw, cel, tick);
            b.anim = if raw == 1 { 0 } else { 1 };
        }
    }
}

/// the animated effects in flight (the generic effect `0x0802A81C`, vtable 0x0804CF3C, and the explosion's damaging
/// variant 0x0804A2C0: +0x10 / +0x14 position, +0x30 first record, +0x34 current record (12 bytes each), +0x38
/// accumulator, +0x3C records left, +0x40 rate) -> `Fx::Anim`, newest in the list like the game's (real run 14-2
/// r069_117: an explosion still playing as the pause menu closes frees its block 20 frames later)
fn import_effects(w: &mut World, ram: &GbaRam) {
    for o in ram.objects() {
        let vt = ram.r32(o);
        // a debris chip (vt 0x08049F38, `0x0800C912` / a bomb's burst): +0x18 the updates left, +0x1C / +0x20 the 24.8
        // position, +0x24 / +0x28 the velocity (each update: velocity, then position) - our chip with the same numbers,
        // its age 32 - left (exit-fade sweep 18-10: four chips of a statue's hit, their shared tiles foreign in ours)
        if vt == 0x0804_9F38 {
            let f = wf_core::fx::Fx::Debris { x: ram.s32(o + 0x1C), y: ram.s32(o + 0x20), vx: ram.s32(o + 0x24), vy: ram.s32(o + 0x28), t: 32 - ram.s32(o + 0x18) };
            w.add(Body::Fx(Box::new(f)));
            continue;
        }
        if vt != 0x0804_A2C0 && vt != 0x0804_CF3C {
            continue;
        }
        let (first, cur) = (ram.r32(o + 0x30), ram.r32(o + 0x34));
        let f = wf_core::fx::Fx::Anim { id: first, x: ram.s32(o + 0x10), y: ram.s32(o + 0x14), rec: (cur.wrapping_sub(first) / 12) as i32, left: ram.s32(o + 0x3C), rate: ram.s32(o + 0x40), acc: ram.s32(o + 0x38) };
        w.add(Body::Fx(Box::new(f)));
    }
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
    // a wander under way (vt 0x0804B318: the runner's current step; run 2 cut 1,254,433 - an enemy switching back to
    // its patrol state continued the wander, chance(2048); ours created a fresh step, its idle's rand(255) first)
    if ram.valid(st) && ram.r32(st) == 0x0804_B318 {
        let q = |k: u32| [0, 4, 8, 12].map(|d| ram.s32(st + k + d));
        wf_core::fieldpatrol::import_wander(w, ei, ram.s32(st + 8), (ram.s32(st + 0x18), ram.s32(st + 0x1C)), (ram.s32(st + 0x20), ram.s32(st + 0x24)), q(0x28), q(0x38));
        return;
    }
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
    build_mid_ids(data, ram, info, runner, rng0, frame0, 0, notes)
}

/// `build_mid`, the entities after the players numbered from `id_base` (the field a cutscene saved: its entities must
/// not share ids with the scene world's - `import_saved_field`)
#[allow(clippy::too_many_arguments)]
fn build_mid_ids(data: &Arc<GameData>, ram: &GbaRam, info: &CutInfo, runner: &mut StoryHost, rng0: [u32; 2], frame0: u32, id_base: u32, notes: &mut Vec<String>) -> Built {
    let story = import_story(data, ram);
    let (x, y, facing) = info.leader.unwrap_or((120, 120, 0));
    let to = info.area;
    let cfg = FieldConfig { world: to.world, area: to.area, variant: to.variant, pos: Some((x, y)), facing, chars: vec![story.current], story: Some(story.clone()), rng: wf_core::battle::RngInit::State(rng0[0], rng0[1]), xp: true, arrive: false, slow_fade_in: false };
    let mut w = field::create(data.clone(), &cfg);
    w.next_id = w.next_id.max(id_base);
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
    // the last area name shown (0x03001984, u16: kept across loads - a reload of the same name shows no banner; run 1
    // cut 955,163: the event reloads 6-1 after its scene map, Kami's Lookout already shown)
    w.scene.banner_last = ram.r16(0x0300_1984) as i64;
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
    import_player_state(&mut w, ram, notes);
    // what the RAM has that the import does not rebuild yet
    let known = [VT_PLAYER, VT_ENEMY, VT_NPC, VT_NPC_TALK, VT_BANNER];
    let other: std::collections::BTreeSet<u32> = ram.objects().into_iter().map(|o| ram.r32(o)).filter(|v| !known.contains(v)).collect();
    if !other.is_empty() {
        notes.push(format!("mid-area: objects of classes {:x?} are not imported", other));
    }
    // the field's mode with the screen still black and nothing running: a pause menu's return (the world restored,
    // the palette fade-in to come - 16 steps; the objects hold still, the cameras ease: menu `Phase::FadeIn`)
    // (the fade itself is the main-loop mode vt 0x0804A4B8: +4 the level reached, +8 the step)
    let mode = ram.r32(0x0300_0F90);
    let fading = (ram.valid(mode) && ram.r32(mode) == 0x0804_A4B8).then(|| ram.s32(mode + 4));
    if let Some(f) = fading {
        let level = (256 - f).clamp(1, 256) as u16;
        w.menu = wf_core::menu::Menu { phase: wf_core::menu::Phase::FadeIn { level }, tr: wf_core::menu::PAUSE, ..Default::default() };
        notes.push(format!("mid-area: a menu's return (fading in, level {level})"));
    } else if ram.screen_black == Some(true) {
        // the screen still black, no fade mode yet: the cut stands inside the close's world-restore iteration (its
        // long lag; run 3's pause spins, cuts 8,548 .. 146,704: 7 .. 54 black frames, then the first fade step). Our
        // close's last leave step (`Phase::Leave`, no-lag model: the exit stub's iteration then the restore's, `tr.leave`
        // 2) - black until it ends, then the fade-in from 256 - 16. The restore ends with a camera update the game shows
        // in its last black frame; our model makes it in the exit stub's step, which the cut is past: made here
        let tr = wf_core::menu::Transition { leave: 2, ..wf_core::menu::PAUSE };
        w.menu = wf_core::menu::Menu { phase: wf_core::menu::Phase::Leave { t: 1 }, tr, reload_close: true, ..Default::default() };
        if w.field.is_some() {
            w.tick_shakes();
            wf_core::field::world_mode_cameras(&mut w);
        }
        notes.push("mid-area: a menu close's world-restore iteration (black, the fade-in next)".into());
    }
    // the pause menu's fade-out (START pressed: the main loop's mode is the base "field drawn, not updated" vt
    // 0x0804A920 itself, +4 the brightness left, from 256 by +8 = 16 a frame; the Status page next - whole-run cut
    // 967,072, the world frozen with the leader hurt)
    if ram.valid(mode) && ram.r32(mode) == 0x0804_A920 && ram.s32(mode + 8) == 16 && fading.is_none() && w.data.menu.is_some() {
        let level = (256 - ram.s32(mode + 4)).clamp(0, 255) as u16;
        let page = wf_core::menu::status::Status::new(&w);
        wf_core::menu::open(&mut w, wf_core::menu::Screen::Status(page), wf_core::menu::PAUSE, false);
        w.menu.phase = wf_core::menu::Phase::FadeOut { level };
        w.menu.held = ram.r16(0x0300_3300 + 2) as u16;
        notes.push(format!("mid-area: the pause menu's fade-out (level {level})"));
    }
    // an exit's fade-out under way (the main-loop mode vt 0x0804BB74, ctor 0x08006D98: +4 the brightness from 256, +8
    // its step, +0xC the player): the exit the leader took (the load reads its def at the end, nothing in RAM says
    // which) is ours whose rect held the leader where the fade began - autowalked k frames back, 0x80 a frame along
    // the facing - and the fade k frames in (whole-run cut 886,110: Nataday Village's door, 18 frames in)
    if ram.valid(mode) && ram.r32(mode) == MODE_FADE_OUT && ram.r32(mode + 0xC) == ram.player() && ram.s32(mode + 8) == 8 {
        import_exit_fade(&mut w, ram, ram.s32(mode + 4), notes);
    }
    // an exit arrival's fade-in under way (the main-loop mode vt 0x0804B57C: +4 the brightness up from 0, +8 its step 8;
    // the leader autowalking in): our arrival fade k frames in - no trigger check until it ends (whole-run cuts 48,862
    // / 94,050: 4 and 15 frames in, ours fired the exit the leader stands on)
    if ram.valid(mode) && ram.r32(mode) == MODE_FADE_IN && ram.s32(mode + 8) == 8 && ram.s32(mode + 4) > 0 && autowalk(ram, ram.player()) {
        let k = ram.s32(mode + 4) / 8;
        if let Some(f) = w.field.as_mut().filter(|f| matches!(f.mode, Mode::Normal)) {
            f.mode = Mode::FadeIn { k };
            notes.push(format!("mid-area: an exit arrival's fade-in, {k} frames in"));
        }
    }
    // an inline script (an area's / a map event's, no event graph) running at the cut outside any event: its
    // interpreter's PC on the context stack lies in its code (r002_006 +20,026: the world map's landing script
    // 0x08073424 for Babidi's ship, stopped in its camera_pan_to after the warp, the screen still black)
    if info.depth == 0 {
        if let Some((id, ran)) = inline_running(ram) {
            // run from inside the frame's object pass (an on-defeat hook: the world list's delayed update call
            // 0x08011A5E under the VM): its end completes that pass (`held_event` -> `resume_held`; castle_ship_28,
            // the 8-27 boss's defeat script 0x0807CDAC: camera_follow, then the pass with the player's walk)
            if (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0801_1A5F) {
                if let Some(f) = w.field.as_mut() {
                    f.held_event = true;
                }
            }
            let mut drop = ran.len().saturating_sub(1);
            // (waiting at end_parallel (0x3A): the group replays from its begin_parallel (0x38), its running commands
            // then take their objects' progress - `import_track_objects`, as `build_mid_code`'s)
            if ran.get(drop).is_some_and(|r| r.0 == 0x3A) {
                if let Some(b) = ran[..drop].iter().rposition(|r| r.0 == 0x38) {
                    drop = b;
                }
            }
            // (the static position counts each action once: a loop or a step split before it ran some several times -
            // the dynamic index is the n-th queued occurrence of that native in a probe run)
            if let Some((tn, targs)) = ran.get(drop).cloned() {
                let nth = ran[..=drop].iter().filter(|(n, a)| *n == tn && (targs.is_none() || *a == targs)).count();
                // (every queued action of the probe run, in order - the predicate sees each once)
                let seen: std::cell::RefCell<Vec<(u8, Vec<i32>)>> = Default::default();
                runner.probe_inline(&mut w, &id, &|n, args| {
                    seen.borrow_mut().push((n, args.to_vec()));
                    false
                });
                let seen = seen.into_inner();
                let cands: Vec<usize> = (0..seen.len()).filter(|&k| seen[k].0 == tn && targs.as_ref().is_none_or(|t| *t == seen[k].1)).collect();
                // (a computed argument - a loop's `set_actor_byte8(g(0), 36)`, g(0) counting down: run 2 cut 763,358 waits in
                // the pass with g(0) = -3, RAM's word - picks the pass: the occurrence whose first argument is the RAM's
                // script word, when exactly one is)
                let ram_g: Vec<i32> = (0..4u32).map(|k| ram.s32(WORDS_G + 4 * k)).collect();
                let by_word: Vec<usize> = if targs.is_none() && cands.len() > 1 {
                    cands.iter().copied().filter(|&k| seen[k].1.first().is_some_and(|a| ram_g.contains(a))).collect()
                } else {
                    vec![]
                };
                if let Some(&k) = (by_word.len() == 1).then(|| &by_word[0]).or_else(|| cands.get(nth.saturating_sub(1))) {
                    // (an instant native the interpreter stands after has run: the game's frame break follows it - the
                    // lift script's pass of g(0) = -3 is done at cut 763,358, the glide starts 5 rows later, as from
                    // an earlier cut)
                    drop = if wf_script::steps::is_instant(tn) { k + 1 } else { k };
                }
            }
            w.menu = Default::default();
            if ram.screen_black == Some(true) {
                w.scene.screen.black = 16;
                w.scene.screen.dim = 256;
            }
            // (a breakable's on-break callback blocking does not hold the field, as `Cause::Break` in play)
            let on_break = w.field.as_ref().and_then(|f| w.data.field.as_ref()?.area(f.area.world, f.area.area)).is_some_and(|a| {
                a.objects.iter().any(|o| matches!(&o.kind, wf_core::fieldcontent::ObjKind::Breakable { on_break: Some(wf_core::fieldcontent::Callback::Script(s)), .. } if *s == id))
            });
            runner.continue_inline_held(&mut w, &id, drop, !on_break);
            fade_progress_from_ram(&mut w, ram, ran[ran.len() - 1].0);
            // its tracks' running walks / glides from their game objects, as an event's (run 2 cuts 763,328 ..
            // 763,606: 12-26's lift script inline_0x08085224 - walk_to, then glide_to in a parallel group)
            let rng = w.rng.clone();
            import_track_objects(&mut w, ram);
            w.rng = rng;
            track_anims_from_rows(&mut w, ram);
            import_cam_pan(&mut w, ram);
            notes.push(format!("mid-area: inline script {id} at its action #{drop} (static #{}) {:#04x}", ran.len() - 1, ran[ran.len() - 1].0));
        }
    }
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held: 0, finished: true }
}

/// the track actors' animations (and facing) as the RAM rows show them (a started command restarted its animation):
/// the RAM object at the entity's 24.8 position (`build_mid_code`; an inline script's tracks too)
/// every NPC's animation clock as its RAM row (by position and sprite) when the row shows the same animation
fn npc_anims_from_rows(w: &mut World, ram: &GbaRam) {
    for &(o, sprite, raw, cel, tick) in ram.actor_rows.iter() {
        let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        let Some(i) = (0..w.ents.len()).find(|&i| !w.ents[i].removed && matches!(w.ents[i].body, Body::Npc(_)) && w.actor(i).is_some_and(|a| a.x == x && a.y == y) && wf_core::scene::sprite_id(w, i) == Some(sprite)) else { continue };
        if let Body::Npc(n) = &mut w.ents[i].body {
            if n.anim.raw == raw {
                (n.anim.frame, n.anim.tick) = (cel, tick);
            }
        }
    }
}

fn track_anims_from_rows(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::Owner;
    let rows: std::collections::BTreeMap<u32, (i32, i32, i32, i32)> = ram.actor_rows.iter().map(|r| (r.0, (r.1, r.2, r.3, r.4))).collect();
    // (and the event's puppets without a track left: an actor whose walk ended - whole-run cut 146,883, s237's Baba)
    let mut owners: Vec<wf_core::world::EntityId> = w
        .scene
        .tracks
        .iter()
        .filter_map(|t| match t.owner {
            Owner::Entity(id) => Some(id),
            _ => None,
        })
        .collect();
    for p in &w.scene.puppets {
        if !owners.contains(&p.id) {
            owners.push(p.id);
        }
    }
    // (and the NPCs whose animation command the replay dropped or ended: run 3 cut 135,588, s453's set_anim(260, 17)
    // before the walk_to it waits in; 220,149, s303's walk_to(18, 390, 242) over - ours had restarted its walk)
    for e in w.ents.iter().filter(|e| !e.removed) {
        if matches!(&e.body, Body::Npc(_)) && !owners.contains(&e.id) {
            owners.push(e.id);
        }
    }
    for id in owners {
        let Some(i) = w.find(id) else { continue };
        let Some((x, y)) = w.actor(i).map(|a| (a.x, a.y)) else { continue };
        // (by position, and by sprite when two stand on one spot - run 2 cut 57,951: s103's Goku and Pikkon spawned at
        // (0, -8))
        let spr = wf_core::scene::sprite_id(w, i);
        let at = |o: &u32| ram.s32(o + 0x1A0) == x && ram.s32(o + 0x1A4) == y;
        let o = if Some(i) == w.player_idx(0) {
            Some(ram.player())
        } else {
            rows.iter().find(|(o, r)| at(o) && Some(r.0) == spr).map(|(o, _)| *o).or_else(|| rows.keys().copied().find(|o| at(o)))
        };
        // (and the facing: a walk the replay found already ended is dropped unstarted, so its turn is not made -
        // whole-run cut 146,883: s237's Baba (222) at the end of her walk up, facing 1 in RAM)
        if let Some(o) = o {
            let f = ram.r8(o + 0x10) & 3;
            match &mut w.ents[i].body {
                wf_core::world::Body::Player(p) => p.a.facing = f,
                wf_core::world::Body::Npc(n) => n.a.facing = f,
                wf_core::world::Body::Enemy(e) => e.a.facing = f,
                _ => {}
            }
        }
        let Some(&(_, raw, cel, tick)) = o.and_then(|o| rows.get(&o)) else { continue };
        let d = Arc::clone(&w.data);
        // (the state's script anim id follows the RAM's state - raw = anim_state(id) + 1 - when ours is another
        // state's: it picks how the state ticks; whole-run cut 146,883: Baba idle in RAM, ours still the walk's id)
        let id_for = |cur: i32| -> i32 {
            if wf_core::scene::anim_state(cur) + 1 == raw {
                cur
            } else {
                (0..25).find(|&a| wf_core::scene::anim_state(a) + 1 == raw).unwrap_or(cur)
            }
        };
        if let Some(pp) = w.scene.puppets.iter_mut().find(|p| p.id == id) {
            (pp.anim.raw, pp.anim.frame, pp.anim.tick) = (raw, cel, tick);
            pp.anim.id = id_for(pp.anim.id);
        }
        match &mut w.ents[i].body {
            wf_core::world::Body::Player(p) => {
                (p.ap.raw, p.ap.frame, p.ap.tick) = (raw, cel, tick);
            }
            wf_core::world::Body::Npc(n) => {
                (n.anim.raw, n.anim.frame, n.anim.tick) = (raw, cel, tick);
                n.anim.id = id_for(n.anim.id);
                // (a map NPC showing a script animation - not its behaviour's idle / walk - is under the script's
                // control as `set_state` puts it: a puppet, the animation the state's own; run 3 cut 137,176, s058's
                // set_anim(163, 12) 27 ticks in, the replay from its actor_wait - ours ran the behaviour's walk tick)
                if n.behaviour.is_some() && raw > 2 {
                    n.anim.mode = wf_core::scene::AnimMode::Table;
                    if !w.scene.puppets.iter().any(|p| p.id == id) {
                        w.scene.puppets.push(wf_core::scene::Puppet { id, anim: wf_core::scene::ActAnim::idle() });
                    }
                }
                // (its behaviour's idle / walk on the RAM's clock: the behaviour's animation is what it shows)
                if let Some(b) = n.behaviour.as_mut().filter(|_| raw <= 2) {
                    (b.raw, b.frame, b.tick) = (raw, cel, tick);
                    b.anim = if raw == 1 { 0 } else { 1 };
                }
            }
            wf_core::world::Body::Enemy(e) => {
                e.set_raw(&d, raw);
                (e.ea.pos.raw, e.ea.pos.frame, e.ea.pos.tick) = (raw, cel, tick);
            }
            _ => {}
        }
    }
    // a script's frame-range controller running an actor - its state +0x14 vt 0x0804DE58: +4 pass, +8 ticks a cel, +0xC
    // passes, +0x10 loop start, +0x14 last cel (as the leader's, `import_leader`): the NPC / puppet runs it on the RAM's
    // cel and tick (run 1 cut 1,639,933, s463's three actors looping cels 2..3 at 5 ticks a cel; ours ran the table's
    // tick and showed cel 2)
    for (&o, &(sprite, raw, cel, tick)) in rows.iter() {
        let st = ram.r32(o + 0x14);
        if o == ram.player() || !ram.valid(st) || ram.r32(st) != 0x0804_DE58 {
            continue;
        }
        let mode = wf_core::scene::AnimMode::Frames { each: ram.s32(st + 8), loops: ram.s32(st + 0xC), loop_start: ram.s32(st + 0x10), last: ram.s32(st + 0x14), pass: ram.s32(st + 4) };
        let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        let Some(i) = (0..w.ents.len()).find(|&i| !w.ents[i].removed && w.actor(i).is_some_and(|a| a.x == x && a.y == y) && wf_core::scene::sprite_id(w, i) == Some(sprite)) else { continue };
        let id = w.ents[i].id;
        let set = |an: &mut wf_core::scene::ActAnim| {
            (an.raw, an.frame, an.tick) = (raw, cel, tick);
            an.mode = mode.clone();
            an.ended = false;
            an.fresh = false;
        };
        if let Some(pp) = w.scene.puppets.iter_mut().find(|p| p.id == id) {
            set(&mut pp.anim);
        } else if let wf_core::world::Body::Npc(n) = &mut w.ents[i].body {
            set(&mut n.anim);
        } else if matches!(w.ents[i].body, wf_core::world::Body::Enemy(_)) {
            // (an enemy-bodied actor the replay's `play_anim_frames` never reached: its puppet, or its AI animation
            // runs on and its script draws - run 3 cut 1,071,990, s344 node 65's Babidi looping cels 2..4)
            let aid = (0..25).find(|&a| wf_core::scene::anim_state(a) + 1 == raw).unwrap_or(0);
            let mut anim = wf_core::scene::ActAnim::of(aid);
            set(&mut anim);
            w.scene.puppets.push(wf_core::scene::Puppet { id, anim });
        }
    }
}

/// each track's running command from its game object (`build_mid_code`; an inline script's tracks too): walks, glides,
/// jumps, `actor_wait`s - the commands before it dropped, the object's progress taken
fn import_track_objects(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::{Cmd, Owner};
    let walks = ram.ewram_with(0x0804_D0A4);
    // `walk_to_actor`'s walk (vt 0x0804D4BC, the same words as a walk: +0xC / +0x10 step, +0x14 / +0x18 the target px it
    // computed at its start, +0x1C steps left)
    let walks_to = ram.ewram_with(0x0804_D4BC);
    let glides = ram.ewram_with(0x0804_AE84);
    let jumps = ram.ewram_with(0x0804_AE5C);
    // the actors' running `actor_wait` objects (an actor's state +0x18C)
    let waits: Vec<u32> = ram.objects().into_iter().map(|a| ram.r32(a + 0x18C)).filter(|&s| ram.valid(s) && ram.r32(s) == 0x0804_AB58).collect();
    for k in 0..w.scene.tracks.len() {
        // (several walks to one target - a crowd walking to the same spot, run 3 cut 58,967, s159 node 42: each its own
        // step - the one whose walker stands nearest this track's actor; the walker is the object whose state +0x18C is
        // the walk)
        let me_at = match w.scene.tracks[k].owner {
            Owner::Entity(id) => w.find(id).and_then(|i| w.actor(i)).map(|a| (a.x, a.y)),
            _ => None,
        };
        // (and only a walker of this track's character: the same sprite as the row's - another character's walk to the
        // same spot is not this one's command)
        let me_sprite = match w.scene.tracks[k].owner {
            Owner::Entity(id) => w.find(id).and_then(|i| wf_core::scene::sprite_id(w, i)),
            _ => None,
        };
        let walker_ok = |o: u32| -> bool {
            let Some(sp) = me_sprite else { return true };
            ram.objects().into_iter().find(|&a| ram.r32(a + 0x18C) == o).and_then(|a| ram.actor_rows.iter().find(|r| r.0 == a)).is_none_or(|r| r.1 == sp)
        };
        let walker_dist = |o: u32| -> i64 {
            let Some((mx, my)) = me_at else { return 0 };
            ram.objects().into_iter().find(|&a| ram.r32(a + 0x18C) == o).map(|a| ((ram.s32(a + 0x1A0) - mx) as i64).abs() + ((ram.s32(a + 0x1A4) - my) as i64).abs()).unwrap_or(i64::MAX)
        };
        let live = |c: &Cmd| -> Option<u32> {
            match c {
                Cmd::MoveTo { target: Some(_), .. } => walks_to.iter().copied().filter(|&o| walker_ok(o)).min_by_key(|&o| walker_dist(o)),
                Cmd::MoveTo { x, y, .. } => walks.iter().copied().filter(|&o| ram.s32(o + 0x14) == *x && ram.s32(o + 0x18) == *y && walker_ok(o)).min_by_key(|&o| walker_dist(o)),
                Cmd::GlideTo { x, y, .. } => glides.iter().copied().find(|&o| ram.s32(o + 0x14) == *x && ram.s32(o + 0x18) == *y),
                Cmd::JumpTo { x, y, .. } => jumps.iter().copied().find(|&o| ram.s32(o + 0x18) == *x && ram.s32(o + 0x1C) == *y),
                _ => None,
            }
        };
        // (a target the script computed from a query before the cut - glide_to(0, actor_x(0), actor_y(0) - 100):
        // asked again now it is another; the head's command with the only live object of its kind takes that
        // object's target - real route 4-1, s515's take-off glide, tiles r002_006+2000 / +13000)
        // (or the actor's own movement object: its state +0x18C - two glides at once, each target re-asked: run 2
        // cut 57,741, s103 node 0 lifts Goku and Pikkon together)
        let own = match w.scene.tracks[k].owner {
            Owner::Entity(id) => w.find(id).and_then(|i| {
                let a = w.actor(i)?;
                let o = if Some(i) == w.player_idx(0) { ram.player() } else { ram.objects().into_iter().find(|&o| ram.s32(o + 0x1A0) == a.x && ram.s32(o + 0x1A4) == a.y)? };
                (o != 0).then(|| ram.r32(o + 0x18C))
            }),
            _ => None,
        };
        let found = w.scene.tracks[k].queue.iter().enumerate().find_map(|(j, c)| live(c).map(|o| (j, o))).or_else(|| {
            let only = |v: &Vec<u32>| (v.len() == 1).then(|| v[0]);
            match w.scene.tracks[k].queue.first() {
                Some(Cmd::GlideTo { .. }) => only(&glides).or_else(|| own.filter(|s| glides.contains(s))).map(|o| (0, o)),
                // (the glide behind the group's instant commands - set_pose / set_attachment, done in the replay's
                // first step: the actor's own glide object says it runs)
                _ => own.filter(|s| glides.contains(s)).and_then(|o| {
                    let j = w.scene.tracks[k].queue.iter().position(|c| matches!(c, Cmd::GlideTo { .. }))?;
                    w.scene.tracks[k].queue[..j].iter().all(|c| matches!(c, Cmd::SetPose { .. } | Cmd::SetAttachment { .. } | Cmd::Face { .. })).then_some((j, o))
                }),
            }
        });
        // a glide at the head with no glide object left in RAM has ended (the object goes in its last step; the
        // script resumes a frame later): done, not glided again from here (tile r002_006+13000)
        // (and a jump: the jump object goes at its landing - run 2 cut 1,923,219, s459 node 0: Vegeta's jump_to over
        // and the group's end_parallel waiting; ours jumped again)
        if found.is_none() && jumps.is_empty() && matches!(w.scene.tracks[k].queue.first(), Some(Cmd::JumpTo { .. })) {
            w.scene.tracks[k].queue.remove(0);
            w.scene.tracks[k].started = false;
            continue;
        }
        if found.is_none() && glides.is_empty() && matches!(w.scene.tracks[k].queue.first(), Some(Cmd::GlideTo { .. })) {
            w.scene.tracks[k].queue.remove(0);
            w.scene.tracks[k].started = false;
            continue;
        }
        // so does a walk at the head with no walk object for it and its actor on the target: the walk object went in
        // its last step (its idle SetAnim drawn), the script goes on in the cut's frame (whole-run cut 887,156:
        // s437 node 10, the priest's walk back ended, `remove_actor` next)
        // (an `actor_wait` before it ended too: no wait object is left, the walk's object is made by the wait - whole-run
        // cut 146,883: s237's actor_wait(269, 15) + walk_to(269, 210, 63) both over)
        if found.is_none() {
            let skip = w.scene.tracks[k].queue.iter().take_while(|c| matches!(c, Cmd::Wait { .. })).count();
            if let (Some(Cmd::MoveTo { x, y, .. }), Owner::Entity(id)) = (w.scene.tracks[k].queue.get(skip), w.scene.tracks[k].owner) {
                let (x, y) = (*x, *y);
                let at = w.find(id).and_then(|i| w.actor(i)).is_some_and(|a| a.x == x << 8 && a.y == y << 8);
                if at && !walks.iter().any(|&o| ram.s32(o + 0x14) == x && ram.s32(o + 0x18) == y) {
                    w.scene.tracks[k].queue.drain(..=skip);
                    w.scene.tracks[k].started = false;
                    continue;
                }
            }
        }
        // a lone `actor_wait` at the head (0x4A, no walk after it: the wait object vt 0x0804AB58 at the actor's state
        // +0x18C, +4 = 0, +8 the frames left): that count (run 3 cuts 122,487 / 215,385: 1 frame left); no wait object
        // left - it went in its last step, the script goes on in the cut's frame: over (run 3 cut 88,742, s003's
        // level-up spin: the wait done, face(0, 0) in f0)
        if found.is_none() && matches!(w.scene.tracks[k].queue.first(), Some(Cmd::Wait { .. })) {
            if let Some(s) = own {
                if ram.valid(s) && ram.r32(s) == 0x0804_AB58 {
                    if ram.r32(s + 4) == 0 {
                        if let Some(Cmd::Wait { left }) = w.scene.tracks[k].queue.first_mut() {
                            *left = ram.s32(s + 8);
                        }
                        w.scene.tracks[k].started = true;
                    }
                } else if s == 0 {
                    w.scene.tracks[k].queue.remove(0);
                    w.scene.tracks[k].started = false;
                }
                continue;
            }
        }
        // an `actor_wait` at the head with no walk made behind it (the actor's own state is the wait object, its +4
        // empty): the wait runs with the RAM's frames left (run 3 cut 68,300: s194 node 6's actor_wait(41, 60), 30
        // left - ours restarted it, the walk after it a frame late)
        if let (Some(Cmd::Wait { .. }), Some(s)) = (w.scene.tracks[k].queue.first(), own.filter(|s| waits.contains(s) && ram.r32(s + 4) == 0)) {
            let left = ram.s32(s + 8);
            if let Some(Cmd::Wait { left: l }) = w.scene.tracks[k].queue.first_mut() {
                *l = left;
            }
            w.scene.tracks[k].started = true;
            continue;
        }
        let Some((pos, o)) = found else { continue };
        // `actor_wait` before the walk (0x4A: the actor's state +0x18C is the wait object vt 0x0804AB58, +4 the walk
        // object it starts at the end - already made, not stepped - +8 the frames left): the wait runs, the walk is
        // the command after it (whole-run cut 142,491: s172 node 0, Vegeta's actor_wait(279, 20) 1 frame in)
        if let Some(wo) = waits.iter().copied().find(|&a| ram.r32(a + 4) == o) {
            if pos > 0 && matches!(w.scene.tracks[k].queue.get(pos - 1), Some(Cmd::Wait { .. })) {
                w.scene.tracks[k].queue.drain(..pos - 1);
                if let Some(Cmd::Wait { left }) = w.scene.tracks[k].queue.first_mut() {
                    *left = ram.s32(wo + 8);
                }
                w.scene.tracks[k].started = true;
            }
            continue;
        }
        if let Some(Cmd::GlideTo { x, y, .. }) = w.scene.tracks[k].queue.get_mut(pos) {
            (*x, *y) = (ram.s32(o + 0x14), ram.s32(o + 0x18));
        }
        let walk_to_actor = matches!(w.scene.tracks[k].queue.get(pos), Some(Cmd::MoveTo { target: Some(_), .. })) && walks_to.contains(&o);
        // the moving actor stands where RAM has it (the object whose state +0x18C is this movement object): the replay
        // placed it by its own commands (run 3 cut 36,338, s101 node 2: four walkers 10 steps from their targets,
        // ours a walk behind)
        // (the same character: its sprite the row's - a track matched by target alone can hold another actor's object)
        if let (Owner::Entity(id), Some(a)) = (w.scene.tracks[k].owner, ram.objects().into_iter().find(|&a| ram.r32(a + 0x18C) == o)) {
            let row_sprite = ram.actor_rows.iter().find(|r| r.0 == a).map(|r| r.1);
            if let Some(i) = w.find(id).filter(|&i| row_sprite.is_some() && wf_core::scene::sprite_id(w, i) == row_sprite) {
                let (ax, ay) = (ram.s32(a + 0x1A0), ram.s32(a + 0x1A4));
                match &mut w.ents[i].body {
                    Body::Player(p) => (p.a.x, p.a.y) = (ax, ay),
                    Body::Enemy(e) => (e.a.x, e.a.y) = (ax, ay),
                    Body::Npc(n) => (n.a.x, n.a.y) = (ax, ay),
                    _ => {}
                }
            }
        }
        if pos > 0 || !w.scene.tracks[k].started {
            w.scene.tracks[k].queue.drain(..pos);
            wf_core::scene::import_start_head(w, k);
        }
        let Some(c) = w.scene.tracks[k].queue.first_mut() else { continue };
        match c {
            // (walk_to_actor: the target it computed at its start, not one from where the other actor is now - run 3
            // cuts 47,851 / 178,464, s110 node 1 / s274 node 4)
            Cmd::MoveTo { x, y, n, t, sx, sy, .. } if walk_to_actor => {
                (*x, *y) = (ram.s32(o + 0x14), ram.s32(o + 0x18));
                (*sx, *sy, *n, *t) = (ram.s32(o + 0xC), ram.s32(o + 0x10), ram.s32(o + 0x1C), 0);
            }
            Cmd::MoveTo { n, t, sx, sy, .. } | Cmd::GlideTo { n, t, sx, sy, .. } => {
                (*sx, *sy, *n, *t) = (ram.s32(o + 0xC), ram.s32(o + 0x10), ram.s32(o + 0x1C), 0);
            }
            Cmd::JumpTo { t, sx, vy, .. } => {
                (*t, *sx, *vy) = (ram.s32(o + 0x3C), ram.s32(o + 8), ram.s32(o + 0xC));
            }
            _ => {}
        }
    }
}

/// the exit fade-out at brightness `level` (see `build_mid`): our exit trigger and the fade's frame count
fn import_exit_fade(w: &mut World, ram: &GbaRam, level: i32, notes: &mut Vec<String>) {
    use wf_core::fieldcontent::{ObjKind, TriggerKind};
    let k = (256 - level) / 8;
    let facing = ram.r8(ram.player() + 0x10) & 3;
    let (dx, dy) = wf_core::geom::DIR_VEC[facing as usize];
    let (x0, y0) = ((ram.s32(ram.player() + 0x1A0) - dx * 0x80 * k) >> 8, (ram.s32(ram.player() + 0x1A4) - dy * 0x80 * k) >> 8);
    let Some(f) = w.field.as_ref() else { return };
    let hit = f.triggers.iter().find_map(|t| {
        let d = field::trigger_def(w, t.obj)?;
        let ObjKind::Trigger { kind: TriggerKind::Exit { to, push } | TriggerKind::EdgeExit { to, push }, .. } = d.kind else { return None };
        let edge = matches!(d.kind, ObjKind::Trigger { kind: TriggerKind::EdgeExit { .. }, .. });
        (x0 >= t.rect.x0 as i32 && x0 < t.rect.x1 as i32 && y0 >= t.rect.y0 as i32 && y0 < t.rect.y1 as i32).then_some((to, push, t.rect, edge))
    });
    let Some((to, push, rect, edge)) = hit else {
        notes.push(format!("mid-area: an exit's fade-out ({k} frames in) with no exit of ours at ({x0}, {y0})"));
        return;
    };
    let (x, y) = if edge || to.relative { (x0 - rect.x0 as i32 + to.x, y0 - rect.y0 as i32 + to.y) } else { (to.x, to.y) };
    let facing = if edge { push } else { to.facing.unwrap_or(push) };
    if let Some(f) = w.field.as_mut() {
        f.mode = Mode::FadeOut { k, to: AreaId { world: to.world, area: to.area, variant: to.variant }, x, y, facing: Some(facing), step: 0 };
    }
    notes.push(format!("mid-area: an exit's fade-out, {k} frames in, to {}-{}", to.world, to.area));
}

/// a camera pan in progress (`camera_pan_to`, object vt 0x0804B8C0: +4 / +8 the target px, +0xC speed, +0x10 /
/// +0x14 the step in 1/1024 px, +0x20 the steps left): the track's replayed pan takes the RAM's step and count, the
/// camera the RAM's position (the replay's start re-ran the pan's first step)
fn import_cam_pan(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::Cmd;
    let Some(o) = ram.ewram_with(0x0804_B8C0).into_iter().find(|&o| ram.s32(o + 0xC) > 0 && ram.s32(o + 0x20) >= 0) else { return };
    let (px, py) = (ram.s32(o + 4), ram.s32(o + 8));
    let mut hit = false;
    for t in w.scene.tracks.iter_mut() {
        if let Some(Cmd::CamPanTo { x, y, n, t: tt, sx, sy, .. }) = t.queue.first_mut() {
            if (*x, *y) == (px, py) {
                (*sx, *sy, *n, *tt) = (ram.s32(o + 0x10), ram.s32(o + 0x14), ram.s32(o + 0x20), 0);
                t.started = true;
                hit = true;
            }
        }
    }
    if hit {
        wf_core::battle::set_camera(w, 0, ram.s32(CAM), ram.s32(CAM + 4));
    }
}

/// emote bubbles showing at the cut (`emote`, object vt 0x0804A298, ctor 0x08016304: +4 / +8 the bubble's px, +0xC the
/// tile handle (0x40 bytes), +0x18 the age, +0x1C the emote + 1, +0x24 the actor object; freed when the age reaches
/// the emote's length - `scene::emote_len`): our emote visual with the frames left (a cut inside a cutscene map at 3-x,
/// run 1 cut 447,123: the bubble 13 frames old, freed at f11 - ours had none, its block stayed foreign)
fn import_emotes(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::{Visual, VisualKind};
    for o in ram.objects().into_iter().filter(|&o| ram.r32(o) == 0x0804_A298) {
        // (+0x1C the bubble's cel count = its tile bytes / 0x40; 8 frames a cel: the emote whose length that is)
        let cels = ram.s32(o + 0x1C);
        let emote = (0..5).find(|&e| wf_core::scene::emote_len(e) == cels * 8).unwrap_or(0);
        let age = ram.s32(o + 0x18);
        let life = cels * 8 - age;
        if life <= 0 {
            continue;
        }
        let a = ram.r32(o + 0x24);
        let owner = ram
            .valid(a)
            .then(|| (ram.s32(a + 0x1A0), ram.s32(a + 0x1A4)))
            .and_then(|(x, y)| (0..w.ents.len()).find(|&i| !w.ents[i].removed && w.actor(i).is_some_and(|ac| ac.x == x && ac.y == y)))
            .map(|i| w.ents[i].id)
            .unwrap_or(wf_core::world::EntityId::MAX);
        let vid = w.next_serial();
        w.scene.visuals.push(Visual { kind: VisualKind::Emote(emote), x: ram.s32(o + 4), y: ram.s32(o + 8), owner, t: age, life, vid });
    }
}

/// a `screen_flash` the replay restarted: the actor's command object (its +0x18C -> vt 0x0804C800: +8 / +0xC / +0x10
/// the colour, +0x14 the length, +0x18 the frames left, counted down each update; the command ends on the update after
/// 1) takes the RAM's progress, and the palette flash with it (run 3 cut 1,071,990: s344 node 65's
/// `screen_flash(169, 255, 255, 255, 15)` 3 frames from its end - ours restarted it, the next `shake_screen` 12 frames
/// late)
fn import_flash_cmds(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::Cmd;
    let cmds: Vec<u32> = ram
        .objects()
        .into_iter()
        .map(|o| ram.r32(o + 0x18C))
        .filter(|&c| ram.valid(c) && ram.r32(c) == 0x0804_C800)
        .collect();
    for c in cmds {
        let (len, left) = (ram.s32(c + 0x14), ram.s32(c + 0x18));
        for t in w.scene.tracks.iter_mut() {
            if let Some(Cmd::ScreenFlash { frames, t: tt, .. }) = t.queue.first_mut() {
                if *frames == len && left > 0 && left <= len {
                    *tt = len - left;
                    t.started = true;
                    if let Some(f) = w.scene.screen.flash.as_mut() {
                        f.t = len - left;
                    }
                }
            }
        }
    }
}

/// an `emote(.., wait)` the replay restarted: its count is the bubble's age (vt 0x0804A298 +0x18, the actor at +0x24),
/// and the bubble the replay made again goes - the RAM's one stays (`import_emotes`; run 1 cut 447,123: s319 node 25
/// waiting in emote 2, 13 frames in)
fn import_emote_cmds(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::{Cmd, Owner, VisualKind};
    let objs: Vec<u32> = ram.objects().into_iter().filter(|&o| ram.r32(o) == 0x0804_A298).collect();
    for k in 0..w.scene.tracks.len() {
        let Owner::Entity(id) = w.scene.tracks[k].owner else { continue };
        let Some(i) = w.find(id) else { continue };
        let Some((ax, ay)) = w.actor(i).map(|a| (a.x, a.y)) else { continue };
        let _ = (ax, ay);
        // (one bubble showing: it is this emote's - the object's +0x24 is not the actor)
        let Some(&o) = (objs.len() == 1).then(|| objs.first()).flatten() else { continue };
        let len_now = ram.s32(o + 0x1C) * 8;
        let age = ram.s32(o + 0x18);
        let mut kind_now = None;
        if let Some(Cmd::Emote { t, emote, .. }) = w.scene.tracks[k].queue.first_mut() {
            if wf_core::scene::emote_len(*emote) != len_now {
                continue;
            }
            kind_now = Some(*emote);
            *t = age;
            w.scene.tracks[k].started = true;
        } else {
            continue;
        }
        // (the replay's bubble: the newest; the RAM's one becomes this actor's)
        for v in w.scene.visuals.iter_mut().filter(|v| matches!(v.kind, VisualKind::Emote(_)) && v.t == age && v.life == len_now - age) {
            v.owner = id;
            if let Some(e) = kind_now {
                v.kind = VisualKind::Emote(e);
            }
        }
        // (the RAM's has the fewest frames left)
        let mine: Vec<(u32, i32)> = w.scene.visuals.iter().filter(|v| matches!(v.kind, VisualKind::Emote(_)) && v.owner == id).map(|v| (v.vid, v.life)).collect();
        if mine.len() > 1 {
            let keep = mine.iter().min_by_key(|m| m.1).map(|m| m.0);
            for (vid, _) in mine.iter().filter(|m| Some(m.0) != keep) {
                w.scene.visuals.retain(|v| v.vid != *vid);
                wf_core::vram::release(w, wf_core::vram::Owner::Visual(*vid));
            }
        }
    }
}

/// props moving at the cut (`prop_move_to`, object vt 0x0804B92C: +8 / +0xC the target px, +0x10 speed, +0x1C /
/// +0x20 the step in 1/256 px, +0x24 / +0x28 the position 24.8, +0x2C the steps left): the replayed command takes the
/// RAM's step and count, the prop the RAM's position (the area load put it where the flags say it ends: run 2 cuts
/// 859,325 .. 860,353, Kyodai Pyramid 14-21's s418 - the four bridge blocks already at their targets, the group's
/// wait over in the next frame; run 1 cut 38,386, s103 node 0's two props over the 0-22 cutscene map)
fn import_prop_moves(w: &mut World, ram: &GbaRam) {
    use wf_core::scene::{Cmd, Owner, Prop};
    let objs = ram.ewram_with(0x0804_B92C);
    let mut at = vec![];
    for t in w.scene.tracks.iter_mut() {
        let Owner::Prop(id) = t.owner else { continue };
        if let Some(Cmd::PropMoveTo { x, y, n, t: tt, sx, sy, .. }) = t.queue.first_mut() {
            if let Some(&o) = objs.iter().find(|&&o| (ram.s32(o + 8), ram.s32(o + 0xC)) == (*x, *y) && ram.s32(o + 0x2C) >= 0) {
                (*sx, *sy, *n, *tt) = (ram.s32(o + 0x1C), ram.s32(o + 0x20), ram.s32(o + 0x2C), 0);
                t.started = true;
                at.push((id, ram.s32(o + 0x24), ram.s32(o + 0x28)));
            }
        }
    }
    let Some(load) = w.field.as_ref().map(|f| f.loads) else { return };
    for (id, x, y) in at {
        let visible = w.field.as_ref().and_then(|f| f.props.get(id.max(0) as usize)).is_none_or(|p| p.visible);
        match w.scene.props.iter_mut().find(|p| p.id == id && p.load == load) {
            Some(p) => (p.x, p.y) = (x, y),
            None => w.scene.props.push(Prop { id, x, y, visible, frame: 0, vy: 0, load }),
        }
    }
    wf_core::scene::sync_props(w);
}

/// a script fade's progress at the cut (the main loop's mode): (steps run, length) - vt 0x0804C588 at +4 / +8, the
/// palette-copy fade vt 0x0804BBA4 at +0x404 / +0x408
fn script_fade_progress(ram: &GbaRam) -> Option<(i32, i32)> {
    let mode = ram.r32(0x0300_0F90);
    if !ram.valid(mode) {
        return None;
    }
    match ram.r32(mode) {
        0x0804_C588 => Some((ram.s32(mode + 4), ram.s32(mode + 8))),
        0x0804_BBA4 => Some((ram.s32(mode + 0x404), ram.s32(mode + 0x408))),
        _ => None,
    }
}

/// a script fade the replay restarted, waiting at the cut (0x78 .. 0x7e): its steps run from the fade mode object
/// (`script_fade_progress`) - the event's mid-code path and an inline script's (run 3 cut 131,618: 2-x's
/// inline_0x08073104 waiting 29 steps into its fade_in(30))
fn fade_progress_from_ram(w: &mut World, ram: &GbaRam, wn: u8) {
    if let Some((run, len)) = script_fade_progress(ram).filter(|_| (0x78..=0x7E).contains(&wn)) {
        if let Some(f) = w.scene.screen.fade.as_mut().filter(|f| f.len == len && f.kind == wf_core::scene::FadeKind::Black) {
            f.t = run.clamp(0, f.len);
            let (t, len, out) = (f.t, f.len, f.out);
            let (lvl, fine) = ((16 * t / len.max(1)).clamp(0, 16), (256 * t / len.max(1)).clamp(0, 256));
            let s = &mut w.scene.screen;
            if out {
                s.black = s.black.max(lvl);
                s.dim = s.dim.max(fine);
            } else {
                s.black = 16 - lvl;
                s.dim = 256 - fine;
            }
        }
    }
}

/// an inline script stopped in its code: (id, the natives its interpreter has run - the last the waiting one)
fn inline_running(ram: &GbaRam) -> Option<(String, Vec<(u8, Option<Vec<i32>>)>)> {
    let depth = ram.r32(0x0300_1034).min(8);
    for k in 1..=depth {
        let it = ram.r32(0x0300_1034 + 4 * k);
        for a in (0x0300_7000u32..0x0300_7FFC).step_by(4) {
            if ram.r32(a) != it {
                continue;
            }
            let pc = ram.r32(a + 4);
            if pc >> 24 != 8 {
                continue;
            }
            if let Some((id, base, code)) = host::inline_at(pc) {
                if let Some(ran) = natives_before(&code, (pc - base) as usize) {
                    return Some((id, ran));
                }
            }
        }
    }
    None
}

/// the leader's Ki Blasts in flight (vt 0x0804B454, the class 0x080197B4; +0x1C owner, +0x10 facing, +0x20 / +0x24
/// 24.8 position, +0x40 / +0x44 velocity, +0x48 ticks, +0x54 stopped this frame, +0x4C the end spark's frames left
/// counting down from 15 - MEASURED on real-run segments r010_001 / r012_014 / r015_006): our straight shot with the
/// same numbers (its end spark counts up to 16: dead = 16 - left); its tile block goes with it (`import_vram`;
/// whole-run cut 809,790: a blast in its last spark frame)
/// the coins lying or flying at the cut (`0x08017F5A`, vt 0x08049CC4: +0x14 .. +0x20 the pickup box, +0x24 the age,
/// +0x28 the animation phase, +0x2C the move counter (60 moves), +0x30 / +0x34 z and its speed, +0x50 / +0x54 the
/// 24.8 position, +0x58 / +0x5C the velocity, +0x60 the def (0x086FC250 + 4 size: +0xC the value) - MEASURED on the
/// exit-fade cuts 14-2 -> 14-1, real run r067_085: a 50 Z coin of age 587 goes at 601, 14 frames into the fade);
/// its tiles (+0xC) go to it in `import_vram`, its shadow is a shared graphic (refcounts)
fn import_coins(w: &mut World, ram: &GbaRam) {
    use wf_core::fieldobj::{Pickup, PickupKind};
    for o in ram.objects().into_iter().filter(|&o| ram.r32(o) == 0x0804_9CC4) {
        let def = ram.r32(o + 0x60);
        let Some(size) = [0x086F_C260u32, 0x086F_C378, 0x086F_C790, 0x086F_C8A8].iter().position(|&d| d == def) else { continue };
        let t = ram.s32(o + 0x24);
        let moves = ram.s32(o + 0x2C);
        let z = ram.s32(o + 0x30);
        let p = Pickup {
            kind: PickupKind::Zenie(size as i32),
            x: ram.s32(o + 0x50),
            y: ram.s32(o + 0x54),
            vx: ram.s32(o + 0x58),
            vy: ram.s32(o + 0x5C),
            z,
            vz: ram.s32(o + 0x34),
            t,
            mv: moves - t,
            landed: moves >= 60 && z == 0,
            phase: ram.r32(o + 0x28),
            bx: [ram.s32(o + 0x14), ram.s32(o + 0x18), ram.s32(o + 0x1C), ram.s32(o + 0x20)],
            solid: None,
            spawn: None,
        };
        w.add(Body::Pickup(Box::new(p)));
    }
}

/// an enemy's thrown rocks in flight (the rock thrower's TauntAnim, ctor 0x08033F28, vt 0x0804CE50: +0x1C the
/// thrower, +0x20 / +0x24 24.8 position, +0x40 / +0x44 velocity, +0x48 the steps left - two physics steps an update,
/// landing at 0 with its puff): our rock effect with those numbers; its tile block goes with it (`import_vram`; run 3
/// cut 158,217, Earth 2-90: a rock one step from landing - its puff's rand(0x1000) at f0)
fn import_rocks(w: &mut World, ram: &GbaRam) {
    for o in ram.objects().into_iter().filter(|&o| ram.r32(o) == 0x0804_CE50) {
        let t = ram.r32(o + 0x1C);
        let (tx, ty) = (ram.s32(t + 0x1A0), ram.s32(t + 0x1A4));
        let Some(owner) = w.ents.iter().find(|e| !e.removed && matches!(&e.body, Body::Enemy(en) if en.a.x == tx && en.a.y == ty)).map(|e| e.id) else { continue };
        let n = ram.s32(o + 0x48);
        if n <= 0 {
            continue;
        }
        wf_core::fx::add(w, wf_core::fx::Fx::Rock { owner, x: ram.s32(o + 0x20), y: ram.s32(o + 0x24), vx: ram.s32(o + 0x40), vy: ram.s32(o + 0x44), n });
    }
}

fn import_player_blasts(w: &mut World, ram: &GbaRam) {
    let po = ram.player();
    let Some(pi) = w.player_idx(0).filter(|_| po != 0) else { return };
    let me = w.ents[pi].id;
    for o in ram.objects().into_iter().filter(|&o| ram.r32(o) == 0x0804_B454 && ram.r32(o + 0x1C) == po) {
        let dir = ram.r8(o + 0x10) & 3;
        let id = wf_core::shots::ki_blast(w, me, dir as _, 1, ram.s32(o + 0x20), ram.s32(o + 0x24), wf_core::damage::atk(4), 1024, false);
        let Some(k) = w.find(id) else { continue };
        let (left, stopped) = (ram.s32(o + 0x4C), ram.s32(o + 0x54) != 0);
        let s = w.shot(k);
        s.t = ram.s32(o + 0x48) as i64;
        if let wf_core::shots::ShotKind::Straight(st) = &mut s.kind {
            st.vx = ram.s32(o + 0x40);
            st.vy = ram.s32(o + 0x44);
            st.b = [8, 8, 16, 16];
            st.rec_box = true;
            st.spawned = true;
            st.dead = if left > 0 { 16 - left } else if stopped { 0 } else { -1 };
        }
        w.pl(pi).blast = id;
    }
}

/// a death effect still counting down at the cut (the delayed-call list 0x0300225C: count, then objects; vt
/// 0x0804955C: +4 frames left, +8 / +0xC the loot px, +0x10 / +0x14 its velocity, +0x18 the type's drop script; the
/// countdown holds while an event runs): our `pending_drops` (real run West City r002_006 +6,000: the level-up event
/// s003 runs at the cut, the drop comes 8 updates after it - B's §11 update 16)
fn import_death_effects(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let n = ram.r32(0x0300_225C).min(32);
    // the list updates newest (last) first: of two effects due in one frame the later-listed one drops first (run 2
    // cut 1,779,433, 21-9: two Enzymes' drops held by a level-up - the game's first coin draws land at the second
    // effect's loot (80, 310); ours dropped (96, 313)'s first and missed a coin, sound 44 at 1,779,633)
    for k in (0..n).rev() {
        let o = ram.r32(0x0300_2260 + 4 * k);
        // a kill's XP still to come (the delayed call vt 0x0804A820: +4 frames left, 1 = the next frame's start, +8 the
        // XP; RAM's record is still without it): ours gives it from `xp_due` at the next frame's start, where a
        // level-up starts hook #3 (run 1 cut 83,786, the frame before a level-up popup: the import dropped the XP)
        // (inserted at the front: `pre_step` gives `xp_due` newest first, this loop walks the list newest first)
        if ram.valid(o) && ram.r32(o) == 0x0804_A820 {
            let (left, xp) = (ram.s32(o + 4), ram.s32(o + 8) as i64);
            match w.player_idx(0) {
                Some(pi) if left == 1 && xp > 0 => {
                    let pid = w.ents[pi].id;
                    if let Some(f) = w.field.as_mut() {
                        f.xp_due.insert(0, (pid, xp));
                    }
                }
                _ => notes.push(format!("mid-area: a kill-XP call ({xp} XP, {left} frames left) not imported")),
            }
            continue;
        }
        if !ram.valid(o) || ram.r32(o) != 0x0804_955C {
            continue;
        }
        let script = ram.r32(o + 0x18);
        // an on-defeat callback (+0x1C) whose enemy (+0x20) is already gone from the object list (not imported): the
        // callback from its area def (the enemy's +0x228 points 8 bytes before it), after the same countdown; a drop
        // script goes on below. Run 2 cut 1,682,927 (21-8): a Blister dead at the cut, coins + open_area(172, 120,
        // 196, 136) to come - ours lost the callback, the walk grid stayed blocked and its coins never flew (sound 44
        // missing at 1,683,032)
        if ram.r32(o + 0x1C) != 0 {
            let eo = ram.r32(o + 0x20);
            let (x, y) = (ram.s32(eo + 0x1A0), ram.s32(eo + 0x1A4));
            let alive = (0..w.ents.len()).any(|k| !w.ents[k].removed && matches!(w.ents[k].body, Body::Enemy(_)) && w.actor(k).is_some_and(|a| a.x == x && a.y == y));
            let rom = ram.r32(eo + 0x228).wrapping_add(8);
            let def = (!alive)
                .then(|| w.field.as_ref().and_then(|f| w.data.field.as_ref()?.area(f.area.world, f.area.area)))
                .flatten()
                .and_then(|a| a.objects.iter().find(|d| d.rom == rom).cloned());
            if let Some(wf_core::fieldcontent::ObjKind::Enemy { ty, on_defeat: Some(run), .. }) = def.map(|d| d.kind) {
                let left = ram.s32(o + 4).max(0) as u32;
                let cause = field::Cause::Defeat { entity: wf_core::world::EntityId::MAX, ty, x: x >> 8, y: y >> 8, loot: Some((ram.s32(o + 8), ram.s32(o + 0xC), ram.s32(o + 0x10), ram.s32(o + 0x14))) };
                if let Some(f) = w.field.as_mut() {
                    f.defeat_later.push((w.frame + left, run, cause));
                }
                if script == 0 {
                    continue;
                }
            }
        }
        // no drop script, an on-defeat callback (+0x1C, its enemy object +0x20): the enemy's area def's `on_defeat`
        // after the countdown (`defeat_later`; the enemy is defeated - out of the placed list). Real run 007,
        // whole-run cut 756,613: Pilaf's machine down, s003's level-up running, s365 to come
        if script == 0 && ram.r32(o + 0x1C) != 0 {
            let eo = ram.r32(o + 0x20);
            let (x, y) = (ram.s32(eo + 0x1A0), ram.s32(eo + 0x1A4));
            let k = (0..w.ents.len()).find(|&k| !w.ents[k].removed && matches!(w.ents[k].body, Body::Enemy(_)) && w.actor(k).is_some_and(|a| a.x == x && a.y == y));
            let placed = k.and_then(|k| w.field.as_ref().and_then(|f| f.placed.iter().find(|p| p.0 == w.ents[k].id).cloned()));
            let cb = placed.and_then(|p| field::area_obj(w, p.1)).and_then(|d| match d.kind {
                wf_core::fieldcontent::ObjKind::Enemy { on_defeat, .. } => on_defeat,
                _ => None,
            });
            let (Some(k), Some(p), Some(run)) = (k, placed, cb) else {
                notes.push(format!("mid-area: a death effect's callback {:#x} has no placed enemy with an on-defeat here", ram.r32(o + 0x1C)));
                continue;
            };
            let (id, ty) = (p.0, w.en_ref(k).ty);
            let (ex, ey) = (w.en_ref(k).a.px(), w.en_ref(k).a.py());
            let left = ram.s32(o + 4).max(0) as u32;
            // (an event holding the world update - its call 0x08011A5E on the stack under the VM - with the
            // countdown's last frame left: the effect's update goes on when the event ends, its callback in that
            // frame - `death_effects`' wait list; 756,613: s365 starts in the frame s003 ends)
            let held = (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0801_1A5F);
            let wait = held && w.scene.depth > 0 && left <= 1;
            let due = if wait { u32::MAX } else { w.frame + left };
            let cause = field::Cause::Defeat { entity: id, ty, x: ex, y: ey, loot: Some((ram.s32(o + 8), ram.s32(o + 0xC), ram.s32(o + 0x10), ram.s32(o + 0x14))) };
            if let Some(f) = w.field.as_mut() {
                f.placed.retain(|q| q.0 != id);
                f.placed_loot.retain(|q| q.0 != id);
                f.defeat_later.push((due, run, cause));
                if wait {
                    f.defeat_wait.push(id);
                }
            }
            continue;
        }
        let Some(ty) = w.data.enemies.iter().find(|t| t.drop_script == script).map(|t| t.ty) else {
            notes.push(format!("mid-area: a death effect's drop script {script:#x} is no enemy type's"));
            continue;
        };
        // (an event holding the field at the cut holds the countdown in the cut's frame too: it ends in it)
        // (an event that is not an inline hook - a save point's s011 - holds it through the busy-field hold from the
        // next frame on: run 2 cut 1,837,704, the coins 8 updates after the Save Game screen closes. A hook - the
        // world update's call 0x08011A5E on the stack under the VM, as a level-up popup after the kill - holds the
        // cut's frame too: tour_castle_pilaf_save14)
        let hook = w.field.as_ref().is_some_and(|f| f.held_event) || (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0801_1A5F);
        let due = w.frame + ram.s32(o + 4).max(0) as u32 + (w.scene.depth > 0 && hook) as u32;
        let d = (due, wf_core::world::EntityId::MAX, ty, ram.s32(o + 8), ram.s32(o + 0xC), ram.s32(o + 0x10), ram.s32(o + 0x14));
        if let Some(f) = w.field.as_mut() {
            f.pending_drops.push(d);
        }
    }
}

/// the breakables as the cut has them (vt 0x0804CE00: +0xC u8 HP, +0x14..+0x20 the box px, +0x24 the shake's frames
/// left, +0x2C the bound prop) and their break calls on the delayed-call list (vt 0x08049BB4: +4 updates left, +8 the
/// breakable; the countdown holds while an event runs - run 2 cut 788,100: the box punched to 0 HP with the kill
/// that levelled up, shaking 5 more frames and its loot 15 updates after the popup closes)
fn import_breakables(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    let objs: Vec<u32> = ram.objects().into_iter().filter(|&o| ram.r32(o) == 0x0804_CE00).collect();
    let n = ram.r32(0x0300_225C).min(32);
    let calls: Vec<(u32, u32)> = (0..n)
        .map(|k| ram.r32(0x0300_2260 + 4 * k))
        .filter(|&c| ram.valid(c) && ram.r32(c) == 0x0804_9BB4)
        .map(|c| (ram.r32(c + 8), ram.s32(c + 4).max(0) as u32))
        .collect();
    let bx_of = |o: u32| [ram.s32(o + 0x14), ram.s32(o + 0x18), ram.s32(o + 0x1C), ram.s32(o + 0x20)];
    for k in 0..w.ents.len() {
        let Body::Breakable(b) = &w.ents[k].body else { continue };
        if w.ents[k].removed || b.broken {
            continue;
        }
        let id = w.ents[k].id;
        let Some(o) = objs.iter().copied().chain(calls.iter().map(|c| c.0)).find(|&o| ram.valid(o) && bx_of(o) == b.bx) else {
            continue;
        };
        let (hp, shake) = (ram.r8(o + 0xC) as i32, ram.s32(o + 0x24).max(0));
        let call = calls.iter().find(|c| c.0 == o).map(|c| c.1);
        let Body::Breakable(b) = &mut w.ents[k].body else { continue };
        b.hp = hp;
        b.shake = shake;
        if hp == 0 {
            b.broken = true;
        }
        if let Some(left) = call {
            // (the call runs in the `left`-th update from here - run 2 cut 788,141: 12 left, the loot at 788,152; an
            // event holding the field at the cut holds the countdown in the cut's frame and in the frame it ends:
            // cut 788,100, 15 left, s003 ends at 788,137, the loot at 788,152)
            let at = w.frame + left + 2 * (w.scene.depth > 0) as u32;
            if let Some(f) = w.field.as_mut() {
                f.deferred.push(wf_core::fieldobj::Deferred::Break { at, entity: id });
            }
        } else if hp == 0 {
            notes.push(format!("mid-area: a breakable at {:?} has no HP and no break call", bx_of(o)));
        }
    }
}

/// an Instant Transmission selection at the cut (the main loop's mode 0x0804D260, the world held): +4 the picks
/// left, +8 the marker object (vt 0x0804E41C: +0x38 its frames, +0x3C / +0x40 the cursor 24.8, +0x44 / +0x48 its
/// velocity), +0xC the candidates' count, +0x10 + 0x1C k a candidate {rect, +0x10 picked, +0x18 the object}. The
/// picks' order is not kept here: picked candidates in list order (r062_001_20-6 +3,000 / +12,000)
fn import_it_select(w: &mut World, pi: usize, ram: &GbaRam, notes: &mut Vec<String>) {
    let mode = ram.r32(0x0300_0F90);
    if !ram.valid(mode) || ram.r32(mode) != 0x0804_D260 {
        return;
    }
    let mk = ram.r32(mode + 8);
    let n = ram.s32(mode + 0xC).clamp(0, 16) as u32;
    let mut cand = vec![];
    for k in 0..n {
        let r = mode + 0x10 + 0x1C * k;
        let o = ram.r32(r + 0x18);
        let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        match (0..w.ents.len()).find(|&e| !w.ents[e].removed && w.actor(e).is_some_and(|a| a.x == x && a.y == y)) {
            Some(e) => cand.push((w.ents[e].id, ram.r32(r + 0x10) & 0xFF != 0)),
            None => {
                notes.push(format!("mid-area: Instant Transmission candidate {k} ({o:#x}) has no entity here (a trigger?)"));
                return;
            }
        }
    }
    let picks: Vec<_> = cand.iter().filter(|c| c.1).map(|c| c.0).collect();
    let left = ram.s32(mode + 4);
    let serial = w.next_serial();
    let sel = wf_core::player::ItSel {
        serial,
        n: left,
        cand,
        order: picks.iter().map(|&e| wf_core::player::ItPick::Ent(e)).collect(),
        picks,
        x: ram.s32(mk + 0x3C),
        y: ram.s32(mk + 0x40),
        vx: ram.s32(mk + 0x44),
        vy: ram.s32(mk + 0x48),
        cursor: left > 0,
        tcand: vec![],
    };
    wf_core::player::import_state(w, pi, wf_core::player::Gs::Hold);
    let me = w.ents[pi].id;
    {
        let p = w.pl(pi);
        p.it_sel = Some(sel);
        p.state = wf_core::player::PState::Skill;
    }
    let f = wf_core::fx::add(w, wf_core::fx::Fx::ItMarkers { t: ram.s32(mk + 0x38), alive: wf_core::fx::Alive::It { p: me, serial } });
    w.modal = Some(vec![me, f]);
}

/// the leader's state machine (mid-area imports): the state object (+0x14) by its vtable -> our state, the
/// animation from the recording's row (the animation-driven states - punch, hurt, once, ko - run on it)
fn import_player_state(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    use wf_core::player::Gs;
    let Some(pi) = w.player_idx(0) else { return };
    let po = ram.player();
    // the hit-damage remainder in 1/1024 HP (player object +0x228) - whatever the state (whole-run cut 185,000: the
    // leader in a scripted transformation, 193 / 1024 carried into the next hit)
    if po != 0 {
        w.pl(pi).hp_acc.acc = ram.s32(po + 0x228) as i64;
    }
    let st = ram.r32(po + 0x14);
    if !ram.valid(st) {
        return;
    }
    // a script's animation state (the only animation id with that state: 0x0804DCA4 SsUp = anim 23, 0x0804A730
    // OnceHold = anim 16): the scene's SetAnim (the player is its puppet), then the RAM's cel / tick
    let scripted = match ram.r32(st) {
        0x0804_DCA4 => Some(23),
        0x0804_A730 => Some(16),
        _ => None,
    };
    import_it_select(w, pi, ram, notes);
    // an Instant Transmission run (state 0x0804E334, update 0x0801BB1A): +4 the facing at its start, +5 the pick it
    // is on, +8 the picks' count, +0xC + 8k {kind byte (1 = a trigger), +4 the object}, +0x3C / +0x40 the px it
    // returns to, +0x44 its frame count (-24 vanish, 0 the jump, 24 the punch); r062_001_20-6 +10,000
    if ram.r32(st) == 0x0804_E334 {
        let n = ram.s32(st + 8).clamp(0, 8) as u32;
        let mut picks = vec![];
        for k in 0..n {
            let (kind, o) = (ram.r8(st + 0xC + 8 * k), ram.r32(st + 0x10 + 8 * k));
            let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
            let ent = (kind != 1).then(|| (0..w.ents.len()).find(|&e| !w.ents[e].removed && w.actor(e).is_some_and(|a| a.x == x && a.y == y))).flatten();
            // a trigger pick (kind 1; event trigger +4 rect s16 x0 y0 x1 y1): the load's trigger with that rect
            let trig = (kind == 1)
                .then(|| {
                    let r = (ram.s16(o + 4), ram.s16(o + 6), ram.s16(o + 8), ram.s16(o + 0xA));
                    w.field.as_ref().and_then(|f| f.triggers.iter().find(|t| (t.rect.x0, t.rect.y0, t.rect.x1, t.rect.y1) == r)).map(|t| t.obj)
                })
                .flatten();
            match (ent, trig) {
                (Some(e), _) => picks.push(wf_core::player::ItPick::Ent(w.ents[e].id)),
                (None, Some(t)) => picks.push(wf_core::player::ItPick::Trig(t)),
                // a pick the run has passed (its object may be gone - whole-run cut 1,107,692, HFIL statues: a
                // freed enemy): never read again
                (None, None) if k < ram.r8(st + 5) as u32 => picks.push(wf_core::player::ItPick::Ent(wf_core::world::EntityId::MAX)),
                (None, None) => {
                    notes.push(format!("mid-area: Instant Transmission pick {k} (kind {kind}, {o:#x}) has no entity / trigger here"));
                    return;
                }
            }
        }
        let targets = picks.iter().filter_map(|p| if let wf_core::player::ItPick::Ent(e) = p { Some(*e) } else { None }).collect();
        let run = wf_core::player::ItRun { serial: 0, dir: ram.r8(st + 4) as _, idx: ram.r8(st + 5) as usize, targets, origin: (ram.s32(st + 0x3C), ram.s32(st + 0x40)), t: ram.s32(st + 0x44), picks };
        wf_core::player::import_state(w, pi, Gs::Skill(14));
        let row = ram.actor_rows.iter().find(|r| r.0 == po).copied();
        let p = w.pl(pi);
        p.it = Some(run);
        p.a.hittable = false;
        if let Some((_, _, raw, cel, tick)) = row {
            p.ap.raw = raw;
            p.ap.frame = cel;
            p.ap.tick = tick;
        }
        return;
    }
    if let Some(aid) = scripted {
        let rng = w.rng.clone();
        wf_core::scene::set_state(w, pi, aid);
        w.rng = rng;
        if let Some(&(_, _, raw, cel, tick)) = ram.actor_rows.iter().find(|r| r.0 == po) {
            let p = w.pl(pi);
            p.ap.raw = raw;
            p.ap.frame = cel;
            p.ap.tick = tick;
        }
        return;
    }
    let gs = match ram.r32(st) {
        0x0804_C790 => Gs::Idle,
        0x0804_A77C => Gs::Idle2,
        0x0804_C3A8 => Gs::Walk,
        0x0804_BEF0 => Gs::Dash,
        0x0804_E00C => Gs::AutoWalk,
        0x0804_C384 => Gs::Bored,
        0x0804_C35C => Gs::Punch,
        0x0804_A188 => Gs::Hold0,
        0x0804_985C => Gs::Hold,
        // (the one-pass state's class serves both rates: the animation's own - SetAnim(18), raw 20, 16 ticks a cel; run
        // 3 cut 68,892, s194's play_anim(0, 18) at tick 27 on cel 1)
        0x0804_D784 => match ram.actor_rows.iter().find(|r| r.0 == po).and_then(|r| wf_core::player::state_for_raw(r.2)) {
            Some(Gs::Once4) => Gs::Once4,
            _ => Gs::Once3,
        },
        0x0804_C744 => Gs::Block,
        0x0804_D75C => Gs::Hurt,
        0x0804_C768 => Gs::Ko,
        // the revert out of a Super Saiyan form (vt 0x0804CC58, anim 24 = raw 12, one pass at 16 ticks a cel; not
        // hittable): gameplay's (the energy ran out) - whole-run cut 137,896, the leader 3 cels in
        0x0804_CC58 => Gs::SsRevert,
        // the Energy Blast state (vt 0x0804DC14, update 0x0801AF3C: the player's tick +0x12 counts, the blast at 16,
        // at 40 a refire if +4 (set by a B press, 0x0801AFE2) else idle) - the selected skill's (whole-run cut
        // 809,832: a blast thrown, refire pending)
        0x0804_DC14 if w.data.skill_kind(w.pl_ref(pi).skill()) == 7 => Gs::Skill(7),
        // a beam skill charging (the press's state, vt 0x0804D9F8: +8 the beam object vt 0x0804AEF8, the player's tick
        // +0x12 the charge count; run 3 cut 30,383, 1-16: Gohan holding B, tick 4) - the selected skill's
        0x0804_D9F8 if matches!(w.data.skill_kind(w.pl_ref(pi).skill()), 15 | 16 | 6 | 1 | 3) => Gs::Skill(w.data.skill_kind(w.pl_ref(pi).skill()) as u8),
        vt => {
            notes.push(format!("mid-area: the leader's state vt {vt:#x} is not imported"));
            return;
        }
    };
    let row = ram.actor_rows.iter().find(|r| r.0 == po).copied();
    wf_core::player::import_state(w, pi, gs);
    // a KO'd leader (the KO state, or its held death pose at HP 0) is down as the KO's enter leaves it: not hittable,
    // and its message handler accepts an overlapping box without damage (`player::on_message`: Kid Buu's beam stops
    // on it and sparks, 5 draws every 5 .. 7 frames - run 2 rows 1,880,523 .. 1,880,659)
    if gs == Gs::Ko || (gs == Gs::Hold && w.pl_ref(pi).rec.hp == 0) {
        let p = w.pl(pi);
        p.dead = true;
        p.a.hittable = false;
        // the KO timer (vt 0x080496BC, +4 the frames left from 200; the death script starts when it ends - run 2 cut
        // 1,880,460: 200 left, s000 at row 1,880,659): our `ko_at` (`menu::gameover::tick`: KO + DEATH_DELAY)
        if let Some(t) = ram.ewram_with(0x0804_96BC).into_iter().next() {
            w.pl(pi).ko_at = w.frame as i64 + ram.s32(t + 4) as i64 - wf_core::menu::gameover::DEATH_DELAY;
        }
    }
    if let Gs::Skill(k @ (15 | 16 | 6 | 1 | 3)) = gs {
        let tick = row.map(|r| r.4).unwrap_or(0);
        let held = ram.r16(INPUT + 2) & 2 != 0;
        wf_core::player::import_beam_charge(w, pi, k as i32, tick, held);
    }
    if gs == Gs::Skill(7) {
        let serial = w.next_serial();
        let p = w.pl(pi);
        let id = p.skill();
        p.t = 0;
        p.sk = Some(wf_core::player::SkillInst { id, serial, held: false, release: -1, refire: ram.r8(st + 4) != 0, ..Default::default() });
    }
    let p = w.pl(pi);
    if let Some((_, _, raw, cel, tick)) = row {
        p.ap.raw = raw;
        p.ap.frame = cel;
        p.ap.tick = tick;
    }
    p.a.blocking = gs == Gs::Block;
    if gs == Gs::SsRevert {
        p.a.hittable = false;
    }
    // the idle state's boredom counter (state object +4, `0x08013022`: bored past 1024; whole-run cut 885,660 - the
    // game's leader got bored 66 frames in)
    if gs == Gs::Idle {
        p.s_cnt = ram.s32(st + 4);
    }
    // the resist bytes (obj +0x1B8 / +0x1B9: 0 while hurt = the i-frames, 128 otherwise; whole-run cuts 808,776 /
    // 809,325 / 967,072: a hurt leader hit again in ours)
    p.resist = ram.r8(po + 0x1B8) as i32;
    p.a.stats.res_m = ram.r8(po + 0x1B8) as i32;
    p.a.stats.res_e = ram.r8(po + 0x1B9) as i32;
}

/// **an event running mid-node over a standing field** (mid-area in-event): the field from RAM as `build_mid`
/// makes it, the event held, its node started afresh (a text node opens its box) and the text box set to the RAM's
/// (object vt 0x08049F60: +0x10 text, +0x14 typing pointer; one character a frame, see `text::typed_at`)
fn build_mid_event(_data: &Arc<GameData>, ram: &GbaRam, runner: &mut StoryHost, ev: (u16, u16, [i32; 4]), mut w: World, calls0: u32, notes: &mut Vec<String>) -> Built {
    let mut box_due = false;
    let to = w.field.as_ref().map(|f| f.area).unwrap_or_default();
    let po = ram.player();
    if po == 0 {
        // a cutscene map with the players offstage (`load_scene*`: no player object): the map's objects from RAM
        wf_core::scene::send_offstage(&mut w);
        field::load_area_with(&mut w, to, 120, 120, None, true);
    } else {
        let (x, y, facing) = (ram.s32(po + 0x1A0) >> 8, ram.s32(po + 0x1A4) >> 8, ram.r8(po + 0x10));
        field::load_area(&mut w, to, x, y, Some(facing));
    }
    w.events.clear();
    // the song the driver plays, not the area's (the rebuild's load asked for it: run 2 cut 59,477 - Other World 1-20
    // under song 16, the area's 25 - then started 16 again at s-script's play_music)
    w.scene.song = song(ram);
    field::finish_arrival(&mut w);
    if let Some(f) = w.field.as_mut() {
        f.arrival = None;
        f.mode = Mode::Normal;
    }
    w.scene.depth = 1;
    field::set_busy(&mut w, true);
    field_from_ram(&mut w, ram, notes);
    import_player_state(&mut w, ram, notes);
    // the screen an earlier fade_out left black (a fade the node's replay runs sets its own): black (whole-run cut
    // 628,017: s508 places the map objects under node 0's fade_out(30), node 9 fades in)
    if ram.screen_black == Some(true) {
        w.scene.screen.black = 16;
        w.scene.screen.dim = 256;
    }
    // a touch-once trigger running its event: the game removes it when the event returns (still in the RAM's list),
    // ours when it fires - gone (whole-run cut 887,156: s437 fired again after its end)
    let name = format!("s{:03}", ev.0);
    // (the trigger itself, for an event started in the cut's frame: below)
    let fresh_trig = w.field.as_ref().and_then(|f| {
        f.triggers.iter().map(|t| t.obj).find(|&o| field::trigger_def(&w, o).is_some_and(|d| matches!(d.kind, wf_core::fieldcontent::ObjKind::Trigger { kind: wf_core::fieldcontent::TriggerKind::Event(ref s) | wf_core::fieldcontent::TriggerKind::SavePoint(ref s), .. } if *s == name)))
    });
    let fired: Vec<usize> = w.field.as_ref().map(|f| f.triggers.iter().enumerate().filter(|(_, t)| t.act == wf_core::fieldcontent::Activation::TouchOnce && field::trigger_def(&w, t.obj).is_some_and(|d| matches!(d.kind, wf_core::fieldcontent::ObjKind::Trigger { kind: wf_core::fieldcontent::TriggerKind::Event(ref s), .. } if *s == name))).map(|(k, _)| k).collect()).unwrap_or_default();
    if let Some(f) = w.field.as_mut() {
        for k in fired.into_iter().rev() {
            f.triggers.remove(k);
        }
    }
    // an event started from inside the frame's object pass (a defeat callback / hook run inline: the world list's
    // update call 0x08011A5E is on the stack under the VM) holds that frame: when it ends, the frame's remaining
    // updates run (`held_event`; mid_12000: Kid Buu's AI re-evaluates in s459's exit frame)
    if (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0801_1A5F) {
        if let Some(f) = w.field.as_mut() {
            f.held_event = true;
        }
    }
    import_talk_pass(ram, runner, &mut w, notes);
    // a script's fade_out before it: the screen stays black (whole-run cut 612,541: s508's node 0 faded out and
    // warped to 0-55, node 1 runs under the black screen)
    if ram.screen_black == Some(true) && w.scene.screen.black == 0 {
        w.scene.screen.black = 16;
        w.scene.screen.dim = 256;
    }
    // stopped inside the node's code: the natives its interpreter has run (the PC after the one that blocks)
    if let Some(ran) = natives_run(ram, runner, ev) {
        return build_mid_code(ram, runner, ev, w, calls0, &ran, notes);
    }
    // an event its trigger started in the cut's frame (`fresh_scene`): it starts as the trigger check starts it, its
    // scene's first step - node 0 - in the next frame (run 2 cut 49,356: s100's trigger at 1-17)
    if ev.1 == 0 && ram.mode_vt() == MODE_SCENE && fresh_scene(ram, ram.r32(ram.r32(MODE) + 4)) {
        use wf_core::fieldcontent::Callback;
        let name = format!("s{:03}", ev.0);
        if let Some(obj) = fresh_trig {
            w.scene.depth = 0;
            field::set_busy(&mut w, false);
            if let Some(f) = w.field.as_mut() {
                f.event_ended = false;
            }
            let ev_run = Event::Field(field::FieldEvent::Run { run: Callback::Event(name), cause: field::Cause::Trigger { obj }, slot: 0 });
            runner.run_events(&mut w, &[ev_run]);
            notes.push(format!("event s{:03}: started by its trigger in the cut's frame (its first node next frame)", ev.0));
            let draws = w.rng.calls.wrapping_sub(calls0);
            return Built { world: w, draws, held: 0, finished: true };
        }
        // a death effect's on-defeat callback started inside the world update (0x08011A5F on the stack): the effect
        // (vt 0x0804955C, its countdown over: +0x1C the callback, +0x20 its enemy) has left the delayed-call list but
        // its pointer stays in the list's first slot. It starts as `defeat_later` starts it, holding the frame (run 2
        // cut 884,710, 11-24: a Majin's drop script, then s377 - the flags of the five Majins - in one frame)
        let held = (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0801_1A5F);
        let de = ram.r32(0x0300_2260);
        let on_defeat = w.field.as_ref().and_then(|f| w.data.field.as_ref()?.area(f.area.world, f.area.area)).is_some_and(|a| {
            a.objects.iter().any(|o| matches!(&o.kind, wf_core::fieldcontent::ObjKind::Enemy { on_defeat: Some(Callback::Event(s)), .. } if *s == name))
        });
        if held && on_defeat && ram.valid(de) && ram.r32(de) == 0x0804_955C && ram.r32(de + 0x1C) != 0 && ram.valid(ram.r32(de + 0x20)) {
            let eo = ram.r32(de + 0x20);
            let (x, y) = (ram.s32(eo + 0x1A0) >> 8, ram.s32(eo + 0x1A4) >> 8);
            let ty = ((ram.r32(eo + 0x224).wrapping_sub(0x0871_1274)) / 28) as i32;
            w.scene.depth = 0;
            field::set_busy(&mut w, false);
            if let Some(f) = w.field.as_mut() {
                f.event_ended = false;
            }
            let cause = field::Cause::Defeat { entity: wf_core::world::EntityId::MAX, ty, x, y, loot: None };
            runner.run_events(&mut w, &[Event::Field(field::FieldEvent::Run { run: Callback::Event(name), cause, slot: 0 })]);
            if let Some(f) = w.field.as_mut() {
                f.held = true;
                f.held_event = true;
            }
            notes.push(format!("event s{:03}: a death effect's on-defeat callback started in the cut's frame", ev.0));
            let draws = w.rng.calls.wrapping_sub(calls0);
            return Built { world: w, draws, held: 0, finished: true };
        }
        // the engine hook #3 (the level-up popups, s003) a kill's XP started inside the frame's object pass (the punch
        // that killed: XP -> level -> the hook run inline from the player's update, 0x08011A5F on the stack; the
        // record is already the levelled one in RAM): it starts as `player::give_xp` starts it, holding the frame
        // (run 2 cut 1,842,712, Inside Buu: a punch kills, Goku levels)
        if ev.0 == 3 && held && fresh_trig.is_none() {
            if let Some(pi) = w.player_idx(0) {
                let entity = w.ents[pi].id;
                let slot = w.pl_ref(pi).slot;
                w.scene.depth = 0;
                field::set_busy(&mut w, false);
                if let Some(f) = w.field.as_mut() {
                    f.event_ended = false;
                }
                // (the kill punch's screen shake runs on through the hook: the field's, imported - run 1 cut 267,114)
                import_field_shake(&mut w, ram);
                let cause = field::Cause::LevelUp { entity };
                runner.run_events(&mut w, &[Event::Field(field::FieldEvent::Run { run: Callback::Event(name), cause, slot })]);
                if let Some(f) = w.field.as_mut() {
                    f.held = true;
                    f.held_event = true;
                }
                notes.push("event s003: the level-up hook a kill's XP started inside the object pass in the cut's frame".into());
                let draws = w.rng.calls.wrapping_sub(calls0);
                return Built { world: w, draws, held: 0, finished: true };
            }
        }
        // the engine hook #0 (the death script) the KO timer started in the cut's frame (`menu::gameover::tick`: its
        // form revert already in RAM): started as the timer starts it, node 0 next frame (run 2 cuts 1,037,213 /
        // 1,319,520)
        // (the KO's own state is over by then - the leader holds at HP 0: down, not hittable)
        let down = w.player_idx(0).filter(|&i| w.pl_ref(i).rec.hp == 0);
        if let (None, 0, Some(i)) = (fresh_trig, ev.0, down) {
            let p = w.pl(i);
            p.dead = true;
            p.a.hittable = false;
            // the death script's loop runs from KO + 199 (`player::ko_freeze`): the player is the only object updated
            // (run 2 cut 1,909,572: a Kid Buu copy's beam hit the KO'd player once more in ours)
            if w.field.is_some() && w.players.len() == 1 {
                w.modal = Some(vec![w.ents[i].id]);
            }
            w.scene.depth = 0;
            field::set_busy(&mut w, false);
            if let Some(f) = w.field.as_mut() {
                f.event_ended = false;
            }
            let ev_run = Event::Field(field::FieldEvent::Run { run: Callback::Event(name), cause: field::Cause::Menu, slot: 0 });
            runner.run_events(&mut w, &[ev_run]);
            notes.push("event s000: the death script the KO timer started in the cut's frame (its first node next frame)".into());
            let draws = w.rng.calls.wrapping_sub(calls0);
            return Built { world: w, draws, held: 0, finished: true };
        }
        // a breakable's on-break callback (the box broken 16 frames before, its `0x0800C628` in the cut's frame): it
        // starts as `fieldobj::break_done` starts it (run 2 cut 856,042: Kyodai Pyramid 14-21's switch block, s418)
        let brk = w.field.as_ref().and_then(|f| {
            let fd = w.data.field.as_ref()?;
            let a = fd.area(f.area.world, f.area.area)?;
            a.objects.iter().find_map(|o| match &o.kind {
                wf_core::fieldcontent::ObjKind::Breakable { prop, on_break: Some(Callback::Event(e)), .. } if *e == name => {
                    let r = f.props.get(*prop.max(&0) as usize).map(|p| p.rect)?;
                    Some((o.at, (r.x0 + r.x1) >> 1, r.y1))
                }
                _ => None,
            })
        });
        if let Some((obj, x, y)) = brk {
            w.scene.depth = 0;
            field::set_busy(&mut w, false);
            let ev_run = Event::Field(field::FieldEvent::Run { run: Callback::Event(name), cause: field::Cause::Break { obj, x, y }, slot: 0 });
            runner.run_events(&mut w, &[ev_run]);
            // (ours stamps the break a frame before the event's first frame: the players are held in the cut's frame,
            // updated again from the next - the punch that broke the box ends there)
            let frame = w.frame;
            if let Some(f) = w.field.as_mut() {
                f.event_ended = false;
                f.busy_since = frame.wrapping_sub(1);
                f.busy_pre_pass = frame;
                f.queued_event = true;
                f.break_event = frame;
                f.break_cb = true;
            }
            notes.push(format!("event s{:03}: started by its breakable's break in the cut's frame (its first node next frame)", ev.0));
            let draws = w.rng.calls.wrapping_sub(calls0);
            return Built { world: w, draws, held: 0, finished: true };
        }
        notes.push(format!("unsupported: event s{:03} started in the cut's frame by no trigger of ours", ev.0));
    }
    runner.continue_event(&mut w, ev.0, ev.1, ev.2, 0, None);
    // the text box as the RAM shows it
    let tb = ram.objects().into_iter().find(|&o| ram.r32(o) == 0x0804_9F60);
    match (tb, w.scene.dialog.as_mut()) {
        (Some(o), Some(d)) => {
            // the typing pointer's units consumed, paged and counted by our line fitter (the game's +0x1C / +0x20
            // count its own lines and scroll, not the A-wait pages)
            let used = ((ram.r32(o + 0x14) as i64 - ram.r32(o + 0x10) as i64) / 2).max(0) as usize;
            let (page, typed) = wf_script_core::text::typed_at(&d.text, used);
            d.page = page.min(d.pages.saturating_sub(1));
            d.typed = typed as i32;
            d.phase = wf_core::scene::DialogPhase::Shown;
            d.t = 64;
            // a choice's cursor object (text box +0x48; its +0x10 counts its draws: the blink)
            let c = ram.r32(o + 0x48);
            d.page_t = if ram.valid(c) { d.t - ram.r32(c + 0x10) as i32 + 1 } else { d.t + 1 };
            // the slow text speeds' frame counter
            d.type_c = ram.r32(o + 0x58) as i32;
            // the last page typed out and A pressed within the close's frames: closing (the box object shows
            // nothing of it: the press's stamp in the input ring 0x03003300 +0x88 against the counter 0x03000F94; mid_24000)
            let need = d.typing.get(d.page as usize).copied().unwrap_or(0) as i32;
            if d.page + 1 >= d.pages && d.typed >= need {
                let now = ram.r32(0x0300_0F94) as i64;
                if let Some(&(_, at)) = press_ring(ram).iter().rev().find(|e| e.0 & 1 != 0) {
                    let t = now - at;
                    // (the box's open level +0x50 - 8 when shown - counts down from the frame after the press: a
                    // press still typing the page was not the close - whole-run cut 886,590, s437: A 7 frames before,
                    // the box at 8)
                    if (0..wf_core::scene::DIALOG_CLOSE as i64).contains(&t) && ram.s32(o + 0x50) as i64 == 8 - t {
                        d.result = Some(0);
                        d.phase = wf_core::scene::DialogPhase::Closing;
                        d.t = t as i32;
                    }
                }
            }
        }
        (None, Some(d)) => {
            // (just opened by the node's start: the game makes the box object - its tiles - later in the cut's
            // iteration; the importer takes them after rebuilding the allocator)
            if d.phase == wf_core::scene::DialogPhase::Opening && d.t == 0 {
                runner.text_open_due = Some(d.portrait != 0 && d.portrait != 7);
            }
            notes.push("mid-area: the event's text box is not open in RAM (opening / closing)".into());
            // (closing, not opening: the box object is gone and the A that closed it is less than the close's
            // frames old - the node goes on when they are out: run 3 cut 129,633, s134 node 3's "Dad? Is that you?"
            // closed by an A 8 frames before, node 4's box opens in the cut's frame; ours waited in node 3)
            let now = ram.r32(0x0300_0F94) as i64;
            let last = press_ring(ram).iter().rev().find(|e| e.0 & 1 != 0).map(|e| now - e.1);
            // (not inside the long iteration that makes a new box - the recording's first rows still on the cut's logic
            // count: a box opening there, its A the previous box's - run 3 cuts 6,166 / 63,334 / 189,183 / 226,104)
            match last.filter(|&t| !ram.mid_iteration && (0..wf_core::scene::DIALOG_CLOSE as i64).contains(&t)) {
                Some(t) => {
                    d.result = Some(0);
                    d.phase = wf_core::scene::DialogPhase::Closing;
                    d.t = t as i32;
                    runner.text_open_due = None;
                }
                None => box_due = true,
            }
        }
        _ => {}
    }
    // the cut inside the long iteration that makes the box (the recording's first rows still carry the cut's logic
    // count): that iteration ends with its camera update, shown in its last frame (run 3 cuts 57,493 s193 / 162,866
    // s289: the camera one step on at the iteration's last row) - ours makes it here
    if box_due && ram.mid_iteration && w.field.is_some() {
        wf_core::field::world_mode_cameras(&mut w);
    }
    notes.push(format!("event s{:03} at node {}: mid-node over a standing field (mid-area)", ev.0, ev.1));
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held: 0, finished: true }
}

/// a cut inside an object pass (the save state taken while a slow iteration's pass was running - the arrival fade-in
/// of HFIL's tour kais_hfil5: a chasing monster's path plan): the doc's first rows still carry the cut's logic count
/// (`GbaRam::mid_iteration`) and the world list's update call is on the stack (return 0x08011A87, the loop index - r4
/// - in the word before, the object being updated 12 bytes below): the objects from that index down to 0 have not
/// been updated in the cut's iteration - they are now (the later ones were)
fn finish_interrupted_pass(w: &mut World, ram: &GbaRam, notes: &mut Vec<String>) {
    if !ram.mid_iteration || ram.load_frozen || w.field.is_none() || w.scene.depth > 0 || wf_core::menu::active(w) {
        return;
    }
    let list = ram.objects();
    let Some(a) = (0x0300_7008u32..0x0300_8000).step_by(4).find(|&a| {
        ram.r32(a) == 0x0801_1A87 && {
            let k = ram.r32(a - 4) as usize;
            k < list.len() && list[k] == ram.r32(a - 12)
        }
    }) else {
        return;
    };
    let (k, o) = (ram.r32(a - 4) as usize, ram.r32(a - 12));
    // (our entity at the object's position: list indices differ - the game's list holds objects ours does not)
    let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
    let Some(id) = (0..w.ents.len()).find(|&i| !w.ents[i].removed && w.actor(i).is_some_and(|a| a.x == x && a.y == y)).map(|i| w.ents[i].id) else {
        notes.push(format!("mid-area: the cut stands in an object pass at list index {k}, its object {o:#x} has no entity"));
        return;
    };
    // the interrupted object's own update had run up to the cut - its draws are in the RAM's generator already: ours
    // runs from the generator rewound by them (found by the update ending on the RAM's state), the rest of the pass
    // from the RAM's (kais_hfil5: the monster's chase decision drew a chance, its path plan was running)
    let rng = w.rng.state();
    let mut done = false;
    for m in 0..=8u32 {
        let mut w2 = w.clone();
        w2.rng.set_state(rewind(rng, m));
        w2.update_one(id);
        if w2.rng.state() == rng {
            *w = w2;
            done = true;
            break;
        }
    }
    if !done {
        w.update_one(id);
    }
    w.finish_pass_after(id);
    // ... and the iteration's step after its pass: an arrival fade-in's brightness step (the fade mode's update follows
    // the objects - kais_hfil5: the fade ended, the leader idle, in the game's iteration 699,645, ours one later)
    if let Some(f) = w.field.as_mut() {
        if let Mode::FadeIn { k } = &mut f.mode {
            *k += 1;
        }
    }
    notes.push(format!("mid-area: the cut stands in an object pass at list index {k} - the rest of the pass is run"));
}

/// an event a talk started inside the player's update (the world list's update call returns to 0x08011A87, its
/// saved loop index - r4 - in the word before; the update it interrupted is the player's, 0x08010BF1) whose area has
/// been left since: its exit resumes that object pass on the new list (`field::resume_pass_after_load`; the real
/// run's 7-6, cut e73c_46000: s304's exit at f425 gives every object a second update). The game's index becomes ours
/// as the actors (and the props object) below it; an index past the list's end is the whole list.
fn import_talk_pass(ram: &GbaRam, runner: &mut StoryHost, w: &mut World, notes: &mut Vec<String>) {
    let Some(a) = (0x0300_7008u32..0x0300_8000).step_by(4).find(|&a| ram.r32(a) == 0x0801_1A87 && ram.r32(a - 8) == 0x0801_0BF1) else { return };
    let (k, obj) = (ram.r32(a - 4) as usize, ram.r32(a - 12));
    let list = ram.objects();
    // still in the list it talked in: no load since, nothing resumes
    if k > 255 || list.contains(&obj) {
        return;
    }
    let leader = ram.r32(0x0300_1C34);
    let ours = if k >= list.len() {
        usize::MAX / 2
    } else {
        field::props_object(w) as usize + list[..k].iter().filter(|&&o| o == leader || matches!(ram.r32(o), VT_PLAYER | VT_ENEMY | VT_NPC | VT_NPC_TALK)).count()
    };
    if let Some(f) = w.field.as_mut() {
        f.talk_pass = Some((ours, f.loads.wrapping_sub(1)));
    }
    runner.hs.talk_event = true;
    notes.push(format!("mid-area: the event's talk interrupted the object pass at index {k} of an earlier area's list (resumed at its exit)"));
}

/// the natives (in order) event `ev`'s node has run, from its interpreter's PC: the interpreters on the context
/// stack 0x03001034 ([0] depth, [1..] objects), each object's PC in the stack word after the one holding it (the VM
/// 0x08004038 keeps it in the pushed r1 slot); the node's code found at its ROM address or (compressed) as the
/// decompressed copy in EWRAM around the PC. Queries (natives that answer a value) are left out: the script asks
/// them again. None = no interpreter stands in the node's code
fn natives_run(ram: &GbaRam, runner: &StoryHost, ev: (u16, u16, [i32; 4])) -> Option<Vec<(u8, Option<Vec<i32>>)>> {
    let (addr, code) = runner.step_code.get(&(ev.0, ev.1))?;
    let depth = ram.r32(0x0300_1034).min(8);
    let mut pc_off = None;
    for k in 1..=depth {
        let it = ram.r32(0x0300_1034 + 4 * k);
        for a in (0x0300_7000u32..0x0300_7FFC).step_by(4) {
            if ram.r32(a) != it {
                continue;
            }
            let pc = ram.r32(a + 4);
            let off = match addr {
                Some(base) if pc >= *base && pc <= base + code.len() as u32 => Some((pc - base) as usize),
                None if pc >> 24 == 2 => (0..=code.len().min(pc as usize)).find(|&o| ram.read(pc - o as u32, code.len()) == *code),
                _ => None,
            };
            // (two interpreter slots can point into the node: the furthest is where it stands - Lookout s512)
            if off.is_some() && off > pc_off {
                pc_off = off;
            }
        }
    }
    natives_before(code, pc_off?)
}

/// the game's interpreter frame running event `ev`'s node (the context stack 0x03001034: +0 depth, then the frames;
/// the one whose PC lies in the node's code, the deepest such) and the PC's offset in that code
fn node_frame(ram: &GbaRam, runner: &StoryHost, ev: (u16, u16, [i32; 4])) -> Option<(u32, usize)> {
    let (addr, code) = runner.step_code.get(&(ev.0, ev.1))?;
    let depth = ram.r32(0x0300_1034).min(8);
    let mut at: Option<(u32, usize)> = None;
    for k in 1..=depth {
        let it = ram.r32(0x0300_1034 + 4 * k);
        for a in (0x0300_7000u32..0x0300_7FFC).step_by(4) {
            if ram.r32(a) != it {
                continue;
            }
            let pc = ram.r32(a + 4);
            let off = match addr {
                Some(base) if pc >= *base && pc <= base + code.len() as u32 => Some((pc - base) as usize),
                None if pc >> 24 == 2 => (0..=code.len().min(pc as usize)).find(|&o| ram.read(pc - o as u32, code.len()) == *code),
                _ => None,
            };
            if let Some(off) = off {
                if at.is_none_or(|(_, o)| off > o) {
                    at = Some((it, off));
                }
            }
        }
    }
    at
}

/// the loop counters of the node's running `loop_push` loops (the interpreter frame +0x44 the depth, +0x48 the
/// counters, innermost last)
fn node_loop_counters(ram: &GbaRam, runner: &StoryHost, ev: (u16, u16, [i32; 4])) -> Option<Vec<i32>> {
    let (it, _) = node_frame(ram, runner, ev)?;
    let lp = ram.r32(it + 0x44);
    (ram.valid(it) && (1..=4).contains(&lp)).then(|| (0..lp).map(|k| ram.s32(it + 0x48 + 4 * k)).collect())
}

/// the waiting native's occurrences in the loop iterations the game's interpreter already finished: a native inside a
/// `loop_push` .. `loop` body (the static scan of `natives_before` sees one pass) - the interpreter frame (the context
/// stack's entry, `0x08004038`'s r4: +0x44 the loop depth, +0x48 the loop counters, innermost last) has the counter
/// at (the body's start count) - (iterations done); each finished pass queued the body's matching natives once (run-2
/// cut 50,863: s070 node 23's `loop` of two play_anim(161, 12), the second pass waiting - counter 1 of 2)
fn loop_extra(ram: &GbaRam, runner: &StoryHost, ev: (u16, u16, [i32; 4]), wn: u8, wargs: &Option<Vec<i32>>) -> usize {
    use wf_script::bytecode::Op;
    let Some((_, code)) = runner.step_code.get(&(ev.0, ev.1)) else { return 0 };
    let Some((it, pc_off)) = node_frame(ram, runner, ev) else { return 0 };
    let Ok(insns) = wf_script::bytecode::decode_reachable(code) else { return 0 };
    let insns: Vec<wf_script::bytecode::Insn> = insns.into_values().collect();
    // the innermost loop whose body holds the PC: its `loop` op (back to `t`) after the PC, the body start `t` before
    let Some(end) = insns.iter().filter(|i| matches!(i.op, Op::Loop(t) if t < pc_off && i.off >= pc_off.saturating_sub(1))).min_by_key(|i| i.off) else { return 0 };
    let Op::Loop(t) = end.op else { return 0 };
    let lp = ram.r32(it + 0x44);
    if lp == 0 || lp > 4 || !ram.valid(it) {
        return 0;
    }
    let cur = ram.s32(it + 0x48 + 4 * (lp - 1));
    // the start count: the constant pushed before the body's `loop_push`
    let Some(push) = insns.iter().position(|i| i.op == Op::LoopPush && i.off + i.len == t) else { return 0 };
    let start = match insns.get(push.wrapping_sub(1)).map(|i| &i.op) {
        Some(Op::PushU8(v)) => *v as i32,
        Some(Op::Push(v)) => *v,
        _ => return 0,
    };
    let done = start - cur;
    if done <= 0 {
        return 0;
    }
    // the body's matching natives (one pass)
    let before = natives_before(code, t).map(|v| v.len()).unwrap_or(0);
    let all = natives_before(code, end.off).unwrap_or_default();
    let per = all[before.min(all.len())..].iter().filter(|(n, a)| *n == wn && (wargs.is_none() || a == wargs)).count();
    done as usize * per
}

/// the natives (in order, with constant arguments) a script's code runs before offset `pc_off` (queries left out)
fn natives_before(code: &[u8], pc_off: usize) -> Option<Vec<(u8, Option<Vec<i32>>)>> {
    use wf_script::bytecode::Op;
    let insns: Vec<wf_script::bytecode::Insn> = wf_script::bytecode::decode_reachable(code).ok()?.into_values().filter(|i| i.off < pc_off).collect();
    let mut ran = vec![];
    for (k, i) in insns.iter().enumerate() {
        let Op::Native(n) = i.op else { continue };
        if wf_script::natives::sig(n).pushes && !wf_script::steps::is_result_action(n) || (wf_script::natives::RUN_SCRIPT..=wf_script::natives::RUN_SCRIPT_P0_P1).contains(&n) {
            continue;
        }
        // its arguments when the instructions before it push constants
        let pops = wf_script::natives::sig(n).pops as usize;
        let mut vals: Vec<i32> = vec![];
        for j in insns[..k].iter().rev() {
            if vals.len() >= pops {
                break;
            }
            match &j.op {
                Op::PushU8(v) => vals.insert(0, *v as i32),
                Op::Push(v) => vals.insert(0, *v),
                Op::PushN(vs) => {
                    for v in vs.iter().rev() {
                        vals.insert(0, *v);
                    }
                }
                _ => break,
            }
        }
        let args = (vals.len() >= pops).then(|| vals[vals.len() - pops..].to_vec());
        ran.push((n, args));
    }
    (!ran.is_empty()).then_some(ran)
}

/// **an event stopped inside a code node** (mid-area): the node's natives before the interpreter's PC ran before
/// the cut; the last one is the one waiting. The node runs again from the group that one belongs to - a batch's
/// `begin_parallel` (0x38) when it waits in `end_parallel` (0x3A), else the waiting native itself - with the
/// natives before it dropped; what the replayed group started is then set to the RAM's: the RNG and the screen
/// shakes as they were (shake objects vt 0x0804A720 the game no longer has: their commands dropped), an actor's
/// animation command playing at its RAM cel / tick
fn build_mid_code(ram: &GbaRam, runner: &mut StoryHost, ev: (u16, u16, [i32; 4]), mut w: World, calls0: u32, ran: &[(u8, Option<Vec<i32>>)], notes: &mut Vec<String>) -> Built {
    use wf_core::scene::{Cmd, Owner};
    // the waiting native among our queued actions (a branch the script did not take has natives in the bytecode
    // but not in the queue): its k-th occurrence by id and constant arguments
    let (wn, wargs) = ran[ran.len() - 1].clone();
    let k = std::cell::Cell::new(ran.iter().filter(|(n, a)| *n == wn && (wargs.is_none() || *a == wargs)).count() + loop_extra(ram, runner, ev, wn, &wargs));
    let seen = std::cell::RefCell::new(vec![]);
    let hits = std::cell::Cell::new(0usize);
    // a constant `wait(n)` (0x88) after an action native: only a wait queued right after that native counts - a
    // `wait(rand(5) + 1)` elsewhere in the node draws the same n (run 3 cut 1,072,328: s344 node 65 waits in step 4's
    // `show_energy_sphere(..); wait(1)` loop, pass 5 of 37; step 8's spawn loop's random waits drew 1 too and the
    // replay stopped there, a `spawn_actor(169)` early)
    let after: Option<u8> = (wn == 0x88 && wargs.is_some() && ran.len() >= 2)
        .then(|| ran[ran.len() - 2].0)
        .filter(|&p| !wf_script::natives::sig(p).pushes && p != 0x88);
    let use_after = std::cell::Cell::new(after.is_some());
    let pred = |n: u8, a: &[i32]| {
        let m = n == wn && wargs.as_ref().is_none_or(|x| x.as_slice() == a) && (!use_after.get() || seen.borrow().last().copied() == after);
        if m {
            hits.set(hits.get() + 1);
        }
        let hit = m && hits.get() == k.get();
        if !hit {
            seen.borrow_mut().push(n);
        }
        hit
    };
    let (w_probe, snap_probe) = (w.clone(), runner.snapshot());
    let mut found = runner.probe_queued(&mut w, ev.0, ev.1, ev.2, &pred);
    if found.is_none() && use_after.get() {
        w = w_probe.clone();
        runner.restore(&snap_probe);
        use_after.set(false);
        seen.borrow_mut().clear();
        hits.set(0);
        found = runner.probe_queued(&mut w, ev.0, ev.1, ev.2, &pred);
    }
    // the count above takes every matching native in the bytecode before the PC, a branch the node did not take
    // included: when the probe's run reached fewer, the waiting one is the last it reached (run 2 cuts 859,325 ..
    // 860,353: s418 node 0 waits in the flag-272 branch's end_parallel, the flag-271 branch's comes before it)
    if found.is_none() && (1..k.get()).contains(&hits.get()) {
        // (from the world before that probe: it ran the node through - s418's flag writes after its wait)
        w = w_probe;
        runner.restore(&snap_probe);
        k.set(hits.get());
        seen.borrow_mut().clear();
        hits.set(0);
        found = runner.probe_queued(&mut w, ev.0, ev.1, ev.2, &pred);
    }
    // a `play_anim` on the leader that transformed it in the cut's frame (the power-up, script anim 23: the RAM's
    // form is the new one, the wait ends in the next frame): the node's queries before it (`is_super_saiyan`) read
    // the form before - the node is replayed with the leader in that form (its `down`), the RAM's put back after
    // (run 2 cut 56,863: s102 node 5, Goku's power-up to Super Saiyan)
    let mut form_after = None;
    if found.is_none() && wn == 0x3C && wargs.as_ref().is_some_and(|a| a.first() == Some(&0)) {
        if let Some(pi) = w.player_idx(0) {
            let f = w.pl_ref(pi).form;
            if let Some(down) = w.data.form(f).map(|d| d.down).filter(|&d| (0..255).contains(&d)) {
                w.pl(pi).form = down;
                seen.borrow_mut().clear();
                hits.set(0);
                found = runner.probe_queued(&mut w, ev.0, ev.1, ev.2, &pred);
                if found.is_some() {
                    form_after = Some((pi, f));
                } else {
                    w.pl(pi).form = f;
                }
            }
        }
    }
    // the probe cannot see the waiting native: (a) a `run_event` whose event object is on the stack but has not
    // stepped yet (its state word 0 - whole-run cut 612,541: s508 node 1 ran `run_event(494, ..)`, s494's node 0
    // runs in the next frame): the node continues past its actions before it, the nested event from its start;
    // (b) a `steps` node past a modal call (`is_result_action`: its result fed to the next step) the probe stops at
    let modal = ran[..ran.len() - 1].iter().rposition(|(n, _)| wf_script::steps::is_result_action(*n));
    let nested_new = found.is_none()
        && (wf_script::natives::RUN_EVENT..=wf_script::natives::RUN_EVENT_P0_P3).contains(&wn)
        && wargs.as_ref().and_then(|a| a.first()).is_some_and(|&e| (0x0300_7000u32..0x0300_8000 - 0x18).step_by(4).any(|a| runner.event_of_header(ram.r32(a)) == Some(e as u16) && ram.r16(a + 8) == 0));
    let (rng, shakes, g0) = (w.rng.clone(), w.g.shakes.clone(), w.story.g);
    // (the leader's state as imported from RAM, before the replayed group's commands)
    let leader_gs = w.player_idx(0).map(|pi| (pi, w.pl_ref(pi).gs));
    let (last, from) = match found {
        Some((idx, _)) => {
            let queued = seen.into_inner();
            let last = idx;
            // (`idx` counts every action the probe passed, `queued` only the ones it asked about - a node's `run_event` is
            // not among them: s314 node 0, whole-run cuts 225,250 .. 225,340 - the batch's begin_parallel is offset by those)
            let off = idx.saturating_sub(queued.len());
            // (the probe's own act list - natives and the other acts its queue held, `StoryHost::probe_acts` - places
            // the batch's begin_parallel exactly: an act not asked about may come after it too, run 2 cut 64,449:
            // s125 node 6's begin_parallel then set_energy)
            let acts = runner.probe_acts.clone();
            let from = if wn != 0x3A {
                idx
            } else if acts.len() == idx {
                acts.iter().rposition(|a| *a == Some(0x38)).unwrap_or(idx)
            } else {
                queued[..idx.min(queued.len())].iter().rposition(|&n| n == 0x38).map(|p| p + off).unwrap_or(idx)
            };
            let outer = outer_event(ram, runner, ev);
            if let Some((o, k)) = outer {
                notes.push(format!("nested in s{:03} node {} (its run_event after {k} actions)", o.0, o.1));
            }
            // the replay re-runs the node's code before the waiting native: its `rand()` draws were made by the game before
            // the cut, so the replay starts from the generator rewound by them - found by the script words g0..g3 the replay
            // sets matching the RAM's (whole-run cut 628,017: s508 node 7, set_pos(-1 - g(0), 262 + rand(53), 78 + rand(134))
            // waiting, the map object already placed)
            let ram_g: Vec<i32> = (0..4u32).map(|k| ram.s32(WORDS_G + 4 * k)).collect();
            let snap = runner.snapshot();
            let mut chosen = None;
            for m in 0..=64u32 {
                let mut w2 = w.clone();
                w2.rng.set_state(rewind(rng.state(), m));
                match outer {
                    Some((o, k)) => runner.continue_mid_nested_in(&mut w2, o, k, from, (ev.0, ev.1)),
                    None => runner.continue_mid_node(&mut w2, ev.0, ev.1, ev.2, from, last),
                }
                if w2.story.g[..4] == ram_g[..] {
                    chosen = Some(w2);
                    break;
                }
                runner.restore(&snap);
            }
            match chosen {
                Some(w2) => w = w2,
                None => match outer {
                    Some((o, k)) => runner.continue_mid_nested_in(&mut w, o, k, from, (ev.0, ev.1)),
                    None => runner.continue_mid_node(&mut w, ev.0, ev.1, ev.2, from, last),
                },
            }
            // a waiting native under a condition inside a loop (`if rand(100) + 1 > 20 { screen_flash(..) }`): the
            // k-th occurrence the probe stopped at is not the game's pass - the replay's draws took other branches. A
            // steps node keeping one local across its waits, that local the one loop's counter (`n0`), takes the RAM's
            // count (run 3 cut 1,071,990: s344 node 65's 15 puffs, the game at counter 7, the replay 2 passes on)
            if outer.is_none() {
                if let Some(&[c]) = node_loop_counters(ram, runner, ev).as_deref() {
                    if runner.set_single_local(c) {
                        notes.push(format!("mid-code: the node's loop counter from RAM ({c})"));
                    }
                }
            }
            (last, from)
        }
        None if nested_new => {
            notes.push(format!("mid-area: s{:03} node {} waits in a run_event whose event has not stepped yet", ev.0, ev.1));
            runner.continue_mid_nested_at(&mut w, ev, ran.len() - 1, 0, false);
            (ran.len() - 1, 0)
        }
        None if modal.is_some() => {
            let r = modal.unwrap_or(0);
            let step = ran[..=r].iter().filter(|(n, _)| wf_script::steps::is_result_action(*n)).count() as i32;
            let rest = &ran[r + 1..];
            let idx = rest.len() - 1;
            let from = if wn == 0x3A { rest[..idx].iter().rposition(|(n, _)| *n == 0x38).unwrap_or(idx) } else { idx };
            // (the modal's result: the node keeps it in p(0) before its next native - s266's `set_p(0, result)`)
            if !runner.continue_mid_step(&mut w, ev.0, ev.1, ev.2, step, ev.2[0], from) {
                notes.push(format!("unsupported: event s{:03} node {}: past a modal, not a steps node", ev.0, ev.1));
                let draws = w.rng.calls.wrapping_sub(calls0);
                return Built { world: w, draws, held: 0, finished: false };
            }
            notes.push(format!("mid-area: s{:03} node {} at step_{step} (past its modal), action {from}", ev.0, ev.1));
            (idx, from)
        }
        None => {
            notes.push(format!("unsupported: event s{:03} node {}: the waiting native {wn:#04x} is not among the node's actions", ev.0, ev.1));
            let draws = w.rng.calls.wrapping_sub(calls0);
            return Built { world: w, draws, held: 0, finished: false };
        }
    };
    // (the transformation's anim: done in the cut's frame - its command goes, the leader is the RAM's again)
    if let Some((pi, f)) = form_after {
        let id = w.ents[pi].id;
        let mut pl = w.pl_ref(pi).clone();
        pl.apply_form(&w, f);
        *w.pl(pi) = pl;
        for t in w.scene.tracks.iter_mut() {
            if t.owner == Owner::Entity(id) && matches!(t.queue.first(), Some(Cmd::PlayAnim { .. })) {
                t.queue.remove(0);
                t.started = false;
            }
        }
        patch_leader(&mut w, ram, notes);
        import_player_state(&mut w, ram, notes);
        if w.scene.tracks.iter().all(|t| t.queue.is_empty()) {
            w.scene.director.wait = None;
            w.scene.director.done = true;
        }
        notes.push(format!("mid-area: s{:03} node {}: the leader's transformation anim ended in the cut's frame", ev.0, ev.1));
    }
    w.rng = rng;
    // the script words the replayed part set (`set_g(0, rand(3))` - s508 node 1: its draw replays on the cut's
    // generator, another value) are the RAM's - when the whole replayed part ran before the cut (a run_event not yet
    // stepped: nothing after it has run); elsewhere code past the cut sets them too (s512's)
    if nested_new {
        w.story.g = g0;
        runner.set_script_g(g0);
    }
    // `wait(n)` (0x88): the main loop's mode is the wait object (0x03000F90 -> vt 0x08049978, +4 frames left)
    let mode = ram.r32(0x0300_0F90);
    if wn == 0x88 && ram.valid(mode) && ram.r32(mode) == 0x0804_9978 {
        if let Some(wf_script_core::Pending::Frames { left, .. }) = runner.hs.pending.as_mut() {
            *left = ram.s32(mode + 4);
        }
    }
    // `warp_fade` (0x76) waiting in its slow fade-out (the main loop's mode vt 0x0804BBA4: +0x404 the steps run, +0x408
    // the length 64): the replayed call restarts the fade (k = -1, `field::warp_fade`) - it takes the RAM's steps
    // (run 2 cut 1,858,572: s479 node 4's warp_fade to 0-50 a step in at the cut - ours loaded a frame late)
    if wn == 0x76 && ram.valid(mode) && ram.r32(mode) == 0x0804_BBA4 {
        let run = ram.s32(mode + 0x404);
        if let Some(Mode::FadeOut { k, step: 4, .. }) = w.field.as_mut().map(|f| &mut f.mode) {
            *k = run;
        }
    }
    // a fade_out (0x78) among the dropped natives, not undone by a fade_in after it: the screen stays black (whole-run
    // cut 568,177: s341 node 0 waits on its set_pos after fade_out(50))
    if ran[..ran.len() - 1].iter().rev().find(|(n, _)| (0x78..=0x79).contains(n)).is_some_and(|(n, _)| *n == 0x78) && w.scene.screen.fade.is_none() {
        w.scene.screen.black = 16;
        w.scene.screen.dim = 256;
    }
    // a script fade waiting (0x78 .. 0x7e): the main loop's mode is the fade object (vt 0x0804C588: +4 the steps run,
    // +8 the length - opening s103 node 18's fade_in(100), whole-run cuts 41,565 / 41,625; s314's fade_in(50) 225,225):
    // the replayed fade restarted from its first step - it takes the RAM's progress
    // (the palette-copy fade object vt 0x0804BBA4 the same, its numbers at +0x404 / +0x408 - run 2 cut 1,853,852: s478
    // node 0's fade_out(50) 49 steps in; ours ran it again, the whole ending 50 frames late)
    fade_progress_from_ram(&mut w, ram, wn);
    // a `load_scene*` whose load the game already made (the field is its target): the replay restarted it from its
    // fade-out - it goes on at its fade-in, with the RAM's progress (the fade mode vt 0x0804C588: +4 steps run, +8
    // length), or past it (run 1 cut 202,593: s260 node 7's load_scene_fade(6, 1, ..) waiting 3 steps into its
    // fade-in over 6-1 - ours faded out and loaded 6-1 again)
    if let Some(wf_script_core::Pending::Phases { list, .. }) = runner.hs.pending.clone() {
        let here = w.field.as_ref().map(|f| (f.area.world, f.area.area));
        let j = list.iter().position(|p| matches!(p, wf_script_core::Phase::LoadScene { world, map, .. } if Some((*world, *map)) == here));
        if let Some(j) = j {
            w.scene.screen.fade = None;
            w.scene.screen.black = 0;
            w.scene.screen.dim = 0;
            let fading = ram.valid(mode) && ram.r32(mode) == 0x0804_C588;
            match list.get(j + 1).cloned() {
                Some(wf_script_core::Phase::Fade { kind, out: false, frames }) if fading => {
                    wf_core::scene::start_fade(&mut w, kind, false, frames);
                    if let Some(f) = w.scene.screen.fade.as_mut() {
                        f.t = ram.s32(mode + 4).clamp(0, f.len);
                        let (t, len) = (f.t, f.len.max(1));
                        w.scene.screen.black = 16 - (16 * t / len).clamp(0, 16);
                        w.scene.screen.dim = 256 - (256 * t / len).clamp(0, 256);
                    }
                    runner.hs.pending = Some(wf_script_core::Pending::Phases { list, k: j + 1, started: true });
                }
                _ => {
                    let n = list.len();
                    runner.hs.pending = Some(wf_script_core::Pending::Phases { list, k: n, started: false });
                }
            }
            notes.push(format!("mid-area: a load_scene already made (phase {j}): on at its fade-in"));
        }
    }
    // the screen shake (the camera slot 0x03002BE0 -> vt 0x0804A720: +4 intensity, +5 period, +6 count, +8 frames
    // left): the group's shake commands before the running one are done, the running one is the RAM's
    w.g.shakes = shakes;
    let so = ram.r32(0x0300_2BE0);
    let live = (ram.valid(so) && ram.r32(so) == 0x0804_A720).then(|| wf_core::world::Shake { intensity: ram.r8(so + 4) as i32, period: ram.r8(so + 5) as i32, cnt: ram.r8(so + 6) as i32, left: ram.s32(so + 8) });
    let shake_cmds: Vec<(i32, i32)> = ram
        .objects()
        .into_iter()
        .map(|o| ram.r32(o + 0x18C))
        .filter(|&c| ram.valid(c) && ram.r32(c) == 0x0804_C828 && ram.s32(c + 0xC) > 0)
        .map(|c| (ram.s32(c + 8), ram.s32(c + 0xC)))
        .collect();
    for t in w.scene.tracks.iter_mut() {
        if !t.queue.iter().any(|c| matches!(c, Cmd::Shake { .. })) {
            continue;
        }
        let cur = live.as_ref().and_then(|l| t.queue.iter().position(|c| matches!(c, Cmd::Shake { intensity, .. } if *intensity == l.intensity)));
        // (the shake object gone, its command still on the actor: the actor's +0x18C -> vt 0x0804C828, +8 the
        // intensity, +0xC the frames left - the script waits on it, the next command not started: run 3 cut 1,072,000,
        // s344 node 65's shake_screen(0, 100, 5) one frame from its end, then actor_wait(169, 15))
        let by_cmd = cur.is_none().then(|| shake_cmds.iter().find_map(|&(i, left)| t.queue.iter().position(|c| matches!(c, Cmd::Shake { intensity, .. } if *intensity == i)).map(|k| (k, left)))).flatten();
        match cur {
            Some(k) => {
                t.queue.drain(..k);
                if let (Some(Cmd::Shake { frames, t: tt, .. }), Some(l)) = (t.queue.first_mut(), live.as_ref()) {
                    // (its start frame ticked it once: t = frames - 1 - left)
                    *tt = *frames - 1 - l.left;
                }
                t.started = true;
            }
            None if by_cmd.is_some() => {
                let (k, left) = by_cmd.unwrap();
                t.queue.drain(..k);
                if let Some(Cmd::Shake { frames, t: tt, .. }) = t.queue.first_mut() {
                    *tt = (*frames - left).max(0);
                }
                t.started = true;
            }
            None => {
                t.queue.retain(|c| !matches!(c, Cmd::Shake { .. }));
                t.started = false;
            }
        }
    }
    if let Some(l) = live {
        // (the mid-area import already took it: once)
        w.g.shakes.retain(|s| *s != l);
        w.g.shakes.push(l);
    }
    // a parallel group the cut waits in (end_parallel, 0x3A): the puffs at the head of a track ran when the group
    // started (instant commands chain in one tick) - their clouds are the RAM's effects (`import_effects`), the
    // replayed commands are dropped (run 2 cut 58,440: s103 node 0's shake_screen + four puff_offset(224, ..) waiting
    // at end_parallel - ours made four more clouds, a draw each)
    if wn == 0x3A {
        for t in w.scene.tracks.iter_mut() {
            let n = t.queue.iter().take_while(|c| matches!(c, Cmd::Puff { .. })).count();
            if n > 0 {
                t.queue.drain(..n);
                t.started = false;
            }
        }
    }
    w.scene.tracks.retain(|t| !t.queue.is_empty() || t.holding);
    // `become_map_object` waiting (0x4F): the RAM already has the map object (imported). Its command object (vt
    // 0x0804DEA0) still on the map object's command list (+0x18C): it ran its start in the frame before the cut and
    // completes on the track's next tick - the wait ends in the cut's frame, the next command starts a frame later
    // (run-2 cut 39,582: s088 node 2's four becomes 2 frames apart, 209 made the frame before the cut, 213 at f1) - the
    // replayed command stays as started, not run again. Gone: the wait ended before the cut, the script goes on in the
    // cut's frame (r062 20-6 +21,000: s476 node 19 exits in the cut's first frame) - dropped, done
    // (u2: the map object the command made in the cut's frame - +0x18C -> vt 0x0804DEA0, +8 the map-object index -
    // owns the started track: run 2 cut 33,448, s086 node 2's become_map_object(209, 1))
    if wn == 0x4F {
        let fresh: Vec<(i32, i32, i32)> = ram
            .objects()
            .into_iter()
            .filter(|&o| ram.r32(o) == VT_ENEMY && ram.valid(ram.r32(o + 0x18C)) && ram.r32(ram.r32(o + 0x18C)) == 0x0804_DEA0)
            .map(|o| (ram.s32(ram.r32(o + 0x18C) + 8), ram.s32(o + 0x1A0), ram.s32(o + 0x1A4)))
            .collect();
        let ents: Vec<(wf_core::world::EntityId, i32, i32)> = w
            .ents
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.removed && matches!(e.body, Body::Enemy(_)))
            .filter_map(|(i, e)| w.actor(i).map(|a| (e.id, a.x, a.y)))
            .collect();
        let pending = ram.objects().into_iter().any(|o| {
            let c = ram.r32(o + 0x18C);
            ram.valid(c) && ram.r32(c) == 0x0804_DEA0
        });
        for t in w.scene.tracks.iter_mut() {
            if let Some(Cmd::BecomeMapObject { index }) = t.queue.first() {
                if pending {
                    if let Some(&(id, _, _)) = fresh.iter().find(|f| f.0 == *index).and_then(|f| ents.iter().find(|e| e.1 == f.1 && e.2 == f.2)) {
                        t.owner = Owner::Entity(id);
                    }
                    t.started = true;
                } else {
                    t.queue.remove(0);
                    t.started = false;
                }
            }
        }
        if !pending && w.scene.tracks.iter().all(|t| t.queue.is_empty()) {
            w.scene.director.wait = None;
            w.scene.director.done = true;
        }
    }
    // each track's running command from its game object (heap objects matched by their target): a walk (vt
    // 0x0804D0A4; a glide 0x0804AE84 the same: +0xC / +0x10 24.8 step, +0x14 / +0x18 target, +0x1C steps left), a jump (vt 0x0804AE5C: +8 x
    // step, +0xC y velocity after the last step, +0x10 / +0x14 24.8 position, +0x18 / +0x1C target, +0x38 air
    // frames, +0x3C frames run: 8 crouch, `air` of flight, 8 landing). The commands before it in its track are
    // done: dropped, it starts (its start effects: the walk state, the facing) and takes the object's progress
    let rng = w.rng.clone();
    import_track_objects(&mut w, ram);
    // a camera pan in progress (run 2 cut 57,831: s103 node 0's camera_pan_to(224, 176, 1000) after its fade_out)
    import_cam_pan(&mut w, ram);
    import_prop_moves(&mut w, ram);
    import_emote_cmds(&mut w, ram);
    import_flash_cmds(&mut w, ram);
    // an enemy the script holds in a pose (its state +0x14 the hold state 0x03001B00, `set_pose`) whose `set_pose` the
    // replay dropped: our puppet on the RAM's cel (run 2 cuts 57,831 .. 58,560: s103's Pikkon, pose 12 then 16; else
    // ours ran its AI animation, whose script draws)
    held_enemy_puppets(&mut w, ram);
    // a replayed `play_anim` on the leader that already ended in the game (the RAM's leader is back in the state it
    // imported with: whole-run cut 153,226, s266's punch anim done, the group still waiting on its prop poof): the
    // command is done, the leader's state the RAM's
    // (only when the leader's command list is empty: a play_anim still running in the game keeps its command object
    // at +0x18C - run 3 cuts 110,180 / 210,621: s003 node 0's level-up pose, the leader imported in the anim's state)
    if let Some((pi, gs)) = leader_gs.filter(|_| ram.player() != 0 && ram.r32(ram.player() + 0x18C) == 0) {
        let me = w.ents[pi].id;
        if w.pl_ref(pi).gs != gs {
            let mut hit = false;
            for t in w.scene.tracks.iter_mut() {
                if t.owner == Owner::Entity(me) && matches!(t.queue.first(), Some(Cmd::PlayAnim { anim }) if *anim != 0) {
                    t.queue.remove(0);
                    t.started = false;
                    hit = true;
                }
            }
            if hit {
                wf_core::player::import_state(&mut w, pi, gs);
                if let Some(r) = ram.actor_rows.iter().find(|r| r.0 == ram.player()) {
                    let p = w.pl(pi);
                    (p.ap.raw, p.ap.frame, p.ap.tick) = (r.2, r.3, r.4);
                }
            }
        }
    }
    // a replayed `play_anim` / item-get presentation (0x58 / 0x80 / 0x93) whose game command is gone (the actor's
    // command list +0x18C empty: it ended and the wait with it in the frame before the cut - the script goes on in the
    // cut's frame; run-2 cut 50,830: s070 node 23's loop of two play_anim(161, 12), the second starts at f0; 62,068:
    // s110 node 3's give_item(1) done, node 4's text box opens at f0): done, the wait ended, the actor as the RAM has it
    // (a `face` (0x53) the same: its command gone from the actor's list, the turn done - run 3 cut 11,327: s061 node 0's
    // face(0, 1) ended the frame before, its next action spawns the actor in the cut's frame)
    if matches!(wn, 0x3C | 0x58 | 0x80 | 0x93 | 0x53) {
        let mut dropped = false;
        for k in 0..w.scene.tracks.len() {
            let Owner::Entity(id) = w.scene.tracks[k].owner else { continue };
            let face = wn == 0x53 && matches!(w.scene.tracks[k].queue.first(), Some(Cmd::Face { .. }));
            if !face && !matches!(w.scene.tracks[k].queue.first(), Some(Cmd::PlayAnim { anim }) if *anim != 0) && !matches!(w.scene.tracks[k].queue.first(), Some(Cmd::ItemGet { .. })) {
                continue;
            }
            let Some(i) = w.find(id) else { continue };
            let Some((x, y)) = w.actor(i).map(|a| (a.x, a.y)) else { continue };
            let leader = w.player_idx(0).filter(|&pi| pi == i);
            let o = if leader.is_some() {
                Some(ram.player())
            } else {
                ram.objects().into_iter().find(|&o| matches!(ram.r32(o), VT_NPC | VT_NPC_TALK | VT_ENEMY) && ram.s32(o + 0x1A0) == x && ram.s32(o + 0x1A4) == y)
            };
            if !o.is_some_and(|o| o != 0 && ram.r32(o + 0x18C) == 0) {
                continue;
            }
            w.scene.tracks[k].queue.remove(0);
            w.scene.tracks[k].started = false;
            dropped = true;
            // (the item icon the replayed start showed, the NPC's held item)
            w.scene.visuals.retain(|v| !(matches!(v.kind, wf_core::scene::VisualKind::ItemIcon(_)) && v.owner == id));
            if let Body::Npc(n) = &mut w.ents[i].body {
                n.hold_item = None;
            }
            if let (Some(pi), Some((_, gs))) = (leader, leader_gs) {
                wf_core::player::import_state(&mut w, pi, gs);
                if let Some(r) = ram.actor_rows.iter().find(|r| r.0 == ram.player()) {
                    let p = w.pl(pi);
                    (p.ap.raw, p.ap.frame, p.ap.tick) = (r.2, r.3, r.4);
                }
            }
        }
        if dropped && w.scene.tracks.iter().all(|t| t.queue.is_empty()) {
            w.scene.director.wait = None;
            w.scene.director.done = true;
        }
    }
    // an `end_parallel` (0x3A) whose batch is over in the game: each track's command an actor's animation or pose
    // (`play_anim` / `set_pose`) and every actor's command list +0x18C empty - the wait ended in the frame before the
    // cut and the script goes on in the cut's frame (run 3 cut 1,796,423, fat Buu s021 node 0: the batch's play_anim(4)
    // over, the next batch's glide of the leader starts at f0 - ours replayed the batch and glided a frame late)
    if wn == 0x3A && !w.scene.tracks.is_empty() {
        let over = w.scene.tracks.iter().all(|t| {
            let Owner::Entity(id) = t.owner else { return false };
            if !matches!(t.queue.first(), Some(Cmd::PlayAnim { .. } | Cmd::SetPose { .. })) {
                return false;
            }
            let Some(i) = w.find(id) else { return false };
            let Some((x, y)) = w.actor(i).map(|a| (a.x, a.y)) else { return false };
            let o = if w.player_idx(0) == Some(i) {
                Some(ram.player())
            } else {
                ram.objects().into_iter().find(|&o| matches!(ram.r32(o), VT_NPC | VT_NPC_TALK | VT_ENEMY) && ram.s32(o + 0x1A0) == x && ram.s32(o + 0x1A4) == y)
            };
            o.is_some_and(|o| o != 0 && ram.r32(o + 0x18C) == 0)
        });
        if over {
            for t in w.scene.tracks.iter_mut() {
                t.queue.clear();
                t.started = false;
            }
            w.scene.director.wait = None;
            w.scene.director.done = true;
            if let Some((pi, gs)) = leader_gs {
                wf_core::player::import_state(&mut w, pi, gs);
                if let Some(r) = ram.actor_rows.iter().find(|r| r.0 == ram.player()) {
                    let p = w.pl(pi);
                    (p.ap.raw, p.ap.frame, p.ap.tick) = (r.2, r.3, r.4);
                }
            }
        }
    }
    w.scene.tracks.retain(|t| !t.queue.is_empty() || t.holding);
    // a prop's poof playing (the command object vt 0x0804E0FC, +8 its frame count: done at 8 x its steps - whole-run
    // cut 153,226: s266's machine poof 32 frames in): the replayed one at the RAM's count
    let mut poofs = ram.ewram_with(0x0804_E0FC).into_iter();
    for t in w.scene.tracks.iter_mut() {
        if let Some(Cmd::PropPoof { t: pt, .. }) = t.queue.first_mut() {
            if let Some(o) = poofs.next() {
                *pt = ram.s32(o + 8);
            }
        }
    }
    w.rng = rng;
    // the track actors' animations as the RAM rows show them (a started command restarted its animation): the
    // RAM object at the entity's 24.8 position
    track_anims_from_rows(&mut w, ram);
    // the camera mover (0x31 camera_center_on / 0x32 camera_scroll_to: the camera slot's object vt 0x0804AB24, +4 /
    // +8 the 22.10 step, +0xC the steps left): the replayed scroll takes the RAM's step and count, the camera the
    // RAM's position (the replay's start measured its steps from the camera at the cut; real run 007, whole-run cuts
    // 756,956 .. 756,974: s365's camera_center_on(223, 2))
    let slot = ram.r32(0x0300_2BE0);
    if ram.valid(slot) && ram.r32(slot) == 0x0804_AB24 {
        if let Some(s) = w.scene.cam.scroll.as_mut() {
            (s.sx, s.sy, s.n, s.t) = (ram.s32(slot + 4), ram.s32(slot + 8), ram.s32(slot + 0xC), 0);
            wf_core::battle::set_camera(&mut w, 0, ram.s32(CAM), ram.s32(CAM + 4));
        }
    }
    notes.push(format!("event s{:03} at node {}: mid-code, action #{last} {wn:#04x} waiting (replayed from #{from})", ev.0, ev.1));
    let draws = w.rng.calls.wrapping_sub(calls0);
    Built { world: w, draws, held: 0, finished: true }
}

/// the 3D world map (an area whose load makes the flyer)
/// the 3-D map's arrival fade-in (main-loop mode object vtable)
const MODE_FADE_IN_3D: u32 = 0x0804_95B4;

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
    let cfg = FieldConfig { world: to.world, area: to.area, variant: to.variant, pos: Some((x, y)), facing, chars: vec![story.current], story: Some(story.clone()), rng: wf_core::battle::RngInit::State(rng0[0], rng0[1]), xp: true, arrive: false, slow_fade_in: false };
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
    // the last area name shown (0x03001984, u16: kept across loads - a reload of the same name shows no banner; run 1
    // cut 955,163: the event reloads 6-1 after its scene map, Kami's Lookout already shown)
    w.scene.banner_last = ram.r16(0x0300_1984) as i64;
    w.rng = Rng::new(rng0[0], rng0[1]);
    w.events.clear();
    let calls0 = w.rng.calls;
    // the event holds the field (a load inside it keeps it held)
    w.scene.depth = 1;
    field::set_busy(&mut w, true);
    let areat = (to.world, to.area, to.variant);
    // which load: the node's own warp / warp_fade into this area, or (none in the node) the one before a `jump`
    // the interpreter already past the node's warp (a later wait in it - Kami's Lookout s512's fade_in_white after its
    // warp to 6-14): the field stands, the node continues mid-step
    // (a cut at the load itself - the screen still black, the load path: opening 1-1, s038's set_pos after its warp)
    // (a black screen with the node past its warp and waiting in a fade_in started after it - the actors it spawned
    // are in RAM: whole-run cut 225,200, s314 node 0's fade_in(50) in its first step - is mid-node too)
    let fade_after_warp = |ran: &Vec<(u8, Option<Vec<i32>>)>| {
        ran.last().is_some_and(|(n, _)| (0x78..=0x7E).contains(n)) && ran[..ran.len() - 1].iter().any(|(n, _)| matches!(n, 0x76 | 0x77))
    };
    // an event its trigger / talk started in the cut's frame (`fresh_scene`): its first node over the standing field
    if ev.1 == 0 && ram.mode_vt() == MODE_SCENE && natives_run(ram, runner, ev).is_none() {
        let obj = ram.r32(ram.r32(MODE) + 4);
        if fresh_scene(ram, obj) {
            return build_mid_event(data, ram, runner, ev, w, calls0, notes);
        }
    }
    // (a black screen the node's own fade_out made, no fade_in after it and nothing loaded since: the field stands -
    // run 2 cut 57,831, s103 node 0's camera_pan_to after its fade_out(25))
    let black_by_fade_out = |ran: &Vec<(u8, Option<Vec<i32>>)>| {
        let before = &ran[..ran.len().saturating_sub(1)];
        let last_fade = before.iter().rposition(|(n, _)| (0x78..=0x79).contains(n));
        last_fade.is_some_and(|i| before[i].0 == 0x78 && !before[i..].iter().any(|(n, _)| (0x70..=0x77).contains(n)))
    };
    // (or black because the fade_out the node waits in is at its end: the field stands - run 2 cut 1,853,852)
    let waiting_fade_out = |ran: &Vec<(u8, Option<Vec<i32>>)>| ran.last().is_some_and(|(n, _)| *n == 0x78) && script_fade_progress(ram).is_some();
    if let Some(ran) = natives_run(ram, runner, ev).filter(|r| ram.screen_black != Some(true) || fade_after_warp(r) || black_by_fade_out(r) || waiting_fade_out(r)) {
        // (a return_to_field (0x75) the node is past - its fade-in over, a later wait: run 2 cut 58,440, s103 node 0 in
        // its director wait 90 frames after the return; run 2 cuts 58,380 / 58,560: its glide and play_anim after the
        // return - is mid-node over the restored field too)
        if ran[..ran.len() - 1].iter().any(|(n, _)| matches!(n, 0x75 | 0x76 | 0x77)) {
            return build_mid_event(data, ram, runner, ev, w, calls0, notes);
        }
        // inside a cutscene map the node loaded (`load_scene` 0x71 run, no `return_to_field` yet, no player object):
        // the scene map with its actors from RAM (run 2 cut 57,951: s103 node 0's Goku and Pikkon over 0-22); the field
        // the scene saved is imported for its `return_to_field` (`import_saved_field`)
        // (a scene map loaded without saving the field - `load_scene` / `load_scene_fade` 0x72 .. 0x74 with no 0x71: a
        // chapter title card, run 3 cuts 3,784 / 4,181, s516's 0-96 mid `fade_in(100)` - is the same: nothing to return to)
        if ram.player() == 0 && ran[..ran.len() - 1].iter().any(|(n, _)| (0x71..=0x74).contains(n)) {
            notes.push("mid-area: inside a load_scene cutscene map".into());
            // (the replay of the node's code before its load_scene sees the room it left - the world-state header the
            // save copied, 0x03001968: s415 node 0's `set_p(0, get_map())` at 14-17 picks its branch by the room)
            let left = (ram.r8(0x0300_1968) as i32, ram.r8(0x0300_1969) as i32, ram.r8(0x0300_196A) as i32);
            let saved = ram.valid(ram.r32(OBJW + 0xBF8));
            if saved {
                runner.hs.area_override = Some(left);
            }
            let b = build_mid_event(data, ram, runner, ev, w, calls0, notes);
            runner.hs.area_override = None;
            return b;
        }
        // the node has run actions and none of them loads (its warp, if any, is still ahead): the event is running
        // mid-node over the field it started on, not at an area entry (real route 4-1, s515's take-off from a
        // trigger: tiles r002_006+2000 / +13000 cut in its glide, the warp_fade to 0-50 two steps later)
        if !ran.is_empty() && !ran.iter().any(|(n, _)| (0x70..=0x77).contains(n)) {
            return build_mid_event(data, ram, runner, ev, w, calls0, notes);
        }
    }
    let probe = runner.probe_event(&mut w, ev.0, ev.1, ev.2, areat);
    // no leader in RAM yet (a cut inside the load's frozen frames: the player object is made at the arrival): the
    // node's own warp places it - its x / y (run 3 cut 139,161, s113 node 0's warp(2, 8, 0, 248, 248); ours put the
    // leader at the default 120, 120)
    let (x, y) = if info.leader.is_none() && probe.is_some() {
        let at = std::cell::Cell::new(None);
        runner.probe_event_for(&mut w, ev.0, ev.1, ev.2, &|n, args| {
            let hit = matches!(n, 0x76 | 0x77) && args.len() >= 5 && (args[0], args[1]) == (areat.0, areat.1);
            if hit {
                at.set(Some((args[3], args[4])));
            }
            hit
        });
        at.get().unwrap_or((x, y))
    } else {
        (x, y)
    };
    // a return_to_field (0x75) in the node: the field the scene left, restored - a mid-area state (`build_returned`)
    if probe.is_none() {
        if let Some((k, _)) = runner.probe_event_for(&mut w, ev.0, ev.1, ev.2, &|n, _| n == 0x75) {
            return build_returned(data, ram, runner, ev, k, w, calls0, notes);
        }
    }
    // an event in its first node that started inside the frame's object pass (the world list's update call
    // 0x08011A5E under the VM: a defeat callback / hook, not an area's entry) runs over a standing field
    if probe.is_none() && ev.1 == 0 && (0x0300_7000u32..0x0300_8000).step_by(4).any(|a| ram.r32(a) == 0x0801_1A5F) {
        return build_mid_event(data, ram, runner, ev, w, calls0, notes);
    }
    let fading = match probe {
        Some((_, 0x76)) => true,
        Some(_) => false,
        // (the modal scene start a trigger / talk set - vt 0x08049D18, the leader standing, the screen lit - is no load's
        // on-enter: its event runs over the standing field; run 3 cut 7,357, 1-1's s039: node 0's text box, its scene
        // object already past its first state)
        None if ev.1 == 0 && !is_3d(data, to) && !(ram.mode_vt() == MODE_SCENE && ram.player() != 0 && ram.screen_black != Some(true)) => false,
        // no load in the node: the event is running mid-node over a field that has been there (mid-area)
        None => return build_mid_event(data, ram, runner, ev, w, calls0, notes),
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
    // a warp_fade into the 3-D map cut during the arrival's fade-in (mode vt 0x080495B4: +4 brightness, 4 a frame):
    // the load is behind, the map has run since - its objects, the flyer and the camera from RAM, the fade's
    // progress (real route 0-55 -> 0-50, s515's take-off: tiles r002_006+9000 / +17000, 53 frames into the fade)
    if fading && is_3d(data, to) && ram.mode_vt() == MODE_FADE_IN_3D {
        let b = ram.s32(ram.r32(MODE) + 4);
        if b > 0 {
            field_from_ram(&mut w, ram, notes);
            if let Some(f) = w.field.as_mut() {
                f.mode = Mode::FadeIn { k: b / 4 };
            }
        }
    }
    // the load frame's end: the cameras (a modal scroll the continuation started takes its step; the script sees it
    // done in the next frame - the game's PC is still in camera_scroll_to at the cut: opening 1-10, s068)
    // (not after held frames: the arrival's last held frame ran the cameras - whole-run cut 600,329, s345 node 22
    // after its warp to 8-70, 2 held frames, set_pose waiting)
    if held == 0 {
        field::world_mode_cameras(&mut w);
    }
    // (the replayed spawns' idle clocks: the RAM's - an NPC turned by the replay re-entered its idle on another
    // draw; run 3 cut 169,333, s016's face(279, 2) after the load: tick 276 in RAM)
    if held == 0 && skipped == 0 {
        npc_anims_from_rows(&mut w, ram);
    }
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
    // a cut inside a pause page (Status / Inventory / its Use dialog): the field from the saved world, then the page
    if let Some(page_ram) = pause_page(ram).and(ram.with_saved_world()) {
        let mut imp = import_emulator_state(data, &page_ram, runner)?;
        open_pause_page(&mut imp.world, ram, &mut imp.notes);
        import_options(&mut imp.world, ram);
        return Ok(imp);
    }
    // the title's Continue (Select Game, the 16-step fade-out 0x0804BBA4 before the slot's load): a menu of the title,
    // its save slots in the cartridge's EEPROM - not a field state (run 2 cut 35,807)
    // (the mode is the generic 16-step fade - the prologue's too: here with no player, no event, the slot list's objects)
    if ram.mode_vt() == 0x0804_BBA4 && ram.player() == 0 && ram.r32(DEPTH) == 0 && ram.objects().iter().any(|&o| ram.r32(o) == 0x0804_BA50) {
        return Err(ImportError::Unsupported("the title's Continue fade-out (mode 0x0804bba4): a title menu, not a field (its save slot is in EEPROM)".into()));
    }
    // the boot screens and the title's pages (legal 0x08049354, logos 0x080492B8, title 0x0804A404 / 0x080494D8,
    // Single / Multiplayer 0x08049B00, Select Game 0x0804947C, its "Are you sure?" 0x08049CEC, options 0x0804A430):
    // menus of the power-on, not a field (run 2 rows 480,612 .. 481,542, after the soft reset at 8-28)
    let boot = [0x0804_9354, 0x0804_92B8, 0x0804_A404, 0x0804_94D8, 0x0804_9B00, 0x0804_947C, 0x0804_9CEC, 0x0804_A430];
    if boot.contains(&ram.mode_vt()) && ram.player() == 0 && ram.r32(DEPTH) == 0 {
        return Err(ImportError::Unsupported(format!("a boot / title screen (mode {:#x}), not a field", ram.mode_vt())));
    }
    let info = inspect(ram);
    runner.text_open_due = None;
    // a state saved inside an event's load iteration, its players not made yet (the recording's first rows still
    // carry the cut's logic count - the load's frozen lag frames): no frame the app stands in (run 3 cut 139,161, s113's
    // warp to 2-8: the counter still for 41 frames, the leader made in the last)
    // (`load_frozen`: the recording shows the leader made later in the same iteration - a cutscene map's offstage
    // players never appear in it, importable)
    if ram.load_frozen && ram.player() == 0 && info.depth > 0 && info.kind == CutKind::MidArea {
        return Err(ImportError::Unsupported(format!("area {:?}: inside an event's load iteration (its frozen lag frames, no leader yet); cut at the iteration's end", info.area)));
    }
    // an area entry inside a running event (a script's warp): the event's position from the scene objects
    // (no leader: a cutscene map the event loaded with the players offstage)
    let in_event = if info.kind != CutKind::MidArea {
        None
    } else if info.depth > 0 {
        running_event(ram, runner)
    } else {
        // (depth 0: a script still running only when an interpreter stands in one of its nodes - Kami's Lookout's
        // arrival s512 with a non-Goku leader, waiting in its fade_in_white)
        running_event_live(ram, runner)
    };
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
    for k in 0..4 {
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
        // (the last build is the one kept: its scripts stay - a mid-area build whose draws never land on the RAM's
        // generator keeps it, real run 20-6 r062_001 +8,000: s475's text box was dropped with a restored runner)
        if k < 3 {
            runner.restore(&probe);
        }
        guess = next;
        last = Some(b);
    }
    let b = last.unwrap();
    if let Some(n) = notes.iter().find(|n| n.starts_with("unsupported: ")) {
        return Err(ImportError::Unsupported(n.trim_start_matches("unsupported: ").to_string()));
    }
    let mut b = b;
    if b.world.rng.state() != target {
        // a mid-area build draws only for itself (the area load's spawns, the AI's enters - the game drew nothing at
        // the cut) and how many depends on the seed (rejection loops: WanderToRandomPoint's goal, spawn spots), so
        // the guesses may not converge: the generator is the RAM's (real run 20-6 r062_001 tiles, inside Buu)
        if info.kind == CutKind::MidArea {
            notes.push(format!("the build's {} draws did not land on the RAM's generator: set to it", b.draws));
            b.world.rng = Rng::new(target[0], target[1]);
        } else {
            return Err(ImportError::Mismatch(format!("RNG {:08x?} after {} draws vs RAM {:08x?}", b.world.rng.state(), b.draws, target)));
        }
    }
    let mut world = b.world;
    // the options (`0x03001914`: text speed, SFX volume, music volume, pad; the game's boot check: any byte over 8 =
    // the defaults) - the typewriter's speed
    let o = [0, 1, 2, 3].map(|k| ram.r8(0x0300_1914 + k));
    if o[..3].iter().all(|&v| v <= 8) {
        world.menu.settings = wf_core::menu::options::Settings { text_speed: o[0], sfx: o[1], music: o[2], pad: o[3] };
    }
    if in_event.is_some() {
        patch_patrols(&mut world, ram, &mut notes);
    }
    enemy_anims_from_rows(&mut world, ram);
    // the inline script under the event: an interpreter standing just after its `run_event` of the event (or of the
    // outermost event of a nested chain) - it goes on when the event ends (gate rkb_001_0-50: 14-1's trigger script,
    // run_event(515) then set_flag(408))
    runner.after_event = None;
    if let (Some(ev), true) = (in_event, runner.running()) {
        let outermost = outer_event(ram, runner, ev).map(|(o, _)| o.0).unwrap_or(ev.0);
        if let Some((id, ran)) = inline_running(ram) {
            let runs = ran.last().is_some_and(|(n, a)| {
                (wf_script::natives::RUN_EVENT..=wf_script::natives::RUN_EVENT_P0_P3).contains(n) && a.as_ref().is_none_or(|x| x.first().is_some_and(|&e| e == ev.0 as i32 || e == outermost as i32))
            });
            if runs {
                notes.push(format!("{id} under the event: goes on after its run_event (action #{})", ran.len() - 1));
                runner.after_event = Some((id, ran.len()));
            }
        }
    }
    if !b.finished {
        notes.push("the load's scripts are still running at the cut (host state carries over)".into());
    }
    check(&mut world, ram, &mut notes);
    import_vram(&mut world, ram);
    if let Some(ev) = in_event {
        let mut evs = vec![ev.0];
        if let Some((o, _)) = outer_event(ram, runner, ev) {
            evs.push(o.0);
        }
        import_saved_field(data, ram, runner, &evs, &mut world, &mut notes);
    }
    import_boss_bar(&mut world, ram);
    import_tile_anims(&mut world, ram);
    // a text box opened by the import whose object the game makes later in the cut's iteration: its tiles now
    if let Some(p) = runner.text_open_due.take() {
        // ... unless the box object is already made, not yet in the world list (its constructor 0x08012030 took the
        // blocks: +0x78 .. +0x7E, the portrait's last): those blocks are its (cutscene-map cuts 1,617,636 (19-3) /
        // run 2 594,120 (8-71): the box in the heap at the cut, in the list a frame later - ours allocated a second
        // set, the purge took the scene actors' blocks)
        if !adopt_text_blocks(&mut world, ram) {
            wf_core::vram::text_open(&mut world, p);
        }
    }
    // a banner object of an earlier load still showing: its countdown (obj +0x10, a byte: the bytes above are heap
    // leftovers - whole-run cut 146,673: word 0x0203DECE, 206 frames left; the object goes the frame after 1) -> our age
    {
        world.scene.banner_made = u32::MAX;
        if let Some(o) = ram.objects().into_iter().find(|&o| ram.r32(o) == VT_BANNER) {
            let rem = ram.r8(o + 0x10) as u32;
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
        world.scene.timers.countdown = u32le(&gd, 0x4D4) as i32;
        world.scene.timers.countdown_on = gd[1] & 4 != 0;
    }
    import_options(&mut world, ram);
    if in_event.is_none() {
        finish_interrupted_pass(&mut world, ram, &mut notes);
    }
    // a closed menu's queued event (world +0x610: count, then the deferred-script objects vt 0x08049264, +8 the event's
    // header - Options -> Sleep's s007, `0x08014B12`): it starts at the next frame's start (`menu::run_then`; run 2
    // cut 1,869,665)
    let n = ram.r32(0x0300_1C4C + 0x610);
    if n > 0 && n < 8 && !wf_core::menu::active(&world) {
        let o = ram.r32(0x0300_1C4C + 0x614);
        if ram.valid(o) && ram.r32(o) == 0x0804_9264 {
            if let Some(e) = runner.event_of_header(ram.r32(o + 8)) {
                world.menu.then = Some(format!("s{e:03}"));
                world.menu.then_armed = true;
            }
        }
    }
    Ok(Imported { world, prev_keys: ram.r16(INPUT + 2) & 0x3FF, lf: info.lf, kind: info.kind, held_frames: b.held, draws: b.draws, notes })
}

/// the enemies the script holds in a pose (state +0x14 the hold state 0x03001B00, `set_pose`): our puppets on the
/// RAM's cel (see `build_mid_code`; the saved field of a cutscene map too)
fn held_enemy_puppets(w: &mut World, ram: &GbaRam) {
    for (o, raw, cel, tick) in ram.actor_rows.iter().map(|r| (r.0, r.2, r.3, r.4)).collect::<Vec<_>>() {
        if ram.r32(o) != VT_ENEMY || ram.r32(o + 0x14) != 0x0300_1B00 {
            continue;
        }
        let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
        let Some(i) = (0..w.ents.len()).find(|&i| !w.ents[i].removed && matches!(&w.ents[i].body, Body::Enemy(e) if e.a.x == x && e.a.y == y)) else { continue };
        let id = w.ents[i].id;
        if w.scene.puppets.iter().any(|p| p.id == id) {
            continue;
        }
        let aid = (0..25).find(|&a| wf_core::scene::anim_state(a) + 1 == raw).unwrap_or(0);
        let anim = wf_core::scene::ActAnim { raw, frame: cel, tick, mode: wf_core::scene::AnimMode::Pose, ..wf_core::scene::ActAnim::of(aid) };
        w.scene.puppets.push(wf_core::scene::Puppet { id, anim });
    }
}

/// a cut inside a cutscene map (`load_scene` after `save_field_on_next_scene`, no player object in the live world):
/// the field the scene saved, for its `return_to_field` (0x75) to restore rather than reload. The game's save
/// (`0x08009196`): the area descriptor it left (`0x0300197C`, 20 bytes) copied to `0x03001968`; the camera
/// (`0x030019A4`) copied into a 0xC4-byte buffer at `[0x030019A0]` (`0x080099B6`: +0 0x5C bytes, +0x90 0x34 bytes);
/// the world struct (`0x03001C4C`, 0xE00 bytes: the object list +4 / +8, the tile allocator) into a buffer at
/// [world +0xBF8] after the objects answered the hide message 0xA (`0x08011DB4`), the live list emptied. The saved
/// field is our mid-area build of a RAM with those copies put back (run 2 cut 57,951: Other World 1-20 under s103's
/// 0-22 scene) -> `SavedWorld` + the host's `SavedField`; its tile allocator is the stash `vram::world_show` takes
fn import_saved_field(data: &Arc<GameData>, ram: &GbaRam, runner: &mut StoryHost, evs: &[u16], world: &mut World, notes: &mut Vec<String>) {
    if ram.player() != 0 || world.scene.saved_world.is_some() || runner.hs.saved_field.is_some() || world.field.is_none() {
        return;
    }
    let sv = ram.r32(OBJW + 0xBF8);
    if !ram.valid(sv) || !ram.valid(sv + 0xDFF) {
        return;
    }
    let n = ram.r32(sv + 4);
    if n == 0 || n > 0x100 {
        return;
    }
    let objs: Vec<u32> = (0..n).map(|k| ram.r32(sv + 8 + 4 * k)).collect();
    // (the leader: the player object, or on the 3D world map the flyer, vt 0x0804E1B4)
    let Some(&pl) = objs.iter().find(|&&o| ram.valid(o) && matches!(ram.r32(o), VT_PLAYER | 0x0804_E1B4)) else { return };
    let mut r2 = ram.clone();
    let put = |r: &mut GbaRam, a: u32, b: &[u8]| {
        for (k, &v) in b.iter().enumerate() {
            let x = a + k as u32;
            if (IWRAM..IWRAM + IWRAM_SIZE as u32).contains(&x) {
                r.iwram[(x - IWRAM) as usize] = v;
            } else if (EWRAM..EWRAM + EWRAM_SIZE as u32).contains(&x) {
                r.ewram[(x - EWRAM) as usize] = v;
            }
        }
    };
    put(&mut r2, OBJW, &ram.read(sv, 0xE00));
    put(&mut r2, WS, &ram.read(0x0300_1968, 0x14));
    put(&mut r2, PLAYER, &pl.to_le_bytes());
    let cb = ram.r32(0x0300_19A0);
    if ram.valid(cb) {
        put(&mut r2, CAM, &ram.read(cb, 0x5C));
        put(&mut r2, CAM + 0x90, &ram.read(cb + 0x90, 0x34));
    }
    // the camera controller the save pushed (`0x080210D6`: the slot list 0x03002BD8 - +8 the mode's controller,
    // +0xC the stack count, +0x10.. the pushed ones; the scene runs on the hold controller 0x03001964): the field's
    let cs = 0x0300_2BD8u32;
    let cnt = ram.r32(cs + 0xC);
    if (1..=8).contains(&cnt) {
        put(&mut r2, cs + 8, &ram.read(cs + 0x10 + 4 * (cnt - 1), 4));
    }
    r2.screen_black = None;
    // the saved objects' actor rows (the recording has the live world's only)
    for &o in &objs {
        if !ram.valid(o) || r2.actor_rows.iter().any(|r| r.0 == o) {
            continue;
        }
        if let Some(r) = actor_row_of(data, ram, o) {
            r2.actor_rows.push(r);
        }
    }
    let info2 = inspect(&r2);
    let base = world.next_id + 1000;
    let mut n2 = vec![];
    let b = build_mid_ids(data, &r2, &info2, runner, world.rng.state(), world.frame, base, &mut n2);
    let mut w2 = b.world;
    held_enemy_puppets(&mut w2, &r2);
    // (the enemies' animation positions from the rows, as the live world's: the import's last step)
    enemy_anims_from_rows(&mut w2, &r2);
    import_vram(&mut w2, &r2);
    // (the event holds the field it saved, as ours saves it inside the event: busy - its enemies wait until the event
    // ends; run 2 cut 568,325, 8-70's soldier under the 8-17 scene)
    // the hold as the scene world has it: busy, and an interrupted frame the event holds (`held_event`: the field's
    // remaining updates run in the frame the event ends - run 1 cut 557,141, 8-70's s325: the soldier's AI enters
    // in that frame; ours saves the field with it inside the event)
    let (busy, held_event, once) = world.field.as_ref().map(|f| (f.busy, f.held_event, f.once_pending)).unwrap_or((false, false, None));
    field::set_busy(&mut w2, busy);
    if let Some(f) = w2.field.as_mut() {
        f.held_event = held_event;
        // (the touch-once trigger whose activation runs the event goes when it ends: the field's)
        if f.once_pending.is_none() {
            f.once_pending = once;
        }
    }
    // the running event's own touch-once trigger (still in the RAM's list: the game removes it when the event
    // returns) - gone, as the live field's (`build_mid_event`; run 1 cut 789,133: 11-17's room-wide s-trigger under
    // the 11-24 scene fired again after its end)
    let names: Vec<String> = evs.iter().map(|e| format!("s{e:03}")).collect();
    let fired: Vec<usize> = w2.field.as_ref().map(|f| f.triggers.iter().enumerate().filter(|(_, t)| t.act == wf_core::fieldcontent::Activation::TouchOnce && field::trigger_def(&w2, t.obj).is_some_and(|d| matches!(d.kind, wf_core::fieldcontent::ObjKind::Trigger { kind: wf_core::fieldcontent::TriggerKind::Event(ref s), .. } if names.contains(s)))).map(|(k, _)| k).collect()).unwrap_or_default();
    if let Some(f) = w2.field.as_mut() {
        for k in fired.into_iter().rev() {
            f.triggers.remove(k);
        }
    }
    // the objects without a shadow record (+0x5C = 0: run 2 cut 57,951 - Goku and Pikkon after s103's poses) take no
    // shadow graphic at the show message (`0x0800F478`): our shadow override (`vram::shadow_set`'s list)
    let no_shadow: Vec<wf_core::world::EntityId> = objs
        .iter()
        .filter(|&&o| ram.valid(o) && ram.r32(o + 0x5C) == 0)
        .filter_map(|&o| {
            let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
            (0..w2.ents.len()).find(|&i| !w2.ents[i].removed && w2.actor(i).is_some_and(|a| a.x == x && a.y == y)).map(|i| w2.ents[i].id)
        })
        .collect();
    if let Some(v) = w2.field.as_mut().and_then(|f| f.vram.as_mut()) {
        for id in no_shadow {
            v.shadow_set.retain(|o| o.0 != id);
            v.shadow_set.push((id, None));
        }
    }
    // the tile allocator as the save left it: the stash (the scene has its own); the shared graphics the objects
    // gave back at the hide message are taken again by `world_show`
    if let Some(v) = w2.field.as_mut().and_then(|f| f.vram.as_mut()) {
        let a = std::mem::replace(&mut v.a, wf_core::vram::ObjVram::fresh());
        let mut o = std::mem::take(&mut v.owners);
        o.retain(|x| !matches!(x.1, wf_core::vram::Owner::Shared(_)));
        v.shared_rc = [0; 5];
        v.stashed = Some(Box::new((a, o, [0; 5])));
    }
    let pid = |w: &World| w.players.first().copied();
    if pid(&w2) != pid(world) {
        notes.push(format!("saved field: player ids differ ({:?} vs {:?}) - not imported", pid(&w2), pid(world)));
        return;
    }
    let Some((x, y, facing)) = info2.leader else { return };
    let a = info2.area;
    runner.hs.saved_field = Some(wf_script_core::SavedField { area: (a.world, a.area, a.variant), x, y, facing, cam: (w2.cameras[0].x, w2.cameras[0].y) });
    world.scene.saved_world = Some(Box::new(field::save_world(&w2)));
    // (the last banner shown is the field's: the scene's backdrop load shows none - run 2 cut 540,172, Babidi's ship
    // 8-66 scene over 8-65: no banner at the walk to 8-64 afterwards)
    if let Some(name) = data.field.as_ref().and_then(|fd| fd.area(a.world, a.area)).map(|d| d.name_id) {
        world.scene.banner_last = name;
    }
    world.next_id = world.next_id.max(w2.next_id);
    notes.push(format!("saved field: {}-{} ({} objects) under the cutscene map, for its return_to_field", a.world, a.area, n));
    for m in n2 {
        notes.push(format!("saved field: {m}"));
    }
}

/// the game options (`0x03001914`: text speed, SFX volume, music volume, 0 - menu/options.rs; global, loaded from the
/// cartridge at boot, not part of a save game): the cut's, on every import (run 2: text speed 6, SFX 4 after its
/// Options visit - every cut after it, whole-run run-2 rows 31,497 ..); and the current game's slot (`0x03001910`:
/// where Save Game writes, the slot a Continue started from)
fn import_options(w: &mut World, ram: &GbaRam) {
    let b = ram.read(0x0300_1914, 4);
    if b[..3].iter().all(|&v| v <= 8) {
        w.menu.settings = wf_core::menu::options::Settings { text_speed: b[0], sfx: b[1], music: b[2], pad: b[3] };
    }
    let slot = ram.r32(0x0300_1910);
    if (slot as usize) < wf_core::menu::save::SLOTS {
        w.saves.current = slot as i32;
    }
}

/// The save slots from the cartridge's EEPROM (8 KiB as the emulator holds it: each 8-byte block byte-reversed; a
/// save state does not carry it - the cut doc's `init.eeprom`, tools/wholerun/resync.py). Slot n at 0x10 + n * 0x8B0
/// (the slot writer `0x080049F2`): the GameData image (0x8A8 bytes) + a checksum / pad word pair. As `0x08004962`
/// writes it: +1 bit 0 = a game exists, bit 3 = the save flag; +0x4D0 = the play time (absolute frames; GameData in
/// RAM holds it as a bias to the frame counter); +0x8A0 world, area, variant (bytes), +0x8A4 / +0x8A6 the leader's
/// x / y (px, u16). Script words L / G and the variant byte are not in the slot (a Continue starts them fresh).
/// (The checksum is not checked: a slot counts when its "game exists" bit is set.)
pub fn import_saves(data: &GameData, ram: &GbaRam, eeprom: &[u8], saves: &mut wf_core::menu::save::Saves) {
    use wf_core::menu::save::{SaveData, SLOTS};
    let b: Vec<u8> = eeprom.chunks(8).flat_map(|c| c.iter().rev().copied().collect::<Vec<_>>()).collect();
    saves.slots.resize(SLOTS, None);
    for s in 0..SLOTS {
        let o = 0x10 + s * 0x8B0;
        let Some(gd) = b.get(o..o + 0x8A8) else { continue };
        if gd[1] & 1 == 0 || gd.iter().all(|&v| v == 0xFF) {
            saves.slots[s] = None;
            continue;
        }
        let mut r = ram.clone();
        let g0 = (GD - IWRAM) as usize;
        r.iwram[g0..g0 + 0x8A8].copy_from_slice(gd);
        let mut story = import_story(data, &r);
        let fresh = Story::new_game(data);
        story.play_frames = (u32le(gd, 0x4D0) as i32).max(0) as u64;
        story.lf_bias = 0;
        story.l = fresh.l;
        story.g = fresh.g;
        story.variant_byte = fresh.variant_byte;
        saves.slots[s] = Some(SaveData {
            story,
            world: gd[0x8A0] as i32,
            area: gd[0x8A1] as i32,
            variant: gd[0x8A2] as i32,
            x: u16le(gd, 0x8A4),
            y: u16le(gd, 0x8A6),
            flag: gd[1] & 8 != 0,
        });
    }
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
    let ours: Vec<(i32, i32, i32, u8, usize)> = w
        .ents
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.removed && matches!(e.body, Body::Npc(_) | Body::Player(_) | Body::Enemy(_)))
        .filter_map(|(i, _)| {
            let a = w.actor(i)?;
            Some((wf_core::scene::sprite_id(w, i).unwrap_or(-1), a.x, a.y, a.facing, i))
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
                // (the RAM's facing wins: an event's replayed turn - or a skipped one - left ours another; run 3 cuts
                // 169,333 / 220,149 / 224,119 / 88,742: actors turned by a node's earlier commands)
                if ours[k].3 != f && f < 4 {
                    notes.push(format!("actor at ({x}, {y}) facing {} vs RAM {f} (RAM's taken)", ours[k].3));
                    match &mut w.ents[ours[k].4].body {
                        Body::Player(p) => p.a.facing = f,
                        Body::Npc(n) => n.a.facing = f,
                        Body::Enemy(e) => e.a.facing = f,
                        _ => {}
                    }
                }
            }
            None => notes.push(format!("RAM actor {o:#x} vt {:#x} at ({}, {}) has no entity there", ram.r32(o), x as f64 / 256.0, y as f64 / 256.0)),
        }
    }
}


/// the object tile allocator (`0x03001C4C`, vram.rs) as the RAM holds it; the actors' blocks (obj +0xC) belong to
/// their entities (matched by position like `check`), the shadows' to their shared graphics (`0x03002B88` + 4k:
/// reference count, handle +2); other blocks stay foreign
/// the map's clocks at the cut: the tile animations' state (0x03001A68: +4 the count, +8 + 8 i: s16 frame, s16
/// countdown, descriptor; ticked by `0x08009CC2`, set up by `0x08009B80` with frame 0 and countdown i + 1) -> the
/// ticks since the load they imply (the smallest one all agree on: the pictures repeat with the periods) ->
/// `Field.entered_iter`; the camera object's counter (0x030019A4 +0x40, the wrap layers' drift) ->
/// `Field.drift_iter`
fn import_tile_anims(w: &mut World, ram: &GbaRam) {
    const A: u32 = 0x0300_1A68;
    let cam = ram.r32(0x0300_19A4 + 0x40);
    let n = ram.r32(A + 4).min(64);
    let mut an: Vec<(i32, i32, Vec<u32>)> = vec![];
    for i in 0..n {
        let b = A + 8 + 8 * i;
        let desc = ram.r32(b + 4);
        let nf = ram.r8(desc) as u32;
        let durs: Vec<u32> = (0..nf).map(|j| ram.r16(ram.r32(desc + 4 + 4 * j)) as u32).collect();
        an.push((ram.r16(b) as i16 as i32, ram.r16(b + 2) as i16 as i32, durs));
    }
    let fits = |t: u32| {
        an.iter().enumerate().all(|(i, (f, c, d))| {
            let first = i as u32 + 1;
            if d.len() <= 1 || d.iter().any(|&x| x == 0) {
                return true;
            }
            if t < first {
                return (*f, *c) == (0, (first - t) as i32);
            }
            let mut r = (t - first) % d.iter().sum::<u32>();
            let mut g = 1 % d.len();
            while r >= d[g] {
                r -= d[g];
                g = (g + 1) % d.len();
            }
            (*f, *c) == (g as i32, (d[g] - r) as i32)
        })
    };
    // the descriptors live in the ROM (not in the RAM image: no durations): only the load's first ticks tell -
    // every animation still on frame 0 with countdown i + 1 - T (rkb 19-3: a scene's load one tick before the cut,
    // the camera counter running on); else the durations, when readable, else the camera's count
    let early = an.first().map(|a| 1 - a.1).filter(|&t| t >= 0 && an.iter().enumerate().all(|(i, a)| a.0 == 0 && a.1 == i as i32 + 1 - t));
    let readable = an.iter().any(|a| a.2.len() > 1);
    let tiles = match early {
        Some(t) => Some(t as u32),
        None if readable && !fits(cam) => (0..200_000u32).find(|&t| fits(t)),
        None => Some(cam),
    };
    if let Some(f) = w.field.as_mut() {
        f.drift_iter = Some(f.cam_iters().wrapping_sub(cam));
        if let Some(t) = tiles {
            f.entered_iter = f.cam_iters().wrapping_sub(t);
        }
    }
}

/// the boss HP bar object (vtable 0x0804CE28) -> `scene.boss_bar` / `boss_bar_out`: +0x10 the enemy (matched by
/// position), +0x15 / +0x16 the fill's base and span (`a * 53 / 100`, `b * 53 / 100 - base`: a / b recovered as the
/// smallest percentages giving them), +0x17 the velocity (+1 sliding in, 0 shown, -1 hidden: sliding out with the
/// fill frozen in +0x15), +0x18 y
fn import_boss_bar(w: &mut World, ram: &GbaRam) {
    w.scene.boss_bar_out = None;
    let Some(o) = ram.objects().into_iter().find(|&o| ram.r32(o) == 0x0804_CE28) else {
        w.scene.boss_bar = None;
        return;
    };
    let (lo, span, vel, y) = (ram.r8(o + 0x15) as i32, ram.r8(o + 0x16) as i32, ram.s8(o + 0x17), ram.s8(o + 0x18));
    if vel < 0 {
        w.scene.boss_bar = None;
        w.scene.boss_bar_out = Some(wf_core::scene::BossBarOut { fill: lo, y, at: w.frame });
        // its tile block goes with the object: y + 9 updates from here (the object deletes itself below y -8; from a
        // fresh hide at y 8 that is `BOSS_BAR_OUT`, 17 - run 2 cut 57,741: y 5 at the cut, block 0x35A freed 14 later)
        wf_core::vram::release_later(w, wf_core::vram::Owner::BossBar, (y + 9).max(0) as u32);
        return;
    }
    let e = ram.r32(o + 0x10);
    let (ex, ey) = (ram.s32(e + 0x1A0), ram.s32(e + 0x1A4));
    let ent = w.ents.iter().find(|t| !t.removed && matches!(&t.body, Body::Enemy(n) if n.a.x == ex && n.a.y == ey)).map(|t| t.id);
    let Some(entity) = ent.or(w.scene.boss_bar.as_ref().map(|b| b.entity)) else { return };
    let pct = |v: i32| (v * 100 + 52) / 53;
    // y = -8 + (frame - at), capped at 8
    w.scene.boss_bar = Some(wf_core::scene::BossBar { entity, a: pct(lo), b: pct(lo + span), at: w.frame.wrapping_sub((y + 8) as u32) });
}

/// a text box object (vt 0x08049F60) made but not in the world list yet: its blocks (+0x78 / +0x7A / +0x7C, the
/// portrait's +0x7E) - foreign in our rebuilt allocator - become the dialog's (`vram::text_open`'s owners). False when
/// there is no such object, or its handles are not all foreign blocks of ours
fn adopt_text_blocks(w: &mut World, ram: &GbaRam) -> bool {
    use wf_core::vram::Owner;
    let listed = ram.objects();
    let Some(v) = w.field.as_mut().and_then(|f| f.vram.as_mut()) else { return false };
    for o in ram.ewram_with(0x0804_9F60) {
        if listed.contains(&o) {
            continue;
        }
        let hs: Vec<(usize, u16)> = (0..4u32).map(|k| (k as usize, ram.r16(o + 0x78 + 2 * k))).filter(|&(_, h)| h != 0 && h != 0xFFFF).collect();
        if hs.is_empty() || !hs.iter().all(|&(_, h)| v.owners.iter().any(|x| x.0 == h && x.1 == Owner::Foreign)) {
            continue;
        }
        for (k, h) in hs {
            if let Some(x) = v.owners.iter_mut().find(|x| x.0 == h) {
                x.1 = if k == 3 { Owner::TextPortrait } else { Owner::Text };
            }
        }
        return true;
    }
    false
}

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
    let mut hud_out: Vec<(u16, i32)> = vec![];
    let mut used_ents: Vec<wf_core::world::EntityId> = vec![];
    let mut signs = 0u32;
    let mut pops: Vec<(u16, i32, u32)> = vec![];
    for o in ram.objects() {
        let vt = ram.r32(o);
        if vt == 0x0804_D1E4 {
            let h = ram.r16(o + 0x16);
            if h != 0xFFFF {
                owners.push((h, Owner::Props));
            }
            continue;
        }
        // an open text box (0x08012030): its blocks +0x78 / +0x7A / +0x7C, the portrait's +0x7E (mid_3000)
        // an animated effect in flight (`import_effects`): its block belongs to the effect entity at its position
        if vt == 0x0804_A2C0 || vt == 0x0804_CF3C {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x10), ram.s32(o + 0x14));
            let ent = (0..w.ents.len()).find(|&k| !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Fx(f) if matches!(**f, wf_core::fx::Fx::Anim { x: fx, y: fy, .. } if fx == x && fy == y)));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        if vt == 0x0804_9F60 {
            for k in 0..4u32 {
                let h = ram.r16(o + 0x78 + 2 * k);
                // (a slot can keep a handle its block no longer has - a live block starts there or it is skipped:
                // Gotenks 6-9 tile +6,000, a box mid-scroll; freeing a stale handle corrupts the free list)
                let live = a.e.get(a.idx[(h >> 1) as usize & 511] as usize).is_some_and(|b| b.used == 1 && b.start == h >> 1);
                if h != 0xFFFF && h != 0 && live {
                    owners.push((h, if k == 3 { Owner::TextPortrait } else { Owner::Text }));
                }
            }
            continue;
        }
        // a damage number in flight (`0x0800AD3E`, vtable 0x0804C904, 0x100 tiles): its updates left (+0x10, one a
        // frame; the object goes at 0) - r033_002_6-1 tiles +8,000 .. +12,000. Its target is not known: no restart
        if vt == 0x0804_C904 {
            let h = ram.r16(o + 0xC);
            if h != 0xFFFF {
                owners.push((h, Owner::Popup));
                pops.push((h, ram.s32(o + 0x10), o));
            }
            continue;
        }
        // the Instant Transmission markers (vt 0x0804E41C, `import_it_select`): its block belongs to our markers
        if vt == 0x0804_E41C {
            let h = ram.r16(o + 0xC);
            let ent = (0..w.ents.len()).find(|&k| !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Fx(f) if matches!(**f, wf_core::fx::Fx::ItMarkers { .. })));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // a transformation's aura effect (vt 0x0804B9E0, the SS-up state's ctor `0x0801C3EC`): its block belongs to our
        // aura effect of that player (`Alive::Transform`, made when the import entered the state) - the state deletes
        // it at its end (whole-run cut 185,000: 7-6's scripted transformation, the block freed at f164)
        if vt == 0x0804_B9E0 {
            let h = ram.r16(o + 0xC);
            let ent = (0..w.ents.len()).find(|&k| {
                !w.ents[k].removed
                    && !used_ents.contains(&w.ents[k].id)
                    && matches!(&w.ents[k].body, Body::Fx(f) if matches!(**f, wf_core::fx::Fx::Loop { alive: wf_core::fx::Alive::Transform { .. }, .. }))
            });
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
                continue;
            }
        }
        // an enemy's energy wave (vt 0x08049C98, `import_waves`): its block belongs to the wave at its tail
        if vt == 0x0804_9C98 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20) << 8, ram.s32(o + 0x24) << 8);
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Shot(s) if matches!(s.kind, wf_core::shots::ShotKind::EnergyWave { .. }) && s.x == x && s.y == y));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // an emote bubble (vt 0x0804A298, `import_emotes`): its block belongs to the emote visual at its position
        if vt == 0x0804_A298 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 4), ram.s32(o + 8));
            if let Some(vid) = w.scene.visuals.iter().find(|v| matches!(v.kind, wf_core::scene::VisualKind::Emote(_)) && v.vid != 0 && (v.x, v.y) == (x, y) && !owners.iter().any(|ow| ow.1 == Owner::Visual(v.vid))).map(|v| v.vid) {
                if h != 0xFFFF {
                    owners.push((h, Owner::Visual(vid)));
                }
            }
            continue;
        }
        // a thrown rock (vt 0x0804CE50, `import_rocks`): its block belongs to the rock effect at its position
        if vt == 0x0804_CE50 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20), ram.s32(o + 0x24));
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Fx(f) if matches!(**f, wf_core::fx::Fx::Rock { x: rx, y: ry, .. } if rx == x && ry == y)));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // a mummy's bandage (vt 0x0804A830, `import_wraps`): its block belongs to the bandage at its position
        if vt == 0x0804_A830 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20), ram.s32(o + 0x24));
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Shot(s) if matches!(s.kind, wf_core::shots::ShotKind::Wrap(_)) && s.x == x && s.y == y));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // the leader's Ki Blast (vt 0x0804B454, `import_player_blasts`): its block belongs to the blast at its position
        if vt == 0x0804_B454 || vt == 0x0804_A368 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20), ram.s32(o + 0x24));
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Shot(s) if matches!(s.kind, wf_core::shots::ShotKind::Straight(_)) && s.x == x && s.y == y));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // an enemy's beam (vt 0x0804AEF8, `import_enemy_beam`; the class takes 0x300): its block belongs to the beam
        // shot from its origin (run 2 1,880,524: Kid Buu's beam on the KO'd leader - the block freed with the beam)
        if vt == 0x0804_AEF8 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20) << 8, ram.s32(o + 0x24) << 8);
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Shot(s) if matches!(s.kind, wf_core::shots::ShotKind::Kame(_)) && s.x == x && s.y == y));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
                continue;
            }
            // the leader's beam object still charging (+0x50 the player; built at the press, no shot yet): the
            // press's `BeamObj` block (run 3 cut 30,383: Gohan's charge, tick 4)
            if h != 0xFFFF && ram.r32(o + 0x50) == ram.player() && ram.player() != 0 {
                if let Some(pi) = w.player_idx(0) {
                    owners.push((h, Owner::BeamObj(w.ents[pi].id)));
                    continue;
                }
            }
        }
        // Kid Buu's beams (vt 0x0804AEF8, `import_enemy_beam`: +0x20 / +0x24 the origin px): a block belongs to the
        // beam shot at its origin (run 2 cut 1,927,800: a retracting beam's block, freed with its destructor)
        if vt == 0x0804_AEF8 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20) << 8, ram.s32(o + 0x24) << 8);
            // (a charging one: ours stands at the fire position - the charging Kame shot)
            let charging = ram.r8(o + 0x10) == 0;
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Shot(s) if matches!(&s.kind, wf_core::shots::ShotKind::Kame(kb) if if charging { kb.charging } else { s.x == x && s.y == y })));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // Super Buu's ball (vt 0x0804A33C, `import_super_ball`): its block belongs to the ball shot at its position
        // (bursting: to its blast effect)
        if vt == 0x0804_A33C {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20), ram.s32(o + 0x24));
            let ent = (0..w.ents.len()).find(|&k| !used_ents.contains(&w.ents[k].id) && match &w.ents[k].body {
                Body::Shot(s) => matches!(s.kind, wf_core::shots::ShotKind::SuperBall { .. }) && s.x == x && s.y == y,
                Body::Fx(f) => matches!(**f, wf_core::fx::Fx::Anim { id: 0x0872_a5dc, x: fx, y: fy, .. } if fx == x >> 8 && fy == y >> 8),
                _ => false,
            });
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // a bomb (vt 0x0804A2E8, `import_bombs`): its block belongs to the bomb shot at its position
        if vt == 0x0804_A2E8 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x20), ram.s32(o + 0x24));
            let ent = (0..w.ents.len()).find(|&k| !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Shot(s) if matches!(s.kind, wf_core::shots::ShotKind::Bomb { .. }) && s.x == x && s.y == y));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // a coin (vt 0x08049CC4, `import_coins`): its block belongs to the coin at its 24.8 position
        if vt == 0x0804_9CC4 {
            let h = ram.r16(o + 0xC);
            let (x, y) = (ram.s32(o + 0x50), ram.s32(o + 0x54));
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Pickup(p) if matches!(p.kind, wf_core::fieldobj::PickupKind::Zenie(_)) && p.x == x && p.y == y));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // a map item lying in the area (vt 0x0804A314, ctor `0x0800DCDC`): its icon's block (+0xC) belongs to our
        // map item pickup (in object order)
        if vt == 0x0804_A314 {
            let h = ram.r16(o + 0xC);
            let ent = (0..w.ents.len()).find(|&k| !w.ents[k].removed && !used_ents.contains(&w.ents[k].id) && matches!(&w.ents[k].body, Body::Pickup(p) if matches!(p.kind, wf_core::fieldobj::PickupKind::MapItem { .. }) && p.landed && p.spawn.is_none()));
            if let (Some(k), true) = (ent, h != 0xFFFF) {
                used_ents.push(w.ents[k].id);
                owners.push((h, Owner::Ent(w.ents[k].id)));
            }
            continue;
        }
        // Instant Transmission's pick markers (vtable 0x0804E120): +0xC their 0x200 block
        if vt == 0x0804_E120 {
            let h = ram.r16(o + 0xC);
            if h != 0xFFFF {
                owners.push((h, Owner::ItPick));
            }
            continue;
        }
        // the boss HP bar (`show_boss_bar`, vtable 0x0804CE28, 0x200) and the 3D world map's own object
        // (`0x0800C0C8`, vtable 0x0804D8C8, 0xF00: a purge's level-15 object - kbr_013 f87, s463's portrait box)
        if vt == 0x0804_CE28 || vt == 0x0804_D8C8 {
            let h = ram.r16(o + 0xC);
            if h != 0xFFFF {
                owners.push((h, if vt == 0x0804_CE28 { Owner::BossBar } else { Owner::Map3d }));
            }
            continue;
        }
        if vt == VT_HUD || vt == VT_BANNER || vt == 0x0804_C070 || vt == 0x0804_D878 {
            let h = ram.r16(o + 0xC);
            if h != 0xFFFF {
                signs += (vt == 0x0804_D878) as u32;
                // a HUD sliding out (`hide_hud`: +0x14 velocity -1, +0x28 y from 8 down; it deletes itself after y -16,
                // `HUD_OUT` = 25 updates from a fresh hide): its block goes y + 17 updates from here (run 2 cut 57,741:
                // s103's hide_hud 3 frames before, y 5)
                if vt == VT_HUD && ram.s32(o + 0x14) < 0 {
                    owners.push((h, Owner::HudOut));
                    hud_out.push((h, ram.s32(o + 0x28) + 17));
                } else {
                    owners.push((h, if vt == VT_HUD { Owner::Hud } else if vt == VT_BANNER { Owner::Banner } else if vt == 0x0804_D878 { Owner::Sign(signs - 1) } else { Owner::StatMark }));
                }
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
    // the flames' shared graphic (`0x080246B4`: 0x03002FB8 its users, +2 its handle; `import_flames`)
    let mut keyed = vec![];
    if ram.r8(0x0300_2FB8) > 0 && ram.r16(0x0300_2FBA) != 0xFFFF {
        owners.push((ram.r16(0x0300_2FBA), Owner::Keyed(wf_core::vram::KEY_FLAME)));
        keyed.push((wf_core::vram::KEY_FLAME, ram.r8(0x0300_2FB8) as u32));
    }
    // the debris chips' shared graphic (`0x0800C454`: 0x03001AF8 its users, +2 its handle; `import_effects`)
    if ram.r8(0x0300_1AF8) > 0 && ram.r16(0x0300_1AFA) != 0xFFFF {
        owners.push((ram.r16(0x0300_1AFA), Owner::Keyed(wf_core::vram::KEY_DEBRIS)));
        keyed.push((wf_core::vram::KEY_DEBRIS, ram.r8(0x0300_1AF8) as u32));
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
    {
        let now = w.frame;
        // a damage number's target: the actor whose +0x1D8 holds it (+0x1D4 the logic counter of its last new number; `0x0800E8E6`
        // reuses the number within 8 frames - whole-run cut 787,315: flames hitting the player every frame)
        let lf = ram.r32(0x0300_0F94);
        let target = |p: u32| -> (wf_core::world::EntityId, u32) {
            let Some(a) = ram.objects().into_iter().find(|&a| matches!(ram.r32(a), VT_PLAYER | VT_ENEMY) && ram.r32(a + 0x1D8) == p) else { return (wf_core::world::EntityId::MAX, 0) };
            let (x, y) = (ram.s32(a + 0x1A0), ram.s32(a + 0x1A4));
            let ent = if a == ram.player() {
                w.player_idx(0).map(|pi| w.ents[pi].id)
            } else {
                w.ents.iter().find(|e| !e.removed && matches!(&e.body, Body::Enemy(en) if en.a.x == x && en.a.y == y)).map(|e| e.id)
            };
            match ent {
                Some(id) => (id, now.wrapping_sub(lf.wrapping_sub(ram.r32(a + 0x1D4)))),
                None => (wf_core::world::EntityId::MAX, 0),
            }
        };
        let popups = pops.iter().map(|&(h, n, o)| {
            let (t, at) = target(o);
            (t, at, h, now + n.max(0) as u32)
        }).collect();
        // what they show (vt 0x0804C904: +0x14 damage in 1/1024, +0x1C the second style, +0x20 / +0x24 x8 / y8)
        let popup_vis = pops.iter().map(|&(h, _, o)| (h, ram.s32(o + 0x20), ram.s32(o + 0x24), now, ram.s32(o + 0x14) as i64, ram.r32(o + 0x1C) != 0)).collect();
        let Some(f) = w.field.as_mut() else { return };
        // (made before anything the replay makes: a stamp below the next object's)
        let popup_at = pops.iter().map(|&(h, _, _)| (h, stamp.saturating_sub(1))).collect();
        // the stat-points mark's draw counter (vt 0x0804C070 +0x14)
        let it = f.iters;
        let stat_at = ram.objects().into_iter().find(|&o| ram.r32(o) == 0x0804_C070).map(|o| it.wrapping_sub(ram.r32(o + 0x14))).unwrap_or(0);
        f.vram = Some(VramState { a, owners, shared_rc, suspended: vec![], hud_out, popups, popup_vis, keyed, stashed: None, delayed: vec![], objs, susp: vec![], shadow_set: vec![], popup_at, pass_at: 0, stat_at });
    }
    // the objects a purge suspended (world +0x40C, `GbaRam::suspended_objects`, in suspension order): our entities at
    // their positions are out of the world too - they come back when the text box closes (run 1 cut 195,432)
    let ids: Vec<wf_core::world::EntityId> = ram
        .suspended_objects()
        .into_iter()
        .filter_map(|o| {
            let (x, y) = (ram.s32(o + 0x1A0), ram.s32(o + 0x1A4));
            (0..w.ents.len()).find(|&k| !w.ents[k].removed && matches!(w.ents[k].body, Body::Npc(_) | Body::Enemy(_)) && w.actor(k).is_some_and(|a| a.x == x && a.y == y)).map(|k| w.ents[k].id)
        })
        .collect();
    if let Some(v) = w.field.as_mut().and_then(|f| f.vram.as_mut()) {
        for id in ids {
            if !v.suspended.contains(&id) {
                v.suspended.push(id);
                v.susp.push(wf_core::vram::Susp::Ent(id));
            }
        }
    }
}
