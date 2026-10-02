// One battle = one Session: the real Buu ruleset running in a World, an input log (one held-button mask per frame),
// events since the last observation, named snapshots and replays.
//
// Save / load / rewind are implemented by *re-simulation*: the sim is deterministic (seeded game RNG, fixed update
// order) and runs ~200k frames/s headless, so a snapshot is just a copy of the input prefix and loading one rebuilds
// the World from the config and replays the inputs (10 000 frames ~ 50 ms). Deep-cloning the World is not possible
// without touching the sim: entities, the battle object and the context hold closures over their creation scope.
import { World } from '../../../site/src/battle/core/world'
import type { BattleConfig, RulesetBattle } from '../../../site/src/battle/core/ruleset'
import { BTN } from '../../../site/src/battle/core/input'
import { setAudioSink, type AudioEventType, type AudioEventData } from '../../../site/src/battle/audio/bus'
import { BuuRng } from '../../../site/src/battle/rulesets/buu/rng'
import { ATK_NAMES } from '../../../site/src/battle/rulesets/buu/damage'
import type { Player } from '../../../site/src/battle/rulesets/buu/player'
import type { Enemy } from '../../../site/src/battle/rulesets/buu/enemy'
import { buuData, buuRuleset, sfxName } from './load'

export type Mask = number
export const BUTTONS = ['A', 'B', 'SELECT', 'START', 'RIGHT', 'LEFT', 'UP', 'DOWN', 'R', 'L'] as const
export type Button = (typeof BUTTONS)[number]

/** "RIGHT+A", "right a", ["RIGHT","A"], 17 -> mask. Aliases: punch=A, skill=B, block=R, cycle=L. */
export function parseButtons(b: unknown): Mask {
  if (b == null || b === '') return 0
  if (typeof b === 'number') return b & 0x3ff
  const parts = Array.isArray(b) ? b.map(String) : String(b).split(/[+,\s|]+/)
  let m = 0
  const alias: Record<string, Button> = { PUNCH: 'A', SKILL: 'B', BLOCK: 'R', CYCLE: 'L', U: 'UP', D: 'DOWN', LT: 'LEFT', RT: 'RIGHT' }
  for (const raw of parts) {
    const p = raw.trim().toUpperCase()
    if (!p || p === 'NONE' || p === '-') continue
    const name = (alias[p] ?? p) as Button
    if (!(name in BTN)) throw new Error(`unknown button "${raw}" (use ${BUTTONS.join(', ')})`)
    m |= BTN[name]
  }
  return m
}
export function maskNames(m: Mask): string {
  return BUTTONS.filter((b) => m & BTN[b]).join('+') || 'none'
}

export interface BattleSpec {
  fighter: string
  form?: number
  level: number
  /** enemy id ('t18', 't26+~t29') - resolved by the server from names */
  enemy?: string
  count?: number
  phases?: { enemy: string; count: number }[]
  arena: string
  seed: number
  skillLevel: number
  statSpend: string
  growth: string
  equipment?: Partial<Record<'body' | 'hands' | 'feet' | 'accessory', number>>
  holyWater?: boolean
}

export function toConfig(s: BattleSpec): BattleConfig {
  const eq = s.equipment ?? {}
  return {
    game: 'buu',
    fighter: {
      fighter: s.fighter,
      form: s.form ?? -1,
      level: s.level,
      bonus: {},
      options: {
        growth: s.growth,
        autoSpend: s.statSpend,
        skillLevel: String(s.skillLevel),
        eq_body: String(eq.body ?? 0),
        eq_hands: String(eq.hands ?? 44),
        eq_feet: String(eq.feet ?? 77),
        eq_accessory: String(eq.accessory ?? 112),
        holyWater: !!s.holyWater,
      },
    },
    phases: s.phases ?? [{ enemy: s.enemy ?? 't68', count: s.count ?? 1 }],
    arena: s.arena,
    seed: s.seed,
    options: { popups: true },
  }
}

export interface GameEvent {
  f: number
  text: string
}

export interface StepDelta {
  hpLost: number
  dealt: number
  kills: number
  hitsTaken: number
  hitsDealt: number
}

export interface Replay {
  format: 'buu-agent-replay/1'
  spec: BattleSpec
  /** run-length encoded held masks: [mask, frames][] */
  inputs: [number, number][]
  frames: number
  hash: string
}

export function rle(a: ArrayLike<number>): [number, number][] {
  const out: [number, number][] = []
  for (let i = 0; i < a.length; i++) {
    const last = out[out.length - 1]
    if (last && last[0] === a[i]) last[1]++
    else out.push([a[i], 1])
  }
  return out
}
export function unrle(r: [number, number][]): number[] {
  const out: number[] = []
  for (const [m, n] of r) for (let i = 0; i < n; i++) out.push(m)
  return out
}

/** FNV-1a over a string */
function fnv(s: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i)
    h = Math.imul(h, 0x01000193) >>> 0
  }
  return h.toString(16).padStart(8, '0')
}

