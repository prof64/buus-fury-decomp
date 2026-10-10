// Loads the reverse-engineered LoG2 tables (data/log2/*.json) and shapes them for the simulator.
// Nothing here is invented: every number comes from the JSON (which the extractors read from the ROM); where a
// value is not in the data the simulator code says so next to the constant.

import { loadData } from '../../../data'
import { SpriteBank, type SpriteEntityJson } from '../../core/sprites'
import type { Growth, Record8 } from './progression'

type J = Record<string, any> // eslint-disable-line @typescript-eslint/no-explicit-any

export interface FormDef {
  id: number
  label: string
  transformed: boolean
  transformLocked: boolean
  actorId: number
  portraitIndex: number
  transformTo: number
  revertTo: number
  smLenQuads: number
  smChargeQuads: number
  smMask: number
  /** 24.8 px/frame */
  smLunge: number
  controller: ControllerKey
  hurt: number[][] // per facing [x0,y0,x1,y1]
}

export type ControllerKey = 'normal' | 'fullpower' | 'bulky' | 'ssj' | 'piccolo'
export interface ControllerDef {
  key: ControllerKey
  name: string
  bonus: [number, number, number] | null
  override: [number, number, number] | null
  /** 24.8 px/frame */
  walk: number
  run: number
}

export interface MoveTypeDef {
  id: number
  name: string
  minEp: number
  cost: number
  description: string
  icon: string
  iconDim: string
}

export interface CharDef {
  index: number
  name: string
  initial: Record8
  initialForm: number
  growth: Growth
  slots: number[]
  slotNames: string[]
  specialMelee: string
  forms: number[]
}

export interface EnemyClass {
  id: number
  name: string
  names: string[]
  hp: number
  exp: number
  STR: number
  POW: number
  END: number
  meleeMult: number
  energyMult: number
  aiState: number
  deathNode: string
  gfx: number[]
  collision: [number, number]
  hurt: number[][]
  description: string
  spawnCount: number
  boss: boolean
  attacks: string[]
  hasGraphics: boolean
}

export interface AiNode {
  addr: string
  type: string
  children?: AiNode[]
  state?: number
  frames?: number
  p_65536?: number
  radius_sq?: number
  radius_px?: number
  speed_256ths?: number
  param?: number
  attack_id_or_type?: number
  heal_frac_65536?: number
  data?: string
}
export interface AiState {
  id: number
  behaviour: AiNode
  onHit: AiNode
  text: string[]
}

export interface FightPhase {
  classRow: number
  map: number
  mapName: string
  player: string | null
}
export interface FightDef {
  key: string
  name: string
  notes: string
  phases: FightPhase[]
}

export interface MapImage {
  map: number
  composite: string
  size: [number, number]
  name: string
}

export interface Log2Data {
  chars: CharDef[]
  forms: FormDef[]
  controllers: Record<ControllerKey, ControllerDef>
  moves: MoveTypeDef[]
  expTable: number[]
  classes: EnemyClass[]
  ai: AiState[]
  fights: FightDef[]
  sprites: SpriteBank
  maps: Map<number, MapImage>
  portraits: Map<number, string>
  vectors?: J
}

const CTRL_BY_RAM: Record<string, ControllerKey> = {
  '0x30021e0': 'normal',
  '0x30021e4': 'fullpower',
  '0x30021e8': 'bulky',
  '0x30021ec': 'ssj',
  '0x30021f0': 'piccolo',
}

const fx = (v: number) => Math.round(v * 256)
/** 'Cell, "Perfect"' -> 'Perfect Cell' */
export const prettyName = (n: string) => n.replace(/^(.*), "(.*)"$/, '$2 $1')

