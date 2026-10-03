//! CPU cost of the sound driver's update (`0x0803F296`), in GBA CPU cycles, from the work the port does.
//!
//! The game's timing depends on it in one place: a menu page redraw holds the main loop for ~3.8 frames, the
//! DMA-completion interrupt then runs the update itself (one 928-sample mix), and whether the loop makes the fourth
//! VBlank depends on how long that mix and the following catch-up update take (`engine/docs/segments.md` §9,
//! "Redraw timing"). This is a bounded-error model, not a cycle-exact one: the inner loop is exact
//! ([`crate::driver::inner_cycles`]), the Thumb side is a linear fit of the driver's work counters against the
//! emulator's cycle counter (std ~125 cycles, max ~360 over 561 updates; the residual is the FIFO DMA's cycle
//! stealing and ROM prefetch state).

use crate::driver::{Driver, Profile};

/// milli-cycles per unit of each work counter (fit: work/earth/timing/fit_upd.py)
#[derive(Clone, Copy, Debug)]
pub struct UpdateCost {
    pub base: i64,
    pub run: i64,
    pub silent_run: i64,
    pub track_mix: i64,
    pub sfx_mix: i64,
    pub split: i64,
    pub upd_vol: i64,
    pub upd_entry: i64,
    pub upd_restart: i64,
    pub upd_pitch: i64,
    pub upd_live: i64,
    pub tick: i64,
    pub row: i64,
    pub cell: i64,
    pub tick_fn: i64,
    pub chunk: i64,
    pub segment: i64,
    pub sample: i64,
}

pub const UPDATE_COST: UpdateCost = UpdateCost {
    base: 1_299_340,
    run: 93_507,
    silent_run: 71_427,
    track_mix: 14_043,
    sfx_mix: -4_494,
    split: 165_619,
    upd_vol: -73_583,
    upd_entry: 210_997,
    upd_restart: 192_874,
    upd_pitch: -90_093,
    upd_live: 33_814,
    tick: 1_084_109,
    row: 1_197_575,
    cell: 355_668,
    tick_fn: 197_937,
    chunk: 76_740,
    segment: 1_160_849,
    sample: 24_814,
};

/// Cycles of an update whose work was `p` (0 for an update that mixed nothing).
pub fn update_cycles(p: &Profile) -> i64 {
    if p.chunks == 0 {
        return 0;
    }
    let c = &UPDATE_COST;
    let m = c.base
        + c.run * p.runs as i64
        + c.silent_run * p.silent_runs as i64
        + c.track_mix * p.track_mix as i64
        + c.sfx_mix * p.sfx_mix as i64
        + c.split * p.splits as i64
        + c.upd_vol * p.upd_vol as i64
        + c.upd_entry * p.upd_entry as i64
        + c.upd_restart * p.upd_restart as i64
        + c.upd_pitch * p.upd_pitch as i64
        + c.upd_live * p.upd_live as i64
        + c.tick * p.ticks as i64
        + c.row * p.rows as i64
        + c.cell * p.cells as i64
        + c.tick_fn * p.tick_fns as i64
        + c.chunk * p.chunks as i64
        + c.segment * p.segments as i64
        + c.sample * p.samples as i64;
    p.inner as i64 + m / 1000
}

/// The driver update for `n` pending samples (`0x0803F296`: nothing below 64, chunks of at most 512), without
/// producing output; returns its cycle cost.
pub fn update(d: &mut Driver, n: u32) -> i64 {
    if n < 64 {
        return 0;
    }
    crate::driver::profile_begin();
    let mut left = n as usize;
    while left > 0 {
        let c = left.min(512);
        d.advance(c);
        left -= c;
    }
    let p = crate::driver::profile_end();
    update_cycles(&p)
}
