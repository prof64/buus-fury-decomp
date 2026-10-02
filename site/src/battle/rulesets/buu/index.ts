// Buu's Fury ruleset: roster (9 record characters, 54 forms, fusions), the 150 enemy types / 71 bosses, Buu maps
// as arenas, story boss chains, setup knobs (level growth, stat points, equipment, skill level) and battle setup.

import { loadAll } from '../../core/assets'
import type {
  ArenaChoice, BattleConfig, ControlHelp, EnemyChoice, FighterChoice, FighterSetup, FormChoice, PresetChoice, Ruleset, RulesetBattle, SetupKnob, StatLine,
} from '../../core/ruleset'
import type { ArenaRuntime, World } from '../../core/world'
import { BuuCtx } from './context'
import { loadBuu, type BuuData, type BuuForm, type EquipDef } from './data'
import { Enemy } from './enemy'
import { drawHud, HUD } from './hud'
import { Player } from './player'
import { fuse, recalcStats, recordAtLevel, type BuuRecord, type GrowthMode } from './progression'
import { BuuRng } from './rng'

const ARENAS: (ArenaChoice & { map: string })[] = [
  { id: 'crater', map: '8-71', name: 'Giant Crater', image: '', size: [0, 0], bounds: { x0: 56, y0: 72, x1: 424, y1: 360 } },
  { id: 'volcano', map: '15-32', name: 'Volcano Summit', image: '', size: [0, 0], bounds: { x0: 40, y0: 96, x1: 440, y1: 376 } },
  { id: 'hfil', map: '18-14', name: 'HFIL', image: '', size: [0, 0], bounds: { x0: 24, y0: 80, x1: 300, y1: 304 } },
  { id: 'wasteland', map: '20-7', name: 'Barren Wasteland', image: '', size: [0, 0], bounds: { x0: 40, y0: 72, x1: 440, y1: 420 } },
  { id: 'demon', map: '8-68', name: 'Demon World', image: '', size: [0, 0], bounds: { x0: 96, y0: 76, x1: 400, y1: 330 } },
  { id: 'canyon', map: '8-70', name: 'Rocky Canyon', image: '', size: [0, 0], bounds: { x0: 88, y0: 168, x1: 470, y1: 400 } },
  { id: 'yakon', map: '8-40', name: "Yakon's Planet", image: '', size: [0, 0], bounds: { x0: 72, y0: 92, x1: 380, y1: 390 } },
  { id: 'crater2', map: '6-16', name: 'Giant Crater (after the blast)', image: '', size: [0, 0], bounds: { x0: 56, y0: 72, x1: 424, y1: 360 } },
  { id: 'htc', map: '6-9', name: 'Hyperbolic Time Chamber', image: '', size: [0, 0], bounds: { x0: 72, y0: 64, x1: 328, y1: 240 } },
  { id: 'stadium', map: '7-6', name: 'World Tournament Stadium', image: '', size: [0, 0], bounds: { x0: 216, y0: 256, x1: 472, y1: 516 } },
  { id: 'forest', map: '8-3', name: 'Southwest Forest', image: '', size: [0, 0], bounds: { x0: 40, y0: 40, x1: 520, y1: 520 } },
]
const ARENA_FOR_MAP: Record<string, string> = Object.fromEntries(ARENAS.map((a) => [a.map, a.id]))
ARENA_FOR_MAP['20-5'] = 'wasteland'

interface FighterDef {
  id: string
  name: string
  /** record character (fusions: derived from `parts`) */
  char: number
  parts?: [number, number]
  forms: number[]
  defaultForm: number
}

const FIGHTERS: FighterDef[] = [
  { id: 'goku', name: 'Goku', char: 0, forms: [2, 0, 1, 3, 4, 5, 6, 7, 8], defaultForm: 2 },
  { id: 'gohan', name: 'Gohan', char: 1, forms: [10, 9, 11, 12, 13, 14, 15, 16, 17, 18], defaultForm: 10 },
  { id: 'goten', name: 'Goten', char: 2, forms: [19, 20], defaultForm: 19 },
  { id: 'trunks', name: 'Trunks', char: 3, forms: [21, 22, 23, 24, 25], defaultForm: 21 },
  { id: 'vegeta', name: 'Vegeta', char: 4, forms: [26, 27, 28, 29, 30], defaultForm: 26 },
  { id: 'gotenks', name: 'Gotenks', char: 5, parts: [2, 3], forms: [31, 32, 33, 34, 35, 36, 37, 38], defaultForm: 31 },
  { id: 'vegito', name: 'Vegito', char: 6, parts: [0, 4], forms: [39, 40], defaultForm: 39 },
  { id: 'gogeta', name: 'Gogeta', char: 7, parts: [0, 4], forms: [41, 42, 43, 44], defaultForm: 41 },
  ...[46, 47, 48, 49, 50, 51, 52, 53].map((f) => ({ id: 'f' + f, name: '', char: 8, forms: [f], defaultForm: f })),
]

