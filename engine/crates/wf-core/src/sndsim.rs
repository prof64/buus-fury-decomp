//! The sound driver as part of the deterministic world, and the CPU-time model that hangs on it.
//!
//! The game's status page redraw (`0x080154D4`) holds the main loop for ~3.8 video frames; whether the loop makes
//! the fourth VBlank (4 frames) or misses it (5) depends on how long the sound mixer ran meanwhile, i.e. on the
//! music and the sound effects playing (`engine/docs/segments.md` §9 "Redraw timing"). So the driver's state
//! lives here, advanced every frame with the same code `wf-audio` plays with, whether or not anything is heard:
//!
//! * `pos` / `phase`: the hardware side. The FIFO DMA's completion interrupt (`0x03000AE0`) advances the
//!   driver's play position by 16 samples every 16,800 CPU cycles; a frame is 280,896 cycles.
//! * a normal frame: the main loop's update (`0x0803F296`) mixes the samples played since the last update.
//! * a long iteration: when the play position gets 928 samples ahead the interrupt itself mixes them; the main
//!   loop's update follows when the iteration ends.
//!
//! [`redraw_frames`] is a bounded-error predictor, not a cycle-exact one (ROM prefetch and DMA cycle stealing
//! are not modelled): its error against the emulator is ~0.45k cycles std, 1.5k max, on an iteration of ~1.12M
//! cycles; a decision closer than [`NEAR`] to the cutoff is counted in [`SoundSim::near`] (and logged with
//! `WF_SND_DEBUG`). Without sound content (`GameData.sound` = None) the pages keep their constant costs.

use crate::world::{Event, World};
use serde::{Deserialize, Serialize};
use wf_audio::driver::{Driver, DriverState};
use wf_view::MenuOp;

/// CPU cycles per video frame (228 lines x 1232)
pub const FRAME: i64 = 280_896;
/// cycles per DMA completion interrupt (16 samples x 1050 cycles)
pub const DMA: i64 = 16_800;
/// the interrupt mixes when the play position is this far ahead of the mixed one
pub const IRQ_MIX: u32 = 928;
/// frame time of the main loop's update in a menu frame without a redraw / in a field frame (the exact field
/// value depends on the frame's work; it only moves a chunk boundary by 16 samples)
const UPDATE_AT_MENU: i64 = 4_766;
const UPDATE_AT_FIELD: i64 = 95_000;
/// a decision within this many cycles of the cutoff is flagged
pub const NEAR: i64 = 1_500;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SoundSim {
    /// the driver (None until the first frame with sound content)
    pub drv: Option<DriverState>,
    /// play position (samples) at the frame's start, position of the last update
    pub pos: u32,
    pub last: u32,
    /// cycles from the frame's start (VBlank) to its first DMA interrupt
    pub phase: u32,
    /// frames whose main update is already accounted for (a long iteration in progress)
    pub skip: u8,
    /// events of this frame already given to the driver
    pub seen: u32,
    /// redraw decisions closer than [`NEAR`] to the cutoff so far
    pub near: u32,
    /// the last timed iteration: video frames between the frame its work ended in and its last one (1 when the
    /// catch-up sound update ran over a VBlank: the restore's camera update shows a frame earlier)
    #[serde(default)]
    pub wrap: u8,
    /// the state came from a game RAM image without the hardware's timer phase (an emulator state imported
    /// without `snd_phase`): the driver still runs, the pages keep their constant redraw costs
    pub uncalibrated: bool,
    /// the field's load count at the last frame's end, and a song an area load asked for: the game plays it at the
    /// load iteration's end (after the map's decompression), so the interrupt mixes of the load's lag frames still
    /// run without it (ship 8-1: song 28 from video f16,808, not the load's f16,790 - the sequencer was 4 rows ahead)
    #[serde(default)]
    pub loads: u32,
    #[serde(default)]
    pub pending_song: Option<i32>,
}

/// the redraw of a page for the time model: the cycles before it in its frame and its own without the mixer
#[derive(Clone, Copy, Debug)]
pub struct Redraw {
    pub t0: i64,
    pub base: i64,
}

pub fn debug() -> bool {
    use std::sync::OnceLock;
    static D: OnceLock<bool> = OnceLock::new();
    *D.get_or_init(|| std::env::var("WF_SND_DEBUG").is_ok())
}

