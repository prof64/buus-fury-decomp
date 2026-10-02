// Buu's Fury ruleset unit tests against values recorded in the real game:
//   data/battle/buu_vectors.json     (tools/battle/gen_buu_vectors.py): 78 ComputeDamage calls with the RNG state at
//                                     entry (player melee/energy, crits, END >= 200, enemy hits, blocked hits,
//                                     i-frames) and 3 LevelUp calls
//   data/battle/buu_enemy_hits.json  (tools/battle/gen_buu_enemy_hits.py): 3162 enemy -> player damage events of all
//                                     79 regular enemy types (no RNG state: the value must be a formula output)
import { describe, expect, it } from 'vitest'
import vec from '../../../data/battle/buu_vectors.json'
import hits from '../../../data/battle/buu_enemy_hits.json'
import characters from '../../../data/buu/characters.json'
import levels from '../../../data/buu/levels.json'
import moves from '../../../data/buu/moves.json'
import { BuuRng } from '../../src/battle/rulesets/buu/rng'
import { ATK, computeDamageRaw, HpAcc } from '../../src/battle/rulesets/buu/damage'
import { fuse, levelOf, levelUp, recalcStats, recordAtLevel, walkSpeed, xpGain, type BuuRecord } from '../../src/battle/rulesets/buu/progression'
import { buuData } from './buu_harness'

describe("Buu's Fury RNG (0x0803FAAC)", () => {
  it('matches the reference algorithm (re/buu/rng.py)', () => {
    const r = new BuuRng(0x12345678, 0x9abcdef0)
    let s0 = 0x12345678 >>> 0
    let s1 = 0x9abcdef0 >>> 0
    for (let i = 0; i < 20; i++) {
      const a = (((s0 + 0x02cbbc71) >>> 0) ^ s1) >>> 0
      const b = ((((((s1 << 3) | (s1 >>> 29)) >>> 0) + 0x0632d80f) >>> 0) ^ a) >>> 0
      s0 = a
      s1 = b
      expect(r.next()).toBe((a - b) >>> 0)
    }
  })
})

describe("Buu's Fury damage (ComputeDamage 0x0800E9D8)", () => {
  it(`reproduces all ${vec.damage.length} damage events recorded in the emulator (crit, raw and final)`, () => {
    let crits = 0
    let clamp = 0
    let blocked = 0
    for (const v of vec.damage) {
      const blocking = v.blocking && (v.hitDir ^ 1) === v.defFacing
      const res = computeDamageRaw(v.kind, v.sub, v.power, v.mult, v.kind === 0 ? v.S : v.POW, v.critBonus, v.kind === 0 ? v.E : v.E2, v.resist, blocking, v.blockFactor, new BuuRng(v.rng[0], v.rng[1]))
      expect({ crit: res.crit ? 1 : 0, raw: res.raw, dmg: res.dmg }, JSON.stringify(v)).toEqual({ crit: v.crit, raw: v.raw, dmg: v.dmg })
      crits += v.crit
      if ((v.kind === 0 ? v.E : v.E2) >= 200) clamp++
      if (blocking) blocked++
    }
    expect(vec.damage.length).toBeGreaterThanOrEqual(48)
    expect(crits).toBeGreaterThan(0)
    expect(clamp).toBeGreaterThan(0)
    expect(blocked).toBeGreaterThan(0)
  })

  it(`every one of the ${hits.hits.reduce((a, h) => a + (h[10] as number), 0)} recorded enemy hits is a formula output`, () => {
    let n = 0
    for (const h of hits.hits as number[][]) {
      const [, kind, sub, power, mult, S, POW, E, E2, actual] = h
      const outs = new Set<number>()
      for (const resist of [128, 0])
        for (let r = 0; r < 26; r++)
          for (const crit of [false, true]) {
            // scripted RNG: the crit roll and rand(26) = r
            const rng = { calls: 0, history: [], frame: 0, next: () => 0, getState: () => [], setState: () => undefined, rand: () => r, chance: () => crit }
            outs.add(computeDamageRaw(kind, sub, power, mult, kind === 0 ? S : POW, 0, kind === 0 ? E : E2, resist, false, 0x1000, rng).dmg)
          }
      expect(outs.has(actual), `row ${JSON.stringify(h)}`).toBe(true)
      n++
    }
    expect(n).toBeGreaterThan(500)
  })

  it('attack-type table 0x086B8088 matches moves.json', () => {
    for (const a of moves.attack_types) expect([ATK[a.sub].kind, ATK[a.sub].power]).toEqual([a.kind, a.power])
  })

  it('HP loss carries the 1/1024 remainder', () => {
    const acc = new HpAcc()
    expect([acc.take(1500), acc.take(1500), acc.take(100)]).toEqual([1, 1, 1])
  })
})

