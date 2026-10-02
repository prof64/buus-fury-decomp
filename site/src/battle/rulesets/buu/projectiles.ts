// Buu's Fury projectiles and attack objects (player skills + enemy classes of bosses.json `projectiles`).
// Speeds, boxes, attack types, multipliers and lifetimes are the data's (notes §5, §8b); what is not decoded is
// marked INFERRED. Drawing is ours: most WF projectile graphics are code-driven effect runs that are not split
// per projectile in sprites.json, so shots are drawn procedurally in the game's colours unless a matching
// effect sheet is known (box-table addresses double as graphics tables).

import { DIR_VEC, overlaps, type Dir, type Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { Entity, World } from '../../core/world'
import { broadcast, type Actor, type HitMsg, type Receiver } from './actor'
import type { BuuCtx } from './context'
import { ATK, type AttackType } from './damage'

export type Team = 0 | 1 | 2

export abstract class Shot implements Entity, Receiver {
  id = 0
  kind = 'shot'
  removed = false
  layer = 1
  t = 0
  x: number
  y: number
  ctx: BuuCtx
  owner: Actor
  dir: Dir
  team: Team
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number) {
    this.ctx = ctx
    this.owner = owner
    this.dir = dir
    this.team = team
    this.x = x
    this.y = y
  }
  get px() {
    return this.x >> 8
  }
  get py() {
    return this.y >> 8
  }
  depth() {
    return this.py
  }
  abstract rect(): Rect
  abstract update(w: World): void
  abstract draw(r: Renderer, w: World): void
  /** owner died -> the projectile flies on harmlessly (messages 0x11/0x12) */
  ownerDead(): boolean {
    return !!(this.owner as unknown as { defeated?: boolean }).defeated || this.owner.removed
  }
  hit(w: World, rect: Rect, atk: AttackType, mult: number, first = true, team: Team = this.team): boolean {
    if (this.ownerDead() && this.team !== 1) return false
    const msg: HitMsg = { team, rect, dir: this.dir, mult, atk, attacker: this.owner, source: this }
    return broadcast(w, msg, first)
  }
  offCamera(w: World) {
    return !overlaps(this.rect(), w.camera)
  }
  /** wall test of every shot update: 0x08009324(box, map +0xAC) = any 8x8 tile under the box blocked on the map's
   *  projectile grid (or the box outside 0..2048). Arenas without collision data: the arena rectangle + a margin. */
  inWall(w: World, r: Rect = this.rect()) {
    const c = this.ctx.col
    if (c) return !c.testRect('shot', r)
    const b = w.arena.bounds
    return this.px < b.x0 - 8 || this.px > b.x1 + 8 || this.py < b.y0 - 40 || this.py > b.y1 + 8
  }
  /** px from (x, y) along `dir` (`step` px at a time, at most `max`) before the next step's point is a projectile
   *  wall (beams that extend to the wall) */
  wallDist(w: World, x: number, y: number, step: number, max: number): number {
    const [dx, dy] = DIR_VEC[this.dir]
    const c = this.ctx.col
    const b = w.arena.bounds
    let L = 0
    while (L < max) {
      const nx = x + dx * (L + step)
      const ny = y + dy * (L + step)
      if (c ? !c.test('shot', nx, ny, nx + 1, ny + 1) : nx < b.x0 - 16 || nx > b.x1 + 16 || ny < b.y0 - 40 || ny > b.y1 + 16) break
      L += step
    }
    return L
  }
  onMessage(_w: World, _msg: HitMsg): boolean {
    return false
  }
  drawDebug(r: Renderer) {
    r.strokeRect(this.rect(), '#fa0')
  }
}

export function glow(r: Renderer, x: number, y: number, rad: number, core: string, edge: string) {
  const g = r.ctx
  const cx = r.sx(x)
  const cy = r.sy(y)
  g.fillStyle = edge
  g.beginPath()
  g.arc(cx, cy, rad, 0, Math.PI * 2)
  g.fill()
  g.fillStyle = core
  g.beginPath()
  g.arc(cx, cy, Math.max(1, rad * 0.55), 0, Math.PI * 2)
  g.fill()
}

function fillRect(r: Renderer, rc: Rect, color: string) {
  r.ctx.fillStyle = color
  r.ctx.fillRect(r.sx(rc.x0), r.sy(rc.y0), rc.x1 - rc.x0, rc.y1 - rc.y0)
}

const centered = (x: number, y: number, w: number, h: number): Rect => ({ x0: x - (w >> 1), y0: y - (h >> 1), x1: x - (w >> 1) + w, y1: y - (h >> 1) + h })

// ---------------------------------------------------------------------------------------------------------------
/** straight-flying shot: player Energy Blast / Burning Attack / Volleyball beam, enemy ki blast, bullets, rifle
 *  shots, tank shells, shuriken, mummy wraps, stone spit, Pikkon barrage. Removed off-camera, at a wall or on hit. */
