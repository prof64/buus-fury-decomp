// Robustness: every Buu's Fury enemy type, every preset and every playable character's skills run without errors.
import { describe, expect, it } from 'vitest'
import { buuData, fighter, makeBuuSim } from './buu_harness'
import { buuRuleset } from '../../src/battle/rulesets/buu'
import { BTN } from '../../src/battle/core/input'

describe("Buu's Fury smoke", () => {
  it('runs all 150 enemy types for 900 frames next to a standing Goku', () => {
    const d = buuData()
    const errors: string[] = []
    for (const t of d.enemies) {
      try {
        const s = makeBuuSim({ enemy: 't' + t.type, arena: 'crater', fighter: fighter({ level: 120 }) })
        s.player.rec.hp = s.player.rec.maxHp = 99999
        s.run(900)
      } catch (e) {
        errors.push(`0x${t.type.toString(16)} ${t.name}: ${(e as Error).stack?.split('\n').slice(0, 3).join(' | ')}`)
      }
    }
    expect(errors).toEqual([])
  })
  it('runs every preset for 3000 frames with a fighting player', () => {
    buuData()
    const errors: string[] = []
    for (const p of buuRuleset.presets()) {
      try {
        const s = makeBuuSim({ enemy: '', phases: p.phases, arena: p.arena, fighter: fighter({ level: 150, fighter: 'vegito', form: 39 }) })
        s.player.rec.hp = s.player.rec.maxHp = 99999
        s.run(3000, (f) => (f % 60 < 30 ? BTN.RIGHT : 0) | (f % 8 < 2 ? BTN.A : 0))
      } catch (e) {
        errors.push(`${p.id}: ${(e as Error).stack?.split('\n').slice(0, 3).join(' | ')}`)
      }
    }
    expect(errors).toEqual([])
  })
  it('every fighter can use every skill of its record', () => {
    buuData()
    const errors: string[] = []
    for (const f of buuRuleset.fighters()) {
      for (const lv of ['0', '2']) {
        try {
          const s = makeBuuSim({ enemy: 't68', arena: 'forest', fighter: fighter({ fighter: f.id, form: f.defaultForm, level: 150, options: { ...fighter().options, skillLevel: lv } }) })
          s.player.rec.ep = s.player.rec.maxEp = 9999
          const n = s.player.skills.length
          for (let k = 0; k < n; k++) {
            s.run(40)
            s.run(2, () => BTN.L)
            s.run(160, (fr) => (fr < 120 ? BTN.B : 0))
          }
          s.run(300, (fr) => (fr % 4 < 2 ? BTN.B : 0))
        } catch (e) {
          errors.push(`${f.id}: ${(e as Error).stack?.split('\n').slice(0, 3).join(' | ')}`)
        }
      }
    }
    expect(errors).toEqual([])
  })
})
