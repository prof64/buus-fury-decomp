// Buu's Fury enemy (enemy table 0x08711274): type stats, HP stored <<10 (obj+0x28C), the AI controller (ai.ts),
// the hit path (aux tree -> ApplyDamage -> ComputeDamage 0x0800E9D8 -> HP; death node = type +0x14 word +8,
// 0x0800FA2A), enemy melee geometry (0x0802A0C4), and the helpers the AI action nodes use (shots, teleports,
// summons, Kid Buu split/merge, Janemba parts and hands, worm segments).

import { DIR_VEC, facingToward, inflate, overlaps, type Dir, type Rect } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import type { World } from '../../core/world'
import { drawText } from '../../core/pixelfont'
import { Actor, broadcast, hitsBody, type HitMsg } from './actor'
import { AiController } from './ai'
import type { BuuCtx, EnemyLike, PlayerLike } from './context'
import { ATK, computeDamage, type AttackType } from './damage'
import type { EnemyType } from './data'
import { StraightShot, type GrowBeam } from './projectiles'
import { audioEvent } from '../../audio/bus'

const GUN_MUZZLE = [[4, 4], [13, -24], [-8, -8], [24, -8]]
const GUN_BOX: [number, number, number, number][] = [[3, 10, 8, 16], [3, 5, 8, 16], [5, 4, 16, 8], [10, 4, 16, 8]]
const WAVE_MUZZLE = { soldier: [[2, 8], [9, -28], [-16, -11], [24, -11]], turret: [[4, 4], [4, -25], [-13, -14], [22, -13]] }
const DIAG = [[-316, 316], [316, -316], [-316, -316], [316, 316]]

export class Enemy extends Actor implements EnemyLike {
  kind = 'enemy'
  ctx: BuuCtx
  type: EnemyType
  label: string
  hp1024: number
  maxHp1024: number
  ai: AiController
  w!: World
  counter = 0
  mark1 = -100000
  mark2 = -100000
  stateStamp = 0
  stunned = false
  lastHitDir: Dir = 0
  vx = 0
  vy = 0
  moving = false
  slot = 0
  children: Enemy[] = []
  parentE: Enemy | null = null
  defeated = false
  ko = 0
  powered = false
  beam: GrowBeam | null = null
  drainDone = false
  spawned = 0
  enzTimer = 120
  shield = 0
  volleyball = false
  /** non-combat decoration (Janemba body parts): never hit, never counted */
  part = false
  /** spawned by the fight's phase list (victory needs these defeated) */
  primary = false
  /** worm segments */
  next: Enemy | null = null
  prev: Enemy | null = null
  cmds: number[] = []
  wormDir: Dir = 0
  contactCd = 0
  partOffset: [number, number] | null = null
  scriptHits = 0
  constructor(ctx: BuuCtx, type: EnemyType, label?: string) {
    super()
    this.ctx = ctx
    this.type = type
    this.label = label ?? type.name
    this.hp1024 = this.maxHp1024 = type.hp * 1024
    this.stats = { STR: type.STR, POW: type.POW, END: type.END, END2: type.END, resM: type.resM, resE: type.resE, critBonus: 0, blockFactor: 0x1000 }
    this.setSprite(type.sprite)
    this.ai = new AiController(this)
    this.setAnim('idle', true, true)
  }

  get hp() {
    return Math.ceil(this.hp1024 / 1024)
  }
  depth() {
    // body parts are drawn over their boss
    return this.part && this.parentE ? this.parentE.py + 1 : this.py
  }
  get maxHp() {
    return this.type.hp
  }
  get player(): PlayerLike | null {
    const p = this.ctx.player
    return p && !p.dead ? p : null
  }

  start(w: World, state = this.type.ai) {
    this.w = w
    // obj+0x1E8 / +0x1EC are stamped with the frame counter at creation (Janemba waits 240 f from its spawn)
    this.mark1 = this.mark2 = this.stateStamp = w.frame
    this.ai.push(state)
  }

  update(w: World) {
    this.w = w
    this.tickAnim()
    if (this.contactCd > 0) this.contactCd--
    if (this.defeated) {
      this.ko++
      if (this.ko > 150) this.alpha = Math.max(0, 1 - (this.ko - 150) / 30)
      if (this.ko > 180) this.removed = true
      return
    }
    if (this.part) {
      const par = this.parentE
      if (!par || par.removed || par.defeated) {
        this.removed = true
        return
      }
      this.x = par.x + (this.partOffset![0] << 8)
      this.y = par.y + (this.partOffset![1] << 8)
      this.z = par.z
      this.facing = par.facing
      return
    }
    this.moving = false
    this.ai.tick()
    if (!this.anim || this.anim.name === 'idle' || this.anim.name === 'walk') {
      if (this.moving && this.anim?.name !== 'walk') this.setAnim('walk', false, true)
      else if (!this.moving && this.anim?.name === 'walk') this.setAnim('idle', false, true)
    }
  }

