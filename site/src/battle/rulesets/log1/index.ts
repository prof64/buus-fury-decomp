// The Legacy of Goku (GBA 2002) ruleset: Goku Lv1-25 vs any of the 34 enemy templates (every sprite variant), on
// real LoG1 maps with their collision layers. Mechanics: re/log1/notes.md (ports in game.ts, player.ts, enemy.ts,
// projectiles.ts). Approximations are listed in APPROX below and shown on the setup screen.

import { loadAll } from '../../core/assets'
import { BTN } from '../../core/input'
import type { ArenaChoice, BattleConfig, ControlHelp, EnemyChoice, FighterChoice, FighterSetup, PresetChoice, Ruleset, RulesetBattle, SetupKnob, StatLine } from '../../core/ruleset'
import type { ArenaRuntime, World } from '../../core/world'
import { damageCap, kiBlastDamage, DEFENSE_ODDS } from './damage'
import { loadLog1, SPEED_TABLE, type Log1Data, type Template } from './data'
import { Enemy } from './enemy'
import { Char, Log1Game, levelRow } from './game'
import { drawHud, FEATHER, HUD_IMG, ICONS } from './hud'
import { BATTLE_ITEMS, Pickup, applyHealingItem } from './items'
import type { Log1Entity } from './entity'
import { Player } from './player'
import { KiBlast } from './projectiles'
import { Log1Rng, Log1Rng2 } from './rng'

interface Arena extends ArenaChoice {
  map: number
  player: [number, number]
  enemy: [number, number]
}

const ARENA_DEFS: Omit<Arena, 'image' | 'size' | 'bounds'>[] = [
  { id: 'cliff', map: 9, name: 'Cliff valley (Raditz)', player: [448, 232], enemy: [448, 152] },
  { id: 'paozu', map: 2, name: 'Mt. Paozu', player: [600, 456], enemy: [680, 400] },
  { id: 'forest', map: 4, name: 'Forest', player: [584, 456], enemy: [504, 432] },
  { id: 'snakeway', map: 11, name: 'Snake Way', player: [312, 868], enemy: [232, 868] },
  { id: 'plateau', map: 12, name: 'Rocky plateau (Hell)', player: [280, 264], enemy: [280, 184] },
  { id: 'kingkai', map: 13, name: "King Kai's planet path (no ki moves)", player: [336, 352], enemy: [336, 272] },
  { id: 'snow', map: 16, name: 'Snow forest', player: [240, 480], enemy: [240, 400] },
  { id: 'city', map: 23, name: 'City road (Nappa, Vegeta)', player: [408, 240], enemy: [408, 160] },
  { id: 'canyon', map: 24, name: 'Canyon', player: [352, 592], enemy: [352, 512] },
  { id: 'namek', map: 28, name: 'Namek', player: [472, 584], enemy: [472, 504] },
  { id: 'ruins', map: 30, name: 'Namek lava ruins (Ginyu Force)', player: [512, 496], enemy: [512, 416] },
  { id: 'henchmen', map: 34, name: 'Namek (Frieza soldiers)', player: [96, 272], enemy: [176, 272] },
  { id: 'frieza', map: 40, name: 'Namek (Frieza)', player: [664, 240], enemy: [664, 160] },
  { id: 'frieza_final', map: 41, name: 'Namek (final Frieza)', player: [624, 224], enemy: [624, 144] },
]
const ARENAS: Arena[] = ARENA_DEFS.map((a) => ({ ...a, image: '', size: [0, 0] as [number, number], bounds: { x0: 0, y0: 0, x1: 0, y1: 0 } }))