export interface StraightOpts {
  vx: number
  vy: number
  /** box relative to the position: x0 = px - ox, y0 = py - oy, size w x h */
  box: [number, number, number, number]
  atk: AttackType
  mult: number
  color: [string, string]
  radius: number
  /** destroyed by a player attack (kind 0) overlapping it (bullets, rifle shots, shuriken, tank shells) */
  fragile?: boolean
  life?: number
  onHitPlayer?: (w: World) => void
  explode?: boolean
  name: string
  /** effect sheet whose frames are this projectile's box/graphics table (e.g. 0x0874E968 gun bullet) */
  fx?: { id: string; frame: (dir: Dir, t: number) => number; flip?: (dir: Dir) => boolean }
  /** pierce = keep flying after a hit (not used by the game's shots) */
  hitAll?: boolean
}

export class StraightShot extends Shot {
  o: StraightOpts
  dead = -1
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, o: StraightOpts) {
    super(ctx, owner, dir, team, x, y)
    this.o = o
    this.kind = o.name
  }
  rect(): Rect {
    const [ox, oy, w, h] = this.o.box
    return { x0: this.px - ox, y0: this.py - oy, x1: this.px - ox + w, y1: this.py - oy + h }
  }
  update(w: World) {
    this.t++
    if (this.dead >= 0) {
      if (++this.dead > 8) this.removed = true
      return
    }
    this.x += this.o.vx
    this.y += this.o.vy
    if (this.o.life && this.t > this.o.life) return this.end(w, false)
    if (this.offCamera(w)) {
      this.removed = true
      return
    }
    if (this.inWall(w)) return this.end(w, true)
    const hitP = this.team !== 1
    if (this.hit(w, this.rect(), this.o.atk, this.o.mult, !this.o.hitAll)) {
      if (hitP && this.o.onHitPlayer) this.o.onHitPlayer(w)
      this.end(w, true)
    }
  }
  end(w: World, fx: boolean) {
    if (fx && this.o.explode !== false) this.ctx.explosion(w, this.px, this.py)
    this.dead = 0
    this.removed = true
  }
  onMessage(_w: World, msg: HitMsg): boolean {
    if (this.o.fragile && this.dead < 0 && msg.team === 1 && msg.atk.kind === 0 && msg.source !== this && overlaps(msg.rect, this.rect())) {
      this.removed = true
      this.ctx.explosion(_w, this.px, this.py)
    }
    return false
  }
  draw(r: Renderer) {
    const fx = this.o.fx && this.ctx.fx(this.o.fx.id)
    if (fx && this.o.fx) {
      fx.drawFrame(r.ctx, this.o.fx.frame(this.dir, this.t), r.sx(this.px), r.sy(this.py), this.o.fx.flip?.(this.dir) ?? false)
      return
    }
    const pulse = this.t & 4 ? 1 : 0
    glow(r, this.px, this.py, this.o.radius + pulse * 0.5, this.o.color[0], this.o.color[1])
  }
}

export const VEL = (dir: Dir, v: number): [number, number] => [DIR_VEC[dir][0] * v, DIR_VEC[dir][1] * v]

/** player / enemy ki blast class 0x080197B4 (3 px/frame, dir table 0x08719C40) */
export function kiBlast(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, atk = ATK[4], mult = 1024): StraightShot {
  const [vx, vy] = VEL(dir, 0x300)
  return new StraightShot(ctx, owner, dir, team, x, y, {
    vx, vy, box: [6, 6, 12, 12], atk, mult, color: team === 1 ? ['#fffbe0', '#ffb020'] : ['#ffffff', '#ff7050'], radius: 5, name: 'kiblast',
  })
}

// ---------------------------------------------------------------------------------------------------------------
/** growing beam (class 0x080206B8): Kamehameha family, Banshee, Big Bang Kamehameha, Kid Buu's Kamehameha.
 *  The head advances `speed` px/frame from the muzzle; every frame it hits each overlapping target, 1 or 2 hit
 *  events alternately (~1.5 hits/frame measured, notes §5) - the alternation is INFERRED. */
