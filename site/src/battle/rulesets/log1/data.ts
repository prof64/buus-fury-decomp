// Loads the reverse-engineered LoG1 tables (data/log1/*.json + data/battle/log1_collision.json) and shapes them for
// the simulator. Every number comes from the JSON (read from the ROM by re/log1/extract_*.py).

import { loadData } from '../../../data'
import { SpriteBank, type SpriteEntityJson, type SpriteSet } from '../../core/sprites'

type J = Record<string, any> // eslint-disable-line @typescript-eslint/no-explicit-any

/** Level table 0x080114A8 {exp, maxHP, maxKi, maxFlight} */
export interface LevelRow {
  level: number
  exp: number
  maxHp: number
  maxKi: number
  maxFlight: number
}

/** Enemy template 0x086B6094 (0x40 bytes, notes §6.1) */
export interface Template {
  id: number
  name: string
  exp: number
  moveType: number
  targetType: number
  fightStyle: number
  groupFlag: number
  flying: number
  aggroTiles: number
  leashHome: [number, number]
  leashRadius: number
  guard: [number, number]
  speedClass: number
  ki: number
  hp: number
  defense: number
  /** sprite names (ROM labels) that use this template (sprite->template table 0x0815A69C) */
  sprites: { name: string; hurt: number[][]; anims: string[]; foot: [number, number]; def: number }[]
}

/** SPEED_TABLE 0x080116A4 (1/16 px per frame) */
export const SPEED_TABLE = [8, 10, 12, 14, 16, 18, 19, 20]

export interface CollisionMap {
  w: number
  h: number
  /** 3 layers (0 ground, 1 flying, 2 projectile walls), 1 bit per tile, set = passable */
  layers: Uint8Array[]
}

export interface MapInfo {
  index: number
  name: string
  composite: string
  size: [number, number]
  sizeTiles: [number, number]
  entrance: [number, number]
  actors: { x: number; y: number; sprite: string; kind: number; template?: number }[]
  pickups: { x: number; y: number; item: number }[]
}

export interface Log1Data {
  levels: LevelRow[]
  templates: Template[]
  sprites: SpriteBank
  /** sprite set by ROM sprite label ("wolf", "hench4", "goku", ...) */
  byLabel: Map<string, SpriteSet>
  maps: Map<number, MapInfo>
  collision: Map<number, CollisionMap>
  items: Map<number, { id: number; kind: string; w: number; h: number; sprite?: SpriteSet }>
  /** player hurtbox (sprite def goku 0x085C1884) per facing [x0,y0,x1,y1] */
  playerHurt: number[][]
}

function b64(s: string): Uint8Array {
  const bin = atob(s)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

const hb = (h: J) => ['down', 'up', 'left', 'right'].map((d) => [h[d].x0, h[d].y0, h[d].x1, h[d].y1])

export function buildLog1Data(src: { characters: J; enemies: J; levels: J; maps: J; items: J; sprites: J; collision: J }): Log1Data {
  const { characters, enemies, levels, maps, items, sprites, collision } = src
  const levelRows: LevelRow[] = (levels.levels as J[]).map((l) => ({ level: l.level, exp: l.exp_required, maxHp: l.max_hp, maxKi: l.max_ki, maxFlight: l.max_flight_energy }))
  const templates: Template[] = (enemies.enemies as J[]).map((e) => ({
    id: e.id,
    name: e.name,
    exp: e.exp_reward,
    moveType: e.move_type,
    targetType: e.target_type,
    fightStyle: e.fight_style,
    groupFlag: e.group_flag,
    flying: e.flying,
    aggroTiles: e.aggro_tiles,
    leashHome: [e.leash_home_x, e.leash_home_y],
    leashRadius: e.leash_radius,
    guard: [e.guard_x, e.guard_y],
    speedClass: e.speed_class,
    ki: e.ki,
    hp: e.hp || 1,
    defense: e.defense,
    sprites: (e.sprites as J[]).map((s) => ({ name: s.sprite_name, hurt: hb(s.sprite_def.hurtbox), anims: s.sprite_def.anims_present, foot: [s.sprite_def.footprint.w, s.sprite_def.footprint.h] as [number, number], def: parseInt(s.sprite_def.def_addr, 16) })),
  }))
  const ents = sprites.entities as SpriteEntityJson[]
  const bank = new SpriteBank(ents)
  const byLabel = new Map<string, SpriteSet>()
  for (const e of ents as (SpriteEntityJson & { rom_labels?: string[] })[]) for (const l of e.rom_labels ?? []) if (!byLabel.has(l)) byLabel.set(l, bank.get(e.id)!)
  const mapImgs = new Map<number, { composite: string; size: [number, number] }>()
  for (const m of sprites.maps as J[]) mapImgs.set(m.map, { composite: m.composite, size: m.size })
  const mapInfo = new Map<number, MapInfo>()
  for (const m of maps.maps as J[]) {
    const img = mapImgs.get(m.index)
    mapInfo.set(m.index, {
      index: m.index,
      name: m.description || `Map ${m.index}`,
      composite: img?.composite ?? '',
      size: img?.size ?? [m.size_tiles.w * 8, m.size_tiles.h * 8],
      sizeTiles: [m.size_tiles.w, m.size_tiles.h],
      entrance: [m.default_entrance.x, m.default_entrance.y],
      actors: (m.actors as J[]).map((a) => ({ x: a.x, y: a.y, sprite: a.sprite, kind: a.kind, template: a.enemy_template })),
      pickups: (m.pickups as J[]).map((p) => ({ x: p.x, y: p.y, item: p.item_id })),
    })
  }
  const col = new Map<number, CollisionMap>()
  for (const [k, v] of Object.entries(collision.maps as Record<string, J>)) col.set(Number(k), { w: v.w, h: v.h, layers: (v.layers as string[]).map(b64) })
  const itemMap = new Map<number, { id: number; kind: string; w: number; h: number; sprite?: SpriteSet }>()
  for (const it of items.items as J[]) itemMap.set(it.id, { id: it.id, kind: it.kind, w: it.size_px.w, h: it.size_px.h, sprite: bank.get(`item_${String(it.id).padStart(2, '0')}`) })
  const goku = (characters.characters as J[])[0]
  return { levels: levelRows, templates, sprites: bank, byLabel, maps: mapInfo, collision: col, items: itemMap, playerHurt: hb(goku.hurtbox) }
}

let cached: Promise<Log1Data> | null = null
export function loadLog1(): Promise<Log1Data> {
  if (!cached) {
    cached = (async () => {
      const names = ['characters', 'enemies', 'levels', 'maps', 'items', 'sprites'] as const
      const vals = (await Promise.all(names.map((n) => loadData('log1', n)))) as J[]
      const collision = (await loadData('battle', 'log1_collision')) as J
      const [characters, enemies, levels, maps, items, sprites] = vals
      return buildLog1Data({ characters, enemies, levels, maps, items, sprites, collision })
    })()
  }
  return cached
}

/** level index (0-based) from experience, 0x08005C32 (row 0 has exp -1) */
export function levelFromExp(levels: LevelRow[], exp: number): number {
  let i = 1
  while (i < levels.length && levels[i].exp <= exp) i++
  return i - 1
}