/// debug: the voices before an update, as the game's RAM has them (pos, step, gain, flags; tracks then voices)
fn dump(what: &str, n: u32, d: &Driver) {
    let st = d.state();
    let v: Vec<String> = st
        .player
        .tracks
        .iter()
        .map(|t| &t.v)
        .chain(st.voices.iter())
        .map(|v| format!("{},{},{},{}", v.pos, v.step, v.gain, v.flags))
        .collect();
    eprintln!("snd {what} n {n} row {} order {} ticks {} cd {} | {}", st.player.row, st.player.order, st.player.ticks_left, st.player.countdown, v.join(" "));
}

/// play position at frame time `t` (interrupts at phase, phase + DMA, ...)
fn pos_at(s: &SoundSim, t: i64) -> u32 {
    let p = s.phase as i64;
    if t < p {
        s.pos
    } else {
        s.pos.wrapping_add(16 * ((t - p) / DMA + 1) as u32)
    }
}

fn apply_events(w: &mut World, d: &mut Driver) {
    let loading = w.field.as_ref().is_some_and(|f| f.loads != w.sound.loads);
    let from = (w.sound.seen as usize).min(w.events.len());
    for e in &w.events[from..] {
        match e {
            Event::Sfx { id, vol } => {
                d.play_sfx(*id as u8, (*vol).clamp(0, 64) as u8, 0);
            }
            Event::SfxStop { id } => d.stop_sfx(*id as u8),
            Event::SfxStopMatch { id } => d.stop_matching_sfx(*id as u8),
            Event::SfxStopAll => d.stop_all_sfx(),
            // the game starts a song only when it is not the one playing (an area change inside the same music:
            // no PlaySong 0x0803F110 call, the sequencer runs on - sb 21-13 -> 21-14)
            Event::Song { id } if loading => {
                w.sound.pending_song = Some(*id);
            }
            Event::Song { id } => {
                let st = d.state();
                if debug() {
                    eprintln!("snd song {id} f{}: playing {} song {:?}", w.frame, st.player.playing, st.player.song);
                }
                if !(st.player.playing && st.player.song == Some(*id as usize)) {
                    d.play_song(*id as usize);
                }
            }
            Event::SongStop => {
                if debug() {
                    eprintln!("snd songstop f{}", w.frame);
                }
                d.stop_song()
            }
            Event::MasterVolumes { music, sfx } => d.set_master_volumes(*music as u8, *sfx as u8),
            _ => {}
        }
    }
    w.sound.seen = w.events.len() as u32;
}

/// run `f` on the world's driver (None without sound content)
fn with_driver<R>(w: &mut World, f: impl FnOnce(&mut World, &mut Driver) -> R) -> Option<R> {
    let bank = w.data.sound.clone()?;
    let mut d = match w.sound.drv.take() {
        Some(st) => Driver::from_state(&bank, st),
        None => Driver::new(&bank),
    };
    let r = f(w, &mut d);
    w.sound.drv = Some(d.state());
    Some(r)
}

/// The end of a frame: the frame's sound events, the main loop's update, the hardware's frame.
pub fn end_frame(w: &mut World) {
    let menu = crate::menu::active(w);
    with_driver(w, |w, d| {
        // a load's song: this logic frame comes after the load's iteration
        let loading = w.field.as_ref().is_some_and(|f| f.loads != w.sound.loads);
        if !loading {
            if let Some(id) = w.sound.pending_song.take() {
                let st = d.state();
                if !(st.player.playing && st.player.song == Some(id as usize)) {
                    d.play_song(id as usize);
                }
            }
        }
        apply_events(w, d);
        if let Some(f) = w.field.as_ref() {
            w.sound.loads = f.loads;
        }
        // an area load with a song (the load iteration runs past this frame's update point: the update is the
        // load's end, `lag_frame_ex`)
        // (the sim's own load lag, `Options::sim_lag`: every area load's iteration runs past this frame's update
        // point - its update is the iteration's end, `load_frames`)
        let load_song = loading && (w.sound.pending_song.is_some() || w.g.load_pending);
        if w.sound.skip > 0 {
            w.sound.skip -= 1;
        } else if load_song {
        } else {
            // a page with its own measured idle step (the DMA interrupts before it delay it by 187 each)
            let page = if menu { crate::menu::idle_cycles(w) } else { None };
            let at = match page {
                Some(t) => t + if (w.sound.phase as i64) < t { 187 } else { 0 },
                None if menu => UPDATE_AT_MENU,
                None => UPDATE_AT_FIELD,
            };
            let p = pos_at(&w.sound, at);
            let n = p.wrapping_sub(w.sound.last) & 1023;
            if debug() {
                eprintln!("snd frame f{} pos {} last {}", w.frame, w.sound.pos, w.sound.last);
                dump("upd", n, d);
            }
            if n >= 64 {
                wf_audio::cost::update(d, n);
                w.sound.last = p;
            }
        }
    });
    w.sound.seen = 0;
    hw_frame(&mut w.sound);
}

