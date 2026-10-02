// A tiny software Canvas 2D (the subset the simulator's draw code uses) + fake `Image` / `document` globals, so the
// real `World.draw` / `drawHud` / `Renderer` render headlessly into RGBA buffers that pngjs encodes. No native deps.
//
// Supported: setTransform / translate / scale (axis-aligned; rotation ignored), save / restore, globalAlpha,
// globalCompositeOperation source-over | lighter | source-atop, fillStyle (#rgb, #rrggbb, #rrggbbaa, rgb(a)()),
// fillRect / clearRect / strokeRect, beginPath + rect / arc + fill / clip (clip = bounding box), drawImage (3/5/9 args,
// nearest neighbour, negative scale = mirror). Text calls are no-ops (the game's own pixel font uses fillRect).
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { PNG } from 'pngjs'

type M = [number, number, number, number, number, number] // a b c d e f

function parseColor(s0: unknown): [number, number, number, number] {
  if (typeof s0 !== "string") return [0, 0, 0, 255]
  let s: string = s0
  s = s.trim()
  if (s[0] === '#') {
    const h = s.slice(1)
    if (h.length === 3 || h.length === 4) {
      const v = [...h].map((c) => parseInt(c + c, 16))
      return [v[0], v[1], v[2], v[3] ?? 255]
    }
    const v = [0, 2, 4, 6].map((i) => parseInt(h.slice(i, i + 2), 16))
    return [v[0], v[1], v[2], h.length >= 8 ? v[3] : 255]
  }
  const m = s.match(/rgba?\(([^)]+)\)/)
  if (m) {
    const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number)
    return [p[0], p[1], p[2], p.length > 3 ? Math.round((p[3] <= 1 ? p[3] : p[3] / 255) * 255) : 255]
  }
  const named: Record<string, [number, number, number, number]> = { black: [0, 0, 0, 255], white: [255, 255, 255, 255], transparent: [0, 0, 0, 0], red: [255, 0, 0, 255] }
  return named[s] ?? [255, 0, 255, 255]
}

interface Shape {
  kind: 'rect' | 'arc'
  x: number
  y: number
  w: number
  h: number
  r?: number
}

export class SoftCanvas {
  private _w = 0
  private _h = 0
  data = new Uint8ClampedArray(0)
  private ctx2d: SoftContext | null = null
  constructor(w = 300, h = 150) {
    this.width = w
    this.height = h
  }
  get width() {
    return this._w
  }
  set width(v: number) {
    this._w = Math.max(0, v | 0)
    this.data = new Uint8ClampedArray(this._w * this._h * 4)
  }
  get height() {
    return this._h
  }
  set height(v: number) {
    this._h = Math.max(0, v | 0)
    this.data = new Uint8ClampedArray(this._w * this._h * 4)
  }
  getContext(_kind?: string, _opts?: unknown): SoftContext {
    if (!this.ctx2d) this.ctx2d = new SoftContext(this)
    return this.ctx2d
  }
  toPNG(): Buffer {
    const png = new PNG({ width: this.width, height: this.height })
    png.data = Buffer.from(this.data.buffer, this.data.byteOffset, this.data.byteLength)
    return PNG.sync.write(png)
  }
}

interface State {
  m: M
  alpha: number
  op: string
  fill: string
  stroke: string
  clip: [number, number, number, number] | null
}

