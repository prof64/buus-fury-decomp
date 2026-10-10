// Deterministic high-level actions. Every macro is a generator that yields one held-button mask per frame, so a macro
// is exactly equivalent to the button presses it produced (they go into the input log / replay like any press).
import { BTN } from '../../../site/src/battle/core/input'
import type { Enemy } from '../../../site/src/battle/rulesets/buu/enemy'
import type { Mask, Session, StepDelta } from './session'
import { canAct, DIRS, findEnemy, footCenter, meleeFacings, PUNCH_RECT, skillInfo } from './observe'
import { buuData } from './load'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Any = any
export type Policy = Generator<Mask, string | void, void>
export type StopOn = 'hurt' | 'kill' | 'hit' | 'phase'

export const DIR_BTN = [BTN.DOWN, BTN.UP, BTN.LEFT, BTN.RIGHT]

export interface RunSummary {
  frames: number
  stopped: string
  dealt: number
  hp_lost: number
  hits_dealt: number
  hits_taken: number
  kills: number
  ep_change: number
}

/** run a policy for at most `max` frames; stop early on the battle's end or a `stopOn` condition */
export function runPolicy(s: Session, pol: Policy, max: number, stopOn: StopOn[] = []): RunSummary {
  const ep0 = s.player.ep
  const sum: RunSummary = { frames: 0, stopped: 'max_frames', dealt: 0, hp_lost: 0, hits_dealt: 0, hits_taken: 0, kills: 0, ep_change: 0 }
  const phase0 = phaseCount(s)
  for (let i = 0; i < max; i++) {
    if (s.outcome() !== 'running') {
      sum.stopped = 'battle_' + s.outcome()
      break
    }
    const r = pol.next()
    if (r.done) {
      sum.stopped = r.value ?? 'done'
      break
    }
    const d: StepDelta = s.step(r.value)
    sum.frames++
    sum.dealt += d.dealt
    sum.hp_lost += d.hpLost
    sum.hits_dealt += d.hitsDealt
    sum.hits_taken += d.hitsTaken
    sum.kills += d.kills
    if (stopOn.includes('hurt') && d.hpLost > 0) {
      sum.stopped = 'hurt'
      break
    }
    if (stopOn.includes('kill') && d.kills > 0) {
      sum.stopped = 'kill'
      break
    }
    if (stopOn.includes('hit') && d.hitsDealt > 0) {
      sum.stopped = 'hit'
      break
    }
    if (stopOn.includes('phase') && phaseCount(s) !== phase0) {
      sum.stopped = 'phase'
      break
    }
  }
  if (sum.stopped === 'max_frames' && s.outcome() !== 'running') sum.stopped = 'battle_' + s.outcome()
  sum.dealt = Math.round(sum.dealt)
  sum.ep_change = s.player.ep - ep0
  return sum
}
function phaseCount(s: Session) {
  return s.enemies().filter((e) => e.primary).length
}

// ---- primitives -----------------------------------------------------------------------------------------------

export function* hold(mask: Mask, frames: number): Policy {
  for (let i = 0; i < frames; i++) yield mask
}

export function* sequence(steps: { mask: Mask; frames: number }[]): Policy {
  for (const st of steps) yield* hold(st.mask, st.frames)
}

// ---- path finding on the walk grid ----------------------------------------------------------------------------

const LAT = 4

