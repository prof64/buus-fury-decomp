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
import { audioEvent } from '../../audio/bus'
import { Debris, type BeamGfx, drawRec, drawRecAddr, FxAnim } from './fx'
import { drawFinalFlash, drawRecAdd, GHOST_SLOTS } from './skillfx'

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
  /** the game's graphics: record `frame(dir, t)` (ROM order) of the fx_<addr> run `id` (the box table doubles as
   *  the graphics table, e.g. 0x0874E968 gun bullet); `t` = the object's tick counter (1 on the first update) */
  fx?: { id: string; frame: (dir: Dir, t: number) => number }
  /** end of flight (wall / hit) shows record `rec(dir, t)` of the same run in place for `ticks` ticks instead of an
   *  explosion object (ki blast spark: record 16 for 16 ticks, 0x080198E4) */
  fxEnd?: { rec: (dir: Dir, t: number) => number; ticks: number }
  /** custom graphics (multi-piece objects, e.g. the Ultra Volleyball wave) */
  fxDraw?: (r: Renderer, s: StraightShot) => void
  /** smoke trail (rifle bullet, tank shell): every 4 ticks a generic effect of `n` records from record `rec` of run
   *  `id` (rate 0x2AAA) at the position - 8 * velocity */
  trail?: { id: string; rec: number; n: number; rate: number }
  /** ticks before the shot moves / hits (velocity 0 until launched, e.g. Burning Attack held in the hands) */
  delay?: number
  /** RNG draws of the game's object at spawn / on an accepted hit (sound picks: rand(2) -> 0x0803E378) */
  rngSpawn?: number[]
  rngHit?: number[]
  /** the sound those picks choose: PlaySfx(base + the first draw) (ki blast: 20 / 48) */
  sfxSpawn?: number
  sfxHit?: number
  /** pierce = keep flying after a hit (not used by the game's shots) */
  hitAll?: boolean
}

export class StraightShot extends Shot {
  o: StraightOpts
  /** ticks shown in the end state (fxEnd), -1 while flying */
  dead = -1
  private spawned = false
  /** spawn position (24.8) */
  readonly x0: number
  readonly y0: number
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, o: StraightOpts) {
    super(ctx, owner, dir, team, x, y)
    this.o = o
    this.kind = o.name
    this.x0 = x
    this.y0 = y
  }
  /** the RNG draws the game's constructor makes (call at creation; otherwise they run on the first update) */
  spawnRng(w: World) {
    if (this.spawned) return
    this.spawned = true
    const r = (this.o.rngSpawn ?? []).map((n) => w.rng.rand(n, `${this.o.name} spawn sfx`))
    if (this.o.sfxSpawn != null) audioEvent('sfx', { id: this.o.sfxSpawn + (r[0] ?? 0), vol: 64 })
  }
  /** launch a delayed shot from (x, y) (24.8) */
  launch(x: number, y: number) {
    this.x = x
    this.y = y
    this.o.delay = 0
  }
  rect(): Rect {
    const [ox, oy, w, h] = this.o.box
    return { x0: this.px - ox, y0: this.py - oy, x1: this.px - ox + w, y1: this.py - oy + h }
  }
  update(w: World) {
    this.spawnRng(w)
    this.t++
    if (this.dead >= 0) {
      if (++this.dead >= (this.o.fxEnd?.ticks ?? 0)) this.removed = true
      return
    }
    if (this.o.delay && this.t <= this.o.delay) return
    this.x += this.o.vx
    this.y += this.o.vy
    const tr = this.o.trail
    if (tr && (this.t & 3) === 0) {
      // smoke behind the shot (0x08027562: pool room >= 0x40)
      this.ctx.fxAnim(w, tr.id, (this.x - 8 * this.o.vx) >> 8, (this.y - 8 * this.o.vy) >> 8, tr.rec, tr.n, tr.rate)
    }
    if (this.o.life && this.t > this.o.life) return this.end(w, false)
    if (this.offCamera(w)) {
      this.removed = true
      return
    }
    if (this.inWall(w)) return this.end(w, true)
    const hitP = this.team !== 1
    if (this.hit(w, this.rect(), this.o.atk, this.o.mult, !this.o.hitAll)) {
      const r = (this.o.rngHit ?? []).map((n) => w.rng.rand(n, `${this.o.name} hit sfx`))
      if (this.o.sfxHit != null) audioEvent('sfx', { id: this.o.sfxHit + (r[0] ?? 0), vol: 64 })
      if (hitP && this.o.onHitPlayer) this.o.onHitPlayer(w)
      this.end(w, true)
    }
  }
  end(w: World, fx: boolean) {
    if (this.o.fxEnd) {
      // spark in place (no explosion object, no further hits)
      this.dead = 0
      return
    }
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
    if (this.o.fxDraw) {
      if (this.dead < 0) this.o.fxDraw(r, this)
      return
    }
    const fx = this.o.fx && this.ctx.fx(this.o.fx.id)
    if (fx && this.o.fx) {
      const i = this.dead >= 0 && this.o.fxEnd ? this.o.fxEnd.rec(this.dir, this.t) : this.o.fx.frame(this.dir, this.t)
      drawRec(r, fx, i, this.px, this.py)
      return
    }

  }
}