export class SoftContext {
  readonly canvas: SoftCanvas
  private st: State = { m: [1, 0, 0, 1, 0, 0], alpha: 1, op: 'source-over', fill: '#000', stroke: '#000', clip: null }
  private stack: State[] = []
  private path: Shape[] = []
  imageSmoothingEnabled = false
  lineWidth = 1
  font = ''
  textAlign = 'left'
  textBaseline = 'alphabetic'
  constructor(c: SoftCanvas) {
    this.canvas = c
  }
  get globalAlpha() {
    return this.st.alpha
  }
  set globalAlpha(v: number) {
    this.st.alpha = v
  }
  get globalCompositeOperation() {
    return this.st.op
  }
  set globalCompositeOperation(v: string) {
    this.st.op = v
  }
  get fillStyle() {
    return this.st.fill
  }
  set fillStyle(v: string) {
    this.st.fill = typeof v === 'string' ? v : '#f0f'
  }
  get strokeStyle() {
    return this.st.stroke
  }
  set strokeStyle(v: string) {
    this.st.stroke = typeof v === 'string' ? v : '#f0f'
  }
  save() {
    this.stack.push({ ...this.st, m: [...this.st.m] as M, clip: this.st.clip ? [...this.st.clip] as State['clip'] : null })
  }
  restore() {
    const s = this.stack.pop()
    if (s) this.st = s
  }
  setTransform(a = 1, b = 0, c = 0, d = 1, e = 0, f = 0) {
    this.st.m = [a, b, c, d, e, f]
  }
  resetTransform() {
    this.setTransform()
  }
  translate(x: number, y: number) {
    const m = this.st.m
    m[4] += m[0] * x + m[2] * y
    m[5] += m[1] * x + m[3] * y
  }
  scale(sx: number, sy: number) {
    const m = this.st.m
    m[0] *= sx
    m[1] *= sx
    m[2] *= sy
    m[3] *= sy
  }
  rotate(_a: number) {
    /* not supported (unused by the sim) */
  }
  private tx(x: number, y: number): [number, number] {
    const m = this.st.m
    return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
  }
  /** device-space rect of a user-space rect (axis-aligned transforms) */
  private devRect(x: number, y: number, w: number, h: number): [number, number, number, number] {
    const [ax, ay] = this.tx(x, y)
    const [bx, by] = this.tx(x + w, y + h)
    return [Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by)]
  }
  private bounds(x0: number, y0: number, x1: number, y1: number): [number, number, number, number] {
    let a = Math.max(0, Math.round(x0))
    let b = Math.max(0, Math.round(y0))
    let c = Math.min(this.canvas.width, Math.round(x1))
    let d = Math.min(this.canvas.height, Math.round(y1))
    const cl = this.st.clip
    if (cl) {
      a = Math.max(a, cl[0])
      b = Math.max(b, cl[1])
      c = Math.min(c, cl[2])
      d = Math.min(d, cl[3])
    }
    return [a, b, c, d]
  }
  private blend(i: number, r: number, g: number, bl: number, a: number) {
    const D = this.canvas.data
    const op = this.st.op
    const sa = (a / 255) * this.st.alpha
    if (sa <= 0) return
    const da = D[i + 3] / 255
    if (op === 'source-atop') {
      if (da === 0) return
      D[i] = r * sa + D[i] * (1 - sa)
      D[i + 1] = g * sa + D[i + 1] * (1 - sa)
      D[i + 2] = bl * sa + D[i + 2] * (1 - sa)
      return
    }
    if (op === 'lighter') {
      D[i] = D[i] + r * sa
      D[i + 1] = D[i + 1] + g * sa
      D[i + 2] = D[i + 2] + bl * sa
      D[i + 3] = Math.min(255, (da + sa) * 255)
      return
    }
    const oa = sa + da * (1 - sa)
    if (oa <= 0) return
    D[i] = (r * sa + D[i] * da * (1 - sa)) / oa
    D[i + 1] = (g * sa + D[i + 1] * da * (1 - sa)) / oa
    D[i + 2] = (bl * sa + D[i + 2] * da * (1 - sa)) / oa
    D[i + 3] = oa * 255
  }
  fillRect(x: number, y: number, w: number, h: number) {
    const [r, g, b, a] = parseColor(this.st.fill)
    const [x0, y0, x1, y1] = this.bounds(...this.devRect(x, y, w, h))
    const W = this.canvas.width
    for (let yy = y0; yy < y1; yy++) for (let xx = x0; xx < x1; xx++) this.blend((yy * W + xx) * 4, r, g, b, a)
  }
  clearRect(x: number, y: number, w: number, h: number) {
    const [x0, y0, x1, y1] = this.bounds(...this.devRect(x, y, w, h))
    const W = this.canvas.width
    for (let yy = y0; yy < y1; yy++) this.canvas.data.fill(0, (yy * W + x0) * 4, (yy * W + x1) * 4)
  }
  strokeRect(x: number, y: number, w: number, h: number) {
    const f = this.st.fill
    this.st.fill = this.st.stroke
    const lw = Math.max(1, this.lineWidth)
    this.fillRect(x - 0.5, y - 0.5, w + 1, lw)
    this.fillRect(x - 0.5, y + h - 0.5, w + 1, lw)
    this.fillRect(x - 0.5, y - 0.5, lw, h + 1)
    this.fillRect(x + w - 0.5, y - 0.5, lw, h + 1)
    this.st.fill = f
  }
  beginPath() {
    this.path = []
  }
  closePath() {}
  moveTo() {}
  lineTo() {}
  rect(x: number, y: number, w: number, h: number) {
    this.path.push({ kind: 'rect', x, y, w, h })
  }
  arc(x: number, y: number, r: number) {
    this.path.push({ kind: 'arc', x, y, w: 0, h: 0, r })
  }
  ellipse(x: number, y: number, rx: number) {
    this.arc(x, y, rx)
  }
  fill() {
    for (const s of this.path) {
      if (s.kind === 'rect') this.fillRect(s.x, s.y, s.w, s.h)
      else {
        const [cx, cy] = this.tx(s.x, s.y)
        const rr = (s.r ?? 0) * Math.abs(this.st.m[0])
        const [r, g, b, a] = parseColor(this.st.fill)
        const [x0, y0, x1, y1] = this.bounds(cx - rr, cy - rr, cx + rr + 1, cy + rr + 1)
        const W = this.canvas.width
        for (let yy = y0; yy < y1; yy++)
          for (let xx = x0; xx < x1; xx++) if ((xx + 0.5 - cx) ** 2 + (yy + 0.5 - cy) ** 2 <= rr * rr) this.blend((yy * W + xx) * 4, r, g, b, a)
      }
    }
  }
  stroke() {}
  clip() {
    let box: [number, number, number, number] | null = null
    for (const s of this.path) {
      const d = s.kind === 'rect' ? this.devRect(s.x, s.y, s.w, s.h) : this.devRect(s.x - (s.r ?? 0), s.y - (s.r ?? 0), 2 * (s.r ?? 0), 2 * (s.r ?? 0))
      box = box ? [Math.min(box[0], d[0]), Math.min(box[1], d[1]), Math.max(box[2], d[2]), Math.max(box[3], d[3])] : d
    }
    if (box) {
      const c = this.st.clip
      const r = box.map(Math.round) as [number, number, number, number]
      this.st.clip = c ? [Math.max(c[0], r[0]), Math.max(c[1], r[1]), Math.min(c[2], r[2]), Math.min(c[3], r[3])] : r
    }
  }
  fillText() {}
  strokeText() {}
  measureText(t: string) {
    return { width: t.length * 6 }
  }
  createLinearGradient() {
    return { addColorStop() {} }
  }
  createRadialGradient() {
    return { addColorStop() {} }
  }
  drawImage(img: { width: number; height: number; data?: Uint8ClampedArray | Buffer; canvas?: unknown } & Partial<SoftCanvas>, ...a: number[]) {
    const src = (img as unknown as { __rgba?: Uint8ClampedArray | Buffer }).__rgba ?? (img as SoftCanvas).data
    if (!src || !img.width || !img.height) return
    let sx = 0, sy = 0, sw = img.width, sh = img.height, dx: number, dy: number, dw: number, dh: number
    if (a.length === 2) [dx, dy, dw, dh] = [a[0], a[1], sw, sh]
    else if (a.length === 4) [dx, dy, dw, dh] = a as [number, number, number, number]
    else [sx, sy, sw, sh, dx, dy, dw, dh] = a as [number, number, number, number, number, number, number, number]
    if (sw <= 0 || sh <= 0 || !dw || !dh) return
    const m = this.st.m
    // device rect corners (a mirrored transform swaps them)
    const [ax, ay] = this.tx(dx, dy)
    const [bx, by] = this.tx(dx + dw, dy + dh)
    const flipX = bx < ax
    const flipY = by < ay
    const X0 = Math.min(ax, bx), X1 = Math.max(ax, bx), Y0 = Math.min(ay, by), Y1 = Math.max(ay, by)
    const [x0, y0, x1, y1] = this.bounds(X0, Y0, X1, Y1)
    const W = this.canvas.width
    const kx = sw / (X1 - X0)
    const ky = sh / (Y1 - Y0)
    void m
    for (let yy = y0; yy < y1; yy++) {
      let v = (yy + 0.5 - Y0) * ky
      if (flipY) v = sh - v
      const syi = Math.floor(sy + v)
      if (syi < 0 || syi >= img.height) continue
      for (let xx = x0; xx < x1; xx++) {
        let u = (xx + 0.5 - X0) * kx
        if (flipX) u = sw - u
        const sxi = Math.floor(sx + u)
        if (sxi < 0 || sxi >= img.width) continue
        const si = (syi * img.width + sxi) * 4
        const al = src[si + 3]
        if (al) this.blend((yy * W + xx) * 4, src[si], src[si + 1], src[si + 2], al)
      }
    }
  }
}

// ---- fake browser globals ---------------------------------------------------------------------------------------

let installed = false
/** Install `Image` (loads site/public/<src> synchronously with pngjs) and `document.createElement('canvas')`. Call once,
 *  only when screenshots are wanted: the sim itself never needs them. */
export function installFakeDom(publicDir: string) {
  if (installed) return
  installed = true
  class FakeImage {
    width = 0
    height = 0
    __rgba: Buffer | null = null
    onload: (() => void) | null = null
    onerror: ((e: unknown) => void) | null = null
    private _src = ''
    get src() {
      return this._src
    }
    set src(v: string) {
      this._src = v
      try {
        const p = resolve(publicDir, v.replace(/^\//, ''))
        const png = PNG.sync.read(readFileSync(p))
        this.width = png.width
        this.height = png.height
        this.__rgba = png.data
        this.onload?.()
      } catch (e) {
        this.onerror?.(e)
      }
    }
  }
  const g = globalThis as unknown as Record<string, unknown>
  g.Image = FakeImage
  g.document = { createElement: (t: string) => (t === 'canvas' ? new SoftCanvas() : {}) }
}
