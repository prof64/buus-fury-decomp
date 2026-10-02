// Buu's Fury look & movement against emulator recordings (made with the scripts in work/compare/: emu_oam.py,
// trace_cmp.py, emu_bump.py, emu_camseq.py, emu_enemyspeed.py): sprite direction labels (the "walking backwards"
// bug), halo / shadow overlays, player walk / dash / punch / wall-bump timing, the enemy step vector, camera easing.
import { describe, expect, it } from 'vitest'
import sprites from '../../../data/buu/sprites.json'
import { fighter, makeBuuSim, type BuuSim } from './buu_harness'
import { BTN } from '../../src/battle/core/input'
import { autoZoom } from '../../src/battle/core/renderer'

interface Step {
  frame: number
  rom: string
  attach?: [number, number]
}
interface Ent {
  id: string
  sprite_index?: number | null
  header?: { shadow?: number[] }
  frames: { rom: string }[]
  animations: Record<string, { directions: Record<string, Step[]> }>
}
const ents = (sprites as unknown as { entities: Ent[] }).entities
const ent = (id: string) => ents.find((e) => e.id === id)!
const byIndex = (i: number) => ents.find((e) => e.sprite_index === i)!

describe('sprite directions (engine order down, up, left, right)', () => {
  it('Goku walking left shows the ROM direction-2 frames, walking right direction 3 (emulator OAM)', () => {
    // emulator: d-pad LEFT -> obj+0x10 = 2 and the OBJ shows frame record 0x0850CED4 (then 0x0850CEE8 after 8 ticks);
    // RIGHT -> obj+0x10 = 3, record 0x0850CF24 (the hflipped copy)
    const walk = ent('106_goku').animations.walk.directions
    expect(walk.left.map((s) => s.rom)).toEqual(['0x0850CED4', '0x0850CEE8', '0x0850CEFC', '0x0850CF10'])
    expect(walk.right[0].rom).toBe('0x0850CF24')
    expect(walk.down[0].rom < walk.up[0].rom && walk.up[0].rom < walk.left[0].rom && walk.left[0].rom < walk.right[0].rom).toBe(true)
  })
  it('every 4-direction animation lists its ROM frame records in down, up, left, right order', () => {
    // the frame table is frame-major {down, up, left, right} per frame (0x08010192): within one frame index the
    // records are laid out in that order for every sprite whose directions use distinct records
    let checked = 0
    for (const e of ents) {
      for (const a of Object.values(e.animations)) {
        const d = a.directions
        if (!d.down || !d.up || !d.left || !d.right) continue
        const r = ['down', 'up', 'left', 'right'].map((k) => d[k][0].rom)
        if (new Set(r).size < 4) continue
        // records of one frame are often adjacent (+0x14 apart for single-OBJ frames): left must come before right
        if (Math.abs(parseInt(r[3], 16) - parseInt(r[2], 16)) > 0x400) continue
        expect(parseInt(r[2], 16) < parseInt(r[3], 16), `${e.id}`).toBe(true)
        checked++
      }
    }
    expect(checked).toBeGreaterThan(500)
  })
})

describe('overlays', () => {
  it('halo forms use defs 108/110/112 with the halo table (indexed by engine anim number)', () => {
    const g = byIndex(108)
    // emulator OAM: halo at pos + (0,-27)/(0,-28) while walking, (-4,-28), (-10,-29), (-4,-28) in the hurt animation facing right
    expect(g.animations.walk.directions.left.map((s) => s.attach)).toEqual([[0, -27], [0, -28], [0, -27], [0, -28]])
    expect(g.animations.hurt.directions.right.map((s) => s.attach)).toEqual([[-4, -28], [-10, -29], [-4, -28]])
    expect(g.animations.idle.directions.right[0].attach).toEqual([0, -28])
    expect(byIndex(110)).toBeTruthy()
    expect(byIndex(112)).toBeTruthy()
  })
  it('ground shadows 0x086B759C exist for every header shadow size', () => {
    for (const s of ['4x2', '10x4', '16x6', '24x10']) expect(ent(`buu_shadow_${s}`)).toBeTruthy()
    expect(byIndex(106).header!.shadow!.slice(2)).toEqual([10, 4])
  })
})

/** trace one input script; positions relative to the start */
function trace(s: BuuSim, keys: [number, number][]) {
  const p = s.player
  const x0 = p.x
  const y0 = p.y
  const out: { x: number; y: number; d: number; a: string; st: string }[] = []
  let prev = 0
  for (const [held, n] of keys) {
    for (let i = 0; i < n; i++) {
      s.w.step({ held, pressed: held & ~prev, released: prev & ~held })
      prev = held
      s.battle.postStep?.(s.w)
      out.push({ x: (p.x - x0) / 256, y: (p.y - y0) / 256, d: p.facing, a: p.anim?.name ?? '-', st: p.state })
    }
  }
  return out
}
function quiet(arena = 'forest') {
  const s = makeBuuSim({ enemy: 't68', arena, fighter: fighter() })
  for (const e of s.enemies) {
    e.x = 30000 << 8
    e.y = 30000 << 8
    e.update = () => undefined
  }
  s.player.x = 144 << 8
  s.player.y = 152 << 8
  s.player.facing = 2
  return s
}