  // ---- geometry helpers -------------------------------------------------------------------------------------
  playerAlive() {
    return !!this.player
  }
  toPlayer(): [number, number] {
    const p = this.player
    if (!p) return [9999, 9999]
    const [px, py] = p.center()
    const [x, y] = this.center()
    return [px - x, py - y]
  }
  dist2() {
    if (!this.player) return Infinity
    const [dx, dy] = this.toPlayer()
    return dx * dx + dy * dy
  }
  facePlayer() {
    if (!this.player) return
    const [dx, dy] = this.toPlayer()
    this.facing = facingToward(dx, dy)
  }
  muzzle(d: number): [number, number] {
    const [cx, cy] = this.center()
    const [dx, dy] = DIR_VEC[this.facing]
    return [cx + dx * d, cy + dy * d]
  }
  muzzleFix(kind: 'soldier' | 'turret'): [number, number] {
    const [ox, oy] = WAVE_MUZZLE[kind][this.facing]
    return [(this.px + ox) << 8, (this.py + oy) << 8]
  }
  inMeleeRange() {
    const p = this.player
    if (!p || !p.hittable) return false
    if (!overlaps(p.bodyBox(), inflate(this.bodyBox(), 4))) return false
    this.facePlayer()
    return true
  }
  playerAttackingMe() {
    const p = this.player
    return !!p && p.attacking() && overlaps(inflate(p.bodyBox(), 8), this.bodyBox())
  }
  playerInFront(reach: number) {
    const p = this.player
    if (!p || !p.hittable) return false
    const [dx, dy] = this.toPlayer()
    const f = facingToward(dx, dy)
    const b = this.bodyBox()
    const r: Rect = f === 3 ? { x0: b.x1, y0: b.y0, x1: b.x1 + reach, y1: b.y1 } : f === 2 ? { x0: b.x0 - reach, y0: b.y0, x1: b.x0, y1: b.y1 } : f === 0 ? { x0: b.x0, y0: b.y1, x1: b.x1, y1: b.y1 + reach } : { x0: b.x0, y0: b.y0 - reach, x1: b.x1, y1: b.y0 }
    if (!overlaps(r, p.bodyBox())) return false
    this.facing = f
    return true
  }
  inPlayerBeamLine() {
    const p = this.player
    if (!p || !p.charging()) return false
    const pb = p.bodyBox()
    const [dx, dy] = DIR_VEC[p.facing]
    const cor: Rect = dx > 0 ? { x0: pb.x1, y0: pb.y0 - 4, x1: pb.x1 + 240, y1: pb.y1 + 4 } : dx < 0 ? { x0: pb.x0 - 240, y0: pb.y0 - 4, x1: pb.x0, y1: pb.y1 + 4 } : dy > 0 ? { x0: pb.x0 - 4, y0: pb.y1, x1: pb.x1 + 4, y1: pb.y1 + 240 } : { x0: pb.x0 - 4, y0: pb.y0 - 240, x1: pb.x1 + 4, y1: pb.y0 }
    return overlaps(cor, this.bodyBox())
  }
  blastIncoming() {
    const p = this.player
    const b = p?.blast
    if (!b || b.removed || (b.dir ^ 1) !== this.facing) return false
    const eb = this.bodyBox()
    const [dx, dy] = DIR_VEC[this.facing]
    const cor: Rect = dx > 0 ? { x0: eb.x1, y0: eb.y0 - 4, x1: eb.x1 + 240, y1: eb.y1 + 4 } : dx < 0 ? { x0: eb.x0 - 240, y0: eb.y0 - 4, x1: eb.x0, y1: eb.y1 + 4 } : dy > 0 ? { x0: eb.x0 - 4, y0: eb.y1, x1: eb.x1 + 4, y1: eb.y1 + 240 } : { x0: eb.x0 - 4, y0: eb.y0 - 240, x1: eb.x1 + 4, y1: eb.y0 }
    return overlaps(cor, b.rect())
  }

  // ---- map collision (enemy grid, map object +0xB0) ---------------------------------------------------------------
  /** tiles this enemy marked blocked on the enemy grid (obj+0x291 == 0 "attached"), x0 y0 x1 y1 px */
  marked: [number, number, number, number] | null = null
  /** on the enemy grid (obj+0x291 == 0). Detach 0x0800F924 is called by the worm body (0x0802D76C, its segments
   *  replay the head's steps without a test), the grab / hug controller (anim 0x11, 0x0804E53C) and the anim 0x13
   *  controller (0x0804E938, rise / vanish); Attach 0x0800F954 when those end. Modelled: body parts, worm segments,
   *  the enemy holding the player, airborne (height offset), defeated (INFERRED mapping of the controllers) */
  attached(): boolean {
    return !this.part && !this.prev && !this.defeated && !this.removed && this.z === 0 && this.ctx.player?.heldBy !== this
  }
  /** vfunc +0x48(0) = 0x0800F740(enemy, 0): mark the footprint's tiles blocked */
  markSelf() {
    const c = this.ctx.col
    if (!c || this.marked || !this.attached()) return
    this.marked = [this.px, this.py, this.px + this.footW, this.py + this.footH]
    c.mark('enemy', ...this.marked, 0)
  }
  /** vfunc +0x48(1): free the marked tiles (the game sets them passable; here they get the terrain value back, so a
   *  footprint pushed onto a wall tile cannot open a hole in the grid) */
  unmarkSelf() {
    const c = this.ctx.col
    if (!c || !this.marked) return
    c.restore('enemy', ...this.marked)
    this.marked = null
  }
  /** keep the marks on the current footprint (positions set directly: spawns, teleports, followers) */
  syncMark() {
    const m = this.marked
    if (m && this.attached() && m[0] === this.px && m[1] === this.py) return
    this.unmarkSelf()
    this.markSelf()
  }
  /** vfunc +0x54 = 0x0800F9EE(enemy, pos): footprint on the enemy grid; a detached enemy (obj+0x291 != 0) always fits */
  passable(_w: World, x: number, y: number): boolean {
    const c = this.ctx.col
    if (!c || !this.attached()) return true
    const m = this.marked
    if (m) c.restore('enemy', ...m)
    const ok = c.test('enemy', x, y, x + this.footW, y + this.footH)
    if (m) c.mark('enemy', ...m, 0)
    return ok
  }
  /** enemy step (ChasePlayer's step 0x0803B9xx, the back-off / step-away steps 0x08018C3A / 0x0801CBA1): unmark, try the full move, else only
   *  the x part (fx), else only the y part (fy), re-mark. true = the full move was taken */
  tryMove(vx: number, vy: number, fx = vx, fy = vy): boolean {
    const w = this.w
    this.unmarkSelf()
    let ok = false
    this.stepped = true
    if (this.fits(w, (this.x + vx) >> 8, (this.y + vy) >> 8)) {
      this.x += vx
      this.y += vy
      ok = true
    } else if (fx && this.fits(w, (this.x + fx) >> 8, this.py)) this.x += fx
    else if (fy && this.fits(w, this.px, (this.y + fy) >> 8)) this.y += fy
    else this.stepped = false
    this.markSelf()
    return ok
  }
  /** the last tryMove moved at all (full or one axis) */
  stepped = false
  moveBy(_w: World, vx: number, vy: number): boolean {
    return this.tryMove(vx, vy)
  }

