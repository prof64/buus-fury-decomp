// Buu's Fury world-object machinery: actors (player + enemies) with 24.8 positions, the collision footprint
// (pos .. pos + header shadow w/h, obj+0x19C), the per-direction body box from the sprite definition (0x0800FE78),
// height offset (obj+0x176, bosses flying off-screen), sprite animation by engine anim number (raw index = n + 1),
// and the hit-message broadcast (0x08011C70). Damage uses damage.ts.

import type { Dir, Rect } from '../../core/geom'
import { overlaps } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { Anim, SpriteSet } from '../../core/sprites'
import type { Entity, World } from '../../core/world'
import type { AttackType, CombatStats } from './damage'

/** hit message: box + hit struct {dir, mult, attack type, attacker} (notes §8b) */
export interface HitMsg {
  /** 0 = enemy attack (hits the player), 1 = player attack (hits enemies), 2 = hits every actor */
  team: 0 | 1 | 2
  rect: Rect
  dir: Dir
  /** 0x400 = x1.0 */
  mult: number
  atk: AttackType
  attacker: Actor
  source?: unknown
  crit?: boolean
  raw?: number
  dmg?: number
}

export interface Receiver {
  onMessage(w: World, msg: HitMsg): boolean
}

export function isReceiver(e: unknown): e is Receiver {
  return typeof (e as Receiver).onMessage === 'function'
}

/** 0x08011C70: send to every object; `first` stops at the first receiver that accepts */
export function broadcast(w: World, msg: HitMsg, first = false): boolean {
  let any = false
  for (const e of w.entities.slice()) {
    if (e.removed || !isReceiver(e)) continue
    if (e.onMessage(w, msg)) {
      any = true
      if (first) break
    }
  }
  w.debugRect(msg.rect, msg.team === 1 ? '#ffd23f' : msg.team === 0 ? '#ff4040' : '#ff80ff', 4)
  return any
}

export abstract class Actor implements Entity, Receiver {
  id = 0
  abstract kind: string
  removed = false
  layer = 0
  x = 0
  y = 0
  /** height offset in px (negative = up, obj+0x176) */
  z = 0
  facing: Dir = 0
  stats: CombatStats = { STR: 1, POW: 1, END: 0, END2: 0, resM: 128, resE: 128, critBonus: 0, blockFactor: 0x1000 }
  /** obj+0x1C8 */
  blocking = false
  /** obj+0x1CC "can be hit / active" */
  hittable = true
  footW = 16
  footH = 8
  body: number[][] = [
    [0, -22, 16, 8],
    [0, -22, 16, 8],
    [0, -23, 16, 7],
    [0, -23, 16, 7],
  ]
  sprite: SpriteSet | undefined
  anim: Anim | undefined
  animT = 0
  animLoop: boolean | undefined
  flash = 0
  alpha = 1

  get px() {
    return this.x >> 8
  }
  get py() {
    return this.y >> 8
  }
  depth() {
    return this.py
  }
  /** body box 0x0800FE78 (hurt rect) */
  bodyBox(): Rect {
    const b = this.body[this.facing] ?? this.body[0]
    return { x0: this.px + b[0], y0: this.py + b[1] + this.z, x1: this.px + b[2], y1: this.py + b[3] + this.z }
  }
  footBox(): Rect {
    return { x0: this.px, y0: this.py, x1: this.px + this.footW, y1: this.py + this.footH }
  }
  center(): [number, number] {
    const b = this.bodyBox()
    return [(b.x0 + b.x1) >> 1, (b.y0 + b.y1) >> 1]
  }
  footCenter(): [number, number] {
    return [this.px + (this.footW >> 1), this.py + (this.footH >> 1)]
  }

