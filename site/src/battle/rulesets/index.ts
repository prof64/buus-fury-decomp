// Registry of game rulesets. To add a game: implement `Ruleset` (see ../RULESET.md) and replace its stub here.

import type { GameKey, Ruleset, RulesetInfo } from '../core/ruleset'
import { log1Ruleset } from './log1'
import { log2Ruleset } from './log2'
import { buuRuleset } from './buu'

export const RULESET_INFO: RulesetInfo[] = [
  log1Ruleset.info,
  log2Ruleset.info,
  buuRuleset.info,
]

const RULESETS: Partial<Record<GameKey, Ruleset>> = {
  log1: log1Ruleset,
  log2: log2Ruleset,
  buu: buuRuleset,
}

export function getRuleset(id: GameKey): Ruleset | undefined {
  return RULESETS[id]
}
