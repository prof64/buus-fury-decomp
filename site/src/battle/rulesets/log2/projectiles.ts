// LoG2 projectiles (player specials and enemy attacks). Numbers from data/log2/moves.json player_attacks and
// enemies.json projectiles; ROM addresses in comments. Positions are 24.8 fixed point, rects integer px.
// Visual frame choice (which effect sprite frame is shown) is ours; hitboxes/speeds/timings are the game's.

import { DIR_VEC, overlaps, type Dir, type Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { SpriteSet } from '../../core/sprites'
import type { Entity, World } from '../../core/world'
import { broadcast, MSG_ATTACK, MSG_PROBE, type Actor, type HitMsg, type Receiver } from './actor'
import type { Log2Ctx } from './context'
import { ATTACKS, computeDamage } from './damage'

export abstract class Projectile implements Entity, Receiver {
  id = 0
  kind = 'projectile'
  removed = false
  layer = 1
  t = 0
  /** centre, 24.8 */
  x: number
  y: number
  ctx: Log2Ctx
  owner: Actor
  facing: Dir
  team: 0 | 1 | 2
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, team: 0 | 1 | 2, x: number, y: number) {
    this.ctx = ctx
    this.owner = owner
    this.facing = facing
    this.team = team
    this.x = x
    this.y = y
  }
  get px() {
    return this.x >> 8
  }
  get py() {
    return this.y >> 8
  }
  depth() {
    return this.py
  }
  abstract rect(): Rect
  abstract update(w: World): void
  abstract draw(r: Renderer, w: World): void
  onMessage(_w: World, _type: number, _msg: HitMsg): boolean {
    return false
  }
  /** off-screen test 0x08013C22 (240x160 camera) */
  offCamera(w: World) {
    return !overlaps(this.rect(), w.camera)
  }
  /** wall test: centre outside the walkable arena */
  inWall(w: World) {
    const b = w.arena.bounds
    return this.px < b.x0 || this.px >= b.x1 || this.py < b.y0 - 24 || this.py >= b.y1
  }
  msg(rect: Rect, attackId: number, scale: number, team: 0 | 1 | 2 = this.team): HitMsg {
    return { team, rect, facing: this.facing, scale, attack: ATTACKS[attackId], attacker: this.owner, source: this }
  }
  /** `first`: deliver to the first receiver that accepts it (single-target projectiles: 0x08006E0A) */
  hit(w: World, rect: Rect, attackId: number, scale: number, team: 0 | 1 | 2 = this.team, first = false): boolean {
    return broadcast(w, MSG_ATTACK, this.msg(rect, attackId, scale, team), first)
  }
  probe(w: World, rect: Rect): boolean {
    return broadcast(w, MSG_PROBE, this.msg(rect, 0, 0))
  }
  drawDebug(r: Renderer) {
    r.strokeRect(this.rect(), '#fa0')
  }
  debugLabel?(): string
}

const box = (cx: number, cy: number, w: number, h: number): Rect => ({ x0: cx - (w >> 1), y0: cy - (h >> 1), x1: cx - (w >> 1) + w, y1: cy - (h >> 1) + h })

// ------------------------------------------------------------------------------------------------------------
// Ki blast class (ctor 0x0800EAD8 / 0x0800E8B0, vt 0x08024CBC): shared by the player's energy blast (attack 3)
// and the enemies' ShootKiBlast. 3.0 px/f along facing, 16x16 centred, single hit, 16-frame impact,
// removed off-camera; enemy blasts are cancelled by an overlapping player energy attack (0x0800EA3B).
export class KiBlast extends Projectile {
  impact = -1
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, team: 0 | 1, x: number, y: number) {
    super(ctx, owner, facing, team, x, y)
  }
  rect() {
    return box(this.px, this.py, 16, 16)
  }
  update(w: World) {
    this.t++
    if (this.impact >= 0) {
      if (++this.impact >= 16) this.removed = true
      return
    }
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 0x300
    this.y += dy * 0x300
    if (this.offCamera(w)) {
      this.removed = true
      return
    }
    if (this.inWall(w)) {
      this.impact = 0
      return
    }
    if (this.hit(w, this.rect(), 3, 0x400, this.team, true)) this.impact = 0
  }
  onMessage(_w: World, type: number, msg: HitMsg): boolean {
    // enemy blasts are destroyed by a player energy attack overlapping them
    if (this.team === 0 && this.impact < 0 && type === MSG_ATTACK && msg.team === 1 && msg.attack.type === 1 && overlaps(msg.rect, this.rect())) this.impact = 0
    return false
  }
  draw(r: Renderer) {
    const s = this.ctx.fx('fx_0840e47c')
    if (!s) return
    if (this.impact >= 0) {
      s.drawFrame(r.ctx, 12, r.sx(this.px), r.sy(this.py), false)
      return
    }
    const base = this.facing === 0 ? 0 : this.facing === 1 ? 4 : 8
    const k = this.t < 4 ? 0 : 1 + ((this.t >> 2) % 3)
    s.drawFrame(r.ctx, base + k, r.sx(this.px), r.sy(this.py), this.facing === 3)
  }
}

