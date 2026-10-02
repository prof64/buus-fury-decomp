// Buu's Fury damage: Character::ComputeDamage 0x0800E9D8 (re/buu/notes.md §4, data/buu/formulas.json), ported
// with the same integer steps (all products stay below 2^53, the game computes the resist step in 64 bits).
// Damage is produced in 1/1024 HP; HP loss carries the remainder (`HpAcc`).
//
//   melee:  crit = sub != 3 && chance(82*S + critBonus)   power += power>>1    P = (S*power*0xCCD) >> 16
//   energy: P = power
//   D >= 200 -> D = min(D/4 + 150, 220)
//   X = ((221-D)*P*297) >> 16 ;  A = S + ((S*S*(S+100)) >> 13) ;  Y = (mult*X + 1023) >> 10
//   raw = (((256 + rand(26)) * Y) >> 8) * A ;  dmg = (resist*raw + 127) >> 7
//   blocking && (hitDir^1) == facing && dmg -> dmg = dmg*blockFactor >> 16

import type { Rng } from '../../core/rng'

export interface AttackType {
  /** table index = sub id (0x086B8088 + 8*sub) */
  sub: number
  /** 0 melee (STR vs END), 1 energy (POW vs END2) */
  kind: 0 | 1
  power: number
}

/** attack-type table 0x086B8088 (data/buu/moves.json attack_types) */
export const ATK: AttackType[] = Array.from({ length: 19 }, (_, sub) => ({
  sub,
  kind: (sub === 0 || sub === 2 || sub === 3 ? 0 : 1) as 0 | 1,
  power: sub === 16 ? 0 : 1024,
}))

export const ATK_NAMES: Record<number, string> = {
  0: 'melee', 1: 'energy', 2: 'punch', 3: 'melee (no crit)', 4: 'Energy Blast', 5: 'Kamehameha', 6: 'Electric Kamehameha', 7: 'Masenko',
  8: 'Kamekameha', 9: 'Energy Punch', 10: 'Burning Attack', 11: 'Final Flash', 12: 'Flame Kamehameha', 13: 'Big Bang', 14: 'Banshee',
  15: 'Energy Sword', 16: 'Volleyball', 17: 'Big Bang Kamehameha', 18: 'Dragon Fist',
}

export interface CombatStats {
  STR: number
  POW: number
  END: number
  END2: number
  /** obj+0x1B8 / +0x1B9, 128 = x1 */
  resM: number
  resE: number
  /** obj+0x1BC = crit% * 656 */
  critBonus: number
  /** obj+0x1C0, 0x1000 = 1/16 */
  blockFactor: number
}

export interface DamageResult {
  crit: boolean
  raw: number
  /** final damage in 1/1024 HP */
  dmg: number
}

const shr = (v: number, n: number) => Math.floor(v / 2 ** n)

export function effectiveDef(D: number): number {
  return D >= 200 ? Math.min(Math.trunc(D / 4) + 150, 220) : D
}

export function computeDamageRaw(
  kind: number, sub: number, power: number, mult: number, S: number, critBonus: number, D: number, resist: number,
  blocking: boolean, blockFactor: number, rng: Rng,
): DamageResult {
  let P = power
  let crit = false
  if (kind === 0) {
    if (sub !== 3) {
      crit = rng.chance(0x52 * S + critBonus, 'crit')
      if (crit) P = P + (P >> 1)
    }
    P = shr(S * P * 0xccd, 16)
  }
  const d = effectiveDef(D)
  const X = shr((221 - d) * P * 297, 16)
  const A = S + shr(S * S * (S + 100), 13)
  const Y = shr(mult * X + 1023, 10)
  const r = rng.rand(26, 'dmg spread')
  const raw = shr((256 + r) * Y, 8) * A
  let dmg = shr(resist * raw + 127, 7)
  if (blocking && dmg) dmg = shr(dmg * blockFactor, 16)
  return { crit, raw, dmg }
}

/** attacker/defender form: picks STR/END/resist by kind; `blocking` must already include the frontal test */
export function computeDamage(atk: AttackType, mult: number, attacker: CombatStats, defender: CombatStats, blocking: boolean, rng: Rng): DamageResult {
  const melee = atk.kind === 0
  return computeDamageRaw(
    atk.kind, atk.sub, atk.power, mult,
    melee ? attacker.STR : attacker.POW, attacker.critBonus,
    melee ? defender.END : defender.END2, melee ? defender.resM : defender.resE,
    blocking, defender.blockFactor, rng,
  )
}

/** HP loss with the 1/1024 remainder carried (player 0x08010BF2) */
export class HpAcc {
  acc = 0
  take(dmg1024: number): number {
    this.acc += dmg1024
    const whole = Math.floor(this.acc / 1024)
    this.acc -= whole * 1024
    return whole
  }
}