export class GrowBeam extends Shot {
  len = 0
  done = false
  atk: AttackType
  mult: number
  width: number
  speed: number
  maxLen: number
  color: [string, string]
  hitsPerFrame: (t: number) => number
  muzzle: () => [number, number]
  constructor(
    ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, muzzle: () => [number, number],
    o: { atk: AttackType; mult: number; width: number; speed: number; maxLen: number; color: [string, string]; hitsPerFrame?: (t: number) => number },
  ) {
    const [x, y] = muzzle()
    super(ctx, owner, dir, team, x << 8, y << 8)
    this.muzzle = muzzle
    this.atk = o.atk
    this.mult = o.mult
    this.width = o.width
    this.speed = o.speed
    this.maxLen = o.maxLen
    this.color = o.color
    this.hitsPerFrame = o.hitsPerFrame ?? ((t) => 1 + (t & 1))
    this.kind = 'beam'
  }
  rect(): Rect {
    const [dx, dy] = DIR_VEC[this.dir]
    const h = this.width >> 1
    const x0 = this.px
    const y0 = this.py
    const x1 = x0 + dx * Math.round(this.len)
    const y1 = y0 + dy * Math.round(this.len)
    if (dx) return { x0: Math.min(x0, x1), y0: y0 - h, x1: Math.max(x0, x1), y1: y0 + h }
    return { x0: x0 - h, y0: Math.min(y0, y1), x1: x0 + h, y1: Math.max(y0, y1) }
  }
  update(w: World) {
    this.t++
    const [mx, my] = this.muzzle()
    this.x = mx << 8
    this.y = my << 8
    if (this.done) {
      this.len -= this.speed * 2
      if (this.len <= 0) this.removed = true
      return
    }
    this.len = Math.min(this.maxLen, this.len + this.speed)
    // stop at the first projectile-wall tile ahead (INFERRED: the beam head uses the same grid as the shots)
    this.len = Math.min(this.len, this.wallDist(w, mx, my, 1, Math.ceil(this.len)))
    // measured hit rates: ~1.6 hits/frame on a target 40 px away, ~0.83 at 100 px (moves.json runs) - modelled as
    // a near part (first 44 px from the muzzle: 2 / 1 hits on alternate frames) and a far part (5 of 6 frames)
    const full = this.rect()
    const [ddx, ddy] = DIR_VEC[this.dir]
    const NEAR = 44
    const cut = (from: number, to: number): Rect => {
      const ax = this.px + ddx * from
      const ay = this.py + ddy * from
      const bx = this.px + ddx * to
      const by = this.py + ddy * to
      return ddx ? { x0: Math.min(ax, bx), y0: full.y0, x1: Math.max(ax, bx), y1: full.y1 } : { x0: full.x0, y0: Math.min(ay, by), x1: full.x1, y1: Math.max(ay, by) }
    }
    const L = Math.round(this.len)
    const n = this.hitsPerFrame(this.t)
    for (let i = 0; i < n; i++) this.hit(w, cut(0, Math.min(L, NEAR)), this.atk, this.mult, false)
    if (L > NEAR && this.t % 6 !== 0) this.hit(w, cut(NEAR, L), this.atk, this.mult, false)
  }
  stop() {
    this.done = true
  }
  draw(r: Renderer) {
    const rc = this.rect()
    if (rc.x1 - rc.x0 <= 0 || rc.y1 - rc.y0 <= 0) return
    fillRect(r, rc, this.color[1])
    const [dx] = DIR_VEC[this.dir]
    const inner = dx ? { x0: rc.x0, y0: rc.y0 + (this.width >> 2), x1: rc.x1, y1: rc.y1 - (this.width >> 2) } : { x0: rc.x0 + (this.width >> 2), y0: rc.y0, x1: rc.x1 - (this.width >> 2), y1: rc.y1 }
    fillRect(r, inner, this.color[0])
    const [hx, hy] = [this.px + DIR_VEC[this.dir][0] * this.len, this.py + DIR_VEC[this.dir][1] * this.len]
    glow(r, hx, hy, (this.width >> 1) + 2 + (this.t & 2 ? 1 : 0), this.color[0], this.color[1])
    glow(r, this.px, this.py, (this.width >> 1) + 1, this.color[0], this.color[1])
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** instant beam (class 0x08036FA2, Majin Vegeta / Yakon; player Final Flash): 24 px wide, extends from the origin
 *  to the wall in 8 px steps, one hit per frame while the target overlaps. */
export class FullBeam extends Shot {
  len = 0
  life: number
  atk: AttackType
  mult: number
  width: number
  color: [string, string]
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, o: { atk: AttackType; mult: number; life: number; width?: number; color?: [string, string] }) {
    super(ctx, owner, dir, team, x << 8, y << 8)
    this.atk = o.atk
    this.mult = o.mult
    this.life = o.life
    this.width = o.width ?? 24
    this.color = o.color ?? ['#fffbd0', '#f0d020']
    this.kind = 'fullbeam'
  }
  rect(): Rect {
    const [dx, dy] = DIR_VEC[this.dir]
    const h = this.width >> 1
    const x1 = this.px + dx * this.len
    const y1 = this.py + dy * this.len
    if (dx) return { x0: Math.min(this.px, x1), y0: this.py - h, x1: Math.max(this.px, x1), y1: this.py + h }
    return { x0: this.px - h, y0: Math.min(this.py, y1), x1: this.px + h, y1: Math.max(this.py, y1) }
  }
  update(w: World) {
    this.t++
    // 8 px steps until the next step is a projectile-wall tile
    if (this.t === 1) this.len = this.wallDist(w, this.px, this.py, 8, 400)
    if (this.t > this.life) {
      this.removed = true
      return
    }
    this.hit(w, this.rect(), this.atk, this.mult, false)
  }
  draw(r: Renderer) {
    const rc = this.rect()
    const f = this.t & 2 ? 1 : 0
    fillRect(r, rc, this.color[1])
    const [dx] = DIR_VEC[this.dir]
    const q = (this.width >> 2) + f
    fillRect(r, dx ? { x0: rc.x0, y0: rc.y0 + q, x1: rc.x1, y1: rc.y1 - q } : { x0: rc.x0 + q, y0: rc.y0, x1: rc.x1 - q, y1: rc.y1 }, this.color[0])
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** area that hits every `every` frames for `life` frames (Big Bang explosion, Masenko blast, Super Buu ball
 *  explosion, Dragon Fist head, Kamikaze ghost burst) */
export class AreaHit extends Shot {
  box: Rect
  life: number
  every: number
  atk: AttackType
  mult: number
  grow: number
  color: [string, string]
  first: boolean
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, box: Rect, o: { life: number; every: number; atk: AttackType; mult: number; grow?: number; color?: [string, string]; first?: boolean }) {
    super(ctx, owner, dir, team, ((box.x0 + box.x1) >> 1) << 8, ((box.y0 + box.y1) >> 1) << 8)
    this.box = box
    this.life = o.life
    this.every = o.every
    this.atk = o.atk
    this.mult = o.mult
    this.grow = o.grow ?? 0
    this.color = o.color ?? ['#ffffff', '#ffd040']
    this.first = o.first ?? false
    this.kind = 'area'
  }
  rect(): Rect {
    return this.box
  }
  update(w: World) {
    this.t++
    if (this.t > this.life) {
      this.removed = true
      return
    }
    if (this.grow && this.t % 2 === 0) this.box = { x0: this.box.x0 - this.grow, y0: this.box.y0 - this.grow, x1: this.box.x1 + this.grow, y1: this.box.y1 + this.grow }
    if ((this.t - 1) % this.every === 0) this.hit(w, this.box, this.atk, this.mult, this.first)
  }
  draw(r: Renderer) {
    const rc = this.box
    const cx = (rc.x0 + rc.x1) >> 1
    const cy = (rc.y0 + rc.y1) >> 1
    const rad = Math.max(4, (rc.x1 - rc.x0) >> 1)
    r.ctx.globalAlpha = this.t > this.life - 8 ? 0.5 : 0.85
    glow(r, cx, cy, rad * (0.85 + 0.15 * Math.sin(this.t / 2)), this.color[0], this.color[1])
    r.ctx.globalAlpha = 1
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Big Bang ball (0x08037480): grows while B is held; released after >= 60 frames it moves 0.58 px/frame and
 *  explodes on contact (17-24 hits measured). Multiplier from the charged EP (fitted: mult = EP*0.284, INFERRED). */
export class BigBangBall extends Shot {
  charge256 = 0
  /** extra radius from holding past 60 frames (fitted to the hold-150 runs: +0 / +0.36 / +0.48 px per frame) */
  grow = 0
  launched = false
  boom = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 1, x, y)
    this.kind = 'bigbang'
  }
  get mult() {
    return Math.max(1, Math.floor((this.charge256 * 73) / 65536))
  }
  get radius() {
    return 9 + Math.min(44, Math.floor(this.grow))
  }
  rect() {
    const r = this.radius
    return centered(this.px, this.py, r * 2, r * 2)
  }
  update(w: World) {
    this.t++
    if (!this.launched) return
    const [dx, dy] = DIR_VEC[this.dir]
    this.x += dx * 148
    this.y += dy * 148
    if (this.offCamera(w) || this.inWall(w)) {
      this.removed = true
      return
    }
    // probe: any enemy body overlapping -> explode
    const probe = this.ctx.liveEnemies().some((e) => e.hittable && overlaps(e.bodyBox(), this.rect()))
    if (probe) {
      this.removed = true
      const sz = (this.radius + 15) * 2
      w.add(new AreaHit(this.ctx, this.owner, this.dir, 1, centered(this.px, this.py, sz, sz), { life: 110, every: 5, atk: ATK[13], mult: this.mult, color: ['#e0f8ff', '#40a0ff'] }))
    }
  }
  draw(r: Renderer) {
    glow(r, this.px, this.py, this.radius + (this.t & 4 ? 1 : 0), '#e8fbff', '#3090ff')
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Masenko (skill 17): cursor that advances while B is held; ~51 frames after the release a blast lands on it.
 *  Cursor speed and delay are fitted to the measured fire frames (f111 for hold 60, f203 for hold 150; INFERRED). */
export class MasenkoCursor extends Shot {
  dist = 24
  released = -1
  mult: number
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, mult: number) {
    super(ctx, owner, dir, 1, 0, 0)
    this.mult = mult
    this.kind = 'masenko'
  }
  pos(): [number, number] {
    const [cx, cy] = this.owner.center()
    const [dx, dy] = DIR_VEC[this.dir]
    return [cx + dx * this.dist, cy + dy * this.dist]
  }
  rect() {
    const [x, y] = this.pos()
    return centered(x, y, 32, 32)
  }
  update(w: World) {
    this.t++
    if (this.released < 0) {
      this.dist = Math.min(48, this.dist + 0.5)
      const [x, y] = this.pos()
      this.x = x << 8
      this.y = y << 8
      return
    }
    if (this.t - this.released >= 51) {
      this.removed = true
      const [x, y] = this.pos()
      w.add(new AreaHit(this.ctx, this.owner, this.dir, 1, centered(x, y, 32, 32), { life: 16, every: 100, atk: ATK[7], mult: this.mult, color: ['#fff8d0', '#ffe040'] }))
    }
  }
  release() {
    if (this.released < 0) this.released = this.t
  }
  draw(r: Renderer) {
    const [x, y] = this.pos()
    const g = r.ctx
    g.strokeStyle = this.released < 0 ? '#ffe040' : this.t & 2 ? '#fff' : '#ffe040'
    g.strokeRect(r.sx(x) - 6.5, r.sy(y) - 6.5, 13, 13)
    if (this.released >= 0) glow(r, this.owner.center()[0], this.owner.center()[1] - 12, 5, '#fff8d0', '#ffd020')
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Kamikaze ghost (skill 13): homes in on the closest enemy, explodes on contact. No hit was recorded in the
 *  emulator, so the blast (energy x2.0) is INFERRED. */
export class Ghost extends Shot {
  vx = 0
  vy = 0
  free = false
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 1, x, y)
    this.kind = 'ghost'
  }
  rect() {
    return centered(this.px, this.py, 12, 12)
  }
  update(w: World) {
    this.t++
    if (!this.free) {
      this.y += Math.sin(this.t / 6) * 40
      return
    }
    const targets = this.ctx.liveEnemies().filter((e) => e.hittable)
    if (!targets.length || this.t > 400) {
      this.removed = true
      this.ctx.puff(w, this.px, this.py)
      return
    }
    let best = targets[0]
    let bd = Infinity
    for (const e of targets) {
      const [cx, cy] = e.center()
      const d = (cx - this.px) ** 2 + (cy - this.py) ** 2
      if (d < bd) {
        bd = d
        best = e
      }
    }
    const [cx, cy] = best.center()
    const dx = cx - this.px
    const dy = cy - this.py
    const m = Math.max(1, Math.hypot(dx, dy))
    this.vx = (this.vx * 7 + (dx / m) * 384) / 8
    this.vy = (this.vy * 7 + (dy / m) * 384) / 8
    this.x += Math.round(this.vx)
    this.y += Math.round(this.vy)
    if (overlaps(best.bodyBox(), this.rect())) {
      this.removed = true
      this.ctx.explosion(w, this.px, this.py)
      this.hit(w, centered(this.px, this.py, 32, 32), ATK[1], 2048, false)
    }
  }
  draw(r: Renderer) {
    r.ctx.globalAlpha = 0.8
    glow(r, this.px, this.py, 5, '#ffffff', '#c8d8ff')
    r.ctx.fillStyle = '#334'
    r.ctx.fillRect(r.sx(this.px) - 2, r.sy(this.py) - 1, 1, 1)
    r.ctx.fillRect(r.sx(this.px) + 1, r.sy(this.py) - 1, 1, 1)
    r.ctx.globalAlpha = 1
  }
}

// ---------------------------------------------------------------------------------------------------------------
// enemy-only classes

/** energy wave (0x08049C98): segment from tail to head; head grows 4/frame to 48 px, then the segment moves
 *  4 px/frame; max 240 frames; after a hit / wall the tail catches up and it vanishes. */
export class EnergyWave extends Shot {
  len = 8
  mode = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x, y)
    this.kind = 'wave'
  }
  rect(): Rect {
    const [dx, dy] = DIR_VEC[this.dir]
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    return { x0: Math.min(this.px, hx) - 4, y0: Math.min(this.py, hy) - 4, x1: Math.max(this.px, hx) + 4, y1: Math.max(this.py, hy) + 4 }
  }
  update(w: World) {
    this.t++
    const [dx, dy] = DIR_VEC[this.dir]
    if (this.mode === 1) {
      this.x += dx * 4 * 256
      this.y += dy * 4 * 256
      this.len -= 4
      if (this.len <= 0) this.removed = true
      return
    }
    if (this.len < 48) this.len += 4
    else {
      this.x += dx * 4 * 256
      this.y += dy * 4 * 256
    }
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    if (this.t > 240 || this.inWall(w, { x0: hx - 4, y0: hy - 4, x1: hx + 4, y1: hy + 4 }) || this.offCamera(w)) {
      this.mode = 1
      return
    }
    if (this.hit(w, this.rect(), ATK[1], 1024)) {
      this.ctx.puff(w, hx, hy)
      this.mode = 1
    }
  }
  onMessage(w: World, msg: HitMsg): boolean {
    // destroyed only by a head-on player attack overlapping the head +-12 px
    if (msg.team !== 1 || this.mode) return false
    const [dx, dy] = DIR_VEC[this.dir]
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    if (msg.dir === (this.dir ^ 1) && overlaps(msg.rect, { x0: hx - 12, y0: hy - 12, x1: hx + 12, y1: hy + 12 })) {
      this.mode = 1
      this.ctx.puff(w, hx, hy)
    }
    return false
  }
  draw(r: Renderer) {
    const rc = this.rect()
    fillRect(r, { x0: rc.x0 + 2, y0: rc.y0 + 2, x1: rc.x1 - 2, y1: rc.y1 - 2 }, '#60e0ff')
    fillRect(r, { x0: rc.x0 + 3, y0: rc.y0 + 3, x1: rc.x1 - 3, y1: rc.y1 - 3 }, '#ffffff')
  }
}

/** flame (0x0804BFCC): 2 px/f along the facing + jitter, 16x16, 20 frames, at most one hit (energy sub 4, x0.05) */
export class Flame extends Shot {
  vx: number
  vy: number
  done = false
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, w: World) {
    super(ctx, owner, dir, 0, x, y)
    const [dx, dy] = DIR_VEC[dir]
    this.vx = dx * 512 + w.rng.rand(192, 'flame jitter') - 96
    this.vy = dy * 512 + w.rng.rand(192, 'flame jitter') - 96
    this.kind = 'flame'
  }
  rect() {
    return centered(this.px, this.py, 16, 16)
  }
  update(w: World) {
    this.t++
    this.x += this.vx
    this.y += this.vy
    if (this.t >= 20) {
      this.removed = true
      return
    }
    if (!this.done && this.hit(w, this.rect(), ATK[4], 51)) this.done = true
  }
  draw(r: Renderer) {
    glow(r, this.px, this.py, 3 + (this.t >> 3), '#fff070', this.t > 12 ? '#c03010' : '#ff7010')
  }
}