// Scatter Shot pellet (ctor 0x080137C4, vt 0x08024D14): 16x16, velocity from table 0x086E362C, attack 6.
export class Pellet extends Projectile {
  impact = -1
  vx: number
  vy: number
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number, vx: number, vy: number) {
    super(ctx, owner, facing, 1, x, y)
    this.vx = vx
    this.vy = vy
  }
  rect() {
    return box(this.px, this.py, 16, 16)
  }
  update(w: World) {
    this.t++
    if (this.impact >= 0) {
      if (++this.impact >= 16) this.removed = true
      return
    }
    this.x += this.vx
    this.y += this.vy
    if (this.offCamera(w)) return void (this.removed = true)
    if (this.inWall(w)) return void (this.impact = 0)
    if (this.hit(w, this.rect(), 6, 0x400, 1, true)) this.impact = 0
  }
  draw(r: Renderer) {
    const s = this.ctx.fx('fx_0840e47c')
    if (!s) return
    if (this.impact >= 0) return s.drawFrame(r.ctx, 12, r.sx(this.px), r.sy(this.py), false)
    const horiz = Math.abs(this.vx) > Math.abs(this.vy)
    const base = horiz ? 8 : this.vy > 0 ? 0 : 4
    s.drawFrame(r.ctx, base + 1 + ((this.t >> 2) % 3), r.sx(this.px), r.sy(this.py), horiz && this.vx > 0)
  }
}

// Masenko Ball (state 0x2D, ctor 0x080139BC): launched on release with C = clamp(charge, 255, 1024):
// v = dir*C (24.8), screen arc vy -= 0x200 then +0x20/f, height 0x800 with +0x200/f, -0x20/f; explodes when the
// height goes below 0: attack 7 over a 16x16 blast, scale min(2C+256, 2048) (0x08013B88).
export class MasenkoBall extends Projectile {
  launched = false
  C = 0
  vx = 0
  vy = 0
  arc = 0
  h = 0x800
  vh = 0x200
  boom = -1
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number) {
    super(ctx, owner, facing, 1, x, y)
  }
  launch(charge: number) {
    this.C = Math.max(255, Math.min(charge, 1024))
    const [dx, dy] = DIR_VEC[this.facing]
    this.vx = dx * this.C
    this.vy = dy * this.C
    this.arc = -0x200
    this.launched = true
  }
  rect() {
    return box(this.px, this.py, this.boom >= 0 ? 16 : 8, this.boom >= 0 ? 16 : 8)
  }
  update(w: World) {
    this.t++
    if (this.boom >= 0) {
      if (++this.boom >= 16) this.removed = true
      return
    }
    if (!this.launched) return
    this.x += this.vx
    this.y += this.vy + this.arc
    this.arc += 0x20
    this.vh -= 0x20
    this.h += this.vh
    if (this.h < 0) {
      this.boom = 0
      this.hit(w, box(this.px, this.py, 16, 16), 7, Math.min(2 * this.C + 256, 2048))
    } else if (this.offCamera(w)) this.removed = true
  }
  draw(r: Renderer) {
    if (this.boom >= 0) {
      this.ctx.fx('fx_086dbc60')?.drawFrame(r.ctx, Math.min(2, this.boom >> 2), r.sx(this.px), r.sy(this.py), false)
      return
    }
    this.ctx.fx('fx_086de8a4')?.drawFrame(r.ctx, this.launched ? 2 : 1 + ((this.t >> 2) & 1), r.sx(this.px), r.sy(this.py), false)
  }
}