/// the hardware's frame: its DMA interrupts
fn hw_frame(s: &mut SoundSim) {
    let p = s.phase as i64;
    let k = (FRAME - p + DMA - 1) / DMA;
    s.pos = s.pos.wrapping_add(16 * k as u32);
    s.phase = (p + k * DMA - FRAME) as u32;
}

/// A video frame the main loop does not finish an iteration in (the game's own lag: loads, long scripts; a host
/// that replays a recording calls this for them): no update, the interrupt mixes when the play position runs
/// [`IRQ_MIX`] ahead.
pub fn lag_frame(w: &mut World) {
    lag_frame_ex(w, false)
}

/// [`lag_frame`]; `last` = the next video frame is a logic frame: a load's iteration ends in this one - the song
/// the load asked for starts (PlaySong after the map's decompression) and the main loop's update follows (ship 8-1:
/// the game's update at the load's last lag frame mixes the new song's first 480 samples)
pub fn lag_frame_ex(w: &mut World, last: bool) {
    with_driver(w, |w, d| {
        if last && w.sound.skip == 0 {
            if let Some(id) = w.sound.pending_song.take() {
                let st = d.state();
                if !(st.player.playing && st.player.song == Some(id as usize)) {
                    d.play_song(id as usize);
                }
                if let Some(f) = w.field.as_ref() {
                    w.sound.loads = f.loads;
                }
                let p = pos_at(&w.sound, UPDATE_AT_FIELD);
                let n = p.wrapping_sub(w.sound.last) & 1023;
                if debug() {
                    eprintln!("snd load-end pos {} last {} phase {}", w.sound.pos, w.sound.last, w.sound.phase);
                    dump("upd", n, d);
                }
                if n >= 64 {
                    wf_audio::cost::update(d, n);
                    w.sound.last = p;
                }
                return;
            }
        }
        let s = &mut w.sound;
        if s.skip > 0 {
            return;
        }
        let ahead = s.pos.wrapping_sub(s.last) & 1023;
        let at_end = ahead + 16 * ((FRAME - s.phase as i64 + DMA - 1) / DMA) as u32;
        if ahead < IRQ_MIX && at_end >= IRQ_MIX {
            if debug() {
                dump("irq", IRQ_MIX, d);
            }
            wf_audio::cost::update(d, IRQ_MIX);
            s.last = s.last.wrapping_add(IRQ_MIX);
        }
    });
    if w.sound.skip > 0 {
        w.sound.skip -= 1;
    }
    hw_frame(&mut w.sound);
}

/// A main-loop iteration with a redraw starting now: the interrupt's mix inside it, the update after it; returns
/// the video frames it takes (None without sound content: the caller keeps its constant).
pub fn redraw_frames(w: &mut World, r: Redraw) -> Option<u8> {
    redraw_frames_irq(w, r, 10_065)
}

