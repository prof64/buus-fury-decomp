// Player skills vs the emulator measurement runs (data/buu/attacks_detail.json skills[].measured: Goku form 2,
// maxEP 999, a Majin Fighter pinned 40 / 100 px in front, B held 3 / 60 / 150 frames). The skill state machines
// are fitted to these runs, so this checks first-hit frames, multipliers / attack types and hit counts.
import { describe, expect, it } from 'vitest'
import attacks from '../../../data/buu/attacks_detail.json'
import { fighter, makeBuuSim } from './buu_harness'
import { BTN } from '../../src/battle/core/input'
import type { HitMsg } from '../../src/battle/rulesets/buu/actor'

interface Trial {
  hold: number
  dist: number
  hits: number
  first_hit: number | null
  mults: number[]
  subs: number[]
}

function run(skill: number, level: number, hold: number, dist: number) {
  const s = makeBuuSim({ enemy: 't68', arena: 'forest', fighter: fighter({ options: { ...fighter().options, skillLevel: String(level) } }) })
  const p = s.player
  p.rec.ep = p.rec.maxEp = 999
  p.skills = [skill]
  p.slot = 0
  const e = s.enemies[0]
  e.hp1024 = e.maxHp1024 = 1e12
  e.ai.tick = () => undefined
  const b = s.w.arena.bounds
  p.x = ((b.x0 + 60) << 8)
  p.y = (((b.y0 + b.y1) >> 1) << 8)
  p.facing = 3
  const pin = () => {
    e.x = p.x + (dist << 8)
    e.y = p.y
    e.ai.stack = e.ai.stack.slice(0, 1)
  }
  const hits: { f: number; mult: number; sub: number }[] = []
  // the game computes the damage of every accepted hit before the on-hit tree runs (0x0800FC2C): count those
  const orig = e.onMessage.bind(e)
  let frame = 0
  e.onMessage = (w, msg: HitMsg) => {
    const r = orig(w, msg)
    if (r && msg.dmg != null) hits.push({ f: frame, mult: msg.mult, sub: msg.atk.sub })
    return r
  }
  for (frame = 1; frame <= 330; frame++) {
    pin()
    s.run(1, () => (frame <= hold ? BTN.B : 0))
  }
  return hits
}

const SKILLS = (attacks.skills as { id: number; name: string; measured?: { level: number; trials: Trial[] }[] }[]).filter((s) => s.measured)
// skills whose behaviour is not measurable this way (no hits recorded / teleport / ghosts / transform)
const SKIP = new Set([12, 13, 14, 18, 19, 20, 21, 22])

describe('player skills vs emulator measurements', () => {
  for (const sk of SKILLS) {
    if (SKIP.has(sk.id)) continue
    for (const m of sk.measured!) {
      for (const t of m.trials) {
        if (t.hits === 0 && t.hold > 3) continue
        it(`${sk.name} Lv${m.level + 1} hold ${t.hold} at ${t.dist} px: ${t.hits ? `first hit f${t.first_hit}, ${t.hits} hits, mult ${t.mults.join('/')}` : 'no hit'}`, () => {
          const hits = run(sk.id, m.level, t.hold, t.dist)
          if (!t.hits) {
            expect(hits.length).toBe(0)
            return
          }
          expect(hits.length).toBeGreaterThan(0)
          const tol = Math.max(4, Math.round(t.first_hit! * 0.12))
          expect(Math.abs(hits[0].f - t.first_hit!), `first hit f${hits[0].f}`).toBeLessThanOrEqual(tol)
          // multipliers / attack sub as recorded (Big Bang's multiplier comes from the charged EP: +-1)
          for (const h of hits) {
            if (sk.id === 2) expect(Math.min(...t.mults.map((v) => Math.abs(v - h.mult)))).toBeLessThanOrEqual(2)
            else if (!(sk.id === 3 && h.mult === 1024)) expect(t.mults).toContain(h.mult)
            expect(t.subs).toContain(h.sub)
          }
          expect(hits.length / t.hits).toBeGreaterThan(0.6)
          expect(hits.length / t.hits).toBeLessThan(sk.id === 2 ? 1.8 : 1.5)
        })
      }
    }
  }
})
