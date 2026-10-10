// Loads the reverse-engineered Buu's Fury tables (data/buu/*.json + data/battle/buu_ai_extra.json + buu_collision.json) and shapes them
// for the simulator. Numbers come from the JSON; anything the data does not give is marked where it is used.

import { loadData } from '../../../data'
import { SpriteBank, type SpriteEntityJson, type SpriteSet } from '../../core/sprites'
import { parseCollision, type CollisionGrid } from './collision'
import type { EAnimRule } from './eanim'
import { setFrameBoxes } from './actor'

type J = Record<string, any> // eslint-disable-line @typescript-eslint/no-explicit-any

/** Buu gfx ids are sprite_index + 9 (site/src/lib/sprites.ts) */
export const GFX_OFFSET = 9

export interface BuuChar {
  index: number
  name: string
  level: number
  hp: number
  ep: number
  STR: number
  POW: number
  END: number
  SPD: number
  form: number
  skills: number[]
  xp: number
  /** level-up growth row g[8] (0x08013678) */
  growth: number[]
  forms: number[]
}

export interface BuuForm {
  id: number
  /** the character's name (form +2) */
  name: string
  /** the game's name for this look (Dojo roster names, data/battle/buu_form_names.json): 'SS Goku', 'Mystic Gohan' */
  label: string
  /** the upper-case name of the HUD / status screen (form +4) */
  hud: string
  char: number
  flags: number
  gfx: number
  up: number
  down: number
  /** measured bonuses on top of the record stats (characters.json measured_bonus) */
  bonus: { STR: number; POW: number; END: number; END2: number; walk: number; dash: number }
  portrait: number
  ss: boolean
  ss3: boolean
  fusion: boolean
  /** the form controller object (0x03001B60.. : SS gauge, idle state, fusion timer ...) */
  controller: number
}

export interface SkillDef {
  id: number
  name: string
  description: string
  maxLevel: number
  /** per level cost table (Lv0..2) or a single fixed cost */
  costs: number[]
  minEp: number
  icon: string
  iconDim: string
}

export interface EquipDef {
  id: number
  name: string
  slot: 'body' | 'hands' | 'feet' | 'accessory'
  level: number
  STR: number
  POW: number
  END: number
  END2: number
  SPD: number
  xp: number
  critPct: number
  blockHalf: boolean
  energyCostHalf: boolean
  energyBlockHalf: boolean
  ssCooldown78: boolean
  thorns: number
  hpRegen256: number
  /** script #2 hooks (melee hit that deals damage) */
  meleeEp: number
  meleeHpInstead: boolean
  luck: number
  note: string
  icon?: string
}

export interface EnemyType {
  type: number
  name: string
  named: boolean
  /** the game has no name for this type: `name` is ours */
  descriptiveName: boolean
  /** the game's own text for this type (database / ROM string), null when it has none */
  gameName: string | null
  hp: number
  exp: number
  ai: number
  STR: number
  POW: number
  END: number
  resM: number
  resE: number
  standardReaction: boolean
  gfx: number | null
  sprite?: SpriteSet
  deathName: string
  deathParam: number
  spawns: { world: number; map: number; x: number; y: number }[]
  database: string
  placeholderGfx?: boolean
  /** animation state rules per raw index (data/battle/buu_enemy_anim_sim.json; eanim.ts) */
  anim?: Record<string, EAnimRule>
}

export interface AiNodeJ {
  addr: string
  fn: string
  name: string
  /** the label in enemy_ai.json when the simulator uses a more accurate name (AI_RENAME) */
  dataName?: string
  kind?: string
  onHit?: string
  params?: Record<string, { raw: number; [k: string]: unknown }>
  children?: AiNodeJ[]
  raw_words?: string[]
  invalid?: boolean
}

export interface BuuData {
  chars: BuuChar[]
  forms: BuuForm[]
  skills: Map<number, SkillDef>
  equipment: EquipDef[]
  xpTable: number[]
  enemies: EnemyType[]
  /** behaviour root per AI state */
  states: Map<number, AiNodeJ>
  /** on-hit (aux) tree per AI state */
  aux: Map<number, AiNodeJ>
  bosses: J
  projectiles: J
  attacks: J
  sprites: SpriteBank
  maps: Map<string, { composite: string; size: [number, number] }>
  /** walk / projectile tile grids per map id ("world-map") */
  collision: Map<string, CollisionGrid>
  /** world position of each map composite's top-left pixel (measured; default 0, 0) */
  origins: Map<string, [number, number]>
  portraits: Map<number, string>
  formulas: J
}

