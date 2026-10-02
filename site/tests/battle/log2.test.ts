// LoG2 ruleset unit tests: RNG, damage formula, stat curve, level-up, exp - against values recorded from the real
// game (data/battle/log2_vectors.json, tools/battle/gen_vectors.py) and the verified Python reference
// (data/battle/log2_formula_vectors.json, re/log2/formula.py).
import { describe, expect, it } from 'vitest'
import formulas from '../../../data/log2/formulas.json'
import moves from '../../../data/log2/moves.json'
import characters from '../../../data/log2/characters.json'
import emu from '../../../data/battle/log2_vectors.json'
import ref from '../../../data/battle/log2_formula_vectors.json'
import { Log2Rng } from '../../src/battle/rulesets/log2/rng'
import { ATTACKS, computeDamageRaw, statCurve } from '../../src/battle/rulesets/log2/damage'
import { killExp, levelOf, levelUp, recordAtLevel, type Growth } from '../../src/battle/rulesets/log2/progression'

describe('LoG2 RNG (0x080213FC)', () => {
  for (const v of ref.rng) {
    it(`sequence from state ${v.state.map((x: number) => x.toString(16)).join(',')}`, () => {
      const r = new Log2Rng(v.state[0], v.state[1])
      expect(Array.from({ length: 8 }, () => r.next())).toEqual(v.next)
    })
  }
  it('rand(n) / chance(p) use the low 16 bits', () => {
    const a = new Log2Rng(5, 9)
    const b = new Log2Rng(5, 9)
    const raw = b.next() & 0xffff
    expect(a.rand(26)).toBe(Math.floor((raw * 26) / 65536))
    const c = new Log2Rng(5, 9)
    expect(c.chance(raw + 1)).toBe(true)
  })
})

describe('LoG2 damage formula (0x0800F910)', () => {
  it(`reproduces ${emu.damage.length} hits recorded in the emulator`, () => {
    for (const v of emu.damage) {
      const r = new Log2Rng(v.rng[0], v.rng[1])
      const res = computeDamageRaw(v.type, v.power, v.S, v.DEF, v.MULT, v.scale, r)
      expect([res.raw, res.dmg, res.crit ? 1 : 0]).toEqual([v.raw, v.dmg, v.crit])
    }
  })
  it(`matches the Python reference on ${ref.damage.length} randomized cases (incl. crits)`, () => {
    let crits = 0
    for (const v of ref.damage) {
      const r = new Log2Rng(v.rng[0], v.rng[1])
      const res = computeDamageRaw(v.type, v.power, v.S, v.DEF, v.MULT, v.scale, r)
      expect([res.raw, res.dmg, res.crit ? 1 : 0]).toEqual([v.raw, v.dmg, v.crit])
      crits += v.crit
    }
    expect(crits).toBeGreaterThan(10)
  })
  it('stat curve f(S) equals the table in formulas.json for S = 0..100', () => {
    const tbl = formulas.stat_curve.table_S_0_to_100 as number[]
    tbl.forEach((f, S) => expect(statCurve(S)).toBe(f))
  })
  it('attack table matches moves.json (0x086DB2CC)', () => {
    for (const a of moves.attack_table) {
      expect(ATTACKS[a.id]).toEqual({ id: a.id, type: a.type, power: a.power })
    }
  })
  it('snake (STR 7) vs Lv40 Piccolo (END 53) does 7 per hit (notes Part C §2)', () => {
    const seen = new Set<number>()
    for (let s = 0; s < 200; s++) seen.add(computeDamageRaw(0, 1024, 7, 53, 128, 1024, new Log2Rng(s * 7919, s)).dmg)
    expect(seen.has(7)).toBe(true)
    expect(Math.max(...seen)).toBeLessThanOrEqual(15) // crit doubles power
  })
})

describe('LoG2 progression', () => {
  const tbl = characters.exp_table as number[]
  it('level(exp) uses the exp table (0x08004214)', () => {
    expect(levelOf(0, tbl)).toBe(1)
    expect(levelOf(40, tbl)).toBe(1)
    expect(levelOf(tbl[39], tbl)).toBe(40)
    expect(levelOf(tbl[49] + 5, tbl)).toBe(50)
  })
  it('kill exp clamp: Lv40 + 13-exp snake = 4747 (verified in game)', () => {
    expect(killExp(13, 40, tbl)).toBe(4747)
    expect(killExp(5, 40, tbl)).toBe(5)
    expect(killExp(1e9, 40, tbl)).toBe((tbl[40] - tbl[39]) >> 1)
    expect(killExp(100, 50, tbl)).toBe(0)
  })
  it('level-up reproduces the emulator (0x08003CA0, Piccolo 40->41)', () => {
    const v = emu.levelup[0]
    const c = characters.characters[v.char]
    const g = c.growth_per_level
    const growth: Growth = { STR: [g.STR.min_8p8, g.STR.max_8p8], POW: [g.POW.min_8p8, g.POW.max_8p8], END: [g.END.min_8p8, g.END.max_8p8] }
    const b = v.before
    const rec = { hp: b.hp, maxHp: b.max_hp, ep: b.ep, maxEp: b.max_ep, STR: b.STR, POW: b.POW, END: b.END, exp: 0 }
    levelUp(rec, growth, v.new_level - v.old_level, new Log2Rng(v.rng[0], v.rng[1]))
    const a = v.after
    expect(rec).toEqual({ hp: a.hp, maxHp: a.max_hp, ep: a.ep, maxEp: a.max_ep, STR: a.STR, POW: a.POW, END: a.END, exp: 0 })
  })
  it('average growth from the new-game record stays inside the per-level min/max envelope', () => {
    const c = characters.characters[4]
    const i = c.initial
    const g = c.growth_per_level
    const growth: Growth = { STR: [g.STR.min_8p8, g.STR.max_8p8], POW: [g.POW.min_8p8, g.POW.max_8p8], END: [g.END.min_8p8, g.END.max_8p8] }
    const init = { hp: i.hp, maxHp: i.max_hp, ep: i.ep, maxEp: i.max_ep, STR: i.STR_8p8, POW: i.POW_8p8, END: i.END_8p8, exp: i.exp }
    const lo = recordAtLevel(init, growth, 40, tbl, 'min')
    const hi = recordAtLevel(init, growth, 40, tbl, 'max')
    const rolled = recordAtLevel(init, growth, 40, tbl, 'rolled', 3)
    for (const k of ['STR', 'POW', 'END'] as const) {
      expect(rolled[k]).toBeGreaterThanOrEqual(lo[k])
      expect(rolled[k]).toBeLessThanOrEqual(hi[k])
    }
    expect(rolled.maxEp).toBeGreaterThanOrEqual(lo.maxEp)
    expect(rolled.maxEp).toBeLessThanOrEqual(hi.maxEp)
  })
})
