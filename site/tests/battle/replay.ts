// Replays the player-move measurement runs recorded in the emulator (re/log2/pl_measurements.json, pl_measure.py)
// in the simulator and returns both hit lists for comparison.
import meas from '../../../re/log2/pl_measurements.json'
import { World } from '../../src/battle/core/world'
import { BTN } from '../../src/battle/core/input'
import { Log2Rng } from '../../src/battle/rulesets/log2/rng'
import { Log2Ctx } from '../../src/battle/rulesets/log2/context'
import { Player } from '../../src/battle/rulesets/log2/player'
import { Enemy } from '../../src/battle/rulesets/log2/enemy'
import { facingToward, type Dir } from '../../src/battle/core/geom'
import type { HitMsg } from '../../src/battle/rulesets/log2/actor'
import { log2Data } from './harness'

export interface Run {
  state_file: string
  slot: number
  input: string
  facing: number
  enemies_pinned: boolean
  hits: { frame: number; enemy: number; attack_id: number; scale: number; rect_rel: number[] }[]
  enemies: { offset: number[] }[]
}
export const RUNS = meas as unknown as Record<string, Run>

// characters / forms of the save states (Lv40 debug Map Test party; Gohan in blue armor)
const CHAR: Record<string, [number, number]> = {
  'log2_pl_gohan.ss': [0, 2],
  'log2_pl_piccolo.ss': [1, 6],
  'log2_pl_vegeta.ss': [2, 8],
  'log2_pl_trunks.ss': [3, 12],
  'log2_pl_goku.ss': [4, 19],
  'log2_pl_hercule.ss': [5, 21],
}

export function parseInput(s: string): number[] {
  const out: number[] = []
  const K: Record<string, number> = { W: 0, A: BTN.A, B: BTN.B, L: BTN.L }
  for (const tok of s.split(' ')) {
    const m = tok.match(/^(.*)\*(\d+)$/)
    const parts = m ? m[1].split(',') : [tok]
    const rep = m ? +m[2] : 1
    for (let r = 0; r < rep; r++)
      for (const p of parts) {
        const [k, n] = p.split(':')
        for (let i = 0; i < +n; i++) out.push(K[k] ?? 0)
      }
  }
  return out
}

export interface SimHit {
  frame: number
  enemy: number
  attack_id: number
  scale: number
  rect_rel: number[]
}

export function replay(name: string): { sim: SimHit[]; real: Run['hits'] } {
  const run = RUNS[name]
  const d = log2Data()
  const [ci, form] = CHAR[run.state_file]
  const w = new World(new Log2Rng(1, 2), { id: 't', name: 't', image: '', size: [2000, 2000], bounds: { x0: 0, y0: 0, x1: 2000, y1: 2000 } })
  const ctx = new Log2Ctx(d, { exp: false, popups: false })
  const c = d.chars[ci]
  const rec = { hp: 999, maxHp: 999, ep: 999, maxEp: 999, STR: 40 << 8, POW: 55 << 8, END: 53 << 8, exp: 0 }
  const player = new Player(ctx, { char: c, form, record: rec, level: 40, slot: run.slot })
  player.x = 1000 << 8
  player.y = 1000 << 8
  player.facing = run.facing as Dir
  ctx.player = player
  w.add(player)
  // the pl_ states have wolves (class 17, hurt top -26) next to the player: their geometry fits every first-hit frame
  const snake = d.classes[17]
  const enemies = run.enemies.map((en, i) => {
    const e = new Enemy(ctx, snake, snake.gfx[0], 'snake' + i)
    e.x = (1000 + en.offset[0]) << 8
    e.y = (1000 + en.offset[1]) << 8
    // The enemies' facing in the save states is not recorded; this rule reproduces every first-hit frame:
    // they face the player, except far (> 80 px) wolves in the Piccolo state, which still face down.
    const far = Math.hypot(en.offset[0], en.offset[1]) > 80
    e.facing = run.state_file === 'log2_pl_piccolo.ss' && far ? 0 : facingToward(-en.offset[0], -en.offset[1])
    e.hp = e.maxHp = 999999
    w.add(e)
    e.start(w)
    return e
  })
  const sim: SimHit[] = []
  let frame = 0
  for (const [i, e] of enemies.entries()) {
    const orig = e.onMessage.bind(e)
    e.onMessage = (ww, type, msg: HitMsg) => {
      const r = orig(ww, type, msg)
      if (r && type === 0) {
        const abs = msg.attack.id === 15 // whole-map rect, absolute
        const ox = abs ? 0 : player.px
        const oy = abs ? 0 : player.py
        sim.push({ frame, enemy: i, attack_id: msg.attack.id, scale: msg.scale, rect_rel: [msg.rect.x0 - ox, msg.rect.y0 - oy, msg.rect.x1 - ox, msg.rect.y1 - oy] })
      }
      return r
    }
    if (run.enemies_pinned) {
      // pinned: AI frozen in place (the measurement script rewrote their position every frame)
      const px = e.x
      const py = e.y
      e.update = () => {
        e.x = px
        e.y = py
      }
    }
  }
  const keys = parseInput(run.input)
  let prev = 0
  for (frame = 0; frame < keys.length; frame++) {
    const held = keys[frame]
    w.step({ held, pressed: held & ~prev, released: prev & ~held })
    prev = held
    if (run.state_file === 'log2_pl_piccolo.ss') {
      // the map clamps the camera in this save state: screen edges at -93..+147 x, -61..+99 y around the player
      // (the beams stop growing at the screen edge: SBC tip max x 147 / y 99 in sbc_r_2 / sbc_d)
      w.camera = { x0: player.px - 93, y0: player.py - 61, x1: player.px + 147, y1: player.py + 99 }
    } else w.centerCamera(player.px + 8, player.py - 8)
  }
  return { sim, real: run.hits }
}
