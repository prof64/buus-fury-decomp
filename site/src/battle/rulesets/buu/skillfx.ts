// Visual objects of the Buu's Fury player skills that the simulator models with simpler hit objects
// (projectiles.ts): the game's own objects, records and timing (re/buu/notes.md "Effect graphics"). Visual only -
// hits stay with the fitted hit objects - except where noted. Positions are relative to the player position
// (obj+0x1A0/0x1A4), per facing (down, up, left, right), as traced in the emulator.

import { DIR_VEC, type Dir } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import { BTN } from '../../core/input'
import type { Entity, World } from '../../core/world'
import type { Actor } from './actor'
import type { BuuCtx } from './context'
import { drawRec, drawRecAddr, drawRecAffine, drawRecFlipped, recGeomAt } from './fx'

type Bank = BuuCtx['data']['sprites']

/** draw a record with the beam-piece convention: OBJ top-left = (x, y) + origin */
export function drawRecAdd(r: Renderer, bank: Bank, addr: number, x: number, y: number) {
  const g = recGeomAt(bank, addr)
  if (!g) return
  drawRecAddr(r, bank, addr, x + 2 * g[0], y + 2 * g[1])
}

/** centre of a record's OBJ box placed with the standard convention at (x, y) */
export function recCentre(bank: Bank, addr: number, x: number, y: number): [number, number] {
  const g = recGeomAt(bank, addr)
  if (!g) return [x, y]
  return [x - g[0] + (g[2] >> 1), y - g[1] + (g[3] >> 1)]
}

abstract class Vis implements Entity {
  id = 0
  kind = 'vis'
  removed = false
  layer = 1
  t = 0
  abstract depth(): number
  abstract update(w: World): void
  abstract draw(r: Renderer, w: World): void
}

// ---------------------------------------------------------------------------------------------------------------
/** Dragon Fist Explosion (chain builder 0x08038AD8, vtable 0x0804D1B8, table 0x0877AD10): 6 segments, root (index 5)
 *  at DF_ROOT, each child at its parent + dir * (6 - k) (0, 1, 3, 6, 10, 15 px along the facing). While active the
 *  segment k moves (0x60 + 0x20 * (5 - k)) / 256 px per tick (head 1 px/tick), middle segments ease toward the
 *  midpoint of their neighbours, the head is steered by the d-pad 3 px/tick (+-56 x, +-32 y). Records dir*4 + 0 body
 *  (32x32), + 1 head, dying head + 1 + (t >> 3) for 24 ticks, then every segment explodes (0x0801952C, root first). */
const DF_ROOT: [number, number][] = [[8, 16], [8, -32], [-8, -8], [24, -8]]
const DF_TABLE = 0x0877ad10

export class DragonFistFx extends Vis {
  ctx: BuuCtx
  owner: Actor
  dir: Dir
  /** segment positions in 1/256 px, index 0 = head .. 5 = root */
  seg: [number, number][] = []
  steer: [number, number] = [0, 0]
  active: () => boolean
  dying = -1
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, active: () => boolean) {
    super()
    this.ctx = ctx
    this.owner = owner
    this.dir = dir
    this.active = active
    this.kind = 'dragonfx'
    const [dx, dy] = DIR_VEC[dir]
    const [rx, ry] = DF_ROOT[dir]
    let x = (owner.px + rx) << 8
    let y = (owner.py + ry) << 8
    const s: [number, number][] = []
    s[5] = [x, y]
    for (let k = 4; k >= 0; k--) {
      x += dx * (6 - k) * 256
      y += dy * (6 - k) * 256
      s[k] = [x, y]
    }
    this.seg = s
  }
  depth() {
    return this.seg[0][1] >> 8
  }
  update(w: World) {
    this.t++
    if (this.dying < 0) {
      if (!this.active()) {
        // the launch frame counts: the segments vanish 24 frames after it (emulator: the 6 puffs' rand(4096) land on
        // the frame the player returns to idle)
        this.dying = 1
        return
      }
      const [dx, dy] = DIR_VEC[this.dir]
      for (let k = 0; k < 6; k++) {
        const v = 0x60 + 0x20 * (5 - k)
        this.seg[k][0] += dx * v
        this.seg[k][1] += dy * v
      }
      for (let k = 1; k < 5; k++) {
        const mx = (this.seg[k - 1][0] + this.seg[k + 1][0]) >> 1
        const my = (this.seg[k - 1][1] + this.seg[k + 1][1]) >> 1
        this.seg[k][0] = (3 * this.seg[k][0] + mx) >> 2
        this.seg[k][1] = (3 * this.seg[k][1] + my) >> 2
      }
      const held = w.input.held
      const sx = held & BTN.LEFT ? -3 : held & BTN.RIGHT ? 3 : 0
      const sy = held & BTN.UP ? -3 : held & BTN.DOWN ? 3 : 0
      const nx = Math.max(-56, Math.min(56, this.steer[0] + sx))
      const ny = Math.max(-32, Math.min(32, this.steer[1] + sy))
      this.seg[0][0] += (nx - this.steer[0]) << 8
      this.seg[0][1] += (ny - this.steer[1]) << 8
      this.steer = [nx, ny]
      return
    }
    if (++this.dying >= 24) {
      // explode the chain, root first
      const bank = this.ctx.data.sprites
      for (let k = 5; k >= 0; k--) {
        const rec = DF_TABLE + 12 * (this.dir * 4 + (k === 0 ? 3 : 0))
        const [cx, cy] = recCentre(bank, rec, this.seg[k][0] >> 8, this.seg[k][1] >> 8)
        this.ctx.explosion(w, cx, cy, 0x1000 + w.rng.rand(0x1000, 'dragon fist explosion rate'))
      }
      this.removed = true
    }
  }
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const order = this.dir === 1 ? [0, 1, 2, 3, 4, 5] : [5, 4, 3, 2, 1, 0]
    for (const k of order) {
      const i = k ? 0 : this.dying < 0 ? 1 : 1 + Math.min(2, this.dying >> 3)
      drawRecAddr(r, bank, DF_TABLE + 12 * (this.dir * 4 + i), this.seg[k][0] >> 8, this.seg[k][1] >> 8)
    }
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Energy Sword (ctor 0x08037C1C, vtable 0x0804B4AC, table 0x0876CBD8) at the player position: charging records
 *  base + (t >> 4) for t < 80 (base 0, facing right 5); slashing (player state 0x0801B088) k = slash tick & 7, record
 *  facing right [14,13,12,11,10,17,16,15][k], otherwise 10 + k. */