/** bomb (0x0804A2E8): lobbed to the target point (~40 f), 30 f fuse, explosion box 16x16 (energy x1.0) */
export class Bomb extends Shot {
  vx: number
  vy: number
  z = -8
  vz = 2
  landed = -1
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, tx: number, ty: number) {
    super(ctx, owner, dir, 0, x, y)
    const cl = (v: number) => Math.max(-128, Math.min(128, v))
    let ddx = cl(tx - (x >> 8))
    let ddy = cl(ty - (y >> 8))
    if (dir >= 2) ddy = 0
    else ddx = 0
    this.vx = (ddx * 0x71d) >> 8
    this.vy = (ddy * 0x71d) >> 8
    this.kind = 'bomb'
  }
  rect() {
    return centered(this.px, this.py, 16, 16)
  }
  update(w: World) {
    this.t++
    if (this.landed < 0) {
      this.x += this.vx
      this.y += this.vy
      this.z -= this.vz
      this.vz -= 0.125
      if (this.z >= 0 || this.t >= 40) {
        this.z = 0
        this.landed = this.t
      }
      return
    }
    if (this.t - this.landed >= 30) {
      this.removed = true
      this.ctx.explosion(w, this.px, this.py)
      this.hit(w, this.rect(), ATK[1], 1024)
    }
  }
  draw(r: Renderer) {
    const blink = this.landed >= 0 && this.t & 2
    glow(r, this.px, this.py + this.z, 4, blink ? '#ffffff' : '#ff5040', '#401010')
  }
}