/// [`redraw_frames`] with the interrupt mixes' cycles scaled by `irq_scale` / 10,000 (their entry / exit; an area load's
/// base already carries them)
fn redraw_frames_irq(w: &mut World, r: Redraw, irq_scale: i64) -> Option<u8> {
    if w.sound.uncalibrated {
        return None;
    }
    let frame = w.frame;
    with_driver(w, |w, d| {
        apply_events(w, d);
        let s = &mut w.sound;
        let phase = s.phase as i64;
        // the DMA interrupts before the redraw delay it (~187 cycles each)
        let r = Redraw { t0: r.t0 + if phase < r.t0 { 187 * ((r.t0 - phase) / DMA + 1) } else { 0 }, ..r };
        // every interrupt after which the play position is IRQ_MIX ahead mixes (one in a redraw, several in a
        // reload); each mix lengthens the iteration by its cycles and the interrupts' entry / exit around it
        let mut t1 = r.t0 + r.base;
        let mut irq = (0i64, 0i64);
        let ahead = (s.pos.wrapping_sub(s.last) & 1023) as i64;
        for j in 1..200i64 {
            // the j-th mix: the interrupt after which the position is j x IRQ_MIX past the last update's
            let need = IRQ_MIX as i64 * j - ahead;
            if need < 0 {
                break;
            }
            let k = (need + 15) / 16;
            // k == 0: already there at the frame's start (the previous iteration ran long)
            let t = if k == 0 { 0 } else { phase + (k - 1) * DMA };
            if t >= t1 {
                break;
            }
            if debug() {
                dump("irq", IRQ_MIX, d);
            }
            let c = wf_audio::cost::update(d, IRQ_MIX) * irq_scale / 10_000;
            s.last = s.last.wrapping_add(IRQ_MIX);
            t1 += c;
            irq = (t, irq.1 + c);
        }
        let t = t1 + 37;
        let p = pos_at(s, t);
        let n = p.wrapping_sub(s.last) & 1023;
        let mut post = 0;
        if n >= 64 {
            post = wf_audio::cost::update(d, n);
            s.last = p;
        }
        let end = t + post;
        // the iteration ends before the VBlank it then waits for
        let frames = ((end - CUTOFF).div_euclid(FRAME) + 1).clamp(1, 250);
        s.wrap = (frames - 1 - t.div_euclid(FRAME)).clamp(0, 3) as u8;
        let margin = frames * FRAME + CUTOFF - end;
        let near = margin.min(FRAME - margin) < NEAR;
        if near {
            s.near += 1;
        }
        if debug() {
            eprintln!(
                "snd redraw f{frame}: t0 {} base {} irq at {} cost {} post n {} cost {} end {} -> {} frames, margin {}{}",
                r.t0,
                r.base,
                irq.0,
                irq.1,
                n,
                post,
                end,
                frames,
                margin,
                if near { " NEAR" } else { "" }
            );
        }
        s.skip = frames as u8;
        frames as u8
    })
}

/// The pause menu's close: the world is rebuilt in one main-loop iteration. Its cycles: the map loader's
/// decompression (`MapDef.reload_cycles`, +0.35 % for the sound DMA's stolen cycles; ~95 % of it), ~206 k of
/// everything else plus two small sprite-frame streams (~19 k; the first depends on the character, 14.3 k / 17.1 k
/// measured, not modelled), +0.52 % of the elapsed time for the VBlank / DMA interrupt handlers, and an interrupt
/// mix every 928 samples. Returns the video frames the iteration takes (None: unknown map / no sound content /
/// uncalibrated). Against the emulator (work/earth/timing/leaveprof.py): 2-9 5,377,882 vs 5,377,145 cycles
/// before the mixes, 3-15 6,412,147 vs 6,414,682.
pub fn reload_frames(w: &mut World, done: bool) -> Option<u8> {
    reload_frames_with(w, done, 0)
}

/// [`reload_frames`] with `more` cycles of other work in the same iteration (a script-opened screen's own close)
pub fn reload_frames_with(w: &mut World, done: bool, more: i64) -> Option<u8> {
    let (c, extra) = reload_parts(w)?;
    // a close by a prompt (B) restores in the iteration after the event's: less before it (its start varies by
    // ~14 k cycles, not modelled)
    let rest = if done { RELOAD_REST } else { RELOAD_REST - 13_700 };
    redraw_frames(w, Redraw { t0: 0, base: (rest + more + extra + c * 10_035 / 10_000) * 100_523 / 100_000 })
}