/** display names per (template, sprite) */
const NAMES: Record<string, string> = {
  squirrel: 'Squirrel',
  snake: 'Snake',
  wolf: 'Wolf',
  piccolo: 'Piccolo',
  raditz: 'Raditz',
  nappa: 'Nappa',
  oozaru1: 'Great Ape Vegeta',
  oozaru2: 'Great Ape Vegeta (alt.)',
  trex: 'Tyrannosaurus',
  trex2: 'Tyrannosaurus (small)',
  ptero1: 'Pterodactyl',
  ptero2: 'Pterodactyl (passive)',
  ptero3: 'Pterodactyl (passive, alt.)',
  robber1: 'Robber',
  robber2: 'Robber (2)',
  robber3: 'Robber (3)',
  devil1: 'Devil',
  devil2: 'Devil (unkillable)',
  crab: 'Crab',
  bubbles: 'Bubbles',
  gregory: 'Gregory',
  ginyu: 'Captain Ginyu',
  recoome: 'Recoome',
  burter: 'Burter',
  jeice: 'Jeice',
  hench1: 'Frieza soldier 1',
  hench2: 'Frieza soldier 2',
  hench3: 'Frieza soldier 2 (alt.)',
  hench4: 'Frieza soldier 3',
  hench5: 'Frieza soldier 3 (alt.)',
  hench6: 'Frieza soldier 4',
  hench7: 'Frieza soldier 5',
  hench8: 'Frieza soldier 6',
  frieza1: 'Frieza (1st form)',
  frieza2: 'Frieza (2nd form)',
  frieza3: 'Frieza (3rd form)',
  frieza4: 'Frieza (final form)',
  snkqueen: 'Princess Snake',
  spirit4: 'Spirit',
  spirit5: 'Spirit (alt.)',
  vegeta1: 'Vegeta',
  vegeta2: 'Vegeta (2nd fight)',
}
const BOSS_TEMPLATES = new Set([4, 5, 17, 18, 19, 26, 27, 28, 29, 30, 32, 33])

const MOVE_TYPE = ['stands still', 'stands still', 'returns to its guard tile', 'patrols / wanders', 'wanders', 'wanders on a leash']
const TARGET_TYPE = ['avoids you (fights back when hit)', 'aggressive', 'passive unless scripted (assist list)', 'joins only when you are already fighting', 'always runs away']
const FIGHT_STYLE = ['always attacks', 'flees while you are nearly unhurt', 'attacks only with more HP than you', 'may flee below half HP (6/101 per decision)']

/** Where the simulator departs from the decoded game (shown on the setup screen) */
export const APPROX = [
  'Secondary RNG (punch damage) advances once per frame + per punch; the ROM also rolls it for map animations, so exact punch values differ from a real run (distribution identical).',
  'Bosses use the generic enemy AI only: their story scripts (dialogue, transformations, scripted moves) were not decoded.',
  'Kamehameha beam graphics are drawn procedurally (the ROM uses custom OAM code 0x0800B778); hit boxes and timings are exact.',
  'Enemy hurt/death flashes are approximate in colour; sound effects are not played.',
  'Healing items are used instantly with SELECT (the game uses the pause menu).',
]

function enemyId(t: number, sprite: string) {
  return `t${t}:${sprite}`
}

export class Log1Ruleset implements Ruleset {
  info = { id: 'log1' as const, title: 'The Legacy of Goku', short: 'LoG I', enabled: true, note: 'Approximations: ' + APPROX.join(' ') }
  levelRange: [number, number] = [1, 25]
  d!: Log1Data

  async load() {
    if (this.d) return
    this.d = await loadLog1()
    for (const a of ARENAS) {
      const m = this.d.maps.get(a.map)
      if (m) {
        a.image = m.composite
        a.size = m.size
        a.bounds = { x0: 0, y0: 0, x1: m.size[0], y1: m.size[1] }
      }
    }
    const sheets = [...this.d.sprites.byId.values()].filter((s) => s.json.category === 'player' || s.json.category === 'projectile').map((s) => s.json.sheet)
    await loadAll([...sheets, HUD_IMG, FEATHER, ...ICONS])
  }