interface Preset extends PresetChoice {
  fighter?: string
}
const PRESETS: Preset[] = [
  { id: 'buu_saga', name: 'Majin Buu saga', note: 'Majin Buu (West City) -> Super Buu (Lookout, two-phase) -> Super Buu (Brain) -> Kid Buu', phases: [{ enemy: 't28', count: 1 }, { enemy: 't116', count: 1 }, { enemy: 't121', count: 1 }, { enemy: 't60', count: 1 }], arena: 'crater' },
  { id: 'majin_buu', name: 'Majin Buu & Babidi (Giant Crater) - survive', note: 'candy beam, stretch punch, eat; Babidi hovers and follows Buu. Buu 0x1A takes no damage in the game (no ApplyDamage in its on-hit trees, 0 HP lost in the emulator): the story ends this encounter, here you win by surviving 60 s', phases: [{ enemy: 't26+~t29', count: 1 }], arena: 'crater' },
  { id: 'majin_buu2', name: 'Majin Buu (2nd encounter) - survive', note: 'invulnerable in the game as well (story encounter): survive 60 s', phases: [{ enemy: 't27', count: 1 }], arena: 'crater' },
  { id: 'super_buu', name: 'Super Buu (two-phase, absorbs ki blasts)', phases: [{ enemy: 't116', count: 1 }], arena: 'crater2' },
  { id: 'super_buu_waste', name: 'Super Buu: Barren Wasteland', note: 'minion swarm (0x76), then the scripted waypoint fight (0x77): he takes no damage, bat his big ball back three times (punch while he glides)', phases: [{ enemy: 't118', count: 1 }, { enemy: 't119', count: 1 }], arena: 'wasteland' },
  { id: 'kid_buu', name: 'Kid Buu', note: 'splits in two when you keep your distance, merges back after 1800 frames', phases: [{ enemy: 't60', count: 1 }], arena: 'wasteland' },
  { id: 'janemba', name: 'Janemba -> Super Janemba (HFIL)', note: 'hand strikes, airborne summons of Mini-Janembas; then Super Janemba (teleports, energy wave)', phases: [{ enemy: 't51', count: 1 }, { enemy: 't122', count: 1 }], arena: 'hfil' },
  { id: 'broly', name: 'Broly (Volcano Summit)', note: 'charged punch, jump smash; landing on a rock stuns him', phases: [{ enemy: 't10', count: 1 }], arena: 'volcano' },
  { id: 'babidi_ship', name: "Babidi's ship: Pui Pui -> Yakon -> Dabura", phases: [{ enemy: 't96', count: 1 }, { enemy: 't145', count: 1 }, { enemy: 't18', count: 1 }], arena: 'demon' },
  { id: 'yakon', name: 'Yakon (use energy attacks!)', phases: [{ enemy: 't145', count: 1 }], arena: 'yakon' },
  { id: 'dabura', name: 'Dabura (dark sphere, stone spit)', phases: [{ enemy: 't18', count: 1 }], arena: 'demon' },
  { id: 'majin_vegeta', name: 'Majin Vegeta (Final Flash)', phases: [{ enemy: 't70', count: 1 }], arena: 'canyon' },
  { id: 'tournament', name: 'World Tournament: Spopovich & Yamu', note: 'bear hug + EP drain', phases: [{ enemy: 't111+t147', count: 1 }], arena: 'stadium' },
  { id: 'htc', name: 'Hyperbolic Time Chamber sparring -> Super Buu', phases: [{ enemy: 't39', count: 1 }, { enemy: 't130', count: 1 }, { enemy: 't115', count: 1 }], arena: 'htc' },
  { id: 'inside_buu', name: "Inside Buu: Gotenks -> Gohan -> Piccolo -> Super Buu", phases: [{ enemy: 't40', count: 1 }, { enemy: 't35', count: 1 }, { enemy: 't90', count: 1 }, { enemy: 't121', count: 1 }], arena: 'htc' },
  { id: 'pikkon', name: 'Pikkon (Other World tournament)', phases: [{ enemy: 't91', count: 1 }], arena: 'stadium' },
  { id: 'totenhotep', name: 'Pharoah Totenhotep (mummy wraps)', phases: [{ enemy: 't129', count: 1 }], arena: 'crater' },
  { id: 'ninja_boss', name: 'Ninja Boss', phases: [{ enemy: 't80', count: 1 }], arena: 'forest' },
  { id: 'guardian', name: "Pilaf's Guardian", phases: [{ enemy: 't63', count: 1 }], arena: 'forest' },
  { id: 'warlord', name: 'Airship Warlord', phases: [{ enemy: 't0', count: 1 }], arena: 'forest' },
  { id: 'blister', name: 'Blister (spawns Enzymes)', phases: [{ enemy: 't6', count: 1 }], arena: 'crater' },
  { id: 'worm', name: 'Worm (head + 5 segments)', phases: [{ enemy: 't142', count: 1 }], arena: 'crater' },
  { id: 'majin_fighters', name: 'Southwest Forest: 4 Majin Fighters', phases: [{ enemy: 't68', count: 4 }], arena: 'forest' },
]

