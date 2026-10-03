// Lockstep differential harness, simulator side: replays an emulator trace (tools/battle/lockstep/record.py ->
// data/battle/buu_lockstep/fNN.json) in the simulator with the same start state and inputs and compares every frame:
// position, facing, animation (raw index + frame index, and the ROM frame record the sprite data maps it to), the
// player's state, HP / EP, form and every sound call. Format: tools/battle/LOCKSTEP.md.
import { readdirSync, readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { World } from '../../src/battle/core/world'
import { DIR_NAMES, type Dir } from '../../src/battle/core/geom'
import { BuuRng } from '../../src/battle/rulesets/buu/rng'
import { buuRuleset } from '../../src/battle/rulesets/buu'
import { ATK, setDamageObserver } from '../../src/battle/rulesets/buu/damage'
import type { AiNodeJ } from '../../src/battle/rulesets/buu/data'
import type { Enemy } from '../../src/battle/rulesets/buu/enemy'
import { broadcast, type Actor } from '../../src/battle/rulesets/buu/actor'
import { setAudioSink } from '../../src/battle/audio/bus'
import type { BattleConfig, RulesetBattle } from '../../src/battle/core/ruleset'
import type { Player } from '../../src/battle/rulesets/buu/player'
import { buuData, fighter } from './buu_harness'

export const LOCKSTEP_DIR = join(__dirname, '../../../data/battle/buu_lockstep')

export interface LsHit {
  at: number
  dir?: number
  atk?: number
  mult?: number
  str?: number
  pow?: number
}
export interface LsTrace {
  name: string
  scenario: { name: string; form: number; hits?: LsHit[]; [k: string]: unknown }
  input: [number, number][]
  init: {
    x: number
    y: number
    dir: number
    raw: number
    frame: number
    tick: number
    state: string
    rng: [number, number]
    rec: { char: number; form: number; hp: number; maxHp: number; ep: number; maxEp: number; base: number[]; eff: number[]; skills: number[]; slot: number; levels: number[]; ssGauge: number; ss3Gauge: number; equip: number[] }
    obj: { walk: number; dash: number; stats: number[]; regen?: number; ssacc?: number; cooldown?: number; idlecnt?: number; camera?: [number, number]; beepAge?: number; hittable?: number; ctl10?: number }
    partner?: Player['partner']
    /** enemy scenarios: the enemy at frame 0 (24.8 footprint position, facing, HP << 10, type, footprint) */
    enemy?: { x: number; y: number; dir: number; hp: number; type: number; foot: [number, number]; raw?: number; frame?: number; tick?: number; mark1Age?: number; mark2Age?: number }
    enemy2?: { x: number; y: number; dir: number; hp: number; type: number; raw: number; frame?: number; tick?: number; mark1Age?: number; mark2Age?: number }
  }
  states: string[]
  /** [n, x, y, dx, dy, dir, raw, frame, state, hp, ep, form, rom] */
  rows: number[][]
  /** [frame, kind, id?, vol?]; kind 'dmg': [frame, 'dmg', target (0 player, 1 the enemy, -1 other), damage 1/1024] */
  sfx: (string | number)[][]
  /** enemy scenarios: [n, x, y, dx, dy, dir, hp] */
  erows?: number[][]
  /** lag frames: the game logic did not run (frame counter 0x03000F94 unchanged) */
  lag?: number[]
}

export interface LsEnemyScn {
  type: number
  at: [number, number]
  hp?: number
  aggro?: boolean
  /** default true: a target dummy (see patchDummy) */
  dummy?: boolean
  /** a real enemy's AI state at frame 1 (default: the type's initial state) */
  state?: number
  /** a second real enemy (its AI starts at frame 2) */
  partner?: { type: number; at: [number, number]; state?: number }
}

export interface LsRow {
  x: number
  y: number
  dir: number
  raw: number
  frame: number
  state: string
  hp: number
  ep: number
  form: number
  rom: number
  /** RNG state word 0 (0x03004A5C): catches a draw too many / too few on the frame it happens */
  rng?: number
  sfx: string[]
  /** ComputeDamage results of the frame: 'target:damage' */
  dmg: string[]
  /** enemy scenarios: the enemy's 24.8 position, facing and HP << 10 */
  ex?: number
  ey?: number
  edir?: number
  ehp?: number
}

export function loadTraces(filter?: string): LsTrace[] {
  if (!existsSync(LOCKSTEP_DIR)) return []
  const out: LsTrace[] = []
  for (const f of readdirSync(LOCKSTEP_DIR).sort()) {
    if (!f.endsWith('.json')) continue
    const j = JSON.parse(readFileSync(join(LOCKSTEP_DIR, f), 'utf8')) as { traces: LsTrace[] }
    for (const t of j.traces) if (!filter || new RegExp(filter).test(t.name)) out.push(t)
  }
  return out
}

const sfxKey = (e: (string | number)[]) => e.join(':')

/** expected rows (index 0 = frame 1) */
export function expand(t: LsTrace): LsRow[] {
  const out: LsRow[] = []
  for (const g of t.rows) {
    const [n, x, y, dx, dy, dir, raw, frame, st, hp, ep, form, rom, rng] = g
    for (let i = 0; i < n; i++) out.push({ x: x + dx * i, y: y + dy * i, dir, raw, frame, state: t.states[st], hp, ep, form, rom, rng, sfx: [], dmg: [] })
  }
  for (const s of t.sfx) {
    const r = out[(s[0] as number) - 1]
    if (!r) continue
    if (s[1] === 'dmg') r.dmg.push(`${s[2]}:${s[3]}`)
    else r.sfx.push(sfxKey(s.slice(1)))
  }
  if (t.erows) {
    let i = 0
    for (const [n, x, y, dx, dy, dir, hp] of t.erows) {
      for (let k = 0; k < n; k++, i++) {
        const r = out[i]
        if (!r) break
        r.ex = x + dx * k
        r.ey = y + dy * k
        r.edir = dir
        r.ehp = hp
      }
    }
  }
  return out
}

const FIGHTER_OF_CHAR = ['goku', 'gohan', 'goten', 'trunks', 'vegeta', 'gotenks', 'vegito', 'gogeta']

/** Majin Fighter state 0x14B's aggro test (IfPlayerWithin2 r2 = 10000 at ROM 0x087043F8 + 0xC): the emulator
 *  scenarios patch it to -1 (never aggro: signed dist2 <= r2); the simulator patches its decoded node the same way while a trace runs */
const MAJIN_AGGRO_NODE = '0x87043f8'
function findNode(n: AiNodeJ, addr: string): AiNodeJ | null {
  if (n.addr === addr) return n
  for (const c of n.children ?? []) {
    const r = findNode(c, addr)
    if (r) return r
  }
  return null
}
function patchAggro(on: boolean): () => void {
  const d = buuData()
  const nodes: AiNodeJ[] = []
  for (const root of d.states.values()) {
    const n = findNode(root, MAJIN_AGGRO_NODE)
    if (n && !nodes.includes(n)) nodes.push(n)
  }
  const old = nodes.map((n) => n.params!.r2.raw)
  if (!on) for (const n of nodes) n.params!.r2.raw = -1
  return () => nodes.forEach((n, i) => (n.params!.r2.raw = old[i]))
}
/** target dummy (emulator: state table 0x0870F514 entry 0x14B's aux pointer -> the ApplyDamage node 0x086FDA00): the
 *  Majin Fighter's idle state takes hits as HP loss only (no reaction, knockback or AI change) */
const APPLY_DAMAGE_NODE = '0x86fda00'
function patchDummy(on: boolean): () => void {
  const d = buuData()
  const old = d.aux.get(0x14b)
  if (on) {
    let node: AiNodeJ | null = null
    for (const root of d.aux.values()) node ??= findNode(root, APPLY_DAMAGE_NODE)
    d.aux.set(0x14b, node!)
  }
  return () => d.aux.set(0x14b, old!)
}
/** the scenario's ROM patches applied to the simulator's decoded AI (restore with the returned function) */
export function patchScenario(t: LsTrace): () => void {
  const en = t.scenario.enemy as LsEnemyScn | undefined
  const a = patchAggro(!en || !!en.aggro)
  const b = patchDummy(!!en && en.dummy !== false)
  return () => {
    b()
    a()
  }
}

/** build the simulator at the trace's frame 0 */
export function setupSim(t: LsTrace): { w: World; player: Player; battle: RulesetBattle; enemy: Enemy | null } {
  const d = buuData()
  const rs = buuRuleset
  const ini = t.init
  const en = t.scenario.enemy as LsEnemyScn | undefined
  // the record's form at frame 0 (an SS3 form with a small gauge may already have reverted while settling)
  const form = ini.rec.form ?? t.scenario.form
  const char = d.forms[t.scenario.form].char
  const eq = ini.rec.equip
  const cfg: BattleConfig = {
    game: 'buu',
    fighter: fighter({
      fighter: char === 8 ? 'f' + form : FIGHTER_OF_CHAR[char],
      form,
      options: { growth: 'average', autoSpend: 'none', skillLevel: String(ini.rec.levels[0] ?? 0), eq_body: String(eq[0]), eq_hands: String(eq[1]), eq_feet: String(eq[2]), eq_accessory: String(eq[3]) },
    }),
    phases: [{ enemy: en ? `t${ini.enemy?.type ?? en.type}` + (ini.enemy2 ? `+t${ini.enemy2.type}` : '') : 't-1', count: 1 }],
    arena: (t.scenario.arena as string | undefined) ?? (en ? 'forest' : 'htc'),
    seed: 1,
    options: { popups: false },
  }
  const w = new World(new BuuRng(0, 0), rs.arenaRuntime(cfg.arena))
  const battle = rs.createBattle(cfg, w)
  const p = w.entities.find((e) => e.kind === 'player') as Player
  const r = p.rec
  r.hp = ini.rec.hp
  r.maxHp = ini.rec.maxHp
  r.ep = ini.rec.ep
  r.maxEp = ini.rec.maxEp
  ;[r.STR, r.POW, r.END, r.SPD] = ini.rec.base
  p.skills = ini.rec.skills.length ? ini.rec.skills : [7]
  p.slot = ini.rec.slot
  p.skillLevel = ini.rec.levels[0] ?? 0
  p.ssGauge = ini.rec.ssGauge
  p.ss3Gauge = ini.rec.ss3Gauge * 65536
  p.applyForm(form)
  p.x = ini.x
  p.y = ini.y
  p.facing = ini.dir as Dir
  p.lockstepInit(ini.raw, ini.frame, ini.tick, ini.state)
  // the story Trunks form objects' countdown (+0xC), the cutscene fusion's (+0x10) and its partner's record
  if (p.storyForm() && ini.obj.regen != null) p.storyCnt = ini.obj.regen | 0
  if (p.form.controller === 0x3001b64 && ini.obj.ctl10 != null) p.fusCnt = ini.obj.ctl10 | 0
  if (ini.partner) p.partner = ini.partner
  if (ini.obj.hittable != null) p.hittable = !!ini.obj.hittable
  // the form controller's accumulators (player+0x22C object: +0xC EP regen, +0x14 SS / SS3 drain)
  if (ini.obj.regen != null) p.regenTimer = ini.obj.regen & 0xffff
  if (ini.obj.ssacc != null && (p.form.ss || p.form.ss3)) p.ssAcc = ini.obj.ssacc & 0xffff
  if (ini.obj.cooldown != null) p.ssCooldown = ini.obj.cooldown
  if (ini.obj.idlecnt != null) p.sCnt = ini.obj.idlecnt
  if (ini.obj.camera) p.ctx.setCamera(ini.obj.camera[0] | 0, ini.obj.camera[1] | 0)
  if (ini.obj.beepAge != null) p.ctx.lastBeep = -ini.obj.beepAge
  // the frame counter 0x03000F94 at frame 0: every f02_vs_enemy_tXX recording starts from the same setup, counter
  // 9634 (read from the emulator after emu_lockstep.setup(); not stored in the traces yet -> record it as init.fc)
  p.ctx.fc0 = (ini as { fc?: number }).fc ?? (ini.enemy ? 9634 : 0)
  let enemy: Enemy | null = null
  if (en && ini.enemy) {
    // the recorded enemy: the fight's own, not a part it spawned (Worm Head's segments come before it in the list)
    enemy = (w.entities.find((e) => e.kind === 'enemy' && !(e as Enemy).parentE) ?? w.entities.find((e) => e.kind === 'enemy')) as Enemy
    enemy.unmarkSelf()
    enemy.x = ini.enemy.x
    enemy.y = ini.enemy.y
    enemy.markSelf()
    enemy.facing = ini.enemy.dir as Dir
    enemy.hp1024 = ini.enemy.hp
    // its animation state and creation stamps (obj+0x1E8 / +0x1EC) at frame 0
    const ie = ini.enemy
    if (ie.raw != null && ie.raw > 0) {
      const a = enemy.sprite?.anim(ie.raw)
      if (a) enemy.anim = a
      enemy.ea.pos = { raw: ie.raw, frame: ie.frame ?? 0, tick: ie.tick ?? 0 }
      enemy.frameIdx = ie.frame ?? 0
    }
    if (ie.mark1Age != null && ie.mark1Age < 1e6) enemy.mark1 = -ie.mark1Age
    if (ie.mark2Age != null && ie.mark2Age < 1e6) enemy.mark2 = -ie.mark2Age
    const i2 = ini.enemy2
    if (i2) {
      const e2 = w.entities.find((x) => x.kind === 'enemy' && x !== enemy && (x as Enemy).type.type === i2.type) as Enemy
      e2.unmarkSelf()
      e2.x = i2.x
      e2.y = i2.y
      e2.markSelf()
      e2.facing = i2.dir as Dir
      e2.hp1024 = i2.hp
      // [H] the emulator keeps the partner in the inert state 0xC (root: the Idle node) until its AI start at frame 2
      if ((t.scenario.enemy as LsEnemyScn | undefined)?.partner) e2.ai.reset(0xc)
      const a2 = i2.raw > 0 ? e2.sprite?.anim(i2.raw) : undefined
      if (a2) {
        e2.anim = a2
        e2.ea.pos = { raw: i2.raw, frame: i2.frame ?? 0, tick: i2.tick ?? 0 }
        e2.frameIdx = i2.frame ?? 0
      }
      if (i2.mark1Age != null && i2.mark1Age < 1e6) e2.mark1 = -i2.mark1Age
      if (i2.mark2Age != null && i2.mark2Age < 1e6) e2.mark2 = -i2.mark2Age
    }
  }
  w.rng.setState(ini.rng)
  return { w, player: p, battle, enemy }
}

function simRow(w: World, p: Player, e: Enemy | null, sfx: string[], dmg: string[]): LsRow {
  const v = p.lockstepView()
  // [H] the view may override the position / facing (the eaten player: the recorder reports 0)
  const r: LsRow = { x: v.x ?? p.x, y: v.y ?? p.y, dir: v.dir ?? p.facing, raw: v.raw, frame: v.frame, state: v.state, hp: p.hp, ep: p.ep, form: p.form.id, rom: v.rom, rng: w.rng.getState()[0] >>> 0, sfx, dmg }
  if (e) {
    r.ex = e.x
    r.ey = e.y
    r.edir = e.facing
    r.ehp = e.hp1024
  }
  return r
}

/** run the trace's inputs in the simulator -> one row per frame */
export function replay(t: LsTrace, each?: FrameHook): LsRow[] {
  const restore = patchScenario(t)
  try {
    return replayIn(t, each)
  } finally {
    restore()
    setDamageObserver(null)
  }
}

/** called after every replayed frame (debugging) */
export type FrameHook = (f: number, sim: { w: World; player: Player; enemy: Enemy | null; lag: boolean }) => void

function replayIn(t: LsTrace, each?: FrameHook): LsRow[] {
  const { w, player, battle, enemy } = setupSim(t)
  const hits = new Map((t.scenario.hits ?? []).map((h) => [h.at, h]))
  const out: LsRow[] = []
  let sfx: string[] = []
  let dmg: string[] = []
  setDamageObserver((def, res) => {
    dmg.push(`${def === player.stats ? 0 : enemy && def === enemy.stats ? 1 : -1}:${res.dmg}`)
  })
  setAudioSink((type, data) => {
    if (type === 'sfx') sfx.push(sfxKey(['play', data!.id!, data!.vol ?? 64]))
    else if (type === 'sfx_stop') sfx.push(sfxKey(['stop', data!.id!]))
    else if (type === 'sfx_stop_all') sfx.push('stopall')
    else if (type === 'sfx_stop_match') sfx.push(sfxKey(['stopmatch', data!.id!]))
    else if (type === 'song') sfx.push(sfxKey(['song', data!.id!]))
  })
  // the emulator injects hits at the start of the frame's game update: an entity at the end of the list (the world
  // updates last to first) does the same here, after World.step advanced the frame counter
  // lag frames (the emulator's game logic did not run): no step, the input is not read
  const lag = new Set(t.lag ?? [])
  let f = 0
  const hook = {
    id: 0, kind: 'lockstep', removed: false, layer: 0, depth: () => 0, draw: () => undefined,
    update: (ww: World) => {
      const h = hits.get(f)
      if (h) injectHit(ww, player, h)
      // a real enemy starts its AI fresh at frame 1 (the emulator's injected GotoState(initial state))
      else if (f === 1 && enemy && (t.scenario.enemy as LsEnemyScn).dummy === false) enemy.ai.replaceTop((t.scenario.enemy as LsEnemyScn).state ?? enemy.type.ai)
      else if (f === 2 && enemy && (t.scenario.enemy as LsEnemyScn).partner) {
        const pt = (t.scenario.enemy as LsEnemyScn).partner!
        const e2 = ww.entities.find((x) => x.kind === 'enemy' && x !== enemy && (x as Enemy).type.type === pt.type) as Enemy | undefined
        e2?.ai.replaceTop(pt.state ?? e2.type.ai)
      }
    },
  }
  w.entities.push(hook as unknown as (typeof w.entities)[number])
  try {
    let prev = 0
    for (const [keys, n] of t.input) {
      for (let i = 0; i < n; i++) {
        f++
        sfx = []
        dmg = []
        if (lag.has(f)) {
          out.push(simRow(w, player, enemy, sfx, dmg))
          each?.(f, { w, player, enemy, lag: true })
          continue
        }
        const inp = { held: keys, pressed: keys & ~prev, released: prev & ~keys }
        battle.preStep?.(w, inp)
        w.step(inp)
        battle.postStep?.(w)
        prev = keys
        out.push(simRow(w, player, enemy, sfx, dmg))
        each?.(f, { w, player, enemy, lag: false })
      }
    }
  } finally {
    setAudioSink(null)
  }
  return out
}

/** the emulator's injected attack message (emu_lockstep.write_hit): box (px-32, py-64)..(px+48, py+32) */
function injectHit(w: World, p: Player, h: LsHit) {
  const px = p.px
  const py = p.py
  const attacker = {
    kind: 'lockstep',
    label: 'injected hit',
    stats: { STR: h.str ?? 30, POW: h.pow ?? 30, END: 0, END2: 0, resM: 128, resE: 128, critBonus: 0, blockFactor: 0x1000 },
  } as unknown as Actor
  broadcast(w, { team: 0, rect: { x0: px - 32, y0: py - 64, x1: px + 48, y1: py + 32 }, dir: (h.dir ?? 2) as Dir, mult: h.mult ?? 0x400, atk: ATK[h.atk ?? 2], attacker })
  // the emulator injects before the frame's script phase: the death script counts from this frame (player.koAt)
  if (p.dead && p.koAt === w.frame + 1) p.koAt = w.frame
}

export interface Divergence {
  frame: number
  fields: string[]
  exp: LsRow
  got: LsRow
}

const FIELDS: (keyof LsRow)[] = ['state', 'raw', 'frame', 'dir', 'x', 'y', 'hp', 'ep', 'form', 'rom', 'sfx', 'dmg', 'ex', 'ey', 'edir', 'ehp', 'rng']

export function compare(exp: LsRow[], got: LsRow[], skip: (keyof LsRow)[] = []): Divergence | null {
  for (let i = 0; i < exp.length; i++) {
    const e = exp[i]
    const g = got[i]
    // game over: KO + 440 the game leaves the battle (the player object is freed: vtable 0, no frame record) - the
    // rest of the recording is the game-over screen, nothing to compare (f02_vs_enemy_t5e / t13)
    if (e.state === 'vt_00000000' && e.rom === 0 && e.hp === 0 && i > 0 && exp[i - 1].hp === 0) break
    if (!g) return { frame: i + 1, fields: ['missing'], exp: e, got: e }
    const bad: string[] = []
    // the sprite has no such animation (the game reads a null / foreign entry and shows nothing sensible): the
    // simulator reports raw -1; frame / ROM record are not comparable
    // (the emulator then finds no def slot (raw -1 / 0) or no valid frame record (rom 0)). When the def was swapped
    // under a running animation (the KO death script), the struct is still shown: the ROM frame record decides.
    const noAnim = g.raw === -1 && (e.raw <= 0 || e.rom === 0)
    const swapped = e.raw === -1 && e.rom !== 0 && e.rom === g.rom
    for (const k of FIELDS) {
      if (skip.includes(k)) continue
      if (noAnim && (k === 'raw' || k === 'frame' || k === 'rom')) continue
      if (swapped && k === 'raw') continue
      if (k === 'rng' && e.rng == null) continue
      if ((k === 'ex' || k === 'ey' || k === 'edir' || k === 'ehp' || k === 'dmg') && e.ex == null) continue
      const a = k === 'sfx' ? e.sfx.join(' ') : k === 'dmg' ? [...e.dmg].sort().join(' ') : e[k]
      const b = k === 'sfx' ? g.sfx.join(' ') : k === 'dmg' ? [...g.dmg].sort().join(' ') : g[k]
      if (a !== b) bad.push(k)
    }
    if (bad.length) return { frame: i + 1, fields: bad, exp: e, got: g }
  }
  return null
}

const fmt = (r: LsRow) =>
  `${r.state.padEnd(12)} raw ${String(r.raw).padStart(2)} f ${String(r.frame).padStart(2)} dir ${r.dir} pos ${(r.x / 256).toFixed(2)},${(r.y / 256).toFixed(2)} hp ${r.hp} ep ${r.ep} form ${r.form} rom ${r.rom.toString(16)} rng ${(r.rng ?? 0).toString(16).padStart(8, '0')} ${r.sfx.join(' ')}` +
  (r.ex != null ? ` | E ${(r.ex / 256).toFixed(2)},${(r.ey! / 256).toFixed(2)} d${r.edir} hp ${(r.ehp! / 1024).toFixed(2)}` : '') +
  (r.dmg.length ? ` dmg ${r.dmg.join(' ')}` : '')

/** human-readable report of the first divergence with `ctx` frames before it */
export function report(t: LsTrace, exp: LsRow[], got: LsRow[], d: Divergence, ctx = 6): string {
  const lines = [`${t.name}: first divergence at frame ${d.frame} (${d.fields.join(', ')}); input: ${inputAt(t, d.frame)}`]
  for (let i = Math.max(0, d.frame - 1 - ctx); i < Math.min(exp.length, d.frame + 3); i++) {
    const mark = i === d.frame - 1 ? '>>' : '  '
    lines.push(`${mark}${String(i + 1).padStart(5)} emu ${fmt(exp[i])}`)
    if (got[i]) lines.push(`${mark}${' '.repeat(5)} sim ${fmt(got[i])}`)
  }
  return lines.join('\n')
}

function inputAt(t: LsTrace, frame: number): string {
  let f = 0
  for (const [k, n] of t.input) {
    if (frame <= f + n) return `keys 0x${k.toString(16)} (segment frames ${f + 1}..${f + n})`
    f += n
  }
  return '-'
}

/** ROM frame record of a sprites.json step (for the 'rom' column) */
export function stepRom(p: Player, raw: number, frame: number, dir: number): number {
  const a = p.sprite?.json.animations && Object.values(p.sprite.json.animations).find((x) => x.raw_index === raw)
  const steps = (a?.directions[DIR_NAMES[dir]] ?? a?.directions.any) as { rom?: string }[] | undefined
  const s = steps?.[frame]
  return s?.rom ? parseInt(s.rom, 16) : 0
}