describe("Buu's Fury progression", () => {
  const table = levels.levels.map((l) => l.total_xp)
  it('level(xp) = 50 + max{i : xp >= T[i]} (XP 2200 -> 60, 4099 -> 62)', () => {
    expect(levelOf(0, table)).toBe(50)
    expect(levelOf(2200, table)).toBe(60)
    expect(levelOf(4099, table)).toBe(62)
    expect(levelOf(1e12, table)).toBe(200)
  })
  it(`LevelUp 0x08013678 reproduces the ${vec.levelup.length} emulator level-ups (exact RNG order)`, () => {
    for (const v of vec.levelup) {
      const c = characters.characters[v.char]
      const rec: BuuRecord = { level: v.old_level, xp: v.before.xp, hp: v.before.hp, maxHp: v.before.maxHp, ep: v.before.ep, maxEp: v.before.maxEp, STR: 0, POW: 0, END: 0, SPD: 0, points: v.before.points }
      levelUp(rec, c.growth.raw_u32, v.old_level, v.new_level, new BuuRng(v.rng[0], v.rng[1]))
      expect({ hp: rec.hp, maxHp: rec.maxHp, ep: rec.ep, maxEp: rec.maxEp, points: rec.points }).toEqual({ hp: v.after.hp, maxHp: v.after.maxHp, ep: v.after.ep, maxEp: v.after.maxEp, points: v.after.points })
    }
  })
  it('a grown record stays inside the min/max growth envelope', () => {
    const d = buuData()
    for (const c of d.chars.slice(0, 5)) {
      const lo = recordAtLevel(c, 150, d.xpTable, 'min')
      const hi = recordAtLevel(c, 150, d.xpTable, 'max')
      const r = recordAtLevel(c, 150, d.xpTable, 'rolled', 7)
      expect(r.maxHp).toBeGreaterThanOrEqual(lo.maxHp)
      expect(r.maxHp).toBeLessThanOrEqual(hi.maxHp)
      expect(r.points).toBe((150 - c.level) * 3)
    }
  })
  it('walk = 5*SPD+102 (verified at SPD 10/50/100/150/255), XP gain rounding (Majin Fighter +1225)', () => {
    expect([10, 50, 100, 150, 255].map(walkSpeed)).toEqual([152, 352, 602, 852, 1377])
    expect(xpGain(1225, 256)).toBe(1225)
    expect(xpGain(1225, 256 + 26)).toBe(1350)
  })
  it('fusion 0x0801375E', () => {
    const a: BuuRecord = { level: 75, xp: 100, hp: 292, maxHp: 292, ep: 213, maxEp: 213, STR: 46, POW: 48, END: 31, SPD: 55, points: 0 }
    const b: BuuRecord = { level: 60, xp: 50, hp: 136, maxHp: 136, ep: 90, maxEp: 90, STR: 30, POW: 25, END: 25, SPD: 55, points: 0 }
    const f = fuse(a, b)
    expect([f.maxHp, f.maxEp, f.STR, f.POW, f.END, f.SPD, f.xp]).toEqual([((292 + 136) * 154) >> 8, ((213 + 90) * 154) >> 8, 48, 46, 38, 71, 75])
  })
  it('RecalcStats: equipment modifiers, effect scripts, Holy Water, clamping', () => {
    const d = buuData()
    const eq = (id: number) => d.equipment[id]
    const rec: BuuRecord = { level: 150, xp: 0, hp: 100, maxHp: 1000, ep: 10, maxEp: 10, STR: 50, POW: 40, END: 30, SPD: 20, points: 0 }
    const s = recalcStats(rec, [eq(40), eq(73), eq(107), eq(153)], false)
    // Saiyan Armor + Gloves + Boots: +50 STR/POW/END/END2; HP 100 > 5% of 1000: no Crisis Ring bonus
    expect([s.STR, s.POW, s.END, s.END2, s.SPD]).toEqual([50 + 20 + 50 + 5, 40 + 50 + 2, 30 + 24 + 50 + 10, 30 + 24 + 50 + 10, 20 + 24])
    rec.hp = 50
    expect(recalcStats(rec, [eq(153)], false).SPD).toBe(70)
    expect(recalcStats(rec, [eq(16)], false).blockFactor).toBe(0x800)
    expect(recalcStats(rec, [eq(34)], false).epCostFactor).toBe(8)
    expect(recalcStats(rec, [eq(110)], false).critPct).toBe(20)
    expect(recalcStats(rec, [eq(42)], false).hpRegen256).toBe(21)
    expect(recalcStats(rec, [], true).STR).toBe(75)
    expect(recalcStats({ ...rec, SPD: 3 }, [eq(72)], false).SPD).toBe(0)
  })
})
