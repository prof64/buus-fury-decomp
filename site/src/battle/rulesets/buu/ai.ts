// Interpreter for the decoded Buu's Fury enemy AI (data/buu/enemy_ai.json + data/battle/buu_ai_extra.json):
// an AI stack of states (obj+0x238) whose top state's behaviour tree runs once per frame, plus the on-hit ("aux")
// tree of the top state, evaluated through each node's onHit function when the enemy is hit.
// Semantics: re/buu/notes.md §8 / §8b, enemy_ai.json node_types[].semantics, and the onHit disassembly in
// tools/battle/gen_buu_ai_extra.py. Every node type of the data is implemented; NODE_NOTES lists the ones whose
// details are inferred (tables not decoded, cosmetic nodes).

import { DIR_VEC, facingToward, overlaps, type Dir } from '../../core/geom'
import { SIN256, type HitMsg } from './actor'
import { GFX_OFFSET, type AiNodeJ } from './data'
import type { Enemy, KnockState } from './enemy'
import { ATK } from './damage'
import { PathMover } from './path'
import {
  Bomb, CandyBeam, DarkSphere, EnergyWave, Flame, FullBeam, GrappleArm, KameBeam, Rock, SJWave, StretchArm, SuperBall, kiBlast,
} from './projectiles'
import { BeamGfx } from './fx'
import { FINAL_FLASH_TABLES } from './skillfx'
import { audioEvent } from '../../audio/bus'
import { ENEMY_SETANIM, GENERIC_STATES, type EAnimRule } from './eanim'

