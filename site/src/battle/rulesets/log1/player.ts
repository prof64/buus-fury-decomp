// LoG1 Goku (player entity, state table 0x0816696C; controller 0x030007AC). Ports of:
//   Standing 0x080029F8 / Walking 0x08002A16 / Bored 0x08002A34 -> 0x08002E80 (move, A, R, B)
//   movement 0x08002CE0 (one direction per frame, UP > DOWN > LEFT > RIGHT, 16/16 px per frame, x2 flying)
//   punch msg 0 + 16-frame Melee1/Melee2 (anim tick 0x08002A52), Ki pose 0x08002B8C/0x08002B9A/0x08002BB0,
//   flight 0x08002A88 (R lands, 1 energy per 8 frames of movement, return to take-off point when stranded),
//   msg 1 handler 0x08003032 (enemy projectiles), punch damage 0x08005676.

import { BTN, DPAD } from '../../core/input'
import type { Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { World } from '../../core/world'
import { punchDamage } from './damage'
import { inRect, Log1Entity, overlapStrict, sendFirst, type HitRecord } from './entity'
import type { Char, Log1Game } from './game'
import { KameBeam, KiBlast, SolarFlare } from './projectiles'

/** punch point offsets from the player position (0x08003446), by facing down, up, left, right */
const PUNCH_POINT: [number, number][] = [
  [8, 8],
  [8, -8],
  [-2, -8],
  [18, -8],
]
/** foot box (offset table 0x08010840) */
const FOOT = { x: 0, y: 0, w: 16, h: 8 }

export class Player extends Log1Entity {
  kind = 'player'
  char: Char
  /** controller +0x14/+0x16, 1/16 px */
  sx = 0
  sy = 0
  /** projectile being charged (for the HUD / debug) */
  charging: KiBlast | KameBeam | SolarFlare | null = null
  dead = false
  deathT = 0
  hurt: number[][]
  constructor(g: Log1Game, char: Char) {
    super(g)
    this.char = char
    this.hurt = g.data.playerHurt
    this.sprite = g.data.byLabel.get('goku')
  }
  setPos(x: number, y: number) {
    this.sx = x << 4
    this.sy = y << 4
    this.x = x
    this.y = y
    this.g.px = x
    this.g.py = y
  }
  hurtRect(): Rect {
    const h = this.hurt[this.facing]
    return { x0: this.x + h[0], y0: this.y + h[1], x1: this.x + h[2], y1: this.y + h[3] }
  }

  enter(n: number) {
    // standing 0x08002998 / flying 0x08002A7A / ki 0x08002B8C clear +0x24; melee, ki, flying, death, bored reset the tick
    if (n === 0 || n === 14 || n === 5 || n === 2) this.t24 = 0
    if (n === 2 || n === 3 || n === 4 || n === 5 || n === 14 || n === 15) this.tick = 0
    if (n === 2) this.dead = true
  }

  update(w: World) {
    if (this.st === 2) {
      // Death 0x08002B70: counts frames (game over at 0x80)
      this.deathT++
      return
    }
    const inp = w.input
    switch (this.st) {
      case 0:
      case 15:
        if (inp.held & DPAD) this.setState(1)
        this.common(w)
        break
      case 1:
        if (!(inp.held & DPAD)) this.setState(0)
        this.common(w)
        break
      case 5:
        if (this.t24 === 2 && this.tick >= 16) this.setState(0)
        break
      case 14:
        this.flyUpdate(w)
        break
      default:
        break
    }
  }

  /** 0x08002E80 */
  private common(w: World) {
    const g = this.g
    const inp = w.input
    this.move(w, 1, 0)
    if (inp.pressed & BTN.A) {
      if (!sendFirst(w, 0, this)) this.setState(3 + (g.fc & 1))
    }
    if (inp.pressed & BTN.R) {
      // take off: the foot box must be free on the flying layer and energy left
      if (g.passable(this.footRect(), 1) && g.flight > 0) {
        g.takeoff = [this.sx, this.sy]
        this.setState(14)
        g.flying = true
      }
      return
    }
    if (inp.pressed & BTN.B) {
      if (g.selected === 2 && g.fc < g.sfReady) return
      this.fireMove(w, g.selected)
      this.setState(5)
    }
  }

  footRect(): Rect {
    return { x0: this.x + FOOT.x, y0: this.y + FOOT.y, x1: this.x + FOOT.x + FOOT.w, y1: this.y + FOOT.y + FOOT.h }
  }

  /** 0x08002CE0 move(speed multiplier, layer) */
  private move(w: World, mult: number, layer: number) {
    const g = this.g
    const held = w.input.held
    const sp = 16 * mult
    const [mw, mh] = [g.mapTiles[0] * 8, g.mapTiles[1] * 8]
    const tryAt = (nx: number, ny: number) => g.passable({ x0: (nx >> 4) + FOOT.x, y0: (ny >> 4) + FOOT.y, x1: (nx >> 4) + FOOT.x + FOOT.w, y1: (ny >> 4) + FOOT.y + FOOT.h }, layer)
    if (held & BTN.UP) {
      this.facing = 1
      let ny = this.sy - sp
      if (ny < 0) ny = 0
      if (tryAt(this.sx, ny)) this.sy = ny
    } else if (held & BTN.DOWN) {
      this.facing = 0
      let ny = this.sy + sp
      const lim = (mh - 8) << 4
      if (ny > lim) ny = lim
      if (tryAt(this.sx, ny)) this.sy = ny
    } else if (held & BTN.LEFT) {
      this.facing = 2
      let nx = this.sx - sp
      if (nx < 0) nx = 0
      if (tryAt(nx, this.sy)) this.sx = nx
    } else if (held & BTN.RIGHT) {
      this.facing = 3
      let nx = this.sx + sp
      const lim = (mw - 16) << 4
      if (nx > lim) nx = lim
      if (tryAt(nx, this.sy)) this.sx = nx
    }
    if (held & DPAD) {
      this.x = this.sx >> 4
      this.y = this.sy >> 4
      g.followCamera(this.frameRect())
    }
    g.px = this.sx >> 4
    g.py = this.sy >> 4
  }

  /** 0x08002A88 */
  private flyUpdate(w: World) {
    const g = this.g
    const inp = w.input
    if (inp.pressed & BTN.R) {
      if (g.passable(this.footRect(), 0)) this.land()
      return
    }
    this.move(w, 2, 1)
    if (!(inp.held & DPAD)) return
    this.t24++
    if (this.t24 < 8) return
    this.t24 = 0
    // 0x08003290: returns 1 and spends a unit while energy is left; 0 (-> land) only when already empty
    if (g.flight > 0) {
      g.flight--
      return
    }
    if (!g.passable(this.footRect(), 0)) {
      // stranded over a wall/water: back to the take-off point
      this.sx = g.takeoff[0]
      this.sy = g.takeoff[1]
      this.x = this.sx >> 4
      this.y = this.sy >> 4
      g.px = this.x
      g.py = this.y
      g.followCamera(this.frameRect())
      this.flash(8)
    }
    this.land()
  }
  private land() {
    this.g.flying = false
    this.setState(1)
  }

  /** fire_move 0x0800B3DC */
  private fireMove(w: World, move: number) {
    const p = move === 1 ? new KameBeam(this.g, this) : move === 2 ? new SolarFlare(this.g, this) : new KiBlast(this.g, this, false, 0)
    this.charging = p
    w.add(p)
  }

  animTick() {
    switch (this.st) {
      case 2:
        // 0x0800293C
        if (this.tick < 64) this.tick++
        this.frameIdx = this.tick >> 4
        return
      case 0: {
        // 0x080029A6: blink; after 4 idle cycles the Bored animation
        const v = this.tick & 0x7f
        if (v === 0x70) {
          if (this.t24 === 4 && this.hasState(15)) {
            this.setState(15)
            this.tick = (this.tick + 1) & 0xff
            return
          }
          this.t24++
        }
        this.frameIdx = v > 0x70 ? 1 : 0
        this.tick = (this.tick + 1) & 0xff
        return
      }
      case 1:
        // 0x08002902: frame = (tick >> 3) & 3
        this.frameIdx = (this.tick >> 3) & 3
        this.tick = (this.tick + 1) & 0xff
        return
      case 3:
      case 4:
        // 0x08002A52
        if (this.tick < 16) {
          this.frameIdx = this.tick >> 2
          this.tick++
        } else {
          this.setState(0)
          this.animTick()
        }
        return
      case 5:
        if (this.t24 === 0) {
          if (!(this.g.world.input.held & BTN.B)) this.t24 = 1
          return
        }
        if (this.tick < 16) this.tick++
        return
      default:
        this.tick = (this.tick + 1) & 0xff
    }
  }

  /** msg 0 (A) is handled by others; msg 1: enemy projectile hit test 0x08003032 */
  message(_w: World, msg: number, arg: unknown): boolean {
    if (msg !== 1) return false
    const rec = arg as HitRecord
    if (!rec.enemyOwned) return false
    if (!overlapStrict(this.hurtRect(), rec.rect)) return false
    this.g.hit(rec.owner, this.char, 1, rec.dmg)
    return true
  }

  /** punch point (0x08003446) */
  punchPoint(): [number, number] {
    const o = PUNCH_POINT[this.facing]
    return [(this.sx >> 4) + o[0], (this.sy >> 4) + o[1]]
  }
  /** pickup point (0x0800339A): 2 px outside the foot box, centred */
  pickupPoint(): [number, number] {
    const x = this.sx >> 4
    const y = this.sy >> 4
    const cx = x + (FOOT.w >> 1)
    const cy = y + (FOOT.h >> 1)
    switch (this.facing) {
      case 0:
        return [cx, y + FOOT.h + 2]
      case 1:
        return [cx, y - 2]
      case 2:
        return [x - 2, cy]
      default:
        return [x + FOOT.w + 2, cy]
    }
  }
  /** the punch that an enemy handler accepted (0x08005604): turn the player's attack anim on and hit */
  punch(target: Char) {
    const g = this.g
    this.setState(3 + (g.fc & 1))
    const dmg = punchDamage(this.char.maxHp, g.rng2.rand2(0x80))
    g.hit(this.char, target, 0, dmg)
  }
  pointIn(r: Rect) {
    const [x, y] = this.punchPoint()
    return inRect(x, y, r)
  }

  draw(r: Renderer, w: World) {
    super.draw(r, w)
  }
  drawDebug(r: Renderer) {
    r.strokeRect(this.hurtRect(), '#3cf')
    r.strokeRect(this.footRect(), '#0f0')
    const [x, y] = this.punchPoint()
    r.strokeRect({ x0: x, y0: y, x1: x + 1, y1: y + 1 }, '#ff0')
  }
  debugLabel() {
    return `Goku ${['stand', 'walk', 'death', 'punch', 'punch', 'ki'][this.st] ?? this.st}${this.g.flying ? ' fly' : ''}\nHP ${this.char.hp} Ki ${this.char.ki}`
  }
  get px() {
    return this.x
  }
  get py() {
    return this.y
  }
}

