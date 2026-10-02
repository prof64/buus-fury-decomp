// Interpreter for the decoded LoG2 enemy AI (data/log2/ai.json): a stack of states (max depth 5) whose
// behaviour / on-hit roots are trees of ROM node structs. Semantics from re/log2/notes.md Part C §4-5 and the
// controller disassembly 0x0801AA46..0x0801AB64 (push = create+enter, replace-top = create, old.exit, new.enter,
// pop = exit, tick = top.update once per frame with the result ignored). Sequence advances one child per frame
// (0x0801B5C0). Every node type that occurs in ai.json is implemented; see NODE_NOTES for the ones whose
// details are inferred.

import { DIR_VEC, facingToward, type Dir } from '../../core/geom'
import type { World } from '../../core/world'
import type { HitMsg } from './actor'
import type { AiNode, AiState } from './data'

/** What the interpreter needs from an enemy (implemented by Enemy). */
export interface AiHost {
  w: World
  facing: Dir
  hittable: boolean
  classAiState: number
  /** centre of the collision box, integer px */
  center(): [number, number]
  /** player centre or null when no live player */
  playerCenter(): [number, number] | null
  playerAlive(): boolean
  playerSeen(r2: number): boolean
  inMeleeRange(b: boolean): boolean
  /** move by 24.8 velocity with wall sliding (full, x only, y only); false if completely blocked */
  slide(vx: number, vy: number, setAnim?: boolean): boolean
  setAnim(ref: string | number | (string | number)[], restart?: boolean, loop?: boolean): void
  animDone(): boolean
  hurtAnimLength(): number
  // actions
  meleeStrike(variants: number): void
  contactCheck(): void
  hitPlayerOnContact(): boolean
  shoot(kind: 'ki' | 'ball' | 'star' | 'lob' | 'laser' | 'beam' | 'beamTao', param?: number): void
  sprayParticle(): void
  drainStart(): boolean
  drainTick(frame: number): boolean
  selfDestruct(): void
  die(explode: boolean, size: number): void
  applyDamage(msg: HitMsg): void
  heal(n: number): void
  knockbackBlocked(msg: HitMsg): boolean
  hitFlash(): void
  glowFlash(): void
  stunAnim(on: boolean): void
  faceAttacker(msg: HitMsg): void
  lastHitDir: Dir
  /** debug */
  trace(node: AiNode, what: string): void
}

export interface RNode {
  node: AiNode
  alive: boolean
  enter(): void
  update(): boolean
  exit(): void
}

export const NODE_NOTES: Record<string, string> = {
  Patrol: 'no spawn waypoints in an arena: stands still (aggro checks still run)',
  IfKnockbackBlocked: 'probe distance (table 0x086FF40C) not decoded: 16 px along the hit direction',
  Pounce: 'contact hit + travel limit as decoded; leap arc is visual only',
  ThrowProjectile: 'throw anim length assumed 32 frames (spawn at frame 8 is decoded)',
  BeamAttack: 'muzzle offset table not decoded: beam starts 16 px in front of the hurt-rect centre',
  BeamAttackTao: 'muzzle offset table not decoded: beam starts 16 px in front of the hurt-rect centre',
}

export class AiController {
  stack: { state: number; rt: RNode }[] = []
  /** states entered, newest last (debug) */
  history: { frame: number; state: number }[] = []

  readonly host: AiHost
  readonly states: AiState[]
  constructor(host: AiHost, states: AiState[]) {
    this.host = host
    this.states = states}

  top() {
    return this.stack[this.stack.length - 1]
  }

  private note(state: number) {
    this.history.push({ frame: this.host.w.frame, state })
    if (this.history.length > 24) this.history.shift()
  }

  push(id: number) {
    if (this.stack.length >= 5) return
    const rt = create(this.states[id].behaviour, this)
    this.stack.push({ state: id, rt })
    this.note(id)
    rt.enter()
  }

