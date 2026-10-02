// Buu's Fury map collision (tools/battle/COLLISION.md): the walk / projectile grids from the ROM map headers and the
// player's try-move 0x08012960 against emulator recordings (data/battle/buu_collision_traces.json, made by
// tools/battle/gen_buu_collision_traces.py): walking into cliffs, rocks and the stone ring, diagonals sliding along
// walls, corner snapping, a dash into a cliff. Every frame must give the same 24.8 position as the game.
import { describe, expect, it } from 'vitest'
import traces from '../../../data/battle/buu_collision_traces.json'
import pathTraces from '../../../data/battle/buu_path_traces.json'
import stepTraces from '../../../data/battle/buu_step_traces.json'
import kbTraces from '../../../data/battle/buu_knockback_traces.json'
import type { KnockState } from '../../src/battle/rulesets/buu/enemy'
import { PathMover } from '../../src/battle/rulesets/buu/path'
import { buuData, fighter, makeBuuSim } from './buu_harness'
import { BTN } from '../../src/battle/core/input'
import { buuRuleset } from '../../src/battle/rulesets/buu'

const KEY: Record<string, number> = { L: BTN.LEFT, R: BTN.RIGHT, U: BTN.UP, D: BTN.DOWN, A: BTN.A, '-': 0 }
const mask = (s: string) => [...s].reduce((m, c) => m | KEY[c], 0)

interface Trace {
  name: string
  map: string
  start: [number, number]
  keys: [string, number][]
  footprint: [number, number]
  pos: [number, number][]
}

function arenaFor(map: string) {
  buuData()
  const a = buuRuleset.arenas().find((x) => x.note === `map ${map}`)
  if (!a) throw new Error(`no arena on map ${map}`)
  return a.id
}

describe('map collision data (data/battle/buu_collision.json)', () => {
  it('every arena map has walk + projectile grids', () => {
    const d = buuData()
    for (const a of buuRuleset.arenas()) {
      const map = a.note!.replace('map ', '')
      const g = d.collision.get(map)
      expect(g, map).toBeTruthy()
      expect(g!.walk.length).toBe(g!.w * g!.h)
      expect(g!.walk.some((v) => v === 1)).toBe(true)
    }
  })
  it('Giant Crater: the cliffs around the floor are walls, the floor is open (the cliff ledge bug)', () => {
    const g = buuData().collision.get('8-71')!
    const walk = (x: number, y: number) => g.walk[(y >> 3) * g.w + (x >> 3)] === 1
    expect(walk(240, 200)).toBe(true)
    expect(walk(40, 184)).toBe(false) // left cliff where Vegeta used to walk
    expect(walk(240, 24)).toBe(false) // top cliff face
    expect(walk(456, 200)).toBe(false) // right cliff
  })
  it('composite origins: stadium 16 px, HFIL -5 px (camera vs screen match in the emulator)', () => {
    const d = buuData()
    expect(d.origins.get('7-6')).toEqual([16, 0])
    expect(d.origins.get('18-14')).toEqual([-5, 0])
    expect(d.origins.get('8-71')).toBeUndefined()
  })
})

describe('player try-move 0x08012960 vs emulator traces', () => {
  for (const t of (traces as unknown as { traces: Trace[] }).traces) {
    it(`${t.name} (map ${t.map}, ${t.pos.length} frames)`, () => {
      const s = makeBuuSim({ enemy: 't68', arena: arenaFor(t.map), fighter: fighter() })
      for (const e of s.enemies) {
        e.unmarkSelf()
        e.x = 30000 << 8
        e.y = 30000 << 8
        e.update = () => undefined
      }
      const p = s.player
      expect([p.footW, p.footH]).toEqual(t.footprint)
      p.x = t.start[0] << 8
      p.y = t.start[1] << 8
      let prev = 0
      let f = 0
      const got: [number, number][] = []
      for (const [k, n] of t.keys) {
        const held = mask(k)
        for (let i = 0; i < n; i++, f++) {
          s.w.step({ held, pressed: held & ~prev, released: prev & ~held })
          prev = held
          s.battle.postStep?.(s.w)
          got.push([p.x, p.y])
        }
      }
      for (let i = 0; i < t.pos.length; i++) {
        expect(got[i], `frame ${i}: sim (${got[i][0] / 256}, ${got[i][1] / 256}) game (${t.pos[i][0] / 256}, ${t.pos[i][1] / 256})`).toEqual(t.pos[i])
      }
    })
  }
})

interface PathRun {
  name: string
  start: [number, number]
  footprint: [number, number]
  goal: [number, number]
  speed: number
  marked: [number, number][]
  pos: [number, number][]
  waypoint: [number, number][]
}