/** BFS over a 4 px lattice of footprint positions (top-left) the player fits at; returns waypoints (top-left px) */
export function planPath(s: Session, tx: number, ty: number): { path: [number, number][]; reached: boolean } {
  const p = s.player
  const w = s.w
  const b = w.arena.bounds
  const cols = Math.ceil((b.x1 - b.x0) / LAT) + 1
  const rows = Math.ceil((b.y1 - b.y0) / LAT) + 1
  const key = (c: number, r: number) => r * cols + c
  const fitsCache = new Int8Array(cols * rows)
  const fits = (c: number, r: number) => {
    if (c < 0 || r < 0 || c >= cols || r >= rows) return false
    const k = key(c, r)
    if (!fitsCache[k]) fitsCache[k] = p.fits(w, b.x0 + c * LAT, b.y0 + r * LAT) ? 1 : 2
    return fitsCache[k] === 1
  }
  const toC = (x: number) => Math.round((x - b.x0) / LAT)
  const toR = (y: number) => Math.round((y - b.y0) / LAT)
  let sc = toC(p.px)
  let sr = toR(p.py)
  if (!fits(sc, sr)) {
    // snap to a fitting neighbour
    outer: for (let d = 1; d < 4; d++)
      for (let dr = -d; dr <= d; dr++)
        for (let dc = -d; dc <= d; dc++)
          if (fits(sc + dc, sr + dr)) {
            sc += dc
            sr += dr
            break outer
          }
  }
  const gc = toC(tx)
  const gr = toR(ty)
  const prev = new Int32Array(cols * rows).fill(-1)
  const start = key(sc, sr)
  prev[start] = start
  const q = [start]
  let best = start
  let bestD = Math.hypot(sc - gc, sr - gr)
  for (let qi = 0; qi < q.length && q.length < 80000; qi++) {
    const k = q[qi]
    const c = k % cols
    const r = (k / cols) | 0
    const d = Math.hypot(c - gc, r - gr)
    if (d < bestD) {
      bestD = d
      best = k
      if (d === 0) break
    }
    for (const [dc, dr] of [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [1, -1], [-1, 1], [-1, -1]]) {
      const nc = c + dc
      const nr = r + dr
      if (!fits(nc, nr)) continue
      if (dc && dr && (!fits(c + dc, r) || !fits(c, r + dr))) continue
      const nk = key(nc, nr)
      if (prev[nk] !== -1) continue
      prev[nk] = k
      q.push(nk)
    }
  }
  const nodes: [number, number][] = []
  for (let k = best; ; k = prev[k]) {
    nodes.push([b.x0 + (k % cols) * LAT, b.y0 + ((k / cols) | 0) * LAT])
    if (k === start) break
  }
  nodes.reverse()
  // keep only corners: drop nodes on a straight run
  const path: [number, number][] = []
  for (let i = 1; i < nodes.length; i++) {
    const a = nodes[i - 1]
    const c = nodes[i]
    const n = nodes[i + 1]
    if (!n || Math.sign(c[0] - a[0]) !== Math.sign(n[0] - c[0]) || Math.sign(c[1] - a[1]) !== Math.sign(n[1] - c[1])) path.push(c)
  }
  const reached = bestD <= 1
  if (reached && path.length) path[path.length - 1] = [tx, ty]
  return { path, reached }
}

/** d-pad mask that moves the footprint's top-left toward (tx, ty) this frame */
export function steer(s: Session, tx: number, ty: number, dead = 0.6): Mask {
  const p = s.player
  const dx = tx - p.x / 256
  const dy = ty - p.y / 256
  let m = 0
  if (dx > dead) m |= BTN.RIGHT
  else if (dx < -dead) m |= BTN.LEFT
  if (dy > dead) m |= BTN.DOWN
  else if (dy < -dead) m |= BTN.UP
  return m
}

/** walk to a footprint top-left (tx, ty), re-planning when stuck */
export function* walkTo(s: Session, tx: number, ty: number, tol = 2): Policy {
  let plan = planPath(s, tx, ty)
  let wp = 0
  let lastPos = [s.player.x, s.player.y]
  let still = 0
  for (;;) {
    const p = s.player
    if (Math.abs(p.px - tx) <= tol && Math.abs(p.py - ty) <= tol) return 'arrived'
    if (!canAct(s)) {
      yield 0
      continue
    }
    while (wp < plan.path.length - 1 && Math.abs(p.px - plan.path[wp][0]) <= 1 && Math.abs(p.py - plan.path[wp][1]) <= 1) wp++
    const target = plan.path[wp] ?? [tx, ty]
    if (wp >= plan.path.length - 1 && !plan.reached && Math.abs(p.px - target[0]) <= 1 && Math.abs(p.py - target[1]) <= 1) return 'unreachable (stopped at the closest point)'
    yield steer(s, target[0], target[1])
    if (p.x === lastPos[0] && p.y === lastPos[1]) {
      if (++still > 8) {
        plan = planPath(s, tx, ty)
        wp = 0
        still = 0
        if (!plan.path.length) return 'blocked'
      }
    } else still = 0
    lastPos = [p.x, p.y]
  }
}

// ---- melee ----------------------------------------------------------------------------------------------------

