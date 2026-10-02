// Normalises the three games' enemy tables into one shape for lists and headers; detail pages also get
// the raw record for game-specific sections.
import type { SpriteEntity, SpriteIndex } from '../lib/sprites'
import { hexNum, titleCase, type J } from '../lib/util'

export interface EnemyVM {
  key: string
  name: string
  sub?: string
  idLabel: string
  rom?: string
  hp: number
  exp: number
  stats: { label: string; value: number; title?: string }[]
  sprites: SpriteEntity[]
  boss: boolean
  used: boolean
  npcRow?: boolean
  spawns: number
  attacks: string[]
  raw: J
  /** values the calculators need */
  calc: { STR?: number; POW?: number; END?: number; meleeMult?: number; energyMult?: number; resistMelee?: number; resistEnergy?: number; def?: number; ki?: number }
}

const LOG1_NAMES: Record<string, string> = {
  squirrel: 'Squirrel',
  snake: 'Snake',
  wolf: 'Wolf',
  piccolo: 'Piccolo',
  raditz: 'Raditz',
  nappa: 'Nappa',
  'strong trex': 'T-Rex (strong)',
  trex: 'T-Rex',
  pterodactyl_1: 'Pterodactyl',
  pterodactyl_2: 'Pterodactyl (passive)',
  robber_1: 'Robber',
  robber_2: 'Robber (2)',
  devil_1: 'Devil',
  devil_2: 'Devil (unkillable)',
  crab: 'Crab',
  bubbles: 'Bubbles',
  gregory: 'Gregory',
  burter: 'Captain Ginyu',
  reccome: 'Recoome',
  jeice: 'Burter / Jeice',
  snakequeen: 'Princess Snake',
  spirit: 'Spirit',
  vegeta: 'Vegeta',
}
const LOG1_BOSSES = new Set([4, 5, 17, 18, 19, 26, 27, 28, 29, 30, 32, 33])

export function log1Enemies(json: J, sp?: SpriteIndex): EnemyVM[] {
  return (json?.enemies ?? []).map((e: J) => {
    const sprites: SpriteEntity[] = (e.sprites ?? [])
      .map((s: J) => sp?.byDef.get(hexNum(s.sprite_def?.def_addr) ?? -1))
      .filter(Boolean)
    const tmpl = String(e.name)
    let name = LOG1_NAMES[tmpl] ?? titleCase(tmpl)
    if (e.id === 32) name = 'Vegeta (1st fight)'
    if (e.id === 33) name = 'Vegeta (2nd form)'
    if (/^frieza_(\d)/.test(tmpl)) name = `Frieza (form ${tmpl.slice(-1)})`
    if (/^henchman_(\d)/.test(tmpl)) name = `Frieza Henchman ${tmpl.slice(-1)}`
    const spriteNames = (e.sprites ?? []).map((s: J) => s.sprite_name).join(', ')
    return {
      key: String(e.id),
      name,
      sub: `template "${tmpl}" · sprite ${spriteNames}`,
      idLabel: `template ${e.id}`,
      rom: e.rom_addr,
      hp: e.hp,
      exp: e.exp_reward,
      stats: [
        { label: 'Ki', value: e.ki },
        { label: 'DEF', value: e.defense, title: 'flat defense used by the defense roll' },
        { label: 'Speed', value: e.speed_px_per_frame, title: 'px per frame' },
        { label: 'Aggro', value: e.aggro_tiles, title: 'tiles' },
      ],
      sprites,
      boss: LOG1_BOSSES.has(e.id),
      used: (e.appears ?? []).length > 0 || LOG1_BOSSES.has(e.id),
      spawns: (e.appears ?? []).reduce((a: number, m: J) => a + (m.placed_enemies ?? 0) + (m.scripted_npc_actors ?? 0), 0),
      attacks: ['melee'].concat(e.ki_blast?.usable ? ['ki blast'] : []),
      raw: e,
      calc: { def: e.defense, ki: e.ki },
    } as EnemyVM
  })
}

