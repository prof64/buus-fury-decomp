// RNG contract shared by all rulesets. Each ruleset implements its own game's generator; the core only needs
// rand/chance and some introspection for the debug overlay.

export interface RngCall {
  frame: number
  fn: 'rand' | 'chance' | 'next'
  arg: number
  result: number
  label: string
}

export interface Rng {
  /** Raw 32-bit output (unsigned). */
  next(label?: string): number
  /** 0..n-1 */
  rand(n: number, label?: string): number
  /** probability p/65536 (the WF convention); rulesets with other conventions document it */
  chance(p: number, label?: string): boolean
  /** number of generator calls so far */
  calls: number
  getState(): number[]
  setState(s: number[]): void
  /** last calls, newest last (debug overlay) */
  history: RngCall[]
  /** frame stamp written into history entries */
  frame: number
}

/** Mixin that records calls; rulesets extend this. */
export abstract class RecordingRng implements Rng {
  calls = 0
  history: RngCall[] = []
  frame = 0
  historyLimit = 64

  protected abstract step(): number
  abstract getState(): number[]
  abstract setState(s: number[]): void

  protected record(fn: RngCall['fn'], arg: number, result: number, label?: string) {
    this.calls++
    this.history.push({ frame: this.frame, fn, arg, result, label: label ?? '' })
    if (this.history.length > this.historyLimit) this.history.splice(0, this.history.length - this.historyLimit)
  }

  next(label?: string): number {
    const v = this.step()
    this.record('next', 0, v, label)
    return v
  }
  rand(n: number, label?: string): number {
    const v = this.step()
    const r = Math.floor(((v & 0xffff) * n) / 65536)
    this.record('rand', n, r, label)
    return r
  }
  chance(p: number, label?: string): boolean {
    const v = this.step()
    const r = (v & 0xffff) < p
    this.record('chance', p, r ? 1 : 0, label)
    return r
  }
}
