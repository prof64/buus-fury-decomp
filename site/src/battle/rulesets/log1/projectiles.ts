// LoG1 ki moves as projectile entities (re/log1/notes.md §3.2, §9, data/log1/moves.json):
//   KiBlast   player (state table 0x081C7B14) and enemy (0x081667B4): charge 0x0800B436, launch 0x0800B554,
//             player flight 0x0800B5A2 (lifetime 10 + ki spent), enemy flight 0x0800BF4E (until off camera)
//   KameBeam  0x081C7AA8: charge 0x0800B716, beam 0x0800BCA0/0x0800BCD8 (grows 3 px/frame, hits everyone once),
//             retract 0x0800BDB4/0x0800BDC0
//   SolarFlare 0x085443A0: charge 0x0800B66E, release 0x0800B692 (freeze 0x080087A0), 32-frame flash

import { BTN } from '../../core/input'
import type { Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { World } from '../../core/world'
import { chargeCost, kiBlastDamage, solarFlareDuration } from './damage'
import { Log1Entity, sendAll, sendFirst, type HitRecord } from './entity'
import type { Log1Game } from './game'
import type { Enemy } from './enemy'
import type { Player } from './player'

type Owner = Player | Enemy

/** spawn offsets 0x08011E50 {dx,dy} by facing (down, up, left, right) */
const SPAWN: [number, number][] = [
  [-4, -11],
  [6, -12],
  [10, -11],
  [-6, -11],
]
/** ki blast launch 0x08011E68 {dx,dy,vx,vy} */
const LAUNCH: [number, number, number, number][] = [
  [0, 8, 0, 3],
  [0, -8, 0, -3],
  [-8, -2, -3, 0],
  [12, -2, 3, 0],
]
/** Kamehameha tables 0x08011E88: grow {dx,dy,dw,dh}, +0x20 retract, +0x40 wall retract, +0x60 start offset */
const K_GROW: [number, number, number, number][] = [
  [0, 0, 0, 3],
  [0, -3, 0, 3],
  [-3, 0, 3, 0],
  [0, 0, 3, 0],
]
const K_RETRACT: [number, number, number, number][] = [
  [0, 3, 0, 0],
  [0, -3, 0, 0],
  [-3, 0, 0, 0],
  [3, 0, 0, 0],
]
const K_WALL: [number, number, number, number][] = [
  [0, 3, 0, -3],
  [0, 0, 0, -3],
  [0, 0, -3, 0],
  [3, 0, -3, 0],
]
const K_START: [number, number][] = [
  [8, 8],
  [0, -8],
  [-20, 1],
  [24, 1],
]

abstract class Proj extends Log1Entity {
  layer = 1
  owner: Owner
  w = 6
  h = 6
  /** +0x24 charge / lifetime */
  c = 0
  /** +0x2C damage / power */
  dmg = 0
  enemyOwned = false
  constructor(g: Log1Game, owner: Owner) {
    super(g)
    this.owner = owner
    this.facing = owner.facing
  }
  rect(): Rect {
    return { x0: this.x, y0: this.y, x1: this.x + this.w, y1: this.y + this.h }
  }
  /** B3FC: place at owner + spawn offset (facing follows the owner) */
  protected follow() {
    this.facing = this.owner.facing
    const o = SPAWN[this.facing]
    this.x = this.owner.x + o[0]
    this.y = this.owner.y + o[1]
  }
  /** 0x0800B436: charge `cost` every 8th frame while the owner stays in its Ki state; else the projectile dies */
  protected charge(cost: number): boolean {
    this.follow()
    const o = this.owner
    if (!o.char.alive || o.st !== 5 || o.removed) {
      this.removed = true
      return false
    }
    if ((this.g.fc & 7) !== 0) return true
    if (o.char.ki < cost) return true
    o.char.ki -= cost
    this.c += cost
    if (o.kind === 'player') this.g.world.stats.epSpent += cost
    return true
  }
  depth() {
    return this.y + this.h
  }
  drawDebug(r: Renderer) {
    r.strokeRect(this.rect(), this.enemyOwned ? '#f44' : '#ff0')
  }
}

/** Ki blast (player move 0 / enemy ki attack). States: 0 charge, 1 fly, 2 vanish. */
export class KiBlast extends Proj {
  kind = 'ki_blast'
  vx = 0
  vy = 0
  size: 'small' | 'medium' | 'large' = 'small'
  /** enemy: power to reach before launching */
  power: number
  constructor(g: Log1Game, owner: Owner, enemy: boolean, power: number) {
    super(g, owner)
    this.enemyOwned = false
    this.power = power
    this.dmg = power
    if (enemy) this.size = 'large'
    this.w = this.h = enemy ? 16 : 6
    this.sprite = enemy && (owner as Enemy).spriteName?.startsWith('v') ? g.data.byLabel.get('vegeta_ki_blast') : g.data.byLabel.get(enemy ? 'ki_blast' : 'ki_blast_small')
    // enter 0x0800B4EC: spawn + charge 10
    this.follow()
    this.c = 10
    this.st = 0
    this.isEnemy = enemy
  }
  isEnemy: boolean

  update(w: World) {
    const g = this.g
    if (this.st === 0) {
      if (this.isEnemy) {
        // 0x0800BF30
        if (this.c < this.power) this.charge((this.power + 7) >> 3)
        else {
          this.enemyOwned = true
          this.launch()
        }
      } else if (w.input.held & BTN.B) {
        this.charge(chargeCost(this.owner.char.maxKi))
      } else {
        // 0x0800B516: size by charge
        this.enemyOwned = false
        if (this.c > 72) {
          this.size = 'large'
          this.w = this.h = 16
          this.sprite = g.data.byLabel.get('ki_blast')
        } else if (this.c > 20) {
          this.size = 'medium'
          this.w = this.h = 8
          this.x += 4
          this.y += 4
          this.sprite = g.data.byLabel.get('ki_blast_medium')
        } else {
          this.size = 'small'
          this.w = this.h = 6
          this.x += 5
          this.y += 5
          this.sprite = g.data.byLabel.get('ki_blast_small')
        }
        this.launch()
      }
      return
    }
    if (this.st === 1) {
      this.x += this.vx
      this.y += this.vy
      if (!this.isEnemy) {
        this.c--
        if (this.c === 0) {
          this.vanish()
          return
        }
      } else {
        const cam = w.camera
        const r = this.rect()
        if (!(r.x0 < cam.x1 && cam.x0 < r.x1 && r.y0 < cam.y1 && cam.y0 < r.y1)) {
          this.vanish()
          return
        }
      }
      const rec: HitRecord = { rect: this.rect(), dmg: this.dmg, enemyOwned: this.enemyOwned, once: false, owner: this.owner.char }
      w.debugRect(rec.rect, this.enemyOwned ? '#f44' : '#ff0', 2)
      if (sendFirst(w, 1, rec) || !g.passable(rec.rect, 2)) this.vanish()
    }
  }
  /** 0x0800B554 fly enter */
  private launch() {
    const l = LAUNCH[this.facing]
    this.st = 1
    this.tick = 0
    this.x += l[0]
    this.y += l[1]
    this.vx = l[2]
    this.vy = l[3]
    this.owner.t24 = 2
    this.dmg = kiBlastDamage(this.owner.char.maxHp)
    if (this.owner.kind === 'player') (this.owner as Player).charging = null
  }
  private vanish() {
    this.st = 2
    this.tick = 0
  }
  animTick() {
    if (this.st === 2) {
      this.tick++
      if (this.tick >= 24) this.removed = true
      return
    }
    this.tick = (this.tick + 1) & 0xff
  }
  draw(r: Renderer) {
    const s = this.sprite
    if (!s) return
    // animation row = state number (0 charge ball, 1 flying comet per direction, 2 vanish)
    const a = s.anim(this.st)
    if (!a) return
    // vanish: frame = tick >> 3 (anim tick 0x0800B4CA)
    if (this.st === 2) s.draw(r.ctx, a, this.facing as 0, r.sx(this.x), r.sy(this.y), 0, { index: this.tick >> 3 })
    else s.draw(r.ctx, a, this.facing as 0, r.sx(this.x), r.sy(this.y), this.tick)
  }
}

/** Kamehameha: states 0 charge, 1 beam (grows 16 frames), 2 retract (8 frames). */
export class KameBeam extends Proj {
  kind = 'kamehameha'
  table: [number, number, number, number][] = K_GROW
  /** +0x14 drawing thickness class (0..2) */
  thick = 0
  constructor(g: Log1Game, owner: Player) {
    super(g, owner)
    this.sprite = g.data.byLabel.get('kamehameha')
    this.follow()
    this.c = 0
    this.w = this.h = 8
  }
  update(w: World) {
    if (this.st === 0) {
      if (w.input.held & BTN.B) {
        this.charge(chargeCost(this.owner.char.maxKi))
        return
      }
      // release: enter 0x0800BCA0
      this.st = 1
      this.tick = 0
      this.thick = 0
      this.w = this.h = 8
      const s = K_START[this.facing]
      this.x += s[0]
      this.y += s[1]
      this.table = K_GROW
      ;(this.owner as Player).charging = null
      return
    }
    if (this.st === 1) {
      const t = this.table[this.facing]
      this.x += t[0]
      this.y += t[1]
      this.w += t[2]
      this.h += t[3]
      const rec: HitRecord = { rect: this.rect(), dmg: this.c, enemyOwned: false, once: true, owner: this.owner.char }
      w.debugRect(rec.rect, '#ff0', 2)
      sendAll(w, 1, rec)
      if (!this.g.passable(rec.rect, 2)) {
        this.table = K_WALL
        this.retract()
        return
      }
      this.tick++
      if (this.tick >= 16) {
        this.table = K_RETRACT
        this.retract()
      }
      return
    }
    // state 2 update 0x0800BDC0
    this.tick++
    if (this.tick >= 8) this.removed = true
  }
  private retract() {
    this.st = 2
    this.tick = 0
    this.owner.t24 = 2
  }
  animTick() {
    if (this.st === 1) {
      this.thick = Math.min(2, Math.trunc((this.tick * this.c) / Math.max(1, this.owner.char.maxKi)) >> 1)
    } else if (this.st === 2) {
      // 0x0800BDD6: slide/shrink by the retract table
      const t = this.table[this.facing]
      this.x += t[0]
      this.y += t[1]
      this.w = Math.max(0, this.w + t[2])
      this.h = Math.max(0, this.h + t[3])
    }
  }
  draw(r: Renderer) {
    // the ROM draws the beam with custom OAM code (0x0800B778); approximated as the thin yellow beam seen in game
    if (this.st === 0) {
      const s = this.sprite
      const a = s?.anim(0)
      if (s && a) s.draw(r.ctx, a, 0, r.sx(this.x), r.sy(this.y), this.g.fc)
      return
    }
    const g = r.ctx
    const rc = this.rect()
    const horiz = this.facing >= 2
    const len = horiz ? rc.x1 - rc.x0 : rc.y1 - rc.y0
    if (len <= 0) return
    const th = 2 + this.thick
    const cx = r.sx((rc.x0 + rc.x1) >> 1)
    const cy = r.sy((rc.y0 + rc.y1) >> 1)
    g.fillStyle = '#e86a00'
    if (horiz) g.fillRect(r.sx(rc.x0), cy - (th >> 1) - 1, len, th + 2)
    else g.fillRect(cx - (th >> 1) - 1, r.sy(rc.y0), th + 2, len)
    g.fillStyle = '#ffe700'
    if (horiz) g.fillRect(r.sx(rc.x0), cy - (th >> 1), len, th)
    else g.fillRect(cx - (th >> 1), r.sy(rc.y0), th, len)
    g.fillStyle = '#fff7c0'
    if (horiz) g.fillRect(r.sx(rc.x0), cy, len, 1)
    else g.fillRect(cx, r.sy(rc.y0), 1, len)
  }
}

/** Solar Flare: 0 charge, 1 flash (32 frames). Freezes every enemy AI. */
export class SolarFlare extends Proj {
  kind = 'solar_flare'
  constructor(g: Log1Game, owner: Player) {
    super(g, owner)
    this.sprite = g.data.byLabel.get('solar_flare')
    this.follow()
    this.c = 0
  }
  update(w: World) {
    if (this.st === 0) {
      if (w.input.held & BTN.B) {
        this.charge(chargeCost(this.owner.char.maxKi))
        return
      }
      // release 0x0800B692
      this.st = 1
      this.tick = 0
      const p = this.owner.char
      if (this.c > p.maxKi) this.c = p.maxKi
      const dur = solarFlareDuration(this.c, p.maxKi)
      this.g.freeze(dur)
      this.g.sfReady = this.g.fc + dur + 60
      ;(this.owner as Player).charging = null
      w.logEvent('info', `Solar Flare: enemies frozen for ${dur} frames`)
    }
  }
  animTick() {
    if (this.st === 1) {
      this.tick++
      if (this.tick >= 32) {
        this.removed = true
        this.owner.t24 = 2
      }
      return
    }
    this.tick = (this.tick + 1) & 0xff
  }
  draw(r: Renderer) {
    const s = this.sprite
    if (!s) return
    // row 0: ball in the hands while charging; row 1: the flash, frame = tick >> 3 (0x0800B6D6)
    const a = s.anim(this.st)
    if (!a) return
    if (this.st === 1) s.draw(r.ctx, a, this.facing as 0, r.sx(this.x), r.sy(this.y), 0, { index: this.tick >> 3 })
    else s.draw(r.ctx, a, this.facing as 0, r.sx(this.x), r.sy(this.y), this.tick)
  }
}

