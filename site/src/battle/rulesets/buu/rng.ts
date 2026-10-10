// Buu's Fury PRNG (0x0803FAAC, state IWRAM 0x03004A5C/60; re/buu/notes.md §1, data/buu/rng.json).
// It is the same generator as LoG2 (same constants, rand(n) = (next&0xFFFF)*n>>16, chance(p) = (next&0xFFFF)<p),
// so the LoG2 class is reused; only the seeding convention is ours (no title-screen entropy is modelled).

import { Log2Rng } from '../log2/rng'

export class BuuRng extends Log2Rng {
  /** deterministic start state from a user seed (the game's state is whatever the boot sequence left) */
  static fromSeed(seed: number): BuuRng {
    const r = new BuuRng(0, 0)
    for (let i = 0; i < (seed >>> 0) % 100000; i++) r.next()
    r.calls = 0
    r.history = []
    return r
  }
}