  // ---- movement -------------------------------------------------------------------------------------------------
  walkBy(vx: number, vy: number): boolean {
    if (!vx && !vy) return true
    this.moving = true
    return this.moveBy(this.w, Math.round(vx), Math.round(vy))
  }
  /** footprint-rect centre (0x0800FDFA rect: (x0 + x1) >> 1) */
  private static fc(a: Actor): [number, number] {
    return [(a.px * 2 + a.footW) >> 1, (a.py * 2 + a.footH) >> 1]
  }
  /** step directly away from the player: 0x08018C3A (BackOffFacingPlayer, labelled StepTowardPlayer in the data)
   *  keeps facing the player, 0x0801CBA1 (StepAwayFromPlayer) turns away. d = player footprint centre - own centre;
   *  m = max(|dx|, |dy|); pos -= trunc(d * speed / m) (`speed` on the major axis), blocked -> one axis at full
   *  speed (x - (dx >= 0 ? speed : -speed)), then y. Returns true (node finished) when no try moved; m = 0 -> no
   *  move, running. Emulator-verified (data/battle/buu_step_traces.json). */
  stepFromPlayer(speed: number, faceToward: boolean): boolean {
    const p = this.player
    if (!p) return true
    const [px, py] = Enemy.fc(p)
    const [x, y] = Enemy.fc(this)
    const dx = px - x
    const dy = py - y
    const adx = Math.abs(dx)
    const ady = Math.abs(dy)
    let m: number
    if (adx > ady) {
      m = adx
      this.facing = faceToward ? (dx < 0 ? 2 : 3) : dx > 0 ? 2 : 3
    } else {
      m = ady
      this.facing = faceToward ? (dy < 0 ? 1 : 0) : dy > 0 ? 1 : 0
    }
    if (!m) return false
    this.moving = true
    this.tryMove(-Math.trunc((dx * speed) / m), -Math.trunc((dy * speed) / m), dx >= 0 ? -speed : speed, dy >= 0 ? -speed : speed)
    return !this.stepped
  }
  /** WalkToPlayer update 0x0802CDE8 (Spopovich): face the player on the dominant axis; within 8 px (major axis of
   *  the centre offset) stand; pos += trunc(d * speed / m) (a one-axis retry only for steps > 4 px, i.e. never);
   *  no step while the walk animation shows frame 1 or 4 (obj+0x11);
   *  blocked -> detour: n = rand(32) - 16 when 0, then sidestep `speed` on the minor axis (n < 0: -, n++; else +, n--)
   *  each frame until n is back to 0 or the sidestep is blocked (n = 0); while n != 0 no direct step is tried. */
  /** index of the shown animation step (the engine's obj+0x11 frame number) */
  animFrame(): number {
    if (!this.anim) return 0
    const step = this.anim.stepAt(this.facing, this.animT, this.animLoop)
    return step ? this.anim.steps(this.facing).indexOf(step) : 0
  }
  walkToPlayer(speed: number, st: { n: number }, animFrame = this.animFrame()): boolean {
    // obj+0x11 = current animation frame: no step on frames 1 and 4 of the walk cycle
    if (animFrame === 1 || animFrame === 4) return false
    const p = this.player
    if (!p) return false
    const [px, py] = Enemy.fc(p)
    const [x, y] = Enemy.fc(this)
    const dx = px - x
    const dy = py - y
    const adx = Math.abs(dx)
    const ady = Math.abs(dy)
    let m: number
    if (adx > ady) {
      m = adx
      this.facing = dx > 0 ? 3 : 2
    } else {
      m = ady
      this.facing = dy > 0 ? 0 : 1
    }
    if (m < 8) return false
    this.moving = true
    const w = this.w
    this.unmarkSelf()
    const qx = Math.trunc((dx * speed) / m)
    const qy = Math.trunc((dy * speed) / m)
    let moved = false
    if (st.n === 0) {
      if (this.fits(w, (this.x + qx) >> 8, (this.y + qy) >> 8)) {
        this.x += qx
        this.y += qy
        moved = true
      } else if (Math.abs(qx) > 0x400 && this.fits(w, (this.x + qx) >> 8, this.py)) {
        this.x += qx
        moved = true
      } else if (Math.abs(qy) > 0x400 && this.fits(w, this.px, (this.y + qy) >> 8)) {
        this.y += qy
        moved = true
      }
    }
    if (!moved) {
      if (st.n === 0) st.n = w.rng.rand(32, 'walk detour') - 16
      let nx = this.x
      let ny = this.y
      const s = st.n < 0 ? -speed : speed
      st.n += st.n < 0 ? 1 : -1
      if (adx >= ady) ny += s
      else nx += s
      if (this.fits(w, nx >> 8, ny >> 8)) {
        this.x = nx
        this.y = ny
      } else st.n = 0
    }
    this.markSelf()
    return false
  }
  /** Wander update 0x0802B708: each frame chance(turn_p) -> facing = rand(4); step `speed` in the facing (table
   *  0x08752EB4 = down, up, left, right); blocked -> facing = rand(4) (no move). Never finishes. */
  wanderStep(speed: number, turnP: number) {
    const w = this.w
    this.moving = true
    if (w.rng.chance(turnP, 'wander turn')) this.facing = w.rng.rand(4, 'wander dir') as Dir
    const [dx, dy] = DIR_VEC[this.facing]
    const vx = (dx * 256 * speed) >> 8
    const vy = (dy * 256 * speed) >> 8
    this.unmarkSelf()
    if (this.fits(w, (this.x + vx) >> 8, (this.y + vy) >> 8)) {
      this.x += vx
      this.y += vy
    } else this.facing = w.rng.rand(4, 'wander dir') as Dir
    this.markSelf()
  }
  /** normalised move toward the player; true when within `stop` px (centre distance) */
  moveToward(speed: number, stop: number, face: boolean, ignoreWalls = false): boolean {
    const p = this.player
    if (!p) return false
    const [dx, dy] = this.toPlayer()
    const d = Math.hypot(dx, dy)
    if (face) this.facing = facingToward(dx, dy)
    if (d <= stop) return true
    const vx = (dx * speed) / d
    const vy = (dy * speed) / d
    if (ignoreWalls) {
      this.moving = true
      this.x += Math.round(vx)
      this.y += Math.round(vy)
      this.clampInside(this.w)
    } else this.walkBy(vx, vy)
    return false
  }
  moveToPoint(x: number, y: number, speed: number): boolean {
    const dx = x - this.px
    const dy = y - this.py
    if (Math.abs(dx) <= 8 && Math.abs(dy) <= 8) return true
    const d = Math.hypot(dx, dy)
    this.facing = facingToward(dx, dy)
    const [ox, oy] = [this.x, this.y]
    if (!this.walkBy((dx * speed) / d, (dy * speed) / d)) {
      // blocked by the arena edge or a wall: the target is unreachable here -> accept the closest point
      const b = this.w.arena.bounds
      if (x < b.x0 || y < b.y0 || x + this.footW > b.x1 || y + this.footH > b.y1) return true
      if (this.x === ox && this.y === oy) return true
    }
    return false
  }
  /** AlignWithPlayer: strafe on the minor axis by speed/256 px per frame; done when the minor offset < 3 px */
  align(speed: number): boolean {
    if (!this.player) return false
    const [dx, dy] = this.toPlayer()
    if (Math.abs(dx) > Math.abs(dy)) {
      if (Math.abs(dy) < 3) {
        this.facing = dx > 0 ? 3 : 2
        return true
      }
      this.facing = dy > 0 ? 0 : 1
      if (!this.walkBy(0, Math.sign(dy) * speed)) {
        this.facing = dx > 0 ? 3 : 2
        return true
      }
    } else {
      if (Math.abs(dx) < 3) {
        this.facing = dy > 0 ? 0 : 1
        return true
      }
      this.facing = dx > 0 ? 3 : 2
      if (!this.walkBy(Math.sign(dx) * speed, 0)) {
        this.facing = dy > 0 ? 0 : 1
        return true
      }
    }
    return false
  }

