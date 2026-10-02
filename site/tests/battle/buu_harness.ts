// Headless Buu's Fury battle harness: builds the data tables from the JSON files and runs the real ruleset /
// entity code without a renderer.
import characters from '../../../data/buu/characters.json'
import levels from '../../../data/buu/levels.json'
import moves from '../../../data/buu/moves.json'
import items from '../../../data/buu/items.json'
import enemies from '../../../data/buu/enemies.json'
import ai from '../../../data/buu/enemy_ai.json'
import bosses from '../../../data/buu/bosses.json'
import attacks from '../../../data/buu/attacks_detail.json'
import sprites from '../../../data/buu/sprites.json'
import formulas from '../../../data/buu/formulas.json'
import extra from '../../../data/battle/buu_ai_extra.json'
import collision from '../../../data/battle/buu_collision.json'
import { buildBuuData, type BuuData } from '../../src/battle/rulesets/buu/data'
import { buuRuleset } from '../../src/battle/rulesets/buu'
import { World } from '../../src/battle/core/world'
import { BuuRng } from '../../src/battle/rulesets/buu/rng'
import type { BattleConfig, FighterSetup, RulesetBattle } from '../../src/battle/core/ruleset'
import type { Player } from '../../src/battle/rulesets/buu/player'
import type { Enemy } from '../../src/battle/rulesets/buu/enemy'

let data: BuuData | null = null
export function buuData(): BuuData {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if (!data) data = buildBuuData({ characters, levels, moves, items, enemies, ai, bosses, attacks, sprites, formulas, extra, collision } as any)
  buuRuleset.d = data
  return data
}

export interface BuuSim {
  w: World
  battle: RulesetBattle
  player: Player
  enemies: Enemy[]
  run(frames: number, keys?: (f: number) => number): void
}

export function fighter(p: Partial<FighterSetup> = {}): FighterSetup {
  return {
    fighter: 'goku',
    form: 2,
    level: 60,
    bonus: {},
    options: { growth: 'average', autoSpend: 'none', skillLevel: '0', eq_body: '0', eq_hands: '44', eq_feet: '77', eq_accessory: '112' },
    ...p,
  }
}

export function makeBuuSim(opts: { enemy: string; count?: number; arena?: string; seed?: number; fighter?: FighterSetup; phases?: { enemy: string; count: number }[] }): BuuSim {
  buuData()
  const rs = buuRuleset
  const cfg: BattleConfig = {
    game: 'buu',
    fighter: opts.fighter ?? fighter(),
    phases: opts.phases ?? [{ enemy: opts.enemy, count: opts.count ?? 1 }],
    arena: opts.arena ?? 'forest',
    seed: opts.seed ?? 1,
    options: { popups: false },
  }
  const w = new World(new BuuRng(opts.seed ?? 12345, (opts.seed ?? 12345) * 3 + 1), rs.arenaRuntime(cfg.arena))
  const battle = rs.createBattle(cfg, w)
  const player = w.entities.find((e) => e.kind === 'player') as Player
  let prev = 0
  const sim: BuuSim = {
    w,
    battle,
    player,
    get enemies() {
      return w.entities.filter((e) => e.kind === 'enemy') as Enemy[]
    },
    run(frames, keys) {
      for (let f = 0; f < frames; f++) {
        const held = keys ? keys(f) : 0
        battle.preStep?.(w, { held, pressed: held & ~prev, released: prev & ~held })
        w.step({ held, pressed: held & ~prev, released: prev & ~held })
        prev = held
        battle.postStep?.(w)
      }
    },
  }
  return sim
}
