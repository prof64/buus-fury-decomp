// Buu's Fury world-object machinery: actors (player + enemies) with 24.8 positions, the collision footprint
// (pos .. pos + header shadow w/h, obj+0x19C), the per-direction body box from the sprite definition (0x0800FE78),
// height offset (obj+0x176, bosses flying off-screen), sprite animation by engine anim number (raw index = n + 1),
// and the hit-message broadcast (0x08011C70). Damage uses damage.ts.

import type { Dir, Rect } from '../../core/geom'
import { overlaps } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { Anim, SpriteBank, SpriteSet } from '../../core/sprites'
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
  /** damage only (ComputeDamage + 0x08010BF2): no reaction, no i-frames (the flames 0x080247C0) */
  noReact?: boolean
  /** message kind 1: tested against the receiver's footprint (0x0800FDFA) instead of its body box */
  foot?: boolean
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
  // the game delivers newest object first (0x08011C70; story playthrough: Goku's punch on two fighters - the newer
  // one is hit first; wf-core world.rs broadcast)
  for (const e of w.entities.slice().reverse()) {
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
  /** code-driven frame index (obj+0x11, the player's state ticks: panim.ts); null = time-driven by animT */
  frameIdx: number | null = null
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
  /** the body box at the ground position (no height) */
  bodyBoxGround(): Rect {
    const b = this.body[this.facing] ?? this.body[0]
    return { x0: this.px + b[0], y0: this.py + b[1], x1: this.px + b[2], y1: this.py + b[3] }
  }
  footBox(): Rect {
    return { x0: this.px, y0: this.py, x1: this.px + this.footW, y1: this.py + this.footH }
  }
  /** the ROM frame record of the frame shown (sprites.json step `rom`), 0 when unknown */
  frameRecord(): number {
    if (!this.sprite || !this.anim) return 0
    const steps = this.anim.steps(this.facing)
    if (!steps.length) return 0
    const st = this.frameIdx != null ? this.anim.stepIndex(this.facing, this.frameIdx) : this.anim.stepAt(this.facing, this.animT, this.animLoop)
    const i = st ? steps.indexOf(st) : -1
    const j = this.sprite.json.animations[this.anim.name] as unknown as { directions: Record<string, { rom?: string }[] | undefined> } | undefined
    const d = j?.directions[['down', 'up', 'left', 'right'][this.facing]] ?? j?.directions.any
    const rom = i >= 0 ? d?.[i]?.rom : undefined
    return rom ? parseInt(rom, 16) : 0
  }
  /** 0x0800FE34: the shown frame record's box (+4 s8 ox, +5 s8 oy, +6 w, +7 h) at the integer position; the body box
   *  when the record is unknown */
  frameBox(): Rect {
    const b = FRAME_BOXES.get(this.frameRecord())
    if (!b) return this.bodyBox()
    return { x0: this.px + b[0], y0: this.py + b[1], x1: this.px + b[0] + b[2], y1: this.py + b[1] + b[3] }
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
      // an empty rect stays empty (0x0800FE78: the Enzyme's body exists only facing down - nothing hits it otherwise,
      // lockstep f02_vs_hold_latch); only a sprite with no box at all gets the default
      const nz = (h: { x0: number; y0: number; x1: number; y1: number }) => [h.x0, h.y0, h.x1, h.y1]
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

  /** map collision for a footprint at (x, y) px (player: walk grid, enemies: enemy grid); default: none */
  passable(_w: World, _x: number, _y: number): boolean {
    return true
  }
  /** footprint (x, y) .. (x + footW, y + footH) inside the arena bounds and on passable tiles */
  fits(w: World, x: number, y: number): boolean {
    const b = w.arena.bounds
    return x >= b.x0 && y >= b.y0 && x + this.footW <= b.x1 && y + this.footH <= b.y1 && this.passable(w, x, y)
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

  /** sprite bank holding the shadow / halo OBJs (set when the ruleset loads) */
  static bank: SpriteBank | null = null
  /** ground shadow from the sprite header (shadow bytes [2], [3] -> table 0x086B759C); null = none */
  shadowSprite(): SpriteSet | null {
    const sh = (this.sprite?.json as unknown as { header?: { shadow?: number[] } } | undefined)?.header?.shadow
    if (!sh || !sh[2]) return null
    return Actor.bank?.get(`buu_shadow_${sh[2]}x${sh[3]}`) ?? null
  }

  /** cosmetic sprite shake (px), applied to body, shadow and halo */
  shakeX = 0
  shakeY = 0

  draw(r: Renderer, _w: World) {
    if (!this.sprite || !this.anim) return
    const x = r.sx(this.px) + this.shakeX
    const y = r.sy(this.py + this.z) + this.shakeY
    const white = this.flash > 0 && (this.flash & 2) !== 0
    // OAM order of a WF actor: halo, body, shadow (lower index on top). The shadow OBJ sits at the entity
    // position on the ground (it does not follow the height offset).
    const sh = this.shadowSprite()
    if (sh && this.alpha > 0) sh.drawFrame(r.ctx, 0, r.sx(this.px) + this.shakeX, r.sy(this.py) + this.shakeY, false, { alpha: this.alpha })
    const fi = this.frameIdx
    this.sprite.draw(r.ctx, this.anim, this.facing, x, y, this.animT, { white, loop: this.animLoop, alpha: this.alpha, index: fi ?? undefined })
    const step = fi != null ? this.anim.stepIndex(this.facing, fi) : this.anim.stepAt(this.facing, this.animT, this.animLoop)
    if (step?.attach) {
      const halo = Actor.bank?.get('buu_halo')
      halo?.drawFrame(r.ctx, 0, x + step.attach[0], y + step.attach[1], false, { alpha: this.alpha })
    }
  }

  drawDebug(r: Renderer, _w: World) {
    r.strokeRect(this.bodyBox(), this.hittable ? '#3cf' : '#357')
    r.strokeRect(this.footBox(), '#0f0')
  }
}

/** the sprite frame records' boxes (data/battle/buu_frame_boxes.json: ROM address -> [ox, oy, w, h]) */
let FRAME_BOXES: Map<number, [number, number, number, number]> = new Map()
export function setFrameBoxes(j: { boxes?: Record<string, [number, number, number, number]> } | undefined) {
  FRAME_BOXES = new Map(Object.entries(j?.boxes ?? {}).map(([k, v]) => [parseInt(k, 16), v]))
}

/** the frame record box of sprite `s`, animation `raw`, direction, frame index at (px, py); null when unknown */
export function frameBoxOf(s: SpriteSet | undefined, raw: number, dir: Dir, frame: number, px: number, py: number): Rect | null {
  const a = s?.anim(raw)
  if (!s || !a) return null
  const j = s.json.animations[a.name] as unknown as { directions: Record<string, { rom?: string }[] | undefined> } | undefined
  const d = j?.directions[['down', 'up', 'left', 'right'][dir]] ?? j?.directions.any
  const rom = d?.[Math.min(frame, (d?.length ?? 1) - 1)]?.rom
  const b = rom ? FRAME_BOXES.get(parseInt(rom, 16)) : undefined
  return b ? { x0: px + b[0], y0: py + b[1], x1: px + b[0] + b[2], y1: py + b[1] + b[3] } : null
}

export function hitsBody(msg: HitMsg, a: Actor) {
  return overlaps(msg.rect, msg.foot ? a.footBox() : a.bodyBox())
}

/** the ROM's sine table 0x087B6EEC: 256 steps per turn, 1.0 = 0x10000 (= trunc(sin * 65536), all 256 checked) */
export const SIN256: number[] = Array.from({ length: 256 }, (_, i) => Math.trunc(Math.sin((i * 2 * Math.PI) / 256) * 65536))

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