describe('player movement (emulator traces, Snakeway, new-game Goku SPD 50: walk 352, dash 672)', () => {
  it('walk: the first held frame only turns / starts the animation, then 1.375 px every frame; frame every 8', () => {
    const t = trace(quiet(), [[0, 2], [BTN.LEFT, 12], [0, 2], [BTN.RIGHT, 3]])
    // emulator x: 0, 0, 0 (walk set), -1.375, -2.75, ...
    expect(t.slice(2, 6).map((f) => f.x)).toEqual([0, -1.375, -2.75, -4.125])
    expect(t[2].a).toBe('walk')
    expect(t[2].d).toBe(2)
    expect(t[14].a).toBe('idle')
    expect(t[16].d).toBe(3)
    expect(t[16].x).toBe(t[15].x)
    expect(t[17].x - t[16].x).toBe(1.375)
  })
  it('dash: double tap within 20 frames, 2.625 px per frame from the frame after the tap', () => {
    const t = trace(quiet(), [[0, 1], [BTN.RIGHT, 2], [0, 2], [BTN.RIGHT, 6]])
    const d = t.slice(5)
    expect(d[0].st).toBe('dash')
    expect(d[1].x - d[0].x).toBe(2.625)
    expect(d[5].x - d[4].x).toBe(2.625)
  })
  it('punch: one A press, back to idle 16 frames later', () => {
    const t = trace(quiet(), [[0, 2], [BTN.A, 1], [0, 20]])
    const first = t.findIndex((f) => f.st === 'punch')
    const back = t.findIndex((f, i) => i > first && f.st === 'idle')
    expect(back - first).toBe(16)
  })
  it('dashing into a wall: hurt animation, pushed back 0.75 px per frame for 23 frames, then walking again', () => {
    const s = quiet()
    const b = s.w.arena.bounds
    s.player.x = (b.x0 + 20) << 8
    const t = trace(s, [[0, 1], [BTN.LEFT, 2], [0, 2], [BTN.LEFT, 60]])
    const hit = t.findIndex((f) => f.st === 'bump')
    expect(hit).toBeGreaterThan(0)
    expect(t[hit].a).toBe('hurt')
    for (let i = 1; i <= 23; i++) expect(t[hit + i].x - t[hit + i - 1].x).toBe(0.75)
    expect(t[hit + 24].st).toBe('walk')
    expect(t[hit + 24].x).toBe(t[hit + 23].x)
    expect(t[hit + 25].x - t[hit + 24].x).toBe(-1.375)
  })
})

describe('camera 0x080077EC', () => {
  it('eases 3/64 (x) and 9/128 (y) per frame toward the footprint centre (emulator, walk right then down)', () => {
    const s = quiet()
    s.w.arena.view = { x0: 0, y0: 0, x1: 4000, y1: 4000 }
    const xs: number[] = []
    const ys: number[] = []
    let prev = 0
    for (const held of [...Array(60).fill(BTN.RIGHT), ...Array(30).fill(BTN.DOWN)]) {
      s.w.step({ held, pressed: held & ~prev, released: prev & ~held })
      prev = held
      s.battle.postStep?.(s.w)
      xs.push(s.w.camera.x0)
      ys.push(s.w.camera.y0)
    }
    // recorded in the emulator from buu_snakeway_start.ss (Goku at 144,152 facing left, camera 32,72)
    expect(xs).toEqual([32, 32, 32, 32, 32, 32, 33, 33, 33, 34, 34, 35, 36, 36, 37, 38, 38, 39, 40, 41, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 57, 58, 59, 60, 61, 62, 63, 65, 66, 67, 68, 69, 71, 72, 73, 74, 76, 77, 78, 79, 81, 82, 83, 85, 86, 87, 88, 89, 91, 92, 93, 93, 94, 95, 96, 97, 98, 98, 99, 100, 100, 101, 101, 102, 102, 103, 103, 104, 104, 104, 105, 105, 106, 106, 106])
    // (the Snakeway floor ends 76 frames in: Goku stops there, so only the first 76 frames are compared for y)
    expect(ys.slice(0, 76)).toEqual([...Array(64).fill(72), 73, 73, 74, 74, 75, 76, 76, 77, 78, 79, 80, 81])
  })
})

describe('enemies', () => {
  it('StepTowardPlayer 0x08018C3A moves `speed` on the major axis (Majin Fighter: (153, k) never (136, 70))', () => {
    const s = makeBuuSim({ enemy: 't68', fighter: fighter() })
    const e = s.enemies[0]
    s.player.rec.hp = s.player.rec.maxHp = 99999
    // player 60 px left and 30 px above: chase is diagonal
    s.player.x = e.x - (60 << 8)
    s.player.y = e.y - (30 << 8)
    const steps: string[] = []
    for (let f = 0; f < 40; f++) {
      const [x, y] = [e.x, e.y]
      s.run(1)
      if (e.x !== x || e.y !== y) steps.push(`${Math.abs(e.x - x)},${Math.abs(e.y - y)}`)
    }
    expect(steps.length).toBeGreaterThan(5)
    for (const st of steps) expect(st.startsWith('153,')).toBe(true)
  })
})

describe('PC view', () => {
  it('auto zoom: smallest zoom >= 2 whose view the arena covers (<= 8% black)', () => {
    expect(autoZoom(1468, 832, { w: 480, h: 400 })).toBe(3) // Giant Crater: 489x277
    expect(autoZoom(1468, 832, { w: 800, h: 720 })).toBe(2) // Nataday Village: 734x416
    expect(autoZoom(1468, 832, { w: 325, h: 320 })).toBe(5) // HFIL
    expect(autoZoom(1468, 832, null)).toBe(2)
  })
})