  setSprite(s: SpriteSet | undefined) {
    this.sprite = s
    const hb = (s?.json.hitboxes as { body_per_direction?: { dir: string; x0: number; y0: number; x1: number; y1: number }[] } | undefined)?.body_per_direction
    if (hb && hb.length) {
      const by = (d: string) => hb.find((h) => h.dir === d) ?? hb[0]
      const nz = (h: { x0: number; y0: number; x1: number; y1: number }) => (h.x1 - h.x0 > 0 ? [h.x0, h.y0, h.x1, h.y1] : [0, -22, 16, 8])
      this.body = ['down', 'up', 'left', 'right'].map((d) => nz(by(d)))
    }
    const hdr = (s?.json as unknown as { header?: { shadow?: number[] } } | undefined)?.header
    if (hdr?.shadow) {
      this.footW = hdr.shadow[0] || 16
      this.footH = hdr.shadow[1] || 8
    }
  }

  /** set by engine anim number (raw index n+1) or sprites.json name(s); first that exists wins */
  setAnim(ref: number | string | (number | string)[], restart = true, loop?: boolean) {
    if (!this.sprite) return
    const refs = (Array.isArray(ref) ? ref : [ref]).map((r) => (typeof r === 'number' ? r + 1 : r))
    const a = this.sprite.pick(...refs, 'idle')
    if (!a) return
    if (a !== this.anim || restart) this.animT = 0
    this.anim = a
    this.animLoop = loop
  }
  animLength(): number {
    return this.anim ? this.anim.length(this.facing) : 0
  }
  animDone(): boolean {
    return !this.anim || this.animT >= this.animLength()
  }

  fits(w: World, x: number, y: number): boolean {
    const b = w.arena.bounds
    return x >= b.x0 && y >= b.y0 && x + this.footW <= b.x1 && y + this.footH <= b.y1
  }
  /** collision try-move (vfunc +0x54): per axis */
  moveBy(w: World, vx: number, vy: number): boolean {
    let ok = true
    if (vx) {
      if (this.fits(w, (this.x + vx) >> 8, this.py)) this.x += vx
      else ok = false
    }
    if (vy) {
      if (this.fits(w, this.px, (this.y + vy) >> 8)) this.y += vy
      else ok = false
    }
    return ok
  }
  clampInside(w: World) {
    const b = w.arena.bounds
    const px = Math.max(b.x0, Math.min(b.x1 - this.footW, this.px))
    const py = Math.max(b.y0, Math.min(b.y1 - this.footH, this.py))
    if (px !== this.px) this.x = px << 8
    if (py !== this.py) this.y = py << 8
  }

  tickAnim() {
    this.animT++
    if (this.flash > 0) this.flash--
  }

  abstract update(w: World): void
  abstract onMessage(w: World, msg: HitMsg): boolean

  draw(r: Renderer, _w: World) {
    if (!this.sprite || !this.anim) return
    const x = r.sx(this.px)
    const y = r.sy(this.py + this.z)
    const white = this.flash > 0 && (this.flash & 2) !== 0
    this.sprite.draw(r.ctx, this.anim, this.facing, x, y, this.animT, { white, loop: this.animLoop, alpha: this.alpha })
  }

  drawDebug(r: Renderer, _w: World) {
    r.strokeRect(this.bodyBox(), this.hittable ? '#3cf' : '#357')
    r.strokeRect(this.footBox(), '#0f0')
  }
}

export function hitsBody(msg: HitMsg, a: Actor) {
  return overlaps(msg.rect, a.bodyBox())
}

/** visual-only effect: plays the frames of a sprite set once */
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
  constructor(sprite: SpriteSet | undefined, x: number, y: number, frames: number[], frameDur = 4) {
    this.sprite = sprite
    this.x = x
    this.y = y
    this.frames = frames
    this.frameDur = frameDur
  }
  depth() {
    return this.y
  }
  update() {
    if (++this.t >= this.frames.length * this.frameDur) this.removed = true
  }
  draw(r: Renderer) {
    if (!this.sprite) return
    const f = this.frames[Math.min(this.frames.length - 1, Math.floor(this.t / this.frameDur))]
    const fr = this.sprite.json.frames[f]
    if (!fr) return
    // effect frames are drawn centred
    this.sprite.drawFrame(r.ctx, f, r.sx(this.x) - (fr.w >> 1) + fr.anchor.x, r.sy(this.y) - (fr.h >> 1) + fr.anchor.y, false)
  }
}
