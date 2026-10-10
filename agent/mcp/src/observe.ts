// Compact, LLM-oriented observations of a Session. Everything here only *reads* the sim through fields the ruleset
// exposes (positions, HP/EP, state labels, AI stack, collision grids) so it survives refactors of the entity code.
import type { Rect } from '../../../site/src/battle/core/geom'
import type { Entity } from '../../../site/src/battle/core/world'
import { Shot } from '../../../site/src/battle/rulesets/buu/projectiles'
import type { Enemy } from '../../../site/src/battle/rulesets/buu/enemy'
import type { Session } from './session'
import { buuData } from './load'

export const DIRS = ['down', 'up', 'left', 'right'] as const
export type DirName = (typeof DIRS)[number]

/** player melee rects 0x083C344C per facing (x0, y0, x1, y1 relative to the footprint's top-left); copied from
 *  player.ts (PUNCH_RECT) so this file does not depend on the player module's exports */
export const PUNCH_RECT = [
  [0, 0, 16, 16],
  [0, -24, 16, 0],
  [-8, -24, 0, 8],
  [16, -24, 24, 8],
]

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Any = any

const overlaps = (a: Rect, b: Rect) => a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1

export function punchRect(px: number, py: number, facing: number): Rect {
  const r = PUNCH_RECT[facing]
  return { x0: px + r[0], y0: py + r[1], x1: px + r[2], y1: py + r[3] }
}

/** facings from which a punch right now would overlap the enemy's body box */
export function meleeFacings(s: Session, e: Enemy): number[] {
  const p = s.player
  const b = e.bodyBox()
  return [0, 1, 2, 3].filter((f) => overlaps(punchRect(p.px, p.py, f), b))
}

export function footCenter(a: { px: number; py: number; footW: number; footH: number }): [number, number] {
  return [a.px + (a.footW >> 1), a.py + (a.footH >> 1)]
}

export function compass(dx: number, dy: number): string {
  const ax = Math.abs(dx)
  const ay = Math.abs(dy)
  if (ax < 4 && ay < 4) return 'here'
  const h = dx > 0 ? 'right' : 'left'
  const v = dy > 0 ? 'down' : 'up'
  if (ax > 2.4 * ay) return h
  if (ay > 2.4 * ax) return v
  return `${v}-${h}`
}

/** coarse player state that the macros / controllers can rely on */
export function playerState(s: Session): string {
  const p = s.player as Any
  return String(p.state ?? p.gs ?? 'unknown')
}
export function canAct(s: Session): boolean {
  const p = s.player as Any
  if (p.dead || p.heldBy) return false
  return ['idle', 'walk', 'dash'].includes(playerState(s))
}

export function aiLabel(e: Enemy): string {
  const c = e.ai as Any
  const top = c?.top?.()
  if (!top) return '-'
  const names: string[] = []
  let n = top.rt
  for (let i = 0; n && i < 24; i++) {
    names.push(n.node?.name)
    n = n.active
  }
  // the last three nodes of the running chain without the pure control-flow ones, e.g. "IfPlayerWithin2>GotoState"
  const flow = /^(Selector|Sequence|Parallel|Random|Loop|Repeat|Succeed|Fail|Root)/
  const meaningful = names.filter((x) => x && !flow.test(x))
  return meaningful.slice(-3).join('>') || String(names[names.length - 1] ?? '-')
}

export function liveEnemies(s: Session): Enemy[] {
  return s.enemies().filter((e) => !e.removed && !e.defeated)
}

