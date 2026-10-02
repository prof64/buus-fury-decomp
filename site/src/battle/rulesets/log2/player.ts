// LoG2 player: state machine (table 0x083B5278), movement, melee, the 14 special move types, forms/controllers.
// Sources: re/log2/notes.md Part B (+ disassembly of the idle/walk/dash/hurt handlers 0x08007F34, 0x08008030,
// 0x0800811E, 0x080082C8, 0x0800E222 and the energy-blast state 0x08014DF4 done for this simulator), and
// data/log2/{characters,moves}.json. Per frame the game runs: form-controller tick -> overlay -> state.update ->
// state.input (0x08008766 / 0x0800772A); we keep that order.

import { BTN, DPAD } from '../../core/input'
import { DIR_VEC, rectAt, type Dir } from '../../core/geom'
import type { World } from '../../core/world'
import type { Renderer } from '../../core/renderer'
import { Actor, broadcast, hitOverlaps, MSG_ATTACK, type HitMsg } from './actor'
import type { Log2Ctx, PlayerLike } from './context'
import { ATTACKS, computeDamage } from './damage'
import type { CharDef, ControllerDef, FormDef } from './data'
import { killExp, levelOf, levelUp, type Record8 } from './progression'
import { BigBangBall, Beam, BurningBolt, KiBlast, MasenkoBall, Pellet, SpiritBomb, SwordWave, type Projectile } from './projectiles'

export const ST = {
  IDLE: 0x00,
  WALK: 0x01,
  HERCULE: 0x05,
  DASH: 0x0d,
  RECOIL: 0x1f,
  HURT: 0x20,
  DYING: 0x21,
  BLAST: 0x22,
  TRANSFORM: 0x23,
  REVERT: 0x24,
  POWERUP: 0x25,
  SPMELEE: 0x27,
  PUNCH_A: 0x28,
  PUNCH_B: 0x29,
  PUNCH_C: 0x2a,
  EPUNCH: 0x2b,
  SCATTER: 0x2c,
  MASENKO: 0x2d,
  BIGBANG: 0x2e,
  BURNING: 0x2f,
  KAME: 0x30,
  SBC: 0x31,
  SWORD: 0x32,
  SPIRIT: 0x33,
  /** held by Cell's DrainGrab (anim object 0x0300227C) */
  GRABBED: 0x100,
} as const

const STATE_NAMES: Record<number, string> = Object.fromEntries(Object.entries(ST).map(([k, v]) => [v, k]))

// punch rects 0x083B5144 (= Energy Punch 0x086DB28C), special-melee rects 0x083B5208 (x0,y0,x1,y1 by facing)
const PUNCH_RECT = [
  [0, 0, 16, 16],
  [0, -24, 16, 0],
  [-8, -24, 0, 8],
  [16, -24, 24, 8],
]
const SPMELEE_RECT = [
  [0, 0, 16, 16],
  [0, -16, 16, 0],
  [-8, -24, 0, 8],
  [16, -24, 24, 8],
]
// hurt / recoil push 0x083B51C8: subtracted from the position each frame (0.75 px/f against the facing)
const KB = [
  [0, 192],
  [0, -192],
  [-192, 0],
  [192, 0],
]
// walk facing table 0x083B5184 [combo][old facing]; combo = h (0 none,1 L,2 R) + 3*v (0 none,1 U,2 D)
const FACE_TBL = [
  [0, 1, 2, 3],
  [2, 2, 2, 2],
  [3, 3, 3, 3],
  [1, 1, 1, 1],
  [1, 1, 2, 2],
  [1, 1, 3, 3],
  [0, 0, 0, 0],
  [0, 0, 2, 2],
  [0, 0, 3, 3],
]
// energy blast spawn offsets (moves.json spawn_pos_by_facing)
const BLAST_POS = [
  [8, 0],
  [8, -8],
  [-2, -8],
  [18, -8],
]
// Scatter Shot table 0x086E362C: per facing 3 x [dx, dy, vx, vy] (24.8 velocities)
const SCATTER = [
  [
    [6, 8, -384, 665],
    [10, 8, 384, 665],
    [8, 8, 0, 768],
  ],
  [
    [6, -16, -384, -665],
    [10, -16, 384, -665],
    [8, -16, 0, -768],
  ],
  [
    [-8, -6, -665, -384],
    [-8, -10, -665, 384],
    [-8, -8, -768, 0],
  ],
  [
    [24, -6, 665, -384],
    [24, -10, 665, 384],
    [24, -8, 768, 0],
  ],
]
// Masenko 0x086E374C / Big Bang 0x086E377C / Burning 0x086E37AC hand offsets (dx, -dy as measured)
const MASENKO_POS = [
  [12, -4],
  [12, -6],
  [10, -4],
  [6, -4],
]
const BALL_POS = [
  [4, -4],
  [12, -4],
  [10, -4],
  [6, -4],
]
// beam muzzles (measured right/down: Kame head (35,-14)-(51,2) at first hit, SBC rect from (26,-14); left/up mirrored)
const KAME_MUZZLE = [
  [8, 7],
  [8, -26],
  [-10, -6],
  [26, -6],
]
const SPIRIT_THROW = [
  [8, 4],
  [8, -16],
  [-2, -6],
  [18, -6],
]
const SBC_MUZZLE = [
  [8, 14],
  [8, -30],
  [-10, -8],
  [26, -8],
]
// Sword Blast: swing rect (measured right (16,-16,32,8); others mirrored), wave spawn (right (26,-4), down (8,12))
const SWORD_RECT = [
  [0, 8, 16, 24],
  [0, -32, 16, -16],
  [-16, -16, 0, 8],
  [16, -16, 32, 8],
]
const WAVE_POS = [
  [8, 12],
  [8, -28],
  [-10, -4],
  [26, -4],
]