const BOSS_TYPES = new Set([0x00, 0x06, 0x0a, 0x0b, 0x12, 0x1a, 0x1b, 0x1c, 0x23, 0x25, 0x27, 0x28, 0x33, 0x3c, 0x3d, 0x3f, 0x46, 0x50, 0x51, 0x5a, 0x5b, 0x60, 0x64, 0x6f, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x81, 0x82, 0x8e, 0x91, 0x93, 0x2d, 0x31, 0x55, 0x6d, 0x6e, 0x70, 0x94, 0x84, 0x87, 0x3b, 0x26, 0x4, 0x19, 0x1d])
const HELPER_TYPES = new Set([0x0d, 0x0e, 0x22, 0x34, 0x35, 0x36, 0x37, 0x54, 0x72, 0x7e, 0x8d, 0x88, 0x89])

const INVULNERABLE = new Set([0x1a, 0x1b])
const SLOTS = ['body', 'hands', 'feet', 'accessory'] as const
const STAT_KEYS = ['STR', 'POW', 'END', 'SPD'] as const

function formLabel(d: BuuData, f: BuuForm): string {
  const charName = f.name
  const tags: string[] = []
  if (f.id === 14) return 'Great Saiyaman'
  if (f.ss3) tags.push('Super Saiyan 3')
  else if (f.ss) tags.push('Super Saiyan')
  if ([2, 4, 6].includes(f.id)) tags.push('halo')
  if (f.id === 1) tags.push('Other World')
  if (f.id === 7 || f.id === 8) tags.push('no transform flag')
  if (f.id === 24) tags.push('slow (speed / 4)')
  if (f.id === 25) tags.push('+64 speed')
  if (f.fusion && (f.flags & 0x20)) tags.push('timed fusion')
  const spr = d.sprites.index(f.gfx - 9)
  return `${tags.length ? tags.join(', ') : spr?.json.category === 'player' ? spr.json.name : charName} · form ${f.id}`
}

function parseEnemyId(id: string): { type: number; primary: boolean }[] {
  return id.split('+').map((p) => ({ type: Number(p.replace(/^~?t/, '')), primary: !p.startsWith('~') }))
}

export class BuuRuleset implements Ruleset {
  info = { id: 'buu' as const, title: "Buu's Fury", short: "Buu's Fury", enabled: true }
  levelRange: [number, number] = [50, 200]
  d!: BuuData

  async load() {
    if (this.d) return
    this.d = await loadBuu()
    for (const a of ARENAS) {
      const m = this.d.maps.get(a.map)
      if (m) {
        a.image = m.composite
        a.size = m.size
      }
    }
    const sheets = new Set<string>()
    for (const f of this.d.forms) {
      const s = this.d.sprites.index(f.gfx - 9)
      if (s) sheets.add(s.json.sheet)
    }
    for (const id of ['fx_086fd9d0', 'fx_08712fdc']) {
      const s = this.d.sprites.get(id)
      if (s) sheets.add(s.json.sheet)
    }
    const icons = [...this.d.skills.values()].flatMap((s) => [s.icon, s.iconDim])
    await loadAll([...sheets, HUD, ...icons])
  }

