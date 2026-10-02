// Shared LoG2 world-object machinery: actors (player + enemies) with 24.8 positions, collision boxes, hurt rects,
// sprite animation and hit-flash; the attack message broadcast (World::broadcast 0x08006DA6) and simple effects.

import type { Dir, Rect } from '../../core/geom'
import { overlaps, rectAt } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { Anim, SpriteSet } from '../../core/sprites'
import type { Entity, World } from '../../core/world'
import type { AttackDesc, CombatStats } from './damage'

/** Hit packet (stack struct built by every attack, notes Part B §7). */
export interface HitMsg {
  /** 0 = hostile to player, 1 = player's attack, 2 = hits everyone */
  team: 0 | 1 | 2
  rect: Rect
  /** direction of the hit (+0x14): attacker / projectile facing, used for knockback */
  facing: Dir
  scale: number
  attack: AttackDesc
  attacker: Actor
  source?: unknown
  /** outputs filled by the receiver's computeDamage */
  crit?: boolean
  raw?: number
  dmg?: number
}

export const MSG_ATTACK = 0
export const MSG_PROBE = 2

/** Receivers of broadcast messages (actors and projectiles that can be cancelled). */
export interface Receiver {
  onMessage(w: World, type: number, msg: HitMsg): boolean
}

export function isReceiver(e: unknown): e is Receiver {
  return typeof (e as Receiver).onMessage === 'function'
}

/** 0x08006DA6 broadcast(list, type, data): every object gets onMessage, results ORed. */
export function broadcast(w: World, type: number, msg: HitMsg, first = false): boolean {
  let any = false
  const list = w.entities.slice()
  for (const e of list) {
    if (e.removed) continue
    if (isReceiver(e) && e.onMessage(w, type, msg)) {
      any = true
      if (first) break
    }
  }
  w.debugRect(msg.rect, msg.team === 1 ? '#ffd23f' : msg.team === 0 ? '#ff4040' : '#ff80ff', type === MSG_PROBE ? 1 : 6)
  return any
}

export abstract class Actor implements Entity, Receiver {
  id = 0
  abstract kind: string
  removed = false
  layer = 0
  /** 24.8 fixed point position (top-left of the collision box) */
  x = 0
  y = 0
  facing: Dir = 0
  /** entity+0x160..0x164 */
  stats: CombatStats = { STR: 1, POW: 1, END: 0, meleeMult: 128, energyMult: 128 }
  /** entity+0x168: can be hit */
  hittable = true
  colW = 16
  colH = 8
  /** hurt rects per facing [x0,y0,x1,y1] relative to the integer position */
  hurt: number[][] = [
    [-2, -26, 18, 10],
    [-2, -26, 18, 10],
    [-2, -26, 18, 10],
    [-2, -26, 18, 10],
  ]
  sprite: SpriteSet | undefined
  anim: Anim | undefined
  animT = 0
  /** code-driven frame index (null = time-driven from sprites.json durations) */
  animIndex: number | null = null
  animLoop: boolean | undefined = undefined
  flash = 0
  /** blink (alpha) instead of flash */
  blink = 0

  get px() {
    return this.x >> 8
  }
  get py() {
    return this.y >> 8
  }
  depth() {
    return this.py
  }
  hurtRect(): Rect {
    return rectAt(this.hurt[this.facing], this.px, this.py)
  }
  colBox(): Rect {
    return { x0: this.px, y0: this.py, x1: this.px + this.colW, y1: this.py + this.colH }
  }
  center(): [number, number] {
    return [this.px + (this.colW >> 1), this.py + (this.colH >> 1)]
  }

  setAnim(ref: string | number | (string | number)[] | undefined, restart = true, loop?: boolean) {
    if (!this.sprite || ref == null) return
    const refs = Array.isArray(ref) ? ref : [ref]
    const a = this.sprite.pick(...refs)
    if (!a) return
    if (a !== this.anim || restart) {
      if (a !== this.anim || restart) this.animT = 0
      this.anim = a
    }
    this.animIndex = null
    this.animLoop = loop
  }
  animLength(): number {
    return this.anim ? this.anim.length(this.facing) : 0
  }
  animDone(): boolean {
    return !this.anim || this.animT >= this.animLength()
  }

  /** would the collision box at integer pos (x,y) stay inside the arena */
  fits(w: World, x: number, y: number): boolean {
    const b = w.arena.bounds
    return x >= b.x0 && y >= b.y0 && x + this.colW <= b.x1 && y + this.colH <= b.y1
  }

  /** 0x08007D24-style move: per axis, blocked axes don't move. Returns false if any axis was blocked. */
  moveTo(w: World, nx: number, ny: number): boolean {
    let ok = true
    if (nx !== this.x) {
      if (this.fits(w, nx >> 8, this.py)) this.x = nx
      else ok = false
    }
    if (ny !== this.y) {
      if (this.fits(w, this.px, ny >> 8)) this.y = ny
      else ok = false
    }
    return ok
  }

  tickAnim() {
    this.animT++
    if (this.flash > 0) this.flash--
    if (this.blink > 0) this.blink--
  }

  abstract update(w: World): void
  abstract onMessage(w: World, type: number, msg: HitMsg): boolean

  draw(r: Renderer, _w: World) {
    if (!this.sprite || !this.anim) return
    if (this.blink > 0 && (this.blink & 2)) return
    const x = r.sx(this.px)
    const y = r.sy(this.py)
    // hit flash (overlay 0x0802459C, 12 frames): alternate white silhouette
    const white = this.flash > 0 && (this.flash & 2) !== 0
    if (this.animIndex != null) this.sprite.draw(r.ctx, this.anim, this.facing, x, y, 0, { index: this.animIndex, white })
    else this.sprite.draw(r.ctx, this.anim, this.facing, x, y, this.animT, { white, loop: this.animLoop })
  }

  drawDebug(r: Renderer, _w: World) {
    r.strokeRect(this.hurtRect(), this.hittable ? '#3cf' : '#357')
    r.strokeRect(this.colBox(), '#0f0')
  }
}

/** Visual-only effect: plays frames of a sprite set once (or for `life` frames) at a fixed spot. */
export class Effect implements Entity {
  id = 0
  kind = 'effect'
  removed = false
  layer = 1
  t = 0
  sprite: SpriteSet | undefined
  x: number
  y: number
  frames: number[]
  frameDur: number
  flip: boolean
  constructor(sprite: SpriteSet | undefined, x: number, y: number, frames: number[], frameDur = 4, flip = false) {
    this.sprite = sprite
    this.x = x
    this.y = y
    this.frames = frames
    this.frameDur = frameDur
    this.flip = flip
  }
  depth() {
    return this.y
  }
  update() {
    this.t++
    if (this.t >= this.frames.length * this.frameDur) this.removed = true
  }
  draw(r: Renderer) {
    if (!this.sprite) return
    const f = this.frames[Math.min(this.frames.length - 1, Math.floor(this.t / this.frameDur))]
    this.sprite.drawFrame(r.ctx, f, r.sx(this.x), r.sy(this.y), this.flip)
  }
}

export function hitOverlaps(msg: HitMsg, a: Actor) {
  return overlaps(msg.rect, a.hurtRect())
}