export interface PlayerInit {
  char: CharDef
  form: number
  record: Record8
  level: number
  slot: number
}

export class Player extends Actor implements PlayerLike {
  kind = 'player'
  char: CharDef
  form!: FormDef
  ctl!: ControllerDef
  rec: Record8
  level: number
  slot: number
  state: number = ST.IDLE
  /** entity+0x12 counter of the current state */
  t = 0
  /** generic state scratch */
  s: { flag?: boolean; charge?: number; phase?: number; r?: number; cd?: number; acc?: number; frame?: number; release?: number; proj?: Projectile | null } = {}
  ctlAcc = 0
  /** double-tap memory 0x03001F40 (+0 last dir key, +0x20 frame) */
  lastDirKey = 0
  lastDirFrame = -1000
  dead = false
  grabbedBy: Actor | null = null
  grabLeft = 0
  /** recent state log for the debug panel */
  enteredAt = 0

  ctx: Log2Ctx
  constructor(ctx: Log2Ctx, init: PlayerInit) {
    super()
    this.ctx = ctx
    this.char = init.char
    this.rec = { ...init.record }
    this.level = init.level
    this.slot = init.slot
    this.applyForm(init.form)
    this.setAnim("idle", true, true)
  }

  // --- record accessors (party record is the source of truth for HP/EP) ---
  get hp() {
    return this.rec.hp
  }
  set hp(v: number) {
    this.rec.hp = v
  }
  get maxHp() {
    return this.rec.maxHp
  }
  get ep() {
    return this.rec.ep
  }
  set ep(v: number) {
    this.rec.ep = v
  }
  get maxEp() {
    return this.rec.maxEp
  }

  get stateName() {
    return STATE_NAMES[this.state] ?? this.state.toString(16)
  }

  /** form controller apply (vtable slot 0): stats + speeds; damage multipliers back to 0x80 */
  applyForm(id: number) {
    const f = this.ctx.data.forms[id]
    this.form = f
    this.ctl = this.ctx.data.controllers[f.controller]
    this.sprite = this.ctx.data.sprites.index(f.actorId)
    this.hurt = f.hurt
    this.refreshStats()
    this.ctlAcc = 0
    if (this.anim) this.setAnim(this.anim.name, false)
  }
  refreshStats() {
    const c = this.ctl
    const base = [this.rec.STR >> 8, this.rec.POW >> 8, this.rec.END >> 8]
    const v = c.override ? c.override : base.map((b, i) => b + (c.bonus ? c.bonus[i] : 0))
    this.stats = { STR: v[0] & 0xff, POW: v[1] & 0xff, END: v[2] & 0xff, meleeMult: 0x80, energyMult: 0x80 }
  }

