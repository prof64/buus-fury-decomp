// Buu's Fury projectiles and attack objects (player skills + enemy classes of bosses.json `projectiles`).
// Speeds, boxes, attack types, multipliers and lifetimes are the data's (notes §5, §8b); what is not decoded is
// marked INFERRED. Drawing is ours: most WF projectile graphics are code-driven effect runs that are not split
// per projectile in sprites.json, so shots are drawn procedurally in the game's colours unless a matching
// effect sheet is known (box-table addresses double as graphics tables).

import { DIR_VEC, overlaps, type Dir, type Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import { BTN } from '../../core/input'
import type { Entity, World } from '../../core/world'
import { broadcast, frameBoxOf, type Actor, type HitMsg, type Receiver } from './actor'
import { EnemyAnim } from './eanim'
import type { SpriteSet } from '../../core/sprites'
import type { BuuCtx } from './context'
import { ATK, type AttackType } from './damage'
import { audioEvent } from '../../audio/bus'
import { Debris, type BeamGfx, drawRec, drawRecAddr, FxAnim, recGeomAt } from './fx'
import { drawFinalFlash, drawRecAdd, drawVolleyPieces, GHOST_SLOTS } from './skillfx'

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
  /** the box follows the blast's record (kiBlastBox) */
  recBox?: boolean
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
  /** the sound of the shot's destroy function (wall, accepted hit, shot down), before its explosion (rifle bullet
   *  0x0802770C: 46) */
  sfxEnd?: number
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
    const [ox, oy, w, h] = this.o.recBox ? kiBlastBox(this.dir, this.t) : this.o.box
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
    if (fx && this.o.sfxEnd != null) audioEvent('sfx', { id: this.o.sfxEnd, vol: 64 })
    if (fx && this.o.explode !== false) this.ctx.explosion(w, this.px, this.py)
    this.dead = 0
    this.removed = true
  }
  onMessage(_w: World, msg: HitMsg): boolean {
    // the tank shell's handler 0x0801CB38: any message of kind 0 but its own overlapping its box destroys it
    // (0x0801C978: explosion, rate 0x2000) and is accepted (Earth segment 3-14: a ki blast shoots a shell down)
    if (this.o.name === 'shell') {
      if (this.dead < 0 && !msg.foot && msg.source !== this && !this.removed && overlaps(msg.rect, this.rect())) {
        this.removed = true
        this.ctx.explosion(_w, this.px, this.py)
        return true
      }
      return false
    }
    if (this.o.fragile && this.dead < 0 && msg.team === 1 && msg.atk.kind === 0 && msg.source !== this && overlaps(msg.rect, this.rect())) {
      this.removed = true
      if (this.o.sfxEnd != null) audioEvent('sfx', { id: this.o.sfxEnd, vol: 64 })
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

// ---- mummy wraps --------------------------------------------------------------------------------------------------
/** bandage velocities (24.8 per frame) by facing, straight and diagonal (0x08756B74: 16 bytes per facing) */
const WRAP_VEL: [number, number, number, number][] = [[0, 448, -316, 316], [0, -448, 316, -316], [-448, 0, -316, -316], [448, 0, 316, 316]]
/** a stuck bandage's offset above the player's position by slot (0x08756BB4) */
const WRAP_SLOT_Y = [-2, -10, 2, 18, 6, 10, -6, 14]
/** the bandage (builder 0x0802D79C, vtable 0x0804A830, update 0x0802D940, messages 0x0802DB10). The box is the
 *  record's (0x08019B4E: records 0x087571F4 + 36 * (dir >> 1), diagonal + 0x48, frame t >> 3, t = 0..23).
 *  Every update first reapplies the player's stats (0x0802D8E6: when the player is hittable, the form controller's
 *  stat reset - resists back to 128 - then walk / dash speed -48 per stuck bandage, counted by message 0x25).
 *  Flying: pos += velocity; a 4-frame stick delay counting down (0 -> stick 0x0802D826); off camera or in a
 *  projectile wall -> removed; else a hit message (attack 0x086B8090 energy sub 1, x1.0, the bandage's facing) to the
 *  actors: accepted -> if its box overlaps the player's body box: sound 48 + rand(2) and, with < 8 bandages stuck, the
 *  4-frame delay, else it bursts (0x0802D89A: removed + explosion 0x08712FDC rate 0x2000 when the pool has room).
 *  Stuck: follows the player at (+4, -slotY) (slot = global 0x030030BC, +1 & 7 per stick); 0x08040284(input, d-pad,
 *  8, 60) (the last 8 d-pad presses within 60 frames, one now) -> removed. A flying bandage is destroyed by any other
 *  object's hit message overlapping its box (0x0802DB10 message 0). */
export class MummyWrap extends Shot {
  vx: number
  vy: number
  diag: boolean
  tick = 0
  stuck = false
  delay = 0
  offX = 0
  offY = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, diag: boolean) {
    super(ctx, owner, dir, 0, x, y)
    this.diag = diag
    const v = WRAP_VEL[dir]
    this.vx = diag ? v[2] : v[0]
    this.vy = diag ? v[3] : v[1]
    this.kind = 'wrap'
  }
  /** the record's box: straight vertical (dir 0 / 1) 16 x 8, horizontal 8 x 16, diagonal 16 x 16; stuck: 8 x 16 */
  rect(): Rect {
    const [ox, oy, w, h] = this.diag ? [8, 8, 16, 16] : this.stuck || this.dir >= 2 ? [4, 8, 8, 16] : [8, 4, 16, 8]
    return { x0: this.px - ox, y0: this.py - oy, x1: this.px - ox + w, y1: this.py - oy + h }
  }
  private stuckCount(w: World): number {
    return w.entities.filter((e) => e instanceof MummyWrap && !e.removed && e.stuck).length
  }
  /** 0x0802D8E6 */
  private reapply(w: World) {
    const p = this.ctx.player
    if (!p || !p.hittable) return
    p.wrapStats(this.stuckCount(w))
  }
  /** 0x0802D89A */
  private burst(w: World) {
    this.removed = true
    this.ctx.explosion(w, this.px, this.py, 0x2000)
  }
  update(w: World) {
    this.reapply(w)
    const p = this.ctx.player
    if (this.stuck) {
      if (p) {
        this.x = p.x - this.offX
        this.y = p.y - this.offY
      }
      if (this.ctx.pressRun(0xf0, 8, 60, w)) {
        this.removed = true
        this.reapply(w)
        return
      }
    } else {
      this.x += this.vx
      this.y += this.vy
      if (++this.tick === 24) this.tick = 0
    }
    if (this.delay) {
      if (--this.delay === 0) this.stick()
    }
    if (this.stuck || this.delay > 0) return
    if (this.offCamera(w) || this.inWall(w)) {
      this.removed = true
      return
    }
    if (!this.hit(w, this.rect(), ATK[1], 1024)) return
    if (p && overlaps(this.rect(), p.bodyBox())) {
      audioEvent('sfx', { id: 48 + w.rng.rand(2, 'wrap hit sfx'), vol: 64 })
      if (this.stuckCount(w) < 8) this.delay = 4
      else this.burst(w)
    } else this.burst(w)
  }
  /** 0x0802D826: at the player's position + (4, -slotY[slot]), slot = 0x030030BC (then + 1 & 7) */
  private stick() {
    const p = this.ctx.player
    if (!p) return
    const slot = this.ctx.wrapSlot
    this.ctx.wrapSlot = (slot + 1) & 7
    this.x = p.x + 0x400
    this.y = p.y - (WRAP_SLOT_Y[slot] << 8)
    this.stuck = true
    this.offX = p.x - this.x
    this.offY = p.y - this.y
  }
  onMessage(w: World, msg: HitMsg): boolean {
    if (msg.source === this || this.stuck || this.removed || !overlaps(msg.rect, this.rect())) return false
    this.burst(w)
    return true
  }
  draw(r: Renderer) {
    // stuck bandages are drawn by the player (player.ts, the wraps count)
    if (this.stuck) return
    const fx = this.ctx.fx('fx_087571f4')
    if (fx) drawRec(r, fx, (this.diag ? 6 : 0) + 3 * (this.dir >> 1) + (this.tick >> 3), this.px, this.py)
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
/** the blast's box (vtable 0x0804B454: the record at obj+0x18, 0x0871370C + 12 * (facing * 4 + frame), frame t >> 3 =
 *  0, 1, 2, 3, then 2, 3): 16 x 16 for frames 0-1, then 8 x 16 (down / up) or 16 x 8 (left / right) (Earth segments
 *  2-9 / 2-10: a blast passing a criminal's edge misses) */
export function kiBlastBox(dir: number, t: number): [number, number, number, number] {
  if (t >> 3 < 2) return [8, 8, 16, 16]
  return dir < 2 ? [4, 8, 8, 16] : [8, 4, 16, 8]
}

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
/** the player's beam (class 0x080206B8, vtable 0x0804AEF8, update 0x0802084C, stop 0x0802082A, destructor 0x08020768):
 *  Kamehameha 15 / Kamekameha 16 / Electric Kamehameha 6 / Banshee 1 / Big Bang Kamehameha 3. The object sits at the
 *  fire position (player + BEAM_POS[..].fire), length 8 from the construction. Each firing tick: head = position +
 *  dir * len, head box = head +- 8; a projectile wall under the head box -> stopped, no head message; else the head
 *  message (every object, the level's mult / attack sub, dir = facing): accepted -> the hit effect (below) and the
 *  head stays (stopped); not accepted -> len += 3 if the head box strictly overlaps the camera rect. Then always the
 *  body message: the box from the position to the head expanded by 6 (result ignored), so a target touching both
 *  takes two hits per tick. Released (state 2): the position moves 3 px per tick along the facing and len -= 3, no
 *  hits; removed once len < 0 (destructor: stop 32 + sound 33). Hit effect (head hits only): when now - next > 0,
 *  next = now + 3 + rand(5), sound 48 + rand(2) and an explosion at head + 4 * dir + (rand(32) - 16, rand(32) - 16),
 *  rate 0x1000 + rand(0x4000); `next` starts at the construction (the skill press). Verified frame by frame against the
 *  emulator (tools/battle/LOCKSTEP.md, *_vs_skill15 / 16 / 6 / 1 / 3). */
export class KameBeam extends Shot {
  len = 8
  done = false
  atk: AttackType
  mult: number
  color: [string, string]
  gfx: BeamGfx | null
  /** obj+0x18: tick counter that selects the piece variants (phase = vt >> 2) */
  vt = 0
  /** obj+0x11: the head was stopped by an accepted hit or a wall -> impact piece */
  stopped = false
  /** obj+0x14: frame of the next hit effect */
  next: number
  /** charging (state 0, before fire()): no update; `sounds`: the object plays its own fire / end sounds (enemy use) */
  charging: boolean
  sounds: boolean
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, built: number, o: { atk: AttackType; mult: number; color: [string, string]; gfx: BeamGfx | null; team?: Team; sounds?: boolean }) {
    super(ctx, owner, dir, o.team ?? 1, x << 8, y << 8)
    this.atk = o.atk
    this.mult = o.mult
    this.color = o.color
    this.gfx = o.gfx
    this.next = built
    this.sounds = !!o.sounds
    this.charging = this.sounds
    this.kind = 'beam'
  }
  /** 0x080207D2: firing (stop 3, sound 32) */
  fire() {
    if (!this.charging) return
    this.charging = false
    audioEvent('sfx_stop', { id: 3 })
    audioEvent('sfx', { id: 32, vol: 64 })
  }
  head(): [number, number] {
    const [dx, dy] = DIR_VEC[this.dir]
    return [this.px + dx * this.len, this.py + dy * this.len]
  }
  headRect(): Rect {
    const [hx, hy] = this.head()
    return { x0: hx - 8, y0: hy - 8, x1: hx + 8, y1: hy + 8 }
  }
  /** the body box: position .. head, expanded by 6 */
  rect(): Rect {
    const [hx, hy] = this.head()
    return { x0: Math.min(this.px, hx) - 6, y0: Math.min(this.py, hy) - 6, x1: Math.max(this.px, hx) + 6, y1: Math.max(this.py, hy) + 6 }
  }
  update(w: World) {
    this.t++
    this.vt++
    const [dx, dy] = DIR_VEC[this.dir]
    if (this.done) {
      this.x += dx * 3 * 256
      this.y += dy * 3 * 256
      this.len -= 3
      if (this.len < 0) {
        this.removed = true
        // destructor 0x08020768: stop 32, sound 33
        if (this.sounds) {
          audioEvent('sfx_stop', { id: 32 })
          audioEvent('sfx', { id: 33, vol: 64 })
        }
      }
      return
    }
    if (this.charging) return
    const hr = this.headRect()
    // the body box uses the head of this tick before any growth
    const body = this.rect()
    if (this.inWall(w, hr)) this.stopped = true
    else if (this.hit(w, hr, this.atk, this.mult, false)) {
      this.stopped = true
      this.hitFx(w)
    } else if (overlaps(hr, w.camera)) this.len += 3
    this.hit(w, body, this.atk, this.mult, false)
  }
  private hitFx(w: World) {
    if (w.frame - this.next <= 0) return
    this.next = w.frame + 3 + w.rng.rand(5, 'beam hit fx delay')
    audioEvent('sfx', { id: 48 + w.rng.rand(2, 'beam hit sfx'), vol: 64 })
    const [hx, hy] = this.head()
    const [dx, dy] = DIR_VEC[this.dir]
    const ex = hx + dx * 4 + w.rng.rand(32, 'beam hit fx x') - 16
    const ey = hy + dy * 4 + w.rng.rand(32, 'beam hit fx y') - 16
    this.ctx.explosion(w, ex, ey, 0x1000 + w.rng.rand(0x4000, 'beam hit fx rate'))
  }
  /** 0x0802082A: still charging -> removed (destructor: stop 3); firing -> retracts */
  stop() {
    if (this.charging) {
      this.removed = true
      if (this.sounds) audioEvent('sfx_stop', { id: 3 })
      return
    }
    this.done = true
  }
  draw(r: Renderer) {
    if (this.gfx) this.gfx.draw(r, this.dir, this.px, this.py, this.len, this.vt, !this.done, this.stopped)
  }
}

// ---------------------------------------------------------------------------------------------------------------
/** Ultra Volleyball Attack wave (Gotenks skill 23, update 0x080384C2): origin at the player + VOLLEY_POS (the spawn
 *  tick), length 8. Each tick (state 0): box = origin .. origin + dir * len, expanded by 2; a projectile wall under it
 *  -> state 1, else one message to every object (x1.0, attack sub 16 = power 0, dir = facing): accepted -> state 1;
 *  not accepted -> len += 4 up to 48, then the origin moves 4 px per tick; after 240 ticks -> state 1. State 1: the
 *  origin moves 4 px and len -= 4 per tick, removed at len <= 0. Lockstep f31_vs_skill23. */
export class VolleyWave extends Shot {
  len = 8
  done = false
  n = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 1, x << 8, y << 8)
    this.kind = 'volley'
  }
  rect(): Rect {
    const [dx, dy] = DIR_VEC[this.dir]
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    return { x0: Math.min(this.px, hx) - 2, y0: Math.min(this.py, hy) - 2, x1: Math.max(this.px, hx) + 2, y1: Math.max(this.py, hy) + 2 }
  }
  update(w: World) {
    this.t++
    const [dx, dy] = DIR_VEC[this.dir]
    if (this.done) {
      this.x += dx * 4 * 256
      this.y += dy * 4 * 256
      this.len -= 4
      if (this.len <= 0) this.removed = true
      return
    }
    const r = this.rect()
    if (this.inWall(w, r) || this.hit(w, r, ATK[16], 0x400, false)) {
      this.done = true
      return
    }
    if (this.len < 0x30) this.len += 4
    else {
      this.x += dx * 4 * 256
      this.y += dy * 4 * 256
    }
    if (++this.n > 0xf0) this.done = true
  }
  draw(r: Renderer) {
    drawVolleyPieces(r, this.ctx.data.sprites, this.dir, this.px, this.py, this.len)
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
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, team: Team, x: number, y: number, o: { atk: AttackType; mult: number; life: number; width?: number; color?: [string, string]; table?: number; visPos?: [number, number][]; hits?: [number, number]; game?: { built: number } }) {
    super(ctx, owner, dir, team, x << 8, y << 8)
    if (o.game) {
      this.game = true
      this.next = o.game.built
      this.scale = 0x20
    }
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
  /** the player's Final Flash (the game's update 0x0803710C): length from the constructor's wall probe (square
   *  position +- 12, len 8, + 8 per free square), scale +0x20 per tick to 0x100, half width 8 * scale >> 8, one message
   *  per tick to every object; an accepted one when now - next > 0: next = now + 3 + rand(5), sound 48 + rand(2).
   *  Released: no hits, scale -0x20 per tick, removed after 8 ticks (destructor: stop 32) */
  game = false
  next = 0
  private endT = 0
  private gameUpdate(w: World) {
    if (this.t === 1) {
      const [dx, dy] = DIR_VEC[this.dir]
      let L = 8
      for (let k = 0; k < 256; k++) {
        const cx = this.px + dx * 8 * k
        const cy = this.py + dy * 8 * k
        if (this.inWall(w, { x0: cx - 12, y0: cy - 12, x1: cx + 12, y1: cy + 12 })) break
        L += 8
      }
      this.len = L
      const o = this.visPos?.[this.dir]
      this.vis = o ? [this.owner.px + o[0], this.owner.py + o[1]] : [this.px, this.py]
    }
    if (this.ending || this.t > this.life) {
      this.ending = true
      this.scale = Math.max(0, this.scale - 0x20)
      if (++this.endT >= 8) this.removed = true
      return
    }
    if (this.scale < 0x100) this.scale += 0x20
    const hw = (8 * this.scale) >> 8
    const [dx, dy] = DIR_VEC[this.dir]
    const ex = this.px + dx * this.len
    const ey = this.py + dy * this.len
    const r = { x0: Math.min(this.px, ex) - hw, y0: Math.min(this.py, ey) - hw, x1: Math.max(this.px, ex) + hw, y1: Math.max(this.py, ey) + hw }
    if (this.hit(w, r, this.atk, this.mult, false) && w.frame - this.next > 0) {
      this.next = w.frame + 3 + w.rng.rand(5, 'final flash hit delay')
      audioEvent('sfx', { id: 48 + w.rng.rand(2, 'final flash hit sfx'), vol: 64 })
    }
  }
  update(w: World) {
    this.t++
    if (this.game) return this.gameUpdate(w)
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
/** Big Bang ball (class 0x08037480, vtable 0x0804A26C, update 0x08037650, launch 0x080378A0, explode 0x0803756E,
 *  destructor 0x0803750E, handler 0x080378F4). Charging at the player + BIGBANG_POS; graphics: ball definition
 *  0x0876ACC0 + 16 * level = {lo, hi, record table, -}; record table + dir*120 + 12*f where f (obj+0x58) follows the
 *  tick counter u (obj+0x54): while charging u++, f = (u >> 3) + ((u >> 1) & 1) for u < 16, at u = 16 u jumps to 96 and
 *  f = (u >> 5) + ((u >> 3) & 1); once f >= lo it is lo (lo = hi) or lo + ((u >> 2) mod (hi - lo)). Launched (state 1):
 *  per tick the frame update, a move of 0.5 px along the facing, u++; removed once its record box (the hit box,
 *  per direction) has left the camera rect on the far side (destructor: stop 32); on every tick with (u & 3) == 0 one
 *  message to every object: record-box centre +- BB_SIZE[f], mult (charge * 0x5762 + 0xFFFF) >> 16, attack sub 13
 *  (accepted: sound 48 + rand(2)) - the ball flies on through the target. It explodes only at a projectile wall (the
 *  centre +- 4) or on an Energy Blast message overlapping its box and the centre +- size * 3/4: stop 32, one message
 *  centre +- 3 * size / 2 with mult 81 * charge, sound 23, then 60 ticks of harmless explosions (BigBangBoom). */
const BB_DEF: [number, number, number][] = [[4, 4, 0x08764e00], [6, 6, 0x08764e00], [7, 10, 0x0876aae0]]
const BB_SIZE = [4, 6, 8, 12, 16, 24, 28, 32, 32, 32]
/** record boxes (ox, oy, w, h) by f facing down (fallback when the record geometry is not loaded) */
const BB_REC: [number, number, number, number][] = [[8, 4, 16, 16], [8, 2, 16, 16], [8, 0, 16, 16], [16, 4, 32, 32], [16, 0, 32, 32], [32, 8, 64, 64], [32, 4, 64, 64], [32, 0, 64, 64], [32, 0, 64, 64], [32, 0, 64, 64]]
/** ball position relative to the player position while charging, per facing (0x0871DD9C) */
export const BIGBANG_POS: [number, number][] = [[2, 2], [12, -22], [-5, -6], [21, -6]]

export class BigBangBall extends Shot {
  charge256 = 0
  launched = false
  /** skill level 0..2 (ball definition) */
  lv: number
  /** obj+0x54 tick counter and obj+0x58 frame */
  u = 0
  f = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, lv = 0) {
    super(ctx, owner, dir, 1, x, y)
    this.kind = 'bigbang'
    this.lv = Math.max(0, Math.min(2, lv))
  }
  get charge() {
    return this.charge256 >> 8
  }
  get mult() {
    return Math.floor((this.charge * 0x5762 + 0xffff) / 65536)
  }
  private geom(): [number, number, number, number] {
    const g = recGeomAt(this.ctx.data.sprites, BB_DEF[this.lv][2] + this.dir * 120 + 12 * this.f)
    return g ? [g[0], g[1], g[2], g[3]] : BB_REC[this.f] ?? BB_REC[0]
  }
  /** the record box */
  rect(): Rect {
    const [ox, oy, w, h] = this.geom()
    return { x0: this.px - ox, y0: this.py - oy, x1: this.px - ox + w, y1: this.py - oy + h }
  }
  private cap() {
    const [lo, hi] = BB_DEF[this.lv]
    if (this.f >= lo) this.f = hi === lo ? lo : lo + ((this.u >> 2) % (hi - lo))
  }
  /** centre of the record box */
  center(): [number, number] {
    const r = this.rect()
    return [(r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1]
  }
  private around(s: number): Rect {
    const [cx, cy] = this.center()
    return { x0: cx - s, y0: cy - s, x1: cx + s, y1: cy + s }
  }
  /** past the camera edge on the side it flies to (0x08019B7C) */
  private gone(w: World): boolean {
    const r = this.rect()
    const c = w.camera
    return this.dir === 0 ? r.y0 >= c.y1 : this.dir === 1 ? r.y1 <= c.y0 : this.dir === 2 ? r.x1 <= c.x0 : r.x0 >= c.x1
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
    const [dx, dy] = DIR_VEC[this.dir]
    this.x += dx * 128
    this.y += dy * 128
    this.u++
    if (this.gone(w)) {
      this.removed = true
      audioEvent('sfx_stop', { id: 32 })
      return
    }
    if (this.inWall(w, this.around(4))) return this.explode(w)
    if ((this.u & 3) === 0 && this.hit(w, this.around(BB_SIZE[this.f]), ATK[13], this.mult, false)) {
      audioEvent('sfx', { id: 48 + w.rng.rand(2, 'big bang hit sfx'), vol: 64 })
    }
  }
  explode(w: World) {
    if (this.removed) return
    this.removed = true
    audioEvent('sfx_stop', { id: 32 })
    const s = (BB_SIZE[this.f] * 3) >> 1
    this.hit(w, this.around(s), ATK[13], 81 * this.charge, false)
    audioEvent('sfx', { id: 23, vol: 64 })
    const [cx, cy] = this.center()
    w.add(new BigBangBoom(this.ctx, cx - s, cy - s, s))
  }
  /** an Energy Blast message (attack sub 4, another object) overlapping the box and the centre +- size * 3/4 */
  onMessage(w: World, msg: HitMsg): boolean {
    if (!this.launched || this.removed || msg.atk.sub !== 4 || msg.source === this) return false
    if (!overlaps(msg.rect, this.rect()) || !overlaps(msg.rect, this.around((BB_SIZE[this.f] * 3) >> 2))) return false
    this.explode(w)
    return true
  }
  draw(r: Renderer) {
    const [, , table] = BB_DEF[this.lv]
    drawRecAddr(r, this.ctx.data.sprites, table + this.dir * 120 + 12 * this.f, this.px, this.py)
  }
}

/** Big Bang explosion phase (state 2 of 0x08037650): 60 ticks; when the countdown is a multiple of 8 (56 .. 8) an
 *  explosion 0x08712FDC (0x0801952C: rate 0x1000 + rand(0x1000), prio 0x38) appears at (x0 + rand(2s), y0 + rand(2s)).
 *  Harmless (the damage is the ball's one explosion message). */
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
/** Masenko (skill 17): a cursor object (ctor 0x08021594, update 0x0802163A, record 0x0872509C) at the body-box centre
 *  + dir * d and a ball object (class 0x08021280, update 0x0802133A) charging at MASENKO_MUZZLE, both from the press.
 *  Cursor (updates before the player's control): d starts at 0; Lv1 / Lv2 d += step (+-2) per tick, step -2 once
 *  d >= max (70 up / down, 100 left / right), +2 again when d < 16; Lv3 the d-pad toward the facing adds 2, away
 *  subtracts 2, clamped to 16..max. The B release launches the ball (0x08021452; facing up 8 px higher first):
 *  vx = dx * d * 0x5F4 >> 8, vy = (dy * d * 0x5F4 >> 8) - 0x200 (+0x20 per tick); a = 0x2000, b = 0x200, per tick
 *  b -= 0x20, a += b: when a < 0 (the 43rd flight tick) one message to every object with the flight record's box at
 *  that point (mult 3687 / 11060 / 16589, attack sub 7; accepted: sound 48 + rand(3)), then the blast records for
 *  4 ticks each, no further damage. Lockstep *_vs_skill17*. */
export const MASENKO_MUZZLE: [number, number][] = [[8, -24], [8, -24], [10, -24], [6, -24]]
/** level definitions 0x0872A690 (12 bytes): sparkle record (fx_08721164 index), blast table, blast records, half size */
const MASENKO_LV = [
  { spark: 9, blast: 0x0872a5dc, n: 4, half: 8 },
  { spark: 9, blast: 0x0872a60c, n: 5, half: 16 },
  { spark: 12, blast: 0x0872a648, n: 6, half: 32 },
]
const MASENKO_SPARK = 0x08721164

export class MasenkoCursor extends Shot {
  d = 0
  step = 2
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
  get max() {
    return this.dir < 2 ? 70 : 100
  }
  pos(): [number, number] {
    const [cx, cy] = this.owner.center()
    const [dx, dy] = DIR_VEC[this.dir]
    return [cx + dx * this.d, cy + dy * this.d]
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
    if (this.released >= 0) return
    if (this.lv < 2) {
      this.d += this.step
      if (this.d >= this.max) this.step = -2
      else if (this.step < 0 && this.d < 16) this.step = 2
    } else {
      const toward = [BTN.DOWN, BTN.UP, BTN.LEFT, BTN.RIGHT][this.dir]
      const away = [BTN.UP, BTN.DOWN, BTN.RIGHT, BTN.LEFT][this.dir]
      if (w.input.held & toward) this.d += 2
      else if (w.input.held & away) this.d -= 2
      this.d = Math.max(16, Math.min(this.max, this.d))
    }
    const [x, y] = this.pos()
    this.x = x << 8
    this.y = y << 8
  }
  /** the B release: the ball flies (the cursor is gone) */
  release() {
    if (this.released >= 0) return
    this.released = this.t
    this.removed = true
    this.ball?.launch(this.d)
  }
  draw(r: Renderer) {
    if (this.released >= 0) return
    const [x, y] = this.pos()
    drawRecAddr(r, this.ctx.data.sprites, 0x0872509c, x, y)
  }
}

export class MasenkoBall extends Shot {
  cursor: MasenkoCursor
  phase: 'charge' | 'fly' | 'blast' = 'charge'
  vx = 0
  vy = 0
  a = 0
  b = 0
  tb = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, cursor: MasenkoCursor, x: number, y: number) {
    super(ctx, owner, dir, 1, x, y)
    this.cursor = cursor
    this.kind = 'masenkoball'
  }
  private flightRec() {
    return MASENKO_SPARK + 12 * (MASENKO_LV[this.cursor.lv].spark + 1 + ((this.t >> 3) & 1))
  }
  rect() {
    const g = recGeomAt(this.ctx.data.sprites, this.flightRec())
    if (!g) return centered(this.px, this.py, 16, 16)
    return { x0: this.px - g[0], y0: this.py - g[1], x1: this.px - g[0] + g[2], y1: this.py - g[1] + g[3] }
  }
  launch(d: number) {
    if (this.dir === 1) this.y -= 8 << 8
    const [dx, dy] = DIR_VEC[this.dir]
    this.vx = (dx * d * 0x5f4) >> 8
    this.vy = ((dy * d * 0x5f4) >> 8) - 0x200
    this.a = 0x2000
    this.b = 0x200
    this.phase = 'fly'
  }
  update(w: World) {
    this.t++
    if (this.phase === 'charge') {
      if (this.cursor.removed && this.cursor.released < 0) this.removed = true
      return
    }
    if (this.phase === 'fly') {
      this.x += this.vx
      this.y += this.vy
      this.vy += 0x20
      this.b -= 0x20
      this.a += this.b
      if (this.a < 0) {
        if (this.hit(w, this.rect(), ATK[7], this.cursor.mult, false)) audioEvent('sfx', { id: 48 + w.rng.rand(3, 'masenko hit sfx'), vol: 64 })
        this.phase = 'blast'
        this.tb = 0
      }
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
/** Kamikaze ghost (skill 13): an actor (sprite 148) with its own animation state and AI. Created at the skill's tick 16
 *  (constructor SetAnim(0): the idle enter draws rand(255); then the appear animation raw 19, once shift 3, whose end
 *  is SetAnim(0) again: rand(255)); in formation it eases to its slot (pos += (slot - pos) >> 5). Released (committed
 *  at the skill's end) it goes to AI state 0x11B: Selector [IfContactExplode (0x080390A0: one message to every object
 *  with its frame box 0x0800FE34, energy sub 1 x4.0, dir = facing; accepted -> sound 48 + rand(2) and state 0x11C),
 *  home in (0x08038F2C, speed 0x300: the nearest enemy by frame-box centres; facing toward it on the major axis;
 *  within 4 px or no enemy -> state 0x11C; else SetAnim(1) and a step d * 0x300 / max(|dx|, |dy|)), state 0x11C].
 *  State 0x11C (SelfDestruct): an explosion (rate 0x1000 + rand(0x1000)) and gone. Lockstep f31_vs_skill13. */
export class Ghost extends Shot {
  free = false
  /** frames between the release and the AI state 0x11B running (the GotoState's tree updates from the next frame) */
  wait = 0
  /** formation slot (0..2, table 0x0871E024) */
  slot: number
  /** the actor's animation (raw / frame / tick) under its state rule */
  ea: EnemyAnim
  facing: Dir
  private boom = false
  private sprite: SpriteSet | undefined
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, slot = 0) {
    super(ctx, owner, dir, 1, x, y)
    this.kind = 'ghost'
    this.slot = slot
    this.facing = owner.facing
    this.sprite = ctx.data.sprites.index(148)
    this.ea = new EnemyAnim({
      nframes: (raw) => this.sprite?.anim(raw)?.steps(this.facing).length ?? 0,
      rand: (n, label) => (this.w ? this.w.rng.rand(n, label) : 0),
      sfx: (id, vol) => audioEvent('sfx', { id, vol }),
    })
  }
  private w: World | null = null
  /** the constructor: SetAnim(0) (rand(255)), then the appear animation */
  init(w: World) {
    this.w = w
    this.ea.enter(1, { rule: 'idle' })
    this.ea.enter(19, { rule: 'once', shift: 3, end: 'idle' })
  }
  rect() {
    return frameBoxOf(this.sprite, this.ea.pos.raw, this.facing, this.ea.pos.frame, this.px, this.py) ?? centered(this.px, this.py, 12, 12)
  }
  update(w: World) {
    this.w = w
    this.t++
    if (this.ea.tick() === 'idle') this.ea.enter(1, { rule: 'idle' })
    if (!this.free) {
      // formation (0x0804DC80): pos += (target - pos) / 32 toward the player + slot offset, once the appear animation
      // has ended (emulator: the ghost holds its spawn point while showing raw 19)
      this.facing = this.owner.facing
      if (this.ea.pos.raw === 19) return
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
    if (this.boom) {
      // state 0x11C SelfDestruct
      this.removed = true
      this.ctx.explosion(w, this.px, this.py, 0x1000 + w.rng.rand(0x1000, 'ghost explosion rate'))
      return
    }
    // IfContactExplode
    if (this.hit(w, this.rect(), ATK[1], 0x1000, false)) {
      audioEvent('sfx', { id: 48 + w.rng.rand(2, 'ghost hit sfx'), vol: 64 })
      this.boom = true
      return
    }
    // home in on the nearest enemy (enemy list order, frame-box centres, strict minimum)
    const r = this.rect()
    const gx = (r.x0 + r.x1) >> 1
    const gy = (r.y0 + r.y1) >> 1
    let best: [number, number] | null = null
    let bd = 0x7fffffff
    for (const e of this.ctx.enemies) {
      if (e.removed || (e as unknown as { defeated?: boolean }).defeated) continue
      const b = (e as unknown as Actor).frameBox()
      const dx = ((b.x0 + b.x1) >> 1) - gx
      const dy = ((b.y0 + b.y1) >> 1) - gy
      const d2 = dx * dx + dy * dy
      if (d2 < bd) {
        bd = d2
        best = [dx, dy]
      }
    }
    if (!best) {
      this.boom = true
      return
    }
    const [dx, dy] = best
    const ax = Math.abs(dx)
    const ay = Math.abs(dy)
    this.facing = ax > ay ? (dx > 0 ? 3 : 2) : dy > 0 ? 0 : 1
    const m = Math.max(ax, ay)
    if (m < 4) {
      this.boom = true
      return
    }
    if (this.ea.pos.raw !== 2) this.ea.enter(2, { rule: 'loop', shift: 3 })
    this.x += Math.trunc((dx * 0x300) / m)
    this.y += Math.trunc((dy * 0x300) / m)
  }
  draw(r: Renderer) {
    const s = this.sprite
    if (!s) return
    const a = s.anim(this.ea.pos.raw)
    if (a) s.draw(r.ctx, a, this.facing, r.sx(this.px), r.sy(this.py), 0, { index: this.ea.pos.frame })
  }
}

// ---------------------------------------------------------------------------------------------------------------
// enemy-only classes

/** energy wave (0x08049C98): segment from tail to head; head grows 4/frame to 48 px, then the segment moves
 *  4 px/frame; max 240 frames; after a hit / wall the tail catches up and it vanishes. */
export class EnergyWave extends Shot {
  len = 8
  mode = 0
  /** +0x1C: the flight counter (mode 0 updates without a hit) */
  life = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x, y)
    this.kind = 'wave'
    // ctor 0x0802A2EC: sound 35 (volume 0x40) (lockstep f02_vs_enemy_t40 / t45)
    audioEvent('sfx', { id: 35, vol: 64 })
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
    // update 0x0802A3D2 mode 0: the segment box (position .. head = position + dir * len, +-4) with the CURRENT
    // length: blocked on the projectile grid (0x08009324) -> mode 1; else the hit (0x08011C0C, energy x1.0)
    // accepted -> puff at the head (rate 0x1000 + rand(0x1000)), mode 1; else grow 4 (len < 48) or move 4, and
    // the counter +0x1C past 240 -> mode 1. (The test comes before the growth: a hit lands one frame after the
    // box first reaches the player - lockstep f02_vs_enemy_t40 / t45.) No camera test.
    const hx = this.px + dx * this.len
    const hy = this.py + dy * this.len
    if (this.inWall(w, this.rect())) {
      this.mode = 1
      return
    }
    if (this.hit(w, this.rect(), ATK[1], 1024)) {
      this.ctx.puffRng(w, hx, hy)
      this.mode = 1
      return
    }
    if (this.len < 48) this.len += 4
    else {
      this.x += dx * 4 * 256
      this.y += dy * 4 * 256
    }
    if (++this.life > 240) this.mode = 1
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
    // 0x080247C0: tests the player only (16 x 16 box, x0.05, sub 4), damage without a reaction, once per flame
    const p = this.ctx.player
    if (!this.done && p && !this.ownerDead()) {
      const msg: HitMsg = { team: 0, rect: this.rect(), dir: this.dir, mult: 51, atk: ATK[4], attacker: this.owner, source: this, noReact: true }
      if (p.onMessage(w, msg)) this.done = true
    }
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
  landed = -1
  /** (ddx, ddy): the thrower's aim offset (px, clamped, one axis; BombThrow) */
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number, ddx: number, ddy: number) {
    super(ctx, owner, dir, 0, x, y)
    this.vx = (ddx * 0x71d) >> 8
    this.vy = ((ddy * 0x71d) >> 8) - 0x200
    this.kind = 'bomb'
  }
  rect() {
    return centered(this.px, this.py, 16, 16)
  }
  /** update (class 0x0804A2E8, 0x0801C80A): counter +1; counters 1..40 move: pos += v, vy += 0x20 (the throw arcs in
   *  screen space, vy starts -0x200); at counter 71 it explodes: one message (16 x 16 box, energy x1.0), 4 debris, the
   *  explosion 0x0801952C (rate 0x1000 + rand(0x1000)) and sound 24 (lockstep f02_vs_enemy_bomb) */
  update(w: World) {
    this.t++
    if (this.t <= 40) {
      this.x += this.vx
      this.y += this.vy
      this.vy += 0x20
      if (this.t === 40) this.landed = this.t
      return
    }
    if (this.t >= 71) {
      this.removed = true
      this.hit(w, this.rect(), ATK[1], 1024)
      for (let i = 0; i < 4; i++) w.add(new Debris(this.ctx.fx('buu_debris'), this.px, this.py, i))
      this.ctx.explosion(w, this.px, this.py, 0x1000 + w.rng.rand(0x1000, 'bomb explosion rate'))
      audioEvent('sfx', { id: 24, vol: 64 })
    }
  }
  draw(r: Renderer) {
    let k = (this.t >> 2) & 3
    if (k === 3) k = 1
    drawRecAddr(r, this.ctx.data.sprites, 0x0871e984 + 12 * k, this.px, this.py)
  }
}

/** Buu stretch arm (class 0x0804CDD4, builder 0x08028C04, update 0x08028CF5). Fields: +0x1C/+0x20 origin (px),
 *  +0x2C/+0x30 unit vector 0x0874EACC[dir], +0x34 length (8 at spawn), +0x10 phase:
 *  0: head = origin + vector * len, box (hx - 8, hy - 8)..(hx + 8, hy + 8); a projectile-grid wall under it -> 1; a
 *    kind-0 message to every object (0x08011C0C: melee type 0x086B8088, x1.0, dir, owner); accepted -> 1; else
 *    len < 80 -> len + 4, else 1
 *  1: len - 4; len <= 0 -> removed */
export class StretchArm extends Shot {
  len = 8
  phase = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x << 8, y << 8)
    this.kind = 'stretch'
  }
  head(): [number, number] {
    const [dx, dy] = DIR_VEC[this.dir]
    return [this.px + dx * this.len, this.py + dy * this.len]
  }
  rect(): Rect {
    const [hx, hy] = this.head()
    return { x0: hx - 8, y0: hy - 8, x1: hx + 8, y1: hy + 8 }
  }
  update(w: World) {
    this.t++
    if (this.phase === 1) {
      this.len -= 4
      if (this.len <= 0) this.removed = true
      return
    }
    const box = this.rect()
    if (this.inWall(w, box) || this.hit(w, box, ATK[0], 1024, false)) {
      this.phase = 1
      return
    }
    if (this.len < 80) this.len += 4
    else this.phase = 1
  }
  /** records 0x0874EFEC + 24 * dir: arm record 0 every 8 px to len - 1, the fist record 1 at dir * len */
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const base = 0x0874efec + 24 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    for (let k = 0; k < this.len - 1; k += 8) drawRecAdd(r, bank, base, this.px + dx * k, this.py + dy * k)
    const [hx, hy] = this.head()
    drawRecAdd(r, bank, base + 12, hx, hy)
  }
}

