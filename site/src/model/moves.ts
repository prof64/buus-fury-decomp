// Move records of the three games normalised into one list, plus helpers that pull every hitbox
// rectangle out of a move record so it can be drawn over the attacker's sprite.
import type { Overlay } from '../components/SpritePlayer'
import type { Rect } from '../lib/sprites'
import { type J } from '../lib/util'

export interface MoveVM {
  key: string
  name: string
  user: 'player' | 'enemy'
  kind: string
  characters?: string
  power?: number
  attackId?: number | string
  scale?: string
  cost?: string
  startup?: string
  hits?: string
  speed?: string
  evidence?: string
  description?: string
  raw: J
  /** extra raw records describing the same move (e.g. LoG2 move-type row) */
  extra?: J
}

const DIRS = ['down', 'up', 'left', 'right']
const COLORS = ['#ff3b3b', '#ff9f1c', '#b04bff', '#14b88a', '#e4c100', '#ff5fa2']

function dirOf(path: string[]): string | undefined {
  for (let i = path.length - 1; i >= 0; i--) {
    const p = path[i].toLowerCase()
    for (const d of DIRS) if (p === d || p.startsWith(d + '_') || p.endsWith('_' + d)) return d
  }
  return undefined
}

function is4(v: J): v is [number, number, number, number] {
  return Array.isArray(v) && v.length === 4 && v.every((x) => typeof x === 'number')
}
function isRectObj(v: J): v is Rect {
  return v && typeof v === 'object' && ['x0', 'y0', 'x1', 'y1'].every((k) => typeof v[k] === 'number')
}

export interface FoundRect {
  dir: string | undefined
  rect: Rect
  label: string
}

/** Recursively collects rectangles (keys containing "rect"/"hitbox"/"head") and spawn points from a move record. */
export function findRects(obj: J, path: string[] = []): FoundRect[] {
  const out: FoundRect[] = []
  if (!obj || typeof obj !== 'object') return out
  const joined = path.join('.').toLowerCase()
  if (is4(obj) && /rect|hitbox|swing|head/.test(joined)) {
    const [x0, y0, x1, y1] = obj
    if (x1 > x0 && y1 > y0) out.push({ dir: dirOf(path), rect: { x0, y0, x1, y1 }, label: path.filter((p) => !DIRS.includes(p)).slice(-2).join(' ') })
    return out
  }
  if (isRectObj(obj) && /rect/.test(joined)) {
    out.push({ dir: (obj as J).facing ?? (obj as J).dir ?? dirOf(path), rect: obj, label: path.slice(-2).join(' ') })
    return out
  }
  if (Array.isArray(obj) && obj.length === 2 && obj.every((x) => typeof x === 'number') && /spawn_pos/.test(joined)) {
    const [x, y] = obj as number[]
    out.push({ dir: dirOf(path), rect: { x0: x - 2, y0: y - 2, x1: x + 2, y1: y + 2 }, label: 'spawn point' })
    return out
  }
  if (Array.isArray(obj)) {
    obj.forEach((v, i) => out.push(...findRects(v, [...path, String(i)])))
    return out
  }
  for (const [k, v] of Object.entries(obj)) {
    if (k === 'evidence' || k === 'note') continue
    out.push(...findRects(v, [...path, k]))
  }
  return out
}

/** Overlays for one facing. Rects with no facing in their path are assumed measured facing right. */
export function overlaysFor(found: FoundRect[], dir: string): Overlay[] {
  const labels = [...new Set(found.map((f) => f.label))]
  return found
    .filter((f) => (f.dir ?? 'right') === dir)
    .map((f) => ({ rect: f.rect, color: COLORS[labels.indexOf(f.label) % COLORS.length], label: f.label, dashed: f.label === 'spawn point' }))
}

export function legendFor(found: FoundRect[]): { label: string; color: string }[] {
  const labels = [...new Set(found.map((f) => f.label))]
  return labels.map((l, i) => ({ label: l, color: COLORS[i % COLORS.length] }))
}

/* ---------------- per game lists ---------------- */

export function log1Moves(moves: J): MoveVM[] {
  return (moves?.moves ?? []).map((m: J) => ({
    key: String(m.id),
    name: m.name,
    user: m.user,
    kind: m.id === 'melee' || m.id === 'enemy_melee' ? 'melee' : m.id === 'fly' ? 'movement' : 'ki',
    damage: m.damage,
    cost: typeof m.ki_cost === 'string' ? m.ki_cost : m.ki_cost !== undefined ? String(m.ki_cost) : m.energy_cost,
    startup: m.startup_frames !== undefined ? `${m.startup_frames} f` : undefined,
    speed: m.speed_px_per_frame !== undefined ? `${m.speed_px_per_frame} px/f` : undefined,
    evidence: m.verified,
    description: typeof m.damage === 'string' ? m.damage : m.damage === 0 ? 'no damage' : undefined,
    raw: m,
  }))
}