  fighters(): FighterChoice[] {
    const goku = this.d.byLabel.get('goku')
    const ss = this.d.byLabel.get('ssgoku')
    const portrait = this.d.sprites.get('portrait_23_goku')?.json.sheet
    return [
      {
        id: 'goku',
        name: 'Goku',
        portrait,
        visual: { sprite: goku?.id, anim: 'idle', dir: 0 },
        forms: [
          { id: 0, name: 'Goku', portrait, visual: { sprite: goku?.id, anim: 'idle', dir: 0 }, note: 'The only playable character. Defense is a constant 250 at every level.' },
          { id: 1, name: 'Super Saiyan', visual: { sprite: ss?.id, anim: 'idle', dir: 0 }, note: 'Sprite variant used by the story for the Frieza finale; no stat effect was found in the code (INFERRED cosmetic).' },
        ],
        defaultForm: 0,
        moves: [
          { button: 'A', name: 'Punch', note: '75-100% of max HP, hits on the press frame' },
          { button: 'B (tap)', name: 'Ki Blast', note: 'max HP / 3, free when tapped' },
          { button: 'B (hold)', name: 'Kamehameha', note: 'damage = Ki spent charging' },
          { button: 'B (hold)', name: 'Solar Flare', note: 'freezes every enemy 60-360 frames' },
          { button: 'R', name: 'Fly / land', note: '2 px/frame; enemies cannot attack you while you fly' },
        ],
      },
    ]
  }

  enemies(): EnemyChoice[] {
    const out: EnemyChoice[] = []
    for (const t of this.d.templates) {
      for (const s of t.sprites) {
        const spr = this.d.byLabel.get(s.name)
        const boss = BOSS_TEMPLATES.has(t.id) || s.name.startsWith('oozaru')
        const tags: string[] = []
        if (t.hp >= 65536) tags.push('negative-damage bug')
        if (t.ki > 0) tags.push('ki blasts')
        if (t.flying) tags.push('flies')
        if (t.hp >= 0x7fffffff) tags.push('unkillable')
        if (s.name.startsWith('oozaru')) tags.push('no melee animation')
        const melee = t.hp >= 65536 ? 'can roll negative (heals you)' : `${t.hp >> 2}-${(t.hp >> 2) + (t.hp >> 1)}`
        out.push({
          id: enemyId(t.id, s.name),
          name: NAMES[s.name] ?? s.name,
          group: t.id === 15 || t.id === 16 ? 'Training target' : boss ? 'Boss' : 'Enemy',
          visual: { sprite: spr?.id, anim: 'idle', dir: 0 },
          hp: t.hp,
          stats: {
            HP: t.hp,
            Ki: t.ki,
            DEF: t.defense,
            EXP: t.exp,
            speed: `${SPEED_TABLE[t.speedClass]}/16 px/f`,
            aggro: `${t.aggroTiles} tiles`,
            melee,
            template: `${t.id} "${t.name}"`,
            class: t.id,
          },
          tags,
          note: `AI: ${TARGET_TYPE[t.targetType] ?? t.targetType}; ${FIGHT_STYLE[t.fightStyle] ?? t.fightStyle}; idle: ${MOVE_TYPE[t.moveType] ?? t.moveType}.` + (s.name.startsWith('oozaru') ? ' The ROM sprite has no Melee animation, so after a melee the generic AI waits until it is hit (faithful to set_char_state).' : ''),
          faithfulAi: true,
        })
      }
    }
    return out
  }

  arenas(): ArenaChoice[] {
    return ARENAS.map(({ map, player: _p, enemy: _e, ...a }) => ({ ...a, note: `map ${map}` }))
  }