/** Buu stretch-arm hitbox (0x08028C04): length 8..32 (+4/f), then shrinks; one hit (melee sub 0, STR) */
export class StretchArm extends Shot {
  ox: number
  oy: number
  hitDone = false
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, ox: number, oy: number) {
    super(ctx, owner, dir, 0, 0, 0)
    this.ox = ox
    this.oy = oy
    this.kind = 'stretch'
  }
  length() {
    return this.t <= 7 ? 4 + 4 * this.t : this.t === 8 ? 32 : Math.max(0, 32 - 4 * (this.t - 8))
  }
  rect(): Rect {
    const x = this.owner.px + this.ox
    const y = this.owner.py + this.oy
    const L = this.length()
    switch (this.dir) {
      case 3:
        return { x0: x, y0: y - 8, x1: x + L, y1: y + 8 }
      case 2:
        return { x0: x - L, y0: y - 8, x1: x, y1: y + 8 }
      case 0:
        return { x0: x - 8, y0: y, x1: x + 8, y1: y + L }
      default:
        return { x0: x - 8, y0: y - L, x1: x + 8, y1: y }
    }
  }
  update(w: World) {
    this.t++
    if (this.t > 15) {
      this.removed = true
      return
    }
    if (!this.hitDone && this.hit(w, this.rect(), ATK[0], 1024)) this.hitDone = true
  }
  draw(r: Renderer) {
    const rc = this.rect()
    fillRect(r, rc, '#f080b0')
    fillRect(r, { x0: rc.x0 + 1, y0: rc.y0 + 3, x1: rc.x1 - 1, y1: rc.y1 - 3 }, '#ffb0d0')
  }
}

