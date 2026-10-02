// LoG1 battle items (data/log1/items.json, notes §7): map pickups taken with A (item handler 0x08005D04, pickup
// point 0x0800339A) and the healing items used from the pause menu (herbs 0x0800170C: +maxHP/4, Senzu 0x08001730:
// full heal, both refused at full HP). Feathers refill flight energy +5/+10/+20 up to the level's max (0x080032A2).

import type { Renderer } from '../../core/renderer'
import type { World } from '../../core/world'
import { inRect, Log1Entity } from './entity'
import type { Log1Game } from './game'
import type { Player } from './player'

export const HERBS = [11, 12, 13]
export const SENZU = 19
export const FEATHERS: Record<number, number> = { 31: 5, 32: 10, 33: 20 }
export const BATTLE_ITEMS = [...HERBS, SENZU, ...Object.keys(FEATHERS).map(Number)]

export class Pickup extends Log1Entity {
  kind = 'item'
  item: number
  w: number
  h: number
  constructor(g: Log1Game, item: number, x: number, y: number) {
    super(g)
    this.item = item
    const it = g.data.items.get(item)
    this.sprite = it?.sprite
    this.w = it?.w ?? 16
    this.h = it?.h ?? 16
    this.x = x
    this.y = y
    this.layer = -1
    // pickups block their rectangle on the ground layer until taken (0x08005DA6 / 0x08005D98)
    g.setPassable(this.rect(), 0, false)
  }
  rect() {
    return { x0: this.x, y0: this.y, x1: this.x + this.w, y1: this.y + this.h }
  }
  depth() {
    return this.y
  }
  update() {}
  message(w: World, msg: number, arg: unknown): boolean {
    if (msg !== 0) return false
    const p = arg as Player
    const [x, y] = p.pickupPoint()
    if (!inRect(x, y, { x0: this.x, y0: this.y, x1: this.x + this.w, y1: this.y + this.h })) return false
    const g = this.g
    if (HERBS.includes(this.item)) {
      if (g.herbs >= 5) return true
      g.herbs++
      w.logEvent('info', 'You receive some herbs!')
    } else if (this.item === SENZU) {
      if (g.senzu >= 3) return true
      g.senzu++
      w.logEvent('info', 'You receive a Senzu bean!')
    } else if (FEATHERS[this.item]) {
      g.flight = Math.min(g.maxFlight, g.flight + FEATHERS[this.item])
      w.logEvent('info', `Feather: flight energy ${g.flight}/${g.maxFlight}`)
    }
    this.removed = true
    g.setPassable(this.rect(), 0, true)
    return true
  }
  draw(r: Renderer) {
    const s = this.sprite
    const a = s?.anim('icon')
    if (!s || !a) return
    // item pictures are anchored on their centre
    s.draw(r.ctx, a, 0, r.sx(this.x + (this.w >> 1)), r.sy(this.y + (this.h >> 1)), 0)
  }
}

/** pause-menu item use (simulator: SELECT). Returns a message or null when nothing could be used. */
export function applyHealingItem(g: Log1Game): string | null {
  const p = g.player
  if (p.hp >= p.maxHp || !p.alive) return null
  if (g.herbs > 0) {
    g.herbs--
    p.hp = Math.min(p.maxHp, p.hp + (p.maxHp >> 2))
    return 'One quarter health restored!'
  }
  if (g.senzu > 0) {
    g.senzu--
    p.hp = p.maxHp
    return 'Full health restored!'
  }
  return null
}
