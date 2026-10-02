// LoG2 damage: EntityBase::computeDamage 0x0800F910 (re/log2/notes.md Part A §4, data/log2/formulas.json).
// Ported instruction by instruction with 32-bit wrap-around (`muls` keeps the low 32 bits, `lsrs` is unsigned,
// `asrs` signed), so even overflowing Big Bang / Spirit Bomb hits match the cartridge.
//
//   crit   = melee ? chance((0xA4*S) & 0xFFFF) : 0          power *= 1 + crit
//   b      = (power * (128 - DEF)) >> 7                      (asrs)
//   f(S)   = (((26*S - 701)*S + 115618)*S) >> 8              (asrs)
//   r      = rand(26) + 256
//   t      = ((r*b) >>> 8) * f >>> 18                        (lsrs, lsrs)
//   t      = (scale*t) >> 10                                 (asrs)  -> msg+0x14 (raw)
//   dmg    = (t*MULT) >> 7                                   (asrs)  -> msg+0x18

import type { Rng } from '../../core/rng'

export interface AttackDesc {
  /** attack table id 0..15 (0x086DB2CC) */
  id: number
  /** 0 = melee (STR, can crit), 1 = energy (POW) */
  type: 0 | 1
  power: number
}

export interface CombatStats {
  STR: number
  POW: number
  END: number
  /** damage-taken multipliers /128 (entity+0x163 melee, +0x164 energy) */
  meleeMult: number
  energyMult: number
}

export interface DamageResult {
  crit: boolean
  /** before the defender multiplier (msg+0x14) */
  raw: number
  /** final damage (msg+0x18) */
  dmg: number
}

export function statCurve(S: number): number {
  return Math.imul(Math.imul(Math.imul(26, S) - 701, S) + 115618, S) >> 8
}

/** Raw integer port. All inputs are the integer bytes the game uses. */
export function computeDamageRaw(type: number, power: number, S: number, DEF: number, MULT: number, scale: number, rng: Rng): DamageResult {
  let crit = 0
  if (type === 0) {
    crit = rng.chance(Math.imul(0xa4, S) & 0xffff, 'crit') ? 1 : 0
    power = Math.imul(power, crit + 1)
  }
  const b = Math.imul(power, 0x80 - DEF) >> 7
  const f = statCurve(S)
  const r = rng.rand(0x1a, 'dmg spread') + 256
  let t = Math.imul(r, b) >>> 8
  t = Math.imul(t, f) >>> 18
  t = Math.imul(scale, t) >> 10
  const dmg = Math.imul(t, MULT) >> 7
  return { crit: !!crit, raw: t, dmg }
}

export function computeDamage(attack: AttackDesc, attacker: CombatStats, defender: CombatStats, scale: number, rng: Rng): DamageResult {
  const melee = attack.type === 0
  return computeDamageRaw(
    attack.type,
    attack.power,
    melee ? attacker.STR : attacker.POW,
    defender.END,
    melee ? defender.meleeMult : defender.energyMult,
    scale,
    rng,
  )
}

/** Attack table 0x086DB2CC (data/log2/moves.json attack_table). */
export const ATTACKS: AttackDesc[] = [
  { id: 0, type: 0, power: 1024 },
  { id: 1, type: 1, power: 1024 },
  { id: 2, type: 0, power: 1024 },
  { id: 3, type: 1, power: 1024 },
  { id: 4, type: 0, power: 1536 },
  { id: 5, type: 0, power: 1536 },
  { id: 6, type: 1, power: 1280 },
  { id: 7, type: 1, power: 2048 },
  { id: 8, type: 1, power: 49152 },
  { id: 9, type: 1, power: 64 },
  { id: 10, type: 1, power: 64 },
  { id: 11, type: 1, power: 102 },
  { id: 12, type: 0, power: 1024 },
  { id: 13, type: 1, power: 3072 },
  { id: 14, type: 1, power: 40960 },
  { id: 15, type: 1, power: 0 },
]
