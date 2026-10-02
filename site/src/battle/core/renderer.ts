// Renders the game at its native resolution into an offscreen buffer, then blits it to the visible canvas with
// crisp integer scaling. The camera is a world-space rectangle; `widescreen` widens the visible view around the
// same logical camera (game logic keeps using the GBA-sized camera rect for culling).

import type { Rect } from './geom'

export const GBA_W = 240
export const GBA_H = 160

export interface ViewMode {
  widescreen: boolean
  /** fixed integer scale, or 0 = largest that fits */
  scale: number
}

export class Renderer {
  readonly buf: HTMLCanvasElement
  readonly ctx: CanvasRenderingContext2D
  viewW = GBA_W
  viewH = GBA_H
  scale = 3
  /** top-left of the view in world pixels */
  camX = 0
  camY = 0
  /** overlay context in display pixels (debug text), valid between endFrame() calls */
  overlay: CanvasRenderingContext2D

  readonly canvas: HTMLCanvasElement
  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas
    this.buf = document.createElement('canvas')
    this.ctx = this.buf.getContext('2d', { alpha: false })!
    this.overlay = canvas.getContext('2d')!
    this.setView(GBA_W, GBA_H)
  }

  setView(w: number, h: number) {
    this.viewW = w
    this.viewH = h
    this.buf.width = w
    this.buf.height = h
    this.ctx.imageSmoothingEnabled = false
  }

  /** Pick view size + integer scale for a container size. */
  fit(containerW: number, containerH: number, mode: ViewMode) {
    let w = GBA_W
    let h = GBA_H
    if (mode.widescreen) {
      // widen to the container aspect, keeping at least the GBA height scaled by an integer
      const s = Math.max(1, Math.floor(containerH / (GBA_H + 64)))
      h = Math.min(Math.floor(containerH / s), GBA_H + 96)
      w = Math.min(Math.floor(containerW / s), 480)
      w = Math.max(w, GBA_W)
      h = Math.max(h, GBA_H)
    }
    const fitScale = Math.max(1, Math.floor(Math.min(containerW / w, containerH / h)))
    this.scale = mode.scale > 0 ? mode.scale : fitScale
    if (w !== this.viewW || h !== this.viewH) this.setView(w, h)
    const cw = w * this.scale
    const ch = h * this.scale
    if (this.canvas.width !== cw || this.canvas.height !== ch) {
      this.canvas.width = cw
      this.canvas.height = ch
    }
  }

  /** Centre the view on the logical GBA camera rect. */
  followCamera(cam: Rect) {
    const cx = (cam.x0 + cam.x1) / 2
    const cy = (cam.y0 + cam.y1) / 2
    this.camX = Math.round(cx - this.viewW / 2)
    this.camY = Math.round(cy - this.viewH / 2)
  }

  beginFrame(bg = '#000') {
    const c = this.ctx
    c.setTransform(1, 0, 0, 1, 0, 0)
    c.globalAlpha = 1
    c.fillStyle = bg
    c.fillRect(0, 0, this.viewW, this.viewH)
  }

  /** world px -> buffer px */
  sx(x: number) {
    return Math.round(x) - this.camX
  }
  sy(y: number) {
    return Math.round(y) - this.camY
  }

  endFrame() {
    const o = this.overlay
    o.setTransform(1, 0, 0, 1, 0, 0)
    o.imageSmoothingEnabled = false
    o.drawImage(this.buf, 0, 0, this.viewW, this.viewH, 0, 0, this.canvas.width, this.canvas.height)
  }

  /** world rect outline in the buffer (debug) */
  strokeRect(r: Rect, color: string) {
    const c = this.ctx
    c.strokeStyle = color
    c.lineWidth = 1
    c.strokeRect(this.sx(r.x0) + 0.5, this.sy(r.y0) + 0.5, r.x1 - r.x0 - 1, r.y1 - r.y0 - 1)
  }

  /** text in display pixels at a world position (debug overlay, drawn after endFrame) */
  overlayText(x: number, y: number, text: string, color = '#fff') {
    const o = this.overlay
    const px = (this.sx(x) + 0.5) * this.scale
    const py = this.sy(y) * this.scale
    o.font = `${Math.max(10, Math.min(13, this.scale * 4))}px ui-monospace, Menlo, monospace`
    o.textAlign = 'center'
    o.textBaseline = 'bottom'
    o.lineWidth = 3
    o.strokeStyle = 'rgba(0,0,0,0.85)'
    o.strokeText(text, px, py)
    o.fillStyle = color
    o.fillText(text, px, py)
  }
}
