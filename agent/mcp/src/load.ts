// Loads the reverse-engineered tables from <repo>/data and builds the Buu ruleset data (same as the vitest harness,
// but from the file system instead of Vite JSON imports).
import { readFileSync, existsSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildBuuData, type BuuData } from '../../../site/src/battle/rulesets/buu/data'
import { buuRuleset } from '../../../site/src/battle/rulesets/buu'
import { Actor } from '../../../site/src/battle/rulesets/buu/actor'

/** repository root: $BUU_REPO, else walk up from this file until data/buu exists */
export function repoRoot(): string {
  if (process.env.BUU_REPO) return resolve(process.env.BUU_REPO)
  let d = dirname(fileURLToPath(import.meta.url))
  for (let i = 0; i < 8; i++) {
    if (existsSync(resolve(d, 'data/buu/characters.json'))) return d
    d = resolve(d, '..')
  }
  throw new Error('cannot find the repository root (set BUU_REPO)')
}

const json = (p: string) => JSON.parse(readFileSync(resolve(repoRoot(), p), 'utf8'))

let data: BuuData | null = null
export function buuData(): BuuData {
  if (data) return data
  const b = (n: string) => json(`data/buu/${n}.json`)
  const x = (n: string) => json(`data/battle/${n}.json`)
  data = buildBuuData({
    characters: b('characters'), levels: b('levels'), moves: b('moves'), items: b('items'), enemies: b('enemies'), ai: b('enemy_ai'),
    bosses: b('bosses'), attacks: b('attacks_detail'), sprites: b('sprites'), formulas: b('formulas'),
    extra: x('buu_ai_extra'), collision: x('buu_collision'), formNames: x('buu_form_names'), animTiming: x('buu_anim_timing'),
  })
  buuRuleset.d = data
  Actor.bank = data.sprites
  return data
}

let sfxNames: Map<number, string> | null = null
/** SFX id -> name (data/buu/audio.json) */
export function sfxName(id: number): string {
  if (!sfxNames) {
    sfxNames = new Map()
    try {
      for (const s of json('data/buu/audio.json').sfx ?? []) sfxNames.set(s.id, s.name)
    } catch {
      /* optional */
    }
  }
  return sfxNames.get(id) ?? `sfx ${id}`
}

export { buuRuleset }