export class Session {
  readonly id: string
  readonly spec: BattleSpec
  readonly cfg: BattleConfig
  w!: World
  battle!: RulesetBattle
  player!: Player
  private prev: Mask = 0
  inputs: number[] = []
  events: GameEvent[] = []
  sfx = new Map<string, number>()
  snapshots = new Map<string, number[]>()
  /** positions of hazards on the previous frame (velocity) */
  prevPos = new Map<number, [number, number]>()
  private lastLog: unknown = null
  private recording = true
  /** frame of the last observe() (events are "since last observe") */
  lastObserved = 0
  createdAt = Date.now()

  constructor(id: string, spec: BattleSpec) {
    this.id = id
    this.spec = spec
    this.cfg = toConfig(spec)
    this.build()
  }

  private build() {
    buuData()
    const w = new World(BuuRng.fromSeed(this.cfg.seed), buuRuleset.arenaRuntime(this.cfg.arena))
    this.battle = buuRuleset.createBattle(this.cfg, w)
    this.w = w
    this.player = w.entities.find((e) => e.kind === 'player') as Player
    this.prev = 0
    this.inputs = []
    this.lastLog = w.log[w.log.length - 1] ?? null
    this.prevPos.clear()
  }

  get frame() {
    return this.w.frame
  }
  outcome() {
    return this.battle.outcome(this.w)
  }
  enemies(): Enemy[] {
    return this.w.entities.filter((e) => e.kind === 'enemy') as Enemy[]
  }

  private sink = (type: AudioEventType, data?: AudioEventData) => {
    if (!this.recording) return
    let name: string
    // only the game's own PlaySfx calls (the coarse bus events duplicate them)
    if (type === 'sfx') name = sfxName(data?.id ?? -1)
    else return
    this.sfx.set(name, (this.sfx.get(name) ?? 0) + 1)
  }

  /** advance one frame with `held` buttons */
  step(held: Mask): StepDelta {
    const w = this.w
    const st = w.stats
    const before = { hp: this.player.hp, dealt: st.damageDealt, kills: st.kills, ht: st.hitsTaken, hd: st.hitsDealt }
    if (this.recording) for (const e of w.entities) if ((e as { rect?: unknown }).rect && e.kind !== 'player' && e.kind !== 'enemy') this.prevPos.set(e.id, [(e as unknown as { px: number }).px, (e as unknown as { py: number }).py])
    const inp = { held, pressed: held & ~this.prev, released: this.prev & ~held }
    setAudioSink(this.sink)
    try {
      this.battle.preStep?.(w, inp)
      w.step(inp)
      this.battle.postStep?.(w)
    } finally {
      setAudioSink(null)
    }
    this.prev = held
    this.inputs.push(held)
    if (this.recording) this.collectLog()
    return { hpLost: Math.max(0, before.hp - this.player.hp), dealt: st.damageDealt - before.dealt, kills: st.kills - before.kills, hitsTaken: st.hitsTaken - before.ht, hitsDealt: st.hitsDealt - before.hd }
  }

  private collectLog() {
    const log = this.w.log
    let i = log.length - 1
    if (i < 0 || log[i] === this.lastLog) return
    while (i >= 0 && log[i] !== this.lastLog) i--
    for (let k = i + 1; k < log.length; k++) {
      const e = log[k]
      if (e.kind === 'ai' || e.kind === 'state') continue
      this.events.push({ f: e.frame, text: this.withIds(tidy(e.text)) })
    }
    if (this.events.length > 300) this.events.splice(0, this.events.length - 300)
    this.lastLog = log[log.length - 1]
  }

  /** the log names enemies by label only: add the id (enemy with that label and the logged HP, else the nearest) */
  private withIds(t: string): string {
    const m = t.match(/^you hit (.+?) with .* \(its hp (-?\d+)\)$/)
    const m2 = t.match(/^(.+?) hit you: /)
    const label = m?.[1] ?? m2?.[1]
    if (!label) return t
    const cands = this.enemies().filter((e) => e.label === label)
    if (!cands.length) return t
    let e = m ? cands.find((x) => x.hp === Math.max(0, Number(m[2]))) : undefined
    if (!e) {
      const [px, py] = [this.player.px, this.player.py]
      e = cands.sort((a, b) => Math.hypot(a.px - px, a.py - py) - Math.hypot(b.px - px, b.py - py))[0]
    }
    return m ? t.replace(`you hit ${label}`, `you hit #${e.id} ${label}`) : t.replace(`${label} hit you`, `#${e.id} ${label} hit you`)
  }

