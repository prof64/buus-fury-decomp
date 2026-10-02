// Registry of game rulesets. To add a game: implement `Ruleset` (see ../RULESET.md) and replace its stub here.

import type { GameKey, Ruleset, RulesetInfo } from '../core/ruleset'
import { log2Ruleset } from './log2'

export const RULESET_INFO: RulesetInfo[] = [
  { id: 'log1', title: 'The Legacy of Goku', short: 'LoG I', enabled: false, note: 'coming soon' },
  log2Ruleset.info,
  { id: 'buu', title: "Buu's Fury", short: "Buu's Fury", enabled: false, note: 'coming soon' },
]

const RULESETS: Partial<Record<GameKey, Ruleset>> = {
  log2: log2Ruleset,
}

export function getRuleset(id: GameKey): Ruleset | undefined {
  return RULESETS[id]
}
