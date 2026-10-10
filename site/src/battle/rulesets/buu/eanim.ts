// Buu's Fury enemy animation timing as the game runs it (tools/battle/ENEMY_ANIM.md, data/battle/buu_enemy_anim.json).
//
// Like the player (panim.ts), an enemy shows animation `raw` at frame obj+0x11 and its state object (obj+0x14)
// advances the frame once per game tick, before the AI tree runs. Enemy SetAnim(id) (vfunc +0x28, 0x0800ECF0) takes
// the raw from 0x08072670 and the state from the *enemy* table 0x080726B8 (vfunc +0x2C, 0x0800FAB6). Attack / special
// nodes put the actor into their own state objects instead; their frame sequences come from the data file (`seq`),
// recorded tick by tick in the emulator.

import { tickBored, tickIdle, tickLoop, tickOnce, type AnimPos } from './panim'

/** one enemy state rule (buu_enemy_anim.json `sim` block / the SetAnim table) */
export interface EAnimRule {
  rule: 'idle' | 'loop' | 'once' | 'once_stop' | 'hold' | 'keep' | 'bored' | 'reverse' | 'seq' | 'tail_loop'
  shift?: number
  /** once: what the end callback does */
  end?: 'idle' | 'hold' | 'none'
  /** seq: [raw, frame, ticks] run-lengths from the entering tick */
  seq?: [number, number, number][]
  /** enter plays the punch swing (50 + rand(2), volume 64) */
  swing?: boolean
  /** seq: sounds the state object makes itself, at ticks after entering (`rand`: id + rand(n)) */
  sfx?: { at: number; id: number; vol: number; rand?: number }[]
  /** seq: every recorded run ended by the state itself after the same number of ticks (else the AI ends it) */
  fixed?: boolean
  /** the state object ('vtable@object') these rules were recorded for */
  controller?: string
}

/** enemy SetAnim id -> raw (0x08072670 + 1) and state rule (0x080726B8; tools/battle/gen_buu_enemy_anim.py `setanim`) */
export const ENEMY_SETANIM: { raw: number; r: EAnimRule }[] = [
  { raw: 1, r: { rule: 'idle' } },
  { raw: 2, r: { rule: 'loop', shift: 3 } },
  { raw: 3, r: { rule: 'loop', shift: 3 } },
  { raw: 4, r: { rule: 'once', shift: 3, end: 'idle' } },
  { raw: 5, r: { rule: 'once', shift: 2, end: 'idle', swing: true } },
  { raw: 6, r: { rule: 'once', shift: 2, end: 'idle', swing: true } },
  { raw: 7, r: { rule: 'once', shift: 2, end: 'idle', swing: true } },
  { raw: 8, r: { rule: 'hold' } },
  { raw: 9, r: { rule: 'bored' } },
  { raw: 10, r: { rule: 'once', shift: 3, end: 'idle' } },
  { raw: 11, r: { rule: 'once_stop', shift: 3, end: 'none' } },
  { raw: 12, r: { rule: 'once_stop', shift: 3, end: 'none' } },
  { raw: 13, r: { rule: 'once', shift: 4, end: 'idle' } },
  { raw: 14, r: { rule: 'loop', shift: 3 } },
  { raw: 15, r: { rule: 'loop', shift: 3 } },
  { raw: 16, r: { rule: 'loop', shift: 3 } },
  { raw: 18, r: { rule: 'once', shift: 5, end: 'hold' } },
  { raw: 19, r: { rule: 'once', shift: 4, end: 'idle' } },
  { raw: 20, r: { rule: 'once', shift: 4, end: 'idle' } },
  { raw: 21, r: { rule: 'once', shift: 4, end: 'idle' } },
  { raw: 22, r: { rule: 'once', shift: 4, end: 'idle' } },
  { raw: 23, r: { rule: 'once', shift: 4, end: 'idle' } },
  { raw: 24, r: { rule: 'once', shift: 4, end: 'idle' } },
]

/** the shared state objects (IWRAM singletons) by address -> rule; a controller key is 'vtable@state object' */
export const GENERIC_STATES: Record<string, EAnimRule> = {
  '0x03001B0C': { rule: 'idle' },
  '0x03002B44': { rule: 'idle' },
  '0x03001B58': { rule: 'loop', shift: 3 },
  '0x03001B48': { rule: 'loop', shift: 3 },
  '0x03001B10': { rule: 'once', shift: 3, end: 'idle' },
  '0x03001B18': { rule: 'once', shift: 4, end: 'idle' },
  '0x03001B50': { rule: 'once', shift: 2, end: 'idle', swing: true },
  '0x03001B30': { rule: 'once_stop', shift: 3, end: 'none' },
  '0x03001B40': { rule: 'once', shift: 5, end: 'hold' },
  '0x03001B04': { rule: 'hold' },
  '0x03001B08': { rule: 'bored' },
  '0x03002B9C': { rule: 'reverse' },
  '0x03003100': { rule: 'reverse' },
}
/** vtables of the 'keep' states: no tick, and entering does not reset the frame (0x03001B00 hold, 0x03001BA4 none) */
export const KEEP_VTABLES = ['0x0804985C', '0x0804A214']

