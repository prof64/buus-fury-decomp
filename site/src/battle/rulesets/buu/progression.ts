// Buu's Fury character progression (re/buu/notes.md §3, data/buu/formulas.json):
//   level(xp) = 50 + max{i : xp >= T[i]}                                   (0x080135D2)
//   LevelUp 0x08013678, per level: r1..r4 = rand(256) x4
//     maxHP += ((g0*r1>>8) + g1) >> 16 ; maxHP += maxHP*((g2*r2>>8) + g3) >> 16
//     maxEP += (g4*r3 + g5) >> 16       ; maxEP += maxEP*((g6*r4>>8) + g7) >> 16
//     HP = maxHP, EP = maxEP, stat points += 3
//   RecalcStats 0x08013C66: base + equipment s8 mods (+ effect scripts) + Holy Water, clamped at 0
//   Fusion 0x0801375E: HP/EP = (A+B)*154>>8, STR/POW/END = (A+B)/2 + 10, SPD = (A+B)/2 + 16

import type { Rng } from '../../core/rng'
import { BuuRng } from './rng'
import type { BuuChar, EquipDef } from './data'

const shr = (v: number, n: number) => Math.floor(v / 2 ** n)

export interface BuuRecord {
  level: number
  xp: number
  hp: number
  maxHp: number
  ep: number
  maxEp: number
  STR: number
  POW: number
  END: number
  SPD: number
  points: number
}

export type GrowthMode = 'rolled' | 'average' | 'min' | 'max'

export function levelOf(xp: number, table: number[]): number {
  let L = 50
  for (let i = 0; i < table.length; i++) if (xp >= table[i]) L = 50 + i
  return L
}

/** one level of 0x08013678 with the four rolls given */
export function levelUpRolls(rec: BuuRecord, g: number[], r: [number, number, number, number]) {
  rec.maxHp = rec.maxHp + shr(shr(g[0] * r[0], 8) + g[1], 16)
  rec.maxHp = rec.maxHp + shr(rec.maxHp * (shr(g[2] * r[1], 8) + g[3]), 16)
  rec.maxEp = rec.maxEp + shr(g[4] * r[2] + g[5], 16)
  rec.maxEp = rec.maxEp + shr(rec.maxEp * (shr(g[6] * r[3], 8) + g[7]), 16)
  rec.hp = rec.maxHp
  rec.ep = rec.maxEp
  rec.points += 3
}

/** LevelUp(record, old, new) with the game RNG (exact draw order) */
export function levelUp(rec: BuuRecord, g: number[], oldL: number, newL: number, rng: Rng) {
  for (let L = oldL; L < newL; L++) {
    const a = rng.rand(256, 'levelup hp')
    const b = rng.rand(256, 'levelup hp%')
    const c = rng.rand(256, 'levelup ep')
    const d = rng.rand(256, 'levelup ep%')
    levelUpRolls(rec, g, [a, b, c, d])
  }
  rec.level = newL
}

export function newGameRecord(c: BuuChar): BuuRecord {
  return { level: c.level, xp: c.xp, hp: c.hp, maxHp: c.hp, ep: c.ep, maxEp: c.ep, STR: c.STR, POW: c.POW, END: c.END, SPD: c.SPD, points: 0 }
}

/** record grown from the new-game record to `level` (one level-up call per level) */
export function recordAtLevel(c: BuuChar, level: number, xpTable: number[], mode: GrowthMode, seed = 0): BuuRecord {
  const rec = newGameRecord(c)
  const rng = BuuRng.fromSeed(seed)
  const target = Math.max(c.level, Math.min(200, level))
  for (let L = c.level; L < target; L++) {
    if (mode === 'rolled') levelUp(rec, c.growth, L, L + 1, rng)
    else {
      const v = mode === 'min' ? 0 : mode === 'max' ? 255 : 128
      levelUpRolls(rec, c.growth, [v, v, v, v])
    }
  }
  rec.level = target
  rec.xp = Math.max(rec.xp, xpTable[target - 50] ?? rec.xp)
  return rec
}