export function log2Moves(moves: J): MoveVM[] {
  const table: J[] = moves?.attack_table ?? []
  const pm: J[] = moves?.player_moves ?? []
  const out: MoveVM[] = (moves?.player_attacks ?? []).map((a: J, i: number) => {
    const at = table.find((t) => t.id === a.attack_table_id)
    const mt = pm.find((p) => a.player_state && p.player_state !== undefined && Number(p.player_state) === parseInt(String(a.player_state), 16))
    return {
      key: 'p' + i,
      name: a.name.replace(/\s*\(.*\)$/, '') || a.name,
      user: 'player',
      kind: a.type ?? (at ? (at.type === 0 ? 'melee' : 'energy') : 'other'),
      characters: a.characters,
      power: a.power ?? at?.power,
      attackId: a.attack_table_id ?? (a.attack_table_ids ? a.attack_table_ids.join('+') : undefined),
      scale: a.scale !== undefined ? String(a.scale) : undefined,
      cost: a.ep ? [a.ep.min !== undefined ? `min ${a.ep.min}` : '', a.ep.start !== undefined ? `start −${a.ep.start}` : '', a.ep.drain ?? a.ep.per_extra_shot ? (a.ep.drain ?? `${a.ep.per_extra_shot}/extra shot`) : ''].filter(Boolean).join(', ') : '—',
      startup: a.timing?.startup_frames !== undefined ? `${a.timing.startup_frames} f` : a.timing?.charge_frames !== undefined ? `${a.timing.charge_frames} f charge` : undefined,
      hits: a.hits ? [a.hits.count, a.hits.pierce ? 'pierce' : '', a.hits.rehit_interval_frames ? `every ${a.hits.rehit_interval_frames} f` : ''].filter(Boolean).join(' · ') : undefined,
      speed: a.projectile?.speed_px_per_frame !== undefined ? `${a.projectile.speed_px_per_frame} px/f` : undefined,
      evidence: a.evidence,
      description: mt?.ui_description,
      raw: a,
      extra: mt,
    } as MoveVM
  })
  for (const e of moves?.enemy_attacks ?? []) {
    out.push({
      key: 'e-' + e.key,
      name: e.key.replace(/_/g, ' '),
      user: 'enemy',
      kind: e.type ?? e.attack_table_entry?.type ?? 'energy',
      power: e.power ?? e.attack_table_entry?.power,
      attackId: e.attack_id,
      scale: e.scale !== undefined ? String(e.scale) : undefined,
      speed: e.projectile?.speed ?? e.projectile_params?.speed,
      evidence: e.evidence ?? e.projectile?.evidence,
      description: e.rule ?? e.hitbox ?? e.projectile?.ends,
      characters: (e.used_by_classes ?? [])
        .slice(0, 6)
        .map((c: J) => (c.names ?? []).join('/') || 'class ' + c.class_id)
        .join(', '),
      raw: e,
    })
  }
  return out
}

export function buuMoves(moves: J, detail: J): MoveVM[] {
  const types: J[] = moves?.attack_types ?? []
  const out: MoveVM[] = [
    {
      key: 'melee',
      name: 'Punch (A)',
      user: 'player',
      kind: 'melee',
      power: 1024,
      attackId: 'sub 2',
      scale: '1024',
      cost: '+1 EP gained per damaging hit',
      startup: '0 f',
      hits: '1',
      evidence: moves?.melee?.status,
      description: moves?.melee?.after,
      raw: { ...(moves?.melee ?? {}), rects_rel_player: detail?.melee?.rects_rel_player },
    },
  ]
  for (const s of moves?.skills ?? []) {
    const d = (detail?.skills ?? []).find((x: J) => x.id === s.id)
    const lv0 = s.measured?.by_level?.['0']
    const subs: number[] = (lv0?.attack_type_subs ?? []).map((p: J) => p[1])
    const at = types.find((t) => subs.includes(t.sub))
    const mults = Object.values<J>(s.measured?.by_level ?? {}).map((l) => (l.damage_multipliers ?? []).join('/') || '–')
    out.push({
      key: 's' + s.id,
      name: s.name,
      user: 'player',
      kind: d?.kind ?? (at ? at.kind_name : 'special'),
      power: at?.power,
      attackId: subs.length ? 'sub ' + subs.join('/') : undefined,
      scale: mults.length ? mults.join(' | ') : undefined,
      cost: s.ep?.ep_cost_by_level ? s.ep.ep_cost_by_level.join(' / ') : s.ep?.canuse_min_ep !== undefined ? `min ${s.ep.canuse_min_ep}` : d?.cost?.fixed !== undefined ? String(d.cost.fixed) : s.ep?.rule,
      startup: lv0?.first_hit_frame !== undefined && lv0?.first_hit_frame !== null ? `first hit f${lv0.first_hit_frame}` : undefined,
      evidence: s.measured?.setup ? 'measured' : undefined,
      description: s.description,
      raw: s,
      extra: d,
    })
  }
  return out
}
