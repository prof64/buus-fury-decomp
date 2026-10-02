// Interpreter for the decoded Buu's Fury enemy AI (data/buu/enemy_ai.json + data/battle/buu_ai_extra.json):
// an AI stack of states (obj+0x238) whose top state's behaviour tree runs once per frame, plus the on-hit ("aux")
// tree of the top state, evaluated through each node's onHit function when the enemy is hit.
// Semantics: re/buu/notes.md §8 / §8b, enemy_ai.json node_types[].semantics, and the onHit disassembly in
// tools/battle/gen_buu_ai_extra.py. Every node type of the data is implemented; NODE_NOTES lists the ones whose
// details are inferred (tables not decoded, cosmetic nodes).

import { DIR_VEC, facingToward, overlaps, type Dir } from '../../core/geom'
import type { HitMsg } from './actor'
import type { AiNodeJ } from './data'
import type { Enemy } from './enemy'
import { ATK } from './damage'
import {
  Bomb, CandyBeam, DarkSphere, EnergyWave, Flame, FullBeam, GrappleArm, GrowBeam, Rock, SJWave, StraightShot, StretchArm, SuperBall, VEL, kiBlast,
} from './projectiles'

export const NODE_NOTES: Record<string, string> = {
  ChasePlayer: 'path-finding replaced by a straight normalised move (arenas are open rectangles)',
  IfPlayerInZone: 'sniper zone rectangle not decoded: true within 160 px',
  IfPlayerInRoomZone: 'vampire lair region not decoded: always true',
  VampireSummon: 'spawns the puff effect only (the spawned object is the 0x086FD9D0 effect run)',
  MoveToFormationA: 'formation offset tables 0x0877AE80/0x0877AECC not decoded: ring of 8 slots at 40/24 px',
  GhostAnimA: 'cosmetic appear/vanish animation',
  TankDrive: 'turn helpers not decoded: axis-aligned drive toward the player at 0.5 px/frame',
  CandyBeam: 'beam offsets for facings other than right are mirrored guesses',
  EnergyBarrage: 'barrage flight path not decoded: short 4 px/frame shots, 12 frames',
  IfUnknown: 'condition 0x0804EE25 not decoded: false',
  ShockStun: 'electric-shock stun length not decoded: 48 frames',
  VolleyballLaunch: 'launched ball flight not decoded: 4 px/frame for 30 frames, hits other enemies',
  InlineScript: 'bytecode not run (scripted escape / cutscene)',
  RunScript: 'cutscene scripts are not run',
  KnockbackSlide: 'reaction constant K not tabulated: K = 256 (~1 px/frame, matches the measured 15 px / 15 f)',
}

export interface RNode {
  node: AiNodeJ
  enter(): void
  update(): number
  exit(): void
  active?: RNode
}

const P = (n: AiNodeJ, k: string, d = 0) => (n.params?.[k]?.raw as number | undefined) ?? d

export class AiController {
  stack: { state: number; rt: RNode }[] = []
  gen = 0
  history: { frame: number; state: number }[] = []
  readonly e: Enemy
  constructor(e: Enemy) {
    this.e = e
  }
  get w() {
    return this.e.w
  }
  top() {
    return this.stack[this.stack.length - 1]
  }
  private build(id: number): RNode {
    const tree = this.e.ctx.data.states.get(id)
    if (!tree) return new Never({ addr: '', fn: '', name: `missing state 0x${id.toString(16)}` }, this)
    return create(tree, this)
  }
  private note(id: number) {
    this.history.push({ frame: this.w?.frame ?? 0, state: id })
    if (this.history.length > 24) this.history.shift()
    this.e.stateStamp = this.w?.frame ?? 0
  }
  push(id: number) {
    if (this.stack.length >= 8) return
    this.gen++
    const rt = this.build(id)
    this.stack.push({ state: id, rt })
    this.note(id)
    rt.enter()
  }
  replaceTop(id: number) {
    if (!this.stack.length) return this.push(id)
    this.gen++
    const old = this.stack[this.stack.length - 1]
    old.rt.exit()
    const rt = this.build(id)
    this.stack[this.stack.length - 1] = { state: id, rt }
    this.note(id)
    rt.enter()
  }
  pop() {
    if (this.stack.length <= 1) return
    this.gen++
    const old = this.stack.pop()!
    old.rt.exit()
    this.e.stateStamp = this.w.frame
  }
  reset(id: number) {
    for (const s of this.stack.reverse()) s.rt.exit()
    this.stack = []
    this.push(id)
  }
  tick() {
    const t = this.top()
    if (!t) return
    const g = this.gen
    const r = t.rt.update()
    // a finished root is restarted (the state's tree runs again)
    if (r && g === this.gen && this.top() === t) {
      t.rt.exit()
      t.rt.enter()
    }
  }
  /** on-hit: the top state's aux tree; returns true when the hit was taken (message accepted) */
  onHit(msg: HitMsg): number {
    const t = this.top()
    if (!t) return 0
    const aux = this.e.ctx.data.aux.get(t.state)
    if (!aux) return 0
    return onHit(aux, this, msg)
  }
  describe(): string {
    const t = this.top()
    if (!t) return '-'
    const path: string[] = []
    let n: RNode | undefined = t.rt
    for (let i = 0; n && i < 6; i++) {
      path.push(n.node.name)
      n = n.active
    }
    return `st 0x${t.state.toString(16)}${this.stack.length > 1 ? ' [' + this.stack.map((s) => s.state.toString(16)).join('>') + ']' : ''} ${path.join('/')}`
  }
}

// ---------------------------------------------------------------------------------------------------------------
// onHit (aux trees): 0 = not handled, 1 = handled, 2 = stop

