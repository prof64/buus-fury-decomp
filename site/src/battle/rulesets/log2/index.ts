// Legacy of Goku II ruleset: roster, enemy catalog, arenas, presets and battle setup on top of the core engine.

import { loadAll } from '../../core/assets'
import type {
  ArenaChoice,
  BattleConfig,
  ControlHelp,
  EnemyChoice,
  FighterChoice,
  FighterSetup,
  PresetChoice,
  Ruleset,
  RulesetBattle,
  SetupKnob,
  StatLine,
} from '../../core/ruleset'
import type { ArenaRuntime, World } from '../../core/world'
import { Log2Ctx } from './context'
import { loadLog2, type Log2Data } from './data'
import { Enemy } from './enemy'
import { drawHud } from './hud'
import { Player } from './player'
import { recordAtLevel, STAT_CAP, type GrowthMode, type Record8 } from './progression'
import { Log2Rng } from './rng'

const ARENAS: (ArenaChoice & { map: number })[] = [
  { id: 'cellgames', map: 324, name: 'The Cell Games Arena', image: '', size: [0, 0], bounds: { x0: 256, y0: 320, x1: 544, y1: 616 } },
  { id: 'sparring', map: 175, name: 'Sparring Arena (Kami’s Lookout)', image: '', size: [0, 0], bounds: { x0: 40, y0: 84, x1: 240, y1: 250 } },
  { id: 'namek', map: 172, name: 'New Namek', image: '', size: [0, 0], bounds: { x0: 80, y0: 168, x1: 360, y1: 376 } },
  { id: 'gingertown', map: 287, name: 'Gingertown plaza', image: '', size: [0, 0], bounds: { x0: 88, y0: 64, x1: 384, y1: 256 } },
  { id: 'desert', map: 195, name: 'Southern Continent', image: '', size: [0, 0], bounds: { x0: 112, y0: 152, x1: 420, y1: 396 } },
  { id: 'islands', map: 304, name: 'Tropical Islands', image: '', size: [0, 0], bounds: { x0: 96, y0: 200, x1: 416, y1: 380 } },
  { id: 'wilds', map: 13, name: 'East District 439 (wilderness)', image: '', size: [0, 0], bounds: { x0: 528, y0: 528, x1: 1080, y1: 1032 } },
]
const ARENA_FOR_MAP: Record<number, string> = { 324: 'cellgames', 325: 'cellgames', 175: 'sparring', 172: 'namek', 287: 'gingertown', 191: 'desert', 195: 'desert', 302: 'islands', 304: 'islands', 105: 'wilds', 46: 'gingertown', 14: 'gingertown', 155: 'gingertown', 215: 'desert', 229: 'desert', 242: 'desert', 251: 'desert', 296: 'islands', 297: 'islands', 298: 'islands' }

// classes with no graphics in the ROM tables (unused rows): placeholder sprite index
const PLACEHOLDER_GFX: Record<number, number> = { 0: 60, 13: 141, 15: 112, 43: 48, 46: 141, 49: 123, 52: 141, 82: 141, 87: 141, 93: 112, 95: 118 }

const CHAR_IDS = ['gohan', 'piccolo', 'vegeta', 'trunks', 'goku', 'hercule']
const DEFAULT_FORM: Record<string, number> = { gohan: 2, piccolo: 6, vegeta: 8, trunks: 12, goku: 19, hercule: 21 }

const STAT_KEYS = ['STR', 'POW', 'END'] as const

function aiFaithful(d: Log2Data, aiState: number): boolean {
  return aiState !== 0 && !!d.ai[aiState]
}

export class Log2Ruleset implements Ruleset {
  info = { id: 'log2' as const, title: 'The Legacy of Goku II', short: 'LoG II', enabled: true }
  levelRange: [number, number] = [1, 50]
  d!: Log2Data