  private fighterDef(id: string) {
    return FIGHTERS.find((f) => f.id === id) ?? FIGHTERS[0]
  }
  private portrait(idx: number) {
    return this.d.portraits.get(idx)
  }

  fighters(): FighterChoice[] {
    const d = this.d
    return FIGHTERS.map((fd) => {
      const forms: FormChoice[] = fd.forms.map((fid) => {
        const f = d.forms[fid]
        const spr = d.sprites.index((({ 117: 115, 119: 118, 121: 120 } as Record<number, number>)[f.gfx] ?? f.gfx) - 9) ?? d.sprites.index(f.gfx - 10)
        const b = f.bonus
        const bonus = b.STR ? `+${b.STR} STR/POW/END, ${b.walk >= 0 ? '+' : ''}${b.walk} speed` : f.id === 24 ? 'speed / 4' : b.walk ? `+${b.walk} speed` : 'record stats'
        return { id: fid, name: formLabel(d, f), visual: { sprite: spr?.id, anim: 'idle', dir: 0 }, portrait: this.portrait(f.portrait), note: `form ${fid} (gfx ${f.gfx}, flags 0x${f.flags.toString(16)}): ${bonus}${f.fusion ? ', fusion' : ''}` }
      })
      const c = d.chars[fd.char]
      const df = d.forms[fd.defaultForm]
      const name = fd.name || df.name
      const skills = fd.char === 8 ? [7] : c.skills
      return {
        id: fd.id,
        name,
        portrait: this.portrait(df.portrait),
        visual: forms[0].visual,
        forms,
        defaultForm: fd.defaultForm,
        moves: [
          { button: 'A', name: 'Melee (instant hit, 17 f recovery)', note: 'crit (82*STR + crit bonus)/65536' },
          { button: 'R', name: 'Block (1/16 from the front)' },
          { button: 'R+B', name: 'Energy shield', note: '~0.2 EP/frame' },
          ...skills.map((s, i) => {
            const sk = d.skills.get(s)!
            return { button: `B (skill ${i + 1})`, name: sk.name, note: sk.costs.length > 1 ? `EP ${sk.costs.join('/')}` : sk.minEp ? `EP ${sk.minEp}` : s === 7 ? '5% max EP / shot' : undefined }
          }),
        ],
      }
    })
  }

  enemies(): EnemyChoice[] {
    const d = this.d
    // story groups used by the presets (several enemy types in one phase; '~' = helper, not needed for victory)
    const groups: EnemyChoice[] = [...new Set(PRESETS.flatMap((p) => p.phases.map((ph) => ph.enemy)).filter((id) => id.includes('+')))].map((id) => {
      const parts = parseEnemyId(id).map((p) => ({ ...p, t: d.enemies[p.type] }))
      const main = parts[0].t
      return {
        id,
        name: parts.map((p) => p.t.name).join(' + '),
        group: 'Story group',
        visual: { sprite: main.sprite?.id, anim: 'idle', dir: 0 },
        hp: parts.filter((p) => p.primary).reduce((a, p) => a + p.t.hp, 0),
        stats: Object.fromEntries(parts.map((p) => [p.t.name + (p.primary ? '' : ' (helper)'), `HP ${p.t.hp} STR ${p.t.STR} POW ${p.t.POW} END ${p.t.END}`])),
        tags: ['group'],
        faithfulAi: parts.every((p) => d.states.has(p.t.ai)),
      }
    })
    return [...groups, ...d.enemies.map((e) => {
      const group = HELPER_TYPES.has(e.type) || e.hp === 0 ? 'Helper / part' : BOSS_TYPES.has(e.type) ? 'Boss' : e.spawns.length ? 'Enemy' : 'Unused / variant'
      const tags: string[] = []
      if (!e.sprite) tags.push('no graphics')
      if (e.placeholderGfx) tags.push('placeholder sprite')
      if (e.resM < 128) tags.push(`melee x${(e.resM / 128).toFixed(2)}`)
      if (e.resE < 128) tags.push(`energy x${(e.resE / 128).toFixed(2)}`)
      if (!e.standardReaction) tags.push('no flinch')
      const boss = d.bosses['0x' + e.type.toString(16)]
      return {
        id: 't' + e.type,
        name: e.name,
        group,
        visual: { sprite: e.sprite?.id, anim: 'idle', dir: 0 },
        hp: e.hp,
        stats: { HP: e.hp, STR: e.STR, POW: e.POW, END: e.END, 'melee taken': `${e.resM}/128`, 'energy taken': `${e.resE}/128`, XP: e.exp, 'AI state': '0x' + e.ai.toString(16), class: '0x' + e.type.toString(16) },
        tags,
        note: boss?.name ?? (e.database || undefined),
        faithfulAi: d.states.has(e.ai),
      }
    })]
  }