function onHit(n: AiNodeJ, c: AiController, msg: HitMsg): number {
  const e = c.e
  const w = c.w
  const kids = n.children ?? []
  const kid = (i = 0) => (kids[i] ? onHit(kids[i], c, msg) : 0)
  switch (n.name) {
    case 'Sequence': {
      let res = 1
      for (const k of kids) {
        const r = onHit(k, c, msg)
        if (r === 2) return 2
        res &= r
      }
      return res
    }
    case 'Selector':
      for (const k of kids) {
        const r = onHit(k, c, msg)
        if (r) return r
      }
      return 0
    case 'IfAttack': {
      const a = P(n, 'atk')
      return msg.atk.sub === a || msg.atk.kind === a ? kid() : 0
    }
    case 'ApplyDamage':
      if (msg.mult === 0) return 0
      return e.takeHit(msg) ? 2 : 1
    case 'GotoState':
      c.replaceTop(P(n, 'state'))
      return 1
    case 'PushState':
      c.push(P(n, 'state'))
      return 1
    case 'PopState':
      c.pop()
      return 2
    case 'SubState': {
      const aux = e.ctx.data.aux.get(P(n, 'state'))
      return aux ? onHit(aux, c, msg) : 0
    }
    case 'Chance':
      return w.rng.chance(P(n, 'p'), 'ai chance (hit)') ? kid() : 0
    case 'IfTimeInState':
      return w.frame - e.stateStamp > P(n, 'frames') ? kid() : 0
    case 'IfCounterEq':
      return e.counter === P(n, 'n') ? kid() : 0
    case 'IfCooldownOver':
      return w.frame - e.mark1 > P(n, 'frames') ? kid() : 0
    case 'IfHpBelow':
      return e.hp1024 < Math.floor((P(n, 'frac') * e.maxHp1024) / 65536) ? kid() : 0
    case 'IfHitFromFront':
      return (msg.dir ^ 1) === e.facing ? kid() : 0
    case 'Succeed':
      return 2
    case 'MarkTime':
      e.mark1 = w.frame
      return 0
    case 'IncCounter':
      e.counter++
      return 0
    case 'SetCounter':
      e.counter = P(n, 'n')
      return 0
    case 'Block':
      e.blocking = true
      e.setAnim(7, false, true)
      return 1
    case 'StunHold':
      e.stunned = true
      return 1
    case 'HoverDrift': {
      const [dx, dy] = DIR_VEC[msg.dir]
      e.vx += dx * 0x400
      e.vy += dy * 0x400
      return 1
    }
    case 'SetHitDir':
      e.lastHitDir = msg.dir
      return 1
    case 'HealFraction':
      if (msg.mult === 0) return 1
      e.heal(Math.floor(((msg.dmg ?? 0) * P(n, 'frac')) / 65536))
      return 1
    case 'GlideToWaypoint':
      // 0x0803C08D: accepted only while the player swings (bats the ball back); each accepted hit plays one of
      // the cutscenes #474-#476 in the game, the third ends the fight (story script, not run here)
      if (!e.ctx.player?.swinging()) return 2
      e.scriptHits++
      w.logEvent('info', `Super Buu hit back (${e.scriptHits}/3): cutscene #${473 + e.scriptHits}`)
      if (e.scriptHits >= 3) e.scriptedDefeat()
      return 1
    case 'AbsorbPose':
    case 'AbsorbPose2':
    case 'FreezeHelperHit':
    case 'ShieldHit':
      return 1
    default:
      return 0
  }
}

// ---------------------------------------------------------------------------------------------------------------
// behaviour runtime nodes

abstract class Base implements RNode {
  readonly node: AiNodeJ
  readonly c: AiController
  constructor(node: AiNodeJ, c: AiController) {
    this.node = node
    this.c = c
  }
  get e() {
    return this.c.e
  }
  get w() {
    return this.c.w
  }
  enter() {}
  abstract update(): number
  exit() {}
}

class Never extends Base {
  update() {
    return 0
  }
}

/** instant action: runs fn on update, finished at once */
class Instant extends Base {
  private fn: () => void
  constructor(n: AiNodeJ, c: AiController, fn: () => void) {
    super(n, c)
    this.fn = fn
  }
  update() {
    this.fn()
    return 1
  }
}

class Sequence extends Base {
  kids: RNode[]
  i = 0
  constructor(n: AiNodeJ, c: AiController) {
    super(n, c)
    this.kids = (n.children ?? []).map((k) => create(k, c))
  }
  get active() {
    return this.kids[this.i]
  }
  enter() {
    this.i = 0
    this.kids[0]?.enter()
  }
  update() {
    if (this.i >= this.kids.length) return 1
    const g = this.c.gen
    const r = this.kids[this.i].update()
    if (g !== this.c.gen) return 0
    if (!r) return 0
    this.kids[this.i].exit()
    this.i++
    if (this.i >= this.kids.length) return 1
    this.kids[this.i].enter()
    return 0
  }
  exit() {
    if (this.i < this.kids.length) this.kids[this.i].exit()
    this.i = this.kids.length
  }
}

class Selector extends Base {
  kids: RNode[]
  last = -1
  constructor(n: AiNodeJ, c: AiController) {
    super(n, c)
    this.kids = (n.children ?? []).map((k) => create(k, c))
  }
  get active() {
    return this.kids[this.last >= 0 ? this.last : 0]
  }
  enter() {
    for (const k of this.kids) k.enter()
  }
  update() {
    const g = this.c.gen
    for (let i = 0; i < this.kids.length; i++) {
      const r = this.kids[i].update()
      if (g !== this.c.gen) return 0
      if (r) {
        this.last = i
        return 1
      }
    }
    return 0
  }
  exit() {
    for (const k of this.kids) k.exit()
  }
}

class Parallel extends Base {
  kids: RNode[]
  done: boolean[] = []
  private any: boolean
  constructor(n: AiNodeJ, c: AiController, any: boolean) {
    super(n, c)
    this.any = any
    this.kids = (n.children ?? []).map((k) => create(k, c))
  }
  get active() {
    return this.kids[0]
  }
  enter() {
    this.done = this.kids.map(() => false)
    for (const k of this.kids) k.enter()
  }
  update() {
    const g = this.c.gen
    for (let i = 0; i < this.kids.length; i++) {
      if (this.done[i]) continue
      const r = this.kids[i].update()
      if (g !== this.c.gen) return 0
      if (r) {
        this.done[i] = true
        this.kids[i].exit()
        if (this.any) return 1
      }
    }
    return this.done.every(Boolean) ? 1 : 0
  }
  exit() {
    this.kids.forEach((k, i) => !this.done[i] && k.exit())
    this.done = this.kids.map(() => true)
  }
}

/** shared conditional decorator (enter 0x080342A9 / update 0x080342AF / exit 0x08034307): while the condition
 *  holds the child is entered/updated and its result returned; when false the child is exited and 0 returned */
class Cond extends Base {
  kid: RNode | null
  on = false
  private cond: () => boolean
  private onEnter?: () => void
  constructor(n: AiNodeJ, c: AiController, cond: () => boolean, onEnter?: () => void) {
    super(n, c)
    this.cond = cond
    this.onEnter = onEnter
    this.kid = n.children?.[0] ? create(n.children[0], c) : null
  }
  get active() {
    return this.kid ?? undefined
  }
  enter() {
    this.on = false
    this.onEnter?.()
  }
  update() {
    if (this.cond()) {
      if (!this.kid) return 1
      if (!this.on) {
        this.kid.enter()
        this.on = true
      }
      const g = this.c.gen
      const r = this.kid.update()
      if (g !== this.c.gen) return 0
      if (r) {
        this.kid.exit()
        this.on = false
      }
      return r
    }
    if (this.on && this.kid) this.kid.exit()
    this.on = false
    return 0
  }
  exit() {
    if (this.on && this.kid) this.kid.exit()
    this.on = false
  }
}

/** frame-driven action: f counts update calls from 1; step(f) returns true when done */
class Act extends Base {
  f = 0
  private on: (a: Act) => void
  private step: (f: number, a: Act) => boolean
  private off?: (a: Act) => void
  data: Record<string, number> = {}
  constructor(n: AiNodeJ, c: AiController, on: (a: Act) => void, step: (f: number, a: Act) => boolean, off?: (a: Act) => void) {
    super(n, c)
    this.on = on
    this.step = step
    this.off = off
  }
  enter() {
    this.f = 0
    this.data = {}
    this.on(this)
  }
  update() {
    this.f++
    return this.step(this.f, this) ? 1 : 0
  }
  exit() {
    this.off?.(this)
  }
}

// ---------------------------------------------------------------------------------------------------------------