  async load() {
    if (this.d) return
    this.d = await loadLog2()
    for (const a of ARENAS) {
      const m = this.d.maps.get(a.map)
      if (m) {
        a.image = m.composite
        a.size = m.size
      }
    }
    const sheets = [...this.d.sprites.byId.values()].filter((s) => s.json.category === 'player' || s.id.startsWith('fx_')).map((s) => s.json.sheet)
    await loadAll([...sheets, 'assets/log2/ui/gameplay/hud.png', ...this.d.moves.flatMap((m) => [m.icon, m.iconDim])])
  }

  private portrait(idx: number) {
    const id = this.d.portraits.get(idx)
    return id ? this.d.sprites.get(id)?.json.sheet : undefined
  }

  /** scouter name, or the sprite's name for rows without a scouter entry */
  displayName(id: number): string {
    const c = this.d.classes[id]
    if (c.names.length) return c.name
    const spr = this.d.sprites.index(c.hasGraphics ? c.gfx[0] : (PLACEHOLDER_GFX[c.id] ?? 60))
    return `${spr?.json.name ?? 'Unknown'} (row ${id})`
  }

  private charId(i: number) {
    return CHAR_IDS[i]
  }

  fighters(): FighterChoice[] {
    const d = this.d
    return d.chars.map((c) => {
      const id = this.charId(c.index)
      const forms = c.forms.map((fid) => {
        const f = d.forms[fid]
        const spr = d.sprites.index(f.actorId)
        const ctl = d.controllers[f.controller]
        const bonus = ctl.override ? `STR/POW/END = ${ctl.override.join('/')}` : ctl.bonus && ctl.bonus.some(Boolean) ? `+${ctl.bonus.join('/+')} STR/POW/END` : 'base stats'
        return {
          id: fid,
          name: f.label,
          visual: { sprite: spr?.id, anim: 'idle', dir: 0 },
          portrait: this.portrait(f.portraitIndex),
          note: `${ctl.name}: ${bonus}, walk ${ctl.walk / 256} / dash ${ctl.run / 256} px/f${f.transformed ? ' (transformed)' : ''}`,
        }
      })
      const df = DEFAULT_FORM[id]
      const moves = [
        { button: 'A', name: 'Punch' },
        ...(d.forms[df].smMask ? [{ button: 'hold A', name: c.specialMelee || 'Special melee' }] : []),
        ...c.slots.map((t, i) => ({ button: `B (slot ${i + 1})`, name: c.slotNames[i] ?? d.moves[t].name, note: `${d.moves[t].minEp} EP to start` })),
      ]
      return {
        id,
        name: c.name,
        portrait: this.portrait(d.forms[df].portraitIndex),
        visual: { sprite: d.sprites.index(d.forms[df].actorId)?.id, anim: 'idle', dir: 0 },
        forms,
        defaultForm: df,
        moves,
      }
    })
  }

  enemies(): EnemyChoice[] {
    const d = this.d
    return d.classes.map((c) => {
      const gfx = c.gfx[0] ?? PLACEHOLDER_GFX[c.id]
      const spr = d.sprites.index(c.hasGraphics ? gfx : (PLACEHOLDER_GFX[c.id] ?? gfx))
      const group = c.id === 0 ? 'Unused row' : c.boss ? 'Boss' : c.spawnCount > 0 ? 'Enemy' : c.exp === 0 ? 'NPC (scouter row)' : 'Unused row'
      const tags: string[] = []
      if (!c.hasGraphics) tags.push('placeholder sprite')
      if (c.aiState === 0) tags.push('no AI (Nop)')
      if (c.aiState === 29) tags.push('absorbs ki')
      if (c.id === 94) tags.push('invulnerable')
      return {
        id: 'c' + c.id,
        name: c.names.length > 1 ? c.names.slice(0, 3).join(' / ') : this.displayName(c.id),
        group,
        visual: { sprite: spr?.id, anim: 'idle', dir: 0 },
        hp: c.hp,
        stats: { HP: c.hp, STR: c.STR, POW: c.POW, END: c.END, 'melee taken': `${c.meleeMult}/128`, 'ki taken': `${c.energyMult}/128`, exp: c.exp, 'AI state': c.aiState, class: c.id },
        tags,
        note: c.description || undefined,
        faithfulAi: aiFaithful(d, c.aiState),
      }
    })
  }