/// An area load's main-loop iteration (the frame after the load's logic frame on): the same map rebuild as a
/// close's reload (`reload_parts` at the arrival) plus the area's own setup (`LOAD_REST`: the object lists, the
/// on-load scripts, the HUD). Returns its video frames (None: unknown map / no sound content / uncalibrated)
pub fn load_frames(w: &mut World) -> Option<u8> {
    let (c, extra) = reload_parts(w)?;
    let grids = w.field.as_ref().and_then(|f| w.data.field.as_ref()?.maps.get(&f.map)?.reload.as_ref().map(|r| r.grids)).unwrap_or(0);
    let c = c + grids;
    let map = w.field.as_ref().map(|f| f.map.clone()).unwrap_or_default();
    // a load that shows the area-name banner (`scene::banner_load`: the name differs from the last one shown) draws
    // its picture and glyphs in the same iteration (real run 13-32 -> 13-3 vs 13-2 -> 13-3); fitted over all loads
    let banner = if w.scene.banner.as_ref().is_some_and(|b| b.1 == 0) { load_table().1 } else { 0 };
    let base = (extra + c * 10_035 / 10_000) * 100_523 / 100_000 + load_rest(&map) + banner;
    if debug() {
        eprintln!("snd load: c {c} extra {extra} banner {banner} base {base}");
    }
    // the iteration started with the load's logic frame (`Session::lag_step` asks one frame later): the sound
    // hardware as it was at that frame's start (Thieves Den 8-2 -> 8-3: the mixes and the end's update - 336
    // samples, not 608 - land where the game's do)
    let (pos, phase) = (w.sound.pos, w.sound.phase as i64);
    let p0 = (phase + FRAME) % DMA;
    let k = (phase + FRAME - p0) / DMA;
    w.sound.phase = p0 as u32;
    w.sound.pos = pos.wrapping_sub(16 * k as u32);
    // (the mixes at their own cost: the load's measured own time carries the interrupts' entry / exit - 8-2 -> 8-3's
    // nine mixes 1,170,273 vs the game's 1,170,931)
    let r = redraw_frames_irq(w, Redraw { t0: 0, base }, 10_000);
    w.sound.pos = pos;
    w.sound.phase = phase as u32;
    // (that frame is over: the skip counts the lag frames still to come)
    if r.is_some() {
        w.sound.skip = w.sound.skip.saturating_sub(1);
    }
    r
}

/// A text box's creation (`scene::open_text`, the script's step): one main-loop iteration of the map's box constant
/// (`dialog_rest.json`: the box's own work - window, font, the window graphics' decompression, ~1.82 M - plus the
/// scene's per-frame work on that map, which differs by ~25 k between maps) + 128 per character of the first page +
/// the portrait's decompression (portraits/index.json `decode_cycles`; none for 0 / 7) + the palette fade's last pass
/// when a screen fade runs or ends in this frame (`fade`, 35.7 k). 683 opens measured on the emulator (every real-run
/// cut + the whole run's prologue; work/shipfix/dlg_measure2.py, mk_dlgrest.py): per-map constants leave 469 within
/// 5 k. Returns the iteration's video frames (None: uncalibrated / no sound)
pub fn dialog_frames(w: &mut World, portrait: u8, first_chars: i64) -> Option<u8> {
    let dec = if portrait == 0 || portrait == 7 {
        0
    } else {
        w.data.menu.as_ref().and_then(|m| m.portrait_cycles.get(portrait as usize - 8).copied()).unwrap_or(0) as i64
    };
    let (default, fade_pass, maps) = dialog_table();
    let rest = w.field.as_ref().and_then(|f| maps.get(&f.map).copied()).unwrap_or(*default);
    // (a fade that ended earlier in this frame also runs its last pass in the iteration - not tracked: sim lag is
    // deprecated, segments.md §9)
    let fade = if w.scene.screen.fade.is_some() { *fade_pass } else { 0 };
    let base = rest + 128 * first_chars + dec + fade;
    if debug() {
        eprintln!(
            "snd dialog: portrait {portrait} chars {first_chars} decode {dec} rest {rest} fade {fade} base {base} map {}",
            w.field.as_ref().map(|f| f.map.as_str()).unwrap_or("-"),
        );
    }
    redraw_frames(w, Redraw { t0: 0, base })
}

/// dialog_rest.json: (default, the fade pass, per-map constants)
fn dialog_table() -> &'static (i64, i64, std::collections::HashMap<String, i64>) {
    static T: std::sync::OnceLock<(i64, i64, std::collections::HashMap<String, i64>)> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(include_str!("dialog_rest.json")).unwrap_or_default();
        let maps = v["maps"].as_object().map(|o| o.iter().map(|(k, x)| (k.clone(), x.as_i64().unwrap_or(0))).collect()).unwrap_or_default();
        (v["default"].as_i64().unwrap_or(1_928_650), v["fade"].as_i64().unwrap_or(0), maps)
    })
}