describe('enemy path following (0x0803BB48 plan / wall-follow / step) vs emulator', () => {
  for (const r of (pathTraces as unknown as { runs: PathRun[] }).runs) {
    it(`${r.name}: Majin Fighter from (${r.start.map((v) => v / 256)}) to (${r.goal}), ${r.pos.length} frames`, () => {
      const s = makeBuuSim({ enemy: 't68', arena: 'forest', fighter: fighter() })
      const e = s.enemies[0]
      e.update = () => undefined
      const col = e.ctx.col!
      e.unmarkSelf()
      // the other objects' marks on the game's enemy grid at the start (the other fighters, NPCs)
      for (const [tx, ty] of r.marked) col.enemy[ty * col.w + tx] = 0
      expect([e.footW, e.footH]).toEqual(r.footprint)
      e.x = r.start[0]
      e.y = r.start[1]
      e.markSelf()
      const path = new PathMover(r.speed)
      path.gx = r.goal[0] << 8
      path.gy = r.goal[1] << 8
      path.counter = 0
      for (let f = 0; f < r.pos.length; f++) {
        path.update(e)
        expect([path.wx, path.wy], `frame ${f} waypoint`).toEqual(r.waypoint[f])
        expect([e.x, e.y], `frame ${f}: sim (${e.x / 256}, ${e.y / 256}) game (${r.pos[f][0] / 256}, ${r.pos[f][1] / 256})`).toEqual(r.pos[f])
      }
    })
  }
})

interface StepRun {
  name: string
  fn: string
  start: [number, number]
  footprint: [number, number]
  player: [number, number]
  playerFootprint: [number, number]
  speed: number
  param2: number
  marked: [number, number][]
  face: number
  frames: { rng: [number, number]; player: [number, number]; b11: number; pos: [number, number]; face: number; n8: number; ret: number }[]
}

describe('enemy step nodes vs emulator (node update called every frame, Southwest Forest)', () => {
  for (const r of (stepTraces as unknown as { runs: StepRun[] }).runs) {
    it(`${r.name} (${r.fn}, ${r.frames.length} frames)`, () => {
      const s = makeBuuSim({ enemy: 't68', arena: 'forest', fighter: fighter() })
      const e = s.enemies[0]
      e.update = () => undefined
      const col = e.ctx.col!
      e.unmarkSelf()
      for (const [tx, ty] of r.marked) col.enemy[ty * col.w + tx] = 0
      expect([e.footW, e.footH]).toEqual(r.footprint)
      expect([s.player.footW, s.player.footH]).toEqual(r.playerFootprint)
      s.player.x = r.player[0]
      s.player.y = r.player[1]
      e.x = r.start[0]
      e.y = r.start[1]
      e.facing = r.face as 0 | 1 | 2 | 3
      e.markSelf()
      const st = { n: r.param2 }
      for (let f = 0; f < r.frames.length; f++) {
        const g = r.frames[f]
        if (g.ret === 0xdeadbeef) continue // a lag frame: the game ran no logic, the injected call did not happen
        s.w.rng.setState([g.rng[0] | 0, g.rng[1] | 0])
        s.player.x = g.player[0]
        s.player.y = g.player[1]
        let ret: boolean | void
        if (r.fn === '0x08018C3A') ret = e.stepFromPlayer(r.speed, true)
        else if (r.fn === '0x0801CBA1') ret = e.stepFromPlayer(r.speed, false)
        else if (r.fn === '0x0802B708') e.wanderStep(r.speed, r.param2)
        else ret = e.walkToPlayer(r.speed, st, g.b11)
        const msg = `frame ${f}: sim (${e.x / 256}, ${e.y / 256}) f${e.facing} game (${g.pos[0] / 256}, ${g.pos[1] / 256}) f${g.face}`
        expect([e.x, e.y, e.facing], msg).toEqual([g.pos[0], g.pos[1], g.face])
        if (r.fn === '0x0802CDE8') expect(st.n, msg).toBe(g.n8)
        if (ret !== undefined) expect(ret ? 1 : 0, msg).toBe(g.ret)
      }
    })
  }
})

interface KbRun {
  name: string
  start: [number, number]
  footprint: [number, number]
  marked: [number, number][]
  holes: [number, number][]
  enter: { rng: [number, number]; K: number; hitDir: number; crit: number; sub3: number; blocking: number; face: number }
  updates: { frame: number; pos: [number, number]; rng: [number, number]; otherHp: number }[]
  other: { at: [number, number]; hp_before: number; hp_after_splat: number | null } | null
}