export const NODE_NOTES: Record<string, string> = {
  IfPlayerInZone: 'sniper zone rectangle not decoded: true within 160 px',
  IfPlayerInRoomZone: 'vampire lair region not decoded: always true',
  VampireSummon: 'spawns the puff effect only (the spawned object is the 0x086FD9D0 effect run)',
  MoveToFormationA: 'formation offset tables 0x0877AE80/0x0877AECC not decoded: ring of 8 slots at 40/24 px',
  GhostAnimA: 'cosmetic appear/vanish animation',
  TankDrive: 'turn helpers not decoded: axis-aligned drive toward the player at 0.5 px/frame',
  IfUnknown: 'condition 0x0804EE25 not decoded: false',
  ShockStun: 'electric-shock stun length not decoded: 48 frames',
  VolleyballLaunch: 'launched ball flight not decoded: 4 px/frame for 30 frames, hits other enemies',
  InlineScript: 'bytecode not run (scripted escape / cutscene)',
  RunScript: 'cutscene scripts are not run',
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
  /** the running EnergyBarrage (its motion controller 0x030030E0) */
  barrage?: Act
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
  }
  push(id: number) {
    if (this.stack.length >= 8) return
    // the pushed state's animation replaces a running barrage controller (exit 0x0802E1BA stops sound 63): the node
    // under the push finds it gone when the state pops (wf-core nodes.rs `push`)
    const b = this.barrage
    if (b && !b.data.gone && b.f < 48) {
      b.data.gone = 1
      audioEvent('sfx_stop_match', { id: 63 })
    }
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
    // 0x0800AB0C ignores the root's result: a finished tree is not restarted (its composites keep their state;
    // emulator: the Majin Fighter's 0x14C Sequence[StandStill, FacePlayer] is not re-entered, lockstep
    // f02_vs_enemy_melee)
    void g
    t.rt.update()
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
    case 'TankDamage':
      // 0x08032F84 (the tanks' armour): as ApplyDamage, the damage / 4 (signed divide 0x08046E58) when the hit's
      // direction is the tank's facing
      if (msg.mult === 0) return 0
      return e.takeHit(e.facing === msg.dir ? { ...msg, dmg: Math.trunc((msg.dmg ?? 0) / 4) } : msg) ? 2 : 1
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
      // 0x080299FE: ((frac * whole max HP) >> 16) whole HP
      return e.hp1024 < Math.floor((P(n, 'frac') * (e.maxHp1024 >> 10)) / 65536) * 1024 ? kid() : 0
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
      // 0x0803A79C: amount = (s32)(dmg * frac) >> 16, a 32-bit product. With frac 100% (Yakon's energy absorb) that
      // is the low 16 bits of the damage, sign-extended: a hit whose damage has bit 15 set (32..64 HP, mod 64)
      // wraps negative and hurts instead of healing (re/buu/YAKON.md, VERIFIED)
      if (msg.mult === 0) return 1
      e.heal(Math.imul(msg.dmg ?? 0, P(n, 'frac')) >> 16)
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
  /** 0x08018EF6: enters the child at the current index (the index is not reset: a re-entered sequence resumes) */
  enter() {
    this.kids[this.i]?.enter()
  }
  /** 0x08018F0C: a finished child exits and the next one enters; after the last the index wraps to 0 WITHOUT
   *  entering the first child again (the next update runs it as it is) and the sequence reports done. A state change
   *  inside does not stop the evaluation (the replaced tree finishes its update) */
  update() {
    if (this.i >= this.kids.length) {
      this.i = 0
      return 1
    }
    const r = this.kids[this.i].update()
    if (!r) return 0
    this.kids[this.i].exit()
    this.i++
    if (this.i >= this.kids.length) {
      this.i = 0
      return 1
    }
    this.kids[this.i].enter()
    return 0
  }
  /** 0x08018F74: exits the current child (the index is kept) */
  exit() {
    if (this.i < this.kids.length) this.kids[this.i].exit()
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
    for (let i = 0; i < this.kids.length; i++) {
      const r = this.kids[i].update()
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

/** ParallelAll (vtable 0x0804BDA8: update 0x08019108) / ParallelAny (0x0804B6D0: 0x08019162): every child is
 *  updated on every update, finished or not; the result is the AND / OR of their results. Enter / exit: all children */
class Parallel extends Base {
  kids: RNode[]
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
    for (const k of this.kids) k.enter()
  }
  update() {
    let r = this.any ? 0 : 1
    for (const k of this.kids) {
      const v = k.update() ? 1 : 0
      r = this.any ? r | v : r & v
    }
    return r
  }
  exit() {
    for (const k of this.kids) k.exit()
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
      // 0x080342AF: the child's result is returned; a finished child is not exited (it runs on while the
      // condition holds)
      return this.kid.update()
    }
    if (this.on && this.kid) this.kid.exit()
    this.on = false
    return 0
  }
  /** 0x08034307: exits the running child (the flag is reset by the next enter) */
  exit() {
    if (this.on && this.kid) this.kid.exit()
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
function timed(n: AiNodeJ, c: AiController, anim: number | (string | number)[], len: number, events: Record<number, () => void>, face = true, rule?: EAnimRule) {
  return new Act(
    n, c,
    () => {
      if (face) c.e.facePlayer()
      c.e.setAnim(anim, true, false, rule)
    },
    (f) => {
      events[f]?.()
      if (f >= len) {
        // one SetAnim(0) at the end: the controller's own end (the recorded sequence, this frame's anim tick) or here
        if (c.e.anim?.name !== 'idle' || !c.e.animEnded) c.e.setAnim('idle', true, true)
        return true
      }
      return false
    },
  )
}

/** CandyBeam origin offsets (px from the footprint position) by facing: 0x08752E48, gfx 0x2B 0x08752E28 */
const CANDY_OFF: [number, number][] = [[12, -8], [14, -38], [-16, -34], [40, -34]]
const CANDY_OFF_2B: [number, number][] = [[12, -8], [14, -38], [-16, -24], [40, -24]]
/** VampireGrab: the vampire stands at the player's position minus this offset (px) by its facing (0x08759334) */
const VAMPIRE_GRAB_OFF: [number, number][] = [[0, 16], [0, -16], [-16, 0], [16, 0]]

// [H] ---- helpers of the hold nodes (BearHug / EnergyDrain) ------------------------------------------------------------
const DPAD_MASK = 0xf0
/** 0x0800EEB0(obj, id) / vfunc +0x34: the actor shows SetAnim(id)'s animation (raw 0x08072670[id] + 1) */
function isAnimId(e: Enemy, id: number): boolean {
  return e.anim?.raw === ENEMY_SETANIM[id]?.raw
}
/** straight-line mover (0x0800A1D4: base 0x08009D24, vtable 0x0804B308; SetAnim(1) on creation): ticks n =
 *  max(|dx|, |dy|) (24.8) / speed (truncating, 0 -> 1, velocity 0), velocity = d / n (truncating), facing on the
 *  dominant axis. Step 0x0800A01D: move by the velocity; the new footprint blocked -> back, SetAnim(0) unless already;
 *  else SetAnim(1) unless already walking, face the stored direction, n - 1; 0 -> exactly at the target, arrived */
class LineMover {
  e: Enemy
  tx: number
  ty: number
  vx = 0
  vy = 0
  n: number
  dir: Dir
  constructor(e: Enemy, tx: number, ty: number, speed: number) {
    this.e = e
    this.tx = tx
    this.ty = ty
    const dx = (tx << 8) - e.x
    const dy = (ty << 8) - e.y
    const ax = Math.abs(dx)
    const ay = Math.abs(dy)
    this.dir = ax >= ay ? (dx >= 0 ? 3 : 2) : dy >= 0 ? 0 : 1
    this.n = Math.trunc(((ax >= ay ? ax : ay) << 8) / speed) >> 8
    if (this.n) {
      this.vx = Math.trunc((dx << 8) / (this.n << 8))
      this.vy = Math.trunc((dy << 8) / (this.n << 8))
    } else this.n = 1
    e.facing = this.dir
    e.setAnim('walk', true, true)
  }
  update(): boolean {
    const e = this.e
    e.unmarkSelf()
    const ok = e.fits(e.w, (e.x + this.vx) >> 8, (e.y + this.vy) >> 8)
    if (!ok) {
      e.markSelf()
      if (!isAnimId(e, 0)) e.setAnim('idle', true, true)
      return false
    }
    e.x += this.vx
    e.y += this.vy
    if (!isAnimId(e, 1)) e.setAnim('walk', true, true)
    e.facing = this.dir
    e.markSelf()
    if (--this.n) return false
    e.unmarkSelf()
    e.x = this.tx << 8
    e.y = this.ty << 8
    e.markSelf()
    return true
  }
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
      return new Cond(n, c, () => e.hp1024 < thr, () => (thr = Math.floor((P(n, 'frac') * (e.maxHp1024 >> 10)) / 65536) * 1024))
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
      // 0x080331EC: only while IsAnim(0); the footprint centres more than 16 px apart along the facing, less than 16
      // across (Earth segment 3-2: a tank that just started driving does not roll its Chance)
      return new Cond(n, c, () => {
        if (!e.isAnim(0)) return false
        const [dx, dy] = e.toPlayer()
        const ax = Math.abs(dx)
        const ay = Math.abs(dy)
        return e.facing === 0 ? ax < 16 && dy > 16 : e.facing === 1 ? ax < 16 && dy < -16 : e.facing === 2 ? ay < 16 && dx < -16 : ay < 16 && dx > 16
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
        // 0x08037B96 (vtable 0x0804B0F8, condition 0x08037B48): a pure overlap test (no hold on the player: several
        // enzymes can latch) - the player's box (0x0800FE78) narrowed 6 px a side against the enemy's whole box, any
        // facing (real run 21-6 r064_004 f254: a 1-px overlap at the box's bottom row latches)
        if (!p) return false
        const pb = p.bodyBox()
        const eb = e.bodyBox()
        return overlaps({ x0: pb.x0 + 6, y0: pb.y0, x1: pb.x1 - 6, y1: pb.y1 }, eb)
      })
    case 'IfButtonMashed': {
      // create 0x080363FA (vtable 0x0804A6D0; built with the state's tree): count +0x10 = 0, stamp +0xC = now - gap - 1.
      // Condition 0x080363C4: a new A/B/d-pad press this frame (input 0x03003300 +4 & 0xF3) -> stamp = now, count + 1,
      // true once count > presses; no press and now - stamp > gap -> count = 0 (lockstep f02_vs_hold_latch)
      const presses = P(n, 'presses')
      const gap = P(n, 'gap')
      let count = 0
      let stamp = e.ctx.clock(e.w) - gap - 1
      return new Cond(n, c, () => {
        const now = e.ctx.clock(e.w)
        if (e.ctx.pressed & 0xf3) {
          stamp = now
          return ++count > presses
        }
        if (now - stamp > gap) count = 0
        return false
      })
    }
    case 'IfPlayerOnTop':
      return new Cond(n, c, () => {
        const [dx, dy] = e.toPlayer()
        return Math.abs(dx) < 16 && Math.abs(dy) < 16
      })
    case 'IfPartnerHolding':
      // [H] 0x0802F945: the first world object of class 0xFC (Spopovich) is in 0x03001B30 (the end of GrabWindup)
      // showing anim id 0x12's animation (raw 20, 0x0800EEB0)
      return new Cond(n, c, () => {
        const spo = e.ctx.liveEnemies().find((o) => [0x6d, 0x6e, 0x6f, 0x70].includes(o.type.type)) as Enemy | undefined
        return !!spo && spo.ea.r === GENERIC_STATES['0x03001B30'] && isAnimId(spo, 0x12)
      })
    case 'IfUnknown':
      // 0x0804EE24: 1 - [0x0300302C] - Majin Buu is not pacified (his hurt states under Babidi's spell pop at once)
      return new Cond(n, c, () => e.ctx.buuPacified !== 1)

    // ---- simple actions ---------------------------------------------------------------------------------------------
    case 'Succeed':
      return new Instant(n, c, () => undefined)
    case 'Idle':
      // node 0x080716F4 (vtable 0x08049A1C): enter no-op, update returns 0 - no animation change
      return new Act(n, c, () => undefined, () => false)
    case 'RunMotionController':
    case 'PartMirrorAnim':
      return new Act(n, c, () => !e.moving && e.setAnim('idle', false, true), () => false)
    case 'StandStill':
      // enter 0x0804E50C: SetAnim(0) (the idle state's enter draws its start tick, rand(255)) (emulator: lockstep
      // f02_vs_enemy_melee, the aggro frame); update 0x08019240: unless IsAnim(0) (vfunc +0x34) SetAnim(0) again,
      // then succeed (lockstep f02_vs_enemy_t5b: a Sequence [StandStill, FacePlayer] resumed after a ChasePlayer walk
      // frame re-idles in the update)
      return new Act(n, c, () => e.setAnim('idle', true, true), () => {
        if (!e.isAnim(0)) e.setAnim('idle', true, true)
        return true
      })
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
      return new Instant(n, c, () => {
        // the node enemy_ai.json calls SpawnPuff (create 0x0802F21C, singleton 0x030030FC, vtable 0x08049A5C): its
        // enter 0x0804E53C detaches the enemy from the enemy grid for good (0x0800F924: obj+0x291 = 1, tiles freed;
        // the Ghost's first state - Kyodai Pyramid 14-17)
        if (n.name === 'SpawnPuff') {
          e.unmarkSelf()
          e.detached = true
        }
        e.ctx.puff(e.w, ...e.center())
      })
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
    case 'ChasePlayerThroughRocks': {
      // 0x0802A00C / 0x0802A028: goal = the player's position, path-follow it (path.ts); within 2 px of it the
      // enemy stands (idle anim) - the node never finishes by itself
      const path = new PathMover(speed)
      return new Act(n, c, () => {
        const p = e.player
        if (!p) return
        path.gx = p.x
        path.gy = p.y
        path.enter(e)
      }, () => {
        const p = e.player
        if (!p) return false
        path.gx = p.x
        path.gy = p.y
        const dx = path.gx - e.x
        const dy = path.gy - e.y
        if ((Math.imul(dx, dx) >> 8) + (Math.imul(dy, dy) >> 8) <= 0x400) {
          // 0x0802A086: within range the update calls vfunc +0x28 = SetAnim(0) on EVERY update (the idle enter draws
          // rand(255) each frame; lockstep f02_vs_enemy_t18, Enzyme)
          e.setAnim('idle', true, true)
          return false
        }
        path.update(e)
        return false
      })
    }
    // enemy_ai.json: StepTowardPlayer (renamed in data.ts AI_RENAME): the game backs off, facing the player
    case 'BackOffFacingPlayer':
    case 'BackOffFacingPlayerThroughRocks':
      // no enter: the step (0x08018CE2) sets SetAnimRaw(1) + state 0x03002B9C (the walk cycle played backwards, tick
      // 0x08018C24) when that is not already the state (a back-off entered again keeps the reversed walk running)
      return new Act(n, c, () => undefined, () => e.stepFromPlayer(speed, true))
    case 'StepAwayFromPlayer':
      return new Act(n, c, () => undefined, () => e.stepFromPlayer(speed, false))
    case 'WalkToPlayer': {
      const st = { n: 0 }
      return new Act(n, c, () => (st.n = 0), () => e.walkToPlayer(speed, st))
    }
    case 'FlyToPlayer':
      return new Act(n, c, () => undefined, () => e.flyToPlayer(speed))
    case 'AlignWithPlayer':
    case 'LineUpForShot':
      return new Act(n, c, () => undefined, () => e.align(speed || 0x73))
    case 'Wander':
      return new Act(n, c, () => undefined, () => {
        e.wanderStep(speed, P(n, 'turn_p'))
        return false
      })
    case 'MoveToPoint': {
      // ctor 0x0802E104 (goal = the map point), enter 0x0803BB00, update 0x0802E0A8: path-follow, finished within
      // 8 px of the point on both axes
      const path = new PathMover(speed)
      return new Act(n, c, () => {
        path.gx = P(n, 'x') << 8
        path.gy = P(n, 'y') << 8
        path.enter(e)
      }, () => {
        path.update(e)
        return Math.abs(path.gx - e.x) < 0x800 && Math.abs(path.gy - e.y) < 0x800
      })
    }
    case 'WanderToRandomPoint': {
      // ctor 0x0803BD64 (speed 0xB3), enter 0x0803BC2C: random points x = 96 + rand(294), y = 112 + rand(240) until
      // one is passable (vfunc +0x54) and at least sqrt(80) px away; update 0x0803BCAA = MoveToPoint's
      const path = new PathMover(0xb3)
      return new Act(n, c, () => {
        path.counter = 0 // not set by this enter (pool memory): re-plan on the first update
        for (let i = 0; i < 1000; i++) {
          const gx = (0x60 + e.w.rng.rand(0x126, 'wander point')) << 8
          const gy = (0x70 + e.w.rng.rand(0xf0, 'wander point')) << 8
          if (!e.passable(e.w, gx >> 8, gy >> 8)) continue
          const dx = gx - e.x
          const dy = gy - e.y
          if ((Math.imul(dx, dx) >> 8) + (Math.imul(dy, dy) >> 8) < 0x5000) continue
          path.gx = gx
          path.gy = gy
          break
        }
      }, () => {
        path.update(e)
        return Math.abs(path.gx - e.x) < 0x800 && Math.abs(path.gy - e.y) < 0x800
      })
    }
    case 'CirclePlayer':
      return new Act(n, c, () => undefined, () => e.circlePlayer(speed))
    case 'HoverDrift':
      // update 0x08031004 (vtable 0x0804CB80): a pending hit impulse (+-0x400 along the hit dir, the reaction above)
      // is clamped to +-128 on x, else on y; then per axis: d = player centre - own centre; d > 0 ? (v < 128: v +=
      // 38) : (v > -128: v -= 38) - no clamp after, so |v| reaches 152 (38 = int(38.4f)); facing by the larger of
      // |vx| / |vy| (ties: y); pos + v, blocked -> x only, then y only. The velocity lives in the node (+4 / +8): a
      // re-entered state starts from 0 (lockstep f02_vs_enemy_t48, after the BackStep state 0x172).
      // (enter 0x0804EB5C: the velocity 0, then SetAnim(1) - 0x0804EB68, the walk)
      return new Act(n, c, () => {
        e.vx = 0
        e.vy = 0
        e.setAnim('walk', true, true)
      }, () => {
        if (Math.abs(e.vx) > 0x200 || Math.abs(e.vy) > 0x200) {
          if (e.vx > 128) e.vx = 128
          else if (e.vx < -128) e.vx = -128
          else e.vy = Math.max(-128, Math.min(128, e.vy))
        }
        const [dx, dy] = e.toPlayer()
        if (dx > 0) {
          if (e.vx < 128) e.vx += 38
        } else if (e.vx > -128) e.vx -= 38
        if (dy > 0) {
          if (e.vy < 128) e.vy += 38
        } else if (e.vy > -128) e.vy -= 38
        e.facing = Math.abs(e.vx) > Math.abs(e.vy) ? (e.vx > 0 ? 3 : 2) : e.vy > 0 ? 0 : 1
        e.walkBy(e.vx, e.vy)
        return false
      })
    case 'BackStep':
      // node vtable 0x0804CFDC. Enter 0x08031388 first runs vfunc +0x10 = 0x0803125C, a bump hit: face the player on
      // the dominant axis, standard melee x1.0 (0x086B8098) with the strikeRect box, accepted -> sound 48 + rand(2)
      // (lockstep f02_vs_enemy_t48); then the hop velocity below. Update 0x080313EA: counter 48 decremented first,
      // 0 -> finished without moving (47 moves); SetAnimRaw(1) + state 0x03002B9C unless already in it.
      return new Act(n, c, (a) => {
        if (e.meleeStrike(0x400, true)) audioEvent('sfx', { id: 48 + e.w.rng.rand(2, 'enemy melee hit sfx'), vol: 64 })
        const s = ((e.w.rng.rand(64, 'backstep') + 96) * P(n, 'k', 16)) >> 7
        const [dx, dy] = DIR_VEC[e.facing]
        const side = ((e.w.rng.rand(32768, 'backstep side') - 16384) * s) >> 12
        a.data.vx = -dx * 8 * s + (dx ? 0 : side)
        a.data.vy = -dy * 8 * s + (dy ? 0 : side)
      }, (f, a) => {
        if (f >= 48) return true
        if (e.ea.r.rule !== 'reverse') e.setAnim(1, true, true, { rule: 'reverse' })
        // 0x080313EA adds both components and takes the step back whole when the footprint does not fit: no slide
        e.tryMove(a.data.vx, a.data.vy, 0, 0)
        return false
      })
    case 'KnockbackSlide': {
      // the parameter enemy_ai.json calls `frames` is the knockback constant K (node +0x0C): 16, 4 or 36
      const kb: KnockState = { vx: 0, vy: 0, n: 16, anim: null }
      // node vtable 0x0804A138: the exit (0x0804F570) does nothing - an interrupted slide leaves the hurt animation
      // running (its own end callback SetAnim(0)s; lockstep f02_vs_hold_vampire: no rand(255) when a hit's GotoState
      // replaces the slide)
      return new Act(n, c, () => e.knockStart(P(n, 'frames'), kb), () => e.knockStep(kb))
    }
    case 'LungeStep':
      // singleton 0x03003288: enter 0x08037A42 = SetAnimHold(4, 0) (vfunc +0x30: raw 5, hold state, frame 0; not the
      // punch state: no swing sound, no rand); update 0x08037A56: while anim 4 shows (0x0800EEB0) step 4 px in the
      // current facing (table 0x0876BA40, no re-aim) if the new position is passable (vfunc +0x54); never finishes
      // (always 0: the Selector falls through to its next children)
      return new Act(n, c, () => {
        e.setAnim(4, true, false, { rule: 'hold' })
      }, () => {
        if (e.ea.pos.raw !== 5) return false
        const [dx, dy] = DIR_VEC[e.facing]
        e.moveBy(e.w, dx * 1024, dy * 1024)
        return false
      })
    case 'FollowMajinBuu':
      return new Act(n, c, () => undefined, () => {
        const buu = e.ctx.liveEnemies().find((o) => [0x1a, 0x1b, 0x1c].includes(o.type.type))
        if (!buu) return false
        // update 0x0802B3EA: d = Buu's footprint centre - own (0x0800FDFA); always faces him (horizontal only when
        // |dx| > |dy|); from `dist` px on the dominant axis on: one step of d * 256 / max(|dx|, |dy|), taken only
        // when the whole step is free (emulator: real route 8-71)
        const [bx, by] = buu.footCenter()
        const [x, y] = e.footCenter()
        const dx = bx - x
        const dy = by - y
        e.facing = (Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 3 : 2) : dy > 0 ? 0 : 1) as Dir
        const m = Math.max(Math.abs(dx), Math.abs(dy))
        if (m >= P(n, 'dist', 50) && m > 0) {
          e.moving = true
          e.tryMove(Math.trunc((dx * 256) / m), Math.trunc((dy * 256) / m), 0, 0)
        }
        return false
      })
    case 'TankDrive': {
      // node 0x08049E20: enter 0x08032824 (turn timer +4 = 0), update 0x08032A64 (never finishes). d = the player's
      // footprint centre - own; a = the minor axis' direction toward the player, b = the major axis' one. Unless
      // |dx| <= 1 or |dy| <= 1: probe(a) -> drive(a), else probe(b) -> drive(b), else (and when aligned) turn(b).
      // turn 0x0803282A: facing == dir -> ok; timer running -> no; else SetAnim(0) unless idle (vfunc +0x34 / +0x28),
      // timer = 60, and the opposite direction is refused (facing kept: a tank never turns around directly).
      // drive 0x0803287E: turn ok -> 0.25 px in the facing (0x40), the footprint tested (vfunc +0x54): moved ->
      // SetAnim(1) unless walking, blocked -> SetAnim(0) unless idle. probe 0x08032948: n = (|player pos - own pos|
      // >> 8) / 8 on the axis (top-left positions; direction 1 measures x, a game quirk), n steps of 8 px along dir
      // must all fit. Lockstep f02_vs_enemy_t7f: facing down, the player left-up -> probe(up) ok, turn refused.
      const T = { timer: 0 }
      const isAnim = (k: number) => (k === 0 ? e.anim?.name === 'idle' : e.anim?.name === 'walk')
      const turn = (d: Dir) => {
        if (e.facing === d) return true
        if (T.timer) return false
        if (!isAnim(0)) e.setAnim('idle', true, true)
        T.timer = 60
        if ((e.facing ^ 1) === d) return false
        e.facing = d
        return true
      }
      const probe = (d: Dir) => {
        const p = e.player
        if (!p) return false
        const diff = d === 0 ? p.y - e.y : p.x - e.x
        const steps = Math.trunc((Math.abs(diff) >> 8) / 8)
        const [sx, sy] = DIR_VEC[d]
        e.unmarkSelf()
        let x = e.x
        let y = e.y
        let ok = true
        for (let i = 0; i < steps; i++) {
          x += sx * 0x800
          y += sy * 0x800
          if (!e.fits(e.w, x >> 8, y >> 8)) {
            ok = false
            break
          }
        }
        e.markSelf()
        return ok
      }
      const drive = (d: Dir) => {
        if (!turn(d)) return
        const [sx, sy] = DIR_VEC[e.facing]
        e.unmarkSelf()
        if (e.fits(e.w, (e.x + sx * 0x40) >> 8, (e.y + sy * 0x40) >> 8)) {
          e.x += sx * 0x40
          e.y += sy * 0x40
          if (!isAnim(1)) e.setAnim('walk', true, true)
        } else if (!isAnim(0)) e.setAnim('idle', true, true)
        e.markSelf()
      }
      return new Act(n, c, () => (T.timer = 0), () => {
        if (T.timer) T.timer--
        if (!e.player) return false
        const [dx, dy] = e.toPlayer()
        let aligned = Math.abs(dx) <= 1 || Math.abs(dy) <= 1
        let a: Dir, b: Dir
        if (Math.abs(dx) < Math.abs(dy)) {
          a = dx >= 0 ? 3 : 2
          b = dy >= 0 ? 0 : 1
        } else {
          a = dy >= 0 ? 0 : 1
          b = dx >= 0 ? 3 : 2
        }
        if (!aligned) {
          if (probe(a)) drive(a)
          else if (probe(b)) drive(b)
          else aligned = true
        }
        if (aligned) turn(b)
        return false
      })
    }
    case 'RandomStep':
      // create 0x08037C06 (vtable 0x0804B1C0; the Skeleton's bones rattle before they rise): enter 0x0804E824 keeps
      // the position; update 0x08037BBC = the kept position + (rand(3) - 1, rand(3) - 1) px written straight to the
      // actor (no grid test), never finished; exit 0x0804F638 puts the kept position back (Kyodai Pyramid 14-20)
      return new Act(n, c, (a) => {
        a.data.x = e.x
        a.data.y = e.y
      }, (_f, a) => {
        const dx = (e.w.rng.rand(3, 'rattle x') - 1) << 8
        const dy = (e.w.rng.rand(3, 'rattle y') - 1) << 8
        e.x = a.data.x + dx
        e.y = a.data.y + dy
        return false
      }, (a) => {
        e.x = a.data.x
        e.y = a.data.y
      })

    // ---- melee --------------------------------------------------------------------------------------------------------
    case 'MeleeAttack':
      // 0x0802A0C4: hit test once on the switch frame (an accepted hit plays its sound), then SetAnim(4 + rand(n)),
      // n = the sprite's punch animations (create fn 0x0802A288); the punch state 0x03001B50 plays the swing sound
      return new Act(n, c, () => {
        if (e.meleeStrike(0x400, true)) e.meleeHitSfx()
        e.setAnim(4 + e.w.rng.rand(e.punchAnims(), 'enemy punch anim'), true, false)
        // update 0x0802A276 = vfunc +0x34(0): done once the actor is back in the idle state (the punch state's end
        // SetAnim(0): 4 * frames ticks, e.g. 12 for a 3-frame punch - lockstep f02_vs_enemy_t21 frame 255)
      }, () => e.anim?.name === 'idle' && e.ea.r.rule === 'idle')
    case 'BioMechPunch':
      // create 0x0803CB14, controller vtable 0x0804C6B4: enter 0x0803C8F0 (counter +0x12 = 0); tick 0x0803C9F8: counter++,
      // frame = counter < 16 ? 0 : ((counter - 16) >> 3) + 1; at counter 16 the hit 0x0803C8FC (standard melee x1.0,
      // 0x086B8098; accepted -> sound 48 + rand(2) whatever the sprite); frame 4 (counter 40) ends the controller and the
      // node (0x0803CAFE: done once the actor left the controller state)
      return timed(n, c, 4, 40, { 16: () => { if (e.meleeStrike(0x400, false)) audioEvent('sfx', { id: 48 + e.w.rng.rand(2, 'enemy melee hit sfx'), vol: 64 }) } })
    case 'HeavyPunch':
      // enter 0x0803CB82: face the player, SetAnimRaw(4) + state 0x030032F4. State tick 0x0803CB50: frame 0 for 48
      // ticks, then (tick - 48) >> 3; SetAnim(0) once that reaches the animation's frame count (48 + 8 * frames
      // ticks). The strike (x1.5) at tick 48; an accepted hit plays 48 + rand(2) (0x0803CCFC, lockstep
      // f02_vs_enemy_t0c / t24)
      return new Act(n, c, (a) => {
        e.facePlayer()
        e.setAnim([4, 5], true, false)
        a.data.len = 48 + 8 * (e.anim ? e.ea.host.nframes(e.anim.raw) : 2)
      }, (f, a) => {
        if (f === 48 && e.meleeStrike(0x600, true)) audioEvent('sfx', { id: 48 + e.w.rng.rand(2, 'heavy punch hit sfx'), vol: 64 })
        if (f < a.data.len) return false
        // one SetAnim(0): the type's recorded state rule may already have ended this tick
        if (e.anim?.name !== 'idle' || !e.animEnded) e.setAnim('idle', true, true)
        return true
      })
    case 'WindupMelee': {
      // singleton 0x03003210: enter 0x0804E808 = SetAnimRaw(4) + SetState(0x03003214) (no facing change); update
      // 0x08050034 = finished as soon as obj+0x14 is no longer that controller: a later node of the same tick (the old
      // tree after the GotoState) that SetAnims ends the windup with no swing (lockstep f02_vs_enemy_t6b).
      // Controller tick 0x08033CF0 (shift 3): counter c = ticks since the enter; frame 0 while c < 32 (the swing at 32:
      // faces the player, hit x1.0, an accepted hit plays PlaySfx(62, vol 0x40) at 0x08033CE8), then frame
      // ((c - 32) >> 3) + 1 up to frame n - 1, which is held 32 ticks: the end (vfunc +0x24 -> SetAnim(0)) at
      // c = 64 + 8 * (n - 2) (n = the animation's frames; Skeleton: 4 -> 80)
      let seq = 0
      let len = 48
      return new Act(n, c, () => {
        e.setAnim([4, 5], true, false)
        seq = e.animSeq
        len = 64 + ((Math.max(2, e.ea.host.nframes(e.ea.pos.raw)) - 2) << 3)
      }, (f) => {
        if (e.animSeq !== seq) return true
        // tick 0x08033CF0: not hittable (obj+0x1CC = 0) from the swing at 32 until the last cel starts
        e.hittable = !(f >= 32 && f < len - 32)
        if (f === 32 && e.meleeStrike(0x400, true)) audioEvent('sfx', { id: 62, vol: 64 })
        if (f >= len) {
          e.setAnim('idle', true, true)
          return true
        }
        return false
      })
    }
    case 'HeavyStrike':
      // create 0x0802F918; enter 0x0802F74C: player active -> vfunc +0x30(4) (wind-up pose: no SetAnim, no sound),
      // armed, timer 60. Update 0x0802F8DA: timer-- ; at 0 the strike 0x0802F77C (sound 62, hit, hit sound) then
      // vfunc +0x28(4) = SetAnim(4) (the punch state: swing 50 + rand(2)); finished once the actor is out of that
      // state (its end SetAnim(0), before the AI) (lockstep f02_vs_enemy_t3e: GotoState 228, strike 288, idle 304)
      return new Act(n, c, (a) => {
        a.data.ok = e.player?.hittable ? 1 : 0
        if (a.data.ok) e.facePlayer()
      }, (f, a) => {
        if (!a.data.ok) return true
        if (f === 60) {
          e.heavyStrike()
          e.setAnim(4, true, false)
          return false
        }
        return f > 60 && e.anim?.name === 'idle'
      })
    case 'ChargedPunch':
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim(18, true, true)
      }, (f) => {
        // controller 0x03003278 (update 0x08036044): no animation change at the strike; at tick 0x8C the MeleeAttack
        // hit (0x0802A0C4's turn + strike rect, x1.5) and on an accepted hit the sound 48 + rand(2) (after the
        // damage roll); the controller's tick ends it at tick 148 with SetAnim(0) (one SetAnim(0): the recorded
        // sequence may already have done it), the node finishes on that update (lockstep f02_vs_enemy_t0a, Broly:
        // entered 365, hit 505, SetAnim(0) 513, next state 514)
        if (f === 140) {
          e.hittable = false
          if (e.meleeStrike(0x600, true)) audioEvent('sfx', { id: 48 + e.w.rng.rand(2, 'charged punch hit sfx'), vol: 64 })
        }
        if (f >= 148) {
          if (e.anim?.name !== 'idle' || !e.animEnded) e.setAnim('idle', true, true)
          return true
        }
        return false
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
    case 'DriftContact': {
      // node create 0x0802C69C (at the state's construction): s = rand(3) + 1, a = rand(256); v = (cos a, sin a) * s
      // >> 10 (SIN256, 1/256 px per frame). Update 0x0802C6EC: after a contact, 16 frames back along -v; else pos += v (no
      // collision test) and a kind-1 message (the player's footprint) with the body box, melee sub 2 x1.0, dir =
      // facing: accepted -> 16 frames back. Lockstep f02_vs_enemy_t07.
      const s3 = e.w.rng.rand(3, 'drift speed') + 1
      const ang = e.w.rng.rand(256, 'drift angle')
      const vx = (SIN256[(ang + 0x40) & 0xff] * s3) >> 10
      const vy = (SIN256[ang] * s3) >> 10
      let back = 0
      // enter 0x0804E850 (node vtable 0x0804B640 entry 0): detach from the enemy grid (0x0800F924: obj+0x291 = 1,
      // tiles freed; real route 21-4)
      return new Act(n, c, () => {
        e.unmarkSelf()
        e.detached = true
      }, () => {
        if (back > 0) {
          back--
          e.x -= vx
          e.y -= vy
          return false
        }
        e.x += vx
        e.y += vy
        const p = e.player
        if (p && p.onMessage(e.w, { team: 0, rect: e.bodyBox(), dir: e.facing, mult: 0x400, atk: ATK[2], attacker: e, source: e, foot: true })) back = 16
        return false
      })
    }

    // ---- ranged -------------------------------------------------------------------------------------------------------
    case 'KiBlast':
      return timed(n, c, [3], 32, {
        16: () => {
          // the player's spawner 0x08019858: origin 0x0800FF44 on the body box (down (cx, y), up (cx, cy), left (x0, cy),
          // right (x1, cy)); the blast (vtable 0x0804B454) tests a 16 x 16 box centred on its position (lockstep
          // f02_vs_enemy_t0a, Broly: spawned at (186, 174) with the enemy at (186, 190) facing left)
          const b = e.bodyBox()
          const cx = (b.x0 + b.x1) >> 1
          const cy = (b.y0 + b.y1) >> 1
          const [mx, my] = e.facing === 0 ? [cx, e.py] : e.facing === 1 ? [cx, cy] : e.facing === 2 ? [b.x0, cy] : [b.x1, cy]
          const s = kiBlast(e.ctx, e, e.facing, 0, mx << 8, my << 8, undefined, undefined, e.w)
          s.o.box = [8, 8, 16, 16]
          s.o.recBox = true
          e.w.add(s)
        },
      })
    case 'GunShot':
      // the node's own state objects: enter (0x080264D4 / 0x08026754 / 0x0802D0B0) SetAnimRaw(0x12) = raw 19, frame =
      // tick >> 2 (gun) / >> 3 (rifle, wrap) - not the type's recorded raw-19 rule (Earth 3-2)
      return timed(n, c, [18, 3], 16, { 4: () => e.shot('gun') }, true, { rule: 'once_stop', shift: 2, end: 'none' })
    case 'RifleShot':
      return timed(n, c, [18, 3], 32, { 8: () => e.shot('rifle') }, true, { rule: 'once_stop', shift: 3, end: 'none' })
    case 'NinjaStarThrow':
      // controller 0x03003074: anim 0x13 (raw 20; real route db6 13-32 f117)
      // a frame per 8 ticks; the star, its sound and the end are the node's (not the recorded sequence's cue / end)
      return timed(n, c, [19, 4], 32, { 8: () => e.shot('star') }, true, { rule: 'once_stop', shift: 3, end: 'none' })
    case 'MummyWrapShot':
      return timed(n, c, [18, 3], 24, { 8: () => e.shot('wrap') }, true, { rule: 'once_stop', shift: 3, end: 'none' })
    case 'MummyWrapBurst':
      return timed(n, c, [18, 3], 24, { 8: () => e.shot('burst') }, false, { rule: 'once_stop', shift: 3, end: 'none' })
    case 'TankShoot': {
      // controller 0x030030EC (tick 0x0802E278): SetAnimRaw(0x13) once, 8 ticks per frame, then SetAnim(0); the shell
      // when the frame reaches 2 (tick 16); finished once the controller is gone (Earth segment 3-14)
      let raw = -1
      let fired = false
      return new Act(
        n, c,
        () => {
          e.setAnim(18, true, false, { rule: 'once', shift: 3, end: 'idle' })
          raw = e.ea.pos.raw
          fired = false
        },
        (f) => {
          if (!fired && e.ea.pos.raw === raw && e.ea.pos.frame === 2) {
            fired = true
            e.shot('shell')
          }
          return f > 1 && e.ea.pos.raw !== raw
        },
      )
    }
    case 'SoldierThrow':
    case 'TurretFire': {
      // motion controllers: SoldierThrow 0x080266D8 (state 0x03002FCC, SetAnimRaw(0x13), tick 0x08026618) and
      // TurretFire 0x0802B684 (state 0x0300305C, SetAnimRaw(0x14), tick 0x0802B5EA); enter: face the player (event
      // 0x0804AE70). Tick: the wave (0x0802A2EC) at tick 8 from the per-facing muzzle (0x0874CA6C / 0x08752E78);
      // the soldier's ends at tick >= 32 once its wave is gone, the turret's at tick 72 (wave or not): SetAnim(0),
      // and the node is finished (lockstep f02_vs_enemy_t45: entered 32, SetAnim(0) at 64; t40: 51 -> 123)
      const soldier = n.name === 'SoldierThrow'
      return new Act(n, c, (a) => {
        e.facePlayer()
        e.setAnim(soldier ? [19, 3] : [20, 3], true, false, { rule: 'once_stop', shift: 3, end: 'none' })
        a.data.raw = e.ea.pos.raw
      }, (f, a) => {
        // the node's update (0x08026726) is finished as soon as the controller is no longer the actor's state object:
        // a hit's reaction replaced it (real route 8-31: a soldier hurt mid-throw moves on once it recovers)
        if (e.ea.pos.raw !== a.data.raw) return true
        // the turret's frame (controller tick 0x0802B5EA): tick >> 3 below 16, 1 up to 64, then 2
        if (!soldier) e.ea.pos.frame = f < 16 ? f >> 3 : f > 64 ? 2 : 1
        // the soldier's (0x08026618): below 32 tick >> 3, the fourth 8 ticks back on frame 1 (0, 1, 2, 1), then kept
        else if (f < 32) e.ea.pos.frame = f >> 3 > 2 ? (((f >> 3) + 1) & 1) + 1 : f >> 3
        if (f === 8) {
          a.data.wave = e.w.add(new EnergyWave(e.ctx, e, e.facing, ...e.muzzleFix(soldier ? 'soldier' : 'turret'))).id
          if (soldier) e.ctx.soldierWave = a.data.wave
        }
        // the soldiers share one controller: the last thrown wave decides (see context.ts soldierWave)
        const live = e.w.entities.some((x) => x.id === (soldier ? e.ctx.soldierWave : a.data.wave) && !x.removed)
        if (soldier ? f >= 32 && !live : f >= 72) {
          e.setAnim('idle', true, true)
          return true
        }
        return false
      })
    }
    case 'CyborgShot':
      // create 0x0802F708; controller state 0x03003108 (enter 0x0802F6A8: face, SetAnimRaw(0x12)); tick 0x0802F5F8:
      // counter++, < 16: frame = counter >> 3, counter 1 spawns the wave (0x0802A2EC); from 16 on, once the wave is no
      // longer in the object list: SetAnim(0) (idle, rand(255)) and the node finishes (lockstep f02_vs_enemy_t11; the
      // raw 18 animation itself never ends the controller)
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim([18, 3], true, false, { rule: 'once', shift: 3, end: 'hold' })
      }, (f, a) => {
        if (f === 1) a.data.wave = e.w.add(new EnergyWave(e.ctx, e, e.facing, ...e.muzzleFix('soldier'))).id
        if (f < 16 || e.w.entities.some((x) => x.id === a.data.wave && !x.removed)) return false
        e.setAnim(0, true, true)
        return true
      })
    case 'TurretOpen':
    case 'TurretClose':
      // 0x0802B701 / 0x0802B8B5: SetAnimRaw(0x13 / 0x12) under the one-shot state 0x03001B30 (8 ticks a frame, the
      // last frame kept); finished on the update that sees the anim frame (obj+0x11) at 3 (lockstep
      // f02_vs_enemy_t40: open entered at 1, the next state at 25; close entered at 26, the next state at 51)
      // TurretOpen draws the gun into the wall: not hittable (obj+0x1CC = 0) until TurretClose (emulator: real route
      // segment 8-36)
      return new Act(n, c, () => {
        e.setAnim(n.name === 'TurretOpen' ? 19 : 18, true, false, { rule: 'once_stop', shift: 3, end: 'none' })
        e.hittable = n.name !== 'TurretOpen'
      }, () => e.ea.pos.frame >= 3)
    case 'BombThrow': {
      // controller 0x0203DD04 (SetAnimRaw(0x12)): the bomb at tick 16, SetAnim(0) at 24 (lockstep f02_vs_enemy_bomb).
      // Enter 0x08026A26 (after facing the player): d = the player's position - own (top-left px), the component
      // across the facing zeroed, each clamped to +-128; the controller keeps v = d * 0x71D >> 8. Tick 0x08026994 at
      // 16: the bomb from the position + 0x0874CAD4[dir] (lockstep f02_vs_enemy_t2b)
      const BOMB_MUZZLE = [[12, -8], [12, -8], [16, -16], [0, -16]]
      const d = { x: 0, y: 0 }
      const t = timed(n, c, [18, 4], 24, {
        16: () => {
          const [ox, oy] = BOMB_MUZZLE[e.facing]
          e.w.add(new Bomb(e.ctx, e, e.facing, (e.px + ox) << 8, (e.py + oy) << 8, d.x, d.y))
        },
      })
      const enter = t.enter.bind(t)
      let raw = -1
      t.enter = () => {
        enter()
        raw = e.ea.pos.raw
        const p = e.player
        const cl = (v: number) => Math.max(-128, Math.min(128, v))
        d.x = p && e.facing >= 2 ? cl(p.px - e.px) : 0
        d.y = p && e.facing < 2 ? cl(p.py - e.py) : 0
      }
      // a hit's reaction replaced the controller: no bomb, finished (Earth segment 2-10: a bomber hurt mid-throw)
      const update = t.update.bind(t)
      t.update = () => (e.ea.pos.raw !== raw ? 1 : update())
      return t
    }
    case 'FlameBreath':
      return new Act(n, c, () => {
        // enter 0x08023FA8: SetAnimRaw(0x12) (raw 19) under the flame controller 0x03002FA8 (no swing), sound 63
        e.facePlayer()
        e.setAnim(18, true, true, { rule: 'hold' })
        audioEvent('sfx', { id: 63, vol: 64 })
      }, (f) => {
        // the controller 0x03002FA8 spawns a flame every tick 4..127; at 128 it ends: SetAnim(0) and the sound 63
        // stopped (0x0803E32C) (lockstep f02_vs_enemy_flames)
        // its frame (tick 0x08023F0E): 0 below tick 4, then (tick >> 2) & 3 with 0 shown as 2
        if (f < 128) {
          const g = (f >> 2) & 3
          e.ea.pos.frame = f < 4 ? 0 : g === 0 ? 2 : g
        }
        if (f >= 4 && f < 128) {
          const NOZ = [[46, 10], [16, -48], [-6, -12], [74, -12]]
          const [ox, oy] = NOZ[e.facing]
          e.w.add(new Flame(e.ctx, e, e.facing, (e.px + ox) << 8, (e.py + oy) << 8, e.w))
        }
        if (f === 128) {
          e.setAnim('idle', true, true)
          audioEvent('sfx_stop_match', { id: 63 })
        }
        return f >= 128
      })
    case 'GrappleArm':
      // enter 0x08037E2D: face the player (0x0801599C), SetAnimRaw(0x12) (raw 19), controller 0x03003294 (enter
      // 0x08037D58 t = 0); finished when the enemy leaves the controller (update 0x08037E7B). Controller tick
      // 0x08037D64 (before the AI): t++; t < 24 frame (t >> 2) & 1, < 28 frame 2, < 32 frame 3; t 28 spawns the arm
      // (0x08037EA8) at the integer position + 0x0876CCB4[dir], kept in 0x03003298; from t 32 SetAnim(0) once the arm
      // is no longer in the world list
      return new Act(n, c, (a) => {
        e.facePlayer()
        e.setAnim(18, true, true, { rule: 'hold' })
        a.data.arm = -1
      }, (t, a) => {
        if (t < 32) {
          const fr = t < 24 ? (t >> 2) & 1 : t < 28 ? 2 : 3
          e.frameIdx = e.ea.pos.frame = fr
          if (t === 28) {
            const O = [[7, 16], [-12, -56], [-24, -20], [48, -20]][e.facing]
            a.data.arm = e.w.add(new GrappleArm(e.ctx, e, e.facing, (e.px + O[0]) << 8, (e.py + O[1]) << 8)).id
          }
          return false
        }
        if (e.w.entities.some((x) => x.id === a.data.arm && !x.removed)) return false
        e.setAnim('idle', true, true)
        return true
      })
    case 'EnergyBarrage':
      return new Act(n, c, (a) => {
        // enter 0x0802E1D8 (Pikkon): face, anim 3 held by the controller 0x030030E0, sound 63
        e.facePlayer()
        e.setAnim(3, true, true, { rule: 'hold' })
        audioEvent('sfx', { id: 63, vol: 64 })
        c.barrage = a
      }, (f, a) => {
        // the controller (update 0x0802E146): tick + 1; at 48 it ends; else its frame min(tick >> 2, 2) and a flame
        // (0x080249E0, FlameBreath's class) at the integer position + 0x08757324[dir]
        if (a.data.gone) return true
        if (f < 48) {
          e.ea.pos.frame = Math.min(f >> 2, 2)
          const OFF = [[4, 4], [13, -24], [-8, -8], [24, -8]]
          const [ox, oy] = OFF[e.facing]
          e.w.add(new Flame(e.ctx, e, e.facing, (e.px + ox) << 8, (e.py + oy) << 8, e.w))
        }
        if (f === 48) audioEvent('sfx_stop_match', { id: 63 })
        return f >= 48
      }, (a) => {
        if (a.f < 48 && !a.data.gone) audioEvent('sfx_stop_match', { id: 63 })
        if (c.barrage === a) c.barrage = undefined
      })
    case 'FinalFlash':
      return timed(n, c, [19, 3], 180, {
        24: () => {
          const [mx, my] = e.muzzle(14)
          // the player's Final Flash class 0x08036FA2 with table 2 (0x087616EC)
          e.w.add(new FullBeam(e.ctx, e, e.facing, 0, mx, my, { atk: ATK[11], mult: 328, life: 156, table: FINAL_FLASH_TABLES[2] }))
        },
      })
    case 'MouthBeamDown':
      return new Act(n, c, () => e.setAnim(19, true, true), (f) => {
        if (f === 32) e.w.add(new FullBeam(e.ctx, e, 0, 0, e.px + 32, e.py - 16, { atk: ATK[11], mult: 328, life: 253, table: FINAL_FLASH_TABLES[2] }))
        return f >= 301
      })
    case 'KidBuuKamehameha':
      // motion controller (vtable 0x0804C690): enter builds the player's beam class 0x080206B8 (KameBeam, Kamehameha
      // Lv1: x0.04, sub 5; sound 3) at the position + 0x0875928C[dir]; its control fires the beam at tick 8 (stop 3,
      // sound 32) and counts; its tick ends the state once tick > 90 (SetAnim(0), the exit stops the beam: it
      // retracts, stop 32 + sound 33 when gone). Lockstep f02_vs_enemy_beam.
      return new Act(n, c, (a) => {
        e.facePlayer()
        e.setAnim(3, true, false)
        const OFF = [[16, 20], [16, -4], [-12, 2], [44, 2]]
        const [ox, oy] = OFF[e.facing]
        const gfx = new BeamGfx(e.ctx.fx('buu_beam_kamehameha_1'), e.ctx.fx('fx_08721164'))
        const b = new KameBeam(e.ctx, e, e.facing, e.px + ox, e.py + oy, e.w.frame, { atk: ATK[5], mult: 41, color: ['#ffe8ff', '#ff70d0'], gfx, team: 0, sounds: true })
        a.data.beam = e.w.add(b).id
        e.beam = b
        audioEvent('sfx', { id: 3, vol: 64 })
      }, (f) => {
        if (f === 9) (e.beam as KameBeam | null)?.fire()
        if (f >= 91) {
          e.setAnim('idle', true, true)
          e.beam?.stop()
          e.beam = null
          return true
        }
        return false
      }, () => {
        e.beam?.stop()
        e.beam = null
      })
    // ---- Dabura's stone spit (state 0x7C; lockstep f02_vs_hold_stone) ----------------------------------------------
    // AimAtPlayer = node 0x030030D4: enter 0x0804E684 SetAnimRaw(0x14) + controller 0x030030D0 (its tick 0x0802DF44
    // only picks the head pose obj+0x11 from the position deltas; the facing obj+0x10 is NOT changed), update
    // 0x0804FFAC returns 1 at once.
    // StoneSpit = node 0x030030CC: enter 0x0804E958 SetAnimRaw(0x14) + controller 0x030030C8, whose tick 0x0802DE7C
    // spits at tick 0 (the frame after the enter; enemy.ts spitCtl); update 0x080500A4: done once the
    // controller tick (+0x12) is > 8 (the 9th update). Neither calls SetAnim(0).
    case 'StoneSpit':
      return new Act(n, c, () => {
        e.setAnim(20, true, true, { rule: 'hold' })
        e.spitCtl = 0
      }, (f) => f >= 9)
    case 'AimAtPlayer':
      return new Act(n, c, () => e.setAnim(20, true, true, { rule: 'hold' }), () => true)

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
      // ---- vampire grab (node vtable 0x0804AB04) ----------------------------------------------------------------
      // enter 0x0803399E: escape count = 0; the player shows raw 10 frame 0, faces the vampire's facing ^ 1 and
      // enters the held state 0x03001B00; the VAMPIRE moves (the player stays): pos = player pos - table 0x08759334
      // [facing] ((0,16) (0,-16) (-16,0) (16,0) px for down / up / left / right); vampire SetAnimRaw(0x12) + its own
      // state (vtable 0x0804C3CC: frames 0..3, then 2 / 3 looping).
      // update 0x08033A2A: done when there is no player, the player's HP <= 0 or the player left 0x03001B00. Keys
      // [0x03003300]+4 & 0xF3 (A/B/d-pad PRESSED this frame: +4 is the press edge, +2 the held keys; the real game
      // keeps the grab through 200 frames of held A/B and lets go after nine presses) -> stamp, count + 1, count > 8 ->
      // player SetAnim(0), done; no press for
      // > 10 frames -> count 0. Frame counter 0x03000F94 & 15 == 0 -> player vfunc +0x5C (HP -= POW << 10, no formula)
      // and vampire vfunc +0x60 (heals POW << 10). Done when the player's HP <= 0.
      // exit 0x08033AF6: obj+0x294 (crit) = 0, obj+0x290 (hit dir) = facing ^ 1 (the KnockbackSlide that follows
      // slides the vampire backwards), SetAnim(9)
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.n = 0
        a.data.t = 0
        if (!p) return
        p.hold(e.w, e, 'vampire', 0)
        p.facing = (e.facing ^ 1) as Dir
        const [ox, oy] = VAMPIRE_GRAB_OFF[e.facing]
        e.unmarkSelf()
        e.x = p.x - (ox << 8)
        e.y = p.y - (oy << 8)
        e.markSelf()
        e.setAnim(18, true, true, { rule: 'loop', shift: 3 })
      }, (_f, a) => {
        const p = e.player
        if (!p || p.hp <= 0 || p.heldBy !== e) return true
        const now = e.ctx.clock(e.w)
        if (e.w.input.pressed & 0xf3) {
          a.data.t = now
          if (++a.data.n > 8) {
            p.release(e.w)
            return true
          }
        } else if (now - a.data.t > 10) a.data.n = 0
        if ((now & 15) === 0) {
          p.loseHp(e.w, e.stats.POW)
          e.heal(e.stats.POW * 1024)
        }
        return p.hp <= 0
      }, () => {
        const p = e.player
        if (p && p.heldBy === e) p.release(e.w)
        e.critHit = false
        e.lastHitDir = (e.facing ^ 1) as Dir
        e.setAnim(9, true, false)
      })
    case 'LatchDrain':
      // Enzyme latch (node singleton 0x03002FE8, vtable 0x0804AA94). Enter 0x080298D8: SetAnimRaw(1) + SetState(a new
      // controller, vtable 0x0804C314, shift 3: enter 0x08029860 keeps the offset enzyme - player, tick 0x0800DEB0
      // (loop), think 0x080298A2: enzyme position = player position + offset). The player is NOT held (it keeps its
      // own states and inputs). Update 0x08029902: no player / player +0x1CC == 0 / HP <= 0 -> finished; the frame
      // counter & 63 == 0 -> player vfunc +0x5C(0x400) (1 HP, no formula); exit: nothing (lockstep f02_vs_hold_latch)
      return new Act(n, c, () => e.latchEnter(), () => {
        const p = e.player
        if (!p || p.dead || p.hp <= 0) return true
        if ((e.ctx.clock(e.w) & 63) === 0) p.loseHp(e.w, 1)
        return false
      })
    case 'GrabWindup':
      // [H] enter 0x0802C465: face the player (0x0801599C), SetAnimRaw(0x12) (raw 19) under the play-once controller
      // 0x0300308C (frame t >> 3; its end 0x0804FC8D: SetAnimRaw(0x13) (raw 20) under 0x03001B30); update 0x0805004D:
      // finished once the enemy is in 0x03001B30
      return new Act(n, c, (a) => {
        e.facePlayer()
        e.setAnim(18, true, true, { rule: 'hold' })
        a.data.n = e.anim?.raw === 19 ? Math.max(1, ...e.anim.dirs.map((d) => d.length)) : 0
      }, (t, a) => {
        if (t >> 3 < a.data.n) {
          e.frameIdx = e.ea.pos.frame = t >> 3
          return false
        }
        e.setAnim(19, true, false, GENERIC_STATES['0x03001B30'])
        return true
      })
    case 'BearHug':
      // [H] Spopovich's bear hug (node vtable 0x0804B1E0). Enter 0x0802C4C9: keep the player's position, SetAnimRaw(0x14)
      // (raw 21) frame 0 under 0x03001B00 (frozen) at Spopovich + (8, -16) (globals 0x03003098 / 0x0300309C, set once);
      // partner = the first world object of class 0x130 (Yamu, 0x08011C70 message 5). Update 0x0802C645: no partner ->
      // finished (the player stays frozen until hit: game bug). Phase 0 (0x0802C565): the partner showing SetAnim(9)'s
      // animation (vfunc +0x34, Yamu's drain done) -> release; the partner showing anim id 0x11 (0x0800EEB0, Yamu
      // draining) -> the presses are ignored; else a d-pad press toggles the player's frame (obj+0x11 ^= 1) and each
      // 0x08040284(mask 0xF0, 20 presses, 90 frames) success counts; the 5th releases. Release: the player back at the
      // kept position, player SetAnim(0), Spopovich SetAnim(9), phase 1: 60 more ticks, then finished
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.phase = 0
        a.data.cnt = 0
        a.data.mash = 0
        const yamu = e.ctx.liveEnemies().find((o) => o.type.type === 0x93 || o.type.type === 0x94)
        a.data.partner = yamu ? yamu.id : -1
        if (!p || p.dead) return
        a.data.sx = p.x
        a.data.sy = p.y
        p.holdPose(e.w, 21, 0)
        p.x = e.x + (8 << 8)
        p.y = e.y - (16 << 8)
      }, (_f, a) => {
        const p = e.player
        const yamu = e.ctx.liveEnemies().find((o) => o.id === a.data.partner) as Enemy | undefined
        if (!yamu) return true
        if (a.data.phase === 1) return a.data.cnt-- === 0
        const release = () => {
          if (p) {
            p.x = a.data.sx
            p.y = a.data.sy
            p.toIdle(e.w)
          }
          e.setAnim(9, true, false)
          a.data.phase = 1
          a.data.cnt = 60
        }
        if (isAnimId(yamu, 9)) {
          release()
          return false
        }
        if (isAnimId(yamu, 0x11) || !p) return false
        const ctx = e.ctx
        if (ctx.pressed & DPAD_MASK) p.poseFrame ^= 1
        if (ctx.pressRun(DPAD_MASK, 20, 90, e.w) && ++a.data.mash === 5) release()
        return false
      })
    case 'EnergyDrain': {
      let mover: LineMover | null = null
      // [H] Yamu's drain (node vtable 0x0804A178, enter 0x0802FC53, update 0x0802FC5D). Phase 0 (0x0802FA70): until a
      // spot is found (0x0802F9A0: partner = the first object of class 0xFC (Spopovich), none -> finished; the spot
      // Spopovich + (40, 1) when Yamu is right of it, else / if blocked + (-40, 1)) a straight-line mover there
      // (0x0800A1D4, speed 0x100); every tick: Spopovich showing SetAnim(9)'s animation -> finished; the mover's step
      // (0x0800A01D); arrived -> face Spopovich's side, phase 1, SetAnimRaw(0x12) under 0x03003118, the target EP =
      // EP - maxEP / 4 (0 when EP <= maxEP / 4). Phase 1 (0x0802FB9C): EP == target -> Yamu SetAnim(9), phase 2 (60
      // ticks); else the byte counter +0x20 + 1: every 8th EP - 1, every 32nd sound 19. The counter is not initialised:
      // the node's fresh heap memory reads 0x6D (emulator), so the first EP loss is 3 ticks after the arrival
      return new Act(n, c, (a) => {
        a.data.phase = 0
        a.data.found = 0
        a.data.cnt = 0x6d
        a.data.partner = -1
        mover = null
      }, (_f, a) => {
        const p = e.player
        if (a.data.phase === 2) return (a.data.cnt = (a.data.cnt - 1) & 0xff) === 0xff
        if (a.data.phase === 1) {
          if (!p) return false
          if (p.ep === a.data.target) {
            e.setAnim(9, true, false)
            a.data.phase = 2
            a.data.cnt = 60
            return false
          }
          a.data.cnt = (a.data.cnt + 1) & 0xff
          if ((a.data.cnt & 7) === 0) p.ep -= 1
          if ((a.data.cnt & 31) === 0) audioEvent('sfx', { id: 19, vol: 64 })
          return false
        }
        if (!a.data.found) {
          const spo = e.ctx.liveEnemies().find((o) => [0x6d, 0x6e, 0x6f, 0x70].includes(o.type.type)) as Enemy | undefined
          a.data.partner = spo ? spo.id : -1
          if (!spo) return true
          e.unmarkSelf()
          let tx = 0
          let ty = spo.y + 0x100
          if (e.x > spo.x && e.fits(e.w, (spo.x + (40 << 8)) >> 8, ty >> 8)) tx = spo.x + (40 << 8)
          else if (e.fits(e.w, (spo.x - (40 << 8)) >> 8, ty >> 8)) tx = spo.x - (40 << 8)
          else ty = -1
          e.markSelf()
          if (ty >= 0) {
            a.data.found = 1
            mover = new LineMover(e, tx >> 8, ty >> 8, 0x100)
          }
        }
        const spo = e.ctx.liveEnemies().find((o) => o.id === a.data.partner) as Enemy | undefined
        if (spo && isAnimId(spo, 9)) return true
        if (!a.data.found || !mover) return false
        if (!mover.update()) return false
        e.facing = spo && e.x > spo.x ? 2 : 3
        a.data.phase = 1
        mover = null
        e.setAnim(18, true, true, { rule: 'hold' })
        a.data.target = p && p.ep > p.maxEp >> 2 ? p.ep - (p.maxEp >> 2) : 0
        return false
      })
    }
    case 'FreezePlayer':
      // ---- Buu fragment freeze (node vtable 0x0804AA3C) ---------------------------------------------------------
      // enter 0x080305AA: player frame 0, SetAnimRaw(0) (raw 1), SetState(0x03001B00) (held), message handler
      // 0x03003144 (hits do full damage, no hurt reaction); the fragment's freeze state (enemy.ts freezeEnter).
      // update 0x0804FF94: done once the fragment left 0x0300314C (the escape in its think SetAnim(0)s it).
      // exit 0x080305E2: player SetAnim(0) and handler 0x03001C3C - after an escape the player enters idle a second
      // time (two rand(255))
      return new Act(n, c, () => {
        const p = e.player
        if (p) p.hold(e.w, e, 'freeze', 0, 1)
        e.freezeEnter()
      }, () => e.stateHook?.name !== 'freeze', () => {
        const p = e.player
        if (!p || p.dead) return
        if (p.heldBy === e) p.release(e.w)
        else p.setAnimId(e.w, 0)
      })
    case 'EatCandy':
      // [H] enter 0x0802B19D: face the player, SetAnimRaw(0x14) (raw 21) under controller 0x0300303C (finished when
      // the enemy leaves it, update 0x0802B1EB). Controller tick 0x0802AF11 (t = obj+0x12 before the + 1):
      //   t 0: keep the player's position (0x03003044 / 48), the player's def 0x36 and a candy-eat state (0x0804C2CC)
      //     placing it at the enemy position + 0x08752DF8[dir]
      //   t 8: the player object leaves the world (0x08011944; its state's destructor clears the candy timer)
      //   t 24..111 every 8th: d = min(POW >> 2, HP); HP - d (0 -> 1 and the KO flag 0x03003038); the enemy heals d
      //     (vfunc +0x60); every 32nd sound 11
      //   t 120: a new player object (0x080109AA) at the kept position (whole pixels), facing down, the form's def,
      //     idle; the KO flag -> KO
      //   t 160: SetAnim(0)
      //   frames: t >> 3 below 24, 3 + ((t + 8) >> 3 & 1) below 112, 5 + ((t - 112) >> 3) below 128
      return new Act(n, c, () => {
        e.facePlayer()
        e.setAnim(20, true, true, { rule: 'hold' })
      }, (t, a) => {
        const p = e.player
        if (t === 1 && p) {
          a.data.sx = p.x
          a.data.sy = p.y
          a.data.ko = 0
          const O = [[-8, -20], [-8, -20], [-8, -20], [48, -20]][e.facing]
          p.candyEat(e.w, (e.px + O[0]) << 8, (e.py + O[1]) << 8)
        }
        if (t < 24) {
          if (t === 8 && p) p.candyGone(e.w)
          e.frameIdx = e.ea.pos.frame = t >> 3
        } else if (t < 112) {
          e.frameIdx = e.ea.pos.frame = 3 + (((t + 8) >> 3) & 1)
          if ((t & 7) === 0 && p) {
            let d = e.stats.POW >> 2
            if (p.hp <= d) d = p.hp
            p.hp -= d
            if (p.hp === 0) {
              p.hp = 1
              a.data.ko = 1
            }
            e.heal(d * 1024)
          }
          if ((t & 31) === 0) audioEvent('sfx', { id: 11, vol: 64 })
        } else if (t < 128) {
          e.frameIdx = e.ea.pos.frame = 5 + ((t - 112) >> 3)
          if (t === 120 && p) p.candyRespawn(e.w, (a.data.sx >> 8) << 8, (a.data.sy >> 8) << 8, !!a.data.ko)
        } else if (t >= 160) {
          e.setAnim('idle', true, true)
          return true
        }
        return false
      })

    // ---- Buu --------------------------------------------------------------------------------------------------------
    case 'BuuStretchPunch':
      // [H] enter 0x0802ACB3: the player HP > 0 and hittable -> face the player, SetAnimRaw(0x13) (raw 20) under
      // controller 0x03003004 (finished when the enemy leaves it). Controller tick 0x0802AB1D: t++; frame 0 below 8,
      // 1 + ((t + 8) >> 3 & 1) below 56, 3 + ((t - 56) >> 3) below 72; t 64 spawns the arm (0x08028C04) at the integer
      // position + 0x08752D6C[dir] for def 0x2B, else + 0x08752D8C (10 px higher sideways), kept in 0x03003008, and
      // plays sound 8; from t 72 SetAnim(0) once the arm is gone from the world
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.ok = p && p.hp > 0 && p.hittable ? 1 : 0
        a.data.arm = -1
        if (a.data.ok) {
          e.facePlayer()
          e.setAnim(19, true, true, { rule: 'hold' })
        }
      }, (t, a) => {
        if (!a.data.ok) return true
        if (t < 72) {
          e.frameIdx = e.ea.pos.frame = t < 8 ? 0 : t < 56 ? 1 + (((t + 8) >> 3) & 1) : 3 + ((t - 56) >> 3)
          if (t === 64) {
            const O = e.type.gfx === 0x2b ? [[9, 16], [15, -56], [-24, -16], [48, -16]] : [[9, 16], [15, -56], [-24, -26], [48, -26]]
            const [ox, oy] = O[e.facing]
            a.data.arm = e.w.add(new StretchArm(e.ctx, e, e.facing, e.px + ox, e.py + oy)).id
            audioEvent('sfx', { id: 8, vol: 64 })
          }
          return false
        }
        if (e.w.entities.some((x) => x.id === a.data.arm && !x.removed)) return false
        e.setAnim('idle', true, true)
        return true
      })
    case 'CandyBeam':
      // [H] enter 0x0802B333: the player HP > 0 and hittable -> face the player, SetAnimRaw(0x12) (raw 19) under
      // controller 0x0300304C; finished when the enemy is not in it (update 0x0802B3A3). Controller tick 0x0802B221:
      // t++; below 32 frame t >> 3, t 16 spawns the beam (0x080286D4) at the integer position + 0x08752E28[dir]
      // for def 0x2B, else + 0x08752E48 (y 10 px higher sideways), kept in 0x03003050; from t 32 SetAnim(0) once it is gone from the world
      return new Act(n, c, (a) => {
        const p = e.player
        a.data.ok = p && p.hp > 0 && p.hittable ? 1 : 0
        a.data.beam = -1
        if (a.data.ok) {
          e.facePlayer()
          e.setAnim(18, true, true, { rule: 'hold' })
        }
      }, (t, a) => {
        if (!a.data.ok) return true
        if (t < 32) {
          e.frameIdx = e.ea.pos.frame = t >> 3
          if (t === 16) {
            const [ox, oy] = (e.type.gfx === 0x2b ? CANDY_OFF_2B : CANDY_OFF)[e.facing]
            a.data.beam = e.w.add(new CandyBeam(e.ctx, e, e.facing, e.px + ox, e.py + oy)).id
          }
          return false
        }
        if (e.w.entities.some((x) => x.id === a.data.beam && !x.removed)) return false
        e.setAnim('idle', true, true)
        return true
      })
    case 'AbsorbStance':
      return new Act(n, c, () => e.setAnim(20, true, false), (f) => f >= 8)
    case 'AbsorbPose':
    case 'AbsorbPose2':
      return playAnim(n, c, 20, false, 8)
    case 'SuperBuuPowerUp':
      return new Act(n, c, () => {
        e.powered = true
        // anim 0x0C under the idle controller 0x03001B30 (once, 8 ticks a cel; segment sb 6-6 on 6-1: raw 13)
        e.setAnim(12, true, false, GENERIC_STATES['0x03001B30'])
        // update 0x0804FEC4: finished once the controller is no longer 0x03001B30 - also when another node replaced
        // it (real run 6-9 r052_006 f15,334: the old tree's KiBlast enter in the frame of the jump to 0x259)
      }, (f) => e.ea.r !== GENERIC_STATES['0x03001B30'] || (f >= 32 && e.animDone()))
    case 'SuperBuuAnim0C':
      // node runtime vtable 0x0804A100 (enter 0x0804E5B8): the power-up's animation without its flag
      return new Act(n, c, () => {
        e.setAnim(12, true, false, GENERIC_STATES['0x03001B30'])
      }, (f) => e.ea.r !== GENERIC_STATES['0x03001B30'] || (f >= 32 && e.animDone()))
    case 'ThrowBigBall':
      // the throw's controller ends 32 frames after the throw with SetAnim(0) (an idle draw); an exit while the ball
      // still charges removes it (real run 20-6 r062_001)
      return new Act(n, c, () => {
        e.setAnim(15, true, true)
        e.w.add(new SuperBall(e.ctx, e))
      }, (f) => {
        if (f >= 92) e.setAnim(0, true, true)
        return f >= 92
      }, () => {
        for (const x of e.w.entities) if (x instanceof SuperBall && x.owner === e && x.phase === 0 && !x.removed) x.removed = true
      })
    case 'GlideToWaypoint':
      return new Act(n, c, (a) => {
        // the waypoint table 0x0877B1BC (ROM)
        const WP = [[224, 104], [344, 128], [304, 256], [120, 264], [112, 144]]
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
      // SetAnimRaw(0xC) under the one-shot state 0x03001B30; the node finishes on the 56th update (lockstep
      // f02_vs_enemy_beam: entered at 172, the split state at 229)
      return new Act(n, c, () => e.setAnim(12, true, false, { rule: 'once_stop', shift: 3, end: 'none' }), (f) => f >= 56)
    case 'KidBuuSplit':
      return new Instant(n, c, () => e.kidBuuSplit())
    case 'KidBuuMerge':
      return new Instant(n, c, () => e.kidBuuMerge())
    case 'BabidiHover':
      // Babidi (emulator: real route 8-71, Vegeta vs Majin Buu). Enter 0x0802AD88: SetAnimRaw(0x13) under the
      // controller 0x03003014 (no idle draw), Buu is no longer pacified, not hittable (obj+0x1CC = 0, back to 1 when
      // the state changes). Controller tick 0x0802AD58: up 1 px a frame to 50; the tick runs 0..23, then 8..23 again,
      // cel = tick >> 3. The node never finishes
      return new Act(n, c, () => {
        e.ctx.buuPacified = 0
        e.setAnim(19, true, true, { rule: 'hold' })
        e.hittable = false
        e.hoverCtl = true
      }, () => false, () => (e.hittable = true))
    case 'BabidiGesture':
      // the landing. Enter 0x0802AE76: SetAnimRaw(0x13) under the controller 0x03003024; tick 0x0802AE3A: down 1 px
      // a frame; on the ground 8 ticks, then SetAnim(0) (idle rand(255)) and the node (0x0802AE98) is finished
      // (the enter acts only while the object already shows anim 0x13 - the descent's; else the node is done at once)
      return new Act(n, c, (a) => {
        a.data.on = e.ea.pos.raw === 20
        if (a.data.on) e.setAnim(19, true, true, { rule: 'hold' })
      }, (_f, a) => {
        if (!a.data.on) return true
        if (e.z < 0) {
          e.z += 1
          return false
        }
        a.data.t = (a.data.t ?? 0) + 1
        if (a.data.t >= 8) {
          e.setAnim(0, true, true)
          return true
        }
        e.ea.pos.frame = a.data.t >> 3
        return false
      })

    // ---- Janemba ------------------------------------------------------------------------------------------------------
    case 'JanembaSpawnParts':
      return new Instant(n, c, () => e.janembaParts())
    case 'JanembaHandStrikeA':
    case 'JanembaHandStrikeB':
      // the part's controller is set by the node's first update; the hand (an enemy, type 0x36) on its 16th tick
      // (lockstep f02_vs_enemy_hand: part controller at 5, hand at 21)
      return new Act(n, c, () => e.setAnim(17, true, false), (f) => {
        if (f === 17) e.handStrike(n.name === 'JanembaHandStrikeA')
        if (f >= 57) {
          // the part's controller ends (SetAnim(0) on the arm part: its idle enter draws rand(255)); the node is done
          e.w.rng.rand(255, 'janemba arm idle tick')
          return true
        }
        return false
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
    case 'MiniHopToward': {
      // node 0x03003230 (enter 0x0803467A: SetAnimRaw(0x12), hop controller 0x0300322C (tick +8 per frame, wraps at
      // 0x100), aim; update 0x080346FC: at tick 0 re-aim with chance(0x4000); tick > 0x80: try the whole step
      // 0x087593F8[dir] * 0x80 >> 8 (+-0.5 px per axis), no sliding). Aim: the diagonal toward the player from the
      // footprint centres (dx > 0: dy > 0 -> 0, dy < 0 -> 3; dx < 0: dy > 0 -> 2, dy < 0 -> 1; a zero keeps the facing)
      const aim = () => {
        const [dx, dy] = e.toPlayer()
        if (dx > 0 && dy > 0) e.facing = 0
        else if (dx > 0 && dy < 0) e.facing = 3
        else if (dx < 0 && dy > 0) e.facing = 2
        else if (dx < 0 && dy < 0) e.facing = 1
      }
      return new Act(n, c, (a) => {
        a.data.ph = 0
        // (the hop controller's own frames: the update below)
        e.setAnim(18, true, true, { rule: 'hold' })
        aim()
      }, (_f, a) => {
        // the hop controller's frame (tick 0x08034630, its counter before the +8): 0 below 0x80, 1 rising, 2 falling
        const c0 = a.data.ph
        e.ea.pos.frame = c0 < 0x80 ? 0 : c0 - 0x80 < 0x40 ? 1 : 2
        a.data.ph = (a.data.ph + 8) & 0xff
        const air = a.data.ph > 0x80
        e.z = a.data.ph >= 0x80 ? -Math.round(Math.sin(((a.data.ph - 0x80) / 128) * Math.PI) * 8) : 0
        if (a.data.ph === 0 && e.w.rng.chance(0x4000, 'mini re-aim')) aim()
        if (air) {
          const V = [[128, 128], [-128, -128], [-128, 128], [128, -128]]
          const [vx, vy] = V[e.facing]
          e.tryMove(vx, vy, 0, 0)
        }
        return false
      })
    }
    case 'MiniHopFlee':
      return new Act(n, c, (a) => (a.data.ph = 0), (_f, a) => {
        a.data.ph = (a.data.ph + 8) & 0xff
        const air = a.data.ph >= 0x80
        e.z = air ? -Math.round(Math.sin(((a.data.ph - 0x80) / 128) * Math.PI) * 8) : 0
        if (a.data.ph === 0) e.facePlayer()
        if (air) {
          const [dx, dy] = e.toPlayer()
          const m = Math.max(1, Math.hypot(dx, dy))
          e.moveBy(e.w, -Math.round((dx / m) * 128), -Math.round((dy / m) * 128))
        }
        return false
      })
    case 'MiniRollAttack':
      return new Act(n, c, (a) => {
        // anim 0x12 (raw 19) under the roll controller (its own frames: the update below)
        e.setAnim([18, 17], true, true, { rule: 'hold' })
        e.rollCtl = 0
        // controller 0x03003248 enter 0x08034CF8: the diagonal toward the player from the footprint centres (dx > 0:
        // dy > 0 -> 0 else 3; dx <= 0: dy > 0 -> 2 else 1), velocity 0x0875947C[dir] * 0x80 >> 8 = +-1.5 px per axis
        const [dx, dy] = e.toPlayer()
        e.facing = (dx > 0 ? (dy > 0 ? 0 : 3) : dy > 0 ? 2 : 1) as Dir
        const V = [[384, 384], [-384, -384], [-384, 384], [384, -384]]
        a.data.vx = V[e.facing][0]
        a.data.vy = V[e.facing][1]
      }, (f, a) => {
        // update 0x08034DE8: the controller's tick +8 per frame; 0x81..0xFF try-move, 0x100 one hit (body box, sub 0)
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
      // Babidi's state 0xCA: enter 0x0802ADF0 SetAnimRaw(0x15) under the controller 0x0300301C; tick 0x0802ADC8:
      // cel = (tick >> 3) & 3, at tick 0x60 SetAnim(0) (idle rand(255)) and the node (0x0802AE06) is finished
      if (e.type.type === 0x1d)
        return new Act(n, c, () => e.setAnim(21, true, true, { rule: 'hold' }), (f) => {
          if (f >= 0x60) {
            e.setAnim(0, true, true)
            return true
          }
          e.ea.pos.tick = f
          e.ea.pos.frame = (f >> 3) & 3
          return false
        })
      return playAnim(n, c, 20, true)
    case 'RecoverAnim':
      return playAnim(n, c, 19, false, 8)
    case 'PlayAnim0':
      // node vtable 0x0804A110: enter 0x0804E5E8 installs the fragment state 0x03003158 (enemy.ts fragEnter); update
      // 0x0804FF0C: done once the actor left that state; exit 0x0804F570 nothing
      return new Act(n, c, () => e.fragEnter(), () => e.stateHook?.name !== 'fragment')
    case 'PlayAnim12':
    case 'PlayAnim12b':
    case 'PlayAnim12Swarm':
    case 'PlayAnim12Yakon':
    case 'PlayAnim12Yakon2':
      return playAnim(n, c, 17, false, 16)
    case 'PlayAnim13':
    case 'PlayAnim13Yakon':
      return playAnim(n, c, 18, false, 16)
    case 'PlayAnimOnce': {
      // the Skeleton's state 0x213: the collapse (def 0x08759370, singleton 0x0300320C): enter 0x0804EAA0 = not
      // hittable (obj+0x1CC = 0) + SetAnimRaw(0x11) under the one-shot state 0x03001B38 ({vtable 0x0804E0AC, shift
      // 2}: 4 ticks a cel, the last one stays); the rise (def 0x0875938C): SetAnimRaw(0x12) under the same state, its
      // exit 0x0804F6CC = SetAnim(0) (the idle enter's rand(255)) + hittable again
      const collapse = parseInt(n.addr, 16) === 0x08759370
      return new Act(n, c, () => {
        if (collapse) e.hittable = false
        e.setAnim(collapse ? 17 : 18, true, false, { rule: 'once_stop', shift: 2, end: 'none' })
      }, (f) => {
        const done = f >= 16 && e.animDone()
        if (done && !collapse) {
          e.setAnim(0, true, true)
          e.hittable = true
        }
        return done
      })
    }
    case 'PlayAnimTimed':
      // Babidi's state 0xC9 (hit on the ground): enter 0x0802AEDA SetAnimRaw(0x14) under the controller 0x03003030
      // and Majin Buu is pacified (0x0300302C = 1); tick 0x0802AEC0: the tick wraps at 0x20, cel = tick >> 3; the
      // node never finishes
      if (e.type.type === 0x1d)
        return new Act(n, c, () => {
          // (the controller ticks with the actor: 4 cels of 8 ticks, looping - also under a knockback; Rust nodes.rs)
          e.setAnim(20, true, true, { rule: 'loop', shift: 3 })
          e.ctx.buuPacified = 1
        }, (f) => {
          e.ea.pos.tick = f & 0x1f
          e.ea.pos.frame = (f & 0x1f) >> 3
          return false
        })
      return wait(n, c, 32, 19)
    case 'DarkSphere':
      return new Act(n, c, () => {
        const [cx, cy] = e.center()
        e.w.add(new DarkSphere(e.ctx, e, (cx + 2) << 8, (cy - 17) << 8))
        e.setAnim(17, true, true)
      }, () => e.ctx.sphereState === 1 || e.ctx.sphereState === 3, () => (e.hittable = true))
    case 'TauntAnim':
      // the controller's enter 0x08033D84: SetAnimRaw(0x13) (Rust nodes.rs)
      return playAnim(n, c, 19, false, 12)
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
          const [x, y] = [e.px + off[0], e.py + off[1]]
          // 0x0802F458: the Enzyme's footprint (sprite def 0x57) must be free on the enemy grid, the Blister detached
          // (vfunc +0x48); else no spawn and a retry next frame (VERIFIED segment sb 21-7)
          const sp = e.ctx.data.sprites.index(0x57 - GFX_OFFSET)
          const c = e.ctx.col
          let free = true
          if (sp && c) {
            const sh = (sp.json as unknown as { header?: { shadow?: number[] } } | undefined)?.header?.shadow
            const [fw, fh] = [sh?.[0] || 16, sh?.[1] || 8]
            const m = e.marked
            const saved = m ? c.save('enemy', ...m) : null
            if (m) c.mark('enemy', ...m, 1)
            free = c.test('enemy', x, y, x + fw, y + fh)
            if (m && saved) c.load('enemy', ...m, saved)
          }
          if (free && e.ctx.spawnEnemy(e.w, 0x18, x, y, e)) {
            e.spawned++
            e.enzTimer = 120
          }
        }
        return false
      })
    case 'MechTransform':
      // create 0x080315A4 (singleton 0x03003170, vtable 0x0804CB90): enter SetAnimRaw(0x15) under the one-shot state
      // 0x03001B30 (once_stop, 8 ticks per frame); update 0x080314DC: running while the actor is in that state, then
      // swap the paired graphics set (Enemy.swapGfx), SetAnim(0) (idle, rand(255)) and finish (lockstep
      // f02_vs_enemy_t48)
      return new Act(n, c, () => e.setAnim(20, true, false), () => {
        if (!e.animDone()) return false
        e.swapGfx()
        e.setAnim(0, true, true)
        return true
      })
    case 'RaiseShield':
    case 'LowerShield':
    case 'ShieldBreak':
      return new Instant(n, c, () => {
        e.shield = n.name === 'RaiseShield' ? 1 : n.name === 'LowerShield' ? 0 : -1
        if (n.name === 'ShieldBreak') e.ctx.puffRng(e.w, ...e.center())
      })
    case 'GhostAnimA':
    case 'GhostAnimB':
      // enters 0x0804EBC8 / 0x0804EB38 = SetAnimRaw(0x12) under the one-shot state 0x03001B30; updates 0x0802DFD8 /
      // 0x0802E040: once that state is over, the paired graphics set through 0x0800EDF8 - A: the shown set -> the
      // unseen one (0x60 -> 0x5E, 0xDC -> 0xDB, 0xE5 -> 0xE4), B back (Kyodai Pyramid 14-17)
      return new Act(n, c, () => e.setAnim(18, true, false), (f) => {
        // (from the node's second update: the one-shot runs on under a held field - run 2 row 1,238,866, 18-10)
        if (!(f >= 1 && e.animDone())) return false
        e.ghostGfx(n.name === 'GhostAnimA')
        return true
      })
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
      // the segments are created by the node's enter 0x0802CBD4 (in the GotoState), the update just finishes
      // (lockstep f02_vs_enemy_t8e: the 5 constructors' rand(255) run inside the injected GotoState)
      return new Act(n, c, () => e.wormSpawnBody(), () => true)
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
        // explosion 0x0801952C (prio 0x37) at the box centre: rate 0x1000 + rand(0x1000)
        e.ctx.explosion(e.w, ...e.center(), 0x1000 + e.w.rng.rand(0x1000, 'volleyball explosion rate'))
      })
    case 'VolleyballRestore':
      return new Instant(n, c, () => {
        e.volleyball = false
        e.ctx.explosion(e.w, ...e.center(), 0x1000 + e.w.rng.rand(0x1000, 'volleyball explosion rate'))
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