  // ---------------------------------------------------------------------------------------------------------
  setState(w: World, id: number) {
    this.exitState(w)
    this.state = id
    this.t = 0
    this.s = {}
    this.enteredAt = w.frame
    this.enterState(w)
  }

  private exitState(_w: World) {
    switch (this.state) {
      case ST.HURT:
      case ST.REVERT:
        this.stats.meleeMult = this.stats.energyMult = 0x80
        break
      case ST.SPMELEE:
        this.hittable = true
        break
      case ST.KAME:
      case ST.SBC:
      case ST.MASENKO:
      case ST.BIGBANG:
      case ST.BURNING:
      case ST.SPIRIT: {
        const p = this.s.proj
        // a held (not yet released) ball / an active beam disappears when the state is interrupted
        if (p && !p.removed) {
          if (p instanceof Beam) p.removed = true
          else if (p instanceof MasenkoBall && !p.launched) p.removed = true
          else if (p instanceof BigBangBall && !p.launched) p.removed = true
          else if (p instanceof BurningBolt && !p.launched) p.removed = true
          else if (p instanceof SpiritBomb && !p.thrown) p.removed = true
        }
        break
      }
    }
  }

  private enterState(_w: World) {
    switch (this.state) {
      case ST.IDLE:
        this.setAnim('idle', false, true)
        break
      case ST.WALK:
        this.setAnim('walk', false, true)
        break
      case ST.DASH:
        this.setAnim(['dash', 'walk'], true, true)
        break
      case ST.RECOIL:
        this.setAnim('hurt')
        this.animIndex = 0
        break
      case ST.HURT:
        this.setAnim('hurt')
        this.animIndex = 0
        this.stats.meleeMult = this.stats.energyMult = 0
        break
      case ST.DYING:
        this.setAnim(['ko', 'knockdown'], true, false)
        this.hittable = false
        this.dead = true
        break
      case ST.PUNCH_A:
      case ST.PUNCH_B:
      case ST.PUNCH_C:
        this.setAnim(6 + (this.state - ST.PUNCH_A), true, false)
        break
      case ST.EPUNCH:
        this.setAnim([8, 'attack1'], true, false)
        break
      case ST.BLAST:
        this.setAnim('kiblast')
        this.animIndex = 0
        this.t = 8
        break
      case ST.SPMELEE:
        this.setAnim('charge_attack')
        this.animIndex = 0
        this.s.phase = 0
        this.s.cd = 16
        break
      case ST.TRANSFORM:
      case ST.POWERUP:
        this.setAnim(['transform', 'meditate', 'idle'], true, true)
        break
      case ST.REVERT:
        this.setAnim(['transform', 'idle'], true, true)
        this.stats.meleeMult = this.stats.energyMult = 0
        break
      case ST.KAME:
      case ST.SBC:
        this.setAnim(['special_beam', 'kiblast'], true, false)
        break
      case ST.SPIRIT:
        this.setAnim(['spirit', 'special_2', 'kiblast'], true, true)
        break
      case ST.MASENKO:
      case ST.BIGBANG:
      case ST.BURNING:
        this.setAnim(['special_2', 'kiblast'], true, false)
        break
      case ST.SCATTER:
        this.setAnim(['special_2', 'kiblast'], true, false)
        break
      case ST.SWORD:
        this.setAnim([8, 'attack1'], true, false)
        break
      case ST.HERCULE:
        this.setAnim(['victory', 'spirit', 'idle'], true, true)
        break
      case ST.GRABBED:
        this.setAnim(['knockback', 'hurt'], true, true)
        break
    }
  }

  // ---------------------------------------------------------------------------------------------------------
  update(w: World) {
    this.tickAnim()
    if (this.state === ST.DYING) return
    this.controllerTick(w)
    if (this.state === ST.DYING) return
    this.stateUpdate(w)
    this.stateInput(w)
    if (w.input.pressed & BTN.L && !this.dead) this.cycleSlot()
  }

  /** L: next learned slot (0x08008B88 -> 0x08004284). All slots are learned in the simulator. */
  cycleSlot() {
    this.slot = (this.slot + 1) & 3
  }