  arenas(): ArenaChoice[] {
    return ARENAS.map(({ map, ...a }) => ({ ...a, note: `map ${map}` }))
  }

  presets(): PresetChoice[] {
    return this.d.fights.map((f) => {
      // collapse repeated spawns of the same class (e.g. Android 17 on three maps) into one phase
      const phases: { enemy: string; count: number }[] = []
      let prev = ''
      for (const p of f.phases) {
        const id = 'c' + p.classRow
        if (f.key === 'android_17' && id === prev) continue
        if (f.key === 'android_20' && phases.length) continue
        phases.push({ enemy: id, count: 1 })
        prev = id
      }
      if (f.key === 'sparring') phases.splice(0, phases.length, ...f.phases.map((p) => ({ enemy: 'c' + p.classRow, count: 1 })))
      return {
        id: f.key,
        name: f.name,
        note: f.notes || f.phases.map((p) => p.mapName).filter((v, i, a) => a.indexOf(v) === i).join(', '),
        phases,
        arena: ARENA_FOR_MAP[f.phases[0]?.map] ?? 'cellgames',
        fighter: f.phases.find((p) => p.player && p.player !== 'unchanged')?.player?.toLowerCase(),
      }
    })
  }

  knobs(): SetupKnob[] {
    return [
      {
        key: 'growth',
        label: 'Level-up rolls',
        kind: 'select',
        default: 'rolled',
        options: [
          { value: 'rolled', label: 'Rolled with the game RNG' },
          { value: 'average', label: 'Average growth' },
          { value: 'min', label: 'Minimum growth' },
          { value: 'max', label: 'Maximum growth' },
        ],
        hint: 'Stats come from replaying the real level-up routine (0x08003CA0) from the new-game record.',
      },
      { key: 'growthSeed', label: 'Growth seed', kind: 'number', min: 0, max: 99999, default: 0, hint: 'title-screen frames before the RNG is used (the game has no other seed)' },
      { key: 'STR', label: 'STR Capsules (+pts)', kind: 'number', min: 0, max: 99, default: 0, bonus: true, hint: 'STR Cap. +1/+3/+5 items add whole points, cap 100' },
      { key: 'POW', label: 'PWR Capsules (+pts)', kind: 'number', min: 0, max: 99, default: 0, bonus: true },
      { key: 'END', label: 'END Capsules (+pts)', kind: 'number', min: 0, max: 99, default: 0, bonus: true },
      {
        key: 'slot',
        label: 'Selected B move',
        kind: 'select',
        default: '3',
        options: [
          { value: '0', label: 'Slot 1' },
          { value: '1', label: 'Slot 2' },
          { value: '2', label: 'Slot 3 (transform)' },
          { value: '3', label: 'Slot 4 (ki blast)' },
        ],
        hint: 'L cycles slots in battle',
      },
      { key: 'exp', label: 'Gain EXP / level up mid-fight', kind: 'boolean', default: false },
    ]
  }

  private record(setup: FighterSetup): { rec: Record8; level: number } {
    const d = this.d
    const ci = Math.max(0, CHAR_IDS.indexOf(setup.fighter))
    const c = d.chars[ci]
    const lv = Math.max(1, Math.min(50, setup.level | 0))
    const rec = recordAtLevel(c.initial, c.growth, lv, d.expTable, (setup.options.growth as GrowthMode) ?? 'rolled', Number(setup.options.growthSeed ?? 0))
    for (const k of STAT_KEYS) {
      const add = Math.max(0, Number(setup.bonus[k] ?? 0)) << 8
      // STR cap refuses at >= 100, POW/END only at == 100; either way the value is clamped to 0x6400
      rec[k] = Math.min(STAT_CAP, rec[k] + add)
    }
    rec.hp = rec.maxHp
    rec.ep = rec.maxEp
    return { rec, level: lv }
  }

