import { useMemo } from 'react'
import { useData } from '../data'
import { useSprites, type SpriteEntity } from '../lib/sprites'
import { enemiesOf } from './enemies'
import type { J } from '../lib/util'

export interface GameSummary {
  sprites: number
  categories: Record<string, number>
  maps: number
  enemiesUsed: number
  enemyRows: number
  bosses: number
  playable: number
  moves: number
  items: number
  aiStates?: number
  levels: string
  heroes: SpriteEntity[]
  bossSprites: SpriteEntity[]
}

const HERO_NAMES: Record<string, string[]> = {
  log1: ['Goku'],
  log2: ['Gohan', 'Piccolo', 'Vegeta', 'Trunks', 'Goku', 'Hercule'],
  buu: ['Goku', 'Gohan', 'Goten', 'Trunks', 'Vegeta', 'Gotenks'],
}

export function useGameSummary(game: string): GameSummary | undefined {
  const sp = useSprites(game)
  const enemies = useData<J>(game, 'enemies')
  const moves = useData<J>(game, 'moves')
  const items = useData<J>(game, 'items')
  const chars = useData<J>(game, 'characters')
  const ai = useData<J>(game, game === 'log2' ? 'ai' : game === 'buu' ? 'enemy_ai' : 'rng')
  const bosses = useData<J>(game, 'bosses')
  return useMemo(() => {
    if (!sp || !enemies || !moves || !items || !chars) return undefined
    const cats: Record<string, number> = {}
    for (const e of sp.all) cats[e.category] = (cats[e.category] ?? 0) + 1
    const vms = enemiesOf(game, enemies, sp)
    if (game === 'buu' && bosses?.bosses) for (const v of vms) if (bosses.bosses['0x' + Number(v.key).toString(16)]) v.boss = true
    const heroes = HERO_NAMES[game].map((n) => sp.findByName(n, ['portrait'])).filter(Boolean) as SpriteEntity[]
    const bossSprites = vms
      .filter((v) => v.boss && v.sprites[0])
      .map((v) => v.sprites[0])
      .filter((s, i, a) => a.indexOf(s) === i)
      .slice(0, 10)
    return {
      sprites: sp.all.length,
      categories: cats,
      maps: sp.json.maps.length,
      enemiesUsed: vms.filter((v) => v.used).length,
      enemyRows: vms.length,
      bosses: vms.filter((v) => v.boss).length,
      playable: chars.characters.length,
      moves: game === 'log1' ? moves.moves.length : game === 'log2' ? moves.player_attacks.length + moves.enemy_attacks.length : moves.skills.length + 1,
      items: game === 'buu' ? items.supplies.length + items.equipment.length : items.items.length,
      aiStates: game === 'log2' ? ai?.states?.length : game === 'buu' ? Object.keys(ai?.states ?? {}).length : undefined,
      levels: game === 'log1' ? '1–25' : game === 'log2' ? '1–50' : '50–200',
      heroes,
      bossSprites,
    }
  }, [sp, enemies, moves, items, chars, ai, bosses, game])
}

export const SIGNATURE: Record<string, { title: string; formula: string; blurb: string }> = {
  log1: {
    title: 'Punch',
    formula: 'dmg = (maxHP × (384 + rand2(128))) >> 9\nthen defense roll: 48% full · 25% −def/4 · 15% −def/2 · 10% −def · 2% miss',
    blurb: 'No attack stat at all: damage scales with Goku’s max HP; enemies hit for a quarter of their current HP plus a random half.',
  },
  log2: {
    title: 'computeDamage 0x0800F910',
    formula: 'b = power·(1+crit)·(128−END) >> 7\nf(S) = ((26S−701)S+115618)S >> 8\ndmg = ((((rand(26)+256)·b>>8)·f(S)>>18)·scale>>10)·MULT>>7',
    blurb: 'One routine for every hit: STR or POW through a cubic curve, END as a linear percentage cut, melee crits double power.',
  },
  buu: {
    title: 'ComputeDamage 0x0800E9D8',
    formula: 'X = (221 − END′)·P·297 >> 16\nA = S + (S²(S+100) >> 13)\nraw = ((256+rand(26))·(mult·X+1023 >> 10) >> 8)·A',
    blurb: 'A new model in 1/1024 HP: melee power ×STR/20, linear defense against 221, crits ×1.5, frontal block 1/16.',
  },
}

export const FACTS: Record<string, string[]> = {
  log1: [
    'Defense is a constant 250 for Goku at every level; enemies roll it on every hit.',
    'No enemy can take more than ⅓ of Goku’s max HP (+1) in one hit.',
    'rand() overflows for 65536+ HP enemies: Frieza and co. can roll negative damage and heal you.',
    'Hidden invincibility code during the Shenron intro: ↑ ↓ ← → B A.',
    'All enemies share one AI routine configured by move type, target type and fight style.',
  ],
  log2: [
    'Kill exp is clamped to 1/128 … 1/2 of your current level’s span.',
    'Benched party members regenerate 5% HP and EP every 596 frames; the active one only 3% EP.',
    'Special Beam Cannon pierces, Kamehameha only damages through its 16×16 head.',
    'Android 19/20 absorb energy attacks and heal 20% of the raw damage.',
    'Hidden debug menu: title option 2 → Music / Sample / Map Test with a level-40 party.',
  ],
  buu: [
    'Stats never grow on their own: 3 points per level, spent on STR/POW/END/SPD.',
    'Walk speed = 5·SPD + 102 (1/256 px/f); Super Saiyan adds +128, SS3 +192.',
    'Loot quirk: after the first coin, every remaining roll of a kill is also a coin.',
    'Fusions get (A+B)·154/256 HP and the average stats +10.',
    'Same RNG constants as LoG II; the damage formula is new.',
  ],
}
