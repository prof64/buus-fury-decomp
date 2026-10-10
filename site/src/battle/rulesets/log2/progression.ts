// LoG2 experience / level-up (re/log2/notes.md Part D §4, data/log2/levels.json):
//   level(exp) = 1 + max{i : exp >= EXP[i]}                       (0x08004214)
//   levelUp (0x08003CA0, n = new - old levels):
//     for STR, POW, END (8.8): v = stat + min*n + rand((max-min+1)*n); v &= 0xFFFF; stat = min(v, 0x6400)
//     per level: maxHP += maxHP*(rand(0x51F)+0xE14) >> 16 ; maxEP += rand(3)+2 ; then HP=maxHP, EP=maxEP
//   kill exp (0x0800E4D2): E = class.exp; if E > 5 and L < 50: span = EXP[L]-EXP[L-1]; E = clamp(E, span>>7, span>>1)

import type { Rng } from '../../core/rng'
import { Log2Rng } from './rng'

export interface Record8 {
  hp: number
  maxHp: number
  ep: number
  maxEp: number
  /** 8.8 fixed point like the RAM record */
  STR: number
  POW: number
  END: number
  exp: number
}

export interface Growth {
  /** [min, max] per level in 1/256 stat points (table 0x081D95F0) */
  STR: [number, number]
  POW: [number, number]
  END: [number, number]
}

export type GrowthMode = 'rolled' | 'average' | 'min' | 'max'

export function levelOf(exp: number, table: number[]): number {
  let L = 1
  for (let i = 0; i < table.length; i++) if (exp >= table[i]) L = i + 1
  return L
}

export const STAT_CAP = 0x6400

/** Exact port of 0x08003CA0 for `n` levels gained (one call). */
export function levelUp(rec: Record8, g: Growth, n: number, rng: Rng) {
  for (const k of ['STR', 'POW', 'END'] as const) {
    const [mn, mx] = g[k]
    let v = rec[k] + rng.rand((mx - mn + 1) * n, 'levelup ' + k) + mn * n
    v &= 0xffff
    rec[k] = Math.min(v, STAT_CAP)
  }
  for (let i = 0; i < n; i++) {
    const r = rng.rand(0x51f, 'levelup hp') + 0xe14
    rec.maxHp = (rec.maxHp + ((rec.maxHp * r) >>> 16)) & 0xffff
    rec.maxEp = (rec.maxEp + rng.rand(3, 'levelup ep') + 2) & 0xffff
  }
  rec.hp = rec.maxHp
  rec.ep = rec.maxEp
}

/** Deterministic variants for the setup screen (average/min/max growth instead of the RNG). */
function levelUpFixed(rec: Record8, g: Growth, mode: Exclude<GrowthMode, 'rolled'>) {
  for (const k of ['STR', 'POW', 'END'] as const) {
    const [mn, mx] = g[k]
    const add = mode === 'min' ? mn : mode === 'max' ? mx : (mn + mx) >> 1
    rec[k] = Math.min(rec[k] + add, STAT_CAP)
  }
  const r = mode === 'min' ? 0xe14 : mode === 'max' ? 0xe14 + 0x51e : 0xe14 + 0x28f
  rec.maxHp = rec.maxHp + ((rec.maxHp * r) >>> 16)
  rec.maxEp = rec.maxEp + (mode === 'min' ? 2 : mode === 'max' ? 4 : 3)
  rec.hp = rec.maxHp
  rec.ep = rec.maxEp
}

/**
 * Character record at `level`, grown one level at a time from the new-game record (the usual way levels are
 * gained: a kill awards at most half a level, so each level-up call has n = 1).
 */
export function recordAtLevel(initial: Record8, g: Growth, level: number, expTable: number[], mode: GrowthMode, seed = 0): Record8 {
  const rec = { ...initial }
  const rng = Log2Rng.fromTitleFrames(seed)
  const start = levelOf(rec.exp, expTable)
  for (let L = start; L < level; L++) {
    if (mode === 'rolled') levelUp(rec, g, 1, rng)
    else levelUpFixed(rec, g, mode)
  }
  rec.exp = Math.max(rec.exp, expTable[Math.max(0, level - 1)])
  return rec
}

/** exp for killing an enemy of class exp `E` with the active character at `level` (0x0800E4D2) */
export function killExp(E: number, level: number, expTable: number[]): number {
  if (E === 0) return 0
  if (E > 5) {
    if (level >= 50) return 0
    const span = expTable[level] - expTable[level - 1]
    if (span >> 7 > E) E = span >> 7
    else if (span >> 1 < E) E = span >> 1
  }
  return E
}