/** the rule of a SetState(controller) showing `raw`: a shared state object, else the type's recorded rule for that
 *  controller (buu_enemy_anim_sim.json), else null (not modelled) */
export function ruleForController(ctl: string, _raw: number, typeRules?: Record<string, EAnimRule & { controller?: string }>): EAnimRule | null {
  const [vt, st] = ctl.split('@')
  if (KEEP_VTABLES.includes(vt)) return { rule: 'keep' }
  if (GENERIC_STATES[st]) return GENERIC_STATES[st]
  return typeRules?.[ctl] ?? null
}

/** the rule the game uses for `raw` when it is set through SetAnim (lowest id showing that raw) */
export function setAnimRule(raw: number): EAnimRule | null {
  return ENEMY_SETANIM.find((s) => s.raw === raw)?.r ?? null
}

export interface EAnimHost {
  /** frames of animation `raw` in the actor's sprite (0 = the sprite lacks it) */
  nframes(raw: number): number
  rand(n: number, label: string): number
  sfx(id: number, vol: number): void
}

/** the actor's animation fields (obj+0x50 raw, +0x11 frame, +0x12 tick) and its state rule */
export class EnemyAnim {
  pos: AnimPos = { raw: 1, frame: 0, tick: 0 }
  r: EAnimRule = { rule: 'idle' }
  /** a one-shot finished (end callback 'none' / 'hold', or the sequence ran out) */
  done = false
  /** ticks since the state was entered (sequence index) */
  t = 0
  host: EAnimHost
  constructor(host: EAnimHost) {
    this.host = host
  }

  /** SetState with rule `r` showing `raw` (enter: tick = frame = 0; idle: tick = rand(255); punch: swing sound) */
  enter(raw: number, r: EAnimRule) {
    this.pos.raw = raw
    this.r = r
    this.done = false
    this.t = 0
    if (r.rule === 'keep') return
    this.pos.frame = 0
    this.pos.tick = 0
    if (r.rule === 'idle') this.pos.tick = this.host.rand(255, 'enemy idle start tick')
    if (r.swing) this.host.sfx(50 + this.host.rand(2, 'enemy swing sfx'), 64)
    if (r.rule === 'seq' && r.seq?.length) {
      this.pos.raw = r.seq[0][0]
      this.pos.frame = r.seq[0][1]
    }
    this.cues(0)
  }

  private cues(t: number) {
    for (const c of this.r.sfx ?? []) if (c.at === t) this.host.sfx(c.id + (c.rand ? this.host.rand(c.rand, 'enemy state sfx') : 0), c.vol)
  }

  /** one game tick of the state (vtable +4). Returns 'idle' when the end callback asks for SetAnim(0). */
  tick(): 'idle' | null {
    const r = this.r
    const n = this.host.nframes(this.pos.raw)
    this.t++
    switch (r.rule) {
      case 'idle':
        tickIdle(this.pos, n)
        return null
      case 'loop':
        tickLoop(this.pos, n, r.shift ?? 3)
        return null
      case 'bored':
        tickBored(this.pos, n)
        return null
      case 'tail_loop':
        // the vampire grab's state 0x0804C3CC (update 0x08033984): tick + 1, from 0x20 back to 0x10, frame = tick >> 3
        this.pos.tick = (this.pos.tick + 1) & 0xffff
        if (this.pos.tick >= 0x20) this.pos.tick = 0x10
        this.pos.frame = this.pos.tick >> 3
        return null
      case 'reverse': {
        let t = this.pos.tick - 1
        if (t < 0) t = n * 8 - 1
        this.pos.tick = t & 0xffff
        this.pos.frame = (t >> 3) & 0xff
        return null
      }
      case 'hold':
      case 'keep':
        return null
      case 'once':
      case 'once_stop': {
        if (this.done) {
          this.pos.tick = (this.pos.tick + 1) & 0xffff
          return null
        }
        if (tickOnce(this.pos, n, r.shift ?? 3)) {
          if (r.end === 'idle') return 'idle'
          this.done = true
        }
        return null
      }
      case 'seq': {
        const seq = r.seq ?? []
        this.cues(this.t)
        let k = this.t
        for (const [raw, frame, cnt] of seq) {
          if (k < cnt) {
            this.pos.raw = raw
            this.pos.frame = frame
            return null
          }
          k -= cnt
        }
        // past the recorded sequence: keep the last frame (the node ends the state)
        this.done = true
        return seq.length && r.end === 'idle' && r.fixed !== false ? 'idle' : null
      }
    }
    return null
  }
}
