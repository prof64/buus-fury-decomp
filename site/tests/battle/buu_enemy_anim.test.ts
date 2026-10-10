// Enemy / boss animation timing and animation-layer sounds against the emulator (tools/battle/ENEMY_ANIM.md).
//
// data/battle/buu_enemy_anim_traces.json (tools/battle/gen_buu_enemy_anim_traces.py) holds, per logic tick, the raw
// animation and frame the game showed for a force-spawned enemy, the animation calls its code made (SetAnim /
// SetAnimRaw / SetState / SetAnimHold) and the sounds the animation layer played. The test drives the simulator's
// model (eanim.ts: the SetAnim table, the shared state rules, the type's recorded controller rules from
// buu_enemy_anim_sim.json) with the same calls and compares every tick.
import { describe, expect, it } from 'vitest'
import traces from '../../../data/battle/buu_enemy_anim_traces.json'
import simRules from '../../../data/battle/buu_enemy_anim_sim.json'
import { ENEMY_SETANIM, EnemyAnim, ruleForController, type EAnimRule } from '../../src/battle/rulesets/buu/eanim'
import { setAudioSink } from '../../src/battle/audio/bus'
import { makeBuuSim } from './buu_harness'

interface Trace {
  name: string
  type: number
  label: string
  init: [number, number, number, string]
  rows: [number, number, number][]
  defs: [string, number][]
  cmds: [number, string, string, ...(string | number)[]][]
  rand: [number, string, number | null][]
  sfx: [number, number][]
  /** ticks the actor missed (0) or got twice (2) in the game's world loop */
  skips: [number, number | string, number?][]
  nframes: Record<string, Record<string, number>>
  canon: Record<string, Record<string, number>>
}

const TR = (traces as unknown as { traces: Trace[] }).traces
const RULES = (simRules as unknown as { types: Record<string, Record<string, EAnimRule & { controller?: string }>> }).types

function expand<T>(rle: [T, number][]): T[] {
  const out: T[] = []
  for (const [v, n] of rle) for (let i = 0; i < n; i++) out.push(v)
  return out
}

/** replay a trace; -> first mismatch (or null), ticks compared, ticks skipped (unmodelled controller) */
function replay(t: Trace) {
  const rows: [number, number][] = []
  for (const [raw, frame, n] of t.rows) for (let i = 0; i < n; i++) rows.push([raw, frame])
  const defs = expand(t.defs.map(([d, n]) => [d, n] as [string, number]))
  const typeRules = RULES['0x' + t.type.toString(16).toUpperCase().padStart(2, '0')]
  let tick = 0
  const rq: (number | null)[] = []
  const sfx: [number, number][] = []
  const ea = new EnemyAnim({
    nframes: (raw) => t.nframes[defs[tick]]?.[String(raw)] ?? 0,
    rand: (n, label) => {
      const v = rq.shift()
      if (v === undefined) throw new Error(`${t.name} tick ${tick}: unexpected rand(${n}) '${label}'`)
      return v ?? 0
    },
    sfx: (id) => sfx.push([tick, id]),
  })
  const [raw0, frame0, tick0, ctl0] = t.init
  ea.pos = { raw: raw0, frame: frame0, tick: tick0 }
  ea.r = ruleForController(ctl0, raw0, typeRules) ?? { rule: 'keep' }
  let known = !!ruleForController(ctl0, raw0, typeRules)
  const cmdsAt = new Map<number, Trace['cmds']>()
  for (const c of t.cmds) cmdsAt.set(c[0], [...(cmdsAt.get(c[0]) ?? []), c])
  const skipAt = new Map(t.skips.filter((x) => x[1] !== 'reenter').map(([k, v]) => [k, v as number]))
  const reenterAt = new Map(t.skips.filter((x) => x[1] === 'reenter').map(([k, , v]) => [k, v as number]))
  const randAt = new Map<number, [number, string, number | null][]>()
  for (const r of t.rand) randAt.set(r[0], [...(randAt.get(r[0]) ?? []), r])
  let compared = 0
  let skipped = 0
  const apply = (c: Trace['cmds'][number]) => {
    const kind = c[2]
    if (kind === 'anim') {
      const s = ENEMY_SETANIM[c[3] as number]
      ea.enter(s.raw, s.r)
      known = true
    } else if (kind === 'raw') {
      ea.pos.raw = c[3] as number
    } else if (kind === 'state') {
      const r = ruleForController(c[3] as string, ea.pos.raw, typeRules)
      known = !!r
      ea.enter(ea.pos.raw, r ?? { rule: 'keep' })
    } else if (kind === 'hold') {
      const s = ENEMY_SETANIM[c[3] as number]
      ea.enter(s.raw, { rule: 'keep' })
      ea.pos.frame = c[4] as number
      known = true
    }
  }
  for (tick = 1; tick < rows.length; tick++) {
    // the values the game's rand() gave the animation layer this tick, in call order
    for (const r of randAt.get(tick) ?? []) rq.push(r[2])
    const cs = cmdsAt.get(tick) ?? []
    for (const c of cs) if (c[1] === 'pre') apply(c)
    const endCmd = cs.some((c) => c[1] === 'end')
    const sk = skipAt.get(tick)
    let res = sk === 0 ? null : ea.tick()
    if (sk === 2 && !res) res = ea.tick()
    // a one-shot's end callback / a state ending itself: SetAnim(0) (idle; its start tick is the recorded rand value).
    // A state whose recorded runs differ in length (`fixed` false) is ended by the game when it decides (`end`).
    if (res === 'idle' || endCmd) {
      if (res !== 'idle' && ea.r.rule === 'seq' && ea.r.fixed !== false) {
        return { bad: `${t.name} (${t.label}) tick ${tick}: the game ended the fixed-length state, the model did not`, compared, skipped, sfx }
      }
      ea.enter(1, ENEMY_SETANIM[0].r)
      known = true
    }
    for (const c of cs) if (c[1] === 'post') apply(c)
    const re = reenterAt.get(tick)
    if (re != null) {
      rq.push(re)
      ea.enter(ea.pos.raw, ea.r)
    }
    rq.length = 0
    const [eraw, eframe] = rows[tick]
    if (!known) {
      skipped++
      continue
    }
    // a graphics swap under a running animation: the struct is from the old def (the recorder finds no slot: raw -1)
    if (eraw === -1) {
      skipped++
      continue
    }
    const canon = t.canon[defs[tick]]?.[String(ea.pos.raw)] ?? ea.pos.raw
    compared++
    if (canon !== eraw || ea.pos.frame !== eframe) {
      return { bad: `${t.name} (${t.label}) tick ${tick}: emulator raw ${eraw} frame ${eframe}, model raw ${canon} frame ${ea.pos.frame} (rule ${ea.r.rule})`, compared, skipped, sfx }
    }
  }
  return { bad: null, compared, skipped, sfx }
}