  /** take the events / sfx counts since the last call */
  drainEvents(max = 40): { events: string[]; sfx: string[]; dropped: number } {
    const ev = this.events
    this.events = []
    // long runs of your own hits collapse into one line per target (the totals are in the action result)
    const mine = ev.filter((e) => e.text.startsWith('you hit '))
    let list = ev
    if (mine.length > 6) {
      const by = new Map<string, { n: number; dmg: number; crit: number; f0: number; f1: number; hp: string }>()
      for (const e of mine) {
        const m = e.text.match(/^you hit (.+?) with (.+?) for ([\d.]+)( CRIT)? \(its hp (-?\d+)\)/)
        const k = m ? `${m[1]} with ${m[2]}` : e.text
        const g = by.get(k) ?? { n: 0, dmg: 0, crit: 0, f0: e.f, f1: e.f, hp: '' }
        g.n++
        g.dmg += m ? Number(m[3]) : 0
        if (m?.[4]) g.crit++
        g.f1 = e.f
        g.hp = m?.[5] ?? ''
        by.set(k, g)
      }
      list = [...ev.filter((e) => !e.text.startsWith('you hit ')), ...[...by.entries()].map(([k, g]) => ({ f: g.f1, text: `you hit ${k} ${g.n}x (f${g.f0}-f${g.f1}), total ${Math.round(g.dmg)}${g.crit ? `, ${g.crit} crits` : ''}, its hp now ${g.hp}` }))].sort((a, b) => a.f - b.f)
    }
    const dropped = Math.max(0, list.length - max)
    const out = list.slice(-max).map((e) => `f${e.f} ${e.text}`)
    const sfx = [...this.sfx.entries()].map(([k, n]) => (n > 1 ? `${k} x${n}` : k))
    this.sfx.clear()
    return { events: out, sfx, dropped }
  }

  /** rebuild and replay `inputs` (no event collection) */
  resimulate(inputs: number[]) {
    this.build()
    this.recording = false
    try {
      for (const m of inputs) this.step(m)
    } finally {
      this.recording = true
    }
    this.lastLog = this.w.log[this.w.log.length - 1] ?? null
    this.events = []
    this.sfx.clear()
    this.lastObserved = this.w.frame
  }

  save(name: string) {
    this.snapshots.set(name, this.inputs.slice())
    if (this.snapshots.size > 64) this.snapshots.delete(this.snapshots.keys().next().value!)
  }
  load(name: string) {
    const s = this.snapshots.get(name)
    if (!s) throw new Error(`no snapshot "${name}" (have: ${[...this.snapshots.keys()].join(', ') || 'none'})`)
    this.resimulate(s)
  }
  rewind(frames: number) {
    const n = Math.max(0, this.inputs.length - Math.max(0, frames | 0))
    this.resimulate(this.inputs.slice(0, n))
  }

  /** hash of the simulation state that matters (frame, RNG, every actor's position / HP / state) */
  hash(): string {
    const w = this.w
    const p = this.player
    const parts: unknown[] = [w.frame, w.rng.getState(), w.rng.calls, p.x, p.y, p.hp, p.ep, p.facing, p.state, w.stats.damageDealt, w.stats.damageTaken]
    for (const e of this.enemies()) parts.push(e.id, e.type.type, e.x, e.y, e.hp1024, e.facing, e.defeated, e.ai.describe())
    for (const e of w.entities) if (e.kind !== 'enemy' && e.kind !== 'player') parts.push(e.kind, (e as unknown as { x?: number }).x ?? 0, (e as unknown as { y?: number }).y ?? 0)
    return fnv(JSON.stringify(parts))
  }

  replay(): Replay {
    return { format: 'buu-agent-replay/1', spec: this.spec, inputs: rle(this.inputs), frames: this.inputs.length, hash: this.hash() }
  }
}

function tidy(t: string): string {
  // "player 2x1024 -> Majin Fighter: 15360/1024 CRIT hp 485" -> "you hit Majin Fighter for 15 (punch) CRIT, hp 485"
  const m = t.match(/^player (\d+)x(\d+) -> (.+?): (\d+)\/1024( CRIT)? hp (-?\d+)$/)
  if (m) return `you hit ${m[3]} with ${ATK_NAMES[Number(m[1])] ?? 'attack ' + m[1]} for ${(Number(m[4]) / 1024).toFixed(1)}${m[5] ? ' CRIT' : ''} (its hp ${m[6]})`
  const m2 = t.match(/^(.+?) -> player: (\d+)\/1024( CRIT)?( blocked)?( \(i-frames\))? = (\d+) HP$/)
  if (m2) return `${m2[1]} hit you: -${m2[6]} HP${m2[3] ? ' CRIT' : ''}${m2[4] ? ' (blocked)' : ''}${m2[5] ? ' (during hurt i-frames)' : ''}`
  const m3 = t.match(/^(\w+) (\d+)x(\d+) -> (.+?): (\d+)\/1024( CRIT)? hp (-?\d+)$/)
  if (m3) return `${m3[1]} hit ${m3[4]} for ${(Number(m3[5]) / 1024).toFixed(1)}${m3[6] ? ' CRIT' : ''} (hp ${m3[7]})`
  return t
}