  /** form controller per-frame tick (vtable slot 1) */
  private controllerTick(w: World) {
    const add = (v: number, max: number) => Math.min(max, v)
    switch (this.ctl.key) {
      case 'normal':
        this.ctlAcc += 110
        if (this.ctlAcc >= 0x10000) {
          this.ctlAcc &= 0xffff
          this.ep = add(this.ep + ((this.maxEp * 0x7ae) >> 16), this.maxEp)
        }
        break
      case 'fullpower':
        this.ctlAcc += 0xccd
        if (this.ctlAcc >= 0x10000) {
          this.ctlAcc &= 0xffff
          this.hp = add(this.hp + ((this.maxHp * 0x290) >> 16), this.maxHp)
        }
        break
      case 'bulky':
      case 'ssj':
      case 'piccolo':
        this.ctlAcc += this.ctl.key === 'bulky' ? 0x667 : 0xccd
        if (this.ctlAcc >= 0x10000) {
          this.ctlAcc &= 0xffff
          if (this.ctl.key === 'piccolo') this.hp = add(this.hp + ((this.maxHp * 0x290) >> 16), this.maxHp)
          this.ep = Math.max(0, this.ep - 1)
          if (this.ep === 0 && this.state !== ST.REVERT && this.state !== ST.HURT) this.setState(w, ST.REVERT)
        }
        break
    }
  }

  private stateUpdate(w: World) {
    const held = w.input.held
    switch (this.state) {
      case ST.RECOIL:
      case ST.HURT:
        // 0x0800E222: counter+1, frame = counter>>3, idle at 32
        if (++this.t >= 32) this.setState(w, ST.IDLE)
        else this.animIndex = this.t >> 3
        break
      case ST.PUNCH_A:
      case ST.PUNCH_B:
      case ST.PUNCH_C:
      case ST.EPUNCH:
        // 0x0800E19C: counter+1, end at 0x10; punch end with A held -> special melee (0x080085AC)
        if (++this.t >= 16) {
          if (this.state !== ST.EPUNCH && held & BTN.A && this.form.smMask) this.setState(w, ST.SPMELEE)
          else this.setState(w, ST.IDLE)
        }
        break
      case ST.BLAST:
        this.updBlast(w)
        break
      case ST.SPMELEE:
        this.updSpMelee(w)
        break
      case ST.SCATTER:
        if (++this.t === 16) {
          for (const [dx, dy, vx, vy] of SCATTER[this.facing]) w.add(new Pellet(this.ctx, this, this.facing, (this.px + dx) << 8, (this.py + dy) << 8, vx, vy))
        }
        if (this.t >= 48) this.setState(w, ST.IDLE)
        break
      case ST.SWORD:
        if (++this.t === 8) {
          const [dx, dy] = WAVE_POS[this.facing]
          w.add(new SwordWave(this.ctx, this, this.facing, (this.px + dx) << 8, (this.py + dy) << 8))
        }
        if (this.t >= 33) this.setState(w, ST.IDLE)
        break
      case ST.HERCULE:
        if (++this.t >= 32) this.setState(w, ST.IDLE)
        break
      case ST.TRANSFORM:
      case ST.POWERUP: {
        const len = this.state === ST.POWERUP ? 64 : 165
        if (++this.t >= len) {
          this.applyForm(this.form.transformTo)
          this.setState(w, ST.IDLE)
        }
        break
      }
      case ST.REVERT:
        if (++this.t >= 60) {
          this.applyForm(this.form.revertTo)
          this.setState(w, ST.IDLE)
        }
        break
      case ST.MASENKO:
      case ST.BIGBANG:
      case ST.BURNING:
      case ST.SPIRIT:
        this.updHeldBall(w)
        break
      case ST.KAME:
      case ST.SBC:
        this.updBeam(w)
        break
      case ST.GRABBED:
        if (w.input.pressed & BTN.A) this.grabLeft -= 16
        if (--this.grabLeft <= 0) {
          this.grabbedBy = null
          this.setState(w, ST.IDLE)
        }
        break
    }
  }