/** Majin Buu candy beam (class 0x0804B3AC, builder 0x080286D4, update 0x080287AD). Fields: +0x20/+0x24 origin (px),
 *  +0x38/+0x3C unit vector 0x0874EA94[dir], +0x40 length (8 at spawn), +0x1C tick counter, +0x10 phase:
 *  0: head = origin + vector * len, box (hx - 8, hy - 8)..(hx + 8, hy + 8); a projectile-grid wall under it -> 1; a
 *    kind-0 message to every object (0x08011C0C: energy type 0x086B8090, multiplier 0, dir, owner); accepted -> the
 *    player turns into candy (Player.candyHit) and 1; else len < 64 -> len + 4, else the origin moves 4 px along;
 *    counter + 1 > 240 -> 1
 *  1: the origin moves 4 px along, len - 4; len <= 0 -> removed */
export class CandyBeam extends Shot {
  len = 8
  phase = 0
  cnt = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x << 8, y << 8)
    this.kind = 'candybeam'
  }
  head(): [number, number] {
    const [dx, dy] = DIR_VEC[this.dir]
    return [this.px + dx * this.len, this.py + dy * this.len]
  }
  rect(): Rect {
    const [hx, hy] = this.head()
    return { x0: hx - 8, y0: hy - 8, x1: hx + 8, y1: hy + 8 }
  }
  private advance() {
    const [dx, dy] = DIR_VEC[this.dir]
    this.x += (dx * 4) << 8
    this.y += (dy * 4) << 8
  }
  update(w: World) {
    this.t++
    if (this.phase === 1) {
      this.advance()
      this.len -= 4
      if (this.len <= 0) this.removed = true
      return
    }
    const box = this.rect()
    if (this.inWall(w, box)) {
      this.phase = 1
      return
    }
    if (!this.ownerDead() && this.hit(w, box, ATK[1], 0, false)) {
      const p = this.ctx.player
      if (p) p.candyHit(w)
      this.phase = 1
      return
    }
    if (this.len < 64) this.len += 4
    else this.advance()
    if (++this.cnt > 240) this.phase = 1
  }
  /** records 0x0874ED7C + 60 * dir: muzzle record k (k = (t >> 2) & 1) at the origin while growing, body record 2
   *  every 8 px, head record 3 + k at dir * len; pieces at position + origin */
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const base = 0x0874ed7c + 60 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    const x = this.px
    const y = this.py
    const k = (this.t >> 2) & 1
    if (this.phase === 0 && this.len < 64) drawRecAdd(r, bank, base + 12 * k, x, y)
    for (let o = 0; o < this.len - 1; o += 8) drawRecAdd(r, bank, base + 24, x + dx * o, y + dy * o)
    const [hx, hy] = this.head()
    drawRecAdd(r, bank, base + 12 * (3 + k), hx, hy)
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
/** Bio Mech grapple arm (class 0x0804B380, builder 0x08037EA8, update 0x08037FB4; spawned by the GrappleArm node's
 *  controller 0x03003294 at enemy position + 0x0876CCB4[dir]). Fields: +0x1C/+0x20 origin (px), +0x2C/+0x30 unit
 *  vector 0x0876CCE8[dir], +0x3C length (8 at spawn), +0x34/+0x38 the grip (px), +0x10 phase:
 *  0 extend: head = origin + vector * len; box (hx, hy - 12)..(hx + 32, hy + 12) (every direction); a projectile-grid
 *    wall under the box -> 1; message kind 0 to every object (0x08011C0C: melee type 0x086B8088, x1.0, dir, owner);
 *    accepted -> player HP > 0 ? grab : 1; not accepted -> len += 4 (len >= 80 -> 1)
 *  grab: grip = head + 0x0876CD08[dir]; the player blocking (+0x1C8) and facing dir ^ 1 -> 1; the grip on the
 *    walk grid (0x080097C2 with the player's footprint) -> player SetState(0x03001B00 hold, frame kept) at the grip,
 *    phase 2; else 1
 *  2 reel: grip -= vector * 4; fits -> the player there, else SetAnim(0) on the player and phase 1; then like 1
 *  1 retract: len -= 4; len <= 0 -> (phase 2: the player SetAnim(0)) removed */
