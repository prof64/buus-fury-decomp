//! Periodic snapshots of a running sim: seek anywhere (restore the nearest snapshot at or before the target,
//! re-simulate the logged inputs), rewind, branch. Dense inside the rewind window, sparse before it.

use std::collections::BTreeMap;

use wf_view::{Sim, SimEvent};

use crate::{lz, InputLog};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineConfig {
    /// Snapshot every `interval` frames inside the rewind window.
    pub interval: u32,
    /// Rewind window (frames behind the newest snapshot that keep every snapshot).
    pub window: u64,
    /// Before the window keep one snapshot per `sparse_every` frames (seeking there re-simulates more).
    pub sparse_every: u32,
    /// LZ-compress stored snapshots (~10x smaller, ~0.1 ms each).
    pub compress: bool,
    /// Memory budget; the oldest sparse snapshots go first (the start one is always kept).
    pub max_bytes: usize,
}

impl Default for TimelineConfig {
    fn default() -> Self {
        TimelineConfig { interval: 30, window: 60 * 60, sparse_every: 240, compress: true, max_bytes: 64 << 20 }
    }
}

impl TimelineConfig {
    /// A config whose dense window is `seconds` of play at the GBA frame rate.
    pub fn with_rewind_seconds(seconds: f64) -> Self {
        TimelineConfig { window: (seconds.max(1.0) * wf_view::GBA_FPS) as u64, ..Default::default() }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Timeline {
    pub cfg: TimelineConfig,
    snaps: BTreeMap<u64, Vec<u8>>,
    bytes: usize,
    /// frames re-simulated by seeks (perf counters)
    pub resimulated: u64,
}

impl Timeline {
    pub fn new(cfg: TimelineConfig) -> Timeline {
        Timeline { cfg, ..Default::default() }
    }
    pub fn clear(&mut self) {
        self.snaps.clear();
        self.bytes = 0;
    }
    pub fn len(&self) -> usize {
        self.snaps.len()
    }
    pub fn is_empty(&self) -> bool {
        self.snaps.is_empty()
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn earliest(&self) -> Option<u64> {
        self.snaps.keys().next().copied()
    }
    pub fn has(&self, frame: u64) -> bool {
        self.snaps.contains_key(&frame)
    }

    /// Store a snapshot of the sim's current frame (no-op when the sim cannot snapshot).
    pub fn capture(&mut self, sim: &dyn Sim) {
        let f = sim.frame();
        if self.snaps.contains_key(&f) {
            return;
        }
        let Some(s) = sim.snapshot() else { return };
        let s = if self.cfg.compress { lz::compress(&s) } else { s };
        self.bytes += s.len();
        self.snaps.insert(f, s);
        self.prune();
    }

    /// Call after every step: captures on interval frames.
    pub fn on_frame(&mut self, sim: &dyn Sim) {
        if sim.frame() % self.cfg.interval.max(1) as u64 == 0 {
            self.capture(sim);
        }
    }

    /// Drop snapshots after `frame` (the inputs after it changed).
    pub fn invalidate_after(&mut self, frame: u64) {
        let gone = self.snaps.split_off(&(frame + 1));
        self.bytes -= gone.values().map(|v| v.len()).sum::<usize>();
    }

    fn prune(&mut self) {
        let (Some(&first), Some(&head)) = (self.snaps.keys().next(), self.snaps.keys().next_back()) else { return };
        let edge = head.saturating_sub(self.cfg.window);
        let sparse = self.cfg.sparse_every.max(1) as u64;
        let drop: Vec<u64> = self.snaps.range(..edge).map(|(&f, _)| f).filter(|&f| f != first && f % sparse != 0).collect();
        for f in drop {
            self.bytes -= self.snaps.remove(&f).map(|v| v.len()).unwrap_or(0);
        }
        while self.bytes > self.cfg.max_bytes && self.snaps.len() > 2 {
            let f = *self.snaps.keys().nth(1).unwrap();
            if f >= edge {
                break;
            }
            self.bytes -= self.snaps.remove(&f).map(|v| v.len()).unwrap_or(0);
        }
    }

    fn restore(&self, sim: &mut dyn Sim, f: u64) -> bool {
        let Some(s) = self.snaps.get(&f) else { return false };
        if self.cfg.compress {
            lz::decompress(s).is_ok_and(|b| sim.restore(&b))
        } else {
            sim.restore(s)
        }
    }

    /// Move `sim` to `target` (clamped to what `log` covers): forward by stepping, backward by restoring the
    /// nearest snapshot at or before it and stepping. Events of re-simulated frames are dropped. Returns the
    /// frame reached.
    pub fn seek(&mut self, sim: &mut dyn Sim, log: &InputLog, target: u64) -> u64 {
        let target = target.min(log.end());
        let cur = sim.frame();
        if target == cur {
            return cur;
        }
        let snap = self.snaps.range(..=target).next_back().map(|(&f, _)| f);
        let from_cur = cur < target && cur >= log.start;
        if !(from_cur && snap.is_none_or(|s| s <= cur)) {
            match snap {
                Some(s) if self.restore(sim, s) => {}
                _ if from_cur => {}
                _ => return sim.frame(),
            }
        }
        let mut ev: Vec<SimEvent> = Vec::new();
        while sim.frame() < target {
            let Some(inp) = log.get(sim.frame()) else { break };
            ev.clear();
            sim.step(inp, &mut ev);
            self.resimulated += 1;
            self.on_frame(sim);
        }
        sim.frame()
    }
}
