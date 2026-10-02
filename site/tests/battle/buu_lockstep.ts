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
import { ATK } from '../../src/battle/rulesets/buu/damage'
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
    obj: { walk: number; dash: number; stats: number[]; regen?: number; ssacc?: number; cooldown?: number; idlecnt?: number; camera?: [number, number]; beepAge?: number }
  }
  states: string[]
  /** [n, x, y, dx, dy, dir, raw, frame, state, hp, ep, form, rom] */
  rows: number[][]
  /** [frame, kind, id?, vol?] */
  sfx: (string | number)[][]
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
    for (let i = 0; i < n; i++) out.push({ x: x + dx * i, y: y + dy * i, dir, raw, frame, state: t.states[st], hp, ep, form, rom, rng, sfx: [] })
  }
  for (const s of t.sfx) {
    const r = out[(s[0] as number) - 1]
    if (r) r.sfx.push(sfxKey(s.slice(1)))
  }
  return out
}

const FIGHTER_OF_CHAR = ['goku', 'gohan', 'goten', 'trunks', 'vegeta', 'gotenks', 'vegito', 'gogeta']

/** build the simulator at the trace's frame 0 */
export function setupSim(t: LsTrace): { w: World; player: Player; battle: RulesetBattle } {
  const d = buuData()
  const rs = buuRuleset
  const ini = t.init
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
    phases: [{ enemy: 't-1', count: 1 }],
    arena: (t.scenario.arena as string | undefined) ?? 'htc',
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
  p.lockstepInit(ini.raw, ini.frame, ini.tick)
  // the form controller's accumulators (player+0x22C object: +0xC EP regen, +0x14 SS / SS3 drain)
  if (ini.obj.regen != null) p.regenTimer = ini.obj.regen & 0xffff
  if (ini.obj.ssacc != null && (p.form.ss || p.form.ss3)) p.ssAcc = ini.obj.ssacc & 0xffff
  if (ini.obj.cooldown != null) p.ssCooldown = ini.obj.cooldown
  if (ini.obj.idlecnt != null) p.sCnt = ini.obj.idlecnt
  if (ini.obj.camera) p.ctx.setCamera(ini.obj.camera[0] | 0, ini.obj.camera[1] | 0)
  if (ini.obj.beepAge != null) p.ctx.lastBeep = -ini.obj.beepAge
  w.rng.setState(ini.rng)
  return { w, player: p, battle }
}

function simRow(w: World, p: Player, sfx: string[]): LsRow {
  const v = p.lockstepView()
  return { x: p.x, y: p.y, dir: p.facing, raw: v.raw, frame: v.frame, state: v.state, hp: p.hp, ep: p.ep, form: p.form.id, rom: v.rom, rng: w.rng.getState()[0] >>> 0, sfx }
}

/** run the trace's inputs in the simulator -> one row per frame */
export function replay(t: LsTrace): LsRow[] {
  const { w, player, battle } = setupSim(t)
  const hits = new Map((t.scenario.hits ?? []).map((h) => [h.at, h]))
  const out: LsRow[] = []
  let sfx: string[] = []
  setAudioSink((type, data) => {
    if (type === 'sfx') sfx.push(sfxKey(['play', data!.id!, data!.vol ?? 64]))
    else if (type === 'sfx_stop') sfx.push(sfxKey(['stop', data!.id!]))
    else if (type === 'sfx_stop_all') sfx.push('stopall')
    else if (type === 'song') sfx.push(sfxKey(['song', data!.id!]))
  })
  // the emulator injects hits at the start of the frame's game update: an entity at the end of the list (the world
  // updates last to first) does the same here, after World.step advanced the frame counter
  const hook = {
    id: 0, kind: 'lockstep', removed: false, layer: 0, depth: () => 0, draw: () => undefined,
    update: (ww: World) => {
      const h = hits.get(ww.frame)
      if (h) injectHit(ww, player, h)
    },
  }
  w.entities.push(hook as unknown as (typeof w.entities)[number])
  try {
    let prev = 0
    for (const [keys, n] of t.input) {
      for (let i = 0; i < n; i++) {
        sfx = []
        const inp = { held: keys, pressed: keys & ~prev, released: prev & ~keys }
        battle.preStep?.(w, inp)
        w.step(inp)
        battle.postStep?.(w)
        prev = keys
        out.push(simRow(w, player, sfx))
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
}

export interface Divergence {
  frame: number
  fields: string[]
  exp: LsRow
  got: LsRow
}

const FIELDS: (keyof LsRow)[] = ['state', 'raw', 'frame', 'dir', 'x', 'y', 'hp', 'ep', 'form', 'rom', 'sfx', 'rng']

export function compare(exp: LsRow[], got: LsRow[], skip: (keyof LsRow)[] = []): Divergence | null {
  for (let i = 0; i < exp.length; i++) {
    const e = exp[i]
    const g = got[i]
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
      const a = k === 'sfx' ? e.sfx.join(' ') : e[k]
      const b = k === 'sfx' ? g.sfx.join(' ') : g[k]
      if (a !== b) bad.push(k)
    }
    if (bad.length) return { frame: i + 1, fields: bad, exp: e, got: g }
  }
  return null
}

const fmt = (r: LsRow) =>
  `${r.state.padEnd(12)} raw ${String(r.raw).padStart(2)} f ${String(r.frame).padStart(2)} dir ${r.dir} pos ${(r.x / 256).toFixed(2)},${(r.y / 256).toFixed(2)} hp ${r.hp} ep ${r.ep} form ${r.form} rom ${r.rom.toString(16)} rng ${(r.rng ?? 0).toString(16).padStart(8, '0')} ${r.sfx.join(' ')}`

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
