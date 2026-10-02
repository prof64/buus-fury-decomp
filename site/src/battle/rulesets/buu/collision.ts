// Buu's Fury map collision (tools/battle/COLLISION.md). Every map has three 256 x 256 bit grids of 8x8 tiles at the
// map object 0x030019A4 (1 = passable):
//   +0xA8 walk   - inflated from map header +0x34; the player tests it (0x08012960 / 0x080097C2 / 0x080097EA)
//   +0xAC shot   - inflated from map header +0x38; projectiles explode on it (0x08009324 from each shot's update)
//   +0xB0 enemy  - a copy of the walk grid (0x080095C4) in which every "attached" enemy marks its own footprint
//                  blocked (0x0800F740 -> 0x08009930), so enemies block each other; enemies test it (0x0800F9EE)
// data/battle/buu_collision.json holds walk + shot per map (tools/battle/buu_collision.py).

import type { Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { Entity, World } from '../../core/world'

type J = Record<string, any> // eslint-disable-line @typescript-eslint/no-explicit-any

export interface CollisionGrid {
  /** tiles; everything outside w x h is blocked */
  w: number
  h: number
  /** 1 byte per tile, 1 = passable */
  walk: Uint8Array
  shot: Uint8Array
}

function b64(s: string): Uint8Array {
  const bin = atob(s)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

/** run lengths (LEB128) over w*h tiles, first run blocked */
function unrle(s: string, n: number): Uint8Array {
  const b = b64(s)
  const out = new Uint8Array(n)
  let i = 0
  let p = 0
  let cur = 0
  while (i < b.length) {
    let v = 0
    let sh = 0
    for (;;) {
      const c = b[i++]
      v += (c & 0x7f) * 2 ** sh
      sh += 7
      if (!(c & 0x80)) break
    }
    if (cur) out.fill(1, p, Math.min(n, p + v))
    p += v
    cur ^= 1
  }
  return out
}

export function parseCollision(json: J | undefined): Map<string, CollisionGrid> {
  const m = new Map<string, CollisionGrid>()
  if (!json?.maps) return m
  for (const [k, v] of Object.entries(json.maps as Record<string, J>)) {
    const n = v.w * v.h
    m.set(k, { w: v.w, h: v.h, walk: unrle(v.walk, n), shot: unrle(v.shot, n) })
  }
  return m
}

export type Layer = 'walk' | 'shot' | 'enemy'

/** the live grids of one battle (the enemy layer is a private copy that enemies mark) */
export class BuuCollision {
  readonly w: number
  readonly h: number
  readonly walk: Uint8Array
  readonly shot: Uint8Array
  readonly enemy: Uint8Array
  constructor(g: CollisionGrid) {
    this.w = g.w
    this.h = g.h
    this.walk = g.walk
    this.shot = g.shot
    this.enemy = g.walk.slice()
  }

  private grid(l: Layer) {
    return l === 'walk' ? this.walk : l === 'shot' ? this.shot : this.enemy
  }

  tile(l: Layer, tx: number, ty: number): boolean {
    if (tx < 0 || ty < 0 || tx >= this.w || ty >= this.h) return false
    return this.grid(l)[ty * this.w + tx] === 1
  }

  /** 0x08009324(box, grid): false if the box leaves 0..2048 or any 8x8 tile under [x0, x1) x [y0, y1) is blocked */
  test(l: Layer, x0: number, y0: number, x1: number, y1: number): boolean {
    if (x0 < 0 || y0 < 0 || x1 > 2048 || y1 > 2048) return false
    const g = this.grid(l)
    const tx1 = (x1 + 7) >> 3
    const ty1 = (y1 + 7) >> 3
    for (let ty = y0 >> 3; ty < ty1; ty++)
      for (let tx = x0 >> 3; tx < tx1; tx++) {
        if (tx >= this.w || ty >= this.h || g[ty * this.w + tx] !== 1) return false
      }
    return true
  }
  testRect(l: Layer, r: Rect): boolean {
    return this.test(l, r.x0, r.y0, r.x1, r.y1)
  }

  /** 0x08009930(box, grid, value): set (1) / clear (0) every tile under the box (clipped to the grid) */
  mark(l: Layer, x0: number, y0: number, x1: number, y1: number, value: 0 | 1) {
    const g = this.grid(l)
    const tx0 = Math.max(0, x0 >> 3)
    const ty0 = Math.max(0, y0 >> 3)
    const tx1 = Math.min(this.w, x1 >= 2048 ? 256 : (x1 + 7) >> 3)
    const ty1 = Math.min(this.h, y1 >= 2048 ? 256 : (y1 + 7) >> 3)
    for (let ty = ty0; ty < ty1; ty++) for (let tx = tx0; tx < tx1; tx++) g[ty * this.w + tx] = value
  }

  /** put the terrain (walk grid) value back on every tile under the box (enemy unmark) */
  restore(l: Layer, x0: number, y0: number, x1: number, y1: number) {
    const g = this.grid(l)
    const tx0 = Math.max(0, x0 >> 3)
    const ty0 = Math.max(0, y0 >> 3)
    const tx1 = Math.min(this.w, (x1 + 7) >> 3)
    const ty1 = Math.min(this.h, (y1 + 7) >> 3)
    for (let ty = ty0; ty < ty1; ty++)
      for (let tx = tx0; tx < tx1; tx++) {
        const i = ty * this.w + tx
        g[i] = this.walk[i]
      }
  }

  /** nearest position (8 px spiral around x, y) where a w x h footprint is free on `l`, inside `inside` and with its
   *  top-left tile in `region` (a tile mask, e.g. the arena's walkable component) */
  freeSpot(l: Layer, x: number, y: number, w: number, h: number, inside?: Rect, region?: Uint8Array | null): [number, number] {
    const ok = (px: number, py: number) =>
      this.test(l, px, py, px + w, py + h) &&
      (!inside || (px >= inside.x0 && py >= inside.y0 && px + w <= inside.x1 && py + h <= inside.y1)) &&
      (!region || region[(py >> 3) * this.w + (px >> 3)] === 1)
    if (ok(x, y)) return [x, y]
    for (let r = 1; r < 64; r++)
      for (let dy = -r; dy <= r; dy++)
        for (let dx = -r; dx <= r; dx++) {
          if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue
          if (ok(x + dx * 8, y + dy * 8)) return [x + dx * 8, y + dy * 8]
        }
    return [x, y]
  }
}

/** 4-connected component of passable walk tiles around the tile under (x, y) px: tile mask + bounding box in px
 *  (null when (x, y) is not walkable) */
export function walkRegion(g: CollisionGrid, x: number, y: number): { mask: Uint8Array; bounds: Rect } | null {
  const sx = x >> 3
  const sy = y >> 3
  if (sx < 0 || sy < 0 || sx >= g.w || sy >= g.h || !g.walk[sy * g.w + sx]) return null
  const mask = new Uint8Array(g.w * g.h)
  const stack = [sy * g.w + sx]
  mask[stack[0]] = 1
  let x0 = sx
  let x1 = sx
  let y0 = sy
  let y1 = sy
  while (stack.length) {
    const i = stack.pop()!
    const tx = i % g.w
    const ty = (i / g.w) | 0
    x0 = Math.min(x0, tx)
    x1 = Math.max(x1, tx)
    y0 = Math.min(y0, ty)
    y1 = Math.max(y1, ty)
    const nb = [tx > 0 ? i - 1 : -1, tx < g.w - 1 ? i + 1 : -1, ty > 0 ? i - g.w : -1, ty < g.h - 1 ? i + g.w : -1]
    for (const j of nb) {
      if (j >= 0 && !mask[j] && g.walk[j]) {
        mask[j] = 1
        stack.push(j)
      }
    }
  }
  return { mask, bounds: { x0: x0 * 8, y0: y0 * 8, x1: (x1 + 1) * 8, y1: (y1 + 1) * 8 } }
}

/** hitbox view: the map collision under everything (magenta = walk wall, orange = enemy footprint marks, cyan = projectile
 *  wall only); nothing is drawn outside the hitbox mode */
export class CollisionOverlay implements Entity {
  id = 0
  kind = 'collision'
  removed = false
  layer = -100
  col: BuuCollision
  constructor(col: BuuCollision) {
    this.col = col
  }
  depth() {
    return -1e9
  }
  update() {}
  draw() {}
  drawDebug(r: Renderer, _w: World) {
    const c = this.col
    const g = r.ctx
    const tx0 = Math.max(0, r.camX >> 3)
    const ty0 = Math.max(0, r.camY >> 3)
    const tx1 = Math.min(c.w, ((r.camX + r.viewW) >> 3) + 1)
    const ty1 = Math.min(c.h, ((r.camY + r.viewH) >> 3) + 1)
    for (let ty = ty0; ty < ty1; ty++)
      for (let tx = tx0; tx < tx1; tx++) {
        const i = ty * c.w + tx
        const color = !c.walk[i] ? 'rgba(255,0,255,0.38)' : !c.enemy[i] ? 'rgba(255,160,0,0.5)' : !c.shot[i] ? 'rgba(0,200,255,0.38)' : ''
        if (!color) continue
        g.fillStyle = color
        g.fillRect(r.sx(tx * 8), r.sy(ty * 8), 8, 8)
      }
  }
}