export const VEL = (dir: Dir, v: number): [number, number] => [DIR_VEC[dir][0] * v, DIR_VEC[dir][1] * v]

/** ki blast frame (update 0x08019888): f = t >> 3, after the 4 records 0..3 it alternates 2 / 3 every 8 ticks */
export const kiFrame = (t: number) => {
  const f = t >> 3
  return f > 3 ? 2 + (f & 1) : f
}

/** player / enemy ki blast class 0x080197B4 (3 px/frame, dir table 0x08719C40). Graphics: run 0x0871370C, records
 *  dir*4 + kiFrame(t); on a wall or an accepted hit it stops and shows the spark record 16 for 16 ticks. The ctor and
 *  an accepted hit each pick a sound with rand(2). */
export function kiBlast(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, atk = ATK[4], mult = 1024, w?: World): StraightShot {
  const [vx, vy] = VEL(dir, 0x300)
  const s = new StraightShot(ctx, owner, dir, team, x, y, {
    vx, vy, box: [6, 6, 12, 12], atk, mult, color: team === 1 ? ['#fffbe0', '#ffb020'] : ['#ffffff', '#ff7050'], radius: 5, name: 'kiblast',
    fx: { id: 'fx_0871370c', frame: (d, t) => d * 4 + kiFrame(t) },
    fxEnd: { rec: () => 16, ticks: 16 },
    rngSpawn: [2],
    rngHit: [2],
    sfxSpawn: 20,
    sfxHit: 48,
  })
  if (w) s.spawnRng(w)
  return s
}

/** ki blast muzzle (object position relative to the player position at spawn, per facing; re/buu/fx_objpos.py) */
export const KI_MUZZLE: [number, number][] = [[8, 0], [8, -7], [5, -7], [13, -7]]

/** Burning Attack (class 0x08029384, vtable 0x0804BFA0): created at the skill start in the hands (BURNING_HOLD,
 *  velocity 0, no hit test), launched from KI_MUZZLE by 0x080293E4. Graphics: run 0x087501EC, record (t >> 3) & 1
 *  in flight; on a wall / hit it stops and shows records 2 / 3 ((t >> 3) & 1) for 16 ticks. */