/** Majin Buu candy beam (0x080286D4): 8 -> 40 px, 19 frames, energy x0 -> the player becomes candy (300 f) */
export class CandyBeam extends Shot {
  ox: number
  oy: number
  done = false
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, ox: number, oy: number) {
    super(ctx, owner, dir, 0, 0, 0)
    this.ox = ox
    this.oy = oy
    this.kind = 'candybeam'
  }
  rect(): Rect {
    const x = this.owner.px + this.ox
    const y = this.owner.py + this.oy
    const head = Math.min(40, 8 + 4 * this.t)
    const tail = Math.max(0, (this.t - 9) * 4)
    const [dx, dy] = DIR_VEC[this.dir]
    const a = [x + dx * tail, y + dy * tail]
    const b = [x + dx * head, y + dy * head]
    return { x0: Math.min(a[0], b[0]) - 3, y0: Math.min(a[1], b[1]) - 3, x1: Math.max(a[0], b[0]) + 3, y1: Math.max(a[1], b[1]) + 3 }
  }
  update(w: World) {
    this.t++
    if (this.t > 19) {
      this.removed = true
      return
    }
    if (!this.done && this.hit(w, this.rect(), ATK[1], 0)) {
      this.done = true
      const p = this.ctx.player
      if (p && !p.dead) {
        this.ctx.candy = 300
        w.logEvent('info', 'player turned into candy (300 f)')
      }
    }
  }
  draw(r: Renderer) {
    const rc = this.rect()
    fillRect(r, rc, '#ff70c0')
    fillRect(r, { x0: rc.x0 + 2, y0: rc.y0 + 2, x1: rc.x1 - 2, y1: rc.y1 - 2 }, '#ffe0f0')
  }
}

