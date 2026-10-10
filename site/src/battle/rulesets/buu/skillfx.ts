// Visual objects of the Buu's Fury player skills that the simulator models with simpler hit objects
// (projectiles.ts): the game's own objects, records and timing (re/buu/notes.md "Effect graphics"). Visual only -
// hits stay with the fitted hit objects - except where noted. Positions are relative to the player position
// (obj+0x1A0/0x1A4), per facing (down, up, left, right), as traced in the emulator.

import { DIR_VEC, type Dir, type Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import { BTN } from '../../core/input'
import type { Entity, World } from '../../core/world'
import { broadcast, type Actor } from './actor'
import { ATK, type AttackType } from './damage'
import type { BuuCtx } from './context'
import { drawRec, drawRecAddr, drawRecAffine, drawRecFlipped, recGeomAt } from './fx'
import { audioEvent } from '../../audio/bus'

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
/** Dragon Fist Explosion (chain builder 0x08038AD8, segment update 0x08038C32, vtable 0x0804D1B8, table 0x0877AD10):
 *  6 segments (index 0 = root at DF_ROOT, 5 = head), each child created at the root + 1, 3, 6, 10, 15 px along the
 *  facing. The segments update before the player (newer objects first), root -> head: a middle segment eases toward
 *  the midpoint of its neighbours (x = (3x + ((parent.x + child.x) >> 1)) >> 2, the parent already updated this tick),
 *  the head is steered by the d-pad (LEFT while the offset > -56: -3 px, else RIGHT while < 56: +3; UP > -32 / DOWN
 *  < 32 the same: +-57 / +-33); each segment then tests its box (body 32 x 32 around the position, head 64 x 64 at
 *  DF_HEAD) and, until the launch, sends one message (x0.32 = 328, attack sub 18, dir = facing) to every object. The
 *  player's control moves the chain (shift: root by v, each child 0x20 more; v = 0x60, 0 when the player was
 *  blocked; not from tick 90) and launches it (0x08038E42: one x2.5 message with the head's box, no more segment
 *  messages); 24 ticks later every segment explodes (root first, rand(0x1000) each). A cancel (the state's exit with
 *  the dragon out) explodes it at once, head first. Records dir*4 + 0 body, + 1 head, dying head + 1 + (t >> 3). */
const DF_ROOT: [number, number][] = [[8, 16], [8, -32], [-8, -8], [24, -8]]
const DF_OFFS = [0, 1, 3, 6, 10, 15]
/** head box origin per facing (box = position - origin .. + 64) */
const DF_HEAD: [number, number][] = [[32, 32], [32, 32], [40, 32], [24, 32]]
const DF_TABLE = 0x0877ad10
const DF_MULT = 328
const DF_LAUNCH_MULT = 0xa00

export class DragonFistFx extends Vis {
  ctx: BuuCtx
  owner: Actor
  dir: Dir
  /** segment positions in 1/256 px, index 0 = root .. 5 = head */
  seg: [number, number][] = []
  /** the boxes of this tick's update */
  boxes: Rect[] = []
  steer: [number, number] = [0, 0]
  /** -1 flying (sending), >= 0 launched: ticks since the launch */
  dying = -1
  /** the player control's shift of this tick (null: none) - the OAM shows the chain before it */
  shifted: number | null = null
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir) {
    super()
    this.ctx = ctx
    this.owner = owner
    this.dir = dir
    this.kind = 'dragonfx'
    const [dx, dy] = DIR_VEC[dir]
    const [rx, ry] = DF_ROOT[dir]
    const x = (owner.px + rx) << 8
    const y = (owner.py + ry) << 8
    this.seg = DF_OFFS.map((o) => [x + dx * o * 256, y + dy * o * 256])
    this.boxes = this.seg.map((_, i) => this.box(i))
  }
  depth() {
    return this.seg[5][1] >> 8
  }
  box(i: number): Rect {
    const x = this.seg[i][0] >> 8
    const y = this.seg[i][1] >> 8
    if (i < 5) return { x0: x - 16, y0: y - 16, x1: x + 16, y1: y + 16 }
    const [ox, oy] = DF_HEAD[this.dir]
    return { x0: x - ox, y0: y - oy, x1: x - ox + 64, y1: y - oy + 64 }
  }
  private msg(rect: Rect, mult: number) {
    return { team: 1 as const, rect, dir: this.dir, mult, atk: ATK[18], attacker: this.owner, source: this }
  }
  update(w: World) {
    this.t++
    this.shifted = null
    if (this.dying >= 0) {
      // the body keeps easing between its neighbours after the launch (no steering, no messages; skill audit
      // sa_f41_skill5_steer)
      for (let i = 1; i < 5; i++) {
        const s = this.seg[i]
        const p = this.seg[i - 1]
        const c = this.seg[i + 1]
        s[0] = (3 * s[0] + ((p[0] + c[0]) >> 1)) >> 2
        s[1] = (3 * s[1] + ((p[1] + c[1]) >> 1)) >> 2
      }
      if (++this.dying >= 24) this.explode(w, false)
      return
    }
    for (let i = 0; i < 6; i++) {
      const s = this.seg[i]
      if (i > 0 && i < 5) {
        const p = this.seg[i - 1]
        const c = this.seg[i + 1]
        s[0] = (3 * s[0] + ((p[0] + c[0]) >> 1)) >> 2
        s[1] = (3 * s[1] + ((p[1] + c[1]) >> 1)) >> 2
      } else if (i === 5) {
        const held = w.input.held
        if (held & BTN.LEFT) {
          if (this.steer[0] > -56) this.steerBy(-3, 0)
        } else if (held & BTN.RIGHT) {
          if (this.steer[0] < 56) this.steerBy(3, 0)
        }
        if (held & BTN.UP) {
          if (this.steer[1] > -32) this.steerBy(0, -3)
        } else if (held & BTN.DOWN) {
          if (this.steer[1] < 32) this.steerBy(0, 3)
        }
      }
      this.boxes[i] = this.box(i)
      broadcast(w, this.msg(this.boxes[i], DF_MULT))
    }
  }
  private steerBy(dx: number, dy: number) {
    this.steer[0] += dx
    this.steer[1] += dy
    this.seg[5][0] += dx << 8
    this.seg[5][1] += dy << 8
  }
  /** 0x08038E8A (player control, tick < 90): root by v, each child 0x20 more along the facing */
  shift(v: number) {
    this.shifted = v
    const [dx, dy] = DIR_VEC[this.dir]
    for (let i = 0; i < 6; i++) {
      this.seg[i][0] += dx * (v + 0x20 * i)
      this.seg[i][1] += dy * (v + 0x20 * i)
    }
  }
  /** 0x08038E42: one x2.5 message with the head's box of this tick, the segments stop sending */
  launch(w: World) {
    if (this.dying >= 0) return
    this.dying = 0
    broadcast(w, this.msg(this.boxes[5], DF_LAUNCH_MULT))
  }
  /** every segment explodes (rand(0x1000) each): root first at the end, head first on a cancel */
  explode(w: World, cancel: boolean) {
    const bank = this.ctx.data.sprites
    for (let n = 0; n < 6; n++) {
      const k = cancel ? 5 - n : n
      const rec = DF_TABLE + 12 * (this.dir * 4 + (k === 5 ? 3 : 0))
      const [cx, cy] = recCentre(bank, rec, this.seg[k][0] >> 8, this.seg[k][1] >> 8)
      this.ctx.explosion(w, cx, cy, 0x1000 + w.rng.rand(0x1000, 'dragon fist explosion rate'))
    }
    this.removed = true
  }
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const order = this.dir === 1 ? [5, 4, 3, 2, 1, 0] : [0, 1, 2, 3, 4, 5]
    const [dx, dy] = DIR_VEC[this.dir]
    for (const k of order) {
      // the dying head's tiles step a frame ahead of the OAM (re-uploaded in place); the chain as built before the
      // player control's shift of the tick (skill audit sa_f41_skill5: the game's OAM)
      const i = k < 5 ? 0 : this.dying < 0 ? 1 : 1 + Math.min(2, (this.dying + 1) >> 3)
      const sh = this.shifted === null ? 0 : this.shifted + 0x20 * k
      drawRecAddr(r, bank, DF_TABLE + 12 * (this.dir * 4 + i), (this.seg[k][0] - dx * sh) >> 8, (this.seg[k][1] - dy * sh) >> 8)
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
  drawVolleyPieces(r, bank, dir, hx - dx * (len - 8), hy - dy * (len - 8), len - 8)
}

/** the wave's pieces: tail record at the origin, middle records every 8 px, head record at origin + dir * len */
export function drawVolleyPieces(r: Renderer, bank: Bank, dir: Dir, ox: number, oy: number, L: number) {
  const [dx, dy] = DIR_VEC[dir]
  const len = L + 8
  const hx = ox + dx * L
  const hy = oy + dy * L
  const base = VOLLEY_TABLE + 36 * dir
  // (the tail in front, the head behind: drawn head first - skill audit sa_f31_skill23)
  drawRecAdd(r, bank, base + 24, hx, hy)
  const ks: number[] = []
  for (let k = 8; k < len - 9; k += 8) ks.push(k)
  for (const k of ks.reverse()) drawRecAdd(r, bank, base + 12, ox + dx * k, oy + dy * k)
  drawRecAdd(r, bank, base, ox, oy)
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
    // (removed at the flare's cycle end three ticks on: skill audit sa_f41_skill3)
    if (this.ended && (this.t & 15) === 3) this.removed = true
  }
  draw(r: Renderer) {
    const set = this.ctx.fx('buu_bbk_flare')
    if (!set) return
    // the tiles are re-uploaded in place: a step shows a frame ahead of the OAM (skill audit sa_f41_skill3); the ball
    // goes with the beam's release, under the flare
    const t = this.t + 1
    if (!this.ended && this.alive()) {
      const st = set.anims.get('ball')?.dirs[this.dir][(t >> 2) & 1]
      if (st) set.drawFrame(r.ctx, st.frame, r.sx(this.x), r.sy(this.y), st.flip)
    }
    const k = (t >> 2) & 3
    for (const name of ['mirror', 'flare']) {
      const st = set.anims.get(name)?.dirs[this.dir][k]
      if (st) set.drawFrame(r.ctx, st.frame, r.sx(this.x), r.sy(this.y), st.flip)
    }
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Flame Kamehameha (link class 0x0802DB6C, vtable 0x0804C99C, table 0x087568B4): charge sparkle (record object at
 *  FLAME_CHARGE, fx_08721164 record 12 + f, f = t >> 3, > 3 -> 2 + (f & 1)) until the fire tick; then level + 3 links,
 *  the first at FLAME_FIRST, each link spawning the next 16 px further at its tick 8. Link record dir*7 + f with
 *  f = t >> 2: non-last links cycle 2,1,2,3 from f = 4, the last link 4,5,6,5 from f = 7. The d-pad perpendicular to
 *  the facing shifts link i by (i+1)^2/2 px per tick, clamped to +-(i+1)^2. Hits (update 0x0802DC1A): every link
 *  each tick sends one message to the first receiver (0x08011C70) with its box of the previous tick (the record box;
 *  a new link's first tick: the facing-down frame-0 box), mult 41 / 164 / 328, attack sub 12; accepted: sound
 *  48 + rand(2). All links vanish when the skill state ends. Lockstep *_vs_skill11*. */
export const FLAME_CHARGE: [number, number][] = [[8, -2], [12, -2], [12, -2], [4, -2]]
const FLAME_FIRST: [number, number][] = [[8, 16], [8, -32], [0, -6], [16, -6]]
const FLAME_TABLE = 0x087568b4

export class FlameKameFx extends Vis {
  ctx: BuuCtx
  owner: Actor
  dir: Dir
  n: number
  atk: AttackType
  mult: number
  /** per link: position, tick, perpendicular shift, the box of the previous tick */
  links: { x: number; y: number; t: number; s: number; box: Rect }[] = []
  alive: () => boolean
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, lv: number, alive: () => boolean, hit: { atk: AttackType; mult: number }) {
    super()
    this.ctx = ctx
    this.owner = owner
    this.dir = dir
    this.n = Math.min(2, lv) + 3
    this.alive = alive
    this.atk = hit.atk
    this.mult = hit.mult
    this.kind = 'flamekamefx'
    const [fx, fy] = FLAME_FIRST[dir]
    this.addLink(owner.px + fx, owner.py + fy)
  }
  private addLink(x: number, y: number) {
    // the constructor's box: the facing-down frame-0 record
    this.links.push({ x, y, t: 0, s: 0, box: this.recBox(FLAME_TABLE, x, y) })
  }
  private recBox(rec: number, x: number, y: number): Rect {
    const g = recGeomAt(this.ctx.data.sprites, rec) ?? [4, 4, 8, 8]
    return { x0: x - g[0], y0: y - g[1], x1: x - g[0] + g[2], y1: y - g[1] + g[3] }
  }
  private frame(i: number): number {
    let f = this.links[i].t >> 2
    if (i === this.n - 1) {
      if (f >= 7) {
        f = (f & 3) + 4
        if (f === 7) f = 5
      }
    } else if (f >= 4) {
      f &= 3
      if (f === 0) f = 2
    }
    return f
  }
  private at(i: number): [number, number] {
    const L = this.links[i]
    const [dx] = DIR_VEC[this.dir]
    const s = Math.trunc(L.s)
    return [L.x + (dx ? 0 : s), L.y + (dx ? s : 0)]
  }
  depth() {
    return this.links[0].y
  }
  update(w: World) {
    if (!this.alive()) {
      this.removed = true
      return
    }
    const [dx, dy] = DIR_VEC[this.dir]
    const held = w.input.held
    const perp = dx ? (held & BTN.UP ? -1 : held & BTN.DOWN ? 1 : 0) : held & BTN.LEFT ? -1 : held & BTN.RIGHT ? 1 : 0
    const n0 = this.links.length
    // newest link first (the object list updates newer objects first); a link spawned now updates from the next tick
    for (let i = n0 - 1; i >= 0; i--) {
      const L = this.links[i]
      L.t++
      const m = (i + 1) * (i + 1)
      L.s = Math.max(-m, Math.min(m, L.s + perp * (m / 2)))
      if (L.t === 8 && i === n0 - 1 && this.links.length < this.n) this.addLink(L.x + dx * 16, L.y + dy * 16)
      // one message to the first receiver, with the box of the previous tick (accepted: sound 48 + rand(2))
      if (broadcast(w, { team: 1, rect: L.box, dir: this.dir, mult: this.mult, atk: this.atk, attacker: this.owner, source: this }, true)) {
        audioEvent('sfx', { id: 48 + w.rng.rand(2, 'flame hit sfx'), vol: 64 })
      }
      const [x, y] = this.at(i)
      L.box = this.recBox(FLAME_TABLE + 0x54 * this.dir + 12 * this.frame(i), x, y)
    }
  }
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    this.links.forEach((_, i) => {
      const [x, y] = this.at(i)
      drawRecAddr(r, bank, FLAME_TABLE + 0x54 * this.dir + 12 * this.frame(i), x, y)
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

/** the Super Saiyan transform aura: looping 3 records of 0x08752D28 at rate 0x4000 at the footprint centre + (0, 1) */
export const SS_AURA = { id: 'fx_08752d28', n: 3, rate: 0x4000 }

export { drawRec }
