// LoG1 damage formulas (re/log1/notes.md §2-4, data/log1/formulas.json, port of re/log1/model.py).
// Every routine is integer-exact; compares are signed like the ARM code.

import type { Log1Rng } from './rng'

/** ARM division routine 0x0800E568: truncates toward zero. */
export function tdiv(n: number, d: number): number {
  return Math.trunc(n / d)
}

export interface DefenderFlags {
  /** char+0x9A: 1 player, 2 enemy */
  team: number
  /** char+0x0A stun/cooldown timer */
  stun: number
  /** char+0x0B AI activity (2 = dormant) */
  activity: number
  /** god mode 0x03000C18 (invincibility code) */
  godmode?: boolean
}

export interface DamageResult {
  /** 0 = no damage ("clink"), 1 = killed, 2 = damaged */
  result: 0 | 1 | 2
  hp: number
  /** damage after the defense roll (negative = the heal quirk) */
  dmg: number
  /** the defense roll (rand(10000)) or -1 when no roll happened */
  roll: number
}

/**
 * apply_damage 0x08007352 (VERIFIED 113/113, verify_damage.py):
 *   enemies ignore damage while stunned (+0x0A != 0) or dormant (+0x0B == 2, defense != 0);
 *   defense roll r = rand(10000): >9000 -def, >7500 -def/2, >5000 -def/4, >4800 miss, else full.
 *   A negative dmg (enemy melee rand overflow) passes the "dmg >= hp" test as false and is subtracted -> heal.
 */
export function applyDamage(rng: Log1Rng, dmg: number, defense: number, hp: number, f: DefenderFlags): DamageResult {
  if (f.team !== 1) {
    if (f.stun !== 0) return { result: 0, hp, dmg: 0, roll: -1 }
    if (f.activity === 2 && defense !== 0) return { result: 0, hp, dmg: 0, roll: -1 }
  }
  let roll = -1
  if (defense !== 0) {
    roll = rng.randInc(10000, 'defense roll')
    if (roll > 9000) dmg = defense > dmg ? 0 : dmg - defense
    else if (roll > 7500) dmg = defense >> 1 > dmg ? 0 : dmg - (defense >> 1)
    else if (roll > 5000) dmg = defense >> 2 > dmg ? 0 : dmg - (defense >> 2)
    else if (roll > 4800) return { result: 0, hp, dmg: 0, roll }
  }
  const god = !!f.godmode && f.team === 1
  if (dmg >= hp) {
    if (god) return { result: 2, hp, dmg, roll }
    return { result: 1, hp: 0, dmg, roll }
  }
  if (dmg === 0) return { result: 0, hp, dmg, roll }
  if (god) return { result: 2, hp, dmg, roll }
  return { result: 2, hp: (hp - dmg) | 0, dmg, roll }
}

/** Player punch 0x08005676 (VERIFIED 10/10): (maxHP * (384 + rand2(128))) >> 9, unsigned 32-bit product. */
export function punchDamage(maxHp: number, r2: number): number {
  return Math.floor(((maxHp * (0x180 + r2)) >>> 0) / 512)
}

/** Ki blast (player and enemy) 0x0800B596: owner maxHP / 3. */
export function kiBlastDamage(ownerMaxHp: number): number {
  return tdiv(ownerMaxHp, 3)
}

/** Enemy melee 0x0800F7EC: rand(hp/2) is rolled first, then + hp/4 (attacker's CURRENT hp). */
export function enemyMelee(rng: Log1Rng, curHp: number, overflow = true): number {
  const r = rng.randInc(curHp >> 1, 'enemy melee', !overflow)
  return (r + (curHp >> 2)) | 0
}

/** Per-hit cap for enemy attacks on the player 0x08006D70: maxHP/3 + 1 (refreshed on stat refresh only). */
export function damageCap(playerMaxHp: number): number {
  return tdiv(playerMaxHp, 3) + 1
}

/** Ki taken every 8 frames while the player charges (0x0800B50A). */
export function chargeCost(maxKi: number): number {
  return (maxKi + 15) >> 4
}

/** Solar Flare freeze length 0x0800B692 (VERIFIED charge 125/400 -> 153). */
export function solarFlareDuration(charge: number, maxKi: number): number {
  return 60 + tdiv(Math.min(charge, maxKi) * 300, maxKi)
}

/** Probabilities of the defense roll outcomes (rand(10000) is uniform on 10001 values). */
export const DEFENSE_ODDS = { full: 4801 / 10001, miss: 200 / 10001, quarter: 2500 / 10001, half: 1500 / 10001, fullDef: 1000 / 10001 }
