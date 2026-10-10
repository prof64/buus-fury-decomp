// Behavioural checks of the simulator against measurements made in the real game (emulator):
// movement speeds (notes Part B §2), aggro radii (Part C §4) and boss hit cadence / damage (bosses.json measured,
// bs_measure.py: Lv40 Piccolo STR40/POW55/END53 standing still, boss placed 30 or 100 px away, 900 frames).
import { describe, expect, it } from 'vitest'
import bosses from '../../../data/log2/bosses.json'
import { makeSim } from './harness'
import { BTN } from '../../src/battle/core/input'

/** damage of every hit the player takes; `plain` = only scale-1024 hits (ki blast / melee strike) */
function damages(sim: ReturnType<typeof makeSim>) {
  const out: number[] = []
  const plain: number[] = []
  const p = sim.player
  const orig = p.applyDamageDirect.bind(p)
  p.applyDamageDirect = (w, dmg, crit, msg) => {
    // drain-grab ticks (no message) count as one attack per grab
    if (dmg > 0 && (msg || !p.grabbedBy || out.length === 0 || (out as number[] & { g?: unknown }).g !== p.grabbedBy)) {
      if (!msg) (out as number[] & { g?: unknown }).g = p.grabbedBy
      out.push(dmg)
      if (msg && msg.scale === 0x400) plain.push(dmg)
    }
    orig(w, dmg, crit, msg)
  }
  return { out, plain }
}

describe('player movement', () => {
  it('walks 1.25 px/f (both axes, no diagonal normalisation) and dashes 2.5 px/f', () => {
    const s = makeSim({ classId: 0, dist: 400 })
    const x0 = s.player.x
    s.run(41, () => BTN.RIGHT) // first frame idle -> walk
    expect((s.player.x - x0) / 256).toBe(50)
    const y0 = s.player.y
    const x1 = s.player.x
    s.run(8, () => BTN.RIGHT | BTN.DOWN)
    expect((s.player.y - y0) / 256).toBe(10)
    expect((s.player.x - x1) / 256).toBe(10)
    // double tap -> dash
    s.run(5, () => 0)
    const x2 = s.player.x
    s.run(31, (f) => (f === 0 || f >= 4 ? BTN.LEFT : 0))
    expect(s.player.stateName).toBe('DASH')
    expect((x2 - s.player.x) / 256).toBeGreaterThan(60)
  })
})

describe('enemy AI vs emulator', () => {
  it('snake (class 53, ai 8) notices the player at 72 px, wolf (class 17, ai 74) at 80 px', () => {
    for (const [cls, from, to, r] of [
      [53, 8, 9, 72],
      [17, 74, 75, 80],
    ]) {
      const s = makeSim({ classId: cls, dist: r + 1 })
      s.run(3)
      expect(s.enemies[0].ai.top().state).toBe(from)
      // approach 1 px at a time (en_verify_aggro.py)
      s.player.x += 256
      s.run(2)
      expect(s.enemies[0].ai.top().state).toBe(to)
    }
  })

  const cases: [string, number, number][] = []
  for (const f of bosses.fights)
    for (const p of f.phases)
      for (const [k, m] of Object.entries((p as { measured?: Record<string, { cls: number; start_dist: number; hits: number; damage: number[] }> }).measured ?? {}))
        if (!cases.some((c) => c[0] === k)) cases.push([k, m.cls, m.start_dist])

  const meas = (k: string) => {
    for (const f of bosses.fights) for (const p of f.phases) for (const [kk, m] of Object.entries((p as { measured?: Record<string, { hits: number; damage: number[] }> }).measured ?? {})) if (kk === k) return m
    throw new Error(k)
  }

  for (const [key, cls, dist] of cases) {
    // Newman (wolf row 74) only reaches the player at 100 px through his map's patrol waypoints (no waypoints here)
    if (key === 'newman_74@100') continue
    it(`${key}: damage per hit within the measured range, hit count comparable`, () => {
      const m = meas(key)
      const seeds = [1, 2, 3, 4, 5, 6]
      let hits = 0
      const all: number[] = []
      for (const seed of seeds) {
        const s = makeSim({ classId: cls, dist, seed })
        const dmg = damages(s)
        s.run(900)
        hits += dmg.out.length
        all.push(...dmg.plain)
      }
      const avg = hits / seeds.length
      const lo = Math.min(...m.damage)
      const hi = Math.max(...m.damage)
      // every simulated hit uses the exact formula; allow the RNG spread / crits the 900-frame sample didn't show
      const outside = all.filter((v) => v < lo * 0.85 || v > hi * 2.3)
      expect(outside, `damages ${[...new Set(all)].sort((a, b) => a - b).join(',')} vs measured ${m.damage}`).toEqual([])
      expect(avg, `avg hits ${avg} vs measured ${m.hits}`).toBeGreaterThan(m.hits * 0.4)
      expect(avg, `avg hits ${avg} vs measured ${m.hits}`).toBeLessThan(m.hits * 1.6)
    })
  }
})