// Big Bang ball (ctor 0x08013E48, vt 0x080241B0): scale starts 32 (+1/frame while held, EP>0); on release jumps
// 16 px then flies 2 px/f; each frame a 24x24 probe; on contact/wall: attack 8 (0xC000) over a 64x64 box, 16-frame
// explosion.
export class BigBangBall extends Projectile {
  /** 32 at creation (+0x44 = 0x20) +1 per frame held; the press frame already counts (measured tap = 33) */
  scale = 33
  launched = false
  boom = -1
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number) {
    super(ctx, owner, facing, 1, x, y)
  }
  launch() {
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 16 * 256
    this.y += dy * 16 * 256
    this.launched = true
  }
  size() {
    return this.scale < 64 ? 8 : this.scale < 128 ? 16 : 32
  }
  rect() {
    return this.boom >= 0 ? box(this.px, this.py, 64, 64) : box(this.px, this.py, this.size(), this.size())
  }
  explode(w: World) {
    this.boom = 0
    this.hit(w, box(this.px, this.py, 64, 64), 8, this.scale)
  }
  update(w: World) {
    this.t++
    if (this.boom >= 0) {
      if (++this.boom >= 16) this.removed = true
      return
    }
    if (!this.launched) return
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 0x200
    this.y += dy * 0x200
    if (this.inWall(w) || this.probe(w, box(this.px, this.py, 24, 24))) this.explode(w)
    else if (this.offCamera(w)) this.removed = true
  }
  draw(r: Renderer) {
    if (this.boom >= 0) {
      this.ctx.fx('fx_086e1590')?.drawFrame(r.ctx, Math.min(3, this.boom >> 2), r.sx(this.px), r.sy(this.py), false)
      this.ctx.fx('fx_086ff09c')?.drawFrame(r.ctx, Math.min(3, this.boom >> 2), r.sx(this.px), r.sy(this.py), false)
      return
    }
    const s = this.ctx.fx('fx_086f77d4')
    const tier = this.scale < 48 ? 5 : this.scale < 64 ? 6 : this.scale < 96 ? 7 : this.scale < 128 ? 8 : 9
    s?.drawFrame(r.ctx, tier, r.sx(this.px), r.sy(this.py), false)
  }
}

// Burning Attack bolt (ctor 0x080141B4, vt 0x0802507C): hovers 8x8 while held; release: +10 px jump, 2.5 px/f,
// 32x32 rect, first target takes attack 9 (power 0x40) - the stun comes from the receiver's AI (state 6/20/115).
export class BurningBolt extends Projectile {
  launched = false
  fade = -1
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number) {
    super(ctx, owner, facing, 1, x, y)
  }
  launch() {
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 10 * 256
    this.y += dy * 10 * 256
    this.launched = true
  }
  rect() {
    return this.launched ? box(this.px, this.py, 32, 32) : box(this.px, this.py, 8, 8)
  }
  update(w: World) {
    this.t++
    if (this.fade >= 0) {
      if (++this.fade >= 16) this.removed = true
      return
    }
    if (!this.launched) return
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 0x280
    this.y += dy * 0x280
    if (this.offCamera(w)) return void (this.removed = true)
    if (this.inWall(w)) return void (this.fade = 0)
    if (this.hit(w, this.rect(), 9, 0x400, 1, true)) this.fade = 0
  }
  draw(r: Renderer) {
    const s = this.ctx.fx('fx_086e272c')
    if (this.fade >= 0) {
      if (this.fade & 2) return
    }
    if (!this.launched) this.ctx.fx('fx_08703b60')?.drawFrame(r.ctx, (this.t >> 2) % 3, r.sx(this.px), r.sy(this.py), false)
    else s?.drawFrame(r.ctx, 2 + ((this.t >> 2) % 4), r.sx(this.px), r.sy(this.py), false)
  }
}

