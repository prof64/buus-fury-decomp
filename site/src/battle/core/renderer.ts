// Renders the game at its native pixel scale into an offscreen buffer, then blits it to the visible canvas with
// crisp integer scaling. The camera is a world-space rectangle (the game's 240x160 logical camera); the drawn view
// is centred on it and may be larger:
//   'gba'  240x160, the largest integer scale that fits (or a fixed one)
//   'wide' legacy widescreen: up to 480x256 around the same camera
//   'pc'   the view is the container size / an integer zoom, so a big window shows several GBA screens of the
//          arena at the same pixel size; zoom 0 = auto (the smallest zoom >= 2 whose view the arena's painted
//          area covers, so the view is never mostly black)
// Game logic keeps using the GBA-sized camera for culling / "off-screen" rules.
// The HUD is drawn into its own transparent buffer (beginHud / endHud) scaled by `hudScale`, so it stays anchored to
// the viewport corners at a readable size whatever the zoom.

import type { Rect } from './geom'

export const GBA_W = 240
export const GBA_H = 160

export type ViewKind = 'gba' | 'wide' | 'pc'

export interface ViewMode {
  /** default 'gba' ('wide' when the legacy `widescreen` flag is set) */
  kind?: ViewKind
  /** legacy flag = kind 'wide' */
  widescreen: boolean
  /** 'gba' / 'wide': fixed integer scale, or 0 = largest that fits */
  scale: number
  /** 'pc': integer zoom 1..6, 0 = auto */
  zoom?: number
}

export function viewKind(m: ViewMode): ViewKind {
  return m.kind ?? (m.widescreen ? 'wide' : 'gba')
}

/** auto zoom of the PC view: the smallest zoom >= 2 whose view is (almost: <= 8% black margin) covered by the
 *  arena's painted area (max 6) */
export function autoZoom(containerW: number, containerH: number, arena?: { w: number; h: number } | null): number {
  let z = 2
  if (arena && arena.w > 0 && arena.h > 0) {
    const black = (z: number) => {
      const vw = Math.floor(containerW / z)
      const vh = Math.floor(containerH / z)
      return 1 - Math.min(1, arena.w / vw) * Math.min(1, arena.h / vh)
    }
    while (z < 6 && black(z) > 0.08) z++
  }
  return z
}

export class Renderer {
  readonly buf: HTMLCanvasElement
  /** current drawing context: the world buffer, or the HUD buffer between beginHud() / endHud() */
  ctx: CanvasRenderingContext2D
  viewW = GBA_W
  viewH = GBA_H
  /** world pixels -> display pixels */
  scale = 3
  /** HUD pixels -> display pixels */
  hudScale = 3
  /** zoom picked by the last fit() ('pc' auto resolves to a number) */
  zoom = 0
  /** top-left of the view in world pixels */
  camX = 0
  camY = 0
  /** overlay context in display pixels (debug text), valid between endFrame() calls */
  overlay: CanvasRenderingContext2D