  preview(setup: FighterSetup): StatLine[] {
    const d = this.d
    const { rec } = this.record(setup)
    const f = d.forms[setup.form] ?? d.forms[DEFAULT_FORM[setup.fighter]]
    const ctl = d.controllers[f.controller]
    const base = [rec.STR >> 8, rec.POW >> 8, rec.END >> 8]
    const eff = ctl.override ?? base.map((b, i) => b + (ctl.bonus ? ctl.bonus[i] : 0))
    return [
      { label: 'HP', value: rec.maxHp },
      { label: 'EP', value: rec.maxEp },
      { label: 'STR', value: eff[0], hint: `record ${(rec.STR / 256).toFixed(2)}` },
      { label: 'POW', value: eff[1], hint: `record ${(rec.POW / 256).toFixed(2)}` },
      { label: 'END', value: eff[2], hint: `record ${(rec.END / 256).toFixed(2)}` },
      { label: 'Walk / dash', value: `${ctl.walk / 256} / ${ctl.run / 256} px/f` },
      { label: 'Crit chance', value: `${((164 * eff[0] * 100) / 65536).toFixed(1)}%` },
      { label: 'EXP', value: rec.exp },
    ]
  }

  controls(): ControlHelp[] {
    return [
      { button: 'DPAD', action: 'Move (8-way, no diagonal normalisation). Double-tap a direction: dash' },
      { button: 'A', action: 'Punch (hits on the press frame). Keep holding after a punch, release when charged: special melee' },
      { button: 'B', action: 'Use the selected special (hold to charge / sustain beams; tap again to rapid-fire ki blasts)' },
      { button: 'L', action: 'Cycle the selected special move' },
      { button: 'START', action: 'Pause' },
    ]
  }

  createRng(seed: number) {
    return Log2Rng.fromTitleFrames(seed)
  }

  sprites() {
    return this.d.sprites
  }

  arenaRuntime(id: string): ArenaRuntime {
    const a = ARENAS.find((x) => x.id === id) ?? ARENAS[0]
    return { id: a.id, name: a.name, image: a.image, size: a.size, bounds: a.bounds }
  }