  // ---- attacks ----------------------------------------------------------------------------------------------------
  /** 0x0802A0C4: face the player on the dominant axis, build the half-box on that side, hit once */
  strikeRect(face = true): Rect | null {
    const p = this.player
    if (!p) return null
    const B = this.bodyBox()
    const F = this.footBox()
    const [cx, cy] = this.center()
    let f = this.facing
    if (face) {
      const [dx, dy] = this.toPlayer()
      f = facingToward(dx, dy)
      this.facing = f
    }
    switch (f) {
      case 3:
        return { x0: cx, y0: cy, x1: B.x1 + 3, y1: B.y1 + 3 }
      case 2:
        return { x0: B.x0 - 4, y0: cy, x1: cx, y1: B.y1 + 3 }
      case 0:
        return { x0: F.x0, y0: F.y0, x1: F.x1, y1: F.y1 - 6 }
      default:
        return { x0: B.x0, y0: B.y0, x1: B.x1, y1: cy }
    }
  }
  meleeStrike(mult: number, face: boolean, adjacent = false, atk: AttackType = ATK[2]): boolean {
    const p = this.player
    if (!p || !p.hittable) return false
    let rect = this.strikeRect(face)
    if (adjacent) {
      // ChargedPunch / IfMeleeHit: the rect adjacent to the facing side
      const b = this.bodyBox()
      const h = 16
      rect = this.facing === 3 ? { x0: b.x1, y0: b.y0, x1: b.x1 + h, y1: b.y1 } : this.facing === 2 ? { x0: b.x0 - h, y0: b.y0, x1: b.x0, y1: b.y1 } : this.facing === 0 ? { x0: b.x0, y0: b.y1 - 8, x1: b.x1, y1: b.y1 + h } : { x0: b.x0, y0: b.y0 - h, x1: b.x1, y1: b.y0 + 8 }
    }
    if (!rect) return false
    return p.onMessage(this.w, { team: 0, rect, dir: this.facing, mult, atk, attacker: this, source: this })
  }
  /** HeavyStrike 0x0802F77C: rect from the body edge toward the player, reaching ~60 px (centre to centre) */
  heavyStrike() {
    const p = this.player
    if (!p || !p.hittable) return
    this.facePlayer()
    const b = this.bodyBox()
    const [cx, cy] = this.center()
    // measured: hits at dx <= 60 px centre to centre, misses at 65 (Goku side-on body half-width 4)
    const R = 57
    const rect: Rect = this.facing === 3 ? { x0: cx, y0: b.y0, x1: cx + R, y1: b.y1 } : this.facing === 2 ? { x0: cx - R, y0: b.y0, x1: cx, y1: b.y1 } : this.facing === 0 ? { x0: b.x0, y0: cy, x1: b.x1, y1: cy + R } : { x0: b.x0, y0: cy - R, x1: b.x1, y1: cy }
    p.onMessage(this.w, { team: 0, rect, dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this })
  }
  /** contact damage (worm, blood cell, mini-yakon): standard melee x1.0 with the body box */
  contactHit(): boolean {
    const p = this.player
    if (!p || !p.hittable || this.contactCd > 0) return false
    if (!overlaps(this.bodyBox(), p.bodyBox())) return false
    const ok = p.onMessage(this.w, { team: 0, rect: this.bodyBox(), dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this })
    if (ok) this.contactCd = 20
    return ok
  }
  /** IfContactExplode (swarm unit 0x0D): energy x4.0 with its own box against the world */
  contactExplode(): boolean {
    const p = this.player
    if (!p || !overlaps(this.bodyBox(), p.bodyBox())) return false
    return p.onMessage(this.w, { team: 0, rect: this.bodyBox(), dir: this.facing, mult: 0x1000, atk: ATK[1], attacker: this, source: this })
  }
  areaHit(rect: Rect, atk: AttackType, mult: number): boolean {
    const p = this.player
    if (!p) return false
    return p.onMessage(this.w, { team: 0, rect, dir: this.facing, mult, atk, attacker: this, source: this })
  }
  /** launched volleyball: hits other enemies (attacker stats: the player's, INFERRED) */
  areaHitEnemies(rect: Rect, p: PlayerLike) {
    broadcast(this.w, { team: 1, rect, dir: this.lastHitDir, mult: 409, atk: ATK[3], attacker: p, source: this })
  }
  shot(kind: 'gun' | 'rifle' | 'star' | 'wrap' | 'burst' | 'shell' | 'spit') {
    const w = this.w
    const f = this.facing
    const [mx, my] = GUN_MUZZLE[f]
    const at: [number, number] = [(this.px + mx) << 8, (this.py + my) << 8]
    const [cx, cy] = this.center()
    const [dx, dy] = DIR_VEC[f]
    const mk = (dir: Dir, x: number, y: number, vx: number, vy: number, box: [number, number, number, number], extra: Partial<import('./projectiles').StraightOpts> = {}) =>
      w.add(new StraightShot(this.ctx, this, dir, 0, x, y, { vx, vy, box, atk: ATK[1], mult: 1024, color: ['#ffffff', '#ff9040'], radius: 3, name: kind, ...extra }))
    switch (kind) {
      case 'gun':
        mk(f, ...at, dx * 672, dy * 672, GUN_BOX[f], { fragile: true, radius: 2, color: ['#fff8c0', '#ffa020'], fx: { id: 'fx_0874e968', frame: (d, t) => d + 4 * ((t >> 2) % 3) } })
        break
      case 'rifle':
        mk(f, ...at, dx * 448, dy * 448, [8, 8, 16, 16], { fragile: true, color: ['#e0ffe0', '#40c040'], fx: { id: 'fx_0874e614', frame: (d) => [0, 1, 2, 2][d], flip: (d) => d === 3 } })
        break
      case 'shell':
        mk(f, (cx + dx * 12) << 8, (cy + dy * 12) << 8, dx * 448, dy * 448, [8, 8, 16, 16], { fragile: true, radius: 4, color: ['#ffd0d0', '#d03020'], fx: { id: 'fx_0871e630', frame: (d) => [0, 1, 2, 2][d], flip: (d) => d === 3 } })
        break
      case 'star':
        mk(f, (cx + dx * 8) << 8, (cy + dy * 8) << 8, dx * 448, dy * 448, [4, 4, 8, 8], { fragile: true, explode: false, color: ['#ffffff', '#a0a0b0'], radius: 3, fx: { id: 'fx_08754a1c', frame: (_d, t) => (t >> 3) & 1 } })
        break
      case 'wrap':
      case 'burst': {
        const wrapHit = () => {
          const p = this.player
          if (p && p.wraps < 8) p.wraps++
        }
        const dirs: Dir[] = kind === 'wrap' ? [f] : [0, 1, 2, 3]
        for (const d of dirs) {
          const [ddx, ddy] = DIR_VEC[d]
          mk(d, (cx + ddx * 8) << 8, (cy + ddy * 8) << 8, ddx * 448, ddy * 448, d >= 2 ? [8, 4, 16, 8] : [4, 8, 8, 16], { onHitPlayer: wrapHit, explode: false, color: ['#f0e8c8', '#b0a070'] })
          if (kind === 'burst') mk(d, cx << 8, cy << 8, DIAG[d][0], DIAG[d][1], [8, 8, 16, 16], { onHitPlayer: wrapHit, explode: false, color: ['#f0e8c8', '#b0a070'] })
        }
        break
      }
      case 'spit': {
        const p = this.player
        if (!p) return
        const [tx, ty] = p.center()
        const ddx = tx - cx
        const ddy = ty - cy
        const m = Math.max(1, Math.abs(ddx), Math.abs(ddy))
        mk(f, cx << 8, (cy - 4) << 8, Math.round((ddx * 512) / m), Math.round((ddy * 512) / m), [4, 4, 8, 8], {
          color: ['#e0e0e0', '#707070'],
          onHitPlayer: (ww) => this.player?.hold(ww, this, 'stone', 297),
        })
        break
      }
    }
  }