export const BURNING_HOLD: [number, number][] = [[8, -2], [12, -2], [19, -6], [-3, -6]]
export const burningFx = { id: 'fx_087501ec', frame: (_d: Dir, t: number) => (t >> 3) & 1 }
export const burningEnd = { rec: (_d: Dir, t: number) => 2 + ((t >> 3) & 1), ticks: 16 }

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
  /** the game's beam graphics (buu_beam_*); null = drawn procedurally */
  gfx: BeamGfx | null = null
  /** beam object position relative to the owner position per facing while firing (BEAM_POS[...].fire) */
  visPos: [number, number][] | null = null
  /** graphics drawn by another object (Flame Kamehameha links) */
  hidden = false
  /** obj+0x18: tick counter that selects the piece variants (phase = vt >> 2) */
  vt = 0
  /** obj+0x11: the head was stopped by an accepted hit or a wall -> impact piece */
  stopped = false
  /** retracting (state 2): position and length of the detached beam (moves 3 px/tick, len -3/tick) */
  private retract: { x: number; y: number; len: number } | null = null
  /** obj+0x14: tick of the next hit explosion */
  private nextFx = -1
  /** the game's length rule (emulator, the player's Kamehameha family): the object sits at the owner + visPos, its
   *  length (+0x38, glen) starts at 8 when fired and grows 3 px per frame while its end is within 8 px of the camera
   *  rect (or until a wall); the drawn beam and its retract (hence the end sound) follow it. The hits keep the fitted
   *  muzzle / speed model (`len`), which reproduces the measured first-hit frames and hit counts. */
  game = false
  glen = 8
  private gstop = false
  constructor(
    ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, muzzle: () => [number, number],
    o: { atk: AttackType; mult: number; width: number; speed: number; maxLen: number; color: [string, string]; hitsPerFrame?: (t: number) => number; gfx?: BeamGfx | null; visPos?: [number, number][] | null; game?: boolean; startLen?: number },
  ) {
    const [x, y] = muzzle()
    super(ctx, owner, dir, team, x << 8, y << 8)
    this.game = !!o.game && !!o.visPos
    if (o.startLen != null) this.len = o.startLen
    this.muzzle = muzzle
    this.atk = o.atk
    this.mult = o.mult
    this.width = o.width
    this.speed = o.speed
    this.maxLen = o.maxLen
    this.color = o.color
    this.hitsPerFrame = o.hitsPerFrame ?? ((t) => 1 + (t & 1))
    this.gfx = o.gfx ?? null
    this.visPos = o.visPos ?? null
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
  /** visual beam origin (the game's object position) and length */
  private visual(): [number, number, number] {
    if (this.retract) return [this.retract.x, this.retract.y, this.retract.len]
    const [dx, dy] = DIR_VEC[this.dir]
    const off = this.visPos?.[this.dir]
    const ox = off ? this.owner.px + off[0] : this.px
    const oy = off ? this.owner.py + off[1] : this.py
    if (this.game) return [ox, oy, this.glen]
    // the head stays where the hits are: muzzle + len along the facing
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    return [ox, oy, Math.max(0, Math.round((hx - ox) * dx + (hy - oy) * dy))]
  }
  update(w: World) {
    this.t++
    this.vt++
    if (this.done) {
      if (this.gfx) {
        // state 2 (0x08020864): the beam detaches, its position moves 3 px/tick along the facing and len -= 3;
        // removed once len < 0 (no hit test)
        if (!this.retract) {
          const [x, y, l] = this.visual()
          this.retract = { x, y, len: l }
        }
        const [dx, dy] = DIR_VEC[this.dir]
        this.retract.x += dx * 3
        this.retract.y += dy * 3
        this.retract.len -= 3
        if (this.retract.len < 0) this.removed = true
        return
      }
      this.len -= this.speed * 2
      if (this.len <= 0) this.removed = true
      return
    }
    if (this.game) {
      const off = this.visPos![this.dir]
      const ox = this.owner.px + off[0]
      const oy = this.owner.py + off[1]
      const [gx, gy] = DIR_VEC[this.dir]
      const c = w.camera
      const hx = ox + gx * this.glen
      const hy = oy + gy * this.glen
      const inside = gx > 0 ? hx < c.x1 + 8 : gx < 0 ? hx > c.x0 - 8 : gy > 0 ? hy < c.y1 + 8 : hy > c.y0 - 8
      if (inside && !this.gstop) {
        const wall = this.wallDist(w, ox, oy, 1, this.glen + 3)
        if (wall < this.glen + 3) this.gstop = true
        this.glen = Math.min(this.glen + 3, wall)
      }
    }
    const [mx, my] = this.muzzle()
    this.x = mx << 8
    this.y = my << 8
    const want = Math.min(this.maxLen, this.len + this.speed)
    // stop at the first projectile-wall tile ahead (INFERRED: the beam head uses the same grid as the shots)
    const wall = this.wallDist(w, mx, my, 1, Math.ceil(want))
    this.len = Math.min(want, wall)
    if (wall < want) this.stopped = true
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
    let hitAny = false
    for (let i = 0; i < n; i++) if (this.hit(w, cut(0, Math.min(L, NEAR)), this.atk, this.mult, false)) hitAny = true
    if (L > NEAR && this.t % 6 !== 0 && this.hit(w, cut(NEAR, L), this.atk, this.mult, false)) hitAny = true
    if (hitAny) this.hitFx(w)
  }
  /** accepted hit (0x08020996): the head shows the impact piece; every 3 + rand(5) ticks a sound (rand(2)) and an
   *  explosion 0x08712FDC at head + 4 along the facing + (rand(32) - 16, rand(32) - 16), rate 0x1000 + rand(0x4000) */
  private hitFx(w: World) {
    this.stopped = true
    if (this.nextFx < 0) this.nextFx = w.frame - 1
    if (w.frame - this.nextFx <= 0) return
    this.nextFx = w.frame + w.rng.rand(5, 'beam hit fx delay') + 3
    audioEvent('sfx', { id: 48 + w.rng.rand(2, 'beam hit sfx'), vol: 64 })
    const [ox, oy, l] = this.visual()
    const [dx, dy] = DIR_VEC[this.dir]
    const ex = ox + dx * (l + 4) + w.rng.rand(32, 'beam hit fx x') - 16
    const ey = oy + dy * (l + 4) + w.rng.rand(32, 'beam hit fx y') - 16
    this.ctx.explosion(w, ex, ey, 0x1000 + w.rng.rand(0x4000, 'beam hit fx rate'))
  }
  stop() {
    this.done = true
  }
  draw(r: Renderer) {
    if (this.hidden) return
    if (this.gfx) {
      const [ox, oy, l] = this.visual()
      this.gfx.draw(r, this.dir, ox, oy, l, this.vt, !this.done, this.stopped)
      return
    }
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
  /** Final Flash record table (FINAL_FLASH_TABLES); null = procedural */
  table: number | null
  /** graphics origin relative to the owner position per facing (the game's object position); null = this position */
  visPos: [number, number][] | null
  /** +0x18 scale: 0x20 at the first tick, +0x20 per tick to 0x100; after the hits end -0x20 per tick to 0 */
  scale = 0
  private ending = false
  private vis: [number, number] | null = null
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, o: { atk: AttackType; mult: number; life: number; width?: number; color?: [string, string]; table?: number; visPos?: [number, number][]; hits?: [number, number] }) {
    super(ctx, owner, dir, team, x << 8, y << 8)
    this.atk = o.atk
    this.mult = o.mult
    this.life = o.life
    this.width = o.width ?? 24
    this.color = o.color ?? ['#fffbd0', '#f0d020']
    this.table = o.table ?? null
    this.visPos = o.visPos ?? null
    this.hitWin = o.hits ?? null
    this.kind = 'fullbeam'
  }
  hitWin: [number, number] | null
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
    if (this.t === 1) {
      this.len = this.wallDist(w, this.px, this.py, 8, 400)
      const o = this.visPos?.[this.dir]
      this.vis = o ? [this.owner.px + o[0], this.owner.py + o[1]] : [this.px, this.py]
    }
    if (this.ending || this.t > this.life) {
      // the game shrinks the beam (state 2) by 0x20 per tick and removes it at 0 (no hits)
      this.ending = true
      this.scale -= 0x20
      if (!this.table || this.scale <= 0) this.removed = true
      return
    }
    this.scale = Math.min(0x100, this.scale + 0x20)
    // hit window [from, to] in ticks (the Final Flash: 8 hits from tick 6 - fitted to the measured runs; the object is
    // held while B is held, the hits are not) - default: every tick until the end
    if (!this.hitWin || (this.t >= this.hitWin[0] && this.t <= this.hitWin[1])) this.hit(w, this.rect(), this.atk, this.mult, false)
  }
  draw(r: Renderer) {
    if (this.table != null) {
      const [x, y] = this.vis ?? [this.px, this.py]
      const [dx, dy] = DIR_VEC[this.dir]
      // the visible length ends where the hit length ends
      const L = Math.max(0, (this.px + dx * this.len - x) * dx + (this.py + dy * this.len - y) * dy)
      drawFinalFlash(r, this.ctx.data.sprites, this.table, this.dir, x, y, L, this.scale)
      return
    }
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
  /** no graphics of its own: the owning objects draw the game's records */
  hidden = true
  draw() {}
}