  private stateInput(w: World) {
    switch (this.state) {
      case ST.IDLE:
        return this.inpIdle(w)
      case ST.WALK:
        return this.inpWalk(w)
      case ST.DASH:
        return this.inpDash(w)
      case ST.RECOIL:
      case ST.HURT: {
        const [kx, ky] = KB[this.facing]
        const nx = this.x - kx
        const ny = this.y - ky
        if (this.fits(w, nx >> 8, ny >> 8)) {
          this.x = nx
          this.y = ny
        }
        return
      }
      case ST.BLAST:
        if (w.input.pressed & BTN.B) this.s.flag = true // 0x08014E86
        return
      case ST.SPMELEE:
        if (!(w.input.held & BTN.A) && this.s.phase === 0) {
          if (this.s.cd! > 0) this.setState(w, ST.IDLE) // released too early: cancel
          else {
            // the release frame itself is attack-anim frame charge_anim_quads*4 (measured: Goku hits +11/+15,
            // Gohan +7/+15, Trunks (2 charge quads) +11/+15 after the release frame)
            this.s.phase = 1
            this.s.r = 0
            this.hittable = false // entity+0x168 = 0 during the release
            this.animIndex = 1
            this.updSpMelee(w)
          }
        }
        return
    }
  }

  private inpIdle(w: World) {
    const { held, pressed } = w.input
    if (held & DPAD) {
      if (pressed & BTN.LEFT) this.facing = 2
      if (pressed & BTN.RIGHT) this.facing = 3
      if (pressed & BTN.UP) this.facing = 1
      if (pressed & BTN.DOWN) this.facing = 0
      this.setState(w, ST.WALK)
    }
    const nd = pressed & DPAD
    if (nd) {
      // double tap of the same single direction within 20 frames -> dash (0x08007FC6)
      if (w.frame - this.lastDirFrame < 20 && (nd & -nd) === nd && this.lastDirKey & nd) this.setState(w, ST.DASH)
      this.lastDirKey = nd
      this.lastDirFrame = w.frame
    }
    if (pressed & BTN.A) this.melee(w)
    else if (pressed & BTN.B) this.special(w)
  }

  private inpWalk(w: World) {
    const { held, pressed } = w.input
    if (!(held & DPAD)) {
      this.setState(w, ST.IDLE)
    } else {
      const sp = this.ctl.walk
      let combo = 0
      if (held & BTN.LEFT) {
        this.moveTo(w, this.x - sp, this.y)
        combo = 1
      } else if (held & BTN.RIGHT) {
        this.moveTo(w, this.x + sp, this.y)
        combo = 2
      }
      if (held & BTN.UP) {
        this.moveTo(w, this.x, this.y - sp)
        combo += 3
      } else if (held & BTN.DOWN) {
        this.moveTo(w, this.x, this.y + sp)
        combo += 6
      }
      this.facing = FACE_TBL[combo][this.facing] as Dir
    }
    if (pressed & BTN.A) this.melee(w)
    else if (pressed & BTN.B) this.special(w)
  }

  private inpDash(w: World) {
    const { held, pressed } = w.input
    if (this.lastDirKey & ~held) {
      this.setState(w, ST.IDLE)
    } else {
      const [dx, dy] = DIR_VEC[this.facing]
      const nx = this.x + dx * this.ctl.run
      const ny = this.y + dy * this.ctl.run
      if (!this.fits(w, nx >> 8, ny >> 8)) {
        this.setState(w, ST.RECOIL) // bumped a wall (0x08008218)
      } else {
        this.x = nx
        this.y = ny
        // perpendicular steering at walk speed
        let sx = this.x
        let sy = this.y
        const ws = this.ctl.walk
        if (held & BTN.UP && this.facing !== 1) sy -= ws
        if (held & BTN.DOWN && this.facing !== 0) sy += ws
        if (held & BTN.LEFT && this.facing !== 2) sx -= ws
        if (held & BTN.RIGHT && this.facing !== 3) sx += ws
        if (this.fits(w, sx >> 8, sy >> 8)) {
          this.x = sx
          this.y = sy
        }
      }
    }
    if (pressed & BTN.A) this.melee(w)
    else if (pressed & BTN.B) this.special(w)
  }

  // ---------------------------------------------------------------------------------------------------------
  private msg(rectArr: number[], attackId: number, scale: number): HitMsg {
    return { team: 1, rect: rectAt(rectArr, this.px, this.py), facing: this.facing, scale, attack: ATTACKS[attackId], attacker: this, source: this }
  }