  // ---- teleports / special -----------------------------------------------------------------------------------------
  ninjaTeleport() {
    const w = this.w
    for (let i = 0; i < 16; i++) {
      const dx = w.rng.rand(201, 'teleport dx') - 100
      const dy = w.rng.rand(201, 'teleport dy') - 100
      if (Math.max(Math.abs(dx), Math.abs(dy)) < 70) continue
      if (!this.fits(w, this.px + dx, this.py + dy)) continue
      this.ctx.puff(w, ...this.center())
      this.x += dx << 8
      this.y += dy << 8
      this.ctx.puff(w, ...this.center())
      return
    }
  }
  teleportBeside(side: number) {
    const p = this.player
    if (!p) return
    const OFF = [[0, 32], [0, -32], [-32, 0], [32, 0]]
    const [ox, oy] = OFF[side & 3]
    this.ctx.puff(this.w, ...this.center())
    const [pfx, pfy] = p.footCenter()
    this.x = (pfx + ox - (this.footW >> 1)) << 8
    this.y = (pfy + oy - (this.footH >> 1)) << 8
    this.clampInside(this.w)
    this.facePlayer()
    this.setAnim(19, true, false)
    this.ctx.puff(this.w, ...this.center())
  }
  kidBuuSplit() {
    const w = this.w
    for (let i = 0; i < 16; i++) {
      const dx = w.rng.rand(192, 'split dx') - 96
      const dy = w.rng.rand(192, 'split dy') - 96
      if (!this.fits(w, this.px + dx, this.py + dy) || !this.fits(w, this.px - dx, this.py - dy)) continue
      this.ctx.puff(w, ...this.center())
      const x0 = this.px
      const y0 = this.py
      this.x = (x0 + dx) << 8
      this.y = (y0 + dy) << 8
      this.hp1024 = (this.hp1024 + 1024) >> 1
      const copy = this.ctx.spawnEnemy(w, 0x3d, x0 - dx, y0 - dy, null, 0x12a) as Enemy | null
      if (copy) copy.hp1024 = this.hp1024
      w.logEvent('info', `Kid Buu splits: ${this.hp} + ${this.hp} HP`)
      return
    }
  }
  kidBuuMerge() {
    const other = this.ctx.liveEnemies().find((o) => o !== this && (o.type.type === 0x3c || o.type.type === 0x3d)) as Enemy | undefined
    if (!other) return
    this.hp1024 += other.hp1024
    const mx = (this.px + other.px) >> 1
    const my = (this.py + other.py) >> 1
    if (this.fits(this.w, mx, my)) {
      this.x = mx << 8
      this.y = my << 8
    }
    this.ctx.puff(this.w, ...this.center())
    this.ctx.puff(this.w, ...other.center())
    other.removed = true
    other.defeated = true
    other.primary = false
    this.w.logEvent('info', `Kid Buu merges back: ${this.hp} HP`)
  }
  janembaParts() {
    const defs: [number, number, number][] = [[0x34, -12, -22], [0x35, 44, -22], [0x37, 16, -41]]
    for (const [t, ox, oy] of defs) {
      const p = this.ctx.spawnEnemy(this.w, t, this.px + ox, this.py + oy, this, -1) as Enemy | null
      if (p) {
        p.part = true
        p.partOffset = [ox, oy]
        p.hittable = false
        p.layer = 0
      }
    }
  }
  handStrike(a: boolean) {
    const p = this.player
    if (!p) return
    const t = this.ctx.data.enemies[0x36]
    const h = new HandRise(this.ctx, t, p.px + (a ? 4 : -20), p.py, a ? 2 : 3)
    this.w.add(h)
  }
  topUpMinis() {
    const pts = [[104, 112], [120, 136], [152, 144], [184, 136], [200, 112]]
    const live = this.ctx.liveEnemies().filter((o) => o.type.type === 0x38).length
    const b = this.w.arena.bounds
    for (let i = 0; i < 5 - live; i++) {
      const [x, y] = pts[i]
      this.ctx.spawnEnemy(this.w, 0x38, Math.max(b.x0, Math.min(b.x1 - 16, x)), Math.max(b.y0, Math.min(b.y1 - 16, y)), this)
    }
  }
  landOnRock(): boolean {
    for (const r of this.w.entities) {
      if (r.kind !== 'rock' || r.removed) continue
      const rock = r as unknown as { hp: number; rect(): Rect; removed: boolean }
      if (!overlaps(rock.rect(), this.footBox())) continue
      rock.hp--
      this.ctx.puff(this.w, ...this.center())
      if (rock.hp <= 0) {
        rock.removed = true
        return true
      }
    }
    return false
  }
  wormSpawnBody() {
    const chain: Enemy[] = [this]
    for (let i = 1; i <= 5; i++) {
      const s = this.ctx.spawnEnemy(this.w, 0x8f, this.px, this.py - 12 * i, this) as Enemy | null
      if (!s) break
      // the ring buffer starts full of the head's direction so the segments keep their 12 px spacing (INFERRED)
      s.cmds = Array(12).fill(this.wormDir)
      chain.push(s)
    }
    for (let i = 1; i < chain.length; i++) {
      chain[i - 1].next = chain[i]
      chain[i].prev = chain[i - 1]
    }
  }
  wormHeadStep() {
    const w = this.w
    if (w.rng.rand(200, 'worm turn') === 0) {
      let d = this.wormDir ^ 2
      if (w.rng.rand(2, 'worm turn2')) d ^= 1
      this.wormDir = d as Dir
    }
    for (let k = 0; k < 4; k++) {
      const [dx, dy] = DIR_VEC[this.wormDir]
      if (this.fits(w, this.px + dx, this.py + dy)) {
        this.x += dx << 8
        this.y += dy << 8
        this.moving = true
        break
      }
      this.wormDir = ((this.wormDir + 1) & 3) as Dir
    }
    this.facing = this.wormDir
    this.next?.cmds.push(this.wormDir)
    this.contactHit()
  }
  wormBodyStep() {
    if (this.cmds.length >= 12) {
      const d = this.cmds.shift()! as Dir
      const [dx, dy] = DIR_VEC[d]
      this.x += dx << 8
      this.y += dy << 8
      this.facing = d
      this.moving = true
      this.next?.cmds.push(d)
    }
    this.contactHit()
  }