  presets(): PresetChoice[] {
    return [
      { id: 'raditz', name: 'Raditz', note: 'Map 9 cliff valley', phases: [{ enemy: enemyId(4, 'raditz'), count: 1 }], arena: 'cliff' },
      { id: 'saiyans', name: 'Nappa → Vegeta', note: 'Map 23 (Vegeta templates 32 then 33)', phases: [{ enemy: enemyId(5, 'nappa'), count: 1 }, { enemy: enemyId(32, 'vegeta1'), count: 1 }, { enemy: enemyId(33, 'vegeta2'), count: 1 }], arena: 'city' },
      { id: 'snake', name: 'Princess Snake', note: 'Snake Way', phases: [{ enemy: enemyId(30, 'snkqueen'), count: 1 }], arena: 'snakeway' },
      { id: 'ginyu', name: 'Ginyu Force', note: 'Map 30: Recoome, Burter, Jeice, Ginyu', phases: [{ enemy: enemyId(18, 'recoome'), count: 1 }, { enemy: enemyId(19, 'burter'), count: 1 }, { enemy: enemyId(19, 'jeice'), count: 1 }, { enemy: enemyId(17, 'ginyu'), count: 1 }], arena: 'ruins' },
      { id: 'frieza', name: 'Frieza (all forms)', note: 'Maps 40/41', phases: [26, 27, 28, 29].map((t, i) => ({ enemy: enemyId(t, `frieza${i + 1}`), count: 1 })), arena: 'frieza' },
      { id: 'soldiers', name: 'Frieza soldiers', note: 'Map 34 squad (henchmen 4-6 roll the negative-damage bug)', phases: [{ enemy: enemyId(23, 'hench6'), count: 2 }, { enemy: enemyId(24, 'hench7'), count: 2 }, { enemy: enemyId(25, 'hench8'), count: 1 }], arena: 'henchmen' },
      { id: 'wolves', name: 'Wolf pack', note: 'Mt. Paozu', phases: [{ enemy: enemyId(2, 'wolf'), count: 3 }], arena: 'paozu' },
      { id: 'dinos', name: 'Dinosaurs', note: 'Tyrannosaurus x2 then the strong one', phases: [{ enemy: enemyId(7, 'trex2'), count: 2 }, { enemy: enemyId(6, 'trex'), count: 1 }], arena: 'canyon' },
    ]
  }

  knobs(): SetupKnob[] {
    return [
      { key: 'kame', label: 'Knows Kamehameha', kind: 'boolean', default: true, hint: 'learned from King Kai (script 0x08543DE4, bit 1 of 0x0300004D)' },
      { key: 'solar', label: 'Knows Solar Flare', kind: 'boolean', default: true, hint: 'learned before Namek (script 0x08544070, bit 2)' },
      {
        key: 'move',
        label: 'Selected ki move',
        kind: 'select',
        default: '0',
        options: [
          { value: '0', label: 'Ki Blast' },
          { value: '1', label: 'Kamehameha' },
          { value: '2', label: 'Solar Flare' },
        ],
        hint: 'L cycles among learned moves in battle',
      },
      { key: 'herbs', label: 'Herbs (+1/4 HP)', kind: 'number', min: 0, max: 5, default: 2, hint: 'carry limit 5; SELECT uses one' },
      { key: 'senzu', label: 'Senzu beans (full heal)', kind: 'number', min: 0, max: 3, default: 0 },
      { key: 'pickups', label: 'Map pickups (herbs, feathers)', kind: 'boolean', default: true, hint: "the arena map's real item placements; take with A" },
      { key: 'bugs', label: 'Authentic bugs', kind: 'boolean', default: true, hint: 'rand() overflow: enemies with >= 65536 HP can roll negative melee damage that HEALS you' },
      { key: 'god', label: 'Invincibility code', kind: 'boolean', default: false, hint: 'UP DOWN LEFT RIGHT B A at the Shenron intro (0x03000C18): HP never drops' },
      { key: 'exp', label: 'EXP / level up mid-fight', kind: 'boolean', default: true },
      { key: 'popups', label: 'Damage numbers (sim only)', kind: 'boolean', default: false, hint: 'LoG1 shows no damage numbers' },
    ]
  }

