// Animated sprite player. Coordinates follow sprites.json: every frame PNG has an `anchor` pixel that sits
// on the entity position (0,0); body/hit rectangles are relative to that same position.
import { useEffect, useMemo, useRef, useState } from 'react'
import { assetUrl } from '../data'
import { bodyRect, type Anim, type AnimStep, type Rect, type SpriteEntity } from '../lib/sprites'

const imgCache = new Map<string, Promise<HTMLImageElement>>()
export function loadImage(url: string): Promise<HTMLImageElement> {
  let p = imgCache.get(url)
  if (!p) {
    p = new Promise((res, rej) => {
      const im = new Image()
      im.onload = () => res(im)
      im.onerror = rej
      im.src = url
    })
    imgCache.set(url, p)
  }
  return p
}

export function useImage(url: string | undefined) {
  const [img, setImg] = useState<HTMLImageElement>()
  useEffect(() => {
    let alive = true
    setImg(undefined)
    if (url) loadImage(url).then((i) => alive && setImg(i), () => {})
    return () => {
      alive = false
    }
  }, [url])
  return img
}

export interface Overlay {
  rect: Rect
  color: string
  label?: string
  dashed?: boolean
}

export function stepsFor(anim: Anim | undefined, dir: string): AnimStep[] {
  if (!anim) return []
  const d = anim.directions as Record<string, AnimStep[] | undefined>
  return d[dir] ?? d.any ?? Object.values(d).find((v) => v && v.length) ?? []
}

export function dirsOf(anim: Anim | undefined): string[] {
  if (!anim) return []
  return ['down', 'up', 'left', 'right', 'any'].filter((d) => (anim.directions as Record<string, unknown>)[d])
}

interface CanvasProps {
  entity: SpriteEntity
  steps: AnimStep[]
  step: number
  dir: string
  zoom: number
  showAnchor?: boolean
  showBody?: boolean
  showFrameBox?: boolean
  overlays?: Overlay[]
  /** extra points to include in the view (entity coordinates) */
  extent?: Rect
  checker?: boolean
}

/** Static drawing of one animation step, auto-framed to the union of all steps so the sprite doesn't jump. */
export function SpriteCanvas(p: CanvasProps) {
  const ref = useRef<HTMLCanvasElement>(null)
  const sheet = useImage(assetUrl(p.entity.sheet))
  const body = p.showBody ? bodyRect(p.entity, p.dir) : undefined
  const bounds = useMemo(() => {
    let x0 = -2,
      y0 = -2,
      x1 = 2,
      y1 = 2
    const add = (r: Rect) => {
      x0 = Math.min(x0, r.x0)
      y0 = Math.min(y0, r.y0)
      x1 = Math.max(x1, r.x1)
      y1 = Math.max(y1, r.y1)
    }
    for (const s of p.steps) {
      const f = p.entity.frames[s.frame]
      if (!f) continue
      if (s.flip) add({ x0: f.anchor.x - f.w, y0: -f.anchor.y, x1: f.anchor.x, y1: -f.anchor.y + f.h })
      else add({ x0: -f.anchor.x, y0: -f.anchor.y, x1: -f.anchor.x + f.w, y1: -f.anchor.y + f.h })
    }
    if (body) add(body)
    for (const o of p.overlays ?? []) add(o.rect)
    if (p.extent) add(p.extent)
    return { x0: x0 - 4, y0: y0 - 4, x1: x1 + 4, y1: y1 + 4 }
  }, [p.steps, p.entity, body, p.overlays, p.extent])

  const w = bounds.x1 - bounds.x0
  const h = bounds.y1 - bounds.y0
  useEffect(() => {
    const c = ref.current
    if (!c) return
    const dpr = window.devicePixelRatio || 1
    const z = p.zoom
    c.width = Math.round(w * z * dpr)
    c.height = Math.round(h * z * dpr)
    c.style.width = `${w * z}px`
    c.style.height = `${h * z}px`
    const ctx = c.getContext('2d')!
    ctx.setTransform(dpr * z, 0, 0, dpr * z, 0, 0)
    ctx.imageSmoothingEnabled = false
    ctx.clearRect(0, 0, w, h)
    if (p.checker) {
      for (let yy = 0; yy < h; yy += 8)
        for (let xx = 0; xx < w; xx += 8) {
          ctx.fillStyle = ((xx + yy) / 8) % 2 ? 'rgba(128,128,128,0.10)' : 'rgba(128,128,128,0.04)'
          ctx.fillRect(xx, yy, 8, 8)
        }
    }
    const ox = -bounds.x0
    const oy = -bounds.y0
    const st = p.steps[p.step]
    const f = st && p.entity.frames[st.frame]
    if (f && sheet) {
      ctx.save()
      ctx.translate(ox, oy)
      if (st.flip) ctx.scale(-1, 1)
      ctx.drawImage(sheet, f.x, f.y, f.w, f.h, -f.anchor.x, -f.anchor.y, f.w, f.h)
      ctx.restore()
      if (p.showFrameBox) {
        ctx.strokeStyle = 'rgba(160,160,160,0.7)'
        ctx.lineWidth = 1 / z
        const fx = st.flip ? f.anchor.x - f.w : -f.anchor.x
        ctx.strokeRect(ox + fx + 0.5 / z, oy - f.anchor.y + 0.5 / z, f.w - 1 / z, f.h - 1 / z)
      }
    }
    const lw = Math.max(1 / z, 0.5)
    const drawRect = (r: Rect, color: string, dashed?: boolean, fill = true) => {
      ctx.save()
      ctx.lineWidth = lw * 1.5
      ctx.strokeStyle = color
      if (dashed) ctx.setLineDash([3 / z * 2, 2 / z * 2])
      if (fill) {
        ctx.globalAlpha = 0.18
        ctx.fillStyle = color
        ctx.fillRect(ox + r.x0, oy + r.y0, r.x1 - r.x0, r.y1 - r.y0)
        ctx.globalAlpha = 1
      }
      ctx.strokeRect(ox + r.x0, oy + r.y0, r.x1 - r.x0, r.y1 - r.y0)
      ctx.restore()
    }
    if (body) drawRect(body, '#2a9df4', true)
    for (const o of p.overlays ?? []) drawRect(o.rect, o.color, o.dashed)
    if (p.showAnchor) {
      ctx.strokeStyle = '#ff4d6d'
      ctx.lineWidth = lw
      ctx.beginPath()
      ctx.moveTo(ox - 3, oy + 0.5 / z)
      ctx.lineTo(ox + 3, oy + 0.5 / z)
      ctx.moveTo(ox + 0.5 / z, oy - 3)
      ctx.lineTo(ox + 0.5 / z, oy + 3)
      ctx.stroke()
    }
  }, [sheet, p.step, p.steps, p.entity, p.zoom, p.showAnchor, p.showFrameBox, body, p.overlays, bounds, w, h, p.checker])

  return <canvas ref={ref} className="sprite-canvas" />
}