  arenas(): ArenaChoice[] {
    return ARENAS.map(({ map, ...a }) => ({ ...a, note: `map ${map}` }))
  }
  presets(): PresetChoice[] {
    return PRESETS
  }

  knobs(): SetupKnob[] {
    const eq = (slot: (typeof SLOTS)[number]): SetupKnob => {
      const list = this.d.equipment.filter((e) => e.slot === slot)
      return {
        key: 'eq_' + slot,
        label: `Equipment: ${slot}`,
        kind: 'select',
        default: String(list[0].id),
        options: list.map((e) => ({ value: String(e.id), label: `${e.name}${e.id === list[0].id ? '' : ` (Lv${e.level})`} ${eqMods(e)}`.trim() })),
        hint: 'items.json: s8 stat modifiers + effect scripts (crit %, block, EP cost, regen, sets...)',
      }
    }
    return [
      {
        key: 'growth', label: 'Level-up rolls', kind: 'select', default: 'rolled',
        options: [
          { value: 'rolled', label: 'Rolled with the game RNG' },
          { value: 'average', label: 'Average (rolls 128)' },
          { value: 'min', label: 'Minimum (rolls 0)' },
          { value: 'max', label: 'Maximum (rolls 255)' },
        ],
        hint: 'max HP / EP from replaying LevelUp 0x08013678 one level at a time from the new-game record',
      },
      { key: 'growthSeed', label: 'Growth seed', kind: 'number', min: 0, max: 99999, default: 0 },
      {
        key: 'autoSpend', label: 'Spend remaining stat points', kind: 'select', default: 'even',
        options: [
          { value: 'even', label: 'Evenly STR/POW/END/SPD' },
          { value: 'STR', label: 'All on STR' },
          { value: 'POW', label: 'All on POW' },
          { value: 'END', label: 'All on END' },
          { value: 'SPD', label: 'All on SPD' },
          { value: 'none', label: 'Leave unspent' },
        ],
        hint: '3 points per level, spent 1:1 on the Status screen (notes §3)',
      },
      { key: 'STR', label: 'Points on STR', kind: 'number', min: 0, max: 450, default: 0, bonus: true },
      { key: 'POW', label: 'Points on POW', kind: 'number', min: 0, max: 450, default: 0, bonus: true },
      { key: 'END', label: 'Points on END', kind: 'number', min: 0, max: 450, default: 0, bonus: true },
      { key: 'SPD', label: 'Points on SPD', kind: 'number', min: 0, max: 450, default: 0, bonus: true },
      eq('body'),
      eq('hands'),
      eq('feet'),
      eq('accessory'),
      {
        key: 'skillLevel', label: 'Skill level', kind: 'select', default: '2',
        options: [
          { value: '0', label: 'Level 1' },
          { value: '1', label: 'Level 2' },
          { value: '2', label: 'Level 3' },
        ],
        hint: 'rec+0x2E: set by story scripts in the game; applies to every upgradable skill',
      },
      { key: 'holyWater', label: 'Start with Holy Water (+25 all, 30 s)', kind: 'boolean', default: false },
      { key: 'xp', label: 'Gain XP / level up mid-fight', kind: 'boolean', default: false },
    ]
  }