// ---------------------------------------------------------------------------------------------------------------
/** Big Bang ball (0x08037480): grows while B is held; released after >= 60 frames it moves 0.58 px/frame and
 *  explodes on contact (17-24 hits measured). Multiplier from the charged EP (fitted: mult = EP*0.284, INFERRED).
 *  Graphics (update 0x08037650): ball definition 0x0876ACC0 + 16 * level = {lo, hi, record table, -}; record
 *  table + dir*120 + 12*f where f (obj+0x58) follows the tick counter u (obj+0x54): while charging u++, f =
 *  (u >> 3) + ((u >> 1) & 1) for u < 16, at u = 16 u jumps to 96 and f = (u >> 5) + ((u >> 3) & 1); once f >= lo it is
 *  lo (lo = hi) or lo + ((u >> 2) mod (hi - lo)) (Lv3 cycles 7..9, also in flight). On contact or a wall it explodes
 *  (0x0803756E): one area hit, then 60 ticks during which every 8th tick spawns a damaging explosion 0x0801952C at a
 *  random point of the box centre +- size*3/2 (sizes 0x0876AD04 by f). */
const BB_DEF: [number, number, number][] = [[4, 4, 0x08764e00], [6, 6, 0x08764e00], [7, 10, 0x0876aae0]]
const BB_SIZE = [4, 6, 8, 12, 16, 24, 28, 32, 32, 32]
/** record boxes (ox, oy, w, h) by f (identical for every direction) */
const BB_REC: [number, number, number, number][] = [[8, 4, 16, 16], [8, 2, 16, 16], [8, 0, 16, 16], [16, 4, 32, 32], [16, 0, 32, 32], [32, 8, 64, 64], [32, 4, 64, 64], [32, 0, 64, 64], [32, 0, 64, 64], [32, 0, 64, 64]]
/** ball position relative to the player position while charging, per facing (re/buu/fx_objpos.py) */
export const BIGBANG_POS: [number, number][] = [[2, 2], [12, -22], [-5, -6], [21, -6]]