/** Super Buu lobbed ball (0x08021280): 60 f charge 40 px above the boss, 43 f arc to the player's position at launch,
 *  explosion box 32 -> 64 px, Masenko type x16.2 (POW) */
export class SuperBall extends Shot {
  phase = 0
  sx = 0
  sy = 0
  tx = 0
  ty = 0
  z = -40
  constructor(ctx: BuuCtx, owner: Actor) {
    super(ctx, owner, 3, 0, 0, 0)
    this.kind = 'superball'
  }
  rect() {
    return centered(this.px, this.py + this.z, 16, 16)
  }
  update(w: World) {
    this.t++
    if (this.phase === 0) {
      this.x = (this.owner.px + 8) << 8
      this.y = (this.owner.py) << 8
      this.z = -40
      if (this.t >= 60) {
        const p = this.ctx.player
        const [tx, ty] = p ? p.footCenter() : [this.px, this.py]
        this.sx = this.x
        this.sy = this.y
        this.tx = tx << 8
        this.ty = ty << 8
        this.phase = 1
        this.t = 0
      }
      return
    }
    if (this.phase === 1) {
      const k = Math.min(1, this.t / 43)
      this.x = Math.round(this.sx + (this.tx - this.sx) * k)
      this.y = Math.round(this.sy + (this.ty - this.sy) * k)
      this.z = -40 * (1 - k) - 32 * Math.sin(Math.PI * k)
      if (this.t >= 43) {
        this.removed = true
        this.ctx.explosion(w, this.px, this.py)
        w.add(new AreaHit(this.ctx, this.owner, 0, 0, centered(this.px, this.py, 32, 32), { life: 16, every: 100, atk: ATK[7], mult: 16589, grow: 2, color: ['#ffffff', '#ff60a0'] }))
      }
    }
  }
  draw(r: Renderer) {
    glow(r, this.px, this.py + this.z, this.phase === 0 ? 4 + Math.min(6, this.t / 10) : 10, '#ffffff', '#ff60b0')
  }
}

/** Super Janemba energy wave (0x080359D4): 2.625 px/f, 32x64 box (64x32 vertical), energy sub 1 x1.0 */
export class SJWave extends Shot {
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x, y)
    this.kind = 'sjwave'
  }
  rect(): Rect {
    if (this.dir >= 2) return { x0: this.px - 32, y0: this.py - 8, x1: this.px, y1: this.py + 56 }
    return { x0: this.px - 8, y0: this.py - 32, x1: this.px + 56, y1: this.py }
  }
  update(w: World) {
    this.t++
    const [dx, dy] = DIR_VEC[this.dir]
    this.x += dx * 672
    this.y += dy * 672
    if (this.offCamera(w)) {
      this.removed = true
      return
    }
    if (this.inWall(w) || this.hit(w, this.rect(), ATK[1], 1024)) {
      this.removed = true
      this.ctx.explosion(w, this.px, this.py)
    }
  }
  draw(r: Renderer) {
    // graphics: the box table 0x0875A6A8 doubles as the wave's OBJ frames (64x15 vertical, 15x64 horizontal)
    const fx = this.ctx.fx('fx_0875a6a8')
    if (fx) {
      fx.drawFrame(r.ctx, this.dir, r.sx(this.px), r.sy(this.py), false)
      return
    }
    const rc = this.rect()
    glow(r, (rc.x0 + rc.x1) >> 1, (rc.y0 + rc.y1) >> 1, 14, '#f0c0ff', '#9020c0')
  }
}

/** Dabura's dark sphere (0x0803AEA8): radius 1 px +51/256 per frame to 64 px (~315 f), then expands 0.5 px/f while
 *  fading (~125 f). Every frame a melee x1.0 hit (STR) on a player inside; each player energy hit shrinks it by
 *  102/256 px; below 1 px it is destroyed (Dabura stunned). */
