// Transport-independent API of the agent interface: every MCP tool is one method here (server.ts only adds schemas),
// so scripts can drive the same code in-process and the Rust port has one place to mirror.
import { buuData, buuRuleset } from './load'
import { parseButtons, Session, unrle, type BattleSpec, type Replay } from './session'
import { asciiMap, DIRS, findEnemy, observe } from './observe'
import * as M from './macros'
import { Controller, controllerPolicy } from './controller'
import { screenshot, type View } from './render'

export interface NewBattleArgs {
  fighter?: string
  form?: number
  level?: number
  enemy?: string
  count?: number
  preset?: string
  arena?: string
  seed?: number
  skill_level?: number
  stat_spend?: string
  growth?: string
  equipment?: BattleSpec['equipment']
}

const sessions = new Map<string, Session>()
let nextId = 1

export function getSession(id: string): Session {
  const s = sessions.get(id)
  if (!s) throw new Error(`no session "${id}" (open: ${[...sessions.keys()].join(', ') || 'none'}; create one with new_battle)`)
  return s
}

function norm(s: string) {
  return s.toLowerCase().replace(/[^a-z0-9]/g, '')
}

export function listFighters() {
  buuData()
  return buuRuleset.fighters().map((f) => ({
    id: f.id,
    name: f.name,
    default_form: f.defaultForm,
    forms: f.forms.map((x) => `${x.id}: ${x.name.replace(/ · form \d+$/, '')}`),
    moves: f.moves.map((m) => `${m.button}: ${m.name}${m.note ? ` (${m.note})` : ''}`),
  }))
}

export function listEnemies(search?: string, group?: string, limit = 25) {
  buuData()
  const presets = buuRuleset.presets()
  const q = search ? norm(search) : ''
  const all = buuRuleset.enemies().filter((e) => (!q || norm(e.name).includes(q) || e.id === search || norm(e.note ?? '').includes(q)) && (!group || norm(e.group).includes(norm(group))))
  return {
    total: all.length,
    enemies: all.slice(0, limit).map((e) => ({
      id: e.id,
      name: e.name,
      group: e.group,
      hp: e.hp,
      stats: `STR ${e.stats.STR ?? '?'} POW ${e.stats.POW ?? '?'} END ${e.stats.END ?? '?'}`,
      ...(e.tags.length ? { tags: e.tags } : {}),
      ...(e.note ? { note: e.note.slice(0, 120) } : {}),
      ...(presets.some((p) => p.phases.some((ph) => ph.enemy.split('+').map((x) => x.replace('~', '')).includes(e.id))) ? { presets: presets.filter((p) => p.phases.some((ph) => ph.enemy.split('+').map((x) => x.replace('~', '')).includes(e.id))).map((p) => p.id) } : {}),
    })),
  }
}

export function listArenas() {
  buuData()
  return buuRuleset.arenas().map((a) => ({ id: a.id, name: a.name, bounds: [a.bounds.x0, a.bounds.y0, a.bounds.x1, a.bounds.y1] }))
}

export function listPresets() {
  buuData()
  const d = buuData()
  return buuRuleset.presets().map((p) => ({
    id: p.id,
    name: p.name,
    arena: p.arena,
    phases: p.phases.map((ph) => `${ph.count}x ${ph.enemy.split('+').map((x) => d.enemies[Number(x.replace(/^~?t/, ''))]?.name + (x.startsWith('~') ? ' (helper)' : '')).join(' + ')}`),
    ...(p.note ? { note: p.note } : {}),
  }))
}

function resolveEnemy(ref: string): string {
  if (/^~?t\d+(\+~?t\d+)*$/.test(ref)) return ref
  const q = norm(ref)
  const list = buuRuleset.enemies()
  const rank = (g: string) => (g === 'Boss' ? 0 : g === 'Enemy' ? 1 : g === 'Story group' ? 2 : 3)
  const exact = list.filter((e) => norm(e.name) === q).sort((a, b) => rank(a.group) - rank(b.group))
  const part = list.filter((e) => norm(e.name).includes(q)).sort((a, b) => rank(a.group) - rank(b.group))
  const hit = exact[0] ?? part[0]
  if (!hit) throw new Error(`no enemy matches "${ref}" (try list_enemies)`)
  return hit.id
}