  createBattle(cfg: BattleConfig, w: World): RulesetBattle {
    const d = this.d
    const ctx = new Log2Ctx(d, { exp: !!cfg.fighter.options.exp, popups: cfg.options.popups !== false })
    const ci = Math.max(0, CHAR_IDS.indexOf(cfg.fighter.fighter))
    const { rec, level } = this.record(cfg.fighter)
    const formId = d.chars[ci].forms.includes(cfg.fighter.form) ? cfg.fighter.form : DEFAULT_FORM[CHAR_IDS[ci]]
    const player = new Player(ctx, { char: d.chars[ci], form: formId, record: rec, level, slot: Number(cfg.fighter.options.slot ?? 3) & 3 })
    const b = w.arena.bounds
    player.x = (b.x0 + 40) << 8
    player.y = ((b.y0 + b.y1) >> 1) << 8
    player.facing = 3
    ctx.player = player
    w.add(player)
    // preload enemy sheets
    const usedGfx = new Set<number>()
    for (const ph of cfg.phases) {
      const c = d.classes[Number(ph.enemy.slice(1))]
      if (c) usedGfx.add(c.hasGraphics ? c.gfx[0] : (PLACEHOLDER_GFX[c.id] ?? 60))
    }
    void loadAll([...usedGfx].map((g) => d.sprites.index(g)?.json.sheet).filter((s): s is string => !!s))
    if (w.arena.image) void loadAll([w.arena.image])

    const enemies: Enemy[] = []
    let phase = -1
    let nextPhaseAt = 0
    const total = cfg.phases.reduce((a, p) => a + p.count, 0)
    w.stats.enemiesTotal = total

    const spawnPhase = (i: number) => {
      const ph = cfg.phases[i]
      const c = d.classes[Number(ph.enemy.slice(1))]
      if (!c) return
      const gfx = c.hasGraphics ? c.gfx[0] : (PLACEHOLDER_GFX[c.id] ?? 60)
      const n = Math.max(1, Math.min(12, ph.count))
      for (let k = 0; k < n; k++) {
        const e = new Enemy(ctx, c, gfx, this.displayName(c.id))
        const col = Math.floor(k / 4)
        const row = k % 4
        const cy = (b.y0 + b.y1) >> 1
        // ~120 px in front of the player (outside most aggro radii, so idle enemies wait to be approached)
        const side = player.px + 8 > (b.x0 + b.x1) >> 1 ? -1 : 1
        const ex = Math.max(b.x0 + 8, Math.min(b.x1 - e.colW - 8, player.px + side * (120 + col * 28)))
        const ey = Math.max(b.y0 + 30, Math.min(b.y1 - e.colH - 4, cy + (row - (Math.min(n, 4) - 1) / 2) * Math.max(30, c.hurt[0][3] - c.hurt[0][1] + 4)))
        e.x = ex << 8
        e.y = Math.round(ey) << 8
        e.facing = side > 0 ? 2 : 3
        w.add(e)
        e.start(w)
        enemies.push(e)
      }
      w.logEvent('info', `phase ${i + 1}: ${c.name} x${n}`)
    }
    phase = 0
    spawnPhase(0)

    const battle: RulesetBattle = {
      postStep(w2) {
        w2.centerCamera(player.px + 8, player.py - 8)
        const live = enemies.filter((e) => !e.removed && !e.defeated)
        if (!live.length && phase < cfg.phases.length - 1 && !player.dead) {
          if (!nextPhaseAt) nextPhaseAt = w2.frame + 90
          else if (w2.frame >= nextPhaseAt) {
            nextPhaseAt = 0
            phase++
            spawnPhase(phase)
          }
        }
      },
      drawHud(r) {
        drawHud(r, ctx, player, enemies, cfg.phases.length > 1 ? `${phase + 1}/${cfg.phases.length}` : '')
      },
      outcome() {
        if (player.dead) return 'lost'
        const live = enemies.filter((e) => !e.defeated)
        if (!live.length && phase >= cfg.phases.length - 1) return 'won'
        return 'running'
      },
      debugLines(w2) {
        const lines = [
          `frame ${w2.frame}  rng calls ${w2.rng.calls}  state ${w2.rng.getState().map((v) => v.toString(16).padStart(8, '0')).join(' ')}`,
          `player ${player.stateName} t=${player.t} pos ${player.px},${player.py} face ${player.facing} HP ${player.hp}/${player.maxHp} EP ${player.ep}/${player.maxEp} form ${player.form.id} STR/POW/END ${player.stats.STR}/${player.stats.POW}/${player.stats.END} mult ${player.stats.meleeMult}`,
        ]
        for (const e of enemies) if (!e.removed) lines.push(`${e.label}#${e.id} HP ${e.hp}/${e.maxHp} ${e.ko ? 'KO ' : ''}${e.ai.describe()}${e.hittable ? '' : ' (unhittable)'}`)
        return lines
      },
      summary(w2) {
        return [
          { label: 'Fighter', value: `${d.chars[ci].name} Lv${player.level}` },
          { label: 'HP left', value: `${player.hp}/${player.maxHp}` },
          { label: 'Phases', value: `${Math.min(phase + 1, cfg.phases.length)}/${cfg.phases.length}` },
          { label: 'RNG calls', value: w2.rng.calls },
        ]
      },
    }
    return battle
  }
}

export const log2Ruleset = new Log2Ruleset()
