// Headless LoG2 battle harness for tests: builds the data tables from the JSON files and runs the real entity code
// without a renderer.
import characters from '../../../data/log2/characters.json'
import moves from '../../../data/log2/moves.json'
import enemies from '../../../data/log2/enemies.json'
import ai from '../../../data/log2/ai.json'
import bosses from '../../../data/log2/bosses.json'
import sprites from '../../../data/log2/sprites.json'
import { buildLog2Data, type Log2Data } from '../../src/battle/rulesets/log2/data'
import { World } from '../../src/battle/core/world'
import { Log2Rng } from '../../src/battle/rulesets/log2/rng'
import { Log2Ctx } from '../../src/battle/rulesets/log2/context'
import { Player } from '../../src/battle/rulesets/log2/player'
import { Enemy } from '../../src/battle/rulesets/log2/enemy'
import type { Record8 } from '../../src/battle/rulesets/log2/progression'

let data: Log2Data | null = null
export function log2Data(): Log2Data {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if (!data) data = buildLog2Data({ characters, moves, enemies, ai, bosses, sprites } as any)
  return data
}

export interface Sim {
  w: World
  ctx: Log2Ctx
  player: Player
  enemies: Enemy[]
  run(frames: number, keys?: (f: number) => number): void
}

/** Lv40 Piccolo with the stats used by bs_measure.py (STR 40 / POW 55 / END 53), standing still. */
export function piccoloLv40(): { rec: Record8; char: number; form: number } {
  return { char: 1, form: 6, rec: { hp: 9999, maxHp: 9999, ep: 137, maxEp: 137, STR: 40 << 8, POW: 55 << 8, END: 53 << 8, exp: 2727775 } }
}

export function makeSim(opts: { classId: number; count?: number; dist: number; seed?: number; char?: number; form?: number; rec?: Record8; bounds?: { x0: number; y0: number; x1: number; y1: number } }): Sim {
  const d = log2Data()
  const rng = new Log2Rng(opts.seed ?? 12345, (opts.seed ?? 12345) * 3 + 1)
  const bounds = opts.bounds ?? { x0: 0, y0: 0, x1: 1000, y1: 1000 }
  const w = new World(rng, { id: 't', name: 't', image: '', size: [1000, 1000], bounds })
  const ctx = new Log2Ctx(d, { exp: false, popups: false })
  const p = piccoloLv40()
  const ci = opts.char ?? p.char
  const player = new Player(ctx, { char: d.chars[ci], form: opts.form ?? p.form, record: opts.rec ?? p.rec, level: 40, slot: 3 })
  player.x = 400 << 8
  player.y = 500 << 8
  player.facing = 3
  ctx.player = player
  w.add(player)
  const c = d.classes[opts.classId]
  const list: Enemy[] = []
  for (let i = 0; i < (opts.count ?? 1); i++) {
    const e = new Enemy(ctx, c, c.gfx[0] ?? 60, c.name)
    // horizontal distance between the collision-box centres
    e.x = (400 + 8 + opts.dist - (c.collision[0] >> 1)) << 8
    e.y = (500 + 4 - (c.collision[1] >> 1) + i * 30) << 8
    e.facing = 2
    w.add(e)
    e.start(w)
    list.push(e)
  }
  return {
    w,
    ctx,
    player,
    enemies: list,
    run(frames, keys) {
      for (let f = 0; f < frames; f++) {
        const held = keys ? keys(f) : 0
        const prev = (this as unknown as { _prev?: number })._prev ?? 0
        ;(this as unknown as { _prev?: number })._prev = held
        w.step({ held, pressed: held & ~prev, released: prev & ~held })
        w.centerCamera(player.px + 8, player.py - 8)
      }
    },
  }
}