/** footprint top-left positions from which a punch with facing f overlaps the enemy body, nearest first */
export function attackSpots(s: Session, e: Enemy): { facing: number; x: number; y: number; d: number }[] {
  const p = s.player
  const b = e.bodyBox()
  const out: { facing: number; x: number; y: number; d: number }[] = []
  for (let f = 0; f < 4; f++) {
    const r = PUNCH_RECT[f]
    const xlo = b.x0 - r[2] + 1
    const xhi = b.x1 - r[0] - 1
    const ylo = b.y0 - r[3] + 1
    const yhi = b.y1 - r[1] - 1
    if (xlo > xhi || ylo > yhi) continue
    const clamp = (v: number, lo: number, hi: number) => Math.max(lo + 2, Math.min(hi - 2, v))
    let x: number
    let y: number
    // stand at the outer edge of the range along the facing axis, and as close to the current line as possible across
    if (f === 3) [x, y] = [xlo + 3, clamp(p.py, ylo, yhi)]
    else if (f === 2) [x, y] = [xhi - 3, clamp(p.py, ylo, yhi)]
    else if (f === 0) [x, y] = [clamp(p.px, xlo, xhi), ylo + 3]
    else [x, y] = [clamp(p.px, xlo, xhi), yhi - 3]
    if (!p.fits(s.w, x, y)) continue
    // horizontal punches are preferred (wider overlap, the way enemies usually line up)
    out.push({ facing: f, x, y, d: Math.hypot(x - p.px, y - p.py) + (f < 2 ? 24 : 0) })
  }
  return out.sort((a, b2) => a.d - b2.d)
}

export function* attack(s: Session, e: Enemy, opts: { hits?: number; approachOnly?: boolean } = {}): Policy {
  const hits0 = s.w.stats.hitsDealt
  let last = 0
  let spot: { x: number; y: number } | null = null
  let plan: ReturnType<typeof planPath> | null = null
  let planAt = -100
  let wp = 0
  const emit = (m: Mask) => {
    last = m
    return m
  }
  for (;;) {
    if (e.removed || e.defeated) return 'target_defeated'
    if (opts.hits && s.w.stats.hitsDealt - hits0 >= opts.hits) return 'hits_done'
    const p = s.player
    if (!canAct(s)) {
      yield emit(0)
      continue
    }
    const mf = meleeFacings(s, e)
    if (mf.includes(p.facing) && e.hittable) {
      if (opts.approachOnly) return 'in_range'
      // a new press is needed for every punch
      yield emit(last & BTN.A ? 0 : BTN.A)
      continue
    }
    if (mf.length && e.hittable) {
      yield emit(DIR_BTN[mf[0]])
      continue
    }
    const spots = attackSpots(s, e)
    const sp = spots[0]
    if (!sp) {
      // no reachable spot (enemy over a wall / in the air): close the distance
      const [ex, ey] = footCenter(e)
      yield emit(steer(s, ex - (p.footW >> 1), ey - (p.footH >> 1)))
      continue
    }
    if (!spot || Math.hypot(sp.x - spot.x, sp.y - spot.y) > 6 || s.w.frame - planAt > 20 || !plan) {
      spot = { x: sp.x, y: sp.y }
      plan = planPath(s, sp.x, sp.y)
      planAt = s.w.frame
      wp = 0
    }
    while (wp < plan.path.length - 1 && Math.abs(p.px - plan.path[wp][0]) <= 1 && Math.abs(p.py - plan.path[wp][1]) <= 1) wp++
    const t = plan.path[wp] ?? [sp.x, sp.y]
    const m = steer(s, t[0], t[1])
    yield emit(m || DIR_BTN[sp.facing])
  }
}

// ---- skills ---------------------------------------------------------------------------------------------------

export function resolveSkill(s: Session, ref: string | number): number {
  const p = s.player as Any
  const skills: number[] = p.skills ?? []
  if (typeof ref === 'number' || /^\d+$/.test(String(ref))) {
    const n = Number(ref)
    if (n < skills.length) return skills[n]
  }
  const q = String(ref).toLowerCase().replace(/[^a-z0-9]/g, '')
  const d = buuData()
  const hit = skills.find((id) => (d.skills.get(id)?.name ?? '').toLowerCase().replace(/[^a-z0-9]/g, '') === q) ?? skills.find((id) => (d.skills.get(id)?.name ?? '').toLowerCase().replace(/[^a-z0-9]/g, '').includes(q))
  if (hit == null) throw new Error(`unknown skill "${ref}" (this fighter has: ${skills.map((id, i) => `${i}=${d.skills.get(id)?.name}`).join(', ')})`)
  return hit
}