  /** record + equipment for a setup */
  build(setup: FighterSetup): { rec: BuuRecord; equip: (EquipDef | undefined)[]; fd: FighterDef; spent: Record<string, number>; free: number } {
    const d = this.d
    const fd = this.fighterDef(setup.fighter)
    const mode = (setup.options.growth as GrowthMode) ?? 'rolled'
    const seed = Number(setup.options.growthSeed ?? 0)
    const lv = Math.max(50, Math.min(200, setup.level | 0))
    let rec: BuuRecord
    if (fd.parts) {
      const a = recordAtLevel(d.chars[fd.parts[0]], lv, d.xpTable, mode, seed)
      const b = recordAtLevel(d.chars[fd.parts[1]], lv, d.xpTable, mode, seed + 1)
      rec = fuse(a, b)
      rec.level = Math.max(a.level, b.level)
      // the fusion has no points of its own; the components' points are spent on the components (INFERRED: halved)
      rec.points = Math.floor((a.points + b.points) / 2)
    } else rec = recordAtLevel(d.chars[fd.char], lv, d.xpTable, mode, seed)
    let free = rec.points
    const spent: Record<string, number> = { STR: 0, POW: 0, END: 0, SPD: 0 }
    for (const k of STAT_KEYS) {
      const n = Math.max(0, Math.min(free, Number(setup.bonus[k] ?? 0)))
      spent[k] += n
      free -= n
    }
    const auto = String(setup.options.autoSpend ?? 'even')
    if (auto === 'even') {
      for (let i = 0; free > 0; i = (i + 1) & 3, free--) spent[STAT_KEYS[i]]++
    } else if (auto !== 'none' && spent[auto] != null) {
      spent[auto] += free
      free = 0
    }
    for (const k of STAT_KEYS) rec[k] = Math.min(255, rec[k] + spent[k])
    rec.points = free
    const equip = SLOTS.map((s) => d.equipment.find((e) => e.id === Number(setup.options['eq_' + s])) ?? d.equipment.find((e) => e.slot === s))
    return { rec, equip, fd, spent, free }
  }

  preview(setup: FighterSetup): StatLine[] {
    const d = this.d
    const { rec, equip, fd, spent, free } = this.build(setup)
    const f = d.forms[setup.form] ?? d.forms[fd.defaultForm]
    const eff = recalcStats(rec, equip, !!setup.options.holyWater)
    const b = f.bonus
    const walk = 5 * eff.SPD + 102
    const lv = rec.level
    return [
      { label: 'Level', value: lv, hint: fd.parts ? 'fusion of the two components at this level' : `new game: level ${d.chars[fd.char].level}` },
      { label: 'HP', value: rec.maxHp },
      { label: 'EP', value: rec.maxEp },
      { label: 'STR', value: eff.STR + b.STR, hint: `record ${rec.STR} (+${spent.STR} pts)` },
      { label: 'POW', value: eff.POW + b.POW, hint: `record ${rec.POW} (+${spent.POW} pts)` },
      { label: 'END / END2', value: `${eff.END + b.END} / ${eff.END2 + b.END2}`, hint: `record ${rec.END} (+${spent.END} pts)` },
      { label: 'SPD', value: eff.SPD, hint: `+${spent.SPD} pts` },
      { label: 'Walk / dash', value: f.id === 24 ? `${(walk >> 2) / 256} / ${((walk + 320) >> 2) / 256} px/f` : `${((walk + b.walk) / 256).toFixed(2)} / ${((walk + 320 + b.dash) / 256).toFixed(2)} px/f` },
      { label: 'Crit chance', value: `${(((82 * (eff.STR + b.STR) + eff.critPct * 656) * 100) / 65536).toFixed(1)}%` },
      { label: 'Stat points', value: `${rec.points + spent.STR + spent.POW + spent.END + spent.SPD - free} spent, ${free} unspent` },
      { label: 'EP cost / block', value: `x${eff.epCostFactor / 16} / x${eff.blockFactor / 0x10000 * 16}/16` },
      { label: 'XP', value: rec.xp },
    ]
  }

  controls(): ControlHelp[] {
    return [
      { button: 'DPAD', action: 'Move (8-way). Double-tap a direction within 20 frames: dash' },
      { button: 'A', action: 'Melee: one instant hit on the press, 17-frame recovery (+1 EP per damaging hit)' },
      { button: 'B', action: 'Use the selected skill (hold to charge / sustain beams; tap again to refire Energy Blasts; release to fire Big Bang / Masenko / Sword)' },
      { button: 'R', action: 'Block: 1/16 damage from the front. Hold R+B: energy shield (drains EP)' },
      { button: 'L', action: 'Cycle the selected skill' },
      { button: 'START', action: 'Pause' },
    ]
  }