  replaceTop(id: number) {
    if (!this.stack.length) return this.push(id)
    const rt = create(this.states[id].behaviour, this)
    const old = this.stack[this.stack.length - 1]
    old.rt.exit()
    this.stack[this.stack.length - 1] = { state: id, rt }
    this.note(id)
    rt.enter()
  }

  pop() {
    if (this.stack.length <= 1) return
    const old = this.stack.pop()!
    old.rt.exit()
  }

  tick() {
    const t = this.top()
    if (t) t.rt.update()
  }

  /** 0x0801AB2A: states[top].on_hit_root.onHit(node, entity, hit) */
  onHit(msg: HitMsg): boolean {
    const t = this.top()
    if (!t) return false
    return onHit(this.states[t.state].onHit, this, msg)
  }

  /** active node path for the debug overlay */
  describe(): string {
    const t = this.top()
    if (!t) return '-'
    const path: string[] = []
    let n: RNode | undefined = t.rt
    for (let i = 0; n && i < 6; i++) {
      path.push(n.node.type)
      n = (n as { active?: RNode }).active
    }
    return `st${t.state}${this.stack.length > 1 ? ' [' + this.stack.map((s) => s.state).join('>') + ']' : ''} ${path.join('/')}`
  }
}

// ---------------------------------------------------------------------------------------------------------------
// on-hit evaluation (word1 of each node)

function onHit(node: AiNode, c: AiController, msg: HitMsg): boolean {
  const h = c.host
  const kids = node.children ?? []
  switch (node.type) {
    case 'Sequence':
    case 'RaceFirst':
    case 'RaceAll':
      for (const k of kids) if (onHit(k, c, msg)) return true
      return false
    case 'ApplyDamage':
      if (msg.scale !== 0) h.applyDamage(msg)
      return false
    case 'IfAttackId': {
      const p = node.attack_id_or_type!
      if (msg.attack.id === p || msg.attack.type === p) return onHit(kids[0], c, msg)
      return false
    }
    case 'IfKnockbackBlocked':
      return onHit(h.knockbackBlocked(msg) ? kids[0] : kids[1], c, msg)
    case 'GotoState':
      c.replaceTop(node.state!)
      return false
    case 'PushState':
      c.push(node.state!)
      return false
    case 'PopState':
      c.pop()
      return false
    case 'SubState':
      return onHit(c.states[node.state!].onHit, c, msg)
    case 'ClassDefaultState':
      return onHit(c.states[h.classAiState].onHit, c, msg)
    case 'Succeed':
      return true
    case 'HitFlash':
      h.hitFlash()
      return false
    case 'GlowFlash':
      h.glowFlash()
      return false
    case 'HealFractionOfDamage':
      h.heal(Math.imul(node.heal_frac_65536!, msg.raw ?? 0) >> 16)
      return false
    case 'FacePlayer':
      h.faceAttacker(msg)
      return false
    case 'Chance':
      if (h.w.rng.chance(node.p_65536!, 'ai chance (hit)')) return onHit(kids[0], c, msg)
      return false
    case 'IfPlayerWithin': {
      const d2 = distSq(h)
      return onHit(d2 != null && d2 <= node.radius_sq! ? kids[0] : kids[1], c, msg)
    }
    case 'IfPlayerSeen':
      return onHit(h.playerSeen(node.radius_sq!) ? kids[0] : kids[1], c, msg)
    default:
      return false
  }
}

function distSq(h: AiHost): number | null {
  const p = h.playerCenter()
  if (!p) return null
  const [x, y] = h.center()
  const dx = p[0] - x
  const dy = p[1] - y
  return dx * dx + dy * dy
}

// ---------------------------------------------------------------------------------------------------------------
// behaviour runtime objects (word0 create -> [enter, update, exit])

abstract class Base implements RNode {
  alive = true
  readonly node: AiNode
  readonly c: AiController
  constructor(node: AiNode, c: AiController) {
    this.node = node
    this.c = c}
  get h() {
    return this.c.host
  }
  enter() {
    this.alive = true
    this.alive = true
  }
  abstract update(): boolean
  exit() {
    this.alive = false
  }
}