  // ---- damage --------------------------------------------------------------------------------------------------------
  /** enemy message handler (0x0800FAFA kind 0): ComputeDamage on every accepted hit first (0x0800FC2C), script #2
   *  (melee EP / HP hooks) when it did damage, then the top state's on-hit tree, whose ApplyDamage node takes the HP */
  onMessage(w: World, msg: HitMsg): boolean {
    if (msg.team === 0 || this.part || this.defeated || this.removed || !this.hittable) return false
    if (!hitsBody(msg, this)) return false
    this.w = w
    const blocking = this.blocking && (msg.dir ^ 1) === this.facing
    const res = computeDamage(msg.atk, msg.mult, msg.attacker.stats, this.stats, blocking, w.rng)
    msg.crit = res.crit
    msg.raw = res.raw
    msg.dmg = res.dmg
    if (msg.attacker.kind === 'player') audioEvent('hit')
    if (res.dmg > 0 && msg.attacker.kind === 'player') (msg.attacker as unknown as { onDealt?: (w: World, m: HitMsg, d: number) => void }).onDealt?.(w, msg, res.dmg)
    this.lastHitDir = msg.dir
    this.ai.onHit(msg)
    return true
  }
  /** ApplyDamage -> vfunc +0x58 (0x0800FA2A): HP -= the computed damage; true when this hit killed the enemy */
  takeHit(msg: HitMsg): boolean {
    const w = this.w
    if (this.hp1024 <= 0) return true
    const dmg = msg.dmg ?? 0
    const before = this.hp
    this.hp1024 -= dmg
    const shown = before - Math.max(0, this.hp)
    if (dmg > 0) {
      this.flash = 12
      if (msg.attacker.kind === 'player') {
        w.stats.hitsDealt++
        w.stats.damageDealt += shown
        if (msg.crit) w.stats.crits++
        w.stats.maxHit = Math.max(w.stats.maxHit, Math.round(dmg / 1024))
      }
      this.ctx.damagePopup(w, this, Math.max(1, Math.round(dmg / 1024)), !!msg.crit, false)
    }
    w.logEvent('damage', `${msg.attacker.kind} ${msg.atk.sub}x${msg.mult} -> ${this.label}: ${dmg}/1024${msg.crit ? ' CRIT' : ''} hp ${Math.max(0, this.hp)}`)
    if (this.hp1024 <= 0) {
      this.hp1024 = 0
      this.onDeath()
      return true
    }
    return false
  }
  takeDirect(w: World, n: number) {
    if (this.defeated) return
    this.hp1024 -= n * 1024
    this.ctx.damagePopup(w, this, n, false, false)
    if (this.hp1024 <= 0) {
      this.hp1024 = 0
      this.onDeath()
    }
  }
  heal(n1024: number) {
    if (n1024 <= 0) return
    this.hp1024 = Math.min(this.maxHp1024, this.hp1024 + n1024)
    if (n1024 >= 1024) this.w.popup('+' + Math.round(n1024 / 1024), this.center()[0], this.bodyBox().y0 - 4, '#7f7', 30)
  }
  scriptedDefeat() {
    this.hp1024 = 0
    this.onDeath()
  }
  /** type +0x14 node word +8 */
  private onDeath() {
    const w = this.w
    // owner died: its projectiles fly on harmlessly (messages 0x11/0x12) - Shot.ownerDead reads `defeated`
    const name = this.type.deathName
    if (name === 'special' && (this.type.type === 0x3c || this.type.type === 0x3d)) {
      // Kid Buu: the surviving body returns to phase 1
      const other = this.ctx.liveEnemies().find((o) => o !== this && (o.type.type === 0x3c || o.type.type === 0x3d)) as Enemy | undefined
      if (other) {
        this.defeated = true
        this.removed = true
        this.primary = false
        this.ctx.puff(w, ...this.center())
        other.primary = true
        other.ai.reset(this.ctx.data.enemies[0x3c].ai)
        return
      }
    }
    this.defeated = true
    this.hittable = false
    this.blocking = false
    w.stats.kills++
    this.ctx.player?.gainXp(w, this.type.exp)
    w.logEvent('info', `${this.label} defeated`)
    for (const s of w.entities) if (s.kind === 'beam' && (s as unknown as { owner: unknown }).owner === this) (s as unknown as GrowBeam).stop()
    if (name === 'Idle') {
      // bosses: no explosion; the defeat cutscene takes over in the game - the body stays down
      this.setAnim(['knockdown', 'ko', 'hurt'], true, false)
      this.ko = 0
    } else this.die(true)
  }
  die(explode: boolean) {
    if (!this.defeated) {
      this.defeated = true
      this.w.stats.kills++
    }
    if (explode) this.ctx.explosion(this.w, ...this.center())
    this.removed = true
  }

