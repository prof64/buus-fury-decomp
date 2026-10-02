// The contract between the game-agnostic engine/UI and a game ruleset (LoG1, LoG2, Buu's Fury).
// Documented in ../RULESET.md.

import type { ButtonName, InputFrame } from './input'
import type { Renderer } from './renderer'
import type { Rng } from './rng'
import type { ArenaRuntime, World } from './world'
import type { Rect } from './geom'
import type { SpriteBank } from './sprites'

export type GameKey = 'log1' | 'log2' | 'buu'

export interface RulesetInfo {
  id: GameKey
  title: string
  short: string
  enabled: boolean
  note?: string
}

/** Something the setup screen can show a picture of: a sprite entity id + animation, or a static image. */
export interface Visual {
  sprite?: string
  anim?: string
  dir?: number
  image?: string
}

export interface FormChoice {
  id: number
  name: string
  visual: Visual
  portrait?: string
  /** short description of what the form changes */
  note?: string
}

export interface FighterChoice {
  id: string
  name: string
  portrait?: string
  visual: Visual
  forms: FormChoice[]
  defaultForm: number
  /** move list for the setup screen */
  moves: { button: string; name: string; note?: string }[]
}

export interface EnemyChoice {
  id: string
  name: string
  /** e.g. 'Boss', 'Enemy', 'NPC / scouter row', 'Unused row' */
  group: string
  visual: Visual
  hp: number
  stats: Record<string, number | string>
  tags: string[]
  note?: string
  /** true when the AI is the real decoded state machine; false = fallback */
  faithfulAi: boolean
}

export interface ArenaChoice {
  id: string
  name: string
  image: string
  size: [number, number]
  bounds: Rect
  /** part of the image the camera may show (fully painted background); default: the whole image */
  view?: Rect
  note?: string
}

export interface PresetChoice {
  id: string
  name: string
  note?: string
  /** enemy ids in fight order (phases) */
  phases: { enemy: string; count: number }[]
  arena?: string
  fighter?: string
}

/** Player-side setup chosen on the setup screen. Ruleset-specific knobs live in `options`. */
export interface FighterSetup {
  fighter: string
  form: number
  level: number
  /** extra stat points (e.g. LoG2 STR/PWR/END Capsules) */
  bonus: Record<string, number>
  options: Record<string, number | string | boolean>
}

export interface BattleConfig {
  game: GameKey
  fighter: FighterSetup
  /** fought in order: each phase spawns `count` copies; the next phase starts when all are defeated */
  phases: { enemy: string; count: number }[]
  arena: string
  seed: number
  options: Record<string, number | string | boolean>
}

export interface StatLine {
  label: string
  value: string | number
  hint?: string
}

export interface SetupKnob {
  key: string
  label: string
  kind: 'number' | 'select' | 'boolean'
  min?: number
  max?: number
  options?: { value: string; label: string }[]
  default: number | string | boolean
  hint?: string
  /** stored in FighterSetup.bonus instead of options */
  bonus?: boolean
}

export interface ControlHelp {
  button: ButtonName | 'DPAD'
  action: string
}

export type Outcome = 'running' | 'won' | 'lost'

/** A live battle created by a ruleset inside a World. */
export interface RulesetBattle {
  /** called every frame before the world updates its entities */
  preStep?(w: World, input: InputFrame): void
  /** called every frame after the entities updated (camera, phase changes, regen...) */
  postStep?(w: World): void
  drawHud(r: Renderer, w: World): void
  outcome(w: World): Outcome
  /** text lines for the debug panel (AI node per enemy, player state, ...) */
  debugLines(w: World): string[]
  /** result-screen extras */
  summary(w: World): StatLine[]
}

export interface Ruleset {
  info: RulesetInfo
  /** load data + sprites (idempotent) */
  load(): Promise<void>
  fighters(): FighterChoice[]
  enemies(): EnemyChoice[]
  arenas(): ArenaChoice[]
  presets(): PresetChoice[]
  levelRange: [number, number]
  /** extra knobs shown on the setup screen (stat bonuses, growth mode, options) */
  knobs(): SetupKnob[]
  /** stats the fighter will start with (setup preview) */
  preview(setup: FighterSetup): StatLine[]
  controls(): ControlHelp[]
  createRng(seed: number): Rng
  /** spawn everything into the world; the world is already created with arena + rng */
  createBattle(cfg: BattleConfig, w: World): RulesetBattle
  arenaRuntime(id: string): ArenaRuntime
  /** sprite sets for previews (Visual.sprite ids) */
  sprites(): SpriteBank
}
