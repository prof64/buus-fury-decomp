// Headless LoG1 battle harness: builds the data tables from the JSON files and restores a frame of real game RAM
// (recorded by tools/battle/gen_log1_vectors.py) into the simulator, then steps it with the same inputs.
import characters from '../../../data/log1/characters.json'
import enemies from '../../../data/log1/enemies.json'
import levels from '../../../data/log1/levels.json'
import maps from '../../../data/log1/maps.json'
import items from '../../../data/log1/items.json'
import sprites from '../../../data/log1/sprites.json'
import collision from '../../../data/battle/log1_collision.json'
import { buildLog1Data, type Log1Data } from '../../src/battle/rulesets/log1/data'
import { World } from '../../src/battle/core/world'
import { Log1Rng, Log1Rng2 } from '../../src/battle/rulesets/log1/rng'
import { Char, Log1Game } from '../../src/battle/rulesets/log1/game'
import { Player } from '../../src/battle/rulesets/log1/player'
import { Enemy } from '../../src/battle/rulesets/log1/enemy'
import { KiBlast } from '../../src/battle/rulesets/log1/projectiles'
import { log1PostStep, log1PreStep } from '../../src/battle/rulesets/log1'
import { KEY_BITS } from './log1_keys'

let data: Log1Data | null = null
export function log1Data(): Log1Data {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if (!data) data = buildLog1Data({ characters, enemies, levels, maps, items, sprites, collision } as any)
  return data
}

/* eslint-disable @typescript-eslint/no-explicit-any */
export type TraceInit = any

export interface Log1Sim {
  g: Log1Game
  w: World
  player: Player
  enemies: Enemy[]
  step(held: number): void
}


export function simFromInit(init: TraceInit): Log1Sim {
  const d = log1Data()
  const rng = new Log1Rng(init.rng[0], init.rng[1])
  const rng2 = new Log1Rng2(init.rng2[0], init.rng2[1])
  const m = d.maps.get(init.map)!
  const w = new World(rng, { id: 't', name: 't', image: '', size: m.size, bounds: { x0: 0, y0: 0, x1: m.size[0], y1: m.size[1] } })
  // the live collision bits of the recorded frame (pickups/NPCs/obstacles of the real map included)
  const live = init.collision ? { w: m.sizeTiles[0], h: m.sizeTiles[1], layers: (init.collision as string[]).map((s) => Uint8Array.from(atob(s), (ch) => ch.charCodeAt(0))) } : null
  const g = new Log1Game(d, rng, rng2, live ?? d.collision.get(init.map) ?? null, m.sizeTiles, { authenticBugs: true, godmode: false, exp: true, popups: false })
  g.world = w
  g.fc = init.fc
  g.pathTimer = init.pathTimer
  g.solar = init.solar
  g.cap = init.cap
  const ip = init.player
  g.flight = ip.flight
  g.maxFlight = ip.maxFlight
  g.selected = ip.selected
  g.learned = ip.learned | 1
  const pc = new Char('Goku', 1, null)
  pc.defense = ip.defense
  pc.hp = ip.hp
  pc.maxHp = ip.maxHp
  pc.ki = ip.ki
  pc.maxKi = ip.maxKi
  pc.kiRegen = ip.regen
  pc.exp = ip.exp
  pc.act = 0
  pc.x = init.seen[0]
  pc.y = init.seen[1]
  pc.tx = pc.x >> 3
  pc.ty = pc.y >> 3
  g.player = pc
  g.awake.push(pc)
  const player = new Player(g, pc)
  pc.ent = player
  player.sx = ip.sx
  player.sy = ip.sy
  player.x = ip.x
  player.y = ip.y
  g.px = ip.sx >> 4
  g.py = ip.sy >> 4
  player.facing = ip.face
  player.st = ip.st
  player.tick = ip.tick ?? 0
  player.t24 = ip.t24 ?? 0
  w.add(player)
  const list: Enemy[] = []
  for (const ie of init.enemies) {
    const t = d.templates[ie.type]
    const c = Char.fromTemplate(t, ie.x, ie.y)
    c.sx = ie.sx
    c.sy = ie.sy
    c.tx = ie.tx
    c.ty = ie.ty
    c.state = ie.state
    c.face = ie.face
    c.stun = ie.stun
    c.act = ie.act
    c.inv = ie.inv
    c.aggro = ie.aggro
    c.stuck = ie.stuck
    c.hp = ie.hp
    c.ki = ie.ki
    c.lastGoal = [ie.lastGoal[0], ie.lastGoal[1]]
    c.pathIdx = ie.pathIdx
    c.pathLen = ie.pathLen
    c.path = ie.path.map((p: number[]) => [p[0], p[1]])
    const sprite = (t.sprites.find((s) => s.def === ie.spriteDef) ?? t.sprites[0]).name
    const e = new Enemy(g, c, d.byLabel.get(sprite), sprite, t.sprites.find((s) => s.name === sprite)?.hurt ?? t.sprites[0].hurt, t.name)
    e.st = ie.entState
    e.tick = ie.entTick ?? 0
    e.t24 = ie.entT24 ?? 0
    e.facing = ie.entFace
    e.x = ie.entX
    e.y = ie.entY
    w.add(e)
    if (c.act === 2) g.dormant.push(c)
    else g.awake.push(c)
    list.push(e)
  }
  g.fireEnemyKi = (c, power) => {
    const e = c.ent as Enemy
    e.setState(5)
    w.add(new KiBlast(g, e, true, power))
  }
  const cam = init.cam
  w.camera = { x0: cam[0], y0: cam[1], x1: cam[2], y1: cam[3] }
  let prev = 0
  return {
    g,
    w,
    player,
    enemies: list,
    step(held: number) {
      const inp = { held, pressed: held & ~prev, released: prev & ~held }
      prev = held
      log1PreStep(g, w, inp, init.map)
      w.step(inp)
      log1PostStep(g, w, player)
    },
  }
}

export function keysAt(keys: [number, number, string][], f: number): number {
  let k = 0
  for (const [a, b, s] of keys) if (a <= f && f <= b) for (const n of s.split('|')) k |= KEY_BITS[n] ?? 0
  return k
}