/// The map rebuild's cycles for the current area / camera / actors (pure): (decompression incl. the actors' frames,
/// the other per-item work). The close's reload and an area load (`load_frames`) both rebuild the map this way
pub fn reload_parts(w: &World) -> Option<(i64, i64)> {
    let f = w.field.as_ref()?;
    let fd = w.data.field.clone()?;
    let rl = fd.maps.get(&f.map)?.reload.as_ref()?;
    let (cx, cy) = (w.cameras[0].rect.x0 as i64, w.cameras[0].rect.y0 as i64);
    let mut c = rl.fixed;
    let mut extra = 0;
    for l in &rl.layers {
        if l.cols * l.rows <= 4 {
            // loaded whole into the layer's four chunk slots
            c += l.ew.iter().sum::<i64>();
            continue;
        }
        // a larger layer: the chunks under the 256 x 256 px tile screen centred on the screen's centre at the
        // layer's scroll, through a buffer (~5 k cycles of copying per chunk). VERIFIED on
        // 8-3 at camera (0, 347): layer offsets 0 / 128 / 192 px load columns {0} / {0, 1} / {0, 1}, rows {1, 2}
        let sx = ((cx * l.rate.0 + l.offset.0) >> 10) + 120 - 128;
        let sy = ((cy * l.rate.1 + l.offset.1) >> 10) + 80 - 128;
        for row in (sy >> 8)..=((sy + 255) >> 8) {
            for col in (sx >> 8)..=((sx + 255) >> 8) {
                if (0..l.cols as i64).contains(&col) && (0..l.rows as i64).contains(&row) {
                    let v = l.iw.get((row * l.cols as i64 + col) as usize).copied().unwrap_or(0);
                    if v != 0 {
                        c += v;
                        extra += 5_000;
                    }
                }
            }
        }
    }
    // the listed tiles' copy to VRAM
    extra += 375 * rl.tiles;
    // every on-screen actor's current frame is decompressed again (0x0803D968; remapped sprites through the
    // remapping decoder), ~4 k cycles of upload each
    let mut frames = 0;
    let mut actors = 0;
    for i in 0..w.ents.len() {
        if w.ents[i].removed || w.actor(i).is_none() || !crate::vram::on_screen(w, i) {
            continue;
        }
        let d = crate::scene::cel_dec(w, i) as i64;
        if d > 0 {
            frames += d;
            actors += 1;
            extra += 4_000;
        }
    }
    // the level signs' labels (`0x0800C300` objects) are drawn into their surfaces again ("%d", `0x0800C298`):
    // ~25 k each (real run 14-1, three signs: the close +76 k over the model without them, the same tileset's 14-2
    // without signs within 5 k; profile: the glyph code 0x0803F6xx only in 14-1)
    let signs = f.signs as i64;
    extra += SIGN_LABEL * signs;
    // an on-screen item on the ground: its two 16 x 16 pictures (measured 6,845 + 7,077 cycles, 8-6)
    let cam = w.cameras[0].rect;
    for e in w.ents.iter().filter(|e| !e.removed) {
        if let crate::world::Body::Pickup(p) = &e.body {
            let (x, y) = (p.x >> 8, (p.y >> 8) - (p.z >> 10));
            if !matches!(p.kind, crate::fieldobj::PickupKind::Zenie(_)) && x < cam.x1 && cam.x0 < x + 16 && y < cam.y1 && cam.y0 < y + 16 {
                frames += 13_900;
            }
        }
    }
    c += frames;
    if debug() {
        eprintln!("snd reload: map {} cam ({cx}, {cy}) decompression {c} ({actors} actors' frames {frames}) + {extra}", f.map);
    }
    Some((c, extra))
}

/// (+2,500 on 2026-10-04: 49 measured pause / Switch Character closes over the real run, game minus model: mean
/// +6,563, median +6,610; bounded by real run 2-9 rE2__050, 3.4 k from its boundary, and 18-2 r038_002, 1.7 k)
const RELOAD_REST: i64 = 119_300;
const SIGN_LABEL: i64 = 25_300;
/// An area load's work beyond the map rebuild and the walk / shot grids (the area's setup: object lists, load scripts,
/// HUD): measured per destination map on every load between the real run's cuts (745 loads, 206 maps; the median per
/// map, the loads into one map agree within +-16 k std) - no map feature tested (tiles, layers, chunks, actors,
/// sprites, signs) explains it across maps (std 80 k). Unmeasured maps: the median of all (load_rest.json)
fn load_rest(map: &str) -> i64 {
    let (d, _, m) = load_table();
    m.get(map).copied().unwrap_or(*d)
}
/// load_rest.json: (default, banner term, per-map constants without the banner)
fn load_table() -> &'static (i64, i64, std::collections::HashMap<String, i64>) {
    static T: std::sync::OnceLock<(i64, i64, std::collections::HashMap<String, i64>)> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(include_str!("load_rest.json")).unwrap_or_default();
        let maps = v["maps"].as_object().map(|o| o.iter().map(|(k, x)| (k.clone(), x.as_i64().unwrap_or(0))).collect()).unwrap_or_default();
        (v["default"].as_i64().unwrap_or(260_000), v["banner"].as_i64().unwrap_or(0), maps)
    })
}