export class BigBangBall extends Shot {
  charge256 = 0
  /** extra radius from holding past 60 frames (fitted to the hold-150 runs: +0 / +0.36 / +0.48 px per frame) */
  grow = 0
  launched = false
  boom = 0
  /** skill level 0..2 (ball definition) */
  lv: number
  /** obj+0x54 tick counter and obj+0x58 frame */
  u = 0
  f = 0
  /** graphics position - hit position (the hits use the fitted muzzle, the graphics the game's ball position) */
  visOff: [number, number] = [0, 0]
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, lv = 0) {
    super(ctx, owner, dir, 1, x, y)
    this.kind = 'bigbang'
    this.lv = Math.max(0, Math.min(2, lv))
  }
  get mult() {
    return Math.max(1, Math.floor((this.charge256 * 73) / 65536))
  }
  get radius() {
    return 9 + Math.min(44, Math.floor(this.grow))
  }
  /** hit / contact box: the fitted radius (the record boxes BB_REC do not reproduce the measured contact frames at
   *  100 px for Lv2 / Lv3) */
  rect() {
    const r = this.radius
    return centered(this.px, this.py, r * 2, r * 2)
  }
  private cap() {
    const [lo, hi] = BB_DEF[this.lv]
    if (this.f >= lo) this.f = hi === lo ? lo : lo + ((this.u >> 2) % (hi - lo))
  }
  /** centre of the current record box */
  center(): [number, number] {
    const [ox, oy, w, h] = BB_REC[this.f] ?? BB_REC[0]
    return [this.px - ox + (w >> 1), this.py - oy + (h >> 1)]
  }
  update(w: World) {
    this.t++
    if (!this.launched) {
      this.u++
      if (this.u < 16) this.f = (this.u >> 3) + ((this.u >> 1) & 1)
      else {
        if (this.u === 16) this.u = 96
        this.f = (this.u >> 5) + ((this.u >> 3) & 1)
      }
      this.cap()
      return
    }
    this.cap()
    this.u++
    const [dx, dy] = DIR_VEC[this.dir]
    this.x += dx * 148
    this.y += dy * 148
    if (this.offCamera(w)) {
      this.removed = true
      return
    }
    // probe: any enemy body overlapping -> explode; a projectile wall explodes it too (0x08037742)
    const probe = this.ctx.liveEnemies().some((e) => e.hittable && overlaps(e.bodyBox(), this.rect()))
    if (probe || this.inWall(w)) {
      this.removed = true
      const sz = (this.radius + 15) * 2
      w.add(new AreaHit(this.ctx, this.owner, this.dir, 1, centered(this.px, this.py, sz, sz), { life: 110, every: 5, atk: ATK[13], mult: this.mult, color: ['#e0f8ff', '#40a0ff'] }))
      // 0x0803756E: the explosions spread over the record box centre +- size * 3/2 (sizes by frame)
      const [cx, cy] = this.center()
      const s = (BB_SIZE[this.f] * 3) >> 1
      w.add(new BigBangBoom(this.ctx, cx + this.visOff[0] - s, cy + this.visOff[1] - s, s))
    }
  }
  draw(r: Renderer) {
    const [, , table] = BB_DEF[this.lv]
    drawRecAddr(r, this.ctx.data.sprites, table + this.dir * 120 + 12 * this.f, this.px + this.visOff[0], this.py + this.visOff[1])
  }
}

/** Big Bang explosion phase (state 2 of 0x08037650): 60 ticks; when the countdown is a multiple of 8 (56 .. 8) an
 *  explosion 0x08712FDC (0x0801952C: rate 0x1000 + rand(0x1000), prio 0x38) appears at (x0 + rand(2s), y0 + rand(2s)).
 *  Visual only here (the sim's damage is the AreaHit above). */
export class BigBangBoom implements Entity {
  id = 0
  kind = 'bigbangboom'
  removed = false
  layer = 1
  n = 60
  ctx: BuuCtx
  x0: number
  y0: number
  s: number
  constructor(ctx: BuuCtx, x0: number, y0: number, s: number) {
    this.ctx = ctx
    this.x0 = x0
    this.y0 = y0
    this.s = s
  }
  depth() {
    return this.y0 + this.s
  }
  update(w: World) {
    if (--this.n <= 0) {
      this.removed = true
      return
    }
    if (this.n & 7) return
    const x = this.x0 + w.rng.rand(this.s * 2, 'big bang fx x')
    const y = this.y0 + w.rng.rand(this.s * 2, 'big bang fx y')
    this.ctx.explosion(w, x, y, 0x1000 + w.rng.rand(0x1000, 'big bang fx rate'))
  }
  draw() {}
}

// ---------------------------------------------------------------------------------------------------------------
/** Masenko (skill 17). The game: a cursor object (ctor 0x08021594, record 0x0872509C) at the body-box centre +
 *  dir * dist and a ball object (class 0x08021280, shared with Super Buu's lobbed ball) charging at MASENKO_MUZZLE;
 *  on release the ball is launched (0x08021452) on a 43-tick arc (vy starts -0x200 and gains 0x20 per tick) and
 *  blasts once (atk sub 7). The game's cursor (Lv1/Lv2: +-2 px/tick bouncing between 16 and 70 / 100; Lv3 d-pad)
 *  does not reproduce the measured hits (only at 40 px, never at 100), so the sim keeps the fitted cursor (24 px,
 *  +0.5 px/frame while held, max 48) and the measured release -> hit delay (51 frames, lag included); the ball's
 *  visible arc (the game's shape) ends on the cursor over the last 43 ticks. */