class Instant extends Base {
  private fn: () => void
  constructor(node: AiNode, c: AiController, fn: () => void) {
    super(node, c)
    this.fn = fn
  }
  update() {
    if (this.alive) this.fn()
    return true
  }
}

class Never extends Base {
  update() {
    return false
  }
}

class Sequence extends Base {
  kids: RNode[]
  i = 0
  constructor(node: AiNode, c: AiController) {
    super(node, c)
    this.kids = (node.children ?? []).map((k) => create(k, c))
  }
  get active() {
    return this.kids[this.i]
  }
  enter() {
    this.alive = true
    this.i = 0
    this.kids[0]?.enter()
  }
  update() {
    if (this.i >= this.kids.length) return true
    if (!this.kids[this.i].update()) return false
    this.kids[this.i].exit()
    this.i++
    if (this.i >= this.kids.length) return true
    if (this.alive) this.kids[this.i].enter()
    return false
  }
  exit() {
    if (this.alive && this.i < this.kids.length) this.kids[this.i].exit()
    this.alive = false
  }
}

class Race extends Base {
  kids: RNode[]
  private all: boolean
  constructor(node: AiNode, c: AiController, all: boolean) {
    super(node, c)
    this.all = all
    this.kids = (node.children ?? []).map((k) => create(k, c))
  }
  get active() {
    return this.kids[0]
  }
  enter() {
    this.alive = true
    for (const k of this.kids) k.enter()
  }
  update() {
    let done = false
    for (const k of this.kids) {
      if (k.update()) {
        done = true
        if (!this.all) return true
      }
      if (!this.alive) return done
    }
    return done
  }
  exit() {
    if (this.alive) for (const k of this.kids) k.exit()
    this.alive = false
  }
}

class Branch extends Base {
  kids: RNode[]
  cur = -1
  private cond: () => boolean
  constructor(node: AiNode, c: AiController, cond: () => boolean) {
    super(node, c)
    this.cond = cond
    this.kids = (node.children ?? []).map((k) => create(k, c))
  }
  get active() {
    return this.kids[this.cur]
  }
  enter() {
    this.alive = true
    this.cur = this.cond() ? 0 : 1
    this.kids[this.cur].enter()
  }
  update() {
    const want = this.cond() ? 0 : 1
    if (want !== this.cur) {
      this.kids[this.cur].exit()
      this.cur = want
      this.kids[this.cur].enter()
    }
    return this.kids[this.cur].update()
  }
  exit() {
    if (this.alive && this.cur >= 0) this.kids[this.cur].exit()
    this.alive = false
  }
}

class Chance extends Base {
  kid: RNode
  constructor(node: AiNode, c: AiController) {
    super(node, c)
    this.kid = create(node.children![0], c)
  }
  get active() {
    return this.kid
  }
  enter() {
    this.alive = true
    this.kid.enter()
  }
  update() {
    if (this.h.w.rng.chance(this.node.p_65536!, 'ai chance')) return this.kid.update()
    return false
  }
  exit() {
    if (this.alive) this.kid.exit()
    this.alive = false
  }
}

class InRange extends Base {
  kid: RNode
  private b: boolean
  constructor(node: AiNode, c: AiController, b: boolean) {
    super(node, c)
    this.b = b
    this.kid = create(node.children![0], c)
  }
  get active() {
    return this.kid
  }
  enter() {
    this.alive = true
    this.kid.enter()
  }
  update() {
    if (this.h.inMeleeRange(this.b)) return this.kid.update()
    return false
  }
  exit() {
    if (this.alive) this.kid.exit()
    this.alive = false
  }
}

class Wait extends Base {
  n = 0
  private pose: boolean
  constructor(node: AiNode, c: AiController, pose = false) {
    super(node, c)
    this.pose = pose
  }
  enter() {
    this.alive = true
    this.n = this.node.frames!
    if (this.pose) this.h.setAnim(['special_pose', 'idle'], true, true)
  }
  update() {
    return --this.n <= 0
  }
}