/// the iteration makes the VBlank when it ends this many cycles before it at the latest (the wait's own entry)
const CUTOFF: i64 = 0;

/// A long iteration of `frames` video frames whose length is not modelled (other pages' redraws): the driver
/// goes through the same interrupt mix and catch-up update.
pub fn long_iteration(w: &mut World, frames: u8) {
    if frames < 2 {
        return;
    }
    with_driver(w, |w, d| {
        apply_events(w, d);
        let s = &mut w.sound;
        let end = frames as i64 * FRAME - 20_000;
        let ahead = s.pos.wrapping_sub(s.last) & 1023;
        if ahead <= IRQ_MIX {
            let k = ((IRQ_MIX - ahead + 15) / 16) as i64;
            let t = if k == 0 { 0 } else { s.phase as i64 + (k - 1) * DMA };
            if t < end {
                wf_audio::cost::update(d, IRQ_MIX);
                s.last = s.last.wrapping_add(IRQ_MIX);
            }
        }
        let p = pos_at(s, end);
        let n = p.wrapping_sub(s.last) & 1023;
        if n >= 64 {
            wf_audio::cost::update(d, n);
            s.last = p;
        }
        s.skip = frames;
    });
}

/// instructions the shadowed glyph routine `0x0803F5A4` runs for one glyph (columns: bit 0 = top row), counted
/// like the measurements this model was fitted on (pc in 0x0803F61C..0x0803F700)
pub fn glyph_instructions(cols: &[u16]) -> i64 {
    let w = cols.len();
    // setup 24, the column copy 13 per column, 27 before the loop, 6 after
    let mut n = 24 + 13 * w as i64 + 27 + 6;
    for k in 0..=w {
        let g = if k < w { cols[k] as u32 } else { 0 };
        let sh = if k > 0 { (cols[k - 1] as u32) << 1 } else { 0 };
        n += 26;
        let (mut g, mut sh) = (g, sh);
        while g | sh != 0 {
            n += if g & 1 != 0 {
                12
            } else if sh & 1 != 0 {
                13
            } else {
                11
            };
            g >>= 1;
            sh >>= 1;
        }
    }
    n
}

/// instructions of the plain glyph routine's column loop (`0x0803F500..0x0803F532`) for one glyph
pub fn plain_glyph_instructions(cols: &[u16]) -> i64 {
    let mut n = 0;
    for &c in cols {
        n += 17;
        let mut c = c as u32;
        while c != 0 {
            n += if c & 1 != 0 { 9 } else { 7 };
            c >>= 1;
        }
    }
    n + 6
}

/// What a page's draw list costs beyond the page's constant part: glyph-routine instructions of the shadowed
/// text, the 16 x 16 pictures (the cursor, the prompts), and the cycles of the plain text lines, fills and pixels
/// (the tabs: one per party member). Per-unit costs from emulator profiles (`work/earth/timing/prof.py`): a plain
/// text line 990 cycles + 315 per glyph + 3.227 per column-loop instruction, a fill row 54 + 4.25 per pixel, a
/// pixel 45.
pub fn ops_cost(t: &crate::menu::text::TextData, ops: &[MenuOp]) -> (i64, i64, i64) {
    let mut gn = 0;
    let mut small = 0;
    let mut extra = 0;
    for op in ops {
        match op {
            MenuOp::Text { font, shadow, text, .. } => {
                let fm = t.font(*font);
                if shadow.is_none() {
                    extra += 990;
                }
                let mut k = 0;
                while k < text.len() {
                    let c = text[k];
                    if c == 8 && k + 1 < text.len() {
                        k += 2;
                        continue;
                    }
                    k += 1;
                    if c < 0xFF && fm.has.get(c as usize).copied().unwrap_or(false) {
                        let cols = fm.cols.get(c as usize).map(|v| v.as_slice()).unwrap_or(&[]);
                        if shadow.is_some() {
                            gn += glyph_instructions(cols);
                        } else {
                            extra += 315 + plain_glyph_instructions(cols) * 3_227 / 1000;
                        }
                    }
                }
            }
            MenuOp::Image { fw: 16, fh: 16, .. } => small += 1,
            MenuOp::Fill { x0, y0, x1, y1, .. } => extra += (y1 - y0).max(0) as i64 * (54 + (x1 - x0).max(0) as i64 * 425 / 100),
            MenuOp::Pixel { .. } => extra += 45,
            _ => {}
        }
    }
    (gn, small, extra)
}