/** node types whose enemy_ai.json label does not match what the game does (emulator-verified, tools/battle/
 *  gen_buu_step_traces.py): 0x08018E29 / update 0x08018C3A moves AWAY from the player while facing it */
export const AI_RENAME: Record<string, string> = {
  StepTowardPlayer: 'BackOffFacingPlayer',
  StepTowardPlayerThroughRocks: 'BackOffFacingPlayerThroughRocks',
}
function renameAi(n: AiNodeJ): AiNodeJ {
  const to = AI_RENAME[n.name]
  const children = n.children?.map(renameAi)
  return { ...n, ...(to ? { name: to, dataName: n.name } : {}), ...(children ? { children } : {}) }
}

/** enemies.json display_name: the game's own name (+ the qualifier that tells same-named types apart), or a
 *  descriptive name for types the game never names (display_name_source 'descriptive') */
function enemyName(e: J): string {
  return e.display_name ?? e.name ?? `Type 0x${e.type.toString(16).toUpperCase()}`
}

const FUSION_FLAG = 0x04

/** the game's frame durations of the player's animations (data/battle/buu_anim_timing.json `durations`, from the
 *  state each SetAnim id runs) on the sprites the forms use: sprite previews show the real timing (the battle itself
 *  runs the state machines, panim.ts) */
function playerAnimTiming(ents: SpriteEntityJson[], forms: J[], timing: J | undefined): SpriteEntityJson[] {
  const dur = timing?.durations as Record<string, { each?: number; two_frame?: number[]; other?: number; loop: boolean }> | undefined
  if (!dur) return ents
  const ids = new Set(forms.map((f) => (f.gfx_id as number) - 9))
  return ents.map((e) => {
    if (e.sprite_index == null || !ids.has(e.sprite_index)) return e
    const animations = Object.fromEntries(
      Object.entries(e.animations).map(([k, a]) => {
        const d = dur[String(a.raw_index)]
        if (!d) return [k, a]
        const directions = Object.fromEntries(
          Object.entries(a.directions).map(([dk, steps]) => [
            dk,
            (steps ?? []).map((s, i, arr) => ({ ...s, duration_frames: d.two_frame && arr.length === 2 ? d.two_frame[i] : (d.each ?? d.other ?? s.duration_frames) })),
          ]),
        )
        return [k, { ...a, loop: d.loop, timing_source: 'game state code (data/battle/buu_anim_timing.json)', directions }]
      }),
    )
    return { ...e, animations }
  })
}