  /** A: 0x08007DE8 - hit on the press frame, then a random punch state 0x28 + rand(3) */
  melee(w: World) {
    const hit = broadcast(w, MSG_ATTACK, this.msg(PUNCH_RECT[this.facing], 2, 0x400))
    if (hit) {
      w.rng.rand(3, 'punch hit sfx')
      this.spark(w, PUNCH_RECT[this.facing])
    }
    this.setState(w, ST.PUNCH_A + w.rng.rand(3, 'punch anim'))
  }

  private spark(w: World, r: number[]) {
    this.ctx.spawnFx(w, 'fx_086e3b48', this.px + ((r[0] + r[2]) >> 1), this.py + ((r[1] + r[3]) >> 1), [0, 1], 3)
  }

  /** B: 0x08007EDA -> move type table 0x081D9900 canUse/execute */
  special(w: World) {
    const type = this.char.slots[this.slot]
    const mv = this.ctx.data.moves[type]
    if (type === 11 || type === 12) {
      if (this.form.transformLocked) return
      if (this.form.transformed) return this.setState(w, ST.REVERT)
      if (this.ep < 10) return
      this.spendEp(w, 10)
      return this.setState(w, type === 11 ? ST.POWERUP : ST.TRANSFORM)
    }
    if (!mv || this.ep < mv.minEp || type === 5) return
    this.spendEp(w, mv.cost)
    switch (type) {
      case 2:
        return this.setState(w, ST.BLAST)
      case 3:
        return this.setState(w, ST.KAME)
      case 9:
        return this.setState(w, ST.SBC)
      case 4: {
        this.setState(w, ST.MASENKO)
        this.s.charge = -150 // -180 at enter (0x08014F4A), +30 already on the press frame (measured hold30 -> C=720)
        return this.holdBall(w, new MasenkoBall(this.ctx, this, this.facing, ...this.at(MASENKO_POS)))
      }
      case 0:
        this.setState(w, ST.BIGBANG)
        return this.holdBall(w, new BigBangBall(this.ctx, this, this.facing, ...this.at(BALL_POS)))
      case 1:
        this.setState(w, ST.BURNING)
        return this.holdBall(w, new BurningBolt(this.ctx, this, this.facing, ...this.at(BALL_POS)))
      case 10:
        this.setState(w, ST.SPIRIT)
        return this.holdBall(w, new SpiritBomb(this.ctx, this, this.facing, (this.px + 8) << 8, (this.py - 24) << 8))
      case 8:
        return this.setState(w, ST.SCATTER)
      case 7: {
        // Energy Punch: immediate melee hit, attack 5 (0x600), 16-frame anim 0x2B
        const hit = broadcast(w, MSG_ATTACK, this.msg(PUNCH_RECT[this.facing], 5, 0x400))
        if (hit) this.spark(w, PUNCH_RECT[this.facing])
        return this.setState(w, ST.EPUNCH)
      }
      case 13: {
        broadcast(w, MSG_ATTACK, this.msg(SWORD_RECT[this.facing], 12, 0x400))
        return this.setState(w, ST.SWORD)
      }
      case 6: {
        // Hercule: attack 15 (power 0) to every entity, whole map rect
        broadcast(w, MSG_ATTACK, { team: 1, rect: { x0: 0, y0: 0, x1: 2048, y1: 2048 }, facing: this.facing, scale: 0, attack: ATTACKS[15], attacker: this })
        return this.setState(w, ST.HERCULE)
      }
    }
  }

  private at(tbl: number[][]): [number, number] {
    const [dx, dy] = tbl[this.facing]
    return [(this.px + dx) << 8, (this.py + dy) << 8]
  }
  private holdBall(w: World, p: Projectile) {
    this.s.proj = w.add(p)
  }
  spendEp(w: World, n: number) {
    const before = this.ep
    this.ep = Math.max(0, this.ep - n)
    w.stats.epSpent += before - this.ep
  }

