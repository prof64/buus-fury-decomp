// LoG2 enemy entity (vtable 0x080252A4): class row stats, AI controller (ai.ts), hit handling (onMessage
// 0x0800E5B8 -> computeDamage -> AI on-hit tree), MeleeStrike geometry (0x0801BD2C), melee-range boxes
// (0x0801BEB8 / 0x0801E8B4), death nodes, and the per-node attack actions.

import { DIR_VEC, facingToward, overlaps, rectCenter, type Dir, type Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { World } from '../../core/world'
import { drawText } from '../../core/pixelfont'
import { Actor, broadcast, hitOverlaps, MSG_ATTACK, MSG_PROBE, type HitMsg } from './actor'
import { AiController, type AiHost } from './ai'
import type { AiNode } from './data'
import type { Log2Ctx } from './context'
import { ATTACKS, computeDamage } from './damage'
import type { EnemyClass } from './data'
import { Beam, EnemyShot, KiBlast, Laser, LobRock, SprayParticle } from './projectiles'

// SprayAttack mouth offsets (table 0x08701E28+4 by facing), relative to the position
const SPRAY_MOUTH: [number, number][] = [
  [40, 16],
  [8, -48],
  [-16, -12],
  [64, -12],
]
// LaserBeam muzzle (0x08703294+0xC) relative to the hurt-rect centre
const LASER_MUZZLE: [number, number][] = [
  [0, 8],
  [0, -8],
  [-8, 0],
  [8, 0],
]

export class Enemy extends Actor implements AiHost {
  kind = 'enemy'
  hp: number
  maxHp: number
  ai: AiController
  w!: World
  lastHitDir: Dir = 0
  /** HP reached 0 (death node ran) */
  defeated = false
  /** boss KO pose (quiet death node 0x086FF3C4): stays on the field */
  ko = false
  glow = 0
  stunned = false
  lastContact = -1000
  drainHits = 0
  traceLog: string[] = []
  classAiState: number

  ctx: Log2Ctx
  cls: EnemyClass
  label: string
  constructor(ctx: Log2Ctx, cls: EnemyClass, gfx: number, label: string) {
    super()
    this.ctx = ctx
    this.cls = cls
    this.label = label
    this.hp = this.maxHp = cls.hp
    this.stats = { STR: cls.STR, POW: cls.POW, END: cls.END, meleeMult: cls.meleeMult, energyMult: cls.energyMult }
    this.colW = cls.collision[0]
    this.colH = cls.collision[1]
    this.hurt = cls.hurt
    this.sprite = ctx.data.sprites.index(gfx)
    this.classAiState = cls.aiState
    this.ai = new AiController(this, ctx.data.ai)
    this.setAnim('idle', true, true)
  }

  /** spawn: push the class row's ai_state (enemy init 0x0800E280) */
  start(w: World) {
    this.w = w
    this.ai.push(this.cls.aiState)
  }

  update(w: World) {
    this.w = w
    this.tickAnim()
    if (this.glow > 0) this.glow--
    if (this.ko) return
    this.ai.tick()
  }

  // ---- AiHost ------------------------------------------------------------------------------------------------
  center(): [number, number] {
    return [this.px + (this.colW >> 1), this.py + (this.colH >> 1)]
  }
  private get player() {
    const p = this.ctx.player
    return p && !p.dead ? p : null
  }
  playerAlive() {
    return !!this.player
  }
  playerCenter(): [number, number] | null {
    const p = this.player
    return p ? p.center() : null
  }
  /** IfPlayerSeen (0x0801D5D0): within r and not directly behind (facing away along the dominant axis) */
  playerSeen(r2: number) {
    const p = this.playerCenter()
    if (!p) return false
    const [x, y] = this.center()
    const dx = p[0] - x
    const dy = p[1] - y
    if (dx * dx + dy * dy > r2) return false
    const toward = facingToward(dx, dy)
    const opposite = [1, 0, 3, 2][toward]
    return this.facing !== opposite
  }
  /** MeleeStrike hit rect (0x0801BD2C); also sets facing */
  strikeRect(): Rect | null {
    const p = this.player
    if (!p) return null
    const P = p.hurtRect()
    const E = this.hurtRect()
    const [ecx, ecy] = rectCenter(E)
    const [pcx, pcy] = rectCenter(P)
    const dx = pcx - ecx
    const dy = pcy - ecy
    if (Math.abs(dx) > Math.abs(dy)) {
      if (dx > 0) {
        this.facing = 3
        return { x0: ecx, y0: ecy, x1: E.x1 + 3, y1: E.y1 + 3 }
      }
      this.facing = 2
      return { x0: E.x0 - 4, y0: ecy, x1: ecx, y1: E.y1 + 3 }
    }
    const C = this.colBox()
    if (dy > 0) {
      this.facing = 0
      return { x0: C.x0, y0: E.y0, x1: C.x1, y1: E.y1 - 6 }
    }
    this.facing = 1
    return { x0: C.x0, y0: E.y0, x1: C.x1, y1: C.y0 }
  }
  /** WhileInMeleeRange: player hurt rect overlaps the range box and the strike rect */
  inMeleeRange(b: boolean) {
    const p = this.player
    if (!p) return false
    const P = p.hurtRect()
    const E = this.hurtRect()
    const box = b ? { x0: E.x0 + 2, y0: E.y0 + 2, x1: E.x1 - 3, y1: E.y1 - 6 } : { x0: E.x0 - 4, y0: E.y0, x1: E.x1 + 3, y1: E.y1 - 6 }
    if (!overlaps(P, box)) return false
    const s = this.strikeRect()
    return !!s && overlaps(P, s)
  }
  slide(vx: number, vy: number, setAnim = false): boolean {
    if (setAnim) this.setAnim(this.anim?.name === 'walk' ? 'walk' : 'walk', false, true)
    const fx = (nx: number, ny: number) => this.fits(this.w, nx >> 8, ny >> 8)
    if (fx(this.x + vx, this.y + vy)) {
      this.x += vx
      this.y += vy
      return true
    }
    if (vx && fx(this.x + vx, this.y)) {
      this.x += vx
      return true
    }
    if (vy && fx(this.x, this.y + vy)) {
      this.y += vy
      return true
    }
    return false
  }
  hurtAnimLength() {
    const a = this.sprite?.anim('hurt')
    return a ? a.length(this.facing) : 32
  }
  meleeStrike(variants: number) {
    const w = this.w
    const p = this.player
    if (p && p.hittable) {
      const rect = this.strikeRect()
      if (rect) {
        const msg: HitMsg = { team: 0, rect, facing: this.facing, scale: 0x400, attack: ATTACKS[2], attacker: this, source: this }
        w.debugRect(rect, '#ff4040', 8)
        p.onMessage(w, MSG_ATTACK, msg)
      }
    }
    w.rng.rand(3, 'enemy strike sfx')
    const v = w.rng.rand(variants, 'enemy strike anim')
    this.setAnim([6 + v, 6, 'attack1', 'idle'], true, false)
  }
  /** 1 if the strike anim (+1 frame, measured 17 f for the snake's 16-frame anim) has finished */
  animDone() {
    return !this.anim || this.animT > this.animLength()
  }
  contactCheck() {
    if (this.w.frame - this.lastContact <= 48) return
    if (this.hitPlayerOnContact()) this.lastContact = this.w.frame
  }
  hitPlayerOnContact(): boolean {
    const p = this.player
    if (!p) return false
    const r = this.hurtRect()
    if (!overlaps(r, p.hurtRect())) return false
    return p.onMessage(this.w, MSG_ATTACK, { team: 0, rect: r, facing: this.facing, scale: 0x400, attack: ATTACKS[2], attacker: this })
  }
  shoot(kind: 'ki' | 'ball' | 'star' | 'lob' | 'laser' | 'beam' | 'beamTao', param = 0) {
    const w = this.w
    const [cx, cy] = rectCenter(this.hurtRect())
    const [dx, dy] = DIR_VEC[this.facing]
    const at = (d: number) => [(cx + dx * d) << 8, (cy + dy * d) << 8] as const
    switch (kind) {
      case 'ki': {
        const [x, y] = at(10)
        w.add(new KiBlast(this.ctx, this, this.facing, 0, x, y))
        break
      }
      case 'ball':
      case 'star': {
        const [x, y] = at(8)
        w.add(new EnemyShot(this.ctx, this, this.facing, x, y, kind === 'star'))
        break
      }
      case 'lob': {
        const p = this.playerCenter()
        if (!p) return
        const [x, y] = at(4)
        w.add(new LobRock(this.ctx, this, this.facing, x, y, p))
        break
      }
      case 'laser': {
        const [mx, my] = LASER_MUZZLE[this.facing]
        w.add(new Laser(this.ctx, this, this.facing, (cx + mx) << 8, (cy + my) << 8, param || 64))
        break
      }
      case 'beam':
      case 'beamTao': {
        const [x, y] = at(16)
        w.add(new Beam(this.ctx, this, this.facing, 0, x, y + (dx ? 0 : 0), 'boss', 10, 0x3000, 120))
        break
      }
    }
  }
  sprayParticle() {
    const [mx, my] = SPRAY_MOUTH[this.facing]
    this.w.add(new SprayParticle(this.ctx, this, this.facing, (this.px + mx) << 8, (this.py + my) << 8, this.w))
  }
  /** DrainGrab enter: grab the player for 240 frames */
  drainStart() {
    const p = this.player
    if (!p) return false
    p.grab(this.w, this, 240)
    this.setAnim(['special_2', 'attack1'], true, true)
    this.drainHits = 0
    return true
  }
  /** hold frame f: hit at frame 8 (scale 256) then every 16 frames (scale 128), Cell heals by the damage */
  drainTick(f: number) {
    const p = this.player
    if (!p || p.grabbedBy !== this) return false
    if (f === 8 || (f > 8 && (f - 8) % 16 === 0)) {
      const res = computeDamage(ATTACKS[2], this.stats, p.stats, f === 8 ? 256 : 128, this.w.rng)
      p.applyDamageDirect(this.w, res.dmg, res.crit)
      this.heal(res.dmg)
    }
    return true
  }
  selfDestruct() {
    const [cx, cy] = this.center()
    this.ctx.spawnFx(this.w, 'fx_086e1590', cx, cy, [0, 1, 2, 3], 4)
    broadcast(this.w, MSG_ATTACK, { team: 2, rect: { x0: cx - 16, y0: cy - 16, x1: cx + 16, y1: cy + 16 }, facing: this.facing, scale: 0x400, attack: ATTACKS[3], attacker: this })
  }
  /** death node fn2: exp award, on-death event, explosion (size 0x28/0x40) unless quiet; removed */
  die(explode: boolean, size: number) {
    if (this.removed) return
    const w = this.w
    this.finishDeath(w)
    if (explode) {
      const [cx, cy] = rectCenter(this.hurtRect())
      this.ctx.spawnFx(w, 'fx_086ff09c', cx, cy, size >= 0x40 ? [0, 1, 2, 3, 3] : [0, 1, 2, 3], size >= 0x40 ? 6 : 4)
      this.removed = true
    }
  }
  private finishDeath(w: World) {
    if (!this.defeated) {
      this.defeated = true
      w.stats.kills++
      if (this.ctx.opts.exp) this.ctx.player?.gainExp(w, this.cls.exp)
      w.logEvent('info', `${this.label} defeated`)
    }
  }
  applyDamage(msg: HitMsg) {
    const w = this.w
    this.lastHitDir = msg.facing
    const dmg = msg.dmg ?? 0
    if (dmg <= 0 || this.hp <= 0) return
    this.ctx.damagePopup(w, this, dmg, !!msg.crit, false)
    if (msg.attacker.kind === 'player') {
      w.stats.damageDealt += Math.min(dmg, this.hp)
      w.stats.hitsDealt++
      if (msg.crit) w.stats.crits++
      w.stats.maxHit = Math.max(w.stats.maxHit, dmg)
    }
    w.logEvent('damage', `atk${msg.attack.id} -> ${this.label}: ${dmg}${msg.crit ? ' CRIT' : ''} (raw ${msg.raw}) hp ${Math.max(0, this.hp - dmg)}`)
    this.hp -= dmg
    this.flash = 12
    if (this.hp <= 0) {
      this.hp = 0
      this.onZeroHp()
    }
  }
  /** class row +0x14 death node */
  private onZeroHp() {
    const w = this.w
    switch (this.cls.deathNode) {
      case '0x86d5694': // Eggbot: GotoState(44) (flash, pose 40, self-destruct burst, explode)
        this.finishDeath(w)
        this.ai.replaceTop(44)
        break
      case '0x86ff3c4': // bosses: exp + on-death event, no explosion; the death cutscene takes over in the game
        this.finishDeath(w)
        this.ko = true
        this.hittable = false
        this.setAnim(['ko', 'knockdown', 'hurt'], true, false)
        break
      case '0x86d5654':
        this.die(true, 0x40)
        break
      default:
        this.die(true, 0x28)
    }
  }
  heal(n: number) {
    this.hp = Math.min(this.maxHp, this.hp + n)
    if (n > 0) this.w.popup('+' + n, this.px + 8, this.py - 30, '#7f7', 30)
  }
  knockbackBlocked(msg: HitMsg) {
    const [dx, dy] = DIR_VEC[msg.facing]
    return !this.fits(this.w, this.px + dx * 16, this.py + dy * 16)
  }
  hitFlash() {
    this.flash = 12
  }
  glowFlash() {
    this.glow = 12
  }
  stunAnim(on: boolean) {
    this.stunned = on
    this.setAnim(on ? ['stunned', 'hurt', 'idle'] : 'idle', true, true)
  }
  faceAttacker(msg: HitMsg) {
    const [ax, ay] = msg.attacker.center()
    const [x, y] = this.center()
    this.facing = facingToward(ax - x, ay - y)
  }
  trace(node: AiNode, what: string) {
    this.traceLog.push(`${this.w.frame} ${node.type} ${what}`)
    if (this.traceLog.length > 20) this.traceLog.shift()
  }

  // ---- messages ------------------------------------------------------------------------------------------------
  onMessage(w: World, type: number, msg: HitMsg): boolean {
    if (this.ko || this.removed) return false
    if (msg.team === 0 || !this.hittable) return false
    if (!hitOverlaps(msg, this)) return false
    if (type === MSG_PROBE) return true
    if (type !== MSG_ATTACK) return false
    if (this.defeated) return true
    const res = computeDamage(msg.attack, msg.attacker.stats, this.stats, msg.scale, w.rng)
    msg.crit = res.crit
    msg.raw = res.raw
    msg.dmg = res.dmg
    this.ai.onHit(msg)
    return true
  }

  draw(r: Renderer, w: World) {
    super.draw(r, w)
    if (this.glow > 0 && this.glow & 2 && this.sprite && this.anim) {
      const g = r.ctx
      g.globalCompositeOperation = 'lighter'
      this.sprite.draw(g, this.anim, this.facing, r.sx(this.px), r.sy(this.py), this.animT, { alpha: 0.6, loop: this.animLoop })
      g.globalCompositeOperation = 'source-over'
    }
    if (this.stunned && !this.ko) {
      // dizzy stars over the head (anim controller 0x03002230)
      const hr = this.hurtRect()
      const t = w.frame
      for (let i = 0; i < 3; i++) {
        const a = t / 8 + (i * Math.PI * 2) / 3
        drawText(r.ctx, '+', r.sx((hr.x0 + hr.x1) / 2 + Math.cos(a) * 7) - 1, r.sy(hr.y0 - 4 + Math.sin(a) * 2), '#ffd23f', null)
      }
    }
  }

  debugLabel() {
    return `${this.label} ${this.hp}/${this.maxHp}\n${this.ai.describe()}`
  }
}
