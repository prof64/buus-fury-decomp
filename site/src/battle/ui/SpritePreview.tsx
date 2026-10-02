import { useEffect, useRef } from 'react'
import type { SpriteSet } from '../core/sprites'
import type { Dir } from '../core/geom'

/** Small animated sprite (setup screen): plays one animation of a sprite set, anchor centred near the bottom. */
export function SpritePreview({ sprite, anim = 'idle', dir = 0, size = 48, scale = 2, animate = true }: { sprite?: SpriteSet; anim?: string; dir?: number; size?: number; scale?: number; animate?: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const c = ref.current
    if (!c || !sprite) return
    const g = c.getContext('2d')!
    g.imageSmoothingEnabled = false
    let t = 0
    let raf = 0
    let alive = true
    sprite.load().then(() => {
      const a = sprite.pick(anim, 'idle', 'walk', 'default') ?? [...sprite.anims.values()][0]
      // fit: find the frame bounds to place the anchor
      const loop = () => {
        if (!alive) return
        g.clearRect(0, 0, size, size)
        sprite.draw(g, a, dir as Dir, (size >> 1) - 8, size - 12, t, { loop: true })
        t++
        if (animate) raf = requestAnimationFrame(loop)
      }
      loop()
    })
    return () => {
      alive = false
      cancelAnimationFrame(raf)
    }
  }, [sprite, anim, dir, size, animate])
  return <canvas ref={ref} width={size} height={size} style={{ width: size * scale, height: size * scale, imageRendering: 'pixelated' }} />
}

export function Portrait({ src, size = 48 }: { src?: string; size?: number }) {
  if (!src) return <div style={{ width: size, height: size }} className="bt-portrait empty" />
  return <img className="bt-portrait" src={import.meta.env.BASE_URL + src} width={size} height={size} alt="" />
}