  readonly canvas: HTMLCanvasElement
  private readonly worldCtx: CanvasRenderingContext2D
  private readonly hud: HTMLCanvasElement
  private readonly hudCtx: CanvasRenderingContext2D
  private hudW = GBA_W
  private hudH = GBA_H
  private inHud = false
  private saved: [number, number] = [GBA_W, GBA_H]

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas
    this.buf = document.createElement('canvas')
    this.worldCtx = this.buf.getContext('2d', { alpha: false })!
    this.ctx = this.worldCtx
    this.hud = document.createElement('canvas')
    this.hudCtx = this.hud.getContext('2d')!
    this.overlay = canvas.getContext('2d')!
    this.setView(GBA_W, GBA_H)
  }

  setView(w: number, h: number) {
    this.viewW = w
    this.viewH = h
    this.buf.width = w
    this.buf.height = h
    this.worldCtx.imageSmoothingEnabled = false
  }

  private setHud(w: number, h: number) {
    this.hudW = w
    this.hudH = h
    if (this.hud.width !== w || this.hud.height !== h) {
      this.hud.width = w
      this.hud.height = h
    }
    this.hudCtx.imageSmoothingEnabled = false
  }

  /** Pick view size + integer scale for a container size. `arena` = painted size of the arena (auto zoom). */
  fit(containerW: number, containerH: number, mode: ViewMode, arena?: { w: number; h: number } | null) {
    containerW = Math.max(1, Math.floor(containerW))
    containerH = Math.max(1, Math.floor(containerH))
    const kind = viewKind(mode)
    let w = GBA_W
    let h = GBA_H
    let scale: number
    let hudScale: number
    if (kind === 'pc') {
      let z = mode.zoom && mode.zoom > 0 ? Math.floor(mode.zoom) : autoZoom(containerW, containerH, arena)
      // never smaller than a GBA screen when the container allows it
      const maxZ = Math.max(1, Math.floor(Math.min(containerW / GBA_W, containerH / GBA_H)))
      z = Math.max(1, Math.min(z, maxZ))
      w = Math.max(1, Math.floor(containerW / z))
      h = Math.max(1, Math.floor(containerH / z))
      scale = z
      this.zoom = z
      hudScale = Math.max(1, Math.min(maxZ, 3))
    } else {
      if (kind === 'wide') {
        // widen to the container aspect, keeping at least the GBA height scaled by an integer
        const s = Math.max(1, Math.floor(containerH / (GBA_H + 64)))
        h = Math.min(Math.floor(containerH / s), GBA_H + 96)
        w = Math.min(Math.floor(containerW / s), 480)
        w = Math.max(w, GBA_W)
        h = Math.max(h, GBA_H)
      }
      const fitScale = Math.max(1, Math.floor(Math.min(containerW / w, containerH / h)))
      scale = mode.scale > 0 ? mode.scale : fitScale
      hudScale = scale
      this.zoom = scale
    }
    this.scale = scale
    this.hudScale = hudScale
    if (w !== this.viewW || h !== this.viewH) this.setView(w, h)
    const cw = w * scale
    const ch = h * scale
    if (this.canvas.width !== cw || this.canvas.height !== ch) {
      this.canvas.width = cw
      this.canvas.height = ch
    }
    this.setHud(Math.ceil(cw / hudScale), Math.ceil(ch / hudScale))
  }

  /** Centre the view on the logical GBA camera rect. */
  followCamera(cam: Rect) {
    const cx = (cam.x0 + cam.x1) / 2
    const cy = (cam.y0 + cam.y1) / 2
    this.camX = Math.round(cx - this.viewW / 2)
    this.camY = Math.round(cy - this.viewH / 2)
  }

  beginFrame(bg = '#000') {
    const c = this.worldCtx
    c.setTransform(1, 0, 0, 1, 0, 0)
    c.globalAlpha = 1
    c.fillStyle = bg
    c.fillRect(0, 0, this.viewW, this.viewH)
    this.hudCtx.setTransform(1, 0, 0, 1, 0, 0)
    this.hudCtx.clearRect(0, 0, this.hudW, this.hudH)
  }

  /** Switch drawing (ctx, viewW, viewH) to the screen-space HUD layer. */
  beginHud() {
    if (this.inHud) return
    this.inHud = true
    this.saved = [this.viewW, this.viewH]
    this.ctx = this.hudCtx
    this.viewW = this.hudW
    this.viewH = this.hudH
  }
  endHud() {
    if (!this.inHud) return
    this.inHud = false
    this.ctx = this.worldCtx
    ;[this.viewW, this.viewH] = this.saved
  }

  /** world px -> buffer px */
  sx(x: number) {
    return Math.round(x) - this.camX
  }
  sy(y: number) {
    return Math.round(y) - this.camY
  }

  endFrame() {
    this.endHud()
    const o = this.overlay
    o.setTransform(1, 0, 0, 1, 0, 0)
    o.imageSmoothingEnabled = false
    o.drawImage(this.buf, 0, 0, this.viewW, this.viewH, 0, 0, this.viewW * this.scale, this.viewH * this.scale)
    o.drawImage(this.hud, 0, 0, this.hudW, this.hudH, 0, 0, this.hudW * this.hudScale, this.hudH * this.hudScale)
  }

  /** world rect outline in the buffer (debug) */
  strokeRect(r: Rect, color: string) {
    const c = this.worldCtx
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