// Sword Blast wave (vt 0x08024B24): 3.5 px/f, 8x16 (horizontal) / 16x8 (vertical), attack 13 (0xC00), stops at
// the first enemy.
export class SwordWave extends Projectile {
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number) {
    super(ctx, owner, facing, 1, x, y)
  }
  rect() {
    return this.facing >= 2 ? box(this.px, this.py, 8, 16) : box(this.px, this.py, 16, 8)
  }
  update(w: World) {
    this.t++
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 0x380
    this.y += dy * 0x380
    if (this.offCamera(w) || this.inWall(w)) return void (this.removed = true)
    if (this.hit(w, this.rect(), 13, 0x400, 1, true)) this.removed = true
  }
  draw(r: Renderer) {
    const s = this.ctx.fx('fx_086e5b0c')
    if (!s) return
    const k = Math.min(2, this.t >> 2)
    if (this.facing >= 2) s.drawFrame(r.ctx, 6 + k, r.sx(this.px), r.sy(this.py), this.facing === 2)
    else s.drawFrame(r.ctx, this.facing === 0 ? k : 3 + k, r.sx(this.px), r.sy(this.py), false)
  }
}

// Spirit Bomb (ctor 0x080165EC, vt 0x08024AF8): size starts 5 (+1/frame held, EP>0), 24 px above the head; thrown
// 8 frames after release at 1 px/f; on impact attack 14 (0xA000) with scale = size over the bomb box expanded by
// its own size on every side, plus (size >= 128) attack 15 to the whole map = 324-frame stun.
export class SpiritBomb extends Projectile {
  size = 5
  thrown = false
  boom = -1
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number) {
    super(ctx, owner, facing, 1, x, y)
  }
  edge() {
    return this.size < 64 ? 8 : this.size < 192 ? 16 : 32
  }
  rect() {
    return box(this.px, this.py, this.edge(), this.edge())
  }
  update(w: World) {
    this.t++
    if (this.boom >= 0) {
      if (++this.boom >= 24) this.removed = true
      return
    }
    if (!this.thrown) return
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 0x100
    this.y += dy * 0x100
    if (this.inWall(w) || this.probe(w, this.rect())) this.detonate(w)
    else if (this.offCamera(w)) this.removed = true
  }
  detonate(w: World) {
    this.boom = 0
    const e = this.edge()
    this.hit(w, box(this.px, this.py, e * 3, e * 3), 14, this.size)
    if (this.size >= 128) this.hit(w, { x0: 0, y0: 0, x1: 2048, y1: 2048 }, 15, 0)
  }
  draw(r: Renderer) {
    const s = this.ctx.fx('fx_086f77d4')
    if (this.boom >= 0) {
      const e = this.edge() * 3
      const g = r.ctx
      g.fillStyle = `rgba(200,240,255,${Math.max(0, 0.8 - this.boom / 30)})`
      g.fillRect(r.sx(this.px) - e / 2, r.sy(this.py) - e / 2, e, e)
      this.ctx.fx('fx_086e1590')?.drawFrame(g, Math.min(3, this.boom >> 2), r.sx(this.px), r.sy(this.py), false)
      return
    }
    const tier = this.size < 24 ? 5 : this.size < 48 ? 6 : this.size < 64 ? 7 : this.size < 128 ? 8 : 9
    s?.drawFrame(r.ctx, tier, r.sx(this.px), r.sy(this.py), false)
  }
}