class Chase extends Base {
  private mode: 'chase' | 'flee' | 'back' | 'backBlocked'
  constructor(node: AiNode, c: AiController, mode: 'chase' | 'flee' | 'back' | 'backBlocked') {
    super(node, c)
    this.mode = mode
  }
  update() {
    const p = this.h.playerCenter()
    if (!p) return false
    const [x, y] = this.h.center()
    let dx = p[0] - x
    let dy = p[1] - y
    const spd = this.node.speed_256ths!
    const m = Math.max(Math.abs(dx), Math.abs(dy))
    if (m === 0) return false
    const toward = facingToward(dx, dy)
    if (this.mode !== 'chase') {
      dx = -dx
      dy = -dy
    }
    const vx = Math.trunc((dx * spd) / m)
    const vy = Math.trunc((dy * spd) / m)
    // ChasePlayer/Flee face the movement's dominant axis; BackAway keeps facing the player
    this.h.facing = this.mode === 'back' || this.mode === 'backBlocked' ? toward : facingToward(dx, dy)
    const moved = this.h.slide(vx, vy, true)
    if (this.mode === 'chase' || this.mode === 'back') return false
    return !moved
  }
}

class Align extends Base {
  update() {
    const p = this.h.playerCenter()
    if (!p) return false
    const [x, y] = this.h.center()
    const dx = p[0] - x
    const dy = p[1] - y
    const spd = this.node.speed_256ths!
    if (Math.abs(dx) > Math.abs(dy)) {
      if (Math.abs(dy) <= 3) {
        this.h.facing = dx > 0 ? 3 : 2
        return true
      }
      this.h.facing = dy > 0 ? 0 : 1
      if (!this.h.slide(0, Math.sign(dy) * spd, true)) {
        this.h.facing = dx > 0 ? 3 : 2
        return true
      }
    } else {
      if (Math.abs(dx) <= 3) {
        this.h.facing = dy > 0 ? 0 : 1
        return true
      }
      this.h.facing = dx > 0 ? 3 : 2
      if (!this.h.slide(Math.sign(dx) * spd, 0, true)) {
        this.h.facing = dy > 0 ? 0 : 1
        return true
      }
    }
    return false
  }
}

/** Knockback / KnockbackInvulnerable (0x0801A947 / 0x0801ACB7) */
class Knockback extends Base {
  vx = 0
  vy = 0
  n = 0
  private invuln: boolean
  constructor(node: AiNode, c: AiController, invuln: boolean) {
    super(node, c)
    this.invuln = invuln
  }
  enter() {
    this.alive = true
    const h = this.h
    const p = this.node.param ?? 16
    const k = ((h.w.rng.rand(64, 'knockback') + 96) * p) >> 7
    const perp = ((h.w.rng.rand(32768, 'knockback jitter') - 16384) * k) >> 12
    const [dx, dy] = DIR_VEC[h.lastHitDir]
    this.vx = dx * 8 * k + (dx ? 0 : perp)
    this.vy = dy * 8 * k + (dy ? 0 : perp)
    this.n = h.hurtAnimLength()
    h.setAnim(['hurt', 'idle'], true, false)
    if (this.invuln) h.hittable = false
  }
  update() {
    this.h.slide(this.vx, this.vy, false)
    return --this.n <= 0
  }
  exit() {
    if (this.alive && this.invuln) this.h.hittable = true
    if (this.alive) this.h.setAnim('idle', true, true)
    this.alive = false
  }
}

/** anim-object attacks: run `len` frames, fire `fn` at frame `at`, done at the end */
class Timed extends Base {
  f = 0
  private len: number
  private onEnter: () => void
  private at: number[]
  private fn: (f: number) => void
  private every?: [number, number]
  constructor(node: AiNode, c: AiController, len: number, onEnter: () => void, at: number[], fn: (f: number) => void, every?: [number, number]) {
    super(node, c)
    this.len = len
    this.onEnter = onEnter
    this.at = at
    this.fn = fn
    this.every = every
  }
  enter() {
    this.alive = true
    this.f = 0
    this.onEnter()
  }
  update() {
    this.f++
    if (this.at.includes(this.f)) this.fn(this.f)
    if (this.every && this.f >= this.every[0] && this.f < this.every[1]) this.fn(this.f)
    if (this.f >= this.len) {
      this.h.setAnim('idle', true, true)
      return true
    }
    return false
  }
}

