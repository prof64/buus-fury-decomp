//! What the rollback sessions need from a simulation: save / load a state, advance one frame with every slot's
//! input, and a checksum. `wf_core` implements it in [`crate::core_game`]; the tests also use a toy game.

pub trait Game {
    /// An in-process snapshot (for wf-core: a `World` clone, ~1.5 µs).
    type State: Clone;
    fn save(&self) -> Self::State;
    fn load(&mut self, s: &Self::State);
    /// One frame with the held buttons of every slot. `first` = this frame is simulated for the first time (its
    /// presentation events are kept); re-simulations during a rollback pass `false`.
    fn advance(&mut self, inputs: &[u16], first: bool);
    /// Checksum of a saved state (the per-frame desync check).
    fn checksum(&self, s: &Self::State) -> u64;
    /// Serialized form of the current state (desync reports, the sync test's byte round-trip mode).
    fn save_bytes(&self) -> Option<Vec<u8>> {
        None
    }
    fn load_bytes(&mut self, _b: &[u8]) -> bool {
        false
    }
}

/// Hash chain: `chain[k] = mix(chain[k-1], hash[k])`. Comparing one chain value compares every frame before it.
pub fn chain_mix(prev: u64, h: u64) -> u64 {
    (prev.rotate_left(23) ^ h).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (h >> 29)
}