// Beams: the player Kamehameha / Special Beam Cannon objects (ctor 0x080143A8 / 0x080157E0). The head grows
// 3 px/f from the muzzle. Kamehameha (and the boss beams reusing it) hits with a 16x16 head box and stops growing
// at the first target, wall or screen edge; SBC is a 12-px thick rect muzzle->tip that pierces and hits everything
// inside every frame.
export class Beam extends Projectile {
  /** the head is already 25 px out when the beam fires (measured: SBC tip 28 px at its first hit, the frame after) */
  len = 25
  stopped = false
  done = false
  kind2: 'kame' | 'sbc' | 'boss'
  /** boss beams: lifetime in frames */
  life: number
  attackId: number
  scale: number
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, team: 0 | 1, /** muzzle point (24.8) */
    x: number, y: number, kind2: 'kame' | 'sbc' | 'boss', attackId: number, scale: number, /** boss beams: lifetime */
    life = 0) {
    super(ctx, owner, facing, team, x, y)
    this.life = life
    this.kind2 = kind2
    this.attackId = attackId
    this.scale = scale
  }
  thick() {
    return this.kind2 === 'sbc' ? 12 : 16
  }
  /** rect of the whole beam muzzle..tip */
  bodyRect(): Rect {
    const [dx, dy] = DIR_VEC[this.facing]
    const t = this.thick() >> 1
    const mx = this.px
    const my = this.py
    const tx = mx + dx * this.len
    const ty = my + dy * this.len
    if (dx) return { x0: Math.min(mx, tx), y0: my - t, x1: Math.max(mx, tx), y1: my + t }
    return { x0: mx - t, y0: Math.min(my, ty), x1: mx + t, y1: Math.max(my, ty) }
  }
  headRect(): Rect {
    const [dx, dy] = DIR_VEC[this.facing]
    const tx = this.px + dx * this.len
    const ty = this.py + dy * this.len
    // 16x16 box at the head, just behind the tip
    return box(tx - dx * 8, ty - dy * 8, 16, 16)
  }
  rect() {
    return this.kind2 === 'sbc' ? this.bodyRect() : this.headRect()
  }
  tipOut(w: World) {
    const [dx, dy] = DIR_VEC[this.facing]
    const tx = this.px + dx * this.len
    const ty = this.py + dy * this.len
    const c = w.camera
    const b = w.arena.bounds
    return tx <= c.x0 || tx >= c.x1 || ty <= c.y0 || ty >= c.y1 || tx <= b.x0 || tx >= b.x1 || ty <= b.y0 - 16 || ty >= b.y1
  }
  /** keep the muzzle attached to the owner */
  follow(x: number, y: number) {
    this.x = x
    this.y = y
  }
  update(w: World) {
    this.t++
    if (this.done) return void (this.removed = true)
    if (this.life && this.t > this.life) return void (this.removed = true)
    if (this.kind2 === 'sbc') {
      if (!this.tipOut(w)) this.len += 3
      this.hit(w, this.rect(), this.attackId, this.scale)
      return
    }
    // Kamehameha head: grows 3 px/f while it touches nothing (re-grows when the target is knocked away),
    // only the first target is hit; damage starts on the second frame (measured kame_r_2 / kame_d: first hit +26)
    this.stopped = this.tipOut(w) || this.probe(w, this.headRect())
    if (!this.stopped) this.len += 3
    if (this.t >= 2) this.hit(w, this.rect(), this.attackId, this.scale, this.team, true)
  }
  draw(r: Renderer) {
    const pal = this.kind2 === 'sbc' ? 'fx_086e4560' : this.kind2 === 'boss' ? 'fx_08704604' : 'fx_086e3458'
    const s = this.ctx.fx(pal)
    const g = r.ctx
    const body = { ...this.bodyRect() }
    // visually start the body at the hands (the hit rect starts at the muzzle)
    if (this.team === 1) {
      const back = 12
      if (this.facing === 3) body.x0 -= back
      else if (this.facing === 2) body.x1 += back
      else if (this.facing === 0) body.y0 -= back
      else body.y1 += back
    }
    // body: solid core with a bright centre line (the ROM pieces are 32-px segments; drawn procedurally here)
    const col = this.kind2 === 'sbc' ? ['#c0306a', '#ffd23f'] : this.kind2 === 'boss' ? ['#e0a000', '#fff8c0'] : ['#3a8cff', '#e8f6ff']
    const horiz = this.facing >= 2
    const th = this.kind2 === 'sbc' ? 6 : 8
    g.fillStyle = col[0]
    if (horiz) g.fillRect(r.sx(body.x0), r.sy(this.py) - (th >> 1), body.x1 - body.x0, th)
    else g.fillRect(r.sx(this.px) - (th >> 1), r.sy(body.y0), th, body.y1 - body.y0)
    g.fillStyle = col[1]
    if (horiz) g.fillRect(r.sx(body.x0), r.sy(this.py) - 1, body.x1 - body.x0, 2)
    else g.fillRect(r.sx(this.px) - 1, r.sy(body.y0), 2, body.y1 - body.y0)
    if (this.kind2 === 'sbc') {
      // spiral
      g.fillStyle = '#ff7ad0'
      const n = this.len
      for (let i = (this.t * 2) % 6; i < n; i += 6) {
        const [dx, dy] = DIR_VEC[this.facing]
        const off = ((i / 6 + this.t) & 1 ? 1 : -1) * 3
        const xx = this.px + dx * i + (dx ? 0 : off)
        const yy = this.py + dy * i + (dy ? 0 : off)
        g.fillRect(r.sx(xx) - 1, r.sy(yy) - 1, 2, 2)
      }
    }
    // head ball from the ROM sprite (16x16 "head" frames)
    if (s && this.kind2 !== 'sbc') {
      const [dx, dy] = DIR_VEC[this.facing]
      const hx = this.px + dx * this.len - (dx ? 8 * (dx > 0 ? 1 : 0) : 8) + (dx < 0 ? 0 : 0)
      const hy = this.py + dy * this.len - (dy ? (dy > 0 ? 8 : 0) : 8)
      const fr = (this.facing === 0 ? 5 : this.facing === 1 ? 13 : this.facing === 2 ? 21 : 29) + ((this.t >> 2) & 1)
      s.drawFrame(g, fr, r.sx(hx), r.sy(hy), false)
    }
  }
}

