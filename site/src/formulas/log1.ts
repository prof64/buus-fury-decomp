// The Legacy of Goku (LoG1) formulas — exact integer math as in re/log1/notes.md and data/log1/formulas.json.
// RNG output is modelled as a uniform 16-bit value x = (a'*b') & 0xFFFF (main) or (raw>>16) (secondary).
import { i32 } from '../lib/util'
import { addTo, distFromCounts, type Dist } from './dist'

/** Main RNG `rand(n)` (0x08005CA8): asr32(int32(x*(n+1)), 16) -> [0, n], NEGATIVE when x*(n+1) >= 2^31. */
export function randMain(x: number, n: number): number {
  return i32(x * (n + 1)) >> 16
}

const cache = new Map<number, Dist>()
/** Exact distribution of rand(n) over all 65536 RNG outputs (includes the negative-result bug). */
export function randMainDist(n: number): Dist {
  let d = cache.get(n)
  if (!d) {
    const counts = new Map<number, number>()
    for (let x = 0; x < 65536; x++) {
      const v = randMain(x, n)
      counts.set(v, (counts.get(v) ?? 0) + 1)
    }
    d = distFromCounts(counts)
    if (cache.size > 64) cache.clear()
    cache.set(n, d)
  }
  return d
}

/** Defense roll in apply_damage (0x08007352): r = rand(10000). Exact bucket probabilities. */
export const DEFENSE_BUCKETS = (() => {
  let full = 0,
    miss = 0,
    q = 0,
    h = 0,
    f = 0
  for (let x = 0; x < 65536; x++) {
    const r = randMain(x, 10000)
    if (r > 9000) f++
    else if (r > 7500) h++
    else if (r > 5000) q++
    else if (r > 4800) miss++
    else full++
  }
  const t = 65536
  return [
    { key: 'full', label: 'Full damage (r ≤ 4800)', p: full / t },
    { key: 'quarter', label: 'dmg − def/4 (5000 < r ≤ 7500)', p: q / t },
    { key: 'half', label: 'dmg − def/2 (7500 < r ≤ 9000)', p: h / t },
    { key: 'fulldef', label: 'dmg − def (r > 9000)', p: f / t },
    { key: 'miss', label: '"Miss": 0 damage (4800 < r ≤ 5000)', p: miss / t },
  ]
})()

/** apply_damage defense step for one raw damage value. Signed compares (negative dmg passes through). */
export function defenseOutcomes(dmg: number, def: number): { key: string; p: number; dmg: number }[] {
  if (def === 0) return [{ key: 'full', p: 1, dmg }]
  const sub = (d: number) => (d > dmg ? 0 : dmg - d)
  return DEFENSE_BUCKETS.map((b) => ({
    key: b.key,
    p: b.p,
    dmg: b.key === 'full' ? dmg : b.key === 'miss' ? 0 : b.key === 'quarter' ? sub(def >> 2) : b.key === 'half' ? sub(def >> 1) : sub(def),
  }))
}

export function applyDefense(raw: Dist, def: number): Dist {
  const out: Dist = new Map()
  for (const [v, p] of raw) for (const o of defenseOutcomes(v, def)) addTo(out, o.dmg, p * o.p)
  return out
}

/** Player punch (0x08005676): (maxHP * (384 + rand2(128))) >> 9, rand2 uniform in [0,127]. */
export function punchDist(maxHP: number): Dist {
  const d: Dist = new Map()
  for (let r = 0; r < 128; r++) addTo(d, Math.floor((maxHP * (384 + r)) / 512), 1 / 128)
  return d
}

export function kiBlastDamage(maxHP: number): number {
  return Math.trunc(maxHP / 3)
}

/** Ki charged per 8-frame tick while holding B. */
export function chargeCost(maxKi: number): number {
  return (maxKi + 15) >> 4
}

/** Ki spent (= Kamehameha damage) after holding B for `frames` frames with `ki` available. */
export function kiSpent(maxKi: number, ki: number, frames: number): { ticks: number; spent: number } {
  const cost = chargeCost(maxKi)
  const ticks = Math.min(Math.floor(frames / 8), cost > 0 ? Math.floor(ki / cost) : 0)
  return { ticks, spent: ticks * cost }
}

export function kiBlastSize(spent: number): string {
  const c = 10 + spent
  return c > 72 ? 'large 16×16' : c > 20 ? 'medium 8×8' : 'small 6×6'
}

export function solarFlareFrames(charge: number, maxKi: number): number {
  return 60 + Math.trunc((Math.min(charge, maxKi) * 300) / Math.max(1, maxKi))
}

/** Enemy melee raw (0x0800F7EC): cur_hp//4 + rand(cur_hp//2), then capped at player maxHP//3 + 1. */
export function enemyMeleeRaw(curHP: number, playerMaxHP?: number): Dist {
  const base = Math.trunc(curHP / 4)
  const rd = randMainDist(Math.trunc(curHP / 2))
  const cap = playerMaxHP !== undefined ? Math.trunc(playerMaxHP / 3) + 1 : Infinity
  const out: Dist = new Map()
  for (const [v, p] of rd) addTo(out, Math.min(base + v, cap), p)
  return out
}

export function damageCap(playerMaxHP: number): number {
  return Math.trunc(playerMaxHP / 3) + 1
}

export const PLAYER_DEFENSE = 250
export const SPEED_TABLE = [8, 10, 12, 14, 16, 18, 19, 20]

export function wakeRadiusPx(aggroTiles: number): number {
  return Math.sqrt((96 * aggroTiles * aggroTiles) & 0xffff)
}

export interface Log1Level {
  level: number
  exp_required: number
  max_hp: number
  max_ki: number
  max_flight_energy: number
  ki_regen_per_16_frames: number
  ki_blast_damage: number
  melee_damage_min: number
  melee_damage_max: number
  max_damage_taken_per_hit: number
  exp_to_next?: number
}
