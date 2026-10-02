// Game-agnostic world: an ordered entity list updated once per fixed frame (last to first, like the Webfoot
// World::update), a seeded RNG, the sampled input, a GBA-sized logical camera, an arena, damage popups and a
// debug event log. Rulesets put their own entity classes in it.

import { imageNow } from './assets'
import { GBA_H, GBA_W, type Renderer } from './renderer'
import type { Rect } from './geom'
import type { InputFrame } from './input'
import type { Rng } from './rng'
import { drawText, textWidth } from './pixelfont'

export interface Entity {
  id: number
  kind: string
  removed: boolean
  /** y-sort key in world pixels */
  depth(): number
  /** 0 = ground objects (y-sorted), 1 = always above (effects, beams), -1 = below (shadows) */
  layer: number
  update(w: World): void
  draw(r: Renderer, w: World): void
  drawDebug?(r: Renderer, w: World): void
  debugLabel?(): string
}

export interface ArenaRuntime {
  id: string
  name: string
  /** background image (map composite) drawn at world origin */
  image: string
  size: [number, number]
  /** walkable rectangle in world px: an entity's collision box must stay inside */
  bounds: Rect
}

export interface Popup {
  text: string
  x: number
  y: number
  age: number
  life: number
  color: string
  vy: number
}

export interface DebugEvent {
  frame: number
  kind: 'damage' | 'ai' | 'state' | 'info'
  text: string
}

export interface BattleStats {
  frames: number
  damageDealt: number
  damageTaken: number
  hitsDealt: number
  hitsTaken: number
  crits: number
  kills: number
  enemiesTotal: number
  maxHit: number
  epSpent: number
  extra: Record<string, number | string>
}

export interface DebugFlags {
  hitboxes: boolean
  labels: boolean
  log: boolean
}

export class World {
  frame = 0
  entities: Entity[] = []
  input: InputFrame = { held: 0, pressed: 0, released: 0 }
  camera: Rect = { x0: 0, y0: 0, x1: GBA_W, y1: GBA_H }
  popups: Popup[] = []
  log: DebugEvent[] = []
  debug: DebugFlags = { hitboxes: false, labels: false, log: false }
  stats: BattleStats = {
    frames: 0,
    damageDealt: 0,
    damageTaken: 0,
    hitsDealt: 0,
    hitsTaken: 0,
    crits: 0,
    kills: 0,
    enemiesTotal: 0,
    maxHit: 0,
    epSpent: 0,
    extra: {},
  }
  /** debug rects drawn for a few frames (attack boxes) */
  flashes: { r: Rect; color: string; until: number }[] = []
  private nextId = 1

  rng: Rng
  arena: ArenaRuntime
  constructor(rng: Rng, arena: ArenaRuntime) {
    this.rng = rng
    this.arena = arena}

  add<T extends Entity>(e: T): T {
    e.id = this.nextId++
    this.entities.push(e)
    return e
  }

  /** Advance one game frame. Objects are updated last-to-first; objects added this frame wait until the next. */
  step(input: InputFrame) {
    this.frame++
    this.stats.frames++
    this.rng.frame = this.frame
    this.input = input
    const n = this.entities.length
    for (let i = n - 1; i >= 0; i--) {
      const e = this.entities[i]
      if (!e.removed) e.update(this)
    }
    this.entities = this.entities.filter((e) => !e.removed)
    for (const p of this.popups) {
      p.age++
      p.y += p.vy
    }
    this.popups = this.popups.filter((p) => p.age < p.life)
    this.flashes = this.flashes.filter((f) => f.until >= this.frame)
  }

  logEvent(kind: DebugEvent['kind'], text: string) {
    this.log.push({ frame: this.frame, kind, text })
    if (this.log.length > 400) this.log.splice(0, this.log.length - 400)
  }

  popup(text: string, x: number, y: number, color = '#fff', life = 40) {
    this.popups.push({ text, x, y, age: 0, life, color, vy: -0.5 })
  }

  debugRect(r: Rect, color: string, frames = 2) {
    if (this.debug.hitboxes) this.flashes.push({ r, color, until: this.frame + frames })
  }

  /** Centre the logical GBA camera on a point, clamped to the background image. */
  centerCamera(x: number, y: number, margin = 40) {
    const [mw, mh] = this.arena.size
    const b = this.arena.bounds
    let cx = Math.round(x - GBA_W / 2)
    let cy = Math.round(y - GBA_H / 2)
    // keep the view on the arena (plus a margin of scenery) and inside the map image
    const lo = (a: number, b2: number, view: number) => [Math.max(0, a - margin), Math.max(0, b2 + margin - view)]
    const [xa, xb] = lo(b.x0, b.x1, GBA_W)
    const [ya, yb] = lo(b.y0 - 24, b.y1, GBA_H)
    cx = xb >= xa ? Math.max(xa, Math.min(cx, xb)) : Math.round((b.x0 + b.x1) / 2 - GBA_W / 2)
    cy = yb >= ya ? Math.max(ya, Math.min(cy, yb)) : Math.round((b.y0 + b.y1) / 2 - GBA_H / 2)
    if (mw > 0) cx = Math.max(0, Math.min(cx, Math.max(0, mw - GBA_W)))
    if (mh > 0) cy = Math.max(0, Math.min(cy, Math.max(0, mh - GBA_H)))
    this.camera = { x0: cx, y0: cy, x1: cx + GBA_W, y1: cy + GBA_H }
  }

  draw(r: Renderer) {
    r.followCamera(this.camera)
    // clamp the (possibly wider) view to the map so we don't show void when the map is large enough
    const [mw, mh] = this.arena.size
    if (mw >= r.viewW) r.camX = Math.max(0, Math.min(r.camX, mw - r.viewW))
    if (mh >= r.viewH) r.camY = Math.max(0, Math.min(r.camY, mh - r.viewH))
    r.beginFrame('#000')
    const bg = imageNow(this.arena.image)
    if (bg) r.ctx.drawImage(bg, -r.camX, -r.camY)
    const sorted = this.entities
      .filter((e) => !e.removed)
      .sort((a, b) => a.layer - b.layer || a.depth() - b.depth() || a.id - b.id)
    for (const e of sorted) e.draw(r, this)
    for (const p of this.popups) {
      const w = textWidth(p.text)
      const blink = p.age > p.life - 10 && p.age % 2 === 1
      if (!blink) drawText(r.ctx, p.text, r.sx(p.x) - (w >> 1), r.sy(p.y), p.color)
    }
    if (this.debug.hitboxes) {
      r.strokeRect(this.arena.bounds, '#ff00ff')
      for (const e of sorted) e.drawDebug?.(r, this)
      for (const f of this.flashes) r.strokeRect(f.r, f.color)
    }
  }

  /** labels in display resolution, after the buffer was blitted */
  drawOverlay(r: Renderer) {
    if (!this.debug.labels) return
    for (const e of this.entities) {
      const lbl = e.debugLabel?.()
      if (!lbl) continue
      const anyE = e as unknown as { px?: number; py?: number }
      if (anyE.px == null || anyE.py == null) continue
      lbl.split('\n').forEach((line, i, arr) => r.overlayText(anyE.px!, anyE.py! - 30 - (arr.length - 1 - i) * 4, line, i === 0 ? '#ffd23f' : '#cfe8ff'))
    }
  }
}
