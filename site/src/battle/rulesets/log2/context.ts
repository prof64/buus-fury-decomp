// Per-battle context shared by LoG2 entities (data tables, sprite lookups, the player, bookkeeping helpers).

import type { SpriteSet } from '../../core/sprites'
import type { Popup, World } from '../../core/world'
import type { Actor } from './actor'
import { Effect } from './actor'
import type { Log2Data } from './data'

export interface PlayerLike extends Actor {
  hp: number
  maxHp: number
  ep: number
  maxEp: number
  dead: boolean
  level: number
  /** non-null while held by Cell's drain grab */
  grabbedBy: Actor | null
  setGrabbed(by: Actor | null, frames: number): void
  grab(w: World, by: Actor, frames: number): void
  gainExp(w: World, classExp: number): void
  /** damage applied "directly" (vtable+0x40: no hurt state) */
  applyDamageDirect(w: World, dmg: number, crit: boolean): void
}

export interface Log2Options {
  /** award kill exp and level up mid-battle like the game */
  exp: boolean
  /** show damage numbers (the game does) */
  popups: boolean
}

export class Log2Ctx {
  player: PlayerLike | null = null
  /** last popup per actor for the 8-frame accumulation (0x0800F832) */
  private lastPopup = new Map<Actor, { frame: number; p: Popup; value: number }>()

  readonly data: Log2Data
  readonly opts: Log2Options
  constructor(data: Log2Data, opts: Log2Options) {
    this.data = data
    this.opts = opts}

  fx(prefix: string): SpriteSet | undefined {
    for (const [id, s] of this.data.sprites.byId) if (id.startsWith(prefix)) return s
    return undefined
  }

  spawnFx(w: World, prefix: string, x: number, y: number, frames: number[], dur = 4, flip = false) {
    w.add(new Effect(this.fx(prefix), x, y, frames, dur, flip))
  }

  /** damage number above the hurt rect; hits within 8 frames on the same target accumulate (0x0800F832) */
  damagePopup(w: World, target: Actor, dmg: number, crit: boolean, toPlayer: boolean) {
    if (!this.opts.popups) return
    const hr = target.hurtRect()
    const x = (hr.x0 + hr.x1) >> 1
    const y = hr.y0 - 6
    const prev = this.lastPopup.get(target)
    const color = crit ? '#ffd23f' : toPlayer ? '#ff6060' : '#ffffff'
    if (prev && w.frame - prev.frame < 8) {
      const p = prev.p
      if (p.age < p.life && w.popups.includes(p)) {
        prev.value += dmg
        p.text = String(prev.value)
        p.age = 0
        p.color = crit ? '#ffd23f' : p.color
        prev.frame = w.frame
        return
      }
    }
    w.popup(String(dmg), x, y, color, 40)
    this.lastPopup.set(target, { frame: w.frame, p: w.popups[w.popups.length - 1], value: dmg })
  }
}
