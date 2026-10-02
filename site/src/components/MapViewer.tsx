// Pan/zoom image viewer for (large) map renders. Drag to pan, wheel / buttons to zoom, pixelated scaling.
import { useEffect, useRef, useState } from 'react'

export function MapViewer({ src, w: w0, h: h0 }: { src: string; w?: number; h?: number }) {
  const box = useRef<HTMLDivElement>(null)
  const [dim, setDim] = useState({ w: w0 ?? 0, h: h0 ?? 0 })
  const { w, h } = dim
  const [view, setView] = useState({ z: 1, x: 0, y: 0 })
  const drag = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null)

  const fit = () => {
    const el = box.current
    if (!el || !w || !h) return
    const z = Math.min(el.clientWidth / w, el.clientHeight / h, 4)
    setView({ z, x: (el.clientWidth - w * z) / 2, y: (el.clientHeight - h * z) / 2 })
  }
  useEffect(fit, [src, w, h]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const el = box.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const r = el.getBoundingClientRect()
      const mx = e.clientX - r.left
      const my = e.clientY - r.top
      setView((v) => {
        const z = Math.max(0.1, Math.min(16, v.z * (e.deltaY < 0 ? 1.2 : 1 / 1.2)))
        return { z, x: mx - ((mx - v.x) * z) / v.z, y: my - ((my - v.y) * z) / v.z }
      })
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [])

  const zoomBy = (f: number) => {
    const el = box.current
    if (!el) return
    const mx = el.clientWidth / 2
    const my = el.clientHeight / 2
    setView((v) => {
      const z = Math.max(0.1, Math.min(16, v.z * f))
      return { z, x: mx - ((mx - v.x) * z) / v.z, y: my - ((my - v.y) * z) / v.z }
    })
  }

  return (
    <div className="mapviewer">
      <div className="mapviewer-tools">
        <button onClick={() => zoomBy(1.25)}>＋</button>
        <button onClick={() => zoomBy(0.8)}>－</button>
        <button onClick={() => setView((v) => ({ ...v, z: 1 }))}>1:1</button>
        <button onClick={fit}>Fit</button>
        <span className="muted mono">
          {w}×{h} px · {Math.round(view.z * 100)}%
        </span>
      </div>
      <div
        ref={box}
        className="mapviewer-box"
        onPointerDown={(e) => {
          ;(e.target as Element).setPointerCapture?.(e.pointerId)
          drag.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y }
        }}
        onPointerMove={(e) => {
          const d = drag.current
          if (d) setView((v) => ({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y }))
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
      >
        <img
          src={src}
          alt="map"
          draggable={false}
          onLoad={(e) => {
            const im = e.currentTarget
            if (im.naturalWidth !== w || im.naturalHeight !== h) setDim({ w: im.naturalWidth, h: im.naturalHeight })
          }}
          style={{ width: w * view.z, height: h * view.z, transform: `translate(${view.x}px, ${view.y}px)` }}
        />
      </div>
    </div>
  )
}