class Strike extends Base {
  enter() {
    this.alive = true
    this.h.meleeStrike(this.node.param ?? 1)
  }
  update() {
    if (this.h.animDone()) {
      this.h.setAnim('idle', true, true)
      return true
    }
    return false
  }
}

/** Charge (0x0801C7F3) and Pounce (0x0801D255): dash toward the player's position at entry */
class Dash extends Base {
  vx = 0
  vy = 0
  travelled = 0
  limit = 0
  hit = false
  private pounce: boolean
  constructor(node: AiNode, c: AiController, pounce: boolean) {
    super(node, c)
    this.pounce = pounce
  }
  enter() {
    this.alive = true
    const p = this.h.playerCenter()
    const [x, y] = this.h.center()
    if (!p) return
    const dx = p[0] - x
    const dy = p[1] - y
    const m = Math.max(Math.abs(dx), Math.abs(dy), 1)
    const spd = this.node.speed_256ths!
    this.vx = Math.trunc((dx * spd) / m)
    this.vy = Math.trunc((dy * spd) / m)
    this.h.facing = facingToward(dx, dy)
    // Charge: ends after passing the player by `param` px; Pounce: after `param` px
    this.limit = (this.pounce ? 0 : m) + (this.node.param ?? 56)
    this.travelled = 0
    this.hit = false
    this.h.setAnim(this.pounce ? ['attack1', 'walk'] : ['walk', 'idle'], true, true)
  }
  update() {
    if (!this.vx && !this.vy) return true
    const moved = this.h.slide(this.vx, this.vy, false)
    this.travelled += Math.max(Math.abs(this.vx), Math.abs(this.vy)) / 256
    if (!this.hit && this.h.hitPlayerOnContact()) this.hit = true
    if (!moved || this.travelled >= this.limit) {
      this.h.setAnim('idle', true, true)
      return true
    }
    return false
  }
}

class Drain extends Base {
  f = 0
  started = false
  release = -1
  enter() {
    this.alive = true
    this.f = 0
    this.release = -1
    this.started = this.h.drainStart()
  }
  update() {
    if (!this.started) return true
    this.f++
    if (this.release < 0) {
      if (!this.h.drainTick(this.f)) this.release = this.f
      return false
    }
    // +8-frame wind-down after the player is released
    if (this.f - this.release >= 8) {
      this.h.setAnim('idle', true, true)
      return true
    }
    return false
  }
}