/** resolve an enemy reference: numeric id, "#id", or name substring; default = nearest primary */
export function findEnemy(s: Session, ref?: string | number | null): Enemy | null {
  const live = liveEnemies(s).filter((e) => !e.part)
  if (ref == null || ref === '' || ref === 'nearest') return nearest(s, live.filter((e) => e.primary).length ? live.filter((e) => e.primary) : live)
  const str = String(ref).replace(/^#/, '')
  if (/^\d+$/.test(str)) return liveEnemies(s).find((e) => e.id === Number(str)) ?? null
  const q = str.toLowerCase()
  return nearest(s, live.filter((e) => e.label.toLowerCase().includes(q)))
}
function nearest(s: Session, list: Enemy[]): Enemy | null {
  const [px, py] = footCenter(s.player)
  let best: Enemy | null = null
  let bd = Infinity
  for (const e of list) {
    const [x, y] = footCenter(e)
    const d = Math.hypot(x - px, y - py)
    if (d < bd) {
      bd = d
      best = e
    }
  }
  return best
}

type Hazard = Entity & { px: number; py: number; owner?: Any; team?: number; t?: number }
/** things that can hurt (shots, beams, Janemba's rising hands, Broly's rocks) or that you fired */
export function hazards(s: Session): Hazard[] {
  return s.w.entities.filter((e) => !e.removed && (e instanceof Shot || e.kind === 'hand' || e.kind === 'rock') && (typeof (e as Any).rect === 'function' || typeof (e as Any).bodyBox === 'function')) as Any
}
export function hazardRect(h: Hazard): Rect {
  const a = h as Any
  return typeof a.rect === 'function' ? a.rect() : a.bodyBox()
}

/** how far (px, max `cap`) the player's footprint can move in each direction before a wall */
export function wallDistances(s: Session, cap = 160): Record<DirName, number> {
  const p = s.player
  const out = {} as Record<DirName, number>
  const vec = [[0, 1], [0, -1], [-1, 0], [1, 0]]
  DIRS.forEach((d, i) => {
    let n = 0
    while (n < cap && p.fits(s.w, p.px + vec[i][0] * (n + 1), p.py + vec[i][1] * (n + 1))) n++
    out[d] = n
  })
  return out
}

/** ASCII map of the arena at `cell` px per character: '#' wall, '.' walkable, 'P' you, a-z enemies (legend), '*'
 *  hostile shot, 'o' your shot. Row/column 0 are at (x0, y0). */
export function asciiMap(s: Session, cell = 16): { origin: [number, number]; cell: number; rows: string[]; legend: Record<string, string> } {
  const w = s.w
  const b = w.arena.bounds
  const p = s.player
  const cols = Math.ceil((b.x1 - b.x0) / cell)
  const rowsN = Math.ceil((b.y1 - b.y0) / cell)
  const grid: string[][] = []
  for (let r = 0; r < rowsN; r++) {
    const row: string[] = []
    for (let c = 0; c < cols; c++) {
      // a cell is walkable when the player's footprint fits centred in it
      const cx = b.x0 + c * cell + (cell >> 1) - (p.footW >> 1)
      const cy = b.y0 + r * cell + (cell >> 1) - (p.footH >> 1)
      row.push(p.fits(w, cx, cy) ? '.' : '#')
    }
    grid.push(row)
  }
  const put = (x: number, y: number, ch: string) => {
    const c = Math.floor((x - b.x0) / cell)
    const r = Math.floor((y - b.y0) / cell)
    if (r >= 0 && r < rowsN && c >= 0 && c < cols) grid[r][c] = ch
  }
  for (const h of hazards(s)) {
    const r = hazardRect(h)
    put((r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1, h.team === 1 ? 'o' : '*')
  }
  const legend: Record<string, string> = {}
  liveEnemies(s)
    .filter((e) => !e.part)
    .forEach((e, i) => {
      const ch = String.fromCharCode(97 + (i % 26))
      legend[ch] = `#${e.id} ${e.label}`
      const [x, y] = footCenter(e)
      put(x, y, ch)
    })
  const [x, y] = footCenter(p)
  put(x, y, 'P')
  return { origin: [b.x0, b.y0], cell, rows: grid.map((r) => r.join('')), legend }
}

export interface ObsOpts {
  map?: boolean
  /** include the static parts (skill list, arena name) */
  full?: boolean
  /** don't drain events (controller frames) */
  keepEvents?: boolean
}

export function skillInfo(s: Session, id: number) {
  const p = s.player as Any
  const d = buuData().skills.get(id)
  let cost: number | undefined
  try {
    cost = p.skillCost?.(id)
  } catch {
    cost = undefined
  }
  let usable: boolean | undefined
  try {
    usable = p.canUseSkill?.(id)
  } catch {
    usable = undefined
  }
  return { name: d?.name ?? `skill ${id}`, ep: cost, usable }
}

export function observe(s: Session, o: ObsOpts = {}) {
  const w = s.w
  const p = s.player as Any
  const [px, py] = footCenter(p)
  const skills: number[] = p.skills ?? []
  const cur = p.skill ?? skills[p.slot ?? 0]
  const player: Record<string, unknown> = {
    pos: [px, py],
    facing: DIRS[p.facing],
    hp: [p.hp, p.maxHp],
    ep: [p.ep, p.maxEp],
    state: playerState(s),
    can_act: canAct(s),
    skill: cur != null ? { slot: p.slot, ...skillInfo(s, cur) } : null,
  }
  if (!p.hittable) player.invulnerable = true
  if (p.heldBy) player.held = p.holdKind ?? true
  if (p.form?.ss || p.form?.ss3) player.ss_gauge = p.form.ss3 ? p.ss3Gauge >> 16 : p.ssGauge
  if (p.ssCooldown > 0) player.ss_cooldown = p.ssCooldown >> 8
  if (s.w.input.held) player.holding = maskNamesLocal(s.w.input.held)
  if (o.full) {
    player.form = p.form?.label ?? p.form?.name
    player.level = p.rec?.level
    player.stats = { STR: p.stats?.STR, POW: p.stats?.POW, END: p.stats?.END, SPD: p.eff?.SPD, walk_px_per_frame: +(p.walkSpeed?.() / 256).toFixed(2) }
    player.skills = skills.map((id, i) => ({ slot: i, ...skillInfo(s, id) }))
    player.foot = [p.footW, p.footH]
  }
  const enemies = liveEnemies(s)
    .filter((e) => !e.part)
    .map((e) => {
      const [ex, ey] = footCenter(e)
      const dx = ex - px
      const dy = ey - py
      const mf = meleeFacings(s, e)
      const r: Record<string, unknown> = {
        id: e.id,
        name: e.label,
        pos: [ex, ey],
        hp: [e.hp, e.maxHp],
        dist: Math.round(Math.hypot(dx, dy)),
        dir: compass(dx, dy),
        facing: DIRS[e.facing],
        ai: aiLabel(e),
      }
      if (mf.length) r.melee = mf.includes(p.facing) ? 'in_range' : `turn_${DIRS[mf[0]]}`
      if (!e.hittable) r.hittable = false
      if (e.z < -4) r.airborne = -e.z
      if (!e.primary) r.optional = true
      // damage taken multipliers (resist bytes, 128 = x1) and on-hit heals by attack kind (decoded aux trees)
      const tk = takes(e)
      if (tk) r.takes = tk
      if (e.stunned) r.stunned = true
      return r
    })
    .sort((a, b) => (a.dist as number) - (b.dist as number))
  const body = p.bodyBox()
  const shots = hazards(s).map((h) => {
    const r = hazardRect(h)
    const cx = (r.x0 + r.x1) >> 1
    const cy = (r.y0 + r.y1) >> 1
    const prev = s.prevPos.get(h.id)
    const vx = prev ? h.px - prev[0] : 0
    const vy = prev ? h.py - prev[1] : 0
    const team = h.team === 1 ? 'yours' : h.team === 2 ? 'everyone' : 'hostile'
    const out: Record<string, unknown> = { kind: h.kind, team, pos: [cx, cy], size: [r.x1 - r.x0, r.y1 - r.y0], vel: [vx, vy] }
    // Janemba's hand: rises under you, hits once during its frames 16..55 (enemy.ts HandRise)
    if (h.kind === 'hand') {
      out.note = 'rising hand: hits if you are on it'
      out.armed_in = Math.max(0, 16 - (h.t ?? 0))
      out.gone_in = Math.max(0, 57 - (h.t ?? 0))
    }
    const own = h.owner as Any
    if (own && own.kind === 'enemy') out.from = `#${own.id} ${own.label}`
    const atk = (h as Any).o?.atk ?? (h as Any).atk
    if (atk) out.damage = atk.kind === 0 ? 'melee' : 'energy'
    if (team !== 'yours') {
      if (overlaps(r, body)) out.eta = 0
      else if (vx || vy) {
        for (let t = 1; t <= 90; t++) {
          if (overlaps({ x0: r.x0 + vx * t, y0: r.y0 + vy * t, x1: r.x1 + vx * t, y1: r.y1 + vy * t }, body)) {
            out.eta = t
            break
          }
        }
      }
    }
    return out
  })
  const res: Record<string, unknown> = {
    session: s.id,
    frame: w.frame,
    outcome: s.outcome(),
    player,
    enemies,
  }
  if (shots.length) res.projectiles = shots
  res.walls = wallDistances(s)
  if (o.full) {
    res.arena = { id: w.arena.id, name: w.arena.name, bounds: [w.arena.bounds.x0, w.arena.bounds.y0, w.arena.bounds.x1, w.arena.bounds.y1] }
  }
  if (o.map) res.map = asciiMap(s)
  if (!o.keepEvents) {
    const ev = s.drainEvents()
    if (ev.events.length) res.events = ev.events
    if (ev.dropped) res.events_dropped = ev.dropped
    if (ev.sfx.length) res.sfx = ev.sfx
  }
  s.lastObserved = w.frame
  return res
}

const takesCache = new Map<number, Record<string, string> | null>()
/** {melee: 'x0.05', energy: 'heals'} - resist bytes plus a scan of the type's on-hit trees for
 *  IfAttack(kind) -> HealFraction (Yakon eats energy attacks) and the absence of any ApplyDamage (story invulnerable) */
export function takes(e: Enemy): Record<string, string> | null {
  const t = e.type
  if (takesCache.has(t.type)) return takesCache.get(t.type)!
  const d = buuData()
  const out: Record<string, string> = {}
  if (t.resM !== 128) out.melee = `x${(t.resM / 128).toFixed(2)}`
  if (t.resE !== 128) out.energy = `x${(t.resE / 128).toFixed(2)}`
  // states reachable from the type's AI root (GotoState / SubState / PushState params), then their aux trees
  const seen = new Set<number>()
  const stack = [t.ai]
  let apply = false
  const heals = new Set<number>()
  const walk = (n: Any, atk: number | null, aux: boolean) => {
    if (!n) return
    for (const [k, v] of Object.entries(n.params ?? {}) as [string, Any][]) if (k === 'state' && typeof v?.raw === 'number' && !seen.has(v.raw)) stack.push(v.raw)
    if (aux && /ApplyDamage/.test(n.name)) apply = true
    if (aux && n.name === 'HealFraction' && atk != null) heals.add(atk)
    const a = n.name === 'IfAttack' ? n.params?.atk?.raw ?? atk : atk
    for (const c of n.children ?? []) walk(c, a, aux)
  }
  while (stack.length && seen.size < 200) {
    const st = stack.pop()!
    if (seen.has(st)) continue
    seen.add(st)
    walk(d.states.get(st), null, false)
    walk(d.aux.get(st), null, true)
  }
  if (heals.has(0)) out.melee = 'heals it'
  if (heals.has(1)) out.energy = 'heals it'
  if (!apply && seen.size) out.all = 'no damage (story invulnerable?)'
  const r = Object.keys(out).length ? out : null
  takesCache.set(t.type, r)
  return r
}

function maskNamesLocal(m: number): string {
  const names = ['A', 'B', 'SELECT', 'START', 'RIGHT', 'LEFT', 'UP', 'DOWN', 'R', 'L']
  return names.filter((_, i) => m & (1 << i)).join('+')
}