export const MASENKO_MUZZLE: [number, number][] = [[8, -24], [8, -24], [10, -24], [6, -24]]
/** level definitions 0x0872A690 (12 bytes): sparkle record (fx_08721164 index), blast table, blast records, half size */
const MASENKO_LV = [
  { spark: 9, blast: 0x0872a5dc, n: 4, half: 8 },
  { spark: 9, blast: 0x0872a60c, n: 5, half: 16 },
  { spark: 12, blast: 0x0872a648, n: 6, half: 32 },
]
const MASENKO_DELAY = 51
const MASENKO_FLIGHT = 43

export class MasenkoCursor extends Shot {
  dist = 24
  released = -1
  mult: number
  lv: number
  ball: MasenkoBall | null = null
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, mult: number, lv = 0) {
    super(ctx, owner, dir, 1, 0, 0)
    this.mult = mult
    this.lv = Math.max(0, Math.min(2, lv))
    this.kind = 'masenko'
  }
  pos(): [number, number] {
    const [cx, cy] = this.owner.center()
    const [dx, dy] = DIR_VEC[this.dir]
    return [cx + dx * this.dist, cy + dy * this.dist]
  }
  rect() {
    const [x, y] = this.pos()
    return centered(x, y, 16, 16)
  }
  update(w: World) {
    this.t++
    if (!this.ball) {
      const [mx, my] = MASENKO_MUZZLE[this.dir]
      this.ball = w.add(new MasenkoBall(this.ctx, this.owner, this.dir, this, (this.owner.px + mx) << 8, (this.owner.py + my) << 8))
    }
    if (this.released < 0) {
      this.dist = Math.min(48, this.dist + 0.5)
      const [x, y] = this.pos()
      this.x = x << 8
      this.y = y << 8
      return
    }
    const since = this.t - this.released
    if (since === MASENKO_DELAY - MASENKO_FLIGHT) this.ball?.launch(...this.pos())
    if (since >= MASENKO_DELAY) {
      this.removed = true
      const [x, y] = this.pos()
      this.ball?.blast(x, y)
      w.add(new AreaHit(this.ctx, this.owner, this.dir, 1, centered(x, y, 32, 32), { life: 16, every: 100, atk: ATK[7], mult: this.mult, color: ['#fff8d0', '#ffe040'] }))
    }
  }
  release() {
    if (this.released < 0) this.released = this.t
  }
  draw(r: Renderer) {
    if (this.released >= 0) return
    const [x, y] = this.pos()
    drawRecAddr(r, this.ctx.data.sprites, 0x0872509c, x, y)
  }
}

/** Masenko ball (class 0x08021280): charge sparkles fx_08721164 spark + k (k = (t >> 3) & 3, 3 -> 1), flight
 *  spark + 1 + ((t >> 3) & 1) on the game's arc (vy starts -0x200 and gains 0x20 per tick, 43 ticks; here from the
 *  muzzle to the target), then the blast records blast + (t >> 2), 4 ticks each. Visual only (hit: AreaHit). */