export function create(node: AiNode, c: AiController): RNode {
  const h = c.host
  const st = node.state!
  switch (node.type) {
    case 'Sequence':
      return new Sequence(node, c)
    case 'RaceFirst':
      return new Race(node, c, false)
    case 'RaceAll':
      return new Race(node, c, true)
    case 'IfPlayerWithin':
      return new Branch(node, c, () => {
        const d2 = distSq(h)
        return d2 != null && d2 <= node.radius_sq!
      })
    case 'IfPlayerSeen':
      return new Branch(node, c, () => h.playerSeen(node.radius_sq!))
    case 'Chance':
      return new Chance(node, c)
    case 'WhileInMeleeRange':
      return new InRange(node, c, false)
    case 'WhileInMeleeRangeB':
      return new InRange(node, c, true)
    case 'GotoState':
      return new Instant(node, c, () => c.replaceTop(st))
    case 'PushState':
      return new Instant(node, c, () => c.push(st))
    case 'PopState':
      return new Instant(node, c, () => c.pop())
    case 'SubState':
      return create(c.states[st].behaviour, c)
    case 'ClassDefaultState':
      return create(c.states[h.classAiState].behaviour, c)
    case 'Wait':
      return new Wait(node, c)
    case 'WaitPose':
      return new Wait(node, c, true)
    case 'ChasePlayer':
      return new Chase(node, c, 'chase')
    case 'FleePlayer':
      return new Chase(node, c, 'flee')
    case 'BackAway':
      return new Chase(node, c, 'back')
    case 'BackAwayUntilBlocked':
      return new Chase(node, c, 'backBlocked')
    case 'AlignWithPlayer':
      return new Align(node, c)
    case 'Knockback':
      return new Knockback(node, c, false)
    case 'KnockbackInvulnerable':
      return new Knockback(node, c, true)
    case 'MeleeStrike':
      return new Strike(node, c)
    case 'ContactDamage':
      return new (class extends Base {
        update() {
          h.contactCheck()
          return false
        }
      })(node, c)
    case 'Charge':
      return new Dash(node, c, false)
    case 'Pounce':
      return new Dash(node, c, true)
    case 'ShootEnergyBall':
      return new Timed(node, c, 32, () => faceAnd(h, 'kiblast'), [8], () => h.shoot('ball'))
    case 'ShootKiBlast':
      return new Timed(node, c, 32, () => faceAnd(h, 'kiblast'), [16], () => h.shoot('ki'))
    case 'LobProjectile':
      return new Timed(node, c, 32, () => faceAnd(h, ['kiblast', 'attack1']), [16], () => h.shoot('lob'))
    case 'ThrowProjectile':
      return new Timed(node, c, 32, () => faceAnd(h, ['kiblast', 'attack1']), [8], () => h.shoot('star'))
    case 'LaserBeam':
      return new Timed(node, c, 60, () => faceAnd(h, ['kiblast', 'attack1', 'idle']), [1], () => h.shoot('laser', node.param))
    case 'SprayAttack':
      return new Timed(node, c, 128, () => faceAnd(h, ['attack1', 'kiblast']), [], () => h.sprayParticle(), [4, 128])
    case 'BeamAttack':
      return new Timed(node, c, 90, () => faceAnd(h, ['special_beam', 'kiblast']), [1], () => h.shoot('beam', signed(node.data)))
    case 'BeamAttackTao':
      return new Timed(node, c, 90, () => faceAnd(h, ['special_beam', 'kiblast']), [1], () => h.shoot('beamTao', signed(node.data)))
    case 'DrainGrab':
      return new Drain(node, c)
    case 'SelfDestructBurst':
      return new Instant(node, c, () => h.selfDestruct())
    case 'DeathExplode':
      return new Instant(node, c, () => h.die(true, node.param === 0x40 ? 0x40 : 0x28))
    case 'HitFlash':
      return new Instant(node, c, () => h.hitFlash())
    case 'GlowFlash':
      return new Instant(node, c, () => h.glowFlash())
    case 'StunAnim':
      return new Instant(node, c, () => h.stunAnim(true))
    case 'ClearAnim':
      return new Instant(node, c, () => h.stunAnim(false))
    case 'SetIdleAnim':
      return new Instant(node, c, () => h.setAnim('idle', false, true))
    case 'FacePlayer':
      return new Instant(node, c, () => {
        const p = h.playerCenter()
        if (!p) return
        const [x, y] = h.center()
        h.facing = facingToward(p[0] - x, p[1] - y)
      })
    case 'FlyerAnimA':
    case 'FlyerAnimB':
      return new Instant(node, c, () => h.setAnim('walk', false, true))
    case 'PlaySound':
    case 'Succeed':
      return new Instant(node, c, () => undefined)
    case 'Patrol':
    case 'Nop':
    default:
      return new Never(node, c)
  }
}

function faceAnd(h: AiHost, anim: string | string[]) {
  const p = h.playerCenter()
  if (p) {
    const [x, y] = h.center()
    h.facing = facingToward(p[0] - x, p[1] - y)
  }
  h.setAnim(anim, true, false)
}

function signed(hex?: string): number {
  if (!hex) return 0
  const v = parseInt(hex, 16)
  return v > 0x7fffffff ? v - 0x100000000 : v
}
