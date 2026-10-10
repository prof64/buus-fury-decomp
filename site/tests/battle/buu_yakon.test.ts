// Yakon (0x91) energy absorb vs the emulator (re/buu/YAKON.md). Fixture: data/battle/buu_yakon_absorb.json from
// re/buu/yakon_record.py: every hit on Yakon with his top AI state, attack, the game's final damage and his HP before
// it. The test replays each hit through the sim's on-hit tree (damage taken from the recording, so the damage
// roll's RNG is out of the picture) and checks his HP after every hit and whether he died.
import { describe, expect, it } from 'vitest'
import rec from '../../../data/battle/buu_yakon_absorb.json'
import { makeBuuSim } from './buu_harness'
import { ATK } from '../../src/battle/rulesets/buu/damage'
import type { HitMsg } from '../../src/battle/rulesets/buu/actor'

interface Hit {
  f: number
  state: number
  kind: number
  sub: number
  mult: number
  hp: number
  dmg: number
}
interface Run {
  name: string
  hp_start: number
  hits: Hit[]
  hp_end: number
  dead: boolean
}

describe('Yakon energy absorb (emulator recordings)', () => {
  for (const run of rec.runs as Run[]) {
    it(`${run.name}: HP after every hit, dead=${run.dead}`, () => {
      const s = makeBuuSim({ enemy: 't145', arena: 'yakon', seed: 1 })
      const e = s.enemies[0]
      e.w = s.w
      e.hp1024 = run.hp_start
      const got: number[] = []
      const want: number[] = []
      run.hits.forEach((h, i) => {
        if (e.defeated) return
        if (e.ai.top()?.state !== h.state) e.ai.reset(h.state)
        const msg: HitMsg = { team: 1, rect: e.bodyBox(), dir: 3, mult: h.mult, atk: ATK[h.sub], attacker: s.player, dmg: h.dmg }
        expect(msg.atk.kind).toBe(h.kind)
        e.ai.onHit(msg)
        got.push(e.hp1024)
        want.push(i + 1 < run.hits.length ? run.hits[i + 1].hp : run.hp_end)
      })
      expect(got).toEqual(want)
      expect(e.defeated).toBe(run.dead)
    })
  }

  it('state 0x31D (wander): every energy hit heals, a 32..64 HP one (bit 15 set) hurts instead', () => {
    const s = makeBuuSim({ enemy: 't145', arena: 'yakon', seed: 1 })
    const e = s.enemies[0]
    e.w = s.w
    e.ai.reset(0x31d)
    const hit = (dmg: number) => e.ai.onHit({ team: 1, rect: e.bodyBox(), dir: 3, mult: 1024, atk: ATK[4], attacker: s.player, dmg })
    e.hp1024 = 1000 << 10
    hit(20 << 10)
    expect(e.hp1024).toBe(1020 << 10)
    hit(40 << 10) // 0xA000 -> -0x6000: -24 HP
    expect(e.hp1024).toBe(996 << 10)
    hit(70 << 10) // 70 mod 64 = 6: +6 HP
    expect(e.hp1024).toBe(1002 << 10)
    e.hp1024 = 5 << 10
    hit(50 << 10) // -14 HP: alive below zero
    expect(e.hp1024).toBe(-9 << 10)
    expect(e.defeated).toBe(false)
  })
})
