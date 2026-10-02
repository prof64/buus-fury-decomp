// LoG2 RNG (ROM 0x080213FC, state 2 x u32 at 0x03003890) — re/log2/notes.md Part D §5, data/log2/rng.json.
//   r1 = (a + 0x02CBBC71) ^ b
//   r0 = (rol(b,3) + 0x0632D80F) ^ r1
//   a = r1; b = r0; return r1 - r0
//   rand(n)   = ((rng() & 0xFFFF) * n) >> 16     (0x0802141C)
//   chance(p) = (rng() & 0xFFFF) < p             (0x0802142E)

import { RecordingRng } from '../../core/rng'

export class Log2Rng extends RecordingRng {
  a: number
  b: number
  constructor(a = 0, b = 0) {
    super()
    this.a = a >>> 0
    this.b = b >>> 0
  }
  protected step(): number {
    const r1 = ((this.a + 0x02cbbc71) >>> 0) ^ this.b
    const rol = ((this.b << 3) | (this.b >>> 29)) >>> 0
    const r0 = (((rol + 0x0632d80f) >>> 0) ^ r1) >>> 0
    this.a = r1 >>> 0
    this.b = r0
    return (r1 - r0) >>> 0
  }
  getState(): number[] {
    return [this.a, this.b]
  }
  setState(s: number[]) {
    this.a = s[0] >>> 0
    this.b = s[1] >>> 0
  }
  /**
   * The game never seeds the RNG explicitly: it starts at (0,0) and the title screen advances it once per frame,
   * so the effective seed is "frames spent on the title". We reproduce exactly that.
   */
  static fromTitleFrames(frames: number): Log2Rng {
    const r = new Log2Rng(0, 0)
    for (let i = 0; i < frames; i++) r.next()
    r.calls = 0
    r.history = []
    return r
  }
}