describe('enemy knockback (KnockbackSlide 0x08018A08 / 0x08018A92) vs emulator', () => {
  for (const r of (kbTraces as unknown as { runs: KbRun[] }).runs) {
    it(`${r.name}: K ${r.enter.K}, hit dir ${r.enter.hitDir}${r.enter.crit ? ', critical' : ''}${r.enter.blocking ? ', blocking' : ''}, ${r.updates.length} updates`, () => {
      const s = makeBuuSim({ enemy: 't68', count: r.other ? 2 : 1, arena: 'forest', fighter: fighter() })
      const [e, o] = s.enemies
      for (const x of s.enemies) {
        x.update = () => undefined
        x.unmarkSelf()
      }
      const col = e.ctx.col!
      for (const [tx, ty] of r.marked) col.enemy[ty * col.w + tx] = 0
      for (const [tx, ty] of r.holes) col.enemy[ty * col.w + tx] = 1
      expect([e.footW, e.footH]).toEqual(r.footprint)
      if (o && r.other) {
        o.x = r.other.at[0] << 8
        o.y = r.other.at[1] << 8
        // its HP when the slide's last update starts (the player's punch may have hit it too)
        o.hp1024 = r.updates[r.updates.length - 1].otherHp
      }
      e.x = r.start[0]
      e.y = r.start[1]
      e.markSelf()
      e.lastHitDir = r.enter.hitDir as 0 | 1 | 2 | 3
      e.critHit = !!r.enter.crit
      e.splatHit = !!r.enter.sub3
      e.blocking = !!r.enter.blocking
      s.w.rng.setState([r.enter.rng[0] | 0, r.enter.rng[1] | 0])
      const kb: KnockState = { vx: 0, vy: 0, n: 16, anim: null }
      e.knockStart(r.enter.K, kb)
      expect(e.facing).toBe(r.enter.hitDir ^ 1)
      let done = -1
      for (let i = 0; i < r.updates.length + 4 && done < 0; i++) {
        const u = r.updates[i]
        if (u) {
          expect([e.x, e.y], `update ${i}: sim (${e.x / 256}, ${e.y / 256}) game (${u.pos[0] / 256}, ${u.pos[1] / 256})`).toEqual(u.pos)
          s.w.rng.setState([u.rng[0] | 0, u.rng[1] | 0])
        }
        // Enemy.update order: anim tick, then the AI (the slide update); the hit (enter) comes earlier in the same frame
        e.tickAnim()
        if (e.knockStep(kb)) done = i
      }
      // the game's last caught update is the one that ends the slide
      expect(done).toBe(r.updates.length - 1)
      if (o && r.other) expect(o.hp1024, 'wall splat damage to the fighter at the wall').toBe(r.other.hp_after_splat ?? r.updates[r.updates.length - 1].otherHp)
    })
  }
})

describe('knockback in a running battle', () => {
  it('a punched Majin Fighter slides away from Goku (reaction state, K 36) and stops sliding', () => {
    const s = makeBuuSim({ enemy: 't68', arena: 'htc', fighter: fighter() })
    const e = s.enemies[0]
    s.player.rec.hp = s.player.rec.maxHp = 99999
    e.unmarkSelf()
    e.x = s.player.x + (18 << 8)
    e.y = s.player.y
    e.markSelf()
    s.player.facing = 3
    const x0 = e.x
    let hit = false
    let maxX = x0
    s.run(30, (f) => (f === 1 ? BTN.A : 0))
    hit = s.w.log.some((l) => /player .* -> Majin Fighter/.test(l.text))
    maxX = Math.max(maxX, e.x)
    expect(hit).toBe(true)
    expect(maxX - x0).toBeGreaterThan(4 << 8)
  })
})

describe('enemies and projectiles on the grids', () => {
  it('an enemy cannot walk into a cliff and enemies block each other (enemy grid marks)', () => {
    const s = makeBuuSim({ enemy: 't68', count: 2, arena: 'crater', fighter: fighter() })
    const [a, b] = s.enemies
    for (const e of [a, b]) e.update = () => undefined
    // walk the first one left into the left cliff
    a.unmarkSelf()
    a.x = 80 << 8
    a.y = 184 << 8
    a.markSelf()
    for (let i = 0; i < 200; i++) a.tryMove(-256, 0)
    const g = buuData().collision.get('8-71')!
    expect(a.px).toBeGreaterThan(40)
    for (let y = a.py >> 3; y < (a.py + a.footH + 7) >> 3; y++) for (let x = a.px >> 3; x < (a.px + a.footW + 7) >> 3; x++) expect(g.walk[y * g.w + x]).toBe(1)
    // the second one walks into the first from the right and stops at its marked tiles
    b.unmarkSelf()
    b.x = (a.px + 64) << 8
    b.y = a.y
    b.markSelf()
    for (let i = 0; i < 200; i++) b.tryMove(-256, 0)
    expect(b.px).toBeGreaterThanOrEqual(a.px + a.footW)
    expect(b.px - (a.px + a.footW)).toBeLessThan(16)
  })
  it('a ki blast fired at the crater cliff explodes on the projectile grid', () => {
    const s = makeBuuSim({ enemy: 't68', arena: 'crater', fighter: fighter() })
    for (const e of s.enemies) {
      e.unmarkSelf()
      e.x = 30000 << 8
      e.update = () => undefined
    }
    const p = s.player
    p.x = 80 << 8
    p.y = 184 << 8
    p.facing = 2
    p.rec.ep = p.rec.maxEp = 999
    p.skills = [7] // Energy Blast
    p.slot = 0
    let minX = Infinity
    let prev = 0
    for (let f = 0; f < 160; f++) {
      const held = f < 4 ? BTN.B : 0
      s.w.step({ held, pressed: held & ~prev, released: prev & ~held })
      prev = held
      for (const e of s.w.entities) if (e.kind !== 'player' && e.kind !== 'enemy' && 'px' in e) minX = Math.min(minX, (e as unknown as { px: number }).px)
    }
    // the cliff's projectile tiles start left of x = 64: the blast never reaches the map edge
    expect(minX).toBeGreaterThan(24)
    expect(minX).toBeLessThan(80)
  })
})