function defaultArena(enemyId: string): string {
  const d = buuData()
  const t = d.enemies[Number(enemyId.split('+')[0].replace(/^~?t/, ''))]
  const arenas = buuRuleset.arenas()
  for (const sp of t?.spawns ?? []) {
    const a = arenas.find((x) => x.note === `map ${sp.world}-${sp.map}`)
    if (a) return a.id
  }
  return 'crater'
}

export function newBattle(a: NewBattleArgs) {
  buuData()
  const fighters = buuRuleset.fighters()
  const fighter = fighters.find((f) => f.id === a.fighter || norm(f.name) === norm(a.fighter ?? 'goku'))
  if (!fighter) throw new Error(`unknown fighter "${a.fighter}" (have: ${fighters.map((f) => f.id).join(', ')})`)
  let phases: BattleSpec['phases']
  let arena = a.arena
  if (a.preset) {
    const all = buuRuleset.presets()
    const p = all.find((x) => x.id === a.preset) ?? all.find((x) => norm(x.name).startsWith(norm(a.preset!))) ?? all.find((x) => norm(x.name).includes(norm(a.preset!)))
    if (!p) throw new Error(`unknown preset "${a.preset}" (list_presets)`)
    phases = p.phases
    arena = arena ?? p.arena
  } else {
    const id = resolveEnemy(a.enemy ?? 't68')
    phases = [{ enemy: id, count: Math.max(1, Math.min(12, a.count ?? 1)) }]
    arena = arena ?? defaultArena(id)
  }
  if (!buuRuleset.arenas().some((x) => x.id === arena)) throw new Error(`unknown arena "${arena}" (list_arenas)`)
  const spec: BattleSpec = {
    fighter: fighter.id,
    form: a.form,
    level: Math.max(50, Math.min(200, a.level ?? 80)),
    phases,
    arena: arena!,
    seed: a.seed ?? 1,
    skillLevel: Math.max(0, Math.min(2, a.skill_level ?? 2)),
    statSpend: a.stat_spend ?? 'even',
    growth: a.growth ?? 'average',
    equipment: a.equipment,
  }
  const id = 's' + nextId++
  const s = new Session(id, spec)
  sessions.set(id, s)
  if (sessions.size > 32) sessions.delete(sessions.keys().next().value!)
  return { session: id, spec, observation: observe(s, { full: true, map: true }) }
}

export function closeSession(id: string) {
  return { closed: sessions.delete(id) }
}
export function listSessions() {
  return [...sessions.values()].map((s) => ({ session: s.id, frame: s.frame, outcome: s.outcome(), fighter: s.spec.fighter, phases: s.spec.phases, snapshots: [...s.snapshots.keys()] }))
}

export function doObserve(id: string, o: { map?: boolean; full?: boolean }) {
  return observe(getSession(id), o)
}

export function doMap(id: string, cell = 16) {
  return asciiMap(getSession(id), cell)
}

export function doScreenshot(id: string, view: View, scale: number, labels: boolean) {
  return screenshot(getSession(id), view, scale, labels)
}

function act(s: Session, pol: M.Policy, max: number, stopOn: M.StopOn[], extra: Record<string, unknown> = {}) {
  const r = M.runPolicy(s, pol, max, stopOn)
  return { result: { ...r, ...extra }, observation: observe(s) }
}

export function press(id: string, a: { buttons?: unknown; frames?: number; steps?: { buttons?: unknown; frames?: number }[]; stop_on?: M.StopOn[] }) {
  const s = getSession(id)
  const steps = a.steps?.length ? a.steps : [{ buttons: a.buttons, frames: a.frames ?? 1 }]
  const seq = steps.map((st) => ({ mask: parseButtons(st.buttons), frames: Math.max(0, Math.min(3600, st.frames ?? 1)) }))
  const total = seq.reduce((n, x) => n + x.frames, 0)
  return act(s, M.sequence(seq), total, a.stop_on ?? [])
}

export function wait(id: string, frames: number, stopOn: M.StopOn[] = []) {
  return act(getSession(id), M.hold(0, frames), Math.min(3600, frames), stopOn)
}

export function moveTo(id: string, x: number, y: number, maxFrames = 600, stopOn: M.StopOn[] = ['hurt']) {
  const s = getSession(id)
  const p = s.player
  // (x, y) is a footprint centre like observation positions
  return act(s, M.walkTo(s, Math.round(x - (p.footW >> 1)), Math.round(y - (p.footH >> 1))), maxFrames, stopOn)
}