  createRng(seed: number) {
    return BuuRng.fromSeed(seed)
  }
  sprites() {
    return this.d.sprites
  }
  arenaRuntime(id: string): ArenaRuntime {
    const a = ARENAS.find((x) => x.id === id) ?? ARENAS[0]
    return { id: a.id, name: a.name, image: a.image, size: a.size, bounds: a.bounds, view: a.view }
  }

  createBattle(cfg: BattleConfig, w: World): RulesetBattle {
    const d = this.d
    const ctx = new BuuCtx(d, { popups: cfg.options.popups !== false, xp: !!cfg.fighter.options.xp })
    const { rec, equip, fd } = this.build(cfg.fighter)
    const formId = fd.forms.includes(cfg.fighter.form) ? cfg.fighter.form : fd.defaultForm
    const skills = fd.char === 8 ? [7] : d.chars[fd.char].skills
    const player = new Player(ctx, {
      name: fd.name || d.forms[formId].name,
      form: formId,
      rec,
      equip,
      skills,
      skillLevel: Number(cfg.fighter.options.skillLevel ?? 2),
      slot: 0,
      holyWater: !!cfg.fighter.options.holyWater,
      formsAllowed: fd.forms,
    })
    const b = w.arena.bounds
    player.x = (b.x0 + 32) << 8
    player.y = ((b.y0 + b.y1) >> 1) << 8
    player.facing = 3
    ctx.player = player
    w.add(player)
    if (w.arena.image && typeof Image !== 'undefined') void loadAll([w.arena.image])

    const spawnEnemy = (ww: World, type: number, x: number, y: number, parent?: Enemy | null, start?: number): Enemy | null => {
      const t = d.enemies[type]
      if (!t) return null
      const e = new Enemy(ctx, t)
      if (t.sprite && typeof Image !== 'undefined') t.sprite.load().catch(() => undefined)
      e.x = x << 8
      e.y = y << 8
      e.clampInside(ww)
      if (parent) {
        e.parentE = parent
        parent.children.push(e)
      }
      ww.add(e)
      ctx.enemies.push(e)
      e.w = ww
      if (start !== -1) e.start(ww, start ?? t.ai)
      return e
    }
    ctx.spawnEnemy = (ww, type, x, y, parent, start) => spawnEnemy(ww, type, x, y, parent as Enemy | null, start)

    let phase = 0
    let nextPhaseAt = 0
    w.stats.enemiesTotal = cfg.phases.reduce((a, p) => a + p.count * parseEnemyId(p.enemy).filter((x) => x.primary).length, 0)
    const spawnPhase = (i: number) => {
      const ph = cfg.phases[i]
      const parts = parseEnemyId(ph.enemy)
      const n = Math.max(1, Math.min(12, ph.count))
      let k = 0
      for (const part of parts) {
        const t = d.enemies[part.type]
        if (!t) continue
        for (let j = 0; j < n; j++, k++) {
          // the boss's own spawn point when the arena is its map, else ~120 px in front of the player
          const sp = t.spawns.find((s) => ARENA_FOR_MAP[`${s.world}-${s.map}`] === w.arena.id && s.x > 0 && s.y > 0)
          let x: number
          let y: number
          if (sp && n === 1 && sp.x >= b.x0 && sp.x < b.x1 && sp.y >= b.y0 && sp.y < b.y1 && Math.abs(sp.x - player.px) > 60) {
            x = sp.x
            y = sp.y
          } else {
            x = Math.min(b.x1 - 40, player.px + 120 + Math.floor(k / 4) * 30)
            y = ((b.y0 + b.y1) >> 1) + ((k % 4) - (Math.min(n * parts.length, 4) - 1) / 2) * 34
          }
          const e = spawnEnemy(w, part.type, x, Math.round(y))
          if (e) {
            // 0-HP scripted objects (body parts, triggers) cannot be defeated: not counted for victory
            e.primary = part.primary && t.hp > 0
            e.facing = 2
          }
        }
      }
      w.logEvent('info', `phase ${i + 1}: ${parts.map((p) => d.enemies[p.type]?.name).join(' + ')}`)
    }
    spawnPhase(0)
    const live = () => ctx.enemies.filter((e) => e.primary && !e.defeated && !e.removed)
    // story encounters whose boss takes no damage in the game (0x1A / 0x1B): win by surviving (simulator rule)
    const SURVIVE = 3600
    let phaseStart = 0
    const surviveLeft = () => {
      const l = live()
      if (!l.length || !l.every((e) => INVULNERABLE.has(e.type.type))) return -1
      return Math.max(0, SURVIVE - (w.frame - phaseStart))
    }

    const battle: RulesetBattle = {
      postStep(w2) {
        w2.centerCamera(player.px + 8, player.py - 8)
        if (!live().length && phase < cfg.phases.length - 1 && !player.dead) {
          if (!nextPhaseAt) {
            nextPhaseAt = w2.frame + 120
            for (const e of ctx.enemies) if (!e.primary && !e.removed && !e.defeated) e.die(false)
          } else if (w2.frame >= nextPhaseAt) {
            nextPhaseAt = 0
            phase++
            phaseStart = w2.frame
            spawnPhase(phase)
          }
        }
        ctx.enemies = ctx.enemies.filter((e) => !e.removed || e.primary)
      },
      drawHud(r) {
        const sv = surviveLeft()
        drawHud(r, ctx, player, ctx.enemies as Enemy[], [cfg.phases.length > 1 ? `${phase + 1}/${cfg.phases.length}` : '', sv >= 0 ? `SURVIVE ${Math.ceil(sv / 60)}` : ''].filter(Boolean).join(' '))
      },
      outcome() {
        if (player.dead) return 'lost'
        if (phase >= cfg.phases.length - 1 && (!live().length || surviveLeft() === 0)) return 'won'
        return 'running'
      },
      debugLines(w2) {
        const lines = [
          `frame ${w2.frame}  rng calls ${w2.rng.calls}  state ${w2.rng.getState().map((v) => v.toString(16).padStart(8, '0')).join(' ')}`,
          `player ${player.state} t=${player.t}${player.sk ? ` skill ${player.sk.id} t${player.sk.t}` : ''} pos ${player.px},${player.py} face ${player.facing} HP ${player.hp}/${player.maxHp} EP ${player.ep}/${player.maxEp} form ${player.form.id} STR/POW/END/END2 ${player.stats.STR}/${player.stats.POW}/${player.stats.END}/${player.stats.END2} SS ${player.ssGauge} SS3 ${player.ss3Gauge >> 16} cd ${player.ssCooldown >> 8}`,
          `candy ${ctx.candy} sphere ${ctx.sphereState} wraps ${player.wraps} held ${player.holdKind ?? '-'}`,
        ]
        for (const e of ctx.enemies as Enemy[]) if (!e.removed && !e.part) lines.push(`${e.label}#${e.id} 0x${e.type.type.toString(16)} @${e.px},${e.py} f${e.facing} HP ${e.hp}/${e.maxHp} z${e.z} ${e.defeated ? 'DEFEATED ' : ''}${e.ai.describe()}${e.hittable ? '' : ' (unhittable)'} cnt ${e.counter}`)
        return lines
      },
      summary(w2) {
        return [
          { label: 'Fighter', value: `${player.name} Lv${player.rec.level}` },
          { label: 'HP left', value: `${player.hp}/${player.maxHp}` },
          { label: 'Phases', value: `${Math.min(phase + 1, cfg.phases.length)}/${cfg.phases.length}` },
          { label: 'RNG calls', value: w2.rng.calls },
        ]
      },
    }
    return battle
  }
}

function eqMods(e: EquipDef): string {
  const m: string[] = []
  for (const [k, v] of [['STR', e.STR], ['POW', e.POW], ['END', e.END], ['END2', e.END2], ['SPD', e.SPD]] as const) if (v) m.push(`${v > 0 ? '+' : ''}${v} ${k}`)
  if (e.critPct) m.push(`crit ${e.critPct}%`)
  if (e.blockHalf) m.push('block x1/2')
  if (e.energyCostHalf) m.push('EP cost x1/2')
  if (e.energyBlockHalf) m.push('shield cost x1/2')
  if (e.ssCooldown78) m.push('SS cooldown x7/8')
  if (e.thorns) m.push(`thorns ${e.thorns}`)
  if (e.hpRegen256) m.push('HP regen')
  if (e.xp) m.push(`XP +${Math.round((e.xp * 100) / 256)}%`)
  if (e.id === 40) m.push('set: +50 with Saiyan Gloves+Boots')
  if (e.id === 153) m.push('+50 all at <=5% HP')
  return m.length ? '[' + m.join(', ') + ']' : ''
}

export const buuRuleset = new BuuRuleset()