export interface FusionParts {
  a: BuuRecord
  b: BuuRecord
}

/** 0x0801375E */
export function fuse(a: BuuRecord, b: BuuRecord): BuuRecord {
  const half = (x: number, y: number) => Math.trunc((x + y) / 2)
  return {
    level: Math.max(a.level, b.level),
    xp: half(a.xp, b.xp),
    hp: shr((a.hp + b.hp) * 154, 8),
    maxHp: shr((a.maxHp + b.maxHp) * 154, 8),
    ep: shr((a.ep + b.ep) * 154, 8),
    maxEp: shr((a.maxEp + b.maxEp) * 154, 8),
    STR: half(a.STR, b.STR) + 10,
    POW: half(a.POW, b.POW) + 10,
    END: half(a.END, b.END) + 10,
    SPD: half(a.SPD, b.SPD) + 16,
    points: 0,
  }
}

/** effective record stats and factors (record +0x4C..+0x67) */
export interface EffStats {
  STR: number
  POW: number
  END: number
  END2: number
  SPD: number
  /** /16 */
  epCostFactor: number
  epBlockFactor: number
  /** /256 */
  xpMult: number
  thorns: number
  /** /65536 */
  blockFactor: number
  /** SS cooldown rate (0x4000) */
  ssCooldownRate: number
  hpRegen256: number
  critPct: number
  meleeEp: number
  meleeHpInstead: boolean
}

export function recalcStats(rec: BuuRecord, equip: (EquipDef | undefined)[], holyWater: boolean): EffStats {
  const s: EffStats = {
    STR: rec.STR, POW: rec.POW, END: rec.END, END2: rec.END, SPD: rec.SPD,
    epCostFactor: 16, epBlockFactor: 16, xpMult: 256, thorns: 0, blockFactor: 0x1000, ssCooldownRate: 0x4000, hpRegen256: 0, critPct: 0, meleeEp: 1, meleeHpInstead: false,
  }
  const ids = new Set(equip.filter(Boolean).map((e) => e!.id))
  for (const e of equip) {
    if (!e) continue
    s.STR += e.STR
    s.POW += e.POW
    s.END += e.END
    s.END2 += e.END2
    s.SPD += e.SPD
    s.xpMult += e.xp
    s.critPct += e.critPct
    if (e.blockHalf) s.blockFactor >>= 1
    if (e.energyCostHalf) s.epCostFactor >>= 1
    if (e.energyBlockHalf) s.epBlockFactor >>= 1
    if (e.ssCooldown78) s.ssCooldownRate = (s.ssCooldownRate * 7) >> 3
    s.thorns += e.thorns
    s.hpRegen256 += e.hpRegen256
    s.meleeEp = e.meleeEp === 2 ? 2 : s.meleeEp + (e.meleeEp === 1 ? 1 : 0)
    if (e.meleeHpInstead) s.meleeHpInstead = true
    // Saiyan Armor script: Gloves (73) + Boots (107) also equipped -> +50 STR/END/END2/POW
    if (e.id === 40 && ids.has(73) && ids.has(107)) {
      s.STR += 50
      s.END += 50
      s.END2 += 50
      s.POW += 50
    }
    // Crisis Ring script: HP*20 <= maxHP -> +50 STR/POW/END/END2/SPD (flag 1: recomputed when HP changes)
    if (e.id === 153 && rec.hp * 20 <= rec.maxHp) {
      s.STR += 50
      s.POW += 50
      s.END += 50
      s.END2 += 50
      s.SPD += 50
    }
  }
  if (holyWater) {
    s.STR += 25
    s.POW += 25
    s.END += 25
    s.END2 += 25
    s.SPD += 25
  }
  for (const k of ['STR', 'POW', 'END', 'END2', 'SPD'] as const) s[k] = Math.max(0, s[k])
  return s
}

export function walkSpeed(SPD: number) {
  return 5 * SPD + 102
}
export function xpGain(enemyExp: number, mult256: number) {
  return shr(enemyExp * mult256 + 255, 8)
}