export function face(id: string, target?: string, direction?: string) {
  const s = getSession(id)
  let dir: number
  if (direction) dir = M.dirIndex(direction)
  else {
    const e = findEnemy(s, target)
    if (!e) throw new Error('no such enemy')
    dir = M.facingToward(s, e)
  }
  return act(s, M.face(s, dir), 70, [], { facing: DIRS[dir] })
}

export function attack(id: string, target?: string, hits?: number, maxFrames = 300, stopOn: M.StopOn[] = []) {
  const s = getSession(id)
  const e = findEnemy(s, target)
  if (!e) throw new Error(target ? `no live enemy matches "${target}"` : 'no live enemies')
  return act(s, M.attack(s, e, { hits }), maxFrames, stopOn, { target: `#${e.id} ${e.label}` })
}

export function useSkill(id: string, a: { skill: string | number; charge_frames?: number; taps?: number; gap?: number; target?: string; align?: boolean; steer?: string; max_frames?: number; stop_on?: M.StopOn[]; settle?: number }) {
  const s = getSession(id)
  const sk = M.resolveSkill(s, a.skill)
  const e = a.target ? findEnemy(s, a.target) : null
  if (a.target && !e) throw new Error(`no live enemy matches "${a.target}"`)
  const name = buuData().skills.get(sk)?.name
  return act(
    s,
    M.useSkill(s, sk, { charge: a.charge_frames ?? 1, taps: Math.max(1, a.taps ?? 1), gap: a.gap ?? 26, target: e, align: a.align, steerDir: parseButtons(a.steer), settle: a.settle ?? 240 }),
    a.max_frames ?? 900,
    a.stop_on ?? [],
    { skill: name, ...(e ? { target: `#${e.id} ${e.label}` } : {}) },
  )
}

export function block(id: string, frames: number, shield = false, stopOn: M.StopOn[] = []) {
  const s = getSession(id)
  return act(s, M.block(frames, shield), frames + 1, stopOn)
}

export function dash(id: string, direction: string, frames = 20, stopOn: M.StopOn[] = []) {
  const s = getSession(id)
  return act(s, M.dash(s, M.dirIndex(direction), frames), frames + 64, stopOn)
}

export function saveState(id: string, name: string) {
  const s = getSession(id)
  s.save(name)
  return { saved: name, frame: s.frame, snapshots: [...s.snapshots.keys()] }
}
export function loadState(id: string, name: string) {
  const s = getSession(id)
  const t0 = performance.now()
  s.load(name)
  return { loaded: name, resim_ms: Math.round(performance.now() - t0), observation: observe(s) }
}
export function rewind(id: string, frames: number) {
  const s = getSession(id)
  const t0 = performance.now()
  s.rewind(frames)
  return { rewound_to: s.frame, resim_ms: Math.round(performance.now() - t0), observation: observe(s) }
}

export function getReplay(id: string) {
  return getSession(id).replay()
}

export function playReplay(r: Replay, keep = true) {
  if (!r || r.format !== 'buu-agent-replay/1') throw new Error('not a buu-agent-replay/1 object (use get_replay)')
  const sid = 's' + nextId++
  const s = new Session(sid, r.spec)
  const t0 = performance.now()
  s.resimulate(unrle(r.inputs))
  const hash = s.hash()
  if (keep) sessions.set(sid, s)
  return { session: keep ? sid : null, frames: s.frame, outcome: s.outcome(), hash, expected: r.hash, deterministic: hash === r.hash, resim_ms: Math.round(performance.now() - t0) }
}

export function runController(id: string, a: { code: string; frames: number; every?: number; stop_on?: M.StopOn[]; timeout_ms?: number }) {
  const s = getSession(id)
  const c = new Controller(a.code, a.timeout_ms ?? 50)
  const st: { error?: string; stoppedBy?: string } = {}
  const t0 = performance.now()
  const r = M.runPolicy(s, controllerPolicy(s, c, Math.max(1, a.every ?? 1), st), Math.min(36000, a.frames), a.stop_on ?? [])
  let logs: string[] = []
  try {
    logs = c.logs()
  } catch {
    /* ignore */
  }
  return { result: { ...r, ...(st.error ? { error: st.error } : {}), logs: logs.slice(-40), wall_ms: Math.round(performance.now() - t0) }, observation: observe(s) }
}