describe('enemy animation model vs emulator recordings', () => {
  let total = 0
  let skippedAll = 0
  for (const t of TR) {
    it(`${t.name} ${t.label}`, () => {
      const r = replay(t)
      total += r.compared
      skippedAll += r.skipped
      expect(r.bad).toBeNull()
      // nearly every tick is covered by a modelled rule
      expect(r.compared).toBeGreaterThan(0.9 * (r.compared + r.skipped))
      // the animation layer's own sounds (punch swing) at the same ticks with the same ids
      const want = t.sfx.map(([k, id]) => `${k}:${id}`)
      const got = r.sfx.filter(([k]) => k > 0).map(([k, id]) => `${k}:${id}`)
      for (const w of want) expect(got).toContain(w)
    })
  }
  it('covers the representative set', () => {
    const types = new Set(TR.map((t) => t.type))
    // regular archetypes + Majin Buu, Super Buu, Kid Buu, Janemba, Super Janemba, Broly, Dabura, Pikkon, Yakon, Spopovich
    for (const x of [0x44, 0x2c, 0x03, 0x1c, 0x73, 0x3c, 0x33, 0x7a, 0x0a, 0x12, 0x5b, 0x91, 0x6d]) expect(types.has(x)).toBe(true)
    if (process.env.EANIM_SUMMARY) console.log(`EANIM ${TR.length} traces, ${total} ticks compared, ${skippedAll} not modelled`)
    expect(total).toBeGreaterThan(50000)
  })
})

describe('enemy animation in the running simulator', () => {
  it('Majin Fighter punches: raw 5 frames 0..3 for 4 ticks each (frame 0 three after the entering tick), swing 50/51 on entry, hit 48/49 when it lands, then idle', () => {
    const sim = makeBuuSim({ enemy: 't68', seed: 7 })
    const e = sim.enemies[0]
    const p = sim.player
    // the fighter right next to the player
    e.x = p.x + (20 << 8)
    e.y = p.y
    const log: string[] = []
    const shown: [number, number, number][] = []
    setAudioSink((type, data) => {
      if (type === 'sfx') log.push(`${sim.w.frame}:${data!.id}`)
    })
    try {
      for (let f = 0; f < 900; f++) {
        p.rec.hp = p.rec.maxHp
        sim.run(1)
        shown.push([sim.w.frame, e.anim?.raw ?? -1, e.frameIdx ?? -1])
      }
    } finally {
      setAudioSink(null)
    }
    const starts = shown.filter((s, i) => s[1] === 5 && (i === 0 || shown[i - 1][1] !== 5))
    expect(starts.length).toBeGreaterThan(2)
    for (const st of starts) {
      const i = shown.indexOf(st)
      const seq = shown.slice(i, i + 17).map((s) => (s[1] === 5 ? s[2] : -1))
      // entering tick shows frame 0 (tick 0); frame k from tick 4k
      expect(seq.slice(0, 16)).toEqual([0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3])
      expect(seq[16]).toBe(-1)
      expect(log.some((l) => l === `${st[0]}:50` || l === `${st[0]}:51`)).toBe(true)
    }
    expect(log.some((l) => /:(48|49)$/.test(l))).toBe(true)
    // idle shows the 2-frame blink: frame 1 at most 15 ticks of every 256
    const idle = shown.filter((s) => s[1] === 1)
    expect(idle.filter((s) => s[2] === 1).length).toBeLessThan(idle.length / 8)
  })
})