  draw(r: Renderer, w: World) {
    // the real ground shadow (sprite header) is drawn by Actor.draw; this soft one only marks airborne / hovering
    // objects whose sprite has none
    if ((this.z < -4 || this.type.type === 0x4 || this.type.type === 0x19 || this.type.type === 0x1d) && !this.shadowSprite()) {
      const g = r.ctx
      g.fillStyle = 'rgba(0,0,0,0.35)'
      g.beginPath()
      g.ellipse(r.sx(this.px + (this.footW >> 1)), r.sy(this.py + (this.footH >> 1)), this.footW >> 1, this.footH >> 1, 0, 0, Math.PI * 2)
      g.fill()
    }
    if (this.volleyball) {
      const [cx, cy] = this.center()
      const g = r.ctx
      g.fillStyle = '#f8f8f0'
      g.beginPath()
      g.arc(r.sx(cx), r.sy(cy + 6), 9, 0, Math.PI * 2)
      g.fill()
      g.strokeStyle = '#d04040'
      g.stroke()
      g.beginPath()
      g.arc(r.sx(cx) - 4, r.sy(cy + 6), 9, -0.6, 0.6)
      g.stroke()
      return
    }
    super.draw(r, w)
    if (this.powered && this.sprite && this.anim && w.frame & 8) {
      const g = r.ctx
      g.globalCompositeOperation = 'lighter'
      this.sprite.draw(g, this.anim, this.facing, r.sx(this.px), r.sy(this.py + this.z), this.animT, { alpha: 0.35, loop: this.animLoop })
      g.globalCompositeOperation = 'source-over'
    }
    if (this.stunned && !this.defeated) {
      const b = this.bodyBox()
      for (let i = 0; i < 3; i++) {
        const a = w.frame / 8 + (i * Math.PI * 2) / 3
        drawText(r.ctx, '+', r.sx((b.x0 + b.x1) / 2 + Math.cos(a) * 7) - 1, r.sy(b.y0 - 4 + Math.sin(a) * 2), '#ffd23f', null)
      }
    }
  }
  debugLabel() {
    return `${this.label} ${this.hp}/${this.maxHp}\n${this.ai.describe()}`
  }
}

