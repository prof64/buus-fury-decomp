// Common LoG1 entity (0x40-byte object, notes §1.3): state number == animation number, tick counter (+0x10),
// facing (+0x13), generic counter (+0x24), hit flash (+0x28). The game runs every entity's update (last to first)
// and later, while rendering, every entity's anim_tick; the battle does the same (anim ticks in postStep).

import type { Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { SpriteSet } from '../../core/sprites'
import type { Entity, World } from '../../core/world'
import type { Log1Game } from './game'

/** animation names of the state numbers (strings at 0x08010F08) as named in sprites.json (rows are looked up by number) */
export const STATE_ANIM: Record<number, string> = {
  0: 'idle',
  1: 'walk',
  2: 'death',
  3: 'melee1',
  4: 'melee2',
  5: 'ki',
  6: 'excited',
  7: 'sad',
  8: 'transform',
  9: 'super_saiyan',
  10: 'monkey',
  14: 'flying',
  15: 'bored',
  16: 'mallet',
}

export abstract class Log1Entity implements Entity {
  id = 0
  abstract kind: string
  removed = false
  layer = 0
  /** +0x20/+0x22 position (px) */
  x = 0
  y = 0
  /** +0x13 facing */
  facing = 0
  /** state number (== animation number) */
  st = 0
  /** +0x10 tick, +0x12 frame index (code-driven), +0x24 generic counter */
  tick = 0
  frameIdx = -1
  t24 = 0
  /** hit flash frames (0x0800BEB8: 8) */
  flashT = 0
  sprite: SpriteSet | undefined
  g: Log1Game
  constructor(g: Log1Game) {
    this.g = g
  }
  depth() {
    return this.y
  }
  /** set_state 0x08002466: switch state, run the enter callback (enter callbacks reset the tick, not set_state) */
  setState(n: number) {
    this.st = n
    this.frameIdx = -1
    this.enter(n)
  }
  enter(_n: number) {}
  hasState(n: number): boolean {
    return !!this.sprite?.anim(n)
  }
  entState() {
    return this.st
  }
  flash(frames: number) {
    this.flashT = Math.max(this.flashT, frames)
  }
  /** death (set_char_state 7 -> 0x0800BEEC): a flash object that switches the entity to Death after 8 updates */
  startDeath() {
    this.flashT = 8
    this.g.world.add(new DeathFlash(this))
  }
  abstract update(w: World): void
  /** anim_tick (render pass) */
  animTick(): void {
    this.tick = (this.tick + 1) & 0xff
  }
  /** message handler (+0x38): returns true when the message was accepted */
  message(_w: World, _msg: number, _arg: unknown): boolean {
    return false
  }

  /** rect of the sprite frame currently shown (0x08002328: position + frame descriptor offset/size) */
  frameRect(): Rect {
    const anim = this.sprite?.anim(this.st) ?? this.sprite?.anim(0)
    const dir = this.facing as 0 | 1 | 2 | 3
    const step = anim ? (this.frameIdx >= 0 ? anim.stepIndex(dir, this.frameIdx) : anim.stepAt(dir, this.tick)) : null
    const f = step ? this.sprite!.json.frames[step.frame] : null
    if (!f) return { x0: this.x, y0: this.y - 24, x1: this.x + 16, y1: this.y + 8 }
    const x0 = this.x - f.anchor.x
    const y0 = this.y - f.anchor.y
    return { x0, y0, x1: x0 + f.w, y1: y0 + f.h }
  }

  draw(r: Renderer, _w: World) {
    if (!this.sprite) return
    const anim = this.sprite.anim(this.st) ?? this.sprite.anim(0)
    if (!anim) return
    const white = this.flashT > 0 && (this.flashT & 2) !== 0
    const dir = this.facing as 0 | 1 | 2 | 3
    if (this.frameIdx >= 0) this.sprite.draw(r.ctx, anim, dir, r.sx(this.x), r.sy(this.y), 0, { index: this.frameIdx, white })
    else this.sprite.draw(r.ctx, anim, dir, r.sx(this.x), r.sy(this.y), this.tick, { white })
  }
}

/** The death flash (state table 0x08011F18): created when a char dies, updated with the other entities; after 8
 *  updates its owner enters the Death state (2). Created before the update pass (AI-tick kills) it runs the same
 *  frame, created during it (punch / projectile kills) it starts the next frame - as in the ROM traces. */
export class DeathFlash implements Entity {
  id = 0
  kind = 'death_flash'
  removed = false
  layer = 1
  n = 8
  owner: Log1Entity
  constructor(owner: Log1Entity) {
    this.owner = owner
  }
  depth() {
    return 0
  }
  update() {
    if (--this.n <= 0) {
      this.owner.setState(2)
      this.removed = true
    }
  }
  draw() {}
}

/** point/rect helpers */
export function inRect(x: number, y: number, r: Rect) {
  return x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1
}
export function overlapStrict(a: Rect, b: Rect) {
  return a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/** msg 1 hit record {rect, damage, enemy_owned, once_per_invuln, owner_char} */
export interface HitRecord {
  rect: Rect
  dmg: number
  enemyOwned: boolean
  once: boolean
  owner: import('./game').Char
}

/** 0x0800283C: send to entities from the end of the list, stop at the first that accepts */
export function sendFirst(w: World, msg: number, arg: unknown): boolean {
  for (let i = w.entities.length - 1; i >= 0; i--) {
    const e = w.entities[i] as Partial<Log1Entity>
    if (e.removed || typeof e.message !== 'function') continue
    if (e.message(w, msg, arg)) return true
  }
  return false
}
/** 0x0800286A: send to every entity */
export function sendAll(w: World, msg: number, arg: unknown) {
  for (let i = w.entities.length - 1; i >= 0; i--) {
    const e = w.entities[i] as Partial<Log1Entity>
    if (e.removed || typeof e.message !== 'function') continue
    e.message(w, msg, arg)
  }
}