/** Drives a step index in game frames (60 Hz) using requestAnimationFrame. */
export function useAnimClock(steps: AnimStep[], playing: boolean, speed: number) {
  const [step, setStep] = useState(0)
  const st = useRef({ step: 0, tick: 0, last: 0 })
  useEffect(() => {
    st.current = { step: 0, tick: 0, last: 0 }
    setStep(0)
  }, [steps])
  useEffect(() => {
    if (!playing || steps.length === 0) return
    let raf = 0
    const loop = (t: number) => {
      const s = st.current
      if (!s.last) s.last = t
      s.tick += ((t - s.last) / 1000) * 60 * speed
      s.last = t
      let changed = false
      let guard = 0
      while (s.tick >= Math.max(1, steps[s.step]?.duration_frames ?? 8) && guard++ < 50) {
        s.tick -= Math.max(1, steps[s.step]?.duration_frames ?? 8)
        s.step = (s.step + 1) % steps.length
        changed = true
      }
      if (changed) setStep(s.step)
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    return () => {
      cancelAnimationFrame(raf)
      st.current.last = 0
    }
  }, [playing, speed, steps])
  const jump = (i: number) => {
    const n = steps.length || 1
    st.current.step = ((i % n) + n) % n
    st.current.tick = 0
    setStep(st.current.step)
  }
  return [step, jump] as const
}

interface PlayerProps {
  entity: SpriteEntity
  initialAnim?: string
  initialDir?: string
  overlays?: Overlay[] | ((dir: string) => Overlay[])
  zoom?: number
  compact?: boolean
  extent?: Rect | ((dir: string) => Rect | undefined)
  onDir?: (d: string) => void
  dir?: string
}

/** Full player: animation & direction pickers, play/pause/step, speed, zoom, overlays and a frame strip. */
export function SpritePlayer(p: PlayerProps) {
  const names = Object.keys(p.entity.animations ?? {})
  const [anim, setAnim] = useState(p.initialAnim && names.includes(p.initialAnim) ? p.initialAnim : names[0])
  const a = p.entity.animations?.[anim]
  const dirs = dirsOf(a)
  const [dirState, setDirState] = useState(p.initialDir ?? 'down')
  const dirWanted = p.dir ?? dirState
  const dir = dirs.includes(dirWanted) ? dirWanted : (dirs[0] ?? 'down')
  const setDir = (d: string) => {
    setDirState(d)
    p.onDir?.(d)
  }
  const steps = useMemo(() => stepsFor(a, dir), [a, dir])
  const [playing, setPlaying] = useState(true)
  const [speed, setSpeed] = useState(1)
  const [zoom, setZoom] = useState(p.zoom ?? 4)
  const [showAnchor, setShowAnchor] = useState(true)
  const [showBody, setShowBody] = useState(!p.compact)
  const [showBox, setShowBox] = useState(false)
  const [step, jump] = useAnimClock(steps, playing, speed)
  useEffect(() => {
    if (!names.includes(anim)) setAnim(names[0])
  }, [p.entity]) // eslint-disable-line react-hooks/exhaustive-deps
  const cur = steps[step]
  const frame = cur && p.entity.frames[cur.frame]
  const overlays = typeof p.overlays === 'function' ? p.overlays(dir) : p.overlays
  const extent = typeof p.extent === 'function' ? p.extent(dir) : p.extent

  return (
    <div className={'player' + (p.compact ? ' compact' : '')}>
      <div className="player-stage">
        <SpriteCanvas
          entity={p.entity}
          steps={steps}
          step={step}
          dir={dir}
          zoom={zoom}
          showAnchor={showAnchor}
          showBody={showBody}
          showFrameBox={showBox}
          overlays={overlays}
          extent={extent}
          checker
        />
      </div>
      <div className="player-controls">
        {names.length > 1 && (
          <label>
            <span>Animation</span>
            <select value={anim} onChange={(e) => setAnim(e.target.value)}>
              {names.map((n) => (
                <option key={n} value={n}>
                  {n} (#{p.entity.animations[n].raw_index})
                </option>
              ))}
            </select>
          </label>
        )}
        {dirs.length > 1 && (
          <div className="seg" role="group" aria-label="Direction">
            {dirs.map((d) => (
              <button key={d} className={d === dir ? 'on' : ''} onClick={() => setDir(d)} title={d}>
                {{ down: '↓', up: '↑', left: '←', right: '→', any: '•' }[d]}
              </button>
            ))}
          </div>
        )}
        <div className="seg">
          <button onClick={() => jump(step - 1)} title="Previous frame">
            ⏮
          </button>
          <button onClick={() => setPlaying(!playing)} title={playing ? 'Pause' : 'Play'} className="wide">
            {playing ? '❚❚' : '▶'}
          </button>
          <button onClick={() => jump(step + 1)} title="Next frame">
            ⏭
          </button>
        </div>
        <label>
          <span>Speed</span>
          <select value={speed} onChange={(e) => setSpeed(+e.target.value)}>
            {[0.125, 0.25, 0.5, 1, 2].map((s) => (
              <option key={s} value={s}>
                {s}×
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>Zoom</span>
          <select value={zoom} onChange={(e) => setZoom(+e.target.value)}>
            {[1, 2, 3, 4, 6, 8].map((s) => (
              <option key={s} value={s}>
                {s}×
              </option>
            ))}
          </select>
        </label>
        {!p.compact && (
          <div className="checks">
            <label>
              <input type="checkbox" checked={showAnchor} onChange={(e) => setShowAnchor(e.target.checked)} /> anchor
            </label>
            <label>
              <input type="checkbox" checked={showBody} onChange={(e) => setShowBody(e.target.checked)} /> body rect
            </label>
            <label>
              <input type="checkbox" checked={showBox} onChange={(e) => setShowBox(e.target.checked)} /> frame box
            </label>
          </div>
        )}
      </div>
      {cur && frame && (
        <div className="player-info mono">
          step {step + 1}/{steps.length} · frame #{cur.frame} · {cur.duration_frames} f{cur.flip ? ' · flipped' : ''} · {frame.w}×
          {frame.h} · anchor ({frame.anchor.x},{frame.anchor.y}){cur.rom ? ` · ${cur.rom}` : ''}
          {a?.timing_source && <span className="muted"> · timing: {a.timing_source}</span>}
        </div>
      )}
      {!p.compact && steps.length > 1 && (
        <div className="frame-strip">
          {steps.map((s, i) => {
            const f = p.entity.frames[s.frame]
            if (!f) return null
            return (
              <button
                key={i}
                className={'strip-cell' + (i === step ? ' on' : '')}
                onClick={() => {
                  setPlaying(false)
                  jump(i)
                }}
                title={`frame ${s.frame}, ${s.duration_frames} f`}
              >
                <img
                  src={assetUrl(f.png)}
                  alt=""
                  style={{ width: f.w * 2, height: f.h * 2, transform: s.flip ? 'scaleX(-1)' : undefined }}
                />
                <span>{s.duration_frames}f</span>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
