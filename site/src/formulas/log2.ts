// Legacy of Goku II formulas — exact integer math from EntityBase::computeDamage 0x0800F910
// (re/log2/notes.md §4, data/log2/formulas.json). rand(n) = ((x & 0xFFFF) * n) >> 16 -> [0, n-1].
import { shr, u32 } from '../lib/util'
import { addTo, type Dist } from './dist'

export function randCounts(n: number): number[] {
  const c = new Array(n).fill(0)
  for (let x = 0; x < 65536; x++) c[(x * n) >> 16]++
  return c
}
const R26 = randCounts(26)

/** Attack-stat curve f(S) = (((26S − 701)S + 115618)S) >> 8. */
export function fS(S: number): number {
  return shr(((26 * S - 701) * S + 115618) * S, 8)
}

export function critChance(STR: number): number {
  return ((0xa4 * STR) & 0xffff) / 65536
}

export interface Log2Hit {
  /** 0 melee (STR, can crit, defender melee mult) / 1 energy (POW, energy mult) */
  type: 0 | 1
  power: number
  /** hit-message scale, 1024 = 1.0 */
  scale: number
  /** attacker STR (melee) or POW (energy) */
  S: number
  /** defender END */
  DEF: number
  /** defender damage-taken multiplier for this type (128 = 1.0) */
  MULT: number
}

/** One damage evaluation for a given rand(26) value r (0..25) and crit flag. */
export function damage(h: Log2Hit, r: number, crit: boolean): number {
  const power = h.power * (crit ? 2 : 1)
  const b = shr(power * (128 - h.DEF), 7)
  const f = fS(h.S)
  let t = shr(u32((r + 256) * b), 8)
  t = shr(u32(t * f), 18)
  t = shr(h.scale * t, 10)
  return shr(t * h.MULT, 7)
}

export function damageDist(h: Log2Hit): { dist: Dist; pCrit: number; normal: [number, number]; crit: [number, number] } {
  const pCrit = h.type === 0 ? critChance(h.S) : 0
  const d: Dist = new Map()
  for (let r = 0; r < 26; r++) {
    const p = R26[r] / 65536
    addTo(d, damage(h, r, false), p * (1 - pCrit))
    if (pCrit > 0) addTo(d, damage(h, r, true), p * pCrit)
  }
  return {
    dist: d,
    pCrit,
    normal: [damage(h, 0, false), damage(h, 25, false)],
    crit: [damage(h, 0, true), damage(h, 25, true)],
  }
}

/** Kill exp (0x0800E4D2): clamp to [span>>7, span>>1] of the active character's current level. */
export function killExp(classExp: number, level: number, expTable: number[]): number {
  if (classExp === 0) return 0
  if (classExp <= 5) return classExp
  if (level >= 50) return 0
  const span = expTable[level] - expTable[level - 1]
  let E = classExp
  if (span >> 7 > E) E = span >> 7
  else if (span >> 1 < E) E = span >> 1
  return E
}

export function levelOf(exp: number, expTable: number[]): number {
  let L = 1
  for (let i = 0; i < expTable.length; i++) if (exp >= expTable[i]) L = i + 1
  return L
}

export interface Growth {
  STR: [number, number]
  POW: [number, number]
  END: [number, number]
}

export interface StatRange {
  min: number
  avg: number
  max: number
}

/** Stat (8.8 fixed) after levelling 1 -> L one level at a time; min/avg/max of the sum of per-level rolls. */
export function statAt(init8p8: number, g: [number, number], L: number): StatRange {
  const n = L - 1
  const cap = 0x6400
  const lo = Math.min(cap, init8p8 + g[0] * n)
  const hi = Math.min(cap, init8p8 + g[1] * n)
  const avg = Math.min(cap, init8p8 + ((g[0] + g[1]) / 2) * n)
  return { min: lo / 256, avg: avg / 256, max: hi / 256 }
}

/** maxHP after levelling: per level maxHP += maxHP*(rand(0x51F)+0xE14) >> 16 (compounding). */
export function maxHpAt(init: number, L: number): StatRange {
  let lo = init,
    hi = init,
    avg = init
  for (let i = 1; i < L; i++) {
    lo += shr(lo * 0xe14, 16)
    hi += shr(hi * (0xe14 + 0x51e), 16)
    avg += (avg * (0xe14 + 0x51e / 2)) / 65536
  }
  return { min: lo, avg: Math.floor(avg), max: hi }
}

/** maxEP after levelling: per level +rand(3)+2 (2..4). */
export function maxEpAt(init: number, L: number): StatRange {
  const n = L - 1
  return { min: init + 2 * n, avg: init + 3 * n, max: init + 4 * n }
}

/** Effective stat bytes on the entity: integer part of 8.8 + form bonus (or override), as in the form controllers. */
export function entityStat(v8p8: number, bonus = 0, override?: number): number {
  if (override !== undefined) return override
  return Math.floor(v8p8) + bonus
}

/** Regen tick: every 65536/110 ≈ 595.8 frames. */
export const REGEN_PERIOD = 65536 / 110
export function regenActiveEp(maxEP: number) {
  return shr(maxEP * 0x7ae, 16)
}
export function regenBench(max: number) {
  return shr(max * 0xccd, 16)
}