/** Janemba Hand type 0x36 created under the player by a hand strike: one hit during its frames 16..55
 *  (body box, melee sub 2 x1.0, attacker = the hand, STR 22), gone after 57 frames */
export class HandRise extends Actor {
  kind = 'hand'
  ctx: BuuCtx
  t = 0
  hit = false
  constructor(ctx: BuuCtx, type: EnemyType, x: number, y: number, facing: Dir) {
    super()
    this.ctx = ctx
    this.x = x << 8
    this.y = y << 8
    this.facing = facing
    this.stats = { STR: type.STR, POW: type.POW, END: type.END, END2: type.END, resM: 128, resE: 128, critBonus: 0, blockFactor: 0x1000 }
    this.setSprite(type.sprite)
    this.setAnim(18, true, false)
    this.hittable = false
  }
  update(w: World) {
    this.tickAnim()
    this.t++
    const p = this.ctx.player
    if (!this.hit && p && !p.dead && this.t >= 16 && this.t <= 55) {
      if (p.onMessage(w, { team: 0, rect: this.bodyBox(), dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this })) this.hit = true
    }
    this.alpha = this.t < 12 ? this.t / 12 : this.t > 50 ? Math.max(0, (57 - this.t) / 7) : 1
    if (this.t >= 57) this.removed = true
  }
  onMessage(): boolean {
    return false
  }
}
