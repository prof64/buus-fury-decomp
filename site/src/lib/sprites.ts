// Typed view of data/<game>/sprites.json (format: tools/extract/GRAPHICS.md §7) plus lookups that
// connect the graphics extraction to the mechanics data (by sprite-definition address or engine id).
import { useMemo } from 'react'
import { useData } from '../data'
import { hexNum, type J } from './util'

export interface Frame {
  x: number
  y: number
  w: number
  h: number
  anchor: { x: number; y: number }
  rom?: string
  png: string
}
export interface AnimStep {
  frame: number
  duration_frames: number
  flip: boolean
  rom?: string
}
export interface Anim {
  raw_index: number
  loop: boolean
  timing_source?: string
  /** frame orders seen in the emulator; Buu entries are sometimes pre-joined strings ("0,1,2") */
  observed_frame_orders?: (number[] | string)[] | null
  rom?: string
  directions: Partial<Record<'down' | 'up' | 'left' | 'right' | 'any', AnimStep[]>>
}
export interface Rect {
  x0: number
  y0: number
  x1: number
  y1: number
}
export interface SpriteEntity {
  id: string
  name: string
  category: string
  name_confidence?: string
  notes?: string | null
  sprite_index?: number
  portrait_index?: number
  item_id?: number
  rom?: Record<string, J>
  header?: Record<string, J>
  sheet: string
  frames: Frame[]
  animations: Record<string, Anim>
  hitboxes?: { body_per_direction?: ({ dir: string } & Rect)[]; note?: string }
  rom_labels?: string[]
}
export interface MapEntry {
  map: number | string
  composite: string
  screenshot?: string
  size: [number, number]
  tileset?: string
  rom_record?: string
  layers: J[]
}
export interface SpritesJson {
  game: string
  conventions: Record<string, string>
  entities: SpriteEntity[]
  maps: MapEntry[]
  tilesets: { id: string; png: string; tiles: number; bpp: number; first_map: J }[]
  ui_captures: J[]
}

export const CATEGORY_ORDER = ['player', 'boss', 'enemy', 'npc', 'projectile', 'effect', 'item', 'portrait']
export const CATEGORY_LABEL: Record<string, string> = {
  player: 'Playable',
  boss: 'Bosses',
  enemy: 'Enemies',
  npc: 'NPCs',
  projectile: 'Projectiles',
  effect: 'Effects',
  item: 'Items',
  portrait: 'Portraits',
}

export interface SpriteIndex {
  json: SpritesJson
  all: SpriteEntity[]
  byId: Map<string, SpriteEntity>
  /** sprite-definition ROM address -> entity */
  byDef: Map<number, SpriteEntity>
  /** engine sprite id (gfx id) -> entity; Buu gfx ids are sprite_index + 9 */
  byGfx: (gfx: number) => SpriteEntity | undefined
  portrait: (index: number) => SpriteEntity | undefined
  findByName: (name: string, cats?: string[]) => SpriteEntity | undefined
}

export function buildIndex(game: string, json: SpritesJson): SpriteIndex {
  const byId = new Map<string, SpriteEntity>()
  const byDef = new Map<number, SpriteEntity>()
  const bySprite = new Map<number, SpriteEntity>()
  const byPortrait = new Map<number, SpriteEntity>()
  for (const e of json.entities) {
    byId.set(e.id, e)
    const d = hexNum(e.rom?.def ?? e.rom?.sprite_def)
    if (d !== undefined && !byDef.has(d)) byDef.set(d, e)
    if (e.sprite_index !== undefined && e.category !== 'portrait' && !bySprite.has(e.sprite_index)) bySprite.set(e.sprite_index, e)
    if (e.category === 'portrait' && e.portrait_index !== undefined) byPortrait.set(e.portrait_index, e)
  }
  const offset = game === 'buu' ? 9 : 0
  const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, '')
  return {
    json,
    all: json.entities,
    byId,
    byDef,
    byGfx: (g) => bySprite.get(g - offset),
    portrait: (i) => byPortrait.get(i),
    findByName: (name, cats) => {
      const n = norm(name)
      const pool = json.entities.filter((e) => !cats || cats.includes(e.category))
      return pool.find((e) => norm(e.name) === n) ?? pool.find((e) => norm(e.name).startsWith(n))
    },
  }
}

export function useSprites(game: string): SpriteIndex | undefined {
  const json = useData<SpritesJson>(game, 'sprites')
  return useMemo(() => (json ? buildIndex(game, json) : undefined), [game, json])
}

/** Picks a representative still frame: idle/down if present, else the first step of the first animation. */
export function previewFrame(e: SpriteEntity): Frame | undefined {
  const anims = e.animations ?? {}
  const a = anims.idle ?? anims.default ?? anims.icon ?? anims.fly ?? Object.values(anims)[0]
  const step = a && (a.directions.down ?? a.directions.any ?? Object.values(a.directions)[0])?.[0]
  return step ? e.frames[step.frame] : e.frames[0]
}

export function bodyRect(e: SpriteEntity, dir: string): Rect | undefined {
  const r = e.hitboxes?.body_per_direction?.find((b) => b.dir === dir)
  if (!r || (r.x0 === 0 && r.x1 === 0 && r.y0 === 0 && r.y1 === 0)) return undefined
  return r
}