export function buildLog2Data(src: { characters: J; moves: J; enemies: J; ai: J; bosses: J; sprites: J }): Log2Data {
  const { characters, moves, enemies, ai, bosses, sprites } = src
  const controllers = {} as Record<ControllerKey, ControllerDef>
  for (const [ram, c] of Object.entries(characters.form_controllers as Record<string, J>)) {
    const key = CTRL_BY_RAM[ram]
    controllers[key] = {
      key,
      name: c.name,
      bonus: c.bonus ?? null,
      override: c.stat_override ?? null,
      walk: fx(c.walk_speed),
      run: fx(c.run_speed),
    }
  }
  const forms: FormDef[] = (characters.forms as J[]).map((f) => ({
    id: f.id,
    label: f.label,
    transformed: !!(f.flags & 1),
    transformLocked: !!(f.flags & 2),
    actorId: f.actor_id,
    portraitIndex: f.aux_gfx_ids[1] - 8,
    transformTo: f.transform_to_form,
    revertTo: f.revert_to_form,
    smLenQuads: f.special_melee.anim_len_quads,
    smChargeQuads: f.special_melee.charge_anim_quads,
    smMask: f.special_melee.hit_quads_mask,
    smLunge: f.special_melee.raw_lunge,
    controller: CTRL_BY_RAM[f.controller.ram],
    hurt: ['down', 'up', 'left', 'right'].map((d) => f.hurtbox.hurtbox_by_facing_x0y0x1y1[d]),
  }))
  const moveNames: Record<number, string> = {}
  const iconOf = (id: number, alt: boolean) => `assets/log2/battle/icons/move_${id}_${alt ? 'alt' : 'n'}.png`
  const mv: MoveTypeDef[] = (moves.player_moves as J[]).map((m) => {
    const name = m.ui_name ?? String(m.name).replace(/ \(.*\)$/, '')
    moveNames[m.id] = name
    return {
      id: m.id,
      name,
      minEp: m.min_ep_to_start,
      cost: m.ep_cost_on_start,
      description: m.ui_description ?? '',
      icon: iconOf(m.id, false),
      iconDim: iconOf(m.id, true),
    }
  })
  const chars: CharDef[] = (characters.characters as J[]).map((c) => {
    const i = c.initial
    const g = c.growth_per_level
    return {
      index: c.index,
      name: c.name,
      initial: { hp: i.hp, maxHp: i.max_hp, ep: i.ep, maxEp: i.max_ep, STR: i.STR_8p8, POW: i.POW_8p8, END: i.END_8p8, exp: i.exp },
      initialForm: i.form,
      growth: {
        STR: [g.STR.min_8p8, g.STR.max_8p8],
        POW: [g.POW.min_8p8, g.POW.max_8p8],
        END: [g.END.min_8p8, g.END.max_8p8],
      },
      slots: i.slots,
      slotNames: (c.move_slots as J[]).map((s) => s.name),
      specialMelee: c.special_melee?.name ?? '',
      forms: c.forms,
    }
  })
  const classes: EnemyClass[] = (enemies.classes as J[]).map((c) => {
    const g0 = (c.graphics as J[]).find((g) => g.collision_box_wh) ?? null
    const names: string[] = (c.names ?? []).filter(Boolean)
    return {
      id: c.id,
      name: names[0] ? prettyName(names[0]) : `Class ${c.id}`,
      names,
      hp: c.hp,
      exp: c.exp,
      STR: c.STR,
      POW: c.POW,
      END: c.END,
      meleeMult: c.melee_taken_mult,
      energyMult: c.energy_taken_mult,
      aiState: c.ai_state,
      deathNode: c.death_node,
      gfx: (c.graphics as J[]).map((g) => g.gfx_id),
      collision: g0 ? g0.collision_box_wh : [16, 8],
      hurt: g0 ? ['down', 'up', 'left', 'right'].map((d) => g0.hurtbox_by_facing[d]) : [[-2, -24, 18, 10], [-2, -24, 18, 10], [-2, -24, 18, 10], [-2, -24, 18, 10]],
      description: c.scouter_entries?.[0]?.description ?? '',
      spawnCount: c.spawn_count,
      boss: !!c.is_boss_or_scripted,
      attacks: c.attacks ?? [],
      hasGraphics: !!g0,
    }
  })
  const aiStates: AiState[] = (ai.states as J[]).map((s) => ({ id: s.id, behaviour: s.behaviour, onHit: s.on_hit, text: s.text }))
  const fights: FightDef[] = (bosses.fights as J[]).map((f) => ({
    key: f.key,
    name: f.name,
    notes: f.notes ?? '',
    phases: (f.phases as J[]).map((p) => ({ classRow: p.class_row, map: p.map_index, mapName: p.map_name, player: p.player_character ?? null })),
  }))
  const mapImgs = new Map<number, MapImage>()
  for (const m of sprites.maps as J[]) mapImgs.set(m.map, { map: m.map, composite: m.composite, size: m.size, name: `Map ${m.map}` })
  const ents = sprites.entities as SpriteEntityJson[]
  const portraits = new Map<number, string>()
  for (const e of ents as (SpriteEntityJson & { portrait_index?: number })[]) if (e.category === 'portrait' && e.portrait_index != null) portraits.set(e.portrait_index, e.id)
  return {
    chars,
    forms,
    controllers,
    moves: mv,
    expTable: characters.exp_table as number[],
    classes,
    ai: aiStates,
    fights,
    sprites: new SpriteBank(ents),
    maps: mapImgs,
    portraits,
  }
}

let loaded: Promise<Log2Data> | null = null

export function loadLog2(): Promise<Log2Data> {
  if (!loaded) {
    loaded = (async () => {
      const names = ['characters', 'moves', 'enemies', 'ai', 'bosses', 'sprites'] as const
      const vals = await Promise.all(names.map((n) => loadData('log2', n)))
      const src = Object.fromEntries(names.map((n, i) => [n, vals[i]])) as Parameters<typeof buildLog2Data>[0]
      return buildLog2Data(src)
    })()
  }
  return loaded
}