export class MasenkoBall extends Shot {
  cursor: MasenkoCursor
  phase: 'charge' | 'fly' | 'blast' = 'charge'
  /** flight: start, target (24.8) and tick */
  from: [number, number] = [0, 0]
  to: [number, number] = [0, 0]
  ft = 0
  tb = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, cursor: MasenkoCursor, x: number, y: number) {
    super(ctx, owner, dir, 1, x, y)
    this.cursor = cursor
    this.kind = 'masenkoball'
  }
  rect() {
    return centered(this.px, this.py, 8, 8)
  }
  launch(tx: number, ty: number) {
    if (this.dir === 1) this.y -= 8 << 8
    this.from = [this.x, this.y]
    this.to = [tx << 8, ty << 8]
    this.ft = 0
    this.phase = 'fly'
  }
  blast(x: number, y: number) {
    this.x = x << 8
    this.y = y << 8
    this.phase = 'blast'
    this.tb = 0
  }
  update() {
    this.t++
    if (this.phase === 'charge') {
      if (this.cursor.removed) this.removed = true
      return
    }
    if (this.phase === 'fly') {
      // linear from -> to plus the game's vertical arc minus its straight part (sum of vy, vy0 = -0x200, +0x20/tick)
      const n = MASENKO_FLIGHT
      const k = Math.min(n, ++this.ft)
      const arc = (i: number) => -0x200 * i + (0x20 * i * (i + 1)) / 2
      const bend = arc(k) - (arc(n) * k) / n
      this.x = Math.round(this.from[0] + ((this.to[0] - this.from[0]) * k) / n)
      this.y = Math.round(this.from[1] + ((this.to[1] - this.from[1]) * k) / n + bend)
      return
    }
    this.tb++
    if (this.tb >> 2 >= MASENKO_LV[this.cursor.lv].n) this.removed = true
  }
  draw(r: Renderer) {
    const L = MASENKO_LV[this.cursor.lv]
    const spark = this.ctx.fx('fx_08721164')
    if (this.phase === 'charge') {
      let k = (this.t >> 3) & 3
      if (k === 3) k = 1
      drawRec(r, spark, L.spark + k, this.px, this.py)
    } else if (this.phase === 'fly') drawRec(r, spark, L.spark + 1 + ((this.t >> 3) & 1), this.px, this.py)
    else drawRecAddr(r, this.ctx.data.sprites, L.blast + 12 * Math.min(L.n - 1, this.tb >> 2), this.px, this.py)
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Kamikaze ghost (skill 13): homes in on the closest enemy, explodes on contact. No hit was recorded in the
 *  emulator, so the blast (energy x2.0) is INFERRED. */
export class Ghost extends Shot {
  vx = 0
  vy = 0
  free = false
  /** frames between the release and the ghost's AI acting (its script starts on the next frame: emulator, the
   *  explosion's rand(4096) lands 2 frames after the release) */
  wait = 0
  /** formation slot (0..2, table 0x0871E024) */
  slot: number
  /** animation clock: raw animation 19 (appearing) then 1 */
  at = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, slot = 0) {
    super(ctx, owner, dir, 1, x, y)
    this.kind = 'ghost'
    this.slot = slot
  }
  rect() {
    return centered(this.px, this.py, 12, 12)
  }
  update(w: World) {
    this.t++
    this.at++
    if (!this.free) {
      // formation (0x0804DC80): pos += (target - pos) / 32 toward the player + slot offset
      const o = GHOST_SLOTS[this.owner.facing][Math.min(2, this.slot)]
      const tx = (this.owner.px + o[0]) << 8
      const ty = (this.owner.py + o[1]) << 8
      this.x += (tx - this.x) >> 5
      this.y += (ty - this.y) >> 5
      return
    }
    if (this.wait > 0) {
      this.wait--
      return
    }
    const targets = this.ctx.liveEnemies().filter((e) => e.hittable)
    if (!targets.length || this.t > 400) {
      this.removed = true
      this.ctx.explosion(w, this.px, this.py, 0x1000 + w.rng.rand(0x1000, 'ghost explosion rate'))
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
    this.vx = (this.vx * 7 + (dx / m) * 768) / 8
    this.vy = (this.vy * 7 + (dy / m) * 768) / 8
    this.x += Math.round(this.vx)
    this.y += Math.round(this.vy)
    if (overlaps(best.bodyBox(), this.rect())) {
      this.removed = true
      // the ghost's AI explodes it (0x0801952C, prio 0x38) at its box centre
      this.ctx.explosion(w, this.px, this.py, 0x1000 + w.rng.rand(0x1000, 'ghost explosion rate'))
      this.hit(w, centered(this.px, this.py, 32, 32), ATK[1], 2048, false)
    }
  }
  draw(r: Renderer) {
    // enemy type 0x54 actor, sprite 148: raw animation 19 while appearing, then 1
    const s = this.ctx.data.sprites.index(148)
    if (!s) return
    const appear = s.anim(19)
    const idle = s.anim(1)
    const al = appear ? appear.length(this.owner.facing) : 0
    if (appear && this.at < al) s.draw(r.ctx, appear, this.owner.facing, r.sx(this.px), r.sy(this.py), this.at, { loop: false })
    else s.draw(r.ctx, idle, this.owner.facing, r.sx(this.px), r.sy(this.py), this.at - al)
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
      this.ctx.puffRng(w, hx, hy)
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
      this.ctx.puffRng(w, hx, hy)
    }
    return false
  }
  /** 0x0802A2EC: the muzzle piece (record (t >> 2) & 1) at the origin while growing (len < 48), the body record 2
   *  every 8 px from the position to len - 1; pieces at position + origin */
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const base = 0x0875043c + 36 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    if (this.mode === 0 && this.len < 48) drawRecAdd(r, bank, base + 12 * ((this.t >> 2) & 1), this.px, this.py)
    for (let k = 0; k < this.len - 1; k += 8) drawRecAdd(r, bank, base + 24, this.px + dx * k, this.py + dy * k)
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
    const f = this.t >> 2
    if (f <= 4) drawRecAddr(r, this.ctx.data.sprites, 0x0872d6b8 + 60 * this.dir + 12 * f, this.px, this.py)
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
      // 0x0801C750: 4 debris pieces + an explosion 0x0801952C (rate 0x1000 + rand(0x1000)) at the box centre
      for (let i = 0; i < 4; i++) w.add(new Debris(this.ctx.fx('buu_debris'), this.px, this.py, i))
      this.ctx.explosion(w, this.px, this.py, 0x1000 + w.rng.rand(0x1000, 'bomb explosion rate'))
      this.hit(w, this.rect(), ATK[1], 1024)
    }
  }
  draw(r: Renderer) {
    let k = (this.t >> 2) & 3
    if (k === 3) k = 1
    drawRecAddr(r, this.ctx.data.sprites, 0x0871e984 + 12 * k, this.px, this.py + Math.round(this.z))
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
    const bank = this.ctx.data.sprites
    const base = 0x0874efec + 24 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    const x = this.owner.px + this.ox
    const y = this.owner.py + this.oy
    const L = this.length()
    for (let k = 0; k < L - 1; k += 8) drawRecAdd(r, bank, base, x + dx * k, y + dy * k)
    drawRecAdd(r, bank, base + 12, x + dx * L, y + dy * L)
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
        // the player's sprite becomes def 45 (three sweets) with frame rand(3); timer 0x03002FE4 = 301
        this.ctx.candyFrame = w.rng.rand(3, 'candy sweet')
        this.ctx.candy = 301
        w.logEvent('info', 'player turned into candy (300 f)')
      }
    }
  }
  /** muzzle record k (k = (t >> 2) & 1) at the origin while len < 64, body record 2 every 8 px, head record 3 + k at
   *  dir * len; pieces at position + origin */
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const base = 0x0874ed7c + 60 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    const x = this.owner.px + this.ox
    const y = this.owner.py + this.oy
    const head = Math.min(40, 8 + 4 * this.t)
    const tail = Math.max(0, (this.t - 9) * 4)
    const k = (this.t >> 2) & 1
    if (tail === 0) drawRecAdd(r, bank, base + 12 * k, x, y)
    for (let o = tail; o < head - 1; o += 8) drawRecAdd(r, bank, base + 24, x + dx * o, y + dy * o)
    drawRecAdd(r, bank, base + 12 * (3 + k), x + dx * head, y + dy * head)
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
        // blast records 0x0872A648 (fx_0872a5dc 9..14), 4 ticks each, no explosion object
        w.add(new FxAnim(this.ctx.fx('fx_0872a5dc'), this.px, this.py, 9, 6, 0x4000))
        w.add(new AreaHit(this.ctx, this.owner, 0, 0, centered(this.px, this.py, 32, 32), { life: 16, every: 100, atk: ATK[7], mult: 16589, grow: 2, color: ['#ffffff', '#ff60a0'] }))
      }
    }
  }
  draw(r: Renderer) {
    const spark = this.ctx.fx('fx_08721164')
    let k = (this.t >> 3) & 3
    if (k === 3) k = 1
    drawRec(r, spark, this.phase === 0 ? 12 + k : 13 + ((this.t >> 3) & 1), this.px, this.py + Math.round(this.z))
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
    // the box table 0x0875A6A8 doubles as the wave's OBJ records (64x15 vertical, 15x64 horizontal)
    drawRecAddr(r, this.ctx.data.sprites, 0x0875a6a8 + 12 * this.dir, this.px, this.py)
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
    this.layer = 2
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
      for (let i = 0; i < 4; i++) this.ctx.puffRng(w, this.px + (i & 1 ? 50 : -50), this.py + (i & 2 ? 50 : -50))
    }
    return false
  }
  /** BG0 (priority 1, above the sprites) filled disc of colour 0x7FFF; BLDCNT 0x1E41 alpha blend over BG1-3 + OBJ:
   *  BLDALPHA 8/8 while growing; fading: EVA = (((128 - r) * 50) >> 6) * 656 >> 12, EVB = 16 */
  draw(r: Renderer) {
    const g = r.ctx
    const fading = this.ctx.sphereState === 2
    const eva = fading ? (((((128 - this.rad) * 50) >> 6) * 656) >> 12) / 16 : 0.5
    if (eva <= 0) return
    g.save()
    g.globalAlpha = Math.min(1, eva)
    if (fading) g.globalCompositeOperation = 'lighter'
    g.fillStyle = '#ffffff'
    g.beginPath()
    g.arc(r.sx(this.px), r.sy(this.py), this.rad, 0, Math.PI * 2)
    g.fill()
    g.restore()
  }
}

