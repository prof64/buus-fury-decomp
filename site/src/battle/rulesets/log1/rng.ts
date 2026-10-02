// LoG1 random number generators (re/log1/notes.md §8, data/log1/rng.json, re/log1/model.py).
//
// Main RNG 0x08005CA8, state a=0x0300098C b=0x03000990 (zero at boot, never seeded):
//   a' = (a + 0x02CBBC71) ^ b
//   b' = (rotl(b,3) + 0x0632D80F) ^ a'
//   rand(n) = asr32(int32(((a'*b') & 0xFFFF) * (n+1)), 16)    -> [0, n] INCLUSIVE; negative when the 32-bit
//                                                                product reaches 2^31 (n >= 32768: game bug)
// Secondary RNG 0x0800A6F4 (state 0x03003D24/28): same recurrence, raw output a'*b' (32 bit);
//   rand2(n) = (((raw >> 16) * n) >> 16) -> [0, n-1]  (punch damage).

import { RecordingRng } from '../../core/rng'

function stepState(s: { a: number; b: number }): void {
  const a = (((s.a + 0x02cbbc71) >>> 0) ^ s.b) >>> 0
  const rol = ((s.b << 3) | (s.b >>> 29)) >>> 0
  const b = (((rol + 0x0632d80f) >>> 0) ^ a) >>> 0
  s.a = a
  s.b = b
}

export class Log1Rng extends RecordingRng {
  a: number
  b: number
  constructor(a = 0, b = 0) {
    super()
    this.a = a >>> 0
    this.b = b >>> 0
  }
  /** raw step: returns (a'*b') & 0xFFFF (the only part rand() uses) */
  protected step(): number {
    stepState(this)
    return Math.imul(this.a, this.b) & 0xffff
  }
  getState(): number[] {
    return [this.a, this.b]
  }
  setState(s: number[]) {
    this.a = s[0] >>> 0
    this.b = s[1] >>> 0
  }
  /** The game's rand(n): integer in [0, n] (inclusive), with the 32-bit signed overflow of `muls` kept. */
  randInc(n: number, label?: string, noOverflow = false): number {
    const p = this.step()
    const v = noOverflow ? Math.floor((p * (n + 1)) / 65536) : Math.imul(p, (n + 1) | 0) >> 16
    this.record('rand', n, v, label)
    return v
  }
  /** core Rng contract (0..n-1) mapped onto the game routine */
  rand(n: number, label?: string): number {
    return this.randInc(n - 1, label)
  }
  /** "frames before the first gameplay call": the game never seeds, so the state depends on how many calls ran */
  static fromSeed(calls: number): Log1Rng {
    const r = new Log1Rng(0, 0)
    for (let i = 0; i < calls; i++) stepState(r)
    return r
  }
}

/** Secondary generator (punch damage, cosmetic rolls). */
export class Log1Rng2 {
  a: number
  b: number
  calls = 0
  constructor(a = 0, b = 0) {
    this.a = a >>> 0
    this.b = b >>> 0
  }
  raw(): number {
    stepState(this)
    this.calls++
    return Math.imul(this.a, this.b) >>> 0
  }
  /** 0x0800A714: [0, n-1] */
  rand2(n: number): number {
    return Math.floor(((this.raw() >>> 16) * n) / 65536)
  }
  /** 0x0800A724 chance(p): (raw >> 16) < p (the per-frame call) */
  chance(p: number): boolean {
    return this.raw() >>> 16 < p
  }
  /** output the game would produce for the CURRENT state (verify_melee.py reconstruction) */
  static outputOf(a: number, b: number): number {
    return Math.imul(a >>> 0, b >>> 0) >>> 0
  }
}