export const SWORD_FULL = 80
const SWORD_TABLE = 0x0876cbd8
export class EnergySwordFx extends Vis {
  ctx: BuuCtx
  owner: Actor
  slash = -1
  alive: () => boolean
  constructor(ctx: BuuCtx, owner: Actor, alive: () => boolean) {
    super()
    this.ctx = ctx
    this.owner = owner
    this.alive = alive
    this.kind = 'swordfx'
  }
  depth() {
    return this.owner.py + 1
  }
  startSlash() {
    if (this.slash < 0) this.slash = 0
  }
  update() {
    if (!this.alive()) {
      this.removed = true
      return
    }
    if (this.slash >= 0) this.slash++
    else this.t++
  }
  draw(r: Renderer) {
    const right = this.owner.facing === 3
    let rec: number
    if (this.slash >= 0) {
      const k = Math.max(0, this.slash - 1) & 7
      rec = right ? [14, 13, 12, 11, 10, 17, 16, 15][k] : 10 + k
    } else rec = (right ? 5 : 0) + Math.min(4, Math.min(SWORD_FULL - 1, this.t) >> 4)
    drawRecAddr(r, this.ctx.data.sprites, SWORD_TABLE + 12 * rec, this.owner.px, this.owner.py)
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Final Flash graphics (class 0x08036FA2, vtable 0x0804AF24; tables 0x0876174C + 12n: 0x0875FE2C, 0x08760A8C,
 *  0x087616EC): start record 2*dir at the origin, body record 2*dir + 1 at dir * (16 + 32k) while < len - 1; while the
 *  scale (+0x18: 0x20, +0x20 per tick to 0x100, -0x20 per tick at the end) is below 0x100 the pieces are affine with
 *  inv = 0x10000 / scale: down (inv, -0x80), up (inv, 0x100), left (-0x80, inv), right (0x100, inv) as (pa, pd). */
export const FINAL_FLASH_TABLES = [0x0875fe2c, 0x08760a8c, 0x087616ec]
/** origin relative to the player position (table 0x0871DF9C) */
export const FINAL_FLASH_POS: [number, number][] = [[8, 0], [8, -16], [-8, -6], [24, -6]]

export function drawFinalFlash(r: Renderer, bank: Bank, table: number, dir: Dir, x: number, y: number, len: number, scale: number) {
  const [dx, dy] = DIR_VEC[dir]
  const pieces: [number, number, number][] = [[table + 24 * dir, x, y]]
  for (let d = 16; d < len - 1; d += 32) pieces.push([table + 24 * dir + 12, x + dx * d, y + dy * d])
  if (scale >= 0x100) {
    for (const [a, px, py] of pieces) drawRecAddr(r, bank, a, px, py)
    return
  }
  if (scale <= 0) return
  const inv = Math.floor(0x10000 / scale)
  const [pa, pd] = [[inv, -0x80], [inv, 0x100], [-0x80, inv], [0x100, inv]][dir]
  for (const [a, px, py] of pieces) drawRecAffine(r, bank, a, px, py, pa, pd)
}

// ---------------------------------------------------------------------------------------------------------------
/** Ultra Volleyball Attack wave (ctor 0x080383D4, vtable 0x0804C55C, table 0x0876E264, pieces at position + origin):
 *  tail dir*3 at the origin, body dir*3 + 1 at dir * k for k = 8, 16, .. while k < len - 9, head dir*3 + 2 at
 *  dir * (len - 8); len 8 -> 48 at 4 per tick. Drawn with the head on (hx, hy). */
const VOLLEY_TABLE = 0x0876e264
export function drawVolleyWave(r: Renderer, bank: Bank, dir: Dir, hx: number, hy: number, t: number, sx = hx, sy = hy) {
  const [dx, dy] = DIR_VEC[dir]
  // the origin never lies behind the spawn point (sx, sy): the wave grows out of the hands
  const run = (hx - sx) * dx + (hy - sy) * dy
  const len = Math.max(8, Math.min(48, 8 + 4 * t, run + 8))
  const ox = hx - dx * (len - 8)
  const oy = hy - dy * (len - 8)
  const base = VOLLEY_TABLE + 36 * dir
  drawRecAdd(r, bank, base, ox, oy)
  for (let k = 8; k < len - 9; k += 8) drawRecAdd(r, bank, base + 12, ox + dx * k, oy + dy * k)
  drawRecAdd(r, bank, base + 24, hx, hy)
}

// ---------------------------------------------------------------------------------------------------------------
/** Big Bang Kamehameha flare (ctor 0x08038886, vtable 0x0804DD6C, buu_bbk_flare): k = (t >> 2) & 3 flare + mirror
 *  piece, the ball ((t >> 2) & 1) until the attack ends; then the flare finishes its cycle (removed when k wraps). */
export const BBK_FLARE_POS: [number, number][] = [[8, 16], [8, -32], [-16, -6], [32, -6]]
export class BbkFlareFx extends Vis {
  ctx: BuuCtx
  x: number
  y: number
  dir: Dir
  alive: () => boolean
  ended = false
  constructor(ctx: BuuCtx, x: number, y: number, dir: Dir, alive: () => boolean) {
    super()
    this.ctx = ctx
    this.x = x
    this.y = y
    this.dir = dir
    this.alive = alive
    this.kind = 'bbkfx'
  }
  depth() {
    return this.y
  }
  update() {
    this.t++
    if (!this.ended && !this.alive()) this.ended = true
    if (this.ended && ((this.t >> 2) & 3) === 0 && (this.t & 3) === 0) this.removed = true
  }
  draw(r: Renderer) {
    const set = this.ctx.fx('buu_bbk_flare')
    if (!set) return
    const k = (this.t >> 2) & 3
    for (const name of ['flare', 'mirror']) {
      const st = set.anims.get(name)?.dirs[this.dir][k]
      if (st) set.drawFrame(r.ctx, st.frame, r.sx(this.x), r.sy(this.y), st.flip)
    }
    if (!this.ended) {
      const st = set.anims.get('ball')?.dirs[this.dir][(this.t >> 2) & 1]
      if (st) set.drawFrame(r.ctx, st.frame, r.sx(this.x), r.sy(this.y), st.flip)
    }
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Flame Kamehameha (link class 0x0802DB6C, vtable 0x0804C99C, table 0x087568B4): charge sparkle (record object at
 *  FLAME_CHARGE, fx_08721164 record 12 + f, f = t >> 3, > 3 -> 2 + (f & 1)) until the fire tick; then level + 3 links,
 *  the first at FLAME_FIRST, each link spawning the next 16 px further at its tick 8. Link record dir*7 + f with
 *  f = t >> 2: non-last links cycle 2,1,2,3 from f = 4, the last link 4,5,6,5 from f = 7. The d-pad perpendicular to
 *  the facing shifts link i by (i+1)^2/2 px per tick, clamped to +-(i+1)^2. */
export const FLAME_CHARGE: [number, number][] = [[8, -2], [12, -2], [12, -2], [4, -2]]
const FLAME_FIRST: [number, number][] = [[8, 16], [8, -32], [0, -6], [16, -6]]
const FLAME_TABLE = 0x087568b4

export class FlameKameFx extends Vis {
  ctx: BuuCtx
  owner: Actor
  dir: Dir
  n: number
  /** per link: [x, y, t, perpendicular shift] */
  links: [number, number, number, number][] = []
  alive: () => boolean
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, lv: number, alive: () => boolean) {
    super()
    this.ctx = ctx
    this.owner = owner
    this.dir = dir
    this.n = Math.min(2, lv) + 3
    this.alive = alive
    this.kind = 'flamekamefx'
    const [fx, fy] = FLAME_FIRST[dir]
    this.links.push([owner.px + fx, owner.py + fy, 0, 0])
  }
  depth() {
    return this.links[0][1]
  }
  update(w: World) {
    if (!this.alive()) {
      this.removed = true
      return
    }
    const [dx, dy] = DIR_VEC[this.dir]
    const held = w.input.held
    const perp = dx ? (held & BTN.UP ? -1 : held & BTN.DOWN ? 1 : 0) : held & BTN.LEFT ? -1 : held & BTN.RIGHT ? 1 : 0
    for (let i = 0; i < this.links.length; i++) {
      const L = this.links[i]
      L[2]++
      const m = (i + 1) * (i + 1)
      L[3] = Math.max(-m, Math.min(m, L[3] + perp * (m / 2)))
      if (L[2] === 8 && i === this.links.length - 1 && this.links.length < this.n) this.links.push([L[0] + dx * 16, L[1] + dy * 16, 0, 0])
    }
  }
  draw(r: Renderer) {
    const [dx] = DIR_VEC[this.dir]
    const bank = this.ctx.data.sprites
    this.links.forEach((L, i) => {
      let f = L[2] >> 2
      const last = i === this.n - 1
      if (last) {
        if (f >= 7) {
          f = (f & 3) + 4
          if (f === 7) f = 5
        }
      } else if (f >= 4) {
        f &= 3
        if (f === 0) f = 2
      }
      const s = Math.trunc(L[3])
      drawRecAddr(r, bank, FLAME_TABLE + 0x54 * this.dir + 12 * f, L[0] + (dx ? 0 : s), L[1] + (dx ? s : 0))
    })
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Instant Transmission markers (class 0x080222E0, vtable 0x0804E120, OAM priority 0): record group + ((t >> 2) & 3)
 *  drawn at P - (16, 16) and again vertically flipped at P - (16, 0) (a 32x32 ring). Group A 0x0872BF04 = the player's
 *  cursor, group B 0x0872BF34 = target markers. The sim's cursor sits on the selected target. */
const IT_A = 0x0872bf04
const IT_B = 0x0872bf34
export class ItMarkersFx extends Vis {
  ctx: BuuCtx
  target: () => [number, number] | null
  targets: () => [number, number][]
  alive: () => boolean
  constructor(ctx: BuuCtx, target: () => [number, number] | null, targets: () => [number, number][], alive: () => boolean) {
    super()
    this.ctx = ctx
    this.target = target
    this.targets = targets
    this.alive = alive
    this.layer = 3
    this.kind = 'itfx'
  }
  depth() {
    return 0
  }
  update() {
    this.t++
    if (!this.alive()) this.removed = true
  }
  private ring(r: Renderer, group: number, x: number, y: number) {
    const a = group + 12 * ((this.t >> 2) & 3)
    drawRecFlipped(r, this.ctx.data.sprites, a, x - 16, y - 16, false, false)
    drawRecFlipped(r, this.ctx.data.sprites, a, x - 16, y, false, true)
  }
  draw(r: Renderer) {
    for (const [x, y] of this.targets()) this.ring(r, IT_B, x, y)
    const p = this.target()
    if (p) this.ring(r, IT_A, p[0], p[1])
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Kamikaze Ghost (skill state 0x0804DC80): ghosts are actors of enemy type 0x54 with sprite 148, spawned at
 *  GHOST_SPAWN; formation slots (table 0x0871E024) per facing for ghosts 0..2; animation raw 19 at the spawn, then 1. */
export const GHOST_SPAWN: [number, number][] = [[6, 1], [-4, -8], [-16, -6], [16, -6]]
export const GHOST_SLOTS: [number, number][][] = [
  [[-26, 17], [38, 17], [6, 25]],
  [[-36, -24], [28, -24], [-4, -32]],
  [[-32, -38], [-32, 26], [-40, -6]],
  [[32, -38], [32, 26], [40, -6]],
]

/** the Energy Punch Lv3 aura (record object 0x080507A0, table 0x08762270): record dir*3 + (state tick >= 56 ? 1 : 0)
 *  at the player + ENERGY_PUNCH_POS from state tick 8 */
export const ENERGY_PUNCH_POS: [number, number][] = [[13, 8], [0, -19], [-8, -3], [24, -3]]
export const ENERGY_PUNCH_TABLE = 0x08762270

/** the Super Saiyan transform aura: looping 3 records of 0x08752D28 at rate 0x4000 at the body-box centre + (0, 1) */
export const SS_AURA = { id: 'fx_08752d28', n: 3, rate: 0x4000 }

export { drawRec }