/** Bio Mech grapple arm (0x0804B380): extends like a wave to 80 px, melee sub 0 x1.0 (STR); a hit holds the player
 *  and reels him in (INFERRED) */
export class GrappleArm extends Shot {
  len = 8
  back = false
  grabbed = false
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
      this.grabbed = true
      const p = this.ctx.player
      if (p && !p.dead) p.hold(w, this.owner, 'grapple', 20)
    }
  }
  /** chain record 0 every 8 px to len - 1, the head at dir * len: open hand record 1, or while grabbing the closed
   *  hand records 2 (back) + 3 (front); pieces at position + origin */
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const base = 0x0876dff8 + 48 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    for (let k = 0; k < this.len - 1; k += 8) drawRecAdd(r, bank, base, this.px + dx * k, this.py + dy * k)
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    if (this.grabbed) {
      drawRecAdd(r, bank, base + 24, hx, hy)
      drawRecAdd(r, bank, base + 36, hx, hy)
    } else drawRecAdd(r, bank, base + 12, hx, hy)
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
    // class 0x08036A44, records 0x0875F7AC: 0 intact, 1 cracked (2 broken, 3-5 regrowing), origin (0, 0)
    if (this.fx) drawRec(r, this.fx, this.hp >= 2 ? 0 : 1, this.x, this.y)
  }
}
