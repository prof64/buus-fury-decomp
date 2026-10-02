// Buu's Fury formulas — exact integer math from Character::ComputeDamage 0x0800E9D8
// (re/buu/notes.md §4, data/buu/formulas.json). Damage is produced in 1/1024 HP.
import { shr } from '../lib/util'
import { addTo, type Dist } from './dist'
import { randCounts } from './log2'

const R26 = randCounts(26)

export interface BuuHit {
  kind: 0 | 1 // 0 melee, 1 energy
  sub: number // attack-type sub id (3 = melee that cannot crit)
  power: number // attack-type power, 1024 = 1.0
  mult: number // per-hit multiplier, 1024 = 1.0
  S: number // attacker STR (melee) / POW (energy)
  D: number // defender END (melee) / END2 (energy)
  resist: number // defender resist byte, 128 = 1.0
  critBonus?: number // attacker obj+0x1BC = equipment crit% * 656
  blockFactor?: number // when the defender blocks from the front: dmg * f >> 16 (0x1000 = 1/16)
}

export function effectiveDef(D: number): number {
  return D >= 200 ? Math.min(Math.trunc(D / 4) + 150, 220) : D
}

/** Attack factor A = S + (S²(S+100) >> 13). */
export function attackFactor(S: number): number {
  return S + shr(S * S * (S + 100), 13)
}

export function critChance(h: BuuHit): number {
  if (h.kind !== 0 || h.sub === 3) return 0
  return Math.min(1, (82 * h.S + (h.critBonus ?? 0)) / 65536)
}

/** Damage in 1/1024 HP for rand(26) = r. */
export function damage1024(h: BuuHit, r: number, crit: boolean): number {
  let power = h.power
  let P: number
  if (h.kind === 0) {
    if (crit) power = power + (power >> 1)
    P = shr(h.S * power * 0xccd, 16)
  } else P = power
  const D = effectiveDef(h.D)
  const X = shr((221 - D) * P * 297, 16)
  const A = attackFactor(h.S)
  const Y = shr(h.mult * X + 1023, 10)
  const raw = shr((256 + r) * Y, 8) * A
  let dmg = shr(h.resist * raw + 127, 7)
  if (h.blockFactor !== undefined && dmg) dmg = shr(dmg * h.blockFactor, 16)
  return dmg
}

/** Distribution of HP lost per hit (fractions kept, as the game carries the 1/1024 remainder). */
export function damageDist(h: BuuHit): { dist: Dist; pCrit: number; normal: [number, number]; crit: [number, number] } {
  const pCrit = critChance(h)
  const d: Dist = new Map()
  for (let r = 0; r < 26; r++) {
    const p = R26[r] / 65536
    addTo(d, damage1024(h, r, false) / 1024, p * (1 - pCrit))
    if (pCrit > 0) addTo(d, damage1024(h, r, true) / 1024, p * pCrit)
  }
  return {
    dist: d,
    pCrit,
    normal: [damage1024(h, 0, false) / 1024, damage1024(h, 25, false) / 1024],
    crit: [damage1024(h, 0, true) / 1024, damage1024(h, 25, true) / 1024],
  }
}

export function xpGain(enemyExp: number, mult256 = 256): number {
  return shr(enemyExp * mult256 + 255, 8)
}

export function walkSpeed(SPD: number): number {
  return 5 * SPD + 102
}
export function dashSpeed(SPD: number): number {
  return walkSpeed(SPD) + 320
}

/** One level-up for a growth row g[8] and the four rand(256) rolls. */
export function levelUpOnce(hp: number, ep: number, g: number[], r: [number, number, number, number]) {
  let maxHP = hp + shr(shr(g[0] * r[0], 8) + g[1], 16)
  maxHP = maxHP + shr(maxHP * (shr(g[2] * r[1], 8) + g[3]), 16)
  let maxEP = ep + shr(g[4] * r[2] + g[5], 16)
  maxEP = maxEP + shr(maxEP * (shr(g[6] * r[3], 8) + g[7]), 16)
  return { maxHP, maxEP }
}

/** maxHP / maxEP from a start level to every later level with all rolls 0 (min), 255 (max) or 128 (typical). */
export function growthCurve(startLevel: number, hp: number, ep: number, g: number[], toLevel = 200) {
  const out: { level: number; hp: [number, number, number]; ep: [number, number, number] }[] = []
  let lo = { maxHP: hp, maxEP: ep },
    mid = { maxHP: hp, maxEP: ep },
    hi = { maxHP: hp, maxEP: ep }
  out.push({ level: startLevel, hp: [hp, hp, hp], ep: [ep, ep, ep] })
  for (let L = startLevel + 1; L <= toLevel; L++) {
    lo = levelUpOnce(lo.maxHP, lo.maxEP, g, [0, 0, 0, 0])
    mid = levelUpOnce(mid.maxHP, mid.maxEP, g, [128, 128, 128, 128])
    hi = levelUpOnce(hi.maxHP, hi.maxEP, g, [255, 255, 255, 255])
    out.push({ level: L, hp: [lo.maxHP, mid.maxHP, hi.maxHP], ep: [lo.maxEP, mid.maxEP, hi.maxEP] })
  }
  return out
}

export function fusion(a: { hp: number; ep: number; STR: number; POW: number; END: number; SPD: number; xp: number }, b: typeof a) {
  return {
    hp: shr((a.hp + b.hp) * 154, 8),
    ep: shr((a.ep + b.ep) * 154, 8),
    STR: Math.trunc((a.STR + b.STR) / 2) + 10,
    POW: Math.trunc((a.POW + b.POW) / 2) + 10,
    END: Math.trunc((a.END + b.END) / 2) + 10,
    SPD: Math.trunc((a.SPD + b.SPD) / 2) + 16,
    xp: Math.trunc((a.xp + b.xp) / 2),
  }
}

/** Loot roll count distribution (script #497), with a luck modifier from equipment. */
export function lootRolls(luck = 0): { rolls: number; p: number }[] {
  const table: [number, number][] = [
    [5, 8],
    [10, 7],
    [20, 6],
    [30, 5],
    [40, 4],
    [55, 3],
    [75, 2],
    [100, 1],
  ]
  const out = new Map<number, number>()
  let prev = 0
  for (const [hi, n] of table) {
    const p = ((hi - prev) / 100) * 0.99
    const k = Math.max(1, Math.min(8, n + luck))
    out.set(k, (out.get(k) ?? 0) + p)
    prev = hi
  }
  return [{ rolls: 0, p: 0.01 }, ...[...out].sort((a, b) => a[0] - b[0]).map(([rolls, p]) => ({ rolls, p }))]
}

/** Coin size distribution of script #500 at a loot level. */
export function coinDist(level: number): { coin: number; p: number }[] {
  const G = (level - 48) * 100
  const c = new Map<number, number>()
  const cnt = randCounts(400)
  for (let r = 0; r < 400; r++) {
    const v = r <= Math.trunc(G / 100) ? 100 : r <= Math.trunc(G / 83) ? 50 : r <= Math.trunc(G / 66) ? 25 : 5
    c.set(v, (c.get(v) ?? 0) + cnt[r] / 65536)
  }
  return [100, 50, 25, 5].map((coin) => ({ coin, p: c.get(coin) ?? 0 }))
}
