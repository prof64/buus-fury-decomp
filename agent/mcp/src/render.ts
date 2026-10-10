// Screenshots: the simulator's own draw code (World.draw + the Buu HUD) rendered through the real Renderer into the
// software canvas (canvas.ts). Optional labels mark enemies with their observation ids.
import { resolve } from 'node:path'
import { Renderer } from '../../../site/src/battle/core/renderer'
import { drawText } from '../../../site/src/battle/core/pixelfont'
import { installFakeDom, SoftCanvas } from './canvas'
import { repoRoot } from './load'
import type { Session } from './session'
import { footCenter, liveEnemies } from './observe'

export type View = 'gba' | 'wide' | 'arena'

export function screenshot(s: Session, view: View = 'wide', scale = 2, labels = true): Buffer {
  installFakeDom(resolve(repoRoot(), 'site/public'))
  const canvas = new SoftCanvas(1, 1)
  const r = new Renderer(canvas as unknown as HTMLCanvasElement)
  const w = s.w
  scale = Math.max(1, Math.min(4, Math.round(scale)))
  if (view === 'arena') {
    const v = w.viewRect() ?? { x0: 0, y0: 0, x1: 480, y1: 320 }
    r.fit((v.x1 - v.x0) * scale, (v.y1 - v.y0) * scale, { kind: 'pc', zoom: scale, widescreen: false, scale: 0 })
  } else if (view === 'wide') r.fit(400 * scale, 256 * scale, { kind: 'pc', zoom: scale, widescreen: false, scale: 0 })
  else r.fit(240 * scale, 160 * scale, { kind: 'gba', widescreen: false, scale })
  // twice: the first pass makes the image cache load every sheet it touches (synchronously, see canvas.ts)
  for (let pass = 0; pass < 2; pass++) {
    w.draw(r)
    if (labels) {
      for (const e of liveEnemies(s)) {
        if (e.part) continue
        const b = e.bodyBox()
        const [fx] = footCenter(e)
        const t = String(e.id)
        drawText(r.ctx, t, r.sx(fx) - t.length * 2, r.sy(b.y0) - 8, '#ffff60')
      }
    }
    r.beginHud()
    s.battle.drawHud(r, w)
    r.endHud()
    r.endFrame()
  }
  return canvas.toPNG()
}