// ------------------------------------------------------------------------------------------------------------
// Enemy projectiles (enemies.json projectiles)

// energy_ball (ctor 0x080160C0, vt 0x08024258): 1.75 px/f, 16x16 at position - origin offset by facing, attack 1.
// thrown_star (ctor 0x0801A0B0, vt 0x08024F0C): 1.75 px/f, 8x8 centred, attack 1. Both cancelled by player energy.
export class EnemyShot extends Projectile {
  impact = -1
  static ORIGIN: Record<Dir, [number, number]> = { 0: [8, 14], 1: [8, 2], 2: [2, 8], 3: [14, 8] }
  star: boolean
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number, star: boolean) {
    super(ctx, owner, facing, 0, x, y)
    this.star = star
  }
  rect(): Rect {
    if (this.star) return box(this.px, this.py, 8, 8)
    const [ox, oy] = EnemyShot.ORIGIN[this.facing]
    return { x0: this.px - ox, y0: this.py - oy, x1: this.px - ox + 16, y1: this.py - oy + 16 }
  }
  update(w: World) {
    this.t++
    if (this.impact >= 0) {
      if (++this.impact >= 12) this.removed = true
      return
    }
    const [dx, dy] = DIR_VEC[this.facing]
    this.x += dx * 0x1c0
    this.y += dy * 0x1c0
    if (this.offCamera(w)) return void (this.removed = true)
    if (this.inWall(w)) return void (this.star ? (this.removed = true) : (this.impact = 0))
    if (this.hit(w, this.rect(), 1, 0x400, 0, true)) this.impact = 0
  }
  onMessage(_w: World, type: number, msg: HitMsg): boolean {
    if (this.impact >= 0 || msg.team !== 1 || !overlaps(msg.rect, this.rect())) return false
    if ((type === MSG_ATTACK && msg.attack.type === 1) || msg.attack.id === 8 || msg.attack.id === 14) this.impact = 0
    return false
  }
  draw(r: Renderer) {
    if (this.impact >= 0) {
      this.ctx.fx('fx_086dbc60')?.drawFrame(r.ctx, Math.min(2, this.impact >> 2), r.sx(this.px), r.sy(this.py), false)
      return
    }
    if (this.star) {
      this.ctx.fx('fx_086fe048')?.drawFrame(r.ctx, (this.t >> 3) & 1, r.sx(this.px), r.sy(this.py), false)
      return
    }
    const s = this.ctx.fx('fx_086e5dc8')
    const fr = this.facing === 0 ? 0 : this.facing === 1 ? 1 : 2
    s?.drawFrame(r.ctx, fr, r.sx(this.px), r.sy(this.py), this.facing === 2)
  }
}

// lob_rock (ctor 0x0801CCC8, vt 0x08024204): ground velocity = (player - self) along the facing axis * 0x71D >> 8,
// height 8 px, vz 2 px/f, gravity 0x20; on landing one 16x16 hit, team 2 (also hurts enemies), explosion.
export class LobRock extends Projectile {
  z = 8 * 256
  vz = 0x200
  vx = 0
  vy = 0
  boom = -1
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number, target: [number, number]) {
    super(ctx, owner, facing, 2, x, y)
    const dxp = target[0] - this.px
    const dyp = target[1] - this.py
    if (facing >= 2) this.vx = (dxp * 0x71d) >> 8
    else this.vy = (dyp * 0x71d) >> 8
  }
  rect() {
    return box(this.px, this.py, 16, 16)
  }
  update(w: World) {
    this.t++
    if (this.boom >= 0) {
      if (++this.boom >= 16) this.removed = true
      return
    }
    this.x += this.vx
    this.y += this.vy
    this.z += this.vz
    this.vz -= 0x20
    if (this.z < 0) {
      this.boom = 0
      this.hit(w, this.rect(), 1, 0x400, 2)
    }
  }
  draw(r: Renderer) {
    if (this.boom >= 0) {
      this.ctx.fx('fx_08703164')?.drawFrame(r.ctx, Math.min(3, this.boom >> 2), r.sx(this.px), r.sy(this.py) + 4, false)
      return
    }
    const g = r.ctx
    g.fillStyle = 'rgba(0,0,0,0.35)'
    g.fillRect(r.sx(this.px) - 4, r.sy(this.py) + 2, 8, 3)
    this.ctx.fx('fx_086ffbfc')?.drawFrame(g, (this.t >> 3) % 3, r.sx(this.px), r.sy(this.py) - (this.z >> 8), false)
  }
}