  /** state 0x22 (0x08014DF4): shot at counter%16==0 (first at +8), refire if B was pressed and EP > 2 */
  private updBlast(w: World) {
    this.t++
    if (this.t >= 0x20) {
      if (this.s.flag && this.ep > 2) {
        this.s.flag = false
        this.spendEp(w, 2)
        this.animIndex = 3 - (this.animIndex ?? 0)
        this.t = 16
      } else {
        this.setState(w, ST.IDLE)
        return
      }
    } else if (this.t === 16) this.animIndex = 1
    if ((this.t & 15) === 0) {
      const [dx, dy] = BLAST_POS[this.facing]
      const gy = this.char.index === 0 && this.facing >= 2 ? 2 : 0
      w.add(new KiBlast(this.ctx, this, this.facing, 1, (this.px + dx) << 8, (this.py + dy + gy) << 8))
    }
  }

  /** state 0x27: charge (form.charge_anim_quads*4 frames + 16-frame countdown), release -> lunge + hits */
  private updSpMelee(w: World) {
    const f = this.form
    if (this.s.phase === 0) {
      this.t++
      const chargeLen = f.smChargeQuads * 4
      if (this.t > chargeLen && this.s.cd! > 0) {
        this.s.cd!--
        if (this.s.cd === 0) {
          this.flash = 12 // charge-complete effect
          this.ctx.spawnFx(w, 'fx_086e272c', this.px + 8, this.py - 12, [0, 1, 2, 3, 4, 5], 2)
        }
      }
      return
    }
    // release: a = attack-anim frame (starts at charge_anim_quads*4); hit on frame 4k-1 for each set bit k
    const r = this.s.r!
    const a = r + f.smChargeQuads * 4
    if (f.smLunge) {
      const [dx, dy] = DIR_VEC[this.facing]
      const nx = this.x + dx * f.smLunge
      const ny = this.y + dy * f.smLunge
      if (this.fits(w, nx >> 8, ny >> 8)) {
        this.x = nx
        this.y = ny
      }
    }
    const k = (a + 1) >> 2
    if ((a + 1) % 4 === 0 && f.smMask & (1 << k)) {
      if (broadcast(w, MSG_ATTACK, this.msg(SPMELEE_RECT[this.facing], 4, 0x400))) this.spark(w, SPMELEE_RECT[this.facing])
    }
    this.animIndex = Math.min(6, 1 + (r >> 2))
    this.s.r = r + 1
    if (a + 1 >= f.smLenQuads * 4) this.setState(w, ST.IDLE)
  }

  private updHeldBall(w: World) {
    const held = !!(w.input.held & BTN.B)
    const p = this.s.proj
    this.t++
    if (this.s.release == null) {
      if (held) {
        if (this.state === ST.MASENKO) this.s.charge! += 30
        else if (this.state === ST.BIGBANG || this.state === ST.SPIRIT) {
          // grow +1/frame while EP > 0; EP -1 every 4 frames (0x080150E0 / 0x080156C6)
          if (this.ep > 0) {
            if (p instanceof BigBangBall) p.scale++
            if ((this.t & 3) === 0) this.spendEp(w, 1)
          }
        }
        if (p instanceof SpiritBomb && this.ep > 0) p.size++
        // keep the held ball in the hands (and the Spirit Bomb above the head)
        if (p && !p.removed) {
          if (p instanceof SpiritBomb) {
            p.x = (this.px + 8) << 8
            p.y = (this.py - 24) << 8
          }
        }
        return
      }
      // B released
      this.s.release = this.t
      if (p instanceof SpiritBomb && this.ep > 0) p.size++
      if (p instanceof MasenkoBall) p.launch(this.s.charge!)
      else if (p instanceof BigBangBall) p.launch()
      else if (p instanceof BurningBolt) p.launch()
      return
    }
    const since = this.t - this.s.release
    if (this.state === ST.SPIRIT) {
      // size keeps growing until the throw, 8 frames after the release (0x080169B8): size = 5 + frames press->throw
      if (since <= 8 && p instanceof SpiritBomb && !p.thrown && this.ep > 0) p.size++
      if (since === 8 && p instanceof SpiritBomb) {
        // thrown from chest height in front of the player (measured: impact centre y -6, x 18 + 1 px/f)
        const [tx, ty] = SPIRIT_THROW[this.facing]
        p.x = (this.px + tx) << 8
        p.y = (this.py + ty) << 8
        p.thrown = true
      }
      if (since >= 24) this.setState(w, ST.IDLE)
    } else if (since >= 32) this.setState(w, ST.IDLE)
  }

