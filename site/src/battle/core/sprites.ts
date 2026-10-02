// Sprite sets driven by data/<game>/sprites.json (schema: tools/extract/GRAPHICS.md §7).
// A frame is drawn with its `anchor` pixel on the entity position; `flip` mirrors around the anchor.

import { imageNow, loadImage } from './assets'
import { DIR_NAMES, type Dir } from './geom'

export interface SpriteFrameJson {
  x: number
  y: number
  w: number
  h: number
  anchor: { x: number; y: number }
}
export interface AnimStepJson {
  frame: number
  duration_frames: number
  flip: boolean
  /** offset [dx, dy] of an attached OBJ (Buu halo) from the entity position */
  attach?: [number, number]
}
export interface AnimJson {
  raw_index: number
  loop: boolean
  timing_source?: string
  directions: Partial<Record<string, AnimStepJson[]>>
}
export interface SpriteEntityJson {
  id: string
  name: string
  category: string
  sprite_index?: number | null
  sheet: string
  frames: SpriteFrameJson[]
  animations: Record<string, AnimJson>
  hitboxes?: unknown
}

export interface AnimStep {
  frame: number
  dur: number
  flip: boolean
  attach?: [number, number]
}
export class Anim {
  readonly total: number[]
  readonly name: string
  readonly raw: number
  readonly loop: boolean
  /** steps per direction 0..3 (down, up, left, right); direction-less anims repeat "any" */
  readonly dirs: AnimStep[][]
  constructor(name: string, raw: number, loop: boolean, dirs: AnimStep[][]) {
    this.dirs = dirs
    this.name = name
    this.raw = raw
    this.loop = loop
    this.total = dirs.map((s) => s.reduce((a, b) => a + b.dur, 0))
  }
  steps(dir: Dir): AnimStep[] {
    return this.dirs[dir].length ? this.dirs[dir] : (this.dirs.find((d) => d.length) ?? [])
  }
  length(dir: Dir): number {
    return this.steps(dir).reduce((a, b) => a + b.dur, 0)
  }
  /** Step shown `t` frames after the animation started (loops or clamps on the last step). */
  stepAt(dir: Dir, t: number, loop = this.loop): AnimStep | null {
    const steps = this.steps(dir)
    if (!steps.length) return null
    const len = steps.reduce((a, b) => a + b.dur, 0) || 1
    let k = loop ? ((t % len) + len) % len : Math.min(Math.max(t, 0), len - 1)
    for (const s of steps) {
      if (k < s.dur) return s
      k -= s.dur
    }
    return steps[steps.length - 1]
  }
  /** Step by index (for code-driven animations that compute the frame number themselves). */
  stepIndex(dir: Dir, i: number): AnimStep | null {
    const steps = this.steps(dir)
    if (!steps.length) return null
    return steps[Math.min(Math.max(i, 0), steps.length - 1)]
  }
}

export interface DrawOpts {
  /** draw as a white silhouette (hit flash) */
  white?: boolean
  alpha?: number
  /** extra horizontal mirror of the whole sprite */
  mirror?: boolean
}

export class SpriteSet {
  readonly anims = new Map<string, Anim>()
  readonly byRaw = new Map<number, Anim>()
  private white: HTMLCanvasElement | null = null

  readonly json: SpriteEntityJson
  constructor(json: SpriteEntityJson) {
    this.json = json
    for (const [name, a] of Object.entries(json.animations)) {
      const any = a.directions.any
      const dirs = DIR_NAMES.map((d) => (a.directions[d] ?? any ?? []).map((s) => ({ frame: s.frame, dur: Math.max(1, s.duration_frames), flip: s.flip, attach: s.attach })))
      const anim = new Anim(name, a.raw_index, a.loop, dirs)
      this.anims.set(name, anim)
      if (!this.byRaw.has(a.raw_index)) this.byRaw.set(a.raw_index, anim)
    }
  }

  get id() {
    return this.json.id
  }
  load(): Promise<HTMLImageElement> {
    return loadImage(this.json.sheet)
  }
  anim(ref: string | number): Anim | undefined {
    return typeof ref === 'number' ? this.byRaw.get(ref) : this.anims.get(ref)
  }
  /** first animation that exists among the given names / raw indices */
  pick(...refs: (string | number)[]): Anim | undefined {
    for (const r of refs) {
      const a = this.anim(r)
      if (a) return a
    }
    return undefined
  }

  private whiteSheet(img: HTMLImageElement): HTMLCanvasElement {
    if (this.white) return this.white
    const c = document.createElement('canvas')
    c.width = img.width
    c.height = img.height
    const g = c.getContext('2d')!
    g.drawImage(img, 0, 0)
    g.globalCompositeOperation = 'source-atop'
    g.fillStyle = '#fff'
    g.fillRect(0, 0, c.width, c.height)
    this.white = c
    return c
  }

  /** Draw one frame index with its anchor at (x, y) (integer screen pixels). */
  drawFrame(ctx: CanvasRenderingContext2D, frameIndex: number, x: number, y: number, flip: boolean, opts: DrawOpts = {}) {
    const img = imageNow(this.json.sheet)
    const f = this.json.frames[frameIndex]
    if (!img || !f) return
    const src: CanvasImageSource = opts.white ? this.whiteSheet(img) : img
    const mirror = flip !== !!opts.mirror
    const a = opts.alpha ?? 1
    if (a !== 1) ctx.globalAlpha = a
    if (!mirror) {
      ctx.drawImage(src, f.x, f.y, f.w, f.h, x - f.anchor.x, y - f.anchor.y, f.w, f.h)
    } else {
      ctx.save()
      ctx.translate(x, 0)
      ctx.scale(-1, 1)
      ctx.drawImage(src, f.x, f.y, f.w, f.h, -f.anchor.x, y - f.anchor.y, f.w, f.h)
      ctx.restore()
    }
    if (a !== 1) ctx.globalAlpha = 1
  }

  /** Draw an animation step `t` frames in (or step index when `index` is given). */
  draw(ctx: CanvasRenderingContext2D, anim: Anim | undefined, dir: Dir, x: number, y: number, t: number, opts: DrawOpts & { index?: number; loop?: boolean } = {}) {
    if (!anim) return
    const s = opts.index != null ? anim.stepIndex(dir, opts.index) : anim.stepAt(dir, t, opts.loop)
    if (!s) return
    this.drawFrame(ctx, s.frame, x, y, s.flip, opts)
  }
}

/** Registry of sprite sets for one game, by entity id and by engine sprite index. */
export class SpriteBank {
  readonly byId = new Map<string, SpriteSet>()
  readonly byIndex = new Map<number, SpriteSet>()
  constructor(entities: SpriteEntityJson[]) {
    for (const e of entities) {
      const s = new SpriteSet(e)
      this.byId.set(e.id, s)
      if (e.sprite_index != null && !this.byIndex.has(e.sprite_index)) this.byIndex.set(e.sprite_index, s)
    }
  }
  get(id: string): SpriteSet | undefined {
    return this.byId.get(id)
  }
  index(i: number): SpriteSet | undefined {
    return this.byIndex.get(i)
  }
}