export class GrappleArm extends Shot {
  len = 8
  phase = 0
  gx = 0
  gy = 0
  constructor(ctx: BuuCtx, owner: Actor, dir: Dir, x: number, y: number) {
    super(ctx, owner, dir, 0, x, y)
    this.kind = 'grapple'
  }
  head(): [number, number] {
    const [dx, dy] = DIR_VEC[this.dir]
    return [this.px + dx * this.len, this.py + dy * this.len]
  }
  rect(): Rect {
    const [hx, hy] = this.head()
    return { x0: hx, y0: hy - 12, x1: hx + 32, y1: hy + 12 }
  }
  update(w: World) {
    this.t++
    const p = this.ctx.player
    const [dx, dy] = DIR_VEC[this.dir]
    if (this.phase === 0) {
      const box = this.rect()
      if (this.inWall(w, box)) {
        this.phase = 1
        return
      }
      if (!this.hit(w, box, ATK[0], 1024, false)) {
        if (this.len >= 80) this.phase = 1
        else this.len += 4
        return
      }
      if (!p || p.hp <= 0) {
        this.phase = 1
        return
      }
      this.phase = 2
      const [hx, hy] = this.head()
      const G = [[8, 20], [8, 0], [-8, 16], [0, 16]][this.dir]
      this.gx = hx + G[0]
      this.gy = hy + G[1]
      // a front block (blocking flag and facing the arm) stops the grab
      if (p.blocking && p.facing === (this.dir ^ 1)) {
        this.phase = 1
        return
      }
      if (p.fits(w, this.gx, this.gy)) {
        p.enterHold(w)
        p.x = this.gx << 8
        p.y = this.gy << 8
      } else this.phase = 1
      return
    }
    if (this.phase === 2 && p) {
      this.gx -= dx * 4
      this.gy -= dy * 4
      if (p.fits(w, this.gx, this.gy)) {
        p.x = this.gx << 8
        p.y = this.gy << 8
      } else {
        p.toIdle(w)
        this.phase = 1
      }
    }
    this.len -= 4
    if (this.len > 0) return
    if (this.phase === 2) p?.toIdle(w)
    this.removed = true
  }
  /** chain record 0 every 8 px to len - 1, the head at dir * len: open hand record 1, or while grabbing the closed
   *  hand records 2 (back) + 3 (front); pieces at position + origin */
  draw(r: Renderer) {
    const bank = this.ctx.data.sprites
    const base = 0x0876dff8 + 48 * this.dir
    const [dx, dy] = DIR_VEC[this.dir]
    for (let k = 0; k < this.len - 1; k += 8) drawRecAdd(r, bank, base, this.px + dx * k, this.py + dy * k)
    const [hx, hy] = this.head()
    if (this.phase === 2) {
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
