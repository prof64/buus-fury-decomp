// LoG1 enemy entity (state table 0x081667F0, handler 0x08005604). The brain is the char + the global AI in
// game.ts; the entity only mirrors the char (walking update 0x08005E06), plays Melee (16 frames, 0x08002C18),
// Ki (0x08005E38/0x08005E4C/0x08005E62) and Death (64 frames, 0x0800293C), and resolves the player's punch (msg 0)
// and projectiles (msg 1) against its per-facing hurtbox.

import type { Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { SpriteSet } from '../../core/sprites'
import type { World } from '../../core/world'
import { inRect, Log1Entity, overlapStrict, type HitRecord } from './entity'
import { Char, CS, type Log1Game } from './game'
import type { Player } from './player'

export class Enemy extends Log1Entity {
  kind = 'enemy'
  char: Char
  hurt: number[][]
  label: string
  /** the sprite ROM label ("vegeta1" -> vegeta's ki blast sprite) */
  spriteName: string
  constructor(g: Log1Game, char: Char, sprite: SpriteSet | undefined, spriteName: string, hurt: number[][], label: string) {
    super(g)
    this.char = char
    this.sprite = sprite
    this.spriteName = spriteName
    this.hurt = hurt
    this.label = label
    char.ent = this
    // foot box = sprite definition +0/+2 (e.g. 32x8 for the tyrannosaurs), copied to the char bbox +0x2A/+0x2B
    const tpl = char.tpl?.sprites.find((s) => s.name === spriteName)
    if (tpl) char.bbox = [0, 0, tpl.foot[0], tpl.foot[1]]
    this.x = char.x
    this.y = char.y
  }
  hurtRect(): Rect {
    const h = this.hurt[this.facing]
    return { x0: this.x + h[0], y0: this.y + h[1], x1: this.x + h[2], y1: this.y + h[3] }
  }
  get defeated() {
    return !this.char.alive
  }

  /** state enter callbacks */
  enter(n: number) {
    const c = this.char
    // enter callbacks that reset the tick: melee 0x08002C04, ki 0x08003B28, flying 0x08002C78, death 0x08002930
    if (n === 2 || n === 3 || n === 4 || n === 5 || n === 14) this.tick = 0
    if (n === 0) {
      // 0x08005DE0: leaving an attack/hurt state starts the cooldown
      if (c.state > 3) {
        c.stun = c.state === CS.hurt ? 20 : 45
        c.state = 0
      }
    } else if (n === 3 || n === 4 || n === 5 || n === 14) {
      this.facing = c.face
      if (n === 5) this.t24 = 0
    }
  }

  update(_w: World) {
    const c = this.char
    switch (this.st) {
      case 1:
      case 14:
        // 0x08005E06: entity follows the char
        this.x = c.x
        this.y = c.y
        this.facing = c.face
        break
      case 5:
        if (this.t24 === 2 && this.tick >= 16) this.setState(0)
        break
      default:
        break
    }
  }

  animTick() {
    const c = this.char
    if (this.st === 2) {
      // Death 0x0800293C: 64 frames, then the body stays
      if (this.tick < 64) this.tick++
      return
    }
    if (!c.alive) return
    switch (this.st) {
      case 3:
      case 4:
        if (this.tick < 16) this.tick++
        else this.setState(0)
        return
      case 5:
        if (this.t24 === 0) return
        if (this.tick < 16) this.tick++
        return
      default:
        this.tick = (this.tick + 1) & 0xff
    }
  }

  /** set_char_state's set_state: only when the sprite has the animation */
  setState(n: number) {
    super.setState(n)
  }

  message(_w: World, msg: number, arg: unknown): boolean {
    const c = this.char
    if (msg === 0) {
      if (!c.alive) return false
      const p = arg as Player
      const [x, y] = p.punchPoint()
      if (!inRect(x, y, this.hurtRect())) return false
      this.facing = p.facing ^ 1
      p.punch(c)
      return true
    }
    if (msg === 1) {
      if (!c.alive) return false
      const rec = arg as HitRecord
      if (rec.enemyOwned) return false
      if (!overlapStrict(this.hurtRect(), rec.rect)) return false
      if (rec.once && c.inv > 0) return true
      this.g.hit(rec.owner, c, 1, rec.dmg)
      return true
    }
    return false
  }

  draw(r: Renderer, w: World) {
    if (!this.sprite) return
    if (this.st === 2) {
      const a = this.sprite.anim('death')
      if (!a) return
      this.sprite.draw(r.ctx, a, this.facing as 0, r.sx(this.x), r.sy(this.y), Math.min(this.tick, a.length(this.facing as 0) - 1))
      return
    }
    super.draw(r, w)
  }
  drawDebug(r: Renderer) {
    r.strokeRect(this.hurtRect(), this.char.inv > 0 || this.char.stun > 0 ? '#357' : '#3cf')
    r.strokeRect(this.char.rect(), '#0f0')
    const c = this.char
    for (let i = c.pathIdx; i < c.pathLen; i++) {
      const [tx, ty] = c.path[i]
      r.strokeRect({ x0: tx * 8, y0: ty * 8, x1: tx * 8 + 8, y1: ty * 8 + 8 }, '#f80')
    }
  }
  debugLabel() {
    const c = this.char
    const act = ['awake', 'path', 'asleep', 'roam'][c.act] ?? c.act
    return `${this.label} ${['idle', 'walk', '', 'fly', 'ki', 'melee', 'hurt', 'dead'][c.state]}\nHP ${c.hp} ${act}${c.stun ? ' stun' + c.stun : ''}${c.inv ? ' inv' + c.inv : ''}${c.aggro ? ' aggro' : ''}`
  }
  get px() {
    return this.x
  }
  get py() {
    return this.y
  }
}
