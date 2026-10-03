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
import { GFX_OFFSET, type EnemyType } from './data'
import { MummyWrap, StraightShot, type GrowBeam, type KameBeam } from './projectiles'
import { audioEvent } from '../../audio/bus'
import { ENEMY_SETANIM, EnemyAnim, setAnimRule, type EAnimRule } from './eanim'

const GUN_MUZZLE = [[4, 4], [13, -24], [-8, -8], [24, -8]]
const GUN_BOX: [number, number, number, number][] = [[3, 10, 8, 16], [3, 5, 8, 16], [5, 4, 16, 8], [10, 4, 16, 8]]
const RIFLE_BOX: [number, number, number, number][] = [[8, 14, 16, 16], [8, 2, 16, 16], [2, 8, 16, 16], [14, 8, 16, 16]]
const SHELL_MUZZLE: [number, number][] = [[30, 60], [34, -12], [-16, 24], [60, 24]]
const SHELL_BOX: [number, number, number, number][] = [[8, 14, 16, 16], [8, 2, 16, 16], [2, 8, 16, 16], [14, 8, 16, 16]]
const WAVE_MUZZLE = { soldier: [[2, 8], [9, -28], [-16, -11], [24, -11]], turret: [[4, 4], [4, -25], [-13, -14], [22, -13]] }
/** 0x086FDA20: knockback sign per direction index (hit dir ^ 1) */
const KB_SIGN = [-8, 8, 8, -8]
/** the Buu fragment's state object 0x03003158 is one IWRAM singleton: its mode (+0xC) and its step (+4 / +8, 1/256
 *  px) are shared by every fragment, like in the game */