export function buildBuuData(src: { characters: J; levels: J; moves: J; items: J; enemies: J; ai: J; bosses: J; attacks: J; sprites: J; formulas: J; extra: J; collision?: J; formNames?: J; animTiming?: J; enemyAnim?: J; frameBoxes?: J }): BuuData {
  setFrameBoxes(src.frameBoxes)
  const { characters, levels, moves, items, enemies, ai, bosses, attacks, sprites, formulas, extra, collision } = src
  const ents = playerAnimTiming(sprites.entities as SpriteEntityJson[], characters.forms as J[], src.animTiming)
  const bank = new SpriteBank(ents.filter((e) => e.category !== 'portrait'))
  const portraits = new Map<number, string>()
  for (const e of ents as (SpriteEntityJson & { portrait_index?: number })[]) if (e.category === 'portrait' && e.portrait_index != null) portraits.set(e.portrait_index, e.sheet)

  const chars: BuuChar[] = (characters.characters as J[]).map((c) => ({
    index: c.index,
    name: c.name,
    level: c.new_game.level,
    hp: c.new_game.hp,
    ep: c.new_game.ep,
    STR: c.new_game.STR,
    POW: c.new_game.POW,
    END: c.new_game.END,
    SPD: c.new_game.SPD,
    form: c.new_game.form,
    skills: c.new_game.skills,
    xp: c.new_game.xp,
    growth: c.growth.raw_u32,
    forms: c.forms,
  }))
  const names = (src.formNames?.forms ?? {}) as Record<string, { name: string; hud: string }>
  const forms: BuuForm[] = (characters.forms as J[]).map((f) => {
    const mb = f.measured_bonus
    const gn = names[String(f.index)]
    return {
      id: f.index,
      name: f.name || chars[f.char_index]?.name || `form ${f.index}`,
      label: gn?.name || f.name || chars[f.char_index]?.name || `form ${f.index}`,
      hud: gn?.hud || f.upper_name || '',
      char: f.char_index,
      flags: f.flags,
      gfx: f.gfx_id,
      up: f.transform_up,
      down: f.transform_down,
      bonus: { STR: mb.STR, POW: mb.POW, END: mb.END, END2: mb.END_energy, walk: mb.walk_speed, dash: mb.dash_speed },
      // portrait index = aux byte - 8 (as LoG2's scouter ids)
      portrait: f.portrait_or_anim_bytes[0] - 8,
      ss: !!(f.flags & 1),
      ss3: !!(f.flags & 2),
      fusion: !!(f.flags & FUSION_FLAG),
      controller: parseInt(String(f.controller_slot), 16),
    }
  })
  const skills = new Map<number, SkillDef>()
  for (const s of moves.skills as J[]) {
    const ep = s.ep ?? {}
    const costs: number[] = ep.ep_cost_by_level ?? [ep.canuse_min_ep ?? ep.spend ?? 0]
    skills.set(s.id, {
      id: s.id,
      name: s.name,
      description: s.description,
      maxLevel: s.max_level ?? 0,
      costs,
      minEp: ep.canuse_min_ep ?? costs[0] ?? 0,
      icon: `assets/buu/battle/icons/skill_${s.id}_n.png`,
      iconDim: `assets/buu/battle/icons/skill_${s.id}_alt.png`,
    })
  }
  const equipment: EquipDef[] = (items.equipment as J[]).map((e) => {
    const eff: J = Object.assign({}, ...(e.script_effects as J[]))
    const other: string = e.other_effects ?? ''
    const luck = /\+3 drop/.test(other) ? 3 : /\+2 drop|effectively \+2/.test(other) ? 2 : /-3 drop/.test(other) ? -3 : /-2 drop/.test(other) ? -2 : /-1 drop/.test(other) ? -1 : 0
    return {
      id: e.id,
      name: e.name,
      slot: e.slot,
      level: e.required_level,
      STR: e.STR,
      POW: e.POW,
      END: e.END,
      END2: e.END_energy,
      SPD: e.SPD,
      xp: e.exp_bonus_256ths,
      critPct: eff.crit_chance_percent ?? 0,
      blockHalf: !!eff.block_damage_factor,
      energyCostHalf: !!eff.energy_attack_cost,
      energyBlockHalf: !!eff.energy_block_cost,
      ssCooldown78: !!eff.ss_cooldown_rate,
      thorns: eff.thorns_damage ?? 0,
      hpRegen256: eff.hp_regen_per_frame_256ths ?? 0,
      meleeEp: /EP \+2 instead/.test(other) ? 2 : /extra EP \+1/.test(other) ? 1 : 0,
      meleeHpInstead: /HP \+1 instead/.test(other),
      luck,
      note: [other, ...(e.script_effects as J[]).map((x) => JSON.stringify(x))].filter(Boolean).join('; '),
    }
  })
  const xpTable = (levels.levels as J[]).map((l) => l.total_xp as number)
  const deathOf = extra.death as Record<string, J>
  const enemiesOut: EnemyType[] = (enemies.enemies as J[]).map((e) => {
    const d = deathOf['0x' + e.type.toString(16)] ?? { name: 'SelfDestruct', param: 8 }
    return {
      type: e.type,
      name: enemyName(e),
      named: !!e.name,
      descriptiveName: e.display_name_source === 'descriptive' || !e.name,
      gameName: e.display_name_source === 'descriptive' ? null : (e.name ?? null),
      hp: e.hp,
      exp: e.exp,
      ai: e.ai_state,
      STR: e.STR,
      POW: e.POW,
      END: e.END,
      resM: e.resist_melee,
      resE: e.resist_energy,
      standardReaction: /^standard/.test(e.hit_reaction ?? ''),
      gfx: e.gfx_ids?.[0] ?? null,
      deathName: d.name,
      deathParam: d.param,
      spawns: (e.spawn_points ?? []).map((s: J) => ({ world: s.world, map: s.map, x: s.x, y: s.y })),
      database: e.database?.[0]?.description ?? '',
      anim: src.enemyAnim?.types?.['0x' + e.type.toString(16).toUpperCase().padStart(2, '0')],
    }
  })
  // variants without graphics borrow the graphics of a type with the same name / the next type (spawned by bosses)
  // Janemba body parts carry their graphics in the spawn records (JanembaSpawnParts 0x08039D7B), not the type table
  // (Worm Body: its spawn template 0x08756A2C, +0x10 = 0x129)
  const PART_GFX: Record<number, number> = { 0x34: 0x93, 0x35: 0x94, 0x37: 0x98, 0x8f: 0x129 }
  // types with no graphics in the type / spawn tables: borrow a family member's graphics (placeholders, marked
  // `placeholderGfx`): Vegeta NPC -> Majin Vegeta, unnamed fighters / helpers -> Human / Majin Fighter; Mega Cyborg /
  // Super Bio Mech get the game's default graphics 173 (what the emulator draws for them)
  const PLACEHOLDER: Record<number, number> = { 0x4a: 173, 0x71: 173, 0x89: 282, 0x87: 36, 0x1e: 173, 0x22: 173, 0x72: 173, 0x7e: 173 }
  for (const e of enemiesOut) {
    if (PART_GFX[e.type]) e.gfx = PART_GFX[e.type]
    if (e.gfx != null && e.gfx < 9) e.gfx = null
    if (e.gfx == null && PLACEHOLDER[e.type]) {
      e.gfx = PLACEHOLDER[e.type]
      e.placeholderGfx = true
    }
    if (e.gfx == null) {
      const same = enemiesOut.find((o) => o.gfx != null && o.name.replace(/ \(variant\).*/, '') === e.name.replace(/ \(variant\).*/, ''))
      const prev = enemiesOut[e.type - 1]
      e.gfx = same?.gfx ?? (prev && prev.gfx != null && /variant|Kid Buu|Broly|Super Buu|Worm/.test(e.name) ? prev.gfx : null)
      if (e.gfx == null && e.type === 0x8f) e.gfx = enemiesOut[0x8e].gfx
      if (e.gfx == null && e.type === 0x90) e.gfx = enemiesOut[0x8e].gfx
    }
    if (e.gfx != null) e.sprite = bank.index(e.gfx - GFX_OFFSET)
  }
  const states = new Map<number, AiNodeJ>()
  for (const [k, v] of Object.entries(ai.states as Record<string, J>)) states.set(parseInt(k, 16), renameAi(v.tree))
  for (const [k, v] of Object.entries(extra.reactions as Record<string, AiNodeJ>)) if (!states.has(parseInt(k, 16))) states.set(parseInt(k, 16), renameAi(v))
  const aux = new Map<number, AiNodeJ>()
  for (const [k, v] of Object.entries(extra.aux as Record<string, AiNodeJ>)) aux.set(parseInt(k, 16), renameAi(v))
  const maps = new Map<string, { composite: string; size: [number, number] }>()
  for (const m of sprites.maps as J[]) if (m.composite && m.size) maps.set(m.map, { composite: m.composite, size: m.size })
  return {
    chars,
    forms,
    skills,
    equipment,
    xpTable,
    enemies: enemiesOut,
    states,
    aux,
    bosses: bosses.bosses,
    projectiles: bosses.projectiles,
    attacks,
    sprites: bank,
    maps,
    collision: parseCollision(collision),
    origins: new Map(Object.entries((collision?.maps ?? {}) as Record<string, J>).filter(([, v]) => v.origin).map(([k, v]) => [k, v.origin as [number, number]])),
    portraits,
    formulas,
  }
}