function wait(n: AiNodeJ, c: AiController, frames: number, anim?: number | string | (string | number)[], loop = true) {
  return new Act(n, c, () => anim != null && c.e.setAnim(anim, true, loop), (f) => f >= frames)
}

/** play an animation (engine number) and finish when it ends (`hold` = never finishes) */
function playAnim(n: AiNodeJ, c: AiController, anim: number | (string | number)[], hold = false, min = 8) {
  return new Act(
    n, c,
    () => c.e.setAnim(anim, true, hold),
    (f) => !hold && f >= min && c.e.animDone(),
  )
}

/** standard facing + controller action: anim, events at frames, done at `len` */
function timed(n: AiNodeJ, c: AiController, anim: number | (string | number)[], len: number, events: Record<number, () => void>, face = true) {
  return new Act(
    n, c,
    () => {
      if (face) c.e.facePlayer()
      c.e.setAnim(anim, true, false)
    },
    (f) => {
      events[f]?.()
      if (f >= len) {
        c.e.setAnim('idle', true, true)
        return true
      }
      return false
    },
  )
}

export function create(n: AiNodeJ, c: AiController): RNode {
  const e = c.e
  const st = P(n, 'state')
  const speed = P(n, 'speed')
  switch (n.name) {
    // ---- composites / transitions --------------------------------------------------------------------------------
    case 'Sequence':
      return new Sequence(n, c)
    case 'Selector':
      return new Selector(n, c)
    case 'ParallelAll':
      return new Parallel(n, c, false)
    case 'ParallelAny':
      return new Parallel(n, c, true)
    case 'GotoState':
      return new Instant(n, c, () => c.replaceTop(st))
    case 'PushState': {
      let pushed = false
      return new Act(n, c, () => (pushed = false), () => {
        if (pushed) return true
        pushed = true
        c.push(st)
        return false
      })
    }
    case 'PopState':
      return new Instant(n, c, () => {
        e.stunned = false
        c.pop()
      })
    case 'SubState': {
      const t = e.ctx.data.states.get(st)
      return t ? create(t, c) : new Never(n, c)
    }

    // ---- conditions -------------------------------------------------------------------------------------------------
    case 'Chance':
      return new Cond(n, c, () => e.w.rng.chance(P(n, 'p'), 'ai chance'))
    case 'IfPlayerWithin':
    case 'IfPlayerWithin2':
      return new Cond(n, c, () => e.dist2() <= P(n, 'r2'))
    case 'IfPlayerFarther':
      return new Cond(n, c, () => e.playerAlive() && e.dist2() > P(n, 'r2'))
    case 'IfPlayerInMeleeRange':
      return new Cond(n, c, () => e.inMeleeRange())
    case 'IfPlayerAttackingMe':
      return new Cond(n, c, () => e.playerAttackingMe())
    case 'IfInPlayerBeamLine':
      return new Cond(n, c, () => e.inPlayerBeamLine())
    case 'IfTimeInState':
      return new Cond(n, c, () => e.w.frame - e.stateStamp > P(n, 'frames'))
    case 'IfCooldownOver':
      return new Cond(n, c, () => e.w.frame - e.mark1 > P(n, 'frames'))
    case 'IfCooldown2Over':
      return new Cond(n, c, () => e.w.frame - e.mark2 > P(n, 'frames'))
    case 'IfCounterEq':
      return new Cond(n, c, () => e.counter === P(n, 'n'))
    case 'IfGlobalFlag':
      return new Cond(n, c, () => e.ctx.candy > 0)
    case 'IfBuuPacified':
      return new Cond(n, c, () => e.ctx.buuPacified !== 0)
    case 'IfAxisAligned':
      return new Cond(n, c, () => {
        const p = e.player
        if (!p || !p.hittable) return false
        const [dx, dy] = e.toPlayer()
        return Math.abs(dx) < 16 || Math.abs(dy) < 16
      })
    case 'IfHpBelow': {
      let thr = 0
      return new Cond(n, c, () => e.hp1024 < thr, () => (thr = Math.floor((P(n, 'frac') * e.maxHp1024) / 65536)))
    }
    case 'IfPlayerInFront':
      return new Cond(n, c, () => e.playerInFront(48))
    case 'IfPlayerInZone':
      return new Cond(n, c, () => e.dist2() <= 160 * 160)
    case 'IfPlayerInRoomZone':
      return new Cond(n, c, () => e.playerAlive())
    case 'IfPlayerBlastIncoming':
      return new Cond(n, c, () => e.blastIncoming())
    case 'IfPlayerInLineOfFire':
      return new Cond(n, c, () => {
        const [dx, dy] = e.toPlayer()
        const [fx, fy] = DIR_VEC[e.facing]
        return fx ? Math.sign(dx) === fx && Math.abs(dy) < 16 : Math.sign(dy) === fy && Math.abs(dx) < 16
      })
    case 'IfContactHit':
      return new Cond(n, c, () => e.meleeStrike(1024, false))
    case 'IfMeleeHit':
      return new Cond(n, c, () => e.meleeStrike(1024, false))
    case 'IfContactExplode':
      return new Cond(n, c, () => e.contactExplode())
    case 'IfLandedOnRock':
      return new Cond(n, c, () => e.landOnRock())
    case 'IfPlayerPetrified':
      return new Cond(n, c, () => !!e.player?.isStone())
    case 'IfPlayerNotPetrified':
      return new Cond(n, c, () => !!e.player && !e.player.isStone())
    case 'IfSphereDestroyed':
      return new Cond(n, c, () => e.ctx.sphereState === 1)
    case 'IfPlayerNearAndHittable':
      return new Cond(n, c, () => {
        const p = e.player
        if (!p || !p.hittable) return false
        const [dx, dy] = e.toPlayer()
        return Math.abs(dx) <= 47 && Math.abs(dy) <= 47
      })
    case 'IfLastKidBuu':
      return new Cond(n, c, () => e.ctx.liveEnemies().filter((o) => o.type.type === 0x3c || o.type.type === 0x3d).length === 1)
    case 'IfChildCount':
      return new Cond(n, c, () => e.children.filter((k) => !k.removed && !k.defeated).length === P(n, 'n'))
    case 'IfLatchOverlap':
      return new Cond(n, c, () => {
        const p = e.player
        if (!p || p.heldBy || e.facing !== 0) return false
        const pb = p.bodyBox()
        const eb = e.bodyBox()
        return overlaps({ x0: pb.x0 + 6, y0: pb.y0, x1: pb.x1 - 6, y1: pb.y1 }, { x0: eb.x0, y0: eb.y1 - 8, x1: eb.x0 + 7, y1: eb.y1 - 1 })
      })
    case 'IfButtonMashed':
      return new Cond(n, c, () => !!e.player && e.player.heldBy !== e)
    case 'IfPlayerOnTop':
      return new Cond(n, c, () => {
        const [dx, dy] = e.toPlayer()
        return Math.abs(dx) < 16 && Math.abs(dy) < 16
      })
    case 'IfPartnerHolding':
      return new Cond(n, c, () => e.ctx.liveEnemies().some((o) => [0x6d, 0x6e, 0x6f, 0x70].includes(o.type.type) && e.ctx.player?.heldBy === o))
    case 'IfUnknown':
      return new Cond(n, c, () => false)

    // ---- simple actions ---------------------------------------------------------------------------------------------
    case 'Succeed':
      return new Instant(n, c, () => undefined)
    case 'Idle':
    case 'RunMotionController':
    case 'PartMirrorAnim':
      return new Act(n, c, () => !e.moving && e.setAnim('idle', false, true), () => false)
    case 'StandStill':
      return new Instant(n, c, () => e.setAnim('idle', false, true))
    case 'FacePlayer':
      return new Instant(n, c, () => e.facePlayer())
    case 'Wait':
      return wait(n, c, P(n, 'frames'))
    case 'MarkTime':
      return new Instant(n, c, () => (e.mark1 = e.w.frame))
    case 'MarkTime2':
      return new Instant(n, c, () => (e.mark2 = e.w.frame))
    case 'SetCounter':
      return new Instant(n, c, () => (e.counter = P(n, 'n')))
    case 'IncCounter':
      return new Instant(n, c, () => e.counter++)
    case 'SelfMessage11':
    case 'RunScript':
    case 'InlineScript':
    case 'StaggerMinions':
      return new Instant(n, c, () => {
        if (n.name === 'StaggerMinions') {
          const r = e.w.rng.rand(8, 'stagger')
          e.children.forEach((k, i) => ((k as Enemy).slot = ((r + i + 1) & 7) || 8))
        }
      })
    case 'SpawnPuff':
    case 'VampireSummon':
    case 'VampireSummon2':
      return new Instant(n, c, () => e.ctx.puff(e.w, ...e.center()))
    case 'SelfDestruct':
      return new Instant(n, c, () => e.die(true))
    case 'TriggerSelf':
      return new Instant(n, c, () => e.die(false))
    case 'Block':
      return new Act(n, c, () => {
        e.blocking = true
        e.setAnim(7, true, true)
      }, () => false, () => {
        e.blocking = false
        e.stateStamp = e.w.frame
      })

    // ---- movement -------------------------------------------------------------------------------------------------------
    case 'ChasePlayer':
    case 'ChasePlayerThroughRocks':
      return new Act(n, c, () => undefined, () => {
        e.moveToward(speed, 2, true)
        return false
      })
    case 'StepTowardPlayer':
    case 'StepTowardPlayerThroughRocks':
      return new Act(n, c, () => undefined, () => {
        e.moveToward(speed, 0, true)
        return false
      })
    case 'StepAwayFromPlayer':
      return new Act(n, c, () => undefined, () => {
        e.moveAway(speed)
        return false
      })
    case 'WalkToPlayer':
      return new Act(n, c, () => undefined, () => e.moveToward(speed, 8, true))
    case 'FlyToPlayer':
      return new Act(n, c, () => undefined, () => e.moveToward(speed, 8, false, true))
    case 'AlignWithPlayer':
    case 'LineUpForShot':
      return new Act(n, c, () => undefined, () => e.align(speed || 0x73))
    case 'Wander':
      return new Act(n, c, (a) => (a.data.dir = e.w.rng.rand(4, 'wander dir')), (_f, a) => {
        if (e.w.rng.chance(P(n, 'turn_p'), 'wander turn')) a.data.dir = e.w.rng.rand(4, 'wander dir')
        const d = a.data.dir as Dir
        e.facing = d
        const [dx, dy] = DIR_VEC[d]
        e.walkBy(dx * speed, dy * speed)
        return false
      })
    case 'MoveToPoint':
      return new Act(n, c, () => undefined, () => e.moveToPoint(P(n, 'x'), P(n, 'y'), speed))
    case 'WanderToRandomPoint':
      return new Act(n, c, (a) => {
        a.data.x = 96 + e.w.rng.rand(296, 'wander point')
        a.data.y = 112 + e.w.rng.rand(240, 'wander point')
      }, (_f, a) => e.moveToPoint(a.data.x, a.data.y, 0x100))
    case 'CirclePlayer':
      return new Act(n, c, () => undefined, () => {
        e.facePlayer()
        const [dx, dy] = DIR_VEC[e.facing]
        e.walkBy(-dy * speed, dx * speed)
        return false
      })
    case 'HoverDrift':
      return new Act(n, c, () => undefined, () => {
        const [dx, dy] = e.toPlayer()
        e.vx = Math.max(-128, Math.min(128, e.vx + Math.sign(dx) * 38))
        e.vy = Math.max(-128, Math.min(128, e.vy + Math.sign(dy) * 38))
        e.facePlayer()
        e.walkBy(e.vx, e.vy)
        return false
      })
    case 'BackStep':
      return new Act(n, c, (a) => {
        const s = ((e.w.rng.rand(64, 'backstep') + 96) * P(n, 'k', 16)) >> 7
        const [dx, dy] = DIR_VEC[e.facing]
        const side = ((e.w.rng.rand(32768, 'backstep side') - 16384) * s) >> 12
        a.data.vx = -dx * 8 * s + (dx ? 0 : side)
        a.data.vy = -dy * 8 * s + (dy ? 0 : side)
        e.setAnim(1, true, true)
      }, (f, a) => {
        e.moveBy(e.w, a.data.vx, a.data.vy)
        return f >= 48
      })
    case 'KnockbackSlide':
      return new Act(n, c, (a) => {
        const k = ((e.w.rng.rand(64, 'knockback') + 96) * 256) >> 7
        const [dx, dy] = DIR_VEC[e.lastHitDir]
        a.data.vx = dx * k
        a.data.vy = dy * k
        e.setAnim(9, true, false)
      }, (f, a) => {
        e.moveBy(e.w, a.data.vx, a.data.vy)
        if (f >= P(n, 'frames')) {
          e.setAnim('idle', true, true)
          return true
        }
        return false
      })
    case 'LungeStep':
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim(4, true, false)
      }, (f) => {
        const [dx, dy] = DIR_VEC[e.facing]
        e.moveBy(e.w, dx * 1024, dy * 1024)
        return f >= 16
      })
    case 'FollowMajinBuu':
      return new Act(n, c, () => undefined, () => {
        const buu = e.ctx.liveEnemies().find((o) => [0x1a, 0x1b, 0x1c].includes(o.type.type))
        if (!buu) return false
        const [bx, by] = buu.center()
        const [x, y] = e.center()
        const dx = bx - x
        const dy = by - y
        if (Math.max(Math.abs(dx), Math.abs(dy)) > P(n, 'dist', 50)) {
          e.walkBy(Math.sign(dx) * 256, Math.sign(dy) * 256)
          e.facing = facingToward(dx, dy)
        }
        return false
      })
    case 'TankDrive':
      return new Act(n, c, () => undefined, () => {
        const [dx, dy] = e.toPlayer()
        e.facing = facingToward(dx, dy)
        const [fx, fy] = DIR_VEC[e.facing]
        if (Math.abs(dx) + Math.abs(dy) > 24) e.walkBy(fx * 128, fy * 128)
        return false
      })
    case 'RandomStep':
      return new Act(n, c, (a) => (a.data.dir = e.w.rng.rand(4, 'random step')), (f, a) => {
        const [dx, dy] = DIR_VEC[a.data.dir as Dir]
        e.walkBy(dx * 256, dy * 256)
        return f >= 16
      })

    // ---- melee --------------------------------------------------------------------------------------------------------
    case 'MeleeAttack':
      // controller 0x03001B50: hit test once on the switch frame, 16-frame punch
      return new Act(n, c, () => {
        e.meleeStrike(0x400, true)
        e.setAnim([4 + (e.w.frame % 3), 4], true, false)
      }, (f) => f >= 16)
    case 'BioMechPunch':
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim(4, true, false)
        e.meleeStrike(0x400, true)
      }, (f) => f >= 16)
    case 'HeavyPunch':
      return timed(n, c, [4, 5], 64, { 48: () => e.meleeStrike(0x600, true) })
    case 'WindupMelee':
      return timed(n, c, [4, 5], 48, { 32: () => e.meleeStrike(0x400, true) })
    case 'HeavyStrike':
      return new Act(n, c, (a) => {
        a.data.ok = e.player?.hittable ? 1 : 0
        if (a.data.ok) e.setAnim(4, true, false)
      }, (f, a) => {
        if (!a.data.ok) return true
        if (f === 58) e.heavyStrike()
        return f >= 76
      })
    case 'ChargedPunch':
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim(18, true, true)
      }, (f) => {
        if (f === 140) {
          e.setAnim(4, true, false)
          e.hittable = false
          e.meleeStrike(0x600, false, true)
        }
        return f >= 149
      }, () => (e.hittable = true))
    case 'MiniYakonLunge':
      return new Act(n, c, (a) => {
        a.data.ok = e.player?.hittable ? 1 : 0
        if (a.data.ok) {
          e.facePlayer()
          e.setAnim(4, true, false)
        }
      }, (f, a) => {
        if (!a.data.ok) return true
        const [dx, dy] = DIR_VEC[e.facing]
        e.moveBy(e.w, dx * 384, dy * 384)
        if (!a.data.hit && e.contactHit()) a.data.hit = 1
        return f >= 24
      })
    case 'DriftContact':
      return new Act(n, c, (a) => {
        const ang = (e.w.rng.rand(256, 'drift angle') / 256) * Math.PI * 2
        const s = e.w.rng.rand(3, 'drift speed') + 1
        a.data.vx = Math.round(Math.cos(ang) * s * 64)
        a.data.vy = Math.round(Math.sin(ang) * s * 64)
        a.data.back = 0
      }, (_f, a) => {
        const k = a.data.back > 0 ? -1 : 1
        if (a.data.back > 0) a.data.back--
        if (!e.moveBy(e.w, k * a.data.vx, k * a.data.vy)) {
          a.data.vx = -a.data.vx
          a.data.vy = -a.data.vy
        }
        if (a.data.back <= 0 && e.contactHit()) a.data.back = 16
        return false
      })

    // ---- ranged -------------------------------------------------------------------------------------------------------
    case 'KiBlast':
      return timed(n, c, [3], 32, {
        16: () => {
          const [mx, my] = e.muzzle(10)
          e.w.add(kiBlast(e.ctx, e, e.facing, 0, mx << 8, my << 8))
        },
      })
    case 'GunShot':
      return timed(n, c, [17, 3], 16, { 4: () => e.shot('gun') })
    case 'RifleShot':
      return timed(n, c, [17, 3], 32, { 8: () => e.shot('rifle') })
    case 'NinjaStarThrow':
      return timed(n, c, [18, 4], 32, { 8: () => e.shot('star') })
    case 'MummyWrapShot':
      return timed(n, c, [17, 3], 24, { 8: () => e.shot('wrap') })
    case 'MummyWrapBurst':
      return timed(n, c, [17, 3], 24, { 8: () => e.shot('burst') }, false)
    case 'TankShoot':
      return new Instant(n, c, () => e.shot('shell'))
    case 'SoldierThrow':
    case 'CyborgShot':
    case 'TurretFire': {
      const fire = n.name === 'CyborgShot' ? 1 : 8
      const len = n.name === 'TurretFire' ? 72 : n.name === 'SoldierThrow' ? 32 : 16
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim([18, 3], true, false)
      }, (f, a) => {
        if (f === fire) a.data.wave = e.w.add(new EnergyWave(e.ctx, e, e.facing, ...e.muzzleFix(n.name === 'TurretFire' ? 'turret' : 'soldier'))).id
        const live = e.w.entities.some((x) => x.id === a.data.wave && !x.removed)
        return f >= len && !live
      })
    }
    case 'TurretOpen':
    case 'TurretClose':
      return playAnim(n, c, n.name === 'TurretOpen' ? 19 : 18, false, 12)
    case 'BombThrow':
      return timed(n, c, [18, 4], 32, {
        16: () => {
          const p = e.player
          if (!p) return
          const [mx, my] = e.muzzle(8)
          const [tx, ty] = p.footCenter()
          e.w.add(new Bomb(e.ctx, e, e.facing, mx << 8, my << 8, tx, ty))
        },
      })
    case 'FlameBreath':
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim([17, 4], true, true)
      }, (f) => {
        if (f >= 4 && f < 128) {
          const NOZ = [[46, 10], [16, -48], [-6, -12], [74, -12]]
          const [ox, oy] = NOZ[e.facing]
          e.w.add(new Flame(e.ctx, e, e.facing, (e.px + ox) << 8, (e.py + oy) << 8, e.w))
        }
        return f >= 128
      })
    case 'GrappleArm':
      return timed(n, c, [4], 60, {
        28: () => {
          const [mx, my] = e.muzzle(8)
          e.w.add(new GrappleArm(e.ctx, e, e.facing, mx << 8, my << 8))
        },
      })
    case 'EnergyBarrage':
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim(3, true, true)
      }, (f) => {
        if (f <= 48) {
          const [mx, my] = e.muzzle(10)
          const [vx, vy] = VEL(e.facing, 1024)
          e.w.add(new StraightShot(e.ctx, e, e.facing, 0, mx << 8, my << 8, { vx, vy, box: [4, 4, 8, 8], atk: ATK[4], mult: 51, color: ['#ffffff', '#ffd060'], radius: 2, life: 12, explode: false, name: 'barrage' }))
        }
        return f >= 48
      })
    case 'FinalFlash':
      return timed(n, c, [19, 3], 180, {
        24: () => {
          const [mx, my] = e.muzzle(14)
          e.w.add(new FullBeam(e.ctx, e, e.facing, 0, mx, my, { atk: ATK[11], mult: 328, life: 156 }))
        },
      })
    case 'MouthBeamDown':
      return new Act(n, c, () => e.setAnim(19, true, true), (f) => {
        if (f === 32) e.w.add(new FullBeam(e.ctx, e, 0, 0, e.px + 32, e.py - 16, { atk: ATK[11], mult: 328, life: 253 }))
        return f >= 301
      })
    case 'KidBuuKamehameha':
      return new Act(n, c, (a) => {
        e.facePlayer()
        e.setAnim(3, true, false)
        a.data.beam = 0
      }, (f, a) => {
        if (f === 8) {
          const OFF = [[8, -2], [12, -2], [12, -2], [4, -2]]
          const [ox, oy] = OFF[e.facing]
          const [dx, dy] = DIR_VEC[e.facing]
          const b = new GrowBeam(e.ctx, e, e.facing, 0, () => {
            const [cx, cy] = e.center()
            return [cx + ox - 8 + dx * 12, cy + oy + dy * 12]
          }, { atk: ATK[5], mult: 41, width: 20, speed: 3, maxLen: 80, color: ['#ffe8ff', '#ff70d0'] })
          a.data.beam = e.w.add(b).id
          e.beam = b
        }
        if (f >= 90) {
          e.beam?.stop()
          e.beam = null
          return true
        }
        return false
      }, () => {
        e.beam?.stop()
        e.beam = null
      })
    case 'StoneSpit':
      return timed(n, c, [19], 16, { 1: () => e.shot('spit') })
    case 'AimAtPlayer':
      return timed(n, c, [19], 8, {})

    // ---- teleports -----------------------------------------------------------------------------------------------------
    case 'NinjaTeleport':
      return new Instant(n, c, () => e.ninjaTeleport())
    case 'TeleportNearPlayer':
      return new Act(n, c, (a) => {
        a.data.side = e.w.rng.rand(4, 'teleport side')
        e.setAnim(17, true, false)
      }, (f, a) => {
        if (f === 36) e.teleportBeside(a.data.side)
        return f >= 54
      })
    case 'TeleportChain4':
      return new Act(n, c, () => e.setAnim(17, true, false), (f) => {
        if (f % 72 === 36) {
          const side = e.ctx.teleChain & 3
          e.teleportBeside(side)
          e.ctx.teleChain = (e.ctx.teleChain + 1) % 12
          if (e.ctx.teleChain % 4 === 0) return false
        }
        return f % 72 === 0 && e.ctx.teleChain % 4 === 0
      })
    case 'NinjaDisguise':
      return new Instant(n, c, () => e.setAnim(17, true, true))
    case 'NinjaReveal':
      return new Act(n, c, (a) => {
        a.data.x = e.x
        a.data.y = e.y
      }, (f, a) => {
        if (f % 2 === 0) {
          e.x = a.data.x + (e.w.rng.rand(3, 'jitter') - 1) * 256
          e.y = a.data.y + (e.w.rng.rand(3, 'jitter') - 1) * 256
        }
        if (f >= 64) {
          e.x = a.data.x
          e.y = a.data.y
          e.ctx.puff(e.w, ...e.center())
          return true
        }
        return false
      })

    // ---- grabs / holds --------------------------------------------------------------------------------------------------
    case 'VampireGrab':
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.ok = p && !p.dead && !p.heldBy ? 1 : 0
        if (a.data.ok) {
          p!.hold(e.w, e, 'vampire', 0)
          e.setAnim(17, true, true)
        }
      }, (_f, a) => {
        const p = e.player
        if (!a.data.ok || !p || p.dead || p.heldBy !== e) return true
        if ((e.w.frame & 15) === 0) {
          const d = p.loseHp(e.w, e.stats.POW)
          e.heal(d * 1024)
        }
        return false
      }, () => {
        const p = e.player
        if (p && p.heldBy === e) p.release(e.w)
      })
    case 'LatchDrain':
      return new Act(n, c, () => {
        const p = e.player
        if (p && !p.heldBy) p.hold(e.w, e, 'latch', 0)
        e.setAnim(0, true, true)
      }, () => {
        const p = e.player
        if (!p || p.dead || p.heldBy !== e) return true
        if ((e.w.frame & 63) === 0) p.loseHp(e.w, 1)
        return false
      }, () => {
        const p = e.player
        if (p && p.heldBy === e) p.release(e.w)
      })
    case 'GrabWindup':
      return playAnim(n, c, 17, false, 12)
    case 'BearHug':
      return new Act(n, c, (a) => {
        const p = e.player
        const yamu = e.ctx.liveEnemies().some((o) => o.type.type === 0x93)
        a.data.ok = p && !p.dead && !p.heldBy && yamu ? 1 : 0
        if (a.data.ok) {
          p!.hold(e.w, e, 'hug', 0)
          e.setAnim(17, true, true)
        }
      }, (f, a) => {
        const p = e.player
        if (!a.data.ok) return f >= 2
        if (!a.data.rel && (!p || p.heldBy !== e || e.drainDone)) {
          a.data.rel = f
          if (p && p.heldBy === e) p.release(e.w)
          e.drainDone = false
        }
        return !!a.data.rel && f - a.data.rel >= 60
      }, () => {
        const p = e.player
        if (p && p.heldBy === e) p.release(e.w)
      })
    case 'EnergyDrain':
      return new Act(n, c, (a) => {
        a.data.phase = 0
        a.data.lost = 0
      }, (f, a) => {
        const p = e.player
        if (!p || p.dead) return true
        if (a.data.phase === 0) {
          if (e.moveToward(0x100, 16, true)) a.data.phase = 1
          if (!p.heldBy) return true
          return false
        }
        if (a.data.phase === 1) {
          if (!p.heldBy) return true
          if (f % 8 === 0 && p.ep > p.maxEp / 4 && a.data.lost < p.maxEp / 4) {
            p.loseEp(1)
            a.data.lost++
          } else if (a.data.lost >= p.maxEp / 4 || p.ep <= p.maxEp / 4) {
            const spo = e.ctx.liveEnemies().find((o) => p.heldBy === o) as Enemy | undefined
            if (spo) spo.drainDone = true
            a.data.phase = 2
            a.data.at = f
          }
          return false
        }
        return f - a.data.at >= 60
      })
    case 'FreezePlayer':
      return new Act(n, c, () => {
        e.ctx.player?.hold(e.w, e, 'freeze', 0)
        e.setAnim(17, true, false)
      }, (f) => f >= 48, () => {
        const p = e.player
        if (p && p.heldBy === e) p.release(e.w)
      })
    case 'EatCandy':
      return new Act(n, c, (a) => {
        const p = e.player
        e.facePlayer()
        e.setAnim(19, true, true)
        a.data.ok = p && !p.dead ? 1 : 0
        if (a.data.ok) p!.hold(e.w, e, 'eat', 0)
      }, (f, a) => {
        const p = e.player
        if (!a.data.ok || !p) return true
        if (f >= 24 && f <= 111 && f % 8 === 0) {
          const d = Math.min(e.stats.POW >> 2, p.hp)
          const lost = p.loseHp(e.w, d, 1)
          e.heal(lost * 1024)
        }
        if (f >= 120) {
          e.ctx.candy = 0
          p.release(e.w)
          return true
        }
        return false
      }, () => {
        const p = e.player
        if (p && p.heldBy === e) {
          e.ctx.candy = 0
          p.release(e.w)
        }
      })

    // ---- Buu --------------------------------------------------------------------------------------------------------
    case 'BuuStretchPunch':
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.ok = p && p.hp > 0 && p.hittable ? 1 : 0
        if (a.data.ok) {
          e.facePlayer()
          e.setAnim(19, true, true)
        }
      }, (f, a) => {
        if (!a.data.ok) return true
        if (f === 64) {
          const OFF = [[9, 16], [15, -56], [-24, -16], [48, -16]]
          const [ox, oy] = OFF[e.facing]
          a.data.arm = e.w.add(new StretchArm(e.ctx, e, e.facing, ox, oy + (e.type.gfx === 0x2b ? -10 : 0))).id
        }
        if (f >= 72) return !e.w.entities.some((x) => x.id === a.data.arm && !x.removed)
        return false
      })
    case 'CandyBeam':
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.ok = p && p.hp > 0 && p.hittable ? 1 : 0
        if (a.data.ok) {
          e.facePlayer()
          e.setAnim(18, true, false)
        }
      }, (f, a) => {
        if (!a.data.ok) return true
        if (f === 16) {
          const OFF = [[16, 8], [16, -50], [-8, -26], [40, -26]]
          const [ox, oy] = OFF[e.facing]
          a.data.beam = e.w.add(new CandyBeam(e.ctx, e, e.facing, ox, oy)).id
        }
        if (f >= 32) return !e.w.entities.some((x) => x.id === a.data.beam && !x.removed)
        return false
      })
    case 'AbsorbStance':
      return new Act(n, c, () => e.setAnim(20, true, false), (f) => f >= 8)
    case 'AbsorbPose':
    case 'AbsorbPose2':
      return playAnim(n, c, 20, false, 8)
    case 'SuperBuuPowerUp':
      return new Act(n, c, () => {
        e.powered = true
        e.setAnim(11, true, false)
      }, (f) => f >= 32 && e.animDone())
    case 'ThrowBigBall':
      return new Act(n, c, () => {
        e.setAnim(15, true, true)
        e.w.add(new SuperBall(e.ctx, e))
      }, (f) => f >= 92)
    case 'GlideToWaypoint':
      return new Act(n, c, (a) => {
        const WP = [[224, 104], [344, 128], [120, 128], [264, 112], [144, 144]]
        e.ctx.waypoint = (e.ctx.waypoint + (e.w.rng.rand(2, 'waypoint') ? 1 : 4)) % 5
        const [tx, ty] = WP[e.ctx.waypoint]
        a.data.vx = Math.trunc((((tx - e.px) << 8) / 60))
        a.data.vy = Math.trunc((((ty - e.py) << 8) / 60))
        e.facing = facingToward(a.data.vx, a.data.vy)
        e.setAnim(14, true, true)
      }, (f, a) => {
        e.x += a.data.vx
        e.y += a.data.vy
        e.clampInside(e.w)
        return f >= 60
      })
    case 'SummonMinion':
      return timed(n, c, [17], 72, {
        40: () => {
          const OFF = [[8, 12], [8, -12], [-12, 8], [12, 8]]
          const [ox, oy] = OFF[e.facing]
          e.ctx.spawnEnemy(e.w, 0x0d, e.px + ox, e.py + oy, e)
        },
      }, false)
    case 'KidBuuSplitAnim':
      return wait(n, c, 55, 11, false)
    case 'KidBuuSplit':
      return new Instant(n, c, () => e.kidBuuSplit())
    case 'KidBuuMerge':
      return new Instant(n, c, () => e.kidBuuMerge())
    case 'BabidiHover':
      return new Act(n, c, () => (e.ctx.buuPacified = 0), (f) => {
        e.z = Math.max(-50, e.z - 1)
        e.setAnim(f & 8 ? 1 : 0, false, true)
        return false
      })
    case 'BabidiGesture':
      return new Act(n, c, () => e.setAnim(18, true, true), () => false)

    // ---- Janemba ------------------------------------------------------------------------------------------------------
    case 'JanembaSpawnParts':
      return new Instant(n, c, () => e.janembaParts())
    case 'JanembaHandStrikeA':
    case 'JanembaHandStrikeB':
      return new Act(n, c, () => e.setAnim(17, true, false), (f) => {
        if (f === 16) e.handStrike(n.name === 'JanembaHandStrikeA')
        return f >= 57
      })
    case 'JanembaAscend':
      return new Act(n, c, () => e.setAnim(18, true, true), (f) => {
        if (f >= 8) e.z = Math.max(24 - 3 * f, -200)
        return f > 128
      })
    case 'JanembaDescendSummon':
      return new Act(n, c, (a) => {
        e.z = -200
        a.data.cd = -1
        e.setAnim(18, true, true)
      }, (f, a) => {
        if (e.z < 0) {
          e.z = Math.min(0, e.z + 3)
          if (e.z === 0) a.data.cd = 16
          return false
        }
        if (a.data.cd > 0 && --a.data.cd === 0) e.topUpMinis()
        return a.data.cd === 0 && f > 70
      })
    case 'WaitMinisDefeated':
      return new Act(n, c, () => undefined, () => !e.ctx.liveEnemies().some((o) => o.type.type === 0x38))
    case 'MiniHopToward':
    case 'MiniHopFlee':
      return new Act(n, c, (a) => (a.data.ph = 0), (_f, a) => {
        a.data.ph = (a.data.ph + 8) & 0xff
        const air = a.data.ph >= 0x80
        e.z = air ? -Math.round(Math.sin(((a.data.ph - 0x80) / 128) * Math.PI) * 8) : 0
        if (a.data.ph === 0) {
          if (n.name === 'MiniHopToward') {
            if (e.w.rng.chance(0x4000, 'mini re-aim')) {
              const [dx, dy] = e.toPlayer()
              a.data.d = dx >= 0 ? (dy >= 0 ? 0 : 3) : dy >= 0 ? 2 : 1
            }
          } else e.facePlayer()
        }
        if (air) {
          if (n.name === 'MiniHopToward') {
            const V = [[128, 128], [-128, -128], [-128, 128], [128, -128]]
            const [vx, vy] = V[a.data.d ?? 0]
            e.moveBy(e.w, vx, vy)
          } else {
            const [dx, dy] = e.toPlayer()
            const m = Math.max(1, Math.hypot(dx, dy))
            e.moveBy(e.w, -Math.round((dx / m) * 128), -Math.round((dy / m) * 128))
          }
        }
        return false
      })
    case 'MiniRollAttack':
      return new Act(n, c, (a) => {
        e.setAnim(17, true, true)
        const [dx, dy] = e.toPlayer()
        a.data.vx = Math.sign(dx) * 384
        a.data.vy = Math.sign(dy) * 384
      }, (f, a) => {
        if (f > 16 && f < 32) e.moveBy(e.w, a.data.vx, a.data.vy)
        if (f === 32) {
          e.areaHit(e.bodyBox(), ATK[0], 0x400)
          return true
        }
        return false
      })
    case 'HandAmbush':
      return new Act(n, c, (a) => {
        const p = e.player
        if (p) {
          const left = e.w.rng.rand(2, 'hand side')
          e.x = (p.px + (left ? 4 : -20)) << 8
          e.y = p.y
          e.facing = left ? 2 : 3
        }
        e.setAnim(17, true, false)
        a.data.hit = 0
        e.alpha = 1
      }, (f, a) => {
        if (f >= 80 && f <= 103 && !a.data.hit && e.areaHit(e.bodyBox(), ATK[2], 0x400)) a.data.hit = 1
        return f >= 106
      })
    case 'HandSink':
      return new Act(n, c, () => undefined, (f) => {
        e.alpha = Math.max(0.2, 1 - f / 107)
        return f >= 107
      })

    // ---- Super Janemba --------------------------------------------------------------------------------------------
    case 'SuperJanembaWave':
      return timed(n, c, [18, 3], 40, {
        24: () => {
          const OFF = [[4, 4], [13, -24], [-8, -8], [24, -8]]
          const [ox, oy] = OFF[e.facing]
          const b = e.bodyBox()
          e.w.add(new SJWave(e.ctx, e, e.facing, (b.x0 + ox) << 8, (b.y0 + oy + 24) << 8))
        },
      })

    // ---- Broly / Dabura / Yakon / others --------------------------------------------------------------------------
    case 'BrolySpawnRocks':
      return new Instant(n, c, () => {
        if (e.w.entities.some((x) => x.kind === 'rock')) return
        for (const [x, y] of [[200, 136], [112, 272], [296, 240]]) {
          const b = e.w.arena.bounds
          if (x + 16 >= b.x0 && x + 48 <= b.x1 && y + 16 >= b.y0 && y + 48 <= b.y1) {
            const rock = new Rock(x, y)
            rock.fx = e.ctx.fx('fx_0875f7ac')
            e.w.add(rock)
          }
        }
      })
    case 'JumpSmash':
      return new Act(n, c, () => e.setAnim(18, true, true), (f) => {
        if (f >= 8 && f < 56) e.z -= 8
        else if (f >= 56 && f < 200) {
          const [dx, dy] = e.toPlayer()
          e.x += Math.sign(dx) * 256
          e.y += Math.sign(dy) * 256
          e.clampInside(e.w)
        } else if (f >= 200 && f < 248) e.z = Math.min(0, e.z + 8)
        if (f === 248) {
          e.z = 0
          for (let i = -1; i <= 1; i++) e.ctx.puff(e.w, e.center()[0] + i * 12, e.py + 4)
          return true
        }
        return false
      })
    case 'FinishLandingAnim':
      return wait(n, c, 11, 'idle')
    case 'StunnedAnim':
    case 'StunnedAnim21':
    case 'PlayAnimHold':
      return playAnim(n, c, 20, true)
    case 'RecoverAnim':
      return playAnim(n, c, 19, false, 8)
    case 'PlayAnim0':
      return wait(n, c, 80, 0)
    case 'PlayAnim12':
    case 'PlayAnim12b':
    case 'PlayAnim12Swarm':
    case 'PlayAnim12Yakon':
    case 'PlayAnim12Yakon2':
      return playAnim(n, c, 17, false, 16)
    case 'PlayAnim13':
    case 'PlayAnim13Yakon':
    case 'PlayAnimOnce':
      return playAnim(n, c, 18, false, 16)
    case 'PlayAnimTimed':
      return wait(n, c, 32, 19)
    case 'DarkSphere':
      return new Act(n, c, () => {
        const [cx, cy] = e.center()
        e.w.add(new DarkSphere(e.ctx, e, (cx + 2) << 8, (cy - 17) << 8))
        e.setAnim(17, true, true)
      }, () => e.ctx.sphereState === 1 || e.ctx.sphereState === 3, () => (e.hittable = true))
    case 'TauntAnim':
      return playAnim(n, c, 4, false, 12)
    case 'Hop':
      return new Act(n, c, (a) => (a.data.ph = 0), (_f, a) => {
        a.data.ph = (a.data.ph + 8) & 0xff
        e.z = a.data.ph >= 0x80 ? -Math.round(Math.sin(((a.data.ph - 0x80) / 128) * Math.PI) * 8) : 0
        return false
      })
    case 'SummonMiniYakons':
      return new Instant(n, c, () => {
        for (let i = 0; i < 5; i++) e.ctx.spawnEnemy(e.w, 0x92, 192 + 20 * i, 96, e)
      })
    case 'WaitSummonsDead':
      return new Act(n, c, () => undefined, () => !e.ctx.liveEnemies().some((o) => o.type.type === 0x92))
    case 'SpawnEnzymes':
      return new Act(n, c, (a) => {
        a.data.t = 120
        a.data.n = e.spawned
      }, () => {
        if (e.spawned >= 8) return false
        if (--e.enzTimer <= 0) {
          const off = e.w.rng.rand(2, 'enzyme spot') ? [16, -8] : [0, 0]
          if (e.ctx.spawnEnemy(e.w, 0x18, e.px + off[0], e.py + off[1], e)) {
            e.spawned++
            e.enzTimer = 120
          }
        }
        return false
      })
    case 'MechTransform':
      return playAnim(n, c, 19, false, 12)
    case 'RaiseShield':
    case 'LowerShield':
    case 'ShieldBreak':
      return new Instant(n, c, () => {
        e.shield = n.name === 'RaiseShield' ? 1 : n.name === 'LowerShield' ? 0 : -1
        if (n.name === 'ShieldBreak') e.ctx.puff(e.w, ...e.center())
      })
    case 'GhostAnimA':
    case 'GhostAnimB':
      return playAnim(n, c, 17, false, 16)
    case 'WaitFormationSlot':
      return new Act(n, c, () => undefined, () => e.slot !== 0)
    case 'MoveToFormationA':
    case 'MoveToFormationB':
      return new Act(n, c, () => undefined, () => {
        const p = e.player
        if (!p) return true
        const r = n.name === 'MoveToFormationA' ? 40 : 24
        const a = ((e.slot || 1) - 1) * (Math.PI / 4)
        const [px, py] = p.footCenter()
        return e.moveToPoint(px + Math.round(Math.cos(a) * r) - (e.footW >> 1), py + Math.round(Math.sin(a) * r) - (e.footH >> 1), 0x100)
      })
    case 'WormSpawnBody':
      return new Instant(n, c, () => e.wormSpawnBody())
    case 'WormHeadMove':
      return new Act(n, c, () => undefined, () => {
        e.wormHeadStep()
        return false
      })
    case 'WormBodyFollow':
      return new Act(n, c, () => undefined, () => {
        e.wormBodyStep()
        return false
      })

    // ---- reaction states (buu_ai_extra.json) ------------------------------------------------------------------------
    case 'StunHold':
      return new Instant(n, c, () => {
        e.stunned = true
        e.setAnim(9, true, true)
      })
    case 'DeathBlink':
      return new Act(n, c, () => undefined, (f) => {
        e.alpha = f & 2 ? 0.3 : 1
        return f >= P(n, 'frames', 32)
      })
    case 'ShockStun':
      return new Act(n, c, () => {
        e.stunned = true
        e.setAnim(9, true, true)
      }, (f) => f >= 48, () => (e.stunned = false))
    case 'VolleyballTransform':
      return new Instant(n, c, () => {
        e.volleyball = true
        e.ctx.puff(e.w, ...e.center())
      })
    case 'VolleyballRestore':
      return new Instant(n, c, () => {
        e.volleyball = false
        e.ctx.puff(e.w, ...e.center())
      })
    case 'VolleyballLaunch':
      return new Act(n, c, () => undefined, (f) => {
        const [dx, dy] = DIR_VEC[e.lastHitDir]
        if (!e.moveBy(e.w, dx * 1024, dy * 1024)) return true
        const p = e.ctx.player
        if (p) e.areaHitEnemies(e.bodyBox(), p)
        return f >= 30
      })
    default:
      return new Never(n, c)
  }
}