export function log2Enemies(json: J, sp?: SpriteIndex): EnemyVM[] {
  return (json?.classes ?? []).map((c: J) => {
    const sprites: SpriteEntity[] = []
    for (const g of c.graphics ?? []) {
      const s = sp?.byGfx(g.gfx_id) ?? (typeof g.descriptor === 'string' ? sp?.byDef.get(hexNum(g.descriptor)!) : undefined)
      if (s && !sprites.includes(s)) sprites.push(s)
    }
    const names: string[] = c.names ?? []
    const npcRow = !c.used_in_game && (c.spawn_count ?? 0) === 0 && !c.is_boss_or_scripted
    return {
      key: String(c.id),
      name: names.length ? (names.length > 3 ? names.slice(0, 3).join(', ') + '…' : names.join(', ')) : `Unnamed class ${c.id}`,
      sub: c.ai_summary?.attacks?.length ? c.ai_summary.attacks.join('; ') : undefined,
      idLabel: `class ${c.id}`,
      rom: c.addr,
      hp: c.hp,
      exp: c.exp,
      stats: [
        { label: 'STR', value: c.STR },
        { label: 'POW', value: c.POW },
        { label: 'END', value: c.END },
      ],
      sprites,
      boss: !!c.is_boss_or_scripted,
      used: !!c.used_in_game,
      npcRow,
      spawns: c.spawn_count ?? 0,
      attacks: c.attack_keys ?? [],
      raw: c,
      calc: { STR: c.STR, POW: c.POW, END: c.END, meleeMult: c.melee_taken_mult, energyMult: c.energy_taken_mult },
    } as EnemyVM
  })
}

const BUU_BOSS = /buu|babidi|dabura|janemba|broly|pui pui|yakon|spopovich|gotenks|vegeta|bojack|frieza|cell|cooler|android|zangya|bido|bujin|kogu|hirudegarn|pikkon|tapion/i

export function buuEnemies(json: J, sp?: SpriteIndex): EnemyVM[] {
  return (json?.enemies ?? []).map((e: J) => {
    const sprites: SpriteEntity[] = []
    for (const g of e.gfx_ids ?? []) {
      const s = sp?.byGfx(g) ?? sp?.byDef.get(hexNum(e.gfx_defs?.[g]) ?? -1)
      if (s && !sprites.includes(s)) sprites.push(s)
    }
    const spawns = (e.spawns ?? []).reduce((a: number, s: J) => a + (s.count ?? 1), 0)
    // story fights: named, no loot script, never flinch, real HP pool (sprite categories are only visual guesses)
    const boss = !!e.name && e.drop_script === '0x0' && /^none/.test(e.hit_reaction ?? '') && e.hp >= 1000 && (BUU_BOSS.test(e.name) || spawns <= 5)
    return {
      key: String(e.type),
      name: e.name ?? `Unnamed type 0x${e.type.toString(16).toUpperCase()}`,
      sub: e.name_source && e.name_source !== 'database' ? `name from ${e.name_source}` : undefined,
      idLabel: `type 0x${e.type.toString(16).toUpperCase()} (${e.type})`,
      rom: e.address,
      hp: e.hp,
      exp: e.exp,
      stats: [
        { label: 'STR', value: e.STR },
        { label: 'POW', value: e.POW },
        { label: 'END', value: e.END },
      ],
      sprites,
      boss,
      used: spawns > 0,
      spawns,
      attacks: [],
      raw: e,
      calc: { STR: e.STR, POW: e.POW, END: e.END, resistMelee: e.resist_melee, resistEnergy: e.resist_energy },
    } as EnemyVM
  })
}

export function enemiesOf(game: string, json: J, sp?: SpriteIndex): EnemyVM[] {
  if (game === 'log1') return log1Enemies(json, sp)
  if (game === 'log2') return log2Enemies(json, sp)
  return buuEnemies(json, sp)
}

export const ENEMY_FILE: Record<string, string> = { log1: 'enemies', log2: 'enemies', buu: 'enemies' }
export const AI_FILE: Record<string, string | undefined> = { log1: undefined, log2: 'ai', buu: 'enemy_ai' }