let loaded: Promise<BuuData> | null = null

export function loadBuu(): Promise<BuuData> {
  if (!loaded) {
    loaded = (async () => {
      const names = ['characters', 'levels', 'moves', 'items', 'enemies', 'enemy_ai', 'bosses', 'attacks_detail', 'sprites', 'formulas'] as const
      const [vals, extra, collision, formNames, animTiming, enemyAnim, frameBoxes] = await Promise.all([Promise.all(names.map((n) => loadData('buu', n))), loadData('battle', 'buu_ai_extra'), loadData('battle', 'buu_collision'), loadData('battle', 'buu_form_names'), loadData('battle', 'buu_anim_timing'), loadData('battle', 'buu_enemy_anim_sim'), loadData('battle', 'buu_frame_boxes')])
      const v = Object.fromEntries(names.map((n, i) => [n, vals[i]])) as Record<string, J>
      return buildBuuData({
        characters: v.characters, levels: v.levels, moves: v.moves, items: v.items, enemies: v.enemies, ai: v.enemy_ai,
        bosses: v.bosses, attacks: v.attacks_detail, sprites: v.sprites, formulas: v.formulas, extra: extra as J, collision: collision as J, formNames: formNames as J, animTiming: animTiming as J, enemyAnim: enemyAnim as J, frameBoxes: frameBoxes as J,
      })
    })()
  }
  return loaded
}