export class DarkSphere extends Shot {
  r256 = 256
  alpha = 1
  constructor(ctx: BuuCtx, owner: Actor, x: number, y: number) {
    super(ctx, owner, 0, 0, x, y)
    this.kind = 'sphere'
    ctx.sphereState = 0
  }
  get rad() {
    return this.r256 / 256
  }
  rect() {
    const rr = Math.round(this.rad)
    return centered(this.px, this.py, rr * 2, rr * 2)
  }
  update(w: World) {
    this.t++
    if (this.ownerDead()) {
      this.removed = true
      return
    }
    if (this.ctx.sphereState === 0) {
      this.r256 = Math.min(64 * 256, this.r256 + 51)
      if (this.r256 >= 64 * 256) this.ctx.sphereState = 2
    } else if (this.ctx.sphereState === 2) {
      this.r256 += 128
      this.alpha = Math.max(0, ((128 - this.rad) * 50) / 64 / 50)
      if (this.alpha <= 0 || this.rad >= 128) {
        this.ctx.sphereState = 3
        this.removed = true
        return
      }
    }
    const p = this.ctx.player
    if (p && !p.dead) {
      const [cx, cy] = p.center()
      if ((cx - this.px) ** 2 + (cy - this.py) ** 2 <= this.rad * this.rad) this.hit(w, p.bodyBox(), ATK[2], 1024)
    }
  }
  onMessage(w: World, msg: HitMsg): boolean {
    if (msg.team !== 1 || msg.atk.kind !== 1 || this.ctx.sphereState !== 0) return false
    if (!overlaps(msg.rect, this.rect())) return false
    this.r256 -= 102
    if (this.r256 < 256) {
      this.ctx.sphereState = 1
      this.removed = true
      for (let i = 0; i < 4; i++) this.ctx.puff(w, this.px + (i & 1 ? 8 : -8), this.py + (i & 2 ? 8 : -8))
    }
    return false
  }
  draw(r: Renderer) {
    const g = r.ctx
    g.globalAlpha = 0.45 * this.alpha
    g.fillStyle = '#300040'
    g.beginPath()
    g.arc(r.sx(this.px), r.sy(this.py), this.rad, 0, Math.PI * 2)
    g.fill()
    g.globalAlpha = 0.9 * this.alpha
    g.strokeStyle = '#c050ff'
    g.stroke()
    g.globalAlpha = 1
  }
}

/** Bio Mech grapple arm (0x0804B380): extends like a wave to 80 px, melee sub 0 x1.0 (STR); a hit holds the player
 *  and reels him in (INFERRED) */
export class GrappleArm extends Shot {
  len = 8
  back = false
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x, y)
    this.kind = 'grapple'
  }
  rect(): Rect {
    const [dx, dy] = DIR_VEC[this.dir]
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    return { x0: Math.min(this.px, hx) - 4, y0: Math.min(this.py, hy) - 4, x1: Math.max(this.px, hx) + 4, y1: Math.max(this.py, hy) + 4 }
  }
  update(w: World) {
    this.t++
    if (this.back) {
      this.len -= 4
      if (this.len <= 0) this.removed = true
      return
    }
    this.len += 4
    if (this.len >= 80) this.back = true
    if (this.hit(w, this.rect(), ATK[0], 1024)) {
      this.back = true
      const p = this.ctx.player
      if (p && !p.dead) p.hold(w, this.owner, 'grapple', 20)
    }
  }
  draw(r: Renderer) {
    const rc = this.rect()
    fillRect(r, { x0: rc.x0 + 3, y0: rc.y0 + 3, x1: rc.x1 - 3, y1: rc.y1 - 3 }, '#a0a0b0')
  }
}

/** Broly's arena rock (0x08036A44): 32x32 solid block; intact -> cracked -> destroyed by Broly's landings */
export class Rock implements Entity {
  id = 0
  kind = 'rock'
  removed = false
  layer = 0
  hp = 2
  x: number
  y: number
  constructor(x: number, y: number) {
    this.x = x
    this.y = y
  }
  /** collision box +16..+48 on both axes (ctor 0x08036A44): the rock fills the pit in the Volcano Summit map */
  rect(): Rect {
    return { x0: this.x + 16, y0: this.y + 16, x1: this.x + 48, y1: this.y + 48 }
  }
  depth() {
    return this.y + 40
  }
  update() {}
  fx: import('../../core/sprites').SpriteSet | undefined
  draw(r: Renderer) {
    // def table 0x0875F7AC: frame 1 intact, 2 cracked (frames are anchored at -11,-11)
    if (this.fx) {
      this.fx.drawFrame(r.ctx, this.hp >= 2 ? 1 : 2, r.sx(this.x) + 1, r.sy(this.y) - 3, false)
      return
    }
    const g = r.ctx
    const x = r.sx(this.x)
    const y = r.sy(this.y)
    g.fillStyle = '#5a3b22'
    g.fillRect(x + 1, y + 10, 30, 36)
    g.fillStyle = '#8a6040'
    g.fillRect(x + 3, y + 6, 26, 18)
    g.fillStyle = '#b08058'
    g.fillRect(x + 6, y + 8, 14, 6)
    if (this.hp < 2) {
      g.fillStyle = '#2a1608'
      g.fillRect(x + 14, y + 12, 2, 26)
      g.fillRect(x + 10, y + 24, 12, 2)
    }
  }
}