  private learned(o: FighterSetup['options']) {
    return 1 | (o.kame !== false ? 2 : 0) | (o.solar !== false ? 4 : 0)
  }

  preview(setup: FighterSetup): StatLine[] {
    const row = levelRow(this.d.levels, setup.level)
    const next = this.d.levels[row.level]
    return [
      { label: 'HP', value: row.maxHp },
      { label: 'Ki', value: row.maxKi },
      { label: 'Defense', value: 250, hint: 'constant (init_char 0x08006E62)' },
      { label: 'Punch', value: `${Math.floor((row.maxHp * 384) / 512)}-${Math.floor((row.maxHp * 511) / 512)}` },
      { label: 'Ki Blast', value: kiBlastDamage(row.maxHp) },
      { label: 'Ki regen', value: `${row.maxKi >> 6} / 16 frames` },
      { label: 'Charge cost', value: `${(row.maxKi + 15) >> 4} Ki / 8 frames` },
      { label: 'Flight energy', value: `${row.maxFlight} (${row.maxFlight * 8} frames moving)` },
      { label: 'Max damage taken', value: `${damageCap(row.maxHp)} per enemy hit` },
      { label: 'EXP', value: next ? `${row.exp} (next ${next.exp})` : `${row.exp} (max)` },
      { label: 'Defense roll', value: `${Math.round(DEFENSE_ODDS.full * 100)}% full, 2% miss` },
    ]
  }

  controls(): ControlHelp[] {
    return [
      { button: 'DPAD', action: 'Walk 1 px/frame, one direction at a time (UP > DOWN > LEFT > RIGHT, no diagonals)' },
      { button: 'A', action: 'Punch (16 frames) / pick up an item in front of you' },
      { button: 'B', action: 'Selected ki move: tap = free Ki Blast; hold to charge (Ki every 8 frames)' },
      { button: 'L', action: 'Cycle the ki move (learned ones)' },
      { button: 'R', action: 'Take off / land (flight energy drains while moving)' },
      { button: 'SELECT', action: 'Use a herb / Senzu bean (pause-menu item, simulator shortcut)' },
      { button: 'START', action: 'Pause' },
    ]
  }

  createRng(seed: number) {
    return Log1Rng.fromSeed(seed)
  }

  sprites() {
    return this.d.sprites
  }

  arenaRuntime(id: string): ArenaRuntime {
    const a = ARENAS.find((x) => x.id === id) ?? ARENAS[0]
    return { id: a.id, name: a.name, image: a.image, size: a.size, bounds: a.bounds, view: a.view }
  }

  /** template + sprite from an enemy id */
  parseEnemy(id: string): { t: Template; sprite: string } | null {
    const m = /^t(\d+):(.+)$/.exec(id)
    if (!m) return null
    const t = this.d.templates[Number(m[1])]
    if (!t) return null
    return { t, sprite: m[2] }
  }

  createBattle(cfg: BattleConfig, w: World): RulesetBattle {
    return createLog1Battle(this, cfg, w)
  }
}

/** Frame start: frame counter, the per-frame secondary-RNG roll, menu shortcuts, then the AI tick 0x08008018. */
export function log1PreStep(g: Log1Game, w: World, input: { pressed: number; held: number }, mapIndex: number) {
  g.fc++
  g.rng2.raw()
  if (input.pressed & BTN.SELECT) {
    const m = applyHealingItem(g)
    if (m) w.logEvent('info', m)
  }
  if (input.pressed & BTN.L) {
    // 0x08003524: next learned move
    let s = g.selected
    for (;;) {
      s++
      if (s >= 3) {
        s = 0
        break
      }
      if ((g.learned >> s) & 1) break
    }
    g.selected = s
  }
  if (mapIndex === 13 || mapIndex === 14) input.pressed &= ~BTN.B // ki moves disabled on King Kai's maps (0x08002F12)
  g.aiTick()
}

