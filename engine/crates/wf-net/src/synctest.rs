//! Local sync test (GGPO / GGRS `SyncTestSession`): every frame is simulated, then the session loads the state
//! `check_distance` frames back, re-simulates those frames with the same inputs and compares every frame's checksum
//! with the first run. Any difference is nondeterminism (state outside the snapshot, uninitialised data, wall-clock
//! or iteration-order dependence, a restore that misses something). With `bytes = true` the rollback goes through
//! the serialized snapshot (`save_bytes` / `load_bytes`) instead of the in-process clone, which also checks the
//! cross-process form.

use std::collections::VecDeque;

use crate::game::Game;

#[derive(Clone, Debug, PartialEq)]
pub struct SyncError {
    /// state index whose checksum differed
    pub frame: i32,
    pub first: u64,
    pub resim: u64,
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sync test: state {} hashed {:016x} first, {:016x} after a rollback", self.frame, self.first, self.resim)
    }
}

pub struct SyncTestSession<G: Game> {
    pub game: G,
    pub check_distance: u32,
    pub bytes: bool,
    frame: i32,
    /// (state index, state) for the last check_distance + 1 states
    states: VecDeque<(i32, G::State)>,
    byte_states: VecDeque<(i32, Vec<u8>)>,
    /// checksum of each state of the first run, by index (last check_distance + 1)
    sums: VecDeque<(i32, u64)>,
    inputs: VecDeque<(i32, Vec<u16>)>,
    pub frames_checked: u64,
    pub resim_frames: u64,
}

impl<G: Game> SyncTestSession<G> {
    pub fn new(game: G, check_distance: u32) -> Self {
        let mut s = SyncTestSession {
            game,
            check_distance: check_distance.max(1),
            bytes: false,
            frame: 0,
            states: VecDeque::new(),
            byte_states: VecDeque::new(),
            sums: VecDeque::new(),
            inputs: VecDeque::new(),
            frames_checked: 0,
            resim_frames: 0,
        };
        s.save();
        s
    }

    pub fn frame(&self) -> i32 {
        self.frame
    }

    fn save(&mut self) {
        let st = self.game.save();
        let h = self.game.checksum(&st);
        self.sums.push_back((self.frame, h));
        if self.bytes {
            if let Some(b) = self.game.save_bytes() {
                self.byte_states.push_back((self.frame, b));
            }
        }
        self.states.push_back((self.frame, st));
        let keep = self.check_distance as usize + 1;
        while self.states.len() > keep {
            self.states.pop_front();
        }
        while self.byte_states.len() > keep {
            self.byte_states.pop_front();
        }
        while self.sums.len() > keep {
            self.sums.pop_front();
        }
        while self.inputs.len() > keep {
            self.inputs.pop_front();
        }
    }

    /// One frame with these inputs (first run, presentation events kept), then the rollback check.
    pub fn advance(&mut self, inputs: &[u16]) -> Result<(), SyncError> {
        self.game.advance(inputs, true);
        self.inputs.push_back((self.frame, inputs.to_vec()));
        self.frame += 1;
        self.save();
        let d = self.check_distance as i32;
        if self.frame < d {
            return Ok(());
        }
        let from = self.frame - d;
        let loaded = if self.bytes {
            self.byte_states.iter().find(|(i, _)| *i == from).map(|(_, b)| b.clone()).is_some_and(|b| self.game.load_bytes(&b))
        } else {
            false
        };
        if !loaded {
            let Some((_, st)) = self.states.iter().find(|(i, _)| *i == from) else { return Ok(()) };
            let st = st.clone();
            self.game.load(&st);
        }
        // the loaded state itself must hash the same
        let check = |s: &Self, k: i32| -> Result<(), SyncError> {
            let st = s.game.save();
            let h = s.game.checksum(&st);
            let first = s.sums.iter().find(|(i, _)| *i == k).map(|x| x.1).unwrap_or(h);
            if first != h {
                return Err(SyncError { frame: k, first, resim: h });
            }
            Ok(())
        };
        check(self, from)?;
        for f in from..self.frame {
            let inp = self.inputs.iter().find(|(i, _)| *i == f).map(|x| x.1.clone()).unwrap_or_default();
            self.game.advance(&inp, false);
            self.resim_frames += 1;
            check(self, f + 1)?;
        }
        self.frames_checked += 1;
        Ok(())
    }
}