// laser (ctor 0x0801E224, vt 0x08024E5C): from the muzzle grows 8 px/f up to `max`, hit area = segment +-4 px,
// at most one hit (attack 3), 60 frames then retracts 8 px/f.
export class Laser extends Projectile {
  len = 0
  hitDone = false
  max: number
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number, max: number) {
    super(ctx, owner, facing, 0, x, y)
    this.max = max
  }
  rect(): Rect {
    const [dx, dy] = DIR_VEC[this.facing]
    const tx = this.px + dx * this.len
    const ty = this.py + dy * this.len
    return { x0: Math.min(this.px, tx) - 4, y0: Math.min(this.py, ty) - 4, x1: Math.max(this.px, tx) + 4, y1: Math.max(this.py, ty) + 4 }
  }
  update(w: World) {
    this.t++
    if (this.t <= 60) this.len = Math.min(this.max, this.len + 8)
    else this.len -= 8
    if (this.len <= 0 && this.t > 60) return void (this.removed = true)
    if (!this.hitDone && this.hit(w, this.rect(), 3, 0x400)) this.hitDone = true
  }
  draw(r: Renderer) {
    const g = r.ctx
    const rc = this.rect()
    g.fillStyle = '#ff3030'
    g.fillRect(r.sx(rc.x0) + 2, r.sy(rc.y0) + 2, rc.x1 - rc.x0 - 4, rc.y1 - rc.y0 - 4)
    g.fillStyle = '#ffd0d0'
    if (this.facing >= 2) g.fillRect(r.sx(rc.x0) + 2, r.sy(this.py), rc.x1 - rc.x0 - 4, 1)
    else g.fillRect(r.sx(this.px), r.sy(rc.y0) + 2, 1, rc.y1 - rc.y0 - 4)
  }
}

// spray_particle (ctor 0x0801DCFC, vt 0x080250A8): 2 px/f along facing +- (rand(192)-96)/256 jitter, 16x16, life
// 20 frames, one hit with attack 3 at scale 51 applied directly (no hit stun, no invulnerability).
export class SprayParticle extends Projectile {
  vx: number
  vy: number
  done = false
  constructor(ctx: Log2Ctx, owner: Actor, facing: Dir, x: number, y: number, w: World) {
    super(ctx, owner, facing, 0, x, y)
    const [dx, dy] = DIR_VEC[facing]
    this.vx = dx * 0x200 + w.rng.rand(192, 'spray jitter') - 96
    this.vy = dy * 0x200 + w.rng.rand(192, 'spray jitter') - 96
  }
  rect() {
    return box(this.px, this.py, 16, 16)
  }
  update(w: World) {
    this.t++
    this.x += this.vx
    this.y += this.vy
    if (this.t >= 20) return void (this.removed = true)
    const p = this.ctx.player
    if (!this.done && p && !p.dead && p.hittable && overlaps(this.rect(), p.hurtRect())) {
      this.done = true
      const res = computeDamage(ATTACKS[3], this.owner.stats, p.stats, 51, w.rng)
      p.applyDamageDirect(w, res.dmg, res.crit)
    }
  }
  draw(r: Renderer) {
    const s = this.ctx.fx('fx_086dd884')
    const fr = this.facing === 0 ? 1 + ((this.t >> 2) & 1) : this.facing === 1 ? 4 + ((this.t >> 2) & 1) : 6
    s?.drawFrame(r.ctx, this.done ? 0 : fr, r.sx(this.px), r.sy(this.py), this.facing === 2)
  }
}

export function spriteOf(ctx: Log2Ctx, id: string): SpriteSet | undefined {
  return ctx.fx(id)
}