/** Frame end (render pass): every entity's anim_tick. (The camera follows Goku from his movement, see followCamera.) */
export function log1PostStep(_g: Log1Game, w: World, _player: Player) {
  for (const e of w.entities) {
    const le = e as unknown as Log1Entity
    if (typeof le.animTick !== 'function' || e.removed) continue
    if (le.flashT > 0) le.flashT--
    le.animTick()
  }
}

/** nearest tile (spiral) where a w x h box at (x,y) is free on `layer` */
export function freeSpot(g: Log1Game, x: number, y: number, layer: number, w = 16, h = 8): [number, number] {
  const ok = (px: number, py: number) => g.passable({ x0: px, y0: py, x1: px + w, y1: py + h }, layer)
  x = x & ~7
  y = y & ~7
  if (ok(x, y)) return [x, y]
  for (let r = 1; r < 40; r++)
    for (let dy = -r; dy <= r; dy++)
      for (let dx = -r; dx <= r; dx++) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue
        if (ok(x + dx * 8, y + dy * 8)) return [x + dx * 8, y + dy * 8]
      }
  return [x, y]
}

export function createLog1Battle(rs: Log1Ruleset, cfg: BattleConfig, w: World): RulesetBattle {
  const d = rs.d
  const o = cfg.fighter.options
  const arena = ARENAS.find((a) => a.id === cfg.arena) ?? ARENAS[0]
  const map = d.maps.get(arena.map)
  const rng = w.rng as Log1Rng
  const rng2 = new Log1Rng2(0, 0)
  for (let i = 0; i < (cfg.seed % 997) + 3; i++) rng2.raw()
  // private copy of the collision bits (pickups block their tiles until taken)
  const base = d.collision.get(arena.map)
  const col = base ? { w: base.w, h: base.h, layers: base.layers.map((l) => l.slice()) } : null
  const g = new Log1Game(d, rng, rng2, col, map?.sizeTiles ?? [64, 64], {
    authenticBugs: o.bugs !== false,
    godmode: !!o.god,
    exp: o.exp !== false,
    popups: !!o.popups,
  })
  g.world = w
  g.learned = rs['learned'](o)
  g.selected = Number(o.move ?? 0)
  if (!((g.learned >> g.selected) & 1)) g.selected = 0
  g.herbs = Math.max(0, Math.min(5, Number(o.herbs ?? 0)))
  g.senzu = Math.max(0, Math.min(3, Number(o.senzu ?? 0)))

  // player char (init_char player branch + level table)
  const row = levelRow(d.levels, cfg.fighter.level)
  const pc = new Char('Goku', 1, null)
  pc.defense = 250
  pc.exp = row.exp
  pc.hp = pc.maxHp = row.maxHp
  pc.ki = pc.maxKi = row.maxKi
  pc.kiRegen = pc.maxKi >> 6
  pc.act = 0
  g.player = pc
  g.awake.push(pc)
  g.cap = damageCap(pc.maxHp)
  g.maxFlight = g.flight = row.maxFlight
  const player = new Player(g, pc)
  if (cfg.fighter.form === 1) player.sprite = d.byLabel.get('ssgoku') ?? player.sprite
  pc.ent = player
  const [psx, psy] = freeSpot(g, arena.player[0], arena.player[1], 0)
  player.setPos(psx, psy)
  pc.setPos(psx, psy)
  player.facing = 1
  w.add(player)
  if (w.arena.image) void loadAll([w.arena.image])

  const enemies: Enemy[] = []
  const total = cfg.phases.reduce((a, p) => a + p.count, 0)
  w.stats.enemiesTotal = total
  let phase = 0
  let nextPhaseAt = 0

  const spawnPhase = (i: number) => {
    const ph = cfg.phases[i]
    const pe = rs.parseEnemy(ph.enemy)
    if (!pe) return
    const { t, sprite } = pe
    const spr = d.byLabel.get(sprite)
    void loadAll([spr?.json.sheet, sprite.startsWith('v') ? d.byLabel.get('vegeta_ki_blast')?.json.sheet : ''].filter((s): s is string => !!s))
    const ts = t.sprites.find((s) => s.name === sprite) ?? t.sprites[0]
    const hurt = ts.hurt
    const n = Math.max(1, Math.min(12, ph.count))
    // later phases appear near Goku if he left the arena's enemy spot (they only wake near the screen)
    let [bx, by] = arena.enemy
    if (i > 0 && Math.abs(player.x - bx) + Math.abs(player.y - by) > 120) {
      bx = player.x
      by = player.y - 72
    }
    for (let k = 0; k < n; k++) {
      const off = k === 0 ? [0, 0] : [((k + 1) >> 1) * (k & 1 ? 24 : -24), (k >> 2) * -16]
      const [ex, ey] = freeSpot(g, bx + off[0], by + off[1], t.flying ? 1 : 0, ts.foot[0], ts.foot[1])
      const c = Char.fromTemplate(t, ex, ey)
      c.face = 0
      const e = new Enemy(g, c, spr, sprite, hurt, NAMES[sprite] ?? t.name)
      e.facing = 0
      w.add(e)
      g.addEnemy(c)
      enemies.push(e)
    }
    w.logEvent('info', `phase ${i + 1}: ${NAMES[sprite] ?? t.name} x${n}`)
  }
  spawnPhase(0)
  // the map's own healing / feather pickups (not where a fighter stands)
  if (o.pickups !== false && map) {
    const taken = [player.footRect(), ...enemies.map((e) => e.char.rect())]
    for (const it of map.pickups) {
      if (!BATTLE_ITEMS.includes(it.item)) continue
      const info = d.items.get(it.item)
      const r = { x0: it.x, y0: it.y, x1: it.x + (info?.w ?? 16), y1: it.y + (info?.h ?? 16) }
      if (taken.some((t) => t.x0 < r.x1 && r.x0 < t.x1 && t.y0 < r.y1 && r.y0 < t.y1)) continue
      w.add(new Pickup(g, it.item, it.x, it.y))
    }
  }

  g.fireEnemyKi = (c, power) => {
    const e = c.ent as Enemy
    e.setState(5)
    w.add(new KiBlast(g, e, true, power))
  }
  const popups = !!o.popups
  g.onHit = (att, def, res, raw, kind) => {
    const st = w.stats
    if (att.team === 1) {
      if (res.result) {
        st.damageDealt += res.dmg
        st.hitsDealt++
        st.maxHit = Math.max(st.maxHit, res.dmg)
      }
      w.logEvent('damage', `Goku ${kind} -> ${(def.ent as Enemy | null)?.label ?? def.name}: raw ${raw}, roll ${res.roll}, dealt ${res.result ? res.dmg : 0}${res.result === 0 ? ' (clink)' : ''}`)
    } else {
      if (res.result) {
        st.damageTaken += res.dmg
        st.hitsTaken++
      }
      w.logEvent('damage', `${(att.ent as Enemy | null)?.label ?? att.name} ${kind} -> Goku: raw ${raw} (cap ${g.cap}), roll ${res.roll}, ${res.result ? (res.dmg < 0 ? `HEALED ${-res.dmg}` : `took ${res.dmg}`) : 'no damage'}`)
      if (res.result && res.dmg < 0) st.extra['negative-damage heals'] = Number(st.extra['negative-damage heals'] ?? 0) + 1
    }
    if (popups && res.result) {
      const ent = def.ent as unknown as Log1Entity
      w.popup(String(res.dmg), ent.x + 8, ent.y - 30, def.team === 1 ? '#ff6060' : '#fff')
    }
  }
  g.onKill = (killer, victim) => {
    if (killer.team === 1) w.stats.kills++
    w.logEvent('info', `${victim.name} defeated${killer.team === 1 && g.opts.exp ? ` (+${victim.exp} exp)` : ''}`)
  }
  g.onLevelUp = (lv) => {
    w.logEvent('info', `LEVEL UP! Lv${lv}`)
    w.stats.extra['level ups'] = Number(w.stats.extra['level ups'] ?? 0) + 1
  }
  g.setCamera(player.x + 8 - 120, player.y - 8 - 80)

  let playerDead = false
  const battle: RulesetBattle = {
    preStep(w2, input) {
      log1PreStep(g, w2, input, arena.map)
    },
    postStep(w2) {
      log1PostStep(g, w2, player)
      if (!pc.alive && !playerDead) {
        playerDead = true
        w2.logEvent('info', 'Goku was defeated')
      }
      const live = enemies.filter((e) => e.char.alive)
      if (!live.length && phase < cfg.phases.length - 1 && pc.alive) {
        if (!nextPhaseAt) nextPhaseAt = w2.frame + 90
        else if (w2.frame >= nextPhaseAt) {
          nextPhaseAt = 0
          phase++
          spawnPhase(phase)
        }
      }
    },
    drawHud(r) {
      drawHud(r, g, player, enemies, cfg.phases.length > 1 ? `${phase + 1}/${cfg.phases.length}` : '')
    },
    outcome() {
      if (playerDead && player.deathT > 64) return 'lost'
      if (!enemies.some((e) => e.char.alive) && phase >= cfg.phases.length - 1 && enemies.length) return 'won'
      return 'running'
    },
    debugLines(w2) {
      const lines = [
        `frame ${w2.frame} fc ${g.fc}  rng ${rng.getState().map((v) => v.toString(16).padStart(8, '0')).join(' ')} calls ${rng.calls}  path timer ${g.pathTimer}  solar ${g.solar}`,
        `Goku Lv${g.level} st ${player.st} t24 ${player.t24} tick ${player.tick} pos ${player.x},${player.y} tile ${pc.tx},${pc.ty} face ${player.facing} HP ${pc.hp}/${pc.maxHp} Ki ${pc.ki}/${pc.maxKi} exp ${pc.exp} cap ${g.cap} fly ${g.flying ? 'yes' : 'no'} ${g.flight}/${g.maxFlight}`,
      ]
      for (const e of enemies) {
        const c = e.char
        if (!c.alive) continue
        const act = ['awake', 'path', 'asleep', 'roam-woken'][c.act] ?? c.act
        lines.push(
          `${e.label}#${e.id} HP ${c.hp}/${c.maxHp} Ki ${c.ki} ${['idle', 'walk', '?', 'fly', 'ki', 'melee', 'hurt', 'dead'][c.state]} ${act} stun ${c.stun} inv ${c.inv} aggro ${c.aggro} tile ${c.tx},${c.ty} path ${c.pathIdx}/${c.pathLen} goal ${c.lastGoal.join(',')} tgt ${c.lastAttacker ? 'attacker' : c.target ? 'target' : '-'}`,
        )
      }
      return lines
    },
    summary(w2) {
      return [
        { label: 'Fighter', value: `Goku Lv${g.level}${cfg.fighter.level !== g.level ? ` (from Lv${cfg.fighter.level})` : ''}` },
        { label: 'HP left', value: `${pc.hp}/${pc.maxHp}` },
        { label: 'Phases', value: `${Math.min(phase + 1, cfg.phases.length)}/${cfg.phases.length}` },
        ...Object.entries(w2.stats.extra).map(([k, v]) => ({ label: k, value: v })),
        { label: 'RNG calls', value: w2.rng.calls },
      ]
    },
  }
  ;(w as unknown as { log1?: unknown }).log1 = { g, player, enemies }
  return battle
}

export const log1Ruleset = new Log1Ruleset()