const FRAG = { mode: 0, vx: 0, vy: 0 }
export interface KnockState {
  vx: number
  vy: number
  n: number
  anim: unknown
}
/** MummyWrapBurst spawn offsets by facing: straight (x, y), diagonal (x, y) (0x08756AA4) */
const WRAP_BURST_OFF = [[0, 36, -12, 12], [0, -36, 12, -12], [-36, 0, -12, -12], [36, 0, 12, 12]]

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
  /** obj+0x210: frame of the last Block exit (IfTimeInState counts from it; never set = long ago) */
  stateStamp = -100000
  stunned = false
  lastHitDir: Dir = 0
  /** obj+0x294: the last accepted hit was critical (hit struct +0x10, written by ComputeDamage) - "launched" */
  critHit = false
  /** obj+0x298: the last accepted hit used attack sub id 3 (a wall-splat hit) */
  splatHit = false
  vx = 0
  vy = 0
  moving = false
  slot = 0
  children: Enemy[] = []
  parentE: Enemy | null = null
  defeated = false
  ko = 0
  powered = false
  beam: GrowBeam | KameBeam | null = null
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
  /** the game's animation timing (eanim.ts): obj+0x50 raw / +0x11 frame / +0x12 tick under the state rule */
  ea = new EnemyAnim({
    nframes: (raw) => this.sprite?.anim(raw)?.steps(this.facing).length ?? 0,
    rand: (n, label) => (this.w ? this.w.rng.rand(n, label) : 0),
    sfx: (id, vol) => audioEvent('sfx', { id, vol }),
  })
  /** the last one-shot ran out (its end callback fired) */
  animEnded = false
  /** the current animation runs under a node's own state object (a forced rule, e.g. the back-off's reversed walk
   *  0x03002B9C), not the SetAnim table's: vfunc +0x34 (0x0800FAC6, "is SetAnim(id) showing") is false */
  ownState = false
  /** a node's own state object (SetState) with per-frame code: the game runs the state's tick (vtable +4) and its
   *  think (+8) in the actor update before the AI tree; any SetAnim (a new SetState) replaces it */
  stateHook: { name: string; run: () => void } | null = null
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
  /** the player object ([0x03001B94]): still there after a KO (the AI's distance tests keep using its position) */
  get player(): PlayerLike | null {
    const p = this.ctx.player
    return p ?? null
  }

  start(w: World, state = this.type.ai) {
    this.w = w
    // obj+0x1E8 / +0x1EC are stamped with the frame counter at creation (Janemba waits 240 f from its spawn); obj+0x210
    // (IfTimeInState) only by the Block node's exit: 0 at creation (emulator: still 0 after a spawn and state changes)
    this.mark1 = this.mark2 = w.frame
    this.ai.push(state)
  }

  /** Dabura's spit controller 0x030030C8 (tick 0x0802DE7C): ticks since its SetState, -1 = not the current state.
   *  It belongs to the animation state, not to the AI node: it spits on its first tick even when the AI has
   *  changed state in between (lockstep f02_vs_hold_stone: StoneSpit entered by the old tree at 74, spit at 75) */
  spitCtl = -1
  update(w: World) {
    this.w = w
    // the player's death script (0x08799D0C) blocks every object update at KO + 199; from KO + 200 an enemy only
    // runs its state tick (caller 0x0800F5E0: no AI) (lockstep f02_vs_enemy_t0c frames 540+)
    const koAt = this.ctx.player?.koAt ?? -1
    if (koAt >= 0 && w.frame - koAt >= 199) {
      if (w.frame - koAt > 199) this.tickAnim()
      return
    }
    this.tickAnim()
    if (this.spitCtl >= 0 && this.spitCtl++ === 0) this.shot('spit')
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
    this.stateHook?.run()
    this.ai.tick()
    if (!this.anim || this.anim.name === 'idle' || this.anim.name === 'walk') {
      // a movement step SetAnim(1)s unless already walking (0x0803B9DA: vfunc +0x34 check); nothing switches back
      // to idle when the movement stops - the next node's own SetAnim does (lockstep f02_vs_enemy_melee)
      // (the back-off's reversed walk or a node's own state is another state object: the check fails and SetAnim(1)
      // re-enters the walk)
      if (this.moving && !this.walking()) this.setAnim('walk', true, true)
    }
  }

  // ---- animation (tools/battle/ENEMY_ANIM.md) ---------------------------------------------------------------------
  /** SetAnim / SetAnimRaw + SetState: the state rule for the raw comes from the type's recorded rules
   *  (buu_enemy_anim_sim.json), else the enemy SetAnim table (0x080726B8); `rule` forces one (e.g. the back-off's
   *  reversed walk). Re-entering restarts the state (idle draws a new start tick). */
  setAnim(ref: number | string | (number | string)[], restart = true, loop?: boolean, rule?: EAnimRule) {
    this.spitCtl = -1
    this.stateHook = null
    const prev = this.anim
    const refs = (Array.isArray(ref) ? ref : [ref]).map((r) => (typeof r === 'number' ? r + 1 : r))
    // the sprite lacks the requested animation: the game's state object still runs (no idle state, no rand(255));
    // the idle animation is only shown (lockstep f02_vs_enemy_gun: GunShot on a sprite without raw 18)
    const missing = !!this.sprite && !refs.includes('idle') && !refs.includes(1) && !this.sprite.pick(...refs)
    super.setAnim(ref, restart, loop)
    if (!this.anim || !this.ea) return
    if (this.anim === prev && !restart && !rule) return
    const raw = this.anim.raw
    // SetAnim(id) 0x0800ECF0: the state object comes from the enemy table 0x080726B8 by id; only the raw depends on
    // the sprite def. The recorded rules are keyed by raw, so a raw shown by two states keeps the first one recorded:
    // SetAnim(0) is always the idle state 0x03001B0C (enter = rand(255); Worm Body: SetAnim(1) shows raw 1 too,
    // lockstep f02_vs_enemy_t8e) and SetAnim(1) the walk loop 0x03001B58 (Gunman: raw 2 is also the back-off's
    // reversed walk 0x03002B9C; vfunc +0x34(1) tests the state, lockstep f02_vs_enemy_gun / t73)
    const plain = !rule && !missing
    const byId = plain && (ref === 'idle' || ref === 0) ? ENEMY_SETANIM[0].r : plain && (ref === 'walk' || ref === 1) ? (this.type.anim?.['0x0804A1D0@0x03001B58'] ?? ENEMY_SETANIM[1].r) : null
    const r = (missing ? ({ rule: 'hold' } as EAnimRule) : null) ?? rule ?? byId ?? this.type.anim?.[String(raw)] ?? setAnimRule(raw) ?? { rule: loop === false ? 'once_stop' : 'loop', shift: 3, end: 'none' }
    this.animEnded = false
    this.ownState = !!rule
    this.animSeq++
    this.ea.enter(raw, r)
    this.frameIdx = this.ea.pos.frame
  }
  /** +1 per state entered through setAnim (a node can tell its controller was replaced) */
  animSeq = 0
  /** vfunc +0x34 IsAnim(id) (0x0800FAC6): the state object (obj+0x14) is the SetAnim table's state for id
   *  (0x080726B8) AND the raw is id's (0x0800EEB0); a forced state (`ownState`, e.g. the back-off's reversed walk) is not */
  isAnim(id: number): boolean {
    return !this.ownState && !this.stateHook && this.ea.pos.raw === id + 1
  }
  /** vfunc +0x34(1): the walk loop state 0x03001B58 is running (not the back-off's reversed walk 0x03002B9C showing
   *  the same raw, nor a node's own state) */
  walking(): boolean {
    return this.isAnim(1)
  }
  tickAnim() {
    super.tickAnim()
    if (!this.anim) return
    // the state's tick (vtable +4) runs before the AI; a one-shot's end callback is SetAnim(0)
    if (this.ea.tick() === 'idle') {
      this.setAnim('idle', true, true)
      this.animEnded = true
    } else if (this.ea.pos.raw !== this.anim.raw) {
      const a = this.sprite?.anim(this.ea.pos.raw)
      if (a) this.anim = a
    }
    this.frameIdx = this.ea.pos.frame
  }
  animDone(): boolean {
    if (!this.anim) return true
    const k = this.ea.r.rule
    if (k === 'once' || k === 'once_stop' || k === 'seq') return this.ea.done || this.animEnded
    // looping states never end by themselves: keep the time-based answer the AI nodes expect
    return this.animEnded || super.animDone()
  }
  /** SetAnimRaw 0x0800ECDA: obj+0x50 only (the state object and its frame / tick stay) */
  setRaw(raw: number) {
    this.ea.pos.raw = raw
    const a = this.sprite?.anim(raw)
    if (a) this.anim = a
  }
  /** SetState(a node's own state object): no tick of its own here (the hook runs it); frame / tick kept */
  private useState(name: string, run: () => void) {
    this.ea.enter(this.ea.pos.raw, { rule: 'keep' })
    this.stateHook = { name, run }
  }

  /** LatchDrain's enter (0x080298D8): SetAnimRaw(1), SetState(controller 0x0804C314: loop, shift 3) whose think
   *  (0x080298A2) glues the enzyme to the player at the offset kept by its enter (0x08029860) */
  latchEnter() {
    const p = this.player
    this.setRaw(1)
    this.ea.enter(1, { rule: 'loop', shift: 3 })
    this.frameIdx = this.ea.pos.frame
    if (!p) return
    const ox = this.x - p.x
    const oy = this.y - p.y
    this.stateHook = {
      name: 'latch',
      run: () => {
        const q = this.player
        if (!q) return
        this.unmarkSelf()
        this.x = q.x + ox
        this.y = q.y + oy
        this.markSelf()
      },
    }
  }

  // ---- Buu fragment (type 0x0E): PlayAnim0 / FreezePlayer ------------------------------------------------------------
  /** PlayAnim0 enter 0x0804E5E8: frame = tick = 0, SetAnimRaw(0), SetState(0x03003158) (vtable 0x0804B788), whose
   *  enter 0x08030618 (tick = frame = 0) runs the think once */
  fragEnter() {
    this.ea.pos.frame = 0
    this.ea.pos.tick = 0
    this.setRaw(1)
    this.useState('fragment', () => {
      this.fragTick()
      this.fragThink()
    })
    this.fragThink()
  }
  /** state tick 0x080307FA: mode 0 frame 0; mode 1 frame 1 at tick 0x30, 2 at 0x50; tick = (tick + 8) & 0x7F (a
   *  16-frame cycle). (Not modelled: the height obj+0x176 = -sin(tick) >> 13 / >> 11 bob - cosmetic, and the game's
   *  body box 0x0800FE78 ignores it.) */
  private fragTick() {
    const pos = this.ea.pos
    const t = pos.tick
    if (FRAG.mode === 0) pos.frame = 0
    else if (FRAG.mode === 1) {
      if (t === 0x30) pos.frame = 1
      else if (t === 0x50) pos.frame = 2
    }
    pos.tick = (t + 8) & 0x7f
    this.frameIdx = pos.frame
  }
  /** state think 0x08030630: at tick 0 pick the step: d = player footprint centre - own centre, step = sign(d) px per
   *  axis (0 counts as negative); mode 1 (SetAnimRaw(0xE)) when the player is within 32 px on both axes in the step
   *  direction, else mode 0 (tick = 0, SetAnimRaw(0)); frame = 0. Every frame: pos + step (x2 in mode 1) tried
   *  through vfunc +0x54; body boxes (0x0800FE78) overlapping the player's -> state 0x03001BA4 (0x0800EE78: none,
   *  the frame is kept) - PlayAnim0 finishes */
  private fragThink() {
    const p = this.player
    if (!p) return
    const pos = this.ea.pos
    if (pos.tick === 0) {
      const [pcx, pcy] = Enemy.fc(p as unknown as Actor)
      const [ecx, ecy] = Enemy.fc(this)
      const dx = pcx - ecx
      const dy = pcy - ecy
      FRAG.vx = dx > 0 ? 256 : -256
      FRAG.vy = dy > 0 ? 256 : -256
      const rx = pcx - (ecx + (dx > 0 ? 32 : -32))
      const ry = pcy - (ecy + (dy > 0 ? 32 : -32))
      const sx = rx > 0 ? 256 : -256
      const sy = ry > 0 ? 256 : -256
      FRAG.mode = (FRAG.vx * sx) >> 8 < 0 && (FRAG.vy * sy) >> 8 < 0 ? 1 : 0
      pos.frame = 0
      if (FRAG.mode === 1) this.setRaw(15)
      else {
        pos.tick = 0
        this.setRaw(1)
      }
    }
    const k = FRAG.mode ? 2 : 1
    const nx = this.x + FRAG.vx * k
    const ny = this.y + FRAG.vy * k
    this.unmarkSelf()
    if (this.fits(this.w, nx >> 8, ny >> 8)) {
      this.x = nx
      this.y = ny
    }
    this.markSelf()
    const noZ = (a: Actor): Rect => {
      const b = a.bodyBox()
      return { x0: b.x0, y0: b.y0 - a.z, x1: b.x1, y1: b.y1 - a.z }
    }
    if (overlaps(noZ(this), noZ(p as unknown as Actor))) {
      this.stateHook = null
      this.ea.enter(this.ea.pos.raw, { rule: 'keep' })
    }
  }
  /** FreezePlayer enter 0x080305AA (node vtable 0x0804AA3C), fragment side: SetAnimRaw(0x12) + SetState(0x0300314C)
   *  (vtable 0x0804C284): enter 0x080304E4 tick = frame = 0, height 0, pos = the player's pos + (0, 1 px) */
  freezeEnter() {
    const p = this.player
    this.setRaw(19)
    this.ea.pos.frame = 0
    this.ea.pos.tick = 0
    if (p) {
      this.unmarkSelf()
      this.x = p.x
      this.y = p.y + 256
      this.markSelf()
    }
    this.useState('freeze', () => {
      // tick 0x08030584: tick + 1; past 7 -> tick 0, frame + 1, past 5 -> 4 (frames 4 / 5 loop)
      const pos = this.ea.pos
      if (++pos.tick > 7) {
        pos.tick = 0
        if (++pos.frame > 5) pos.frame = 4
      }
      this.frameIdx = pos.frame
      // think 0x08030538: a d-pad press this frame -> tick + 2; 0x08040284(input, 0xF3, 16, 90) (the last 16
      // A/B/d-pad presses all within 90 frames, one of them now) -> player SetAnim(0), frame = tick = 0, SetAnim(0)
      if (this.ctx.pressed & 0xf0) pos.tick += 2
      if (this.ctx.pressRun(0xf3, 16, 90, this.w)) {
        const pl = this.player
        if (pl) (pl as unknown as { release(w: World): void }).release(this.w)
        pos.frame = 0
        pos.tick = 0
        this.setAnim('idle', true, true)
      }
    })
  }
  /** MeleeAttack 0x0802A0C4: the number of punch animations (raw 5 / 6 / 7 present, create fn 0x0802A288) */
  punchAnims(): number {
    const s = this.sprite
    if (!s) return 0
    return s.anim(7) ? 3 : s.anim(6) ? 2 : s.anim(5) ? 1 : 0
  }
  /** the sound of an accepted melee hit on the player (0x0802A1D4): gfx 0x60 / 0xE5 / 0xDC -> 30, 0xEE / 0xF4 /
   *  0xED -> 62, else 48 + rand(2) */
  meleeHitSfx() {
    const g = this.type.gfx
    if (g === 0x60 || g === 0xe5 || g === 0xdc) audioEvent('sfx', { id: 30, vol: 64 })
    else if (g === 0xee || g === 0xf4 || g === 0xed) audioEvent('sfx', { id: 62, vol: 64 })
    else audioEvent('sfx', { id: 48 + this.w.rng.rand(2, 'enemy melee hit sfx'), vol: 64 })
  }

  // ---- geometry helpers -------------------------------------------------------------------------------------
  playerAlive() {
    return !!this.player
  }
  /** footprint centre to footprint centre (0x0800FDFA box: 24.8 position >> 8 + obj+0x19C / +0x19D; the distance
   *  conditions 0x08029C90 / 0x08029BE0 compare dx*dx + dy*dy <= r2) */
  toPlayer(): [number, number] {
    const p = this.player
    if (!p) return [9999, 9999]
    const [px, py] = p.footCenter()
    const [x, y] = this.footCenter()
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
  /** 0x0800FF44: the shot origin on the body box (0x0800FE78), centre c: down (cx, position y), up (cx, cy), left
   *  (x0, cy), right (x1, cy) */
  handPoint(): [number, number] {
    const b = this.bodyBox()
    const cx = (b.x0 + b.x1) >> 1
    const cy = (b.y0 + b.y1) >> 1
    return this.facing === 0 ? [cx, this.py] : this.facing === 1 ? [cx, cy] : this.facing === 2 ? [b.x0, cy] : [b.x1, cy]
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
    // 0x08029ED8: the body boxes (0x0800FE78) must overlap the enemy's box grown to x0 - 4 .. x1 + 3, y0 .. y1 - 6;
    // then the enemy turns toward the player exactly like MeleeAttack (strikeRect) and the result is whether the
    // player's body box overlaps that strike rect (the turn stays when it does not; lockstep f02_vs_enemy_t46)
    const b = this.bodyBox()
    if (!overlaps(p.bodyBox(), { x0: b.x0 - 4, y0: b.y0, x1: b.x1 + 3, y1: b.y1 - 6 })) return false
    const r = this.strikeRect(true)
    return !!r && overlaps(p.bodyBox(), r)
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
    return !this.part && !this.prev && !this.defeated && !this.detached && !this.removed && this.z === 0 && this.ctx.player?.heldBy !== this
  }
  /** obj+0x291 set by the Detach node (enemy_ai.json "SpawnPuff", enter 0x0804E53C -> 0x0800F924): off the enemy grid
   *  for good (the Ghost's first state) */
  detached = false
  /** vfunc +0x48(0) = 0x0800F740(enemy, 0): mark the footprint's tiles blocked */
  markSelf() {
    const c = this.ctx.col
    if (!c || this.marked || !this.attached()) return
    this.marked = [this.px, this.py, this.px + this.footW, this.py + this.footH]
    c.mark('enemy', ...this.marked, 0)
  }
  /** vfunc +0x48(1): set the marked tiles passable - also terrain tiles under a footprint that overlapped a wall: the
   *  game's hole quirk (emulator-verified: a fighter knocked out of a tree trunk slides through it) */
  unmarkSelf() {
    const c = this.ctx.col
    if (!c || !this.marked) return
    c.mark('enemy', ...this.marked, 1)
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
    if (!m) return c.test('enemy', x, y, x + this.footW, y + this.footH)
    // test with the own marks lifted (a test-only peek: the tiles get their previous values back)
    const saved = c.save('enemy', ...m)
    c.mark('enemy', ...m, 1)
    const ok = c.test('enemy', x, y, x + this.footW, y + this.footH)
    c.load('enemy', ...m, saved)
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
  // ---- knockback (KnockbackSlide 0x08018BCA; reaction states 2, 3, 4, 0xE, 0xF of the shared on-hit trees) ----------
  /** enter 0x08018A08: K = node +0x0C (x4 after a critical hit); d = hit dir ^ 1 (the enemy turns to face the
   *  attacker); s = (rand(64) + 96) * K >>> 7; velocity along the hit direction = table 0x086FDA20[d] (-8, 8, 8, -8) * s,
   *  across it (rand(0x8000) - 0x4000) * s >> 12; hurt anim 9 unless blocking (obj+0x1C8) */
  knockStart(K: number, kb: KnockState) {
    const w = this.w
    kb.n = 16
    if (this.critHit) K *= 4
    const d = (this.lastHitDir ^ 1) as Dir
    const s = ((w.rng.rand(64, 'knockback') + 96) * K) >>> 7
    const main = KB_SIGN[d] * s
    const perp = ((w.rng.rand(0x8000, 'knockback side') - 0x4000) * s) >> 12
    if (d >> 1) {
      kb.vx = main
      kb.vy = perp
    } else {
      kb.vx = perp
      kb.vy = main
    }
    this.facing = d
    kb.anim = null
    if (!this.blocking) {
      this.setAnim(9, true, false)
      kb.anim = this.anim ?? null
    }
  }
  /** update 0x08018A92: blocking -> guard anim 7, 16 frames; else until the hurt anim ends. Each frame free the
   *  footprint's tiles, move by the velocity, test the footprint rect on the enemy grid (vfunc +0x50); blocked ->
   *  move back, and after a critical (or sub-3) hit: wall splat - a hit (attack sub 3, x409/1024, attacker = this enemy,
   *  rect = the blocked footprint) to the other actors, this enemy unhittable meanwhile; the slide then ends.
   *  true = finished */
  knockStep(kb: KnockState): boolean {
    if (this.blocking) {
      if (!this.anim || this.anim.raw !== 8) this.setAnim(7, false, true)
      if (--kb.n === 0) return true
    } else if (!kb.anim || this.anim !== kb.anim || this.animDone()) return true
    const w = this.w
    this.unmarkSelf()
    this.x += kb.vx
    this.y += kb.vy
    if (this.fits(w, this.px, this.py)) {
      this.markSelf()
      return false
    }
    const rect: Rect = { x0: this.px, y0: this.py, x1: this.px + this.footW, y1: this.py + this.footH }
    this.x -= kb.vx
    this.y -= kb.vy
    this.markSelf()
    if (!this.critHit && !this.splatHit) return false
    const was = this.hittable
    this.hittable = false
    broadcast(w, { team: 1, rect, dir: this.lastHitDir, mult: 409, atk: ATK[3], attacker: this, source: this })
    this.hittable = was
    return true
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
    // the back-off runs under its own reversed-walk state (set by its enter): its step makes no SetAnim(1)
    if (!faceToward) this.moving = true
    this.tryMove(-Math.trunc((dx * speed) / m), -Math.trunc((dy * speed) / m), dx >= 0 ? -speed : speed, dy >= 0 ? -speed : speed)
    return !this.stepped
  }
  /** CirclePlayer update 0x0802F23A (Cyborgs; vtable 0x0804B1A0): d = player footprint centre - own centre, m =
   *  max(|dx|, |dy|); face the player on the major axis; the minor component of d is negated, then pos -= trunc(d' *
   *  speed / m) like stepFromPlayer: away from the player on the major axis, toward it on the minor one (not an
   *  orbit). Blocked -> one axis at full speed, then the other; nothing moved -> finished (1). m = 0 -> 0. Walk anim
   *  (SetAnimRaw 1 + state 0x03003100) unless already walking. Lockstep f02_vs_enemy_t11. */
  circlePlayer(speed: number): boolean {
    const p = this.player
    if (!p) return true
    const [px, py] = Enemy.fc(p)
    const [x, y] = Enemy.fc(this)
    let dx = px - x
    let dy = py - y
    const adx = Math.abs(dx)
    const ady = Math.abs(dy)
    let m: number
    if (adx > ady) {
      m = adx
      this.facing = dx < 0 ? 2 : 3
      dy = -dy
    } else {
      m = ady
      this.facing = dy < 0 ? 1 : 0
      dx = -dx
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
    return this.anim ? this.ea.pos.frame : 0
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
  /** FlyToPlayer update 0x08034ED8 (vampires): d = player footprint centre - own centre, m = max(|dx|, |dy|); faces
   *  the major axis (ties vertical) every update; m < 8 -> 0 (never finishes there); else SetAnim(1) unless walking,
   *  pos + trunc(d * speed / m) tried through vfunc +0x54: placed -> 0, blocked -> 1 (finished) */
  flyToPlayer(speed: number): boolean {
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
    if (!this.walking()) this.setAnim(1, true, true)
    const nx = this.x + Math.trunc((dx * speed) / m)
    const ny = this.y + Math.trunc((dy * speed) / m)
    this.unmarkSelf()
    const ok = this.fits(this.w, nx >> 8, ny >> 8)
    if (ok) {
      this.x = nx
      this.y = ny
    }
    this.markSelf()
    return !ok
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
  /** footstep sound 52 by sprite (def = gfx): gfx 0x4E / 0x11 / 0x4C on walk ticks 0 / 16 (the path follower 0x0803BB48
   *  after its step, AlignWithPlayer 0x0801CEEC before its move); gfx 0xA4 / 0x44 / 0xBF on ticks 8 / 24 (path
   *  follower only) */
  footstep(path: boolean) {
    const g = this.type.gfx
    // the game's step SetAnim(1)s (0x0803B9DA) before this test: a first walking step counts as walk tick 0 (the
    // simulator switches to the walk animation after the AI; lockstep f02_vs_enemy_t14)
    const t = this.moving && this.anim?.name !== 'walk' ? 0 : this.ea.pos.tick
    if ((g === 0x4e || g === 0x11 || g === 0x4c) && (t === 0 || t === 16)) audioEvent('sfx', { id: 52, vol: 64 })
    else if (path && (g === 0xa4 || g === 0x44 || g === 0xbf) && (t === 8 || t === 24)) audioEvent('sfx', { id: 52, vol: 64 })
  }
  /** the shown graphics set (gfx id; MechTransform swaps it) */
  gfxNow: number | null = null
  /** MechTransform 0x080314DC: swap to the paired graphics set via 0x0800EDF8 (the def at obj+0x58: 0xBB / 0xBC,
   *  0xE6 / 0xE7, 0x85 / 0x86). The ball sets have smaller body boxes (-5..20 x -18..7) - IfPlayerInMeleeRange and
   *  the hurt box follow (lockstep f02_vs_enemy_t48); same footprint (shadow 16 x 8). */
  swapGfx() {
    const g = this.gfxNow ?? this.type.gfx
    const PAIRS: Record<number, number> = { 0xbb: 0xbc, 0xbc: 0xbb, 0xe6: 0xe7, 0xe7: 0xe6, 0x85: 0x86, 0x86: 0x85 }
    const to = g != null ? PAIRS[g] : undefined
    if (to == null) return
    const s = this.ctx.data.sprites.index(to - GFX_OFFSET)
    if (!s) return
    this.gfxNow = to
    this.setSprite(s)
  }
  /** GhostAnimA / GhostAnimB (0x0802DFD8 / 0x0802E040): the Ghost / Phantom / Poltergeist graphics set <-> its unseen
   *  twin via 0x0800EDF8; the new set shows its first (idle) animation from the start: obj+0x34 = the def's table,
   *  +0x38 / +0x30 = 0, with no rand(255) draw (the one-shot state had already handed back to the normal one) */
  ghostGfx(vanish: boolean) {
    const g = this.gfxNow ?? this.type.gfx
    const PAIRS: [number, number][] = [[0x60, 0x5e], [0xdc, 0xdb], [0xe5, 0xe4]]
    const pr = PAIRS.find((q) => q[vanish ? 0 : 1] === g)
    if (!pr) return
    const to = pr[vanish ? 1 : 0]
    const s = this.ctx.data.sprites.index(to - GFX_OFFSET)
    if (!s) return
    this.gfxNow = to
    this.setSprite(s)
    const host = this.ea.host as { rand: (n: number, why?: string) => number }
    const rand = host.rand
    host.rand = () => 0
    try {
      this.setAnim(0, true, true)
    } finally {
      host.rand = rand
    }
  }
  /** message 0xF (0x0800F44A, the death script's freeze): the current animation state's vfunc +0x10 - the walk state
   *  (0x03003100) SetAnim(0)s (0x0800E050); other states: INFERRED nothing */
  stopMsg() {
    if (this.anim?.name === 'walk') this.setAnim(0, true, true)
  }
  /** a node that ends a movement: SetAnim(0) if the walk animation is showing (vfunc +0x34(1), 0x0801CE58) */
  stopWalk() {
    // vfunc +0x34(1) = 0x0800FAC6: the state object is SetAnim(1)'s (0x080726B8[1]) and the raw too (0x0800EEB0) -
    // the back-off's reversed walk (state 0x03002B9C, same raw) is not "walking": no SetAnim(0) (lockstep
    // f02_vs_enemy_t6c: AlignWithPlayer done at once after a back-off, no idle draw)
    if (this.walking()) this.setAnim('idle', true, true)
  }
  /** AlignWithPlayer 0x0801CF6E: a step that fits SetAnims(1) unless IsAnim(1) (e.g. after a back-off's reversed walk:
   *  lockstep f02_vs_enemy_t0a frame 43) */
  private alignWalk() {
    if (!this.isAnim(1)) this.setAnim('walk', true, true)
  }
  /** 0x0801CFA8: a blocked step ends the node with SetAnim(0) if walking (no walk is restarted after the AI); the facing
   *  stays the step's (no facing write on that path; VERIFIED Babidi's ship segments 8-31 / 8-42) */
  private alignBlocked() {
    this.stopWalk()
    this.moving = false
    return true
  }
  align(speed: number): boolean {
    if (!this.player) return false
    const [dx, dy] = this.toPlayer()
    if (Math.abs(dx) > Math.abs(dy) ? Math.abs(dy) >= 3 : Math.abs(dx) >= 3) this.footstep(false)
    if (Math.abs(dx) > Math.abs(dy)) {
      if (Math.abs(dy) < 3) {
        this.stopWalk()
        this.facing = dx > 0 ? 3 : 2
        return true
      }
      this.facing = dy > 0 ? 0 : 1
      if (!this.walkBy(0, Math.sign(dy) * speed)) {
        return this.alignBlocked()
      }
      this.alignWalk()
    } else {
      if (Math.abs(dx) < 3) {
        this.stopWalk()
        this.facing = dy > 0 ? 0 : 1
        return true
      }
      this.facing = dx > 0 ? 3 : 2
      if (!this.walkBy(Math.sign(dx) * speed, 0)) {
        return this.alignBlocked()
      }
      this.alignWalk()
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
      // 0x0802A0EE..0x0802A19A: the BODY box centres (0x0800FE78 of the player and of the enemy, the box of the
      // facing before the turn), not the footprints: |dx| > |dy| -> dx > 0 ? right : left, else dy > 0 ? down : up
      // (lockstep f02_vs_enemy_t46: Majin Vegeta 1.4 px right of the player on the same row turns down)
      const [px, py] = p.center()
      const dx = px - cx
      const dy = py - cy
      f = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 3 : 2) : dy > 0 ? 0 : 1
      this.facing = f
    }
    switch (f) {
      case 3:
        return { x0: cx, y0: cy, x1: B.x1 + 3, y1: B.y1 + 3 }
      case 2:
        return { x0: B.x0 - 4, y0: cy, x1: cx, y1: B.y1 + 3 }
      // vertical (0x0802A176): x from the footprint (0x0800FDFA), y from the body box: down = body y0 .. body y1 - 6,
      // up = body y0 .. footprint y0
      case 0:
        return { x0: F.x0, y0: B.y0, x1: F.x1, y1: B.y1 - 6 }
      default:
        return { x0: F.x0, y0: B.y0, x1: F.x1, y1: F.y0 }
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
  heavyStrike(): boolean {
    const p = this.player
    if (!p || !p.hittable) return false
    // sound 62 first (0x0802F7B2), whether the hit lands or not
    audioEvent('sfx', { id: 62, vol: 64 })
    this.facePlayer()
    const b = this.bodyBox()
    const [cx, cy] = this.center()
    // measured: hits at dx <= 60 px centre to centre, misses at 65 (Goku side-on body half-width 4)
    const R = 57
    const rect: Rect = this.facing === 3 ? { x0: cx, y0: b.y0, x1: cx + R, y1: b.y1 } : this.facing === 2 ? { x0: cx - R, y0: b.y0, x1: cx, y1: b.y1 } : this.facing === 0 ? { x0: b.x0, y0: cy, x1: b.x1, y1: cy + R } : { x0: b.x0, y0: cy - R, x1: b.x1, y1: cy }
    // accepted -> the hit sound 48 + rand(2) (0x0802F8AC)
    const ok = p.onMessage(this.w, { team: 0, rect, dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this })
    if (ok) audioEvent('sfx', { id: 48 + this.w.rng.rand(2, 'enemy melee hit sfx'), vol: 64 })
    return true
  }
  /** contact damage (worm, blood cell, mini-yakon): standard melee x1.0 with the body box */
  contactHit(foot = false): boolean {
    const p = this.player
    if (!p || !p.hittable || this.contactCd > 0) return false
    if (!overlaps(this.bodyBox(), foot ? p.footBox() : p.bodyBox())) return false
    const ok = p.onMessage(this.w, { team: 0, rect: this.bodyBox(), dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this, foot })
    // the worm's counter is decremented instead of the contact test (20 frames without a test, segment sb 21-14);
    // the update's own decrement runs first
    if (ok) this.contactCd = foot ? 21 : 20
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
    const mk = (dir: Dir, x: number, y: number, vx: number, vy: number, box: [number, number, number, number], extra: Partial<import('./projectiles').StraightOpts> = {}) => {
      const sh = w.add(new StraightShot(this.ctx, this, dir, 0, x, y, { vx, vy, box, atk: ATK[1], mult: 1024, color: ['#ffffff', '#ff9040'], radius: 3, name: kind, ...extra }))
      // the firing node's own sound (gun 0x080264EA / rifle 0x0802676A / tank 0x0801C900: sound 28) on the spawn frame
      sh.spawnRng(w)
      return sh
    }
    switch (kind) {
      case 'gun':
        // gun bullet 0x080274B4: record dir + 4f, f = 0,1,2,1 every 4 ticks
        mk(f, ...at, dx * 672, dy * 672, GUN_BOX[f], { sfxSpawn: 28, fragile: true, radius: 2, color: ['#fff8c0', '#ffa020'], fx: { id: 'fx_0874e968', frame: (d, t) => d + 4 * [0, 1, 2, 1][(t >> 2) & 3] } })
        break
      case 'rifle':
        // rifle bullet 0x080276A8: record dir, smoke trail records 4-6 (rate 0x2AAA) every 4 ticks; box = the
        // record's (0x0874E614 + 12 * dir: x - b0, y - b1, b2 x b3); an accepted hit plays 48 + rand(2), the destroy
        // 0x0802770C sound 46 (lockstep f02_vs_enemy_t2e)
        mk(f, ...at, dx * 448, dy * 448, RIFLE_BOX[f], { sfxSpawn: 28, rngHit: [2], sfxHit: 48, sfxEnd: 46, fragile: true, color: ['#e0ffe0', '#40c040'], fx: { id: 'fx_0874e614', frame: (d) => d }, trail: { id: 'fx_0874e614', rec: 4, n: 3, rate: 0x2aaa } })
        break
      case 'shell':
        // tank shell 0x0801C900: record dir, smoke trail records 4-6 (rate 0x2AAA) every 4 ticks; from the tank
        // controller 0x0802E278 at the position + 0x08757354 [facing], box = the record's (0x0871E630 + 12 * dir)
        // (Earth segment 3-14: the shell that hits Gohan)
        mk(f, (this.px + SHELL_MUZZLE[f][0]) << 8, (this.py + SHELL_MUZZLE[f][1]) << 8, dx * 448, dy * 448, SHELL_BOX[f], { sfxSpawn: 28, rngHit: [2], sfxHit: 48, fragile: true, radius: 4, color: ['#ffd0d0', '#d03020'], fx: { id: 'fx_0871e630', frame: (d) => d }, trail: { id: 'fx_0871e630', rec: 4, n: 3, rate: 0x2aaa } })
        break
      case 'star':
        // throw controller 0x0802C08A (tick 8 of 32): the star 0x0802C790 (vtable 0x0804BAA0) at the position (obj+0x1A0)
        // + the muzzle table 0x087548F0 (= GUN_MUZZLE), velocity 0x0875497C; then PlaySfx(61, vol 0x40) (0x0802C128)
        mk(f, ...at, dx * 448, dy * 448, [4, 4, 8, 8], { sfxSpawn: 61, fragile: true, explode: false, color: ['#ffffff', '#a0a0b0'], radius: 3, fx: { id: 'fx_08754a1c', frame: (_d, t) => (t >> 3) & 1 } })
        break
      case 'wrap':
      case 'burst': {
        // MummyWrapShot controller 0x030030A8 (tick 0x0802D0DC): one straight bandage at the position + 0x08756A74
        // [facing] (= GUN_MUZZLE; lockstep f02_vs_enemy_t59 / t4d / t81). MummyWrapBurst 0x030030B0 (0x0802D214): per
        // facing one straight and one diagonal at the position + 0x08756AA4 [facing] (INFERRED: same origin convention
        // as the single shot)
        if (kind === 'wrap') {
          w.add(new MummyWrap(this.ctx, this, f, ...(at as [number, number]), false))
          break
        }
        for (const d of [0, 1, 2, 3] as Dir[]) {
          const o = WRAP_BURST_OFF[d]
          w.add(new MummyWrap(this.ctx, this, d, (this.px + o[0]) << 8, (this.py + o[1]) << 8, false))
          w.add(new MummyWrap(this.ctx, this, d, (this.px + o[2]) << 8, (this.py + o[3]) << 8, true))
        }
        break
      }
      case 'spit': {
        // Dabura's spit: controller tick 0x0802DE7C -> ctor 0x0802E444 (vtable 0x0804AC5C). Origin = the enemy position
        // + 0x087572C8[pose] (every pose: (0, -32)); target = the player's footprint centre (0x0800FDFA); velocity
        // d * 512 / max(|dx|, |dy|) (pixel deltas, truncating division 0x08046E58); box = record 0x087579B8+ (8, 8, 16,
        // 16); dir = the enemy's facing. Update 0x0802E568: move, removed off the area, else a message (ATK[1] x1.0,
        // team 0); accepted -> the player's stone state (player.ts petrify) and the spit is removed (no explosion)
        const p = this.player
        if (!p) return
        const ox = this.px
        const oy = this.py - 32
        const [tx, ty] = p.footCenter()
        const ddx = tx - ox
        const ddy = ty - oy
        const m = Math.max(Math.abs(ddx), Math.abs(ddy)) || 1
        // one of 5 aim sectors (2 records each, toggling every 4 ticks) from the aim vector
        const sector = 2 * ddx < -5 * ddy ? 0 : 12 * ddx < -5 * ddy ? 2 : 2 * ddx > 5 * ddy ? 8 : 12 * ddx > 5 * ddy ? 6 : 4
        mk(f, ox << 8, oy << 8, Math.trunc((ddx * 512) / m), Math.trunc((ddy * 512) / m), [8, 8, 16, 16], {
          color: ['#e0e0e0', '#707070'],
          explode: false,
          fx: { id: 'fx_087579b8', frame: (_d, t) => sector + ((t >> 2) & 1) },
          onHitPlayer: (ww) => this.player?.petrify?.(ww),
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
      this.ctx.puffRng(w, ...this.center())
      this.x += dx << 8
      this.y += dy << 8
      this.ctx.puffRng(w, ...this.center())
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
      // 0x08032BB8: d is in 1/256 px (the 24.8 position +- d; lockstep f02_vs_enemy_beam: 137.13 -> 137.22)
      // 0x08009904: both footprints on walkable map tiles (the enemy marks are not tested)
      const col = this.ctx.col
      const ok = (x: number, y: number) => !col || col.test('walk', x, y, x + this.footW, y + this.footH)
      if (!ok((this.x + dx) >> 8, (this.y + dy) >> 8) || !ok((this.x - dx) >> 8, (this.y - dy) >> 8)) continue
      // the puff 0x086FD9D0, rate 0x1000 + rand(0x1000), at the sprite box centre
      this.ctx.puffRng(w, ...this.center())
      const x0 = this.x
      const y0 = this.y
      this.unmarkSelf()
      this.x = x0 + dx
      this.y = y0 + dy
      this.markSelf()
      this.hp1024 = (this.hp1024 + 1024) >> 1
      const copy = this.ctx.spawnEnemy(w, 0x3d, (x0 - dx) >> 8, (y0 - dy) >> 8, null, 0x12a) as Enemy | null
      if (copy) {
        copy.unmarkSelf()
        copy.x = x0 - dx
        copy.y = y0 - dy
        copy.markSelf()
      }
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
    // an enemy object: its constructor's SetAnim(0) draws the idle start tick
    this.w.rng.rand(255, 'hand idle tick')
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
      // the 12-slot ring (0x0802D650) replays the slot it overwrites when a command arrives: a body moves only on a
      // received command (VERIFIED segment sb 21-9: each segment starts one frame after the one before it)
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
      // 0x0802D546: the extra quarter turn when rand(2) == 0 (bne skips the eor)
      if (w.rng.rand(2, 'worm turn2') === 0) d ^= 1
      this.wormDir = d as Dir
    }
    for (let k = 0; k < 4; k++) {
      const [dx, dy] = DIR_VEC[this.wormDir]
      if (this.fits(w, this.px + dx, this.py + dy)) {
        this.x += dx << 8
        this.y += dy << 8
        break
      }
      this.wormDir = ((this.wormDir + 1) & 3) as Dir
    }
    this.facing = this.wormDir
    this.next?.cmds.push(this.wormDir)
    this.contactHit(true)
  }
  /** the head and the segments stay in the idle state while they move (no SetAnim(1): VERIFIED segment sb 21-11) */
  wormBodyStep() {
    // the last segment (no next, 0x0802D674): sprite def 0x12A (Worm Tail) set again every frame (0x0800EDF8: the
    // animation restarts) + SetAnim(1)
    if (!this.next || this.next.removed) {
      const s = this.ctx.data.sprites.index(0x12a - GFX_OFFSET)
      if (s) {
        this.gfxNow = 0x12a
        this.setSprite(s)
        this.setAnim(1, true, true)
      }
    }
    if (this.cmds.length > 12) {
      const d = this.cmds.shift()! as Dir
      const [dx, dy] = DIR_VEC[d]
      this.x += dx << 8
      this.y += dy << 8
      this.facing = d
      this.next?.cmds.push(d)
    }
    this.contactHit(true)
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
    // 0x0800FC62: obj+0x294 = the crit flag, obj+0x298 = (attack sub == 3), then the on-hit tree
    this.critHit = !!res.crit
    this.splatHit = msg.atk.sub === 3
    this.ai.onHit(msg)
    return true
  }
  /** ApplyDamage -> vfunc +0x58 (0x0800FA2A): HP -= the computed damage; true when this hit killed the enemy */
  takeHit(msg: HitMsg): boolean {
    const w = this.w
    // the ApplyDamage node (0x080187A4) stores the hit's direction (obj+0x290) before the call: an on-hit tree
    // without it keeps the old one (emulator: real route 8-71, Babidi's hit state)
    this.lastHitDir = msg.dir
    // 0x0800FA2A: only HP == 0 counts as already dead; a negative HP (see heal) still takes the hit and dies
    if (this.hp1024 === 0) return true
    const dmg = msg.dmg ?? 0
    const before = Math.max(0, this.hp)
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
  /** vfunc +0x60 (0x0800FA80): HP += ceil(n / 1024) whole HP, capped at max HP. No death check: a negative amount
   *  (HealFraction's 32-bit wrap) can leave HP <= 0 with the enemy still alive until its next ApplyDamage. */
  heal(n1024: number) {
    const add = ((n1024 + 0x3ff) >> 10) << 10
    if (add === 0) return
    this.hp1024 = Math.min(this.maxHp1024, this.hp1024 + add)
    if (add > 0) this.w.popup('+' + add / 1024, this.center()[0], this.bodyBox().y0 - 4, '#7f7', 30)
    else this.ctx.damagePopup(this.w, this, -add / 1024, false, false)
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
    // 0x08018960: explosion 0x0801952C at the body-box centre, rate 0x1000 + rand(0x1000)
    if (explode) this.ctx.explosion(this.w, ...this.center(), 0x1000 + this.w.rng.rand(0x1000, 'death explosion rate'))
    this.removed = true
  }

  draw(r: Renderer, w: World) {
    // ground shadows: only the sprite header's shadow OBJ (Actor.draw); the game draws none for the others
    if (this.volleyball) {
      // 0x0803C830: the struck enemy shows sprite def 281, raw animation 1, frame 0, facing right
      const orb = this.ctx.data.sprites.index(281)
      const save = [this.sprite, this.anim, this.frameIdx, this.facing] as const
      this.sprite = orb
      this.anim = orb?.anim(1)
      this.frameIdx = 0
      this.facing = 3
      super.draw(r, w)
      ;[this.sprite, this.anim, this.frameIdx, this.facing] = save
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
    // the rising controller (0x0804C338) ends on tick 40 with SetAnim(0) (rand(255)); hits from tick 16
    if (this.t === 40) w.rng.rand(255, 'hand idle tick')
    if (!this.hit && p && !p.dead && this.t >= 16 && this.t < 40) {
      if (p.onMessage(w, { team: 0, rect: this.bodyBox(), dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this })) {
        this.hit = true
        // 0x0803A0F2: the hit sound 48 + rand(2)
        audioEvent('sfx', { id: 48 + w.rng.rand(2, 'hand hit sfx'), vol: 64 })
      }
    }
    this.alpha = this.t < 12 ? this.t / 12 : this.t > 50 ? Math.max(0, (57 - this.t) / 7) : 1
    if (this.t >= 57) this.removed = true
  }
  onMessage(): boolean {
    return false
  }
}