  /** Kamehameha 0x30 / SBC 0x31: 24-frame charge, beam while B held, EP drain via a /256 accumulator */
  private updBeam(w: World) {
    const held = !!(w.input.held & BTN.B)
    const kame = this.state === ST.KAME
    this.t++
    if (!this.s.proj) {
      if (this.t >= 24) {
        if (!held) return this.setState(w, ST.IDLE)
        const m = (kame ? KAME_MUZZLE : SBC_MUZZLE)[this.facing]
        this.s.proj = w.add(new Beam(this.ctx, this, this.facing, 1, (this.px + m[0]) << 8, (this.py + m[1]) << 8, kame ? 'kame' : 'sbc', kame ? 10 : 11, 0x400))
        this.s.acc = 0
        this.animIndex = 2
      } else return
    } else if (!held) return this.setState(w, ST.IDLE)
    this.s.acc! += kame ? 0x56 : 0x22
    const drain = this.s.acc! >> 8
    this.s.acc! &= 0xff
    if (drain) this.spendEp(w, drain)
    if (this.ep <= 0) return this.setState(w, ST.IDLE)
  }

  // ---------------------------------------------------------------------------------------------------------
  /** onMessage (vt slot 9 0x08008945): attack query; team 0/2 hits the player */
  onMessage(w: World, type: number, msg: HitMsg): boolean {
    if (type !== MSG_ATTACK) return false
    if (msg.team === 1 || !this.hittable || this.dead) return false
    if (!hitOverlaps(msg, this)) return false
    const res = computeDamage(msg.attack, msg.attacker.stats, this.stats, msg.scale, w.rng)
    msg.crit = res.crit
    msg.raw = res.raw
    msg.dmg = res.dmg
    this.takeHit(w, res.dmg, res.crit, msg)
    return true
  }

  /** 0x08008806: damage != 0 -> apply (0x08008784) then hurt state 0x20 unless dead */
  takeHit(w: World, dmg: number, crit: boolean, msg?: HitMsg) {
    if (dmg === 0) return
    this.applyDamageDirect(w, dmg, crit, msg)
    if (!this.dead) this.setState(w, ST.HURT)
  }

  applyDamageDirect(w: World, dmg: number, crit: boolean, msg?: HitMsg) {
    if (dmg === 0 || this.dead) return
    this.flash = 12
    this.ctx.damagePopup(w, this, dmg, crit, true)
    w.stats.damageTaken += Math.min(dmg, this.hp)
    w.stats.hitsTaken++
    w.logEvent('damage', `${msg ? 'atk' + msg.attack.id : 'direct'} -> player: ${dmg}${crit ? ' CRIT' : ''} (hp ${Math.max(0, this.hp - dmg)})`)
    this.hp = Math.max(0, this.hp - dmg)
    if (this.hp === 0) {
      this.grabbedBy = null
      this.setState(w, ST.DYING)
    }
  }

  setGrabbed(by: Actor | null, frames: number) {
    this.grabbedBy = by
    this.grabLeft = frames
  }
  /** enter the held state (called by the DrainGrab node with the world) */
  grab(w: World, by: Actor, frames: number) {
    this.setGrabbed(by, frames)
    this.setState(w, ST.GRABBED)
  }

  /** kill exp + level-up (0x0800E4D2 -> 0x08003D58 -> 0x08003CA0) */
  gainExp(w: World, classExp: number) {
    const tbl = this.ctx.data.expTable
    const E = killExp(classExp, this.level, tbl)
    if (!E) return
    this.rec.exp += E
    const nl = levelOf(this.rec.exp, tbl)
    if (nl > this.level) {
      levelUp(this.rec, this.char.growth, nl - this.level, w.rng)
      this.level = nl
      this.refreshStats()
      w.popup('LEVEL ' + nl + '!', this.px + 8, this.py - 34, '#8cf', 90)
      w.logEvent('info', `level up -> ${nl}`)
    }
  }

  draw(r: Renderer, w: World) {
    super.draw(r, w)
  }

  debugLabel() {
    return `${this.stateName} t${this.t}\nSTR${this.stats.STR} POW${this.stats.POW} END${this.stats.END}`
  }
}