/** cycle L until `id` is selected */
export function* selectSkill(s: Session, id: number): Policy {
  const p = s.player as Any
  for (let tries = 0; tries < 12 && p.skill !== id; ) {
    if (!canAct(s)) {
      yield 0
      continue
    }
    yield BTN.L
    yield 0
    tries++
  }
  if (p.skill !== id) return 'could_not_select'
}

/** the facing that points from the player at the enemy (dominant axis) */
export function facingToward(s: Session, e: Enemy): number {
  const [px, py] = footCenter(s.player)
  const [ex, ey] = footCenter(e)
  const dx = ex - px
  const dy = ey - py
  return Math.abs(dx) >= Math.abs(dy) ? (dx > 0 ? 3 : 2) : dy > 0 ? 0 : 1
}

export function* face(s: Session, dir: number): Policy {
  for (let i = 0; i < 60 && !canAct(s); i++) yield 0
  if (s.player.facing !== dir) {
    yield DIR_BTN[dir]
    yield 0
  }
}

/** line up on the target's row (horizontal shot) or column, keeping the distance, then face it */
export function* align(s: Session, e: Enemy, maxFrames = 120): Policy {
  for (let i = 0; i < maxFrames; i++) {
    if (e.removed || e.defeated) return 'target_defeated'
    const [px, py] = footCenter(s.player)
    const [ex, ey] = footCenter(e)
    const dx = ex - px
    const dy = ey - py
    if (Math.abs(dx) >= Math.abs(dy)) {
      if (Math.abs(dy) <= 2) break
      yield dy > 0 ? BTN.DOWN : BTN.UP
    } else {
      if (Math.abs(dx) <= 2) break
      yield dx > 0 ? BTN.RIGHT : BTN.LEFT
    }
  }
  yield 0
}

export interface SkillOpts {
  charge: number
  taps: number
  gap: number
  target?: Enemy | null
  align?: boolean
  /** extra d-pad held while charging (Flame Kamehameha whip, Dragon Fist head, Energy Sword movement) */
  steerDir?: Mask
  settle: number
}

export function* useSkill(s: Session, id: number, o: SkillOpts): Policy {
  const sel = yield* selectSkill(s, id)
  if (sel) return sel
  if (o.target) {
    if (o.align) {
      const r = yield* align(s, o.target)
      if (r) return r
    }
    yield* face(s, facingToward(s, o.target))
  }
  for (let i = 0; i < 60 && !canAct(s); i++) yield 0
  const info = skillInfo(s, id)
  if (info.usable === false) return `not_usable (EP ${s.player.ep}, needs ${info.ep})`
  for (let k = 0; k < o.taps; k++) {
    for (let f = 0; f < Math.max(1, o.charge); f++) yield BTN.B | (o.steerDir ?? 0)
    yield 0
    for (let g = 1; g < o.gap && k < o.taps - 1; g++) yield 0
  }
  for (let i = 0; i < o.settle; i++) {
    if (canAct(s)) return 'skill_done'
    yield 0
  }
  return 'skill_still_running'
}

export function* dash(s: Session, dir: number, frames: number): Policy {
  // the double tap is only recognised from the idle state (idle control 0x08013022), not while walking
  for (let i = 0; i < 60 && (!canAct(s) || (s.player as Any).state !== 'idle'); i++) yield 0
  yield DIR_BTN[dir]
  yield 0
  for (let i = 0; i < Math.max(1, frames); i++) yield DIR_BTN[dir]
  yield 0
}

export function* block(frames: number, shield: boolean): Policy {
  yield BTN.R
  for (let i = 1; i < frames; i++) yield BTN.R | (shield ? BTN.B : 0)
  yield 0
}

export function dirIndex(d: string): number {
  const i = (DIRS as readonly string[]).indexOf(String(d).toLowerCase())
  if (i < 0) throw new Error(`direction must be one of ${DIRS.join(', ')}`)
  return i
}

export { findEnemy }
