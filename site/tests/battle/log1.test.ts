// LoG1 ruleset tests against values recorded in the real game (data/battle/log1_vectors.json,
// tools/battle/gen_log1_vectors.py) and the verified Python model (re/log1/model.py).
import { describe, expect, it } from 'vitest'
import vec from '../../../data/battle/log1_vectors.json'
import levels from '../../../data/log1/levels.json'
import { Log1Rng, Log1Rng2 } from '../../src/battle/rulesets/log1/rng'
import { applyDamage, chargeCost, damageCap, kiBlastDamage, punchDamage, solarFlareDuration } from '../../src/battle/rulesets/log1/damage'
import { levelFromExp } from '../../src/battle/rulesets/log1/data'
import { keysAt, log1Data, simFromInit } from './log1_harness'

/* eslint-disable @typescript-eslint/no-explicit-any */
const V = vec as any

describe('LoG1 RNG (0x08005CA8 / 0x0800A6F4)', () => {
  it('rand(n) is inclusive and overflows like muls/asrs for n >= 32768', () => {
    // model.py: prod = (a'*b') & 0xFFFF; v = int32(prod*(n+1)) >> 16
    const r = new Log1Rng(0, 0)
    const vals = Array.from({ length: 2000 }, () => r.randInc(10))
    expect(Math.min(...vals)).toBe(0)
    expect(Math.max(...vals)).toBe(10)
    const big = new Log1Rng(0, 0)
    const neg = Array.from({ length: 2000 }, () => big.randInc(40000)).filter((v) => v < 0)
    expect(neg.length).toBeGreaterThan(0)
  })
  it('secondary rand2(n) is in [0, n-1]', () => {
    const r = new Log1Rng2(1, 2)
    const vals = Array.from({ length: 2000 }, () => r.rand2(128))
    expect(Math.min(...vals)).toBe(0)
    expect(Math.max(...vals)).toBe(127)
  })
})

describe('LoG1 apply_damage 0x08007352 (defense roll)', () => {
  it(`reproduces all ${V.damage.length} calls recorded in the emulator`, () => {
    expect(V.damage.length).toBe(113)
    let ok = 0
    for (const v of V.damage) {
      const r = new Log1Rng(v.rng[0], v.rng[1])
      const res = applyDamage(r, v.dmg, v.defense, v.hp, { team: v.team, stun: v.stun, activity: v.act, godmode: !!v.god })
      expect([res.result, res.hp]).toEqual([v.result, v.hpAfter])
      ok++
    }
    expect(ok).toBe(113)
  })
})

describe('LoG1 punch 0x08005676', () => {
  it(`predicts the ${V.punch.length} punches recorded in the emulator`, () => {
    expect(V.punch.length).toBeGreaterThanOrEqual(10)
    for (const v of V.punch) {
      const r2 = new Log1Rng2(v.rng2[0], v.rng2[1])
      expect(punchDamage(v.maxHp, r2.rand2(0x80))).toBe(v.dmg)
    }
  })
  it('range 75%..99.8% of max HP (level table column)', () => {
    for (const l of levels.levels) {
      expect(punchDamage(l.max_hp, 0)).toBe(l.melee_damage_min)
      expect(punchDamage(l.max_hp, 127)).toBe(l.melee_damage_max)
      expect(kiBlastDamage(l.max_hp)).toBe(l.ki_blast_damage)
      expect(damageCap(l.max_hp)).toBe(l.max_damage_taken_per_hit)
      expect(l.max_ki >> 6).toBe(l.ki_regen_per_16_frames)
    }
  })
})

describe('LoG1 formulas and level table', () => {
  it('level from exp (0x08005C32) over the 25-row table', () => {
    const d = log1Data()
    expect(d.levels.length).toBe(25)
    expect(levelFromExp(d.levels, 0)).toBe(0)
    expect(levelFromExp(d.levels, 349)).toBe(0)
    expect(levelFromExp(d.levels, 350)).toBe(1)
    expect(levelFromExp(d.levels, 3600000)).toBe(24)
    expect(levelFromExp(d.levels, 99999999)).toBe(24)
    expect(d.levels.map((l) => [l.exp, l.maxHp, l.maxKi, l.maxFlight])).toEqual(levels.levels.map((l) => [l.exp_required, l.max_hp, l.max_ki, l.max_flight_energy]))
  })
  it('Solar Flare duration and charge cost (VERIFIED values)', () => {
    expect(solarFlareDuration(125, 400)).toBe(153)
    expect(solarFlareDuration(0, 400)).toBe(60)
    expect(solarFlareDuration(9999, 400)).toBe(360)
    expect(chargeCost(400)).toBe(25)
    expect(chargeCost(100)).toBe(7)
  })
})

/** replay a recorded trace; returns the list of mismatching frames (column-wise) */
export function replay(name: string, maxFrames = Infinity) {
  const t = V.traces.find((x: any) => x.name === name)
  const sim = simFromInit(t.init)
  const bad: string[] = []
  const n = Math.min(t.frames, maxFrames)
  const hits: number[][] = []
  let cur = 0
  const orig = sim.g.hit.bind(sim.g)
  const idx = (c: any) => (c === sim.g.player ? 0 : 1 + sim.enemies.findIndex((e) => e.char === c))
  // hit() 0x08007416 entry: attacker, defender, flag, raw damage (before the cap)
  sim.g.hit = (att, def, flag, raw) => {
    hits.push([cur, idx(att), idx(def), flag, raw])
    orig(att, def, flag, raw)
  }
  for (let f = 1; f <= n; f++) {
    cur = f
    sim.step(keysAt(t.keys, f))
    const row = t.rows[f - 1]
    const got = [sim.player.sx, sim.player.sy, sim.g.player.hp, sim.g.player.ki, sim.player.st, sim.g.flight]
    for (const e of sim.enemies) {
      const c = e.char
      got.push(c.x, c.y, c.state, c.stun, c.act, c.hp, c.ki, c.alive ? 1 : 0)
    }
    const diff = got.map((v, i) => (v !== row[i] ? `${t.columns[i]}${i >= 6 ? '#' + Math.floor((i - 6) / 8) : ''}=${v} (game ${row[i]})` : '')).filter(Boolean)
    if (diff.length) bad.push(`f${f}: ${diff.join(', ')}`)
  }
  // punch damage uses the secondary RNG, which the ROM also advances for map animations: values are checked by the
  // punch vectors above, here only the punch itself (frame, attacker, defender) must match
  const norm = (h: number[]) => (h[1] === 0 && h[3] === 0 ? [h[0], h[1], h[2], h[3], -1] : h)
  return { bad, hits: hits.map(norm), expectedHits: t.hits.filter((h: number[]) => h[0] <= n).map(norm) }
}

describe('LoG1 frame-by-frame replays of emulator recordings', () => {
  // wolf_chase: Goku dies at frame 1088; the game-over screen takes over 0x80 frames after the Death state
  const until: Record<string, number> = { wolf_chase: 1224 }
  for (const name of ['player_moves', 'flight', 'wolf_chase', 'henchman', 'raditz', 'kame_solar', 'hench_ki', 'hench_blast', 'wolf_pack', 'squirrels', 'ptero']) {
    it(`${name}: same positions, states, timers, HP/Ki every frame`, () => {
      const r = replay(name, until[name])
      expect(r.bad.slice(0, 5)).toEqual([])
      expect(r.hits).toEqual(r.expectedHits)
    })
  }
})