/// shadowed text of a draw list: (glyphs drawn, their columns, set pixels, lines)
pub fn text_counts(t: &crate::menu::text::TextData, ops: &[MenuOp]) -> (i64, i64, i64, i64) {
    let (mut g, mut c, mut b, mut l) = (0, 0, 0, 0);
    for op in ops {
        if let MenuOp::Text { font, shadow: Some(_), text, .. } = op {
            let fm = t.font(*font);
            l += 1;
            let mut k = 0;
            while k < text.len() {
                let ch = text[k];
                if ch == 8 && k + 1 < text.len() {
                    k += 2;
                    continue;
                }
                k += 1;
                if ch < 0xFF && fm.has.get(ch as usize).copied().unwrap_or(false) {
                    let cols = fm.cols.get(ch as usize).map(|v| v.as_slice()).unwrap_or(&[]);
                    g += 1;
                    c += cols.len() as i64;
                    b += cols.iter().map(|x| x.count_ones() as i64).sum::<i64>();
                }
            }
        }
    }
    (g, c, b, l)
}

/// Cycles of a draw list for the pages whose constant part is measured per page (inventory, journal, ...): the
/// shadowed glyphs (3.5664 per glyph-routine instruction), the pictures (16 x 16: 7,284, the keyed cursor 6,596,
/// others 28.45 per pixel: the blit `0x0803F922`), and the plain text / fills / pixels of [`ops_cost`]. The
/// backgrounds, panels and the surface copies are part of the page's constant.
pub fn layer_cycles(t: &crate::menu::text::TextData, ops: &[MenuOp], line: i64) -> i64 {
    let (gn, small, extra) = ops_cost(t, ops);
    let lines = text_counts(t, ops).3;
    // the shadowed glyph routine: 3.17 cycles per counted instruction + `line` per text line (the caller's part,
    // fitted per page: six inventory lists agree within 250 cycles; the status page's 3.5664 is its own fit)
    let mut c = gn * 31_700 / 10_000 + lines * line + small * 7_284 + extra;
    for op in ops {
        if let MenuOp::Image { path, fw, fh, .. } = op {
            if (*fw, *fh) != (16, 16) {
                c += *fw as i64 * *fh as i64 * 2_845 / 100;
            } else if path.ends_with("cursor.png") {
                c -= 688;
            }
        }
    }
    c
}

/// debug: a draw list's operations other than shadowed text, compact
pub fn ops_summary(ops: &[MenuOp]) -> String {
    let mut out = Vec::new();
    let mut px = 0;
    for op in ops {
        match op {
            MenuOp::Text { shadow: Some(_), .. } => {}
            MenuOp::Pixel { .. } => px += 1,
            MenuOp::Text { font, text, .. } => out.push(format!("text[{}]{}", font.id(), String::from_utf16_lossy(text))),
            MenuOp::TextOutline { font, text, .. } => out.push(format!("outline[{}]{}", font.id(), String::from_utf16_lossy(text))),
            MenuOp::Fill { x0, y0, x1, y1, .. } => out.push(format!("fill{}x{}", x1 - x0, y1 - y0)),
            MenuOp::Panel { x0, y0, x1, y1 } => out.push(format!("panel{}x{}", x1 - x0, y1 - y0)),
            MenuOp::Background { w, rows, .. } => out.push(format!("bg{w}x{rows}")),
            MenuOp::Image { path, fw, fh, .. } => out.push(format!("img{fw}x{fh}:{}", path.rsplit('/').next().unwrap_or(""))),
            _ => out.push("other".into()),
        }
    }
    format!("{} px{px}", out.join(" "))
}

/// The driver from the game's RAM: `ram` = player header + tracks + voices (`0x030038A4`, 0x18 + 15 x 0x60 +
/// 16 x 0x40), the sound object's counters (`0x03003898` +4 / +8) and the cycles to the next DMA interrupt.
pub fn import(w: &mut World, ram: &[u8], pos: u32, last: u32, phase: Option<u32>) {
    let Some(bank) = w.data.sound.clone() else { return };
    let mut d = Driver::new(&bank);
    d.seed_ram(ram);
    w.sound = SoundSim { drv: Some(d.state()), pos, last, phase: phase.unwrap_or(0), uncalibrated: phase.is_none(), loads: w.field.as_ref().map(|f| f.loads).unwrap_or(0), ..Default::default() };
}
