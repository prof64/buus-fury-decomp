// Buu's Fury player: movement (walk 5*SPD+102, dash +320 on a double tap within 20 f), melee (one instant hit on
// the A press, rects 0x083C344C, 16-frame recovery), block (R: 1/16 from the front), energy shield (R+B), the 23
// skills with their EP rules (cost table 0x0871DA98 scaled by record+0x56/16, SS gauge while transformed), forms
// (SS / SS3 gauges, Saiyaman), regen (timer +110/frame -> 3% EP), 24-frame hurt state with zero resists, crits,
// equipment hooks (EP/HP per melee hit, HP regen, Crisis Ring) and the hold states enemies put the player in.
// Sources: re/buu/notes.md §3-§6, data/buu/{characters,moves,attacks_detail,formulas,items}.json.
// Skill state machines are not decoded instruction by instruction: their timings are fitted to the measured runs
// in moves.json / attacks_detail.json (first-hit frames, hit counts, EP spent) and marked INFERRED where guessed.

import { BTN, DPAD } from '../../core/input'
import { DIR_VEC, rectAt, type Dir, type Rect } from '../../core/geom'
import type { World } from '../../core/world'
import type { Renderer } from '../../core/renderer'
import { drawText } from '../../core/pixelfont'
import { Actor, broadcast, hitsBody, type HitMsg } from './actor'
import type { BuuCtx, HoldKind, PlayerLike } from './context'
import { ATK, computeDamage, HpAcc } from './damage'
import type { BuuForm, EquipDef } from './data'
import { recalcStats, levelOf, levelUp, xpGain, type BuuRecord, type EffStats } from './progression'
import { audioEvent } from '../../audio/bus'
import { AreaHit, BigBangBall, FullBeam, Ghost, GrowBeam, MasenkoCursor, StraightShot, kiBlast, VEL, type Shot } from './projectiles'

/** melee rects 0x083C344C (x0,y0,x1,y1 relative to the position; same for every form) */
export const PUNCH_RECT = [
  [0, 0, 16, 16],
  [0, -24, 16, 0],
  [-8, -24, 0, 8],
  [16, -24, 24, 8],
]
// walk facing table (combo = h (0 none, 1 L, 2 R) + 3*v (0 none, 1 U, 2 D)) - same convention as LoG2
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
/** beam drain table 0x0871E148 in 1/256 EP per frame (0.418 / 1.5 / 5 EP) */
const BEAM_DRAIN = [107, 384, 1280]

export type PState = 'idle' | 'walk' | 'dash' | 'punch' | 'block' | 'shield' | 'hurt' | 'bump' | 'dying' | 'skill' | 'transform' | 'held'

const SKILL_MULT: Record<number, number[]> = {
  1: [1640], // Banshee
  3: [164], // Big Bang Kamehameha
  4: [410, 820, 1649], // Burning Attack
  5: [328], // Dragon Fist
  6: [41, 164, 328], // Electric Kamehameha
  8: [226, 226, 226], // Energy Punch
  9: [10240], // Energy Sword
  10: [41, 164, 328], // Final Flash
  11: [41, 164, 328], // Flame Kamehameha
  15: [41, 164, 328], // Kamehameha
  16: [41, 164, 328], // Kamekameha
  17: [3687, 11060, 16589], // Masenko
}
const SKILL_SUB: Record<number, number> = { 1: 14, 3: 17, 4: 10, 5: 18, 6: 6, 7: 4, 8: 9, 9: 15, 10: 11, 11: 12, 15: 5, 16: 8, 17: 7, 23: 16 }
const BEAM_COLOR: Record<number, [string, string]> = {
  1: ['#fff0ff', '#e040e0'], 3: ['#e8f4ff', '#3a7cff'], 6: ['#ffffff', '#60e8ff'], 11: ['#fff8c0', '#ff6020'], 15: ['#e8ffff', '#30b8ff'], 16: ['#e8ffff', '#30b8ff'], 10: ['#fffbd0', '#f0d020'],
}

export interface PlayerInit {
  name: string
  form: number
  rec: BuuRecord
  equip: (EquipDef | undefined)[]
  skills: number[]
  skillLevel: number
  slot: number
  holyWater: boolean
  /** player character index for SS / form rules (fusion = component chars) */
  formsAllowed: number[]
}

export class Player extends Actor implements PlayerLike {
  kind = 'player'
  ctx: BuuCtx
  name: string
  form!: BuuForm
  rec: BuuRecord
  equip: (EquipDef | undefined)[]
  eff!: EffStats
  skills: number[]
  skillLevel: number
  slot: number
  state: PState = 'idle'
  t = 0
  /** skill scratch */
  sk: { id: number; t: number; held: boolean; release: number; proj?: Shot | null; acc: number; refire?: boolean; ghosts?: Ghost[]; target?: number; charged?: boolean; charge?: number; hits?: number } | null = null
  hpAcc = new HpAcc()
  epAcc = 0
  regenTimer = 0
  hpRegenAcc = 0
  dead = false
  heldBy: Actor | null = null
  holdKind: HoldKind | null = null
  holdLeft = 0
  holdCount = 0
  holdLast = 0
  blast: StraightShot | null = null
  wraps = 0
  wrapPresses: number[] = []
  /** SS gauge (rec+0x1E) and SS3 gauge (+0x20) in 1/65536 EP */
  ssGauge = 0
  ssAcc = 0
  ss3Gauge = 0
  /** rec+4 SS cooldown */
  ssCooldown = 0
  holyLeft = 0
  lastDirKey = 0
  lastDirFrame = -1000
  walk = 0
  dashSpd = 0
  transformTo = -1
  formsAllowed: number[]
  punchAnim = 0
  constructor(ctx: BuuCtx, init: PlayerInit) {
    super()
    this.ctx = ctx
    this.name = init.name
    this.rec = { ...init.rec }
    this.equip = init.equip
    this.skills = init.skills
    this.skillLevel = init.skillLevel
    this.slot = Math.min(init.slot, Math.max(0, init.skills.length - 1))
    this.formsAllowed = init.formsAllowed
    this.holyLeft = init.holyWater ? 1800 : 0
    this.applyForm(init.form)
    this.setAnim('idle', true, true)
  }

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
  get skill(): number {
    return this.skills[this.slot] ?? 7
  }
  get inSS() {
    return this.form.ss || this.form.ss3
  }

  // ---- stats ---------------------------------------------------------------------------------------------
  applyForm(id: number) {
    const f = this.ctx.data.forms[id]
    this.form = f
    // gfx id = sprite index + 9 (the halo forms 2/4/6 use defs 108/110/112: Goku's frames + the halo table)
    let spr = this.ctx.data.sprites.index(f.gfx - 9)
    for (let k = 1; !spr && k <= 3; k++) spr = this.ctx.data.sprites.index(f.gfx - 9 - k)
    this.setSprite(spr)
    this.refreshStats()
    if (this.anim) this.setAnim(this.anim.name, false)
  }
  /** RecalcStats 0x08013C66 + apply to the player object 0x0800E074 + form bonuses */
  refreshStats() {
    this.eff = recalcStats(this.rec, this.equip, this.holyLeft > 0)
    const b = this.form.bonus
    const e = this.eff
    const hurt = this.state === 'hurt'
    this.stats = {
      STR: e.STR + b.STR,
      POW: e.POW + b.POW,
      END: e.END + b.END,
      END2: e.END2 + b.END2,
      resM: hurt ? 0 : 128,
      resE: hurt ? 0 : 128,
      critBonus: e.critPct * 656,
      blockFactor: e.blockFactor,
    }
    const w = 5 * e.SPD + 102
    if (b.walk < 0 && this.form.id === 24) {
      // Trunks form 24: walk and dash speed / 4 (0x0800E550)
      this.walk = w >> 2
      this.dashSpd = (w + 320) >> 2
    } else {
      this.walk = w + b.walk
      this.dashSpd = w + 320 + b.dash
    }
  }
  walkSpeed() {
    return Math.max(0, this.walk - 48 * this.wraps)
  }
  dashSpeed() {
    return Math.max(0, this.dashSpd - 48 * this.wraps)
  }

  // ---- EP / gauge ------------------------------------------------------------------------------------------
  scaled(cost: number) {
    return (cost * this.eff.epCostFactor) >> 4
  }
  /** 0x080134B8: EP (or the SS / SS3 gauge) >= cost * factor/16 */
  canAfford(cost: number) {
    const c = this.scaled(cost)
    if (this.form.ss3) return this.ss3Gauge >> 16 >= c
    if (this.form.ss) return this.ssGauge >= c
    return this.ep >= c
  }
  /** 0x08013504 */
  spend(w: World, cost: number, scaledAlready = false) {
    const c = scaledAlready ? cost : this.scaled(cost)
    if (c <= 0) return
    if (this.form.ss3) this.ss3Gauge = Math.max(0, this.ss3Gauge - c * 65536)
    else if (this.form.ss) this.ssGauge = Math.max(0, this.ssGauge - c)
    else {
      const before = this.ep
      this.ep = Math.max(0, this.ep - c)
      w.stats.epSpent += before - this.ep
    }
  }
  /** fractional drain (1/256 EP per frame, scaled by the cost factor) */
  drain(w: World, per256: number) {
    this.epAcc += (per256 * this.eff.epCostFactor) >> 4
    const n = this.epAcc >> 8
    this.epAcc &= 0xff
    if (n) this.spend(w, n, true)
  }
  energyLeft() {
    if (this.form.ss3) return this.ss3Gauge >> 16
    if (this.form.ss) return this.ssGauge
    return this.ep
  }

  // ---- PlayerLike ------------------------------------------------------------------------------------------
  attacking() {
    return this.state === 'punch' || this.state === 'skill' || this.state === 'dash'
  }
  charging() {
    return this.state === 'skill'
  }
  isStone() {
    return this.holdKind === 'stone'
  }
  isCandy() {
    return this.ctx.candy > 0
  }
  swinging() {
    return this.state === 'punch'
  }
  hold(w: World, by: import('./actor').Actor, kind: HoldKind, frames: number) {
    if (this.dead) return
    this.endSkill(w)
    this.heldBy = by
    this.holdKind = kind
    this.holdLeft = frames
    this.holdCount = 0
    this.holdLast = w.frame
    this.state = 'held'
    this.t = 0
    this.blocking = false
    if (kind === 'stone') this.setAnim('hurt', true, false)
    else if (kind === 'eat') this.alpha = 0
    else this.setAnim(['hurt', 'knockback'], true, true)
  }
  release(w: World) {
    if (this.state !== 'held') return
    this.heldBy = null
    this.holdKind = null
    this.alpha = 1
    this.setState(w, 'idle')
  }
  loseHp(w: World, n: number, floor = 0): number {
    if (this.dead || n <= 0) return 0
    const d = Math.min(n, Math.max(0, this.hp - floor))
    this.hp -= d
    w.stats.damageTaken += d
    this.ctx.damagePopup(w, this, d, false, true)
    this.afterHpChange(w)
    return d
  }
  loseEp(n: number) {
    this.ep = Math.max(0, this.ep - n)
  }
  gainXp(w: World, exp: number) {
    if (!this.ctx.opts.xp || !exp) return
    const g = xpGain(exp, this.eff.xpMult)
    this.rec.xp += g
    const nl = levelOf(this.rec.xp, this.ctx.data.xpTable)
    if (nl > this.rec.level) {
      const old = this.rec.level
      const growth = this.ctx.data.chars[this.form.char]?.growth
      if (growth) levelUp(this.rec, growth, old, nl, w.rng)
      this.rec.level = nl
      this.refreshStats()
      w.popup('LEVEL UP!', this.px + 8, this.py - 34, '#8cf', 90)
      audioEvent('level_up')
      w.logEvent('info', `level ${old} -> ${nl} (+${(nl - old) * 3} stat points)`)
    }
  }

  // ---- state machine ---------------------------------------------------------------------------------------
  setState(w: World, s: PState) {
    const was = this.state
    if (was === 'skill' && s !== 'skill') this.endSkill(w)
    this.state = s
    this.t = 0
    this.shakeX = this.shakeY = 0
    this.blocking = s === 'block' || s === 'shield'
    switch (s) {
      case 'idle':
        this.setAnim('idle', was !== 'idle', true)
        break
      case 'walk':
        this.setAnim('walk', was !== 'walk', true)
        break
      case 'dash':
        this.setAnim(['dash', 'walk'], true, true)
        break
      case 'block':
      case 'shield':
        this.setAnim('guard', false, true)
        break
      case 'hurt':
      case 'bump':
        this.setAnim(['hurt', 'knockback'], true, false)
        break
      case 'dying':
        this.setAnim(['knockdown', 'ko', 'hurt'], true, false)
        this.dead = true
        this.hittable = false
        break
    }
    if (was === 'hurt' || s === 'hurt') this.refreshStats()
  }

  update(w: World) {
    this.tickAnim()
    if (this.dead) return
    this.t++
    this.passive(w)
    if (this.dead) return
    const inp = w.input
    // candy / stone: no actions (INFERRED for candy: the controller 0x0804BE60 is not decoded)
    switch (this.state) {
      case 'idle':
      case 'walk':
        if (this.ctx.candy > 0) return this.moveInput(w, false)
        if (inp.held & BTN.R) return this.setState(w, inp.held & BTN.B && this.energyLeft() > 0 ? 'shield' : 'block')
        if (inp.pressed & BTN.A) return this.melee(w)
        if (inp.pressed & BTN.B) return this.startSkill(w)
        if (inp.pressed & BTN.L) this.cycleSkill()
        this.moveInput(w, true)
        return
      case 'dash':
        return this.updDash(w)
      case 'punch':
        if (this.t >= 16) this.setState(w, 'idle')
        return
      case 'block':
        if (!(inp.held & BTN.R)) return this.setState(w, 'idle')
        if (inp.held & BTN.B && this.energyLeft() > 0) this.setState(w, 'shield')
        return
      case 'shield': {
        if (!(inp.held & BTN.R)) return this.setState(w, 'idle')
        if (!(inp.held & BTN.B)) return this.setState(w, 'block')
        // 0x08012BC6: acc += rec+0x57 * 0x3277 >> 4; spend acc >> 16 (~0.197 EP/frame)
        this.epAcc += (this.eff.epBlockFactor * 0x3277) >> 4
        const n = this.epAcc >> 16
        this.epAcc &= 0xffff
        if (n) this.spend(w, n, true)
        if (this.energyLeft() <= 0) this.setState(w, 'block')
        return
      }
      case 'hurt': {
        // 24-frame hurt state, both resists 0, ~17 px knockback
        const [dx, dy] = DIR_VEC[this.hurtDir]
        this.moveBy(w, dx * 182, dy * 182)
        if (this.t > 24) this.setState(w, 'idle')
        return
      }
      case 'bump': {
        // dashed into a wall (emulator trace work/compare/emu_bump.py): hurt animation, pushed back 192/256 px per
        // frame for 23 frames, no damage; the body shakes for the first 10 frames; then back to walk / idle
        if (this.t >= 24) {
          this.setState(w, 'idle')
          return this.moveInput(w, true)
        }
        const [dx, dy] = DIR_VEC[this.facing]
        this.moveBy(w, -dx * 192, -dy * 192)
        if (this.t < 10) {
          // INFERRED pattern (the game jitters the sprite by up to 4 px; the generator is not decoded): cosmetic only,
          // does not draw from the game RNG
          const h = Math.imul(w.frame + 1, 0x9e3779b1) >>> 0
          this.shakeX = (h % 9) - 4
          this.shakeY = ((h >>> 8) % 9) - 4
        } else this.shakeX = this.shakeY = 0
        return
      }
      case 'skill':
        return this.updSkill(w)
      case 'transform':
        return this.updTransform(w)
      case 'held':
        return this.updHeld(w)
    }
  }
  hurtDir: Dir = 0

  /** the player tests the map's walk grid (map object +0xA8) with its footprint (obj+0x19C size) */
  passable(_w: World, x: number, y: number): boolean {
    const c = this.ctx.col
    return !c || c.test('walk', x, y, x + this.footW, y + this.footH)
  }

  /** walking try-move 0x08012960(player, new 24.8 pos, flags): take the new position if the footprint fits; else
   *  slide around a corner: flags & 1 (vertical step) snaps x to the nearest multiple of 8 when x is within 2 px
   *  of it, flags & 2 (horizontal step) does the same for y. Both coordinates come from the first test that fits. */
  walkTry(w: World, vx: number, vy: number, flags: 1 | 2) {
    const nx24 = this.x + vx
    const ny24 = this.y + vy
    let nx = nx24 >> 8
    const ny = ny24 >> 8
    if (this.fits(w, nx, ny)) {
      this.x = nx24
      this.y = ny24
      return
    }
    let x24 = nx24
    if (flags & 1 && ((nx & 7) < 2 || (nx & 7) > 5)) {
      const sx = (nx + 4) & ~7
      if (this.fits(w, sx, ny)) {
        this.x = x24 = sx << 8
        this.y = ny24
        nx = sx
      }
    }
    if (flags & 2 && ((ny & 7) < 2 || (ny & 7) > 5)) {
      const sy = (ny + 4) & ~7
      if (this.fits(w, nx, sy)) {
        this.x = x24
        this.y = sy << 8
      }
    }
  }

  fits(w: World, x: number, y: number): boolean {
    if (!super.fits(w, x, y)) return false
    for (const e of w.entities) {
      if (e.kind === 'rock' && !e.removed) {
        const r = (e as unknown as { rect(): Rect }).rect()
        if (x < r.x1 && r.x0 < x + this.footW && y < r.y1 && r.y0 < y + this.footH) return false
      }
    }
    return true
  }

  cycleSkill() {
    if (this.skills.length) this.slot = (this.slot + 1) % this.skills.length
  }

  private passive(w: World) {
    // EP regen 0x0800E1D0: timer += 110, tick on overflow -> EP += maxEP*0x7AE>>16
    this.regenTimer += 110
    if (this.regenTimer >= 0x10000) {
      this.regenTimer -= 0x10000
      this.ep = Math.min(this.maxEp, this.ep + ((this.maxEp * 0x7ae) >> 16))
    }
    if (this.eff.hpRegen256) {
      this.hpRegenAcc += this.eff.hpRegen256
      if (this.hpRegenAcc >= 256) {
        const n = this.hpRegenAcc >> 8
        this.hpRegenAcc &= 0xff
        this.hp = Math.min(this.maxHp, this.hp + n)
      }
    }
    if (this.holyLeft > 0 && --this.holyLeft === 0) this.refreshStats()
    if (this.ctx.candy > 0) this.ctx.candy--
    // SS gauge drain: acc += 0xCCD, -1 per overflow (20 frames); SS3: (22*maxEP + 0x3333) per frame in 1/65536
    if (this.state !== 'transform') {
      if (this.form.ss3) {
        this.ss3Gauge -= 22 * this.maxEp + 0x3333
        if (this.ss3Gauge <= 0) this.revert(w)
      } else if (this.form.ss) {
        this.ssAcc += 0xccd
        if (this.ssAcc >= 0x10000) {
          this.ssAcc -= 0x10000
          this.ssGauge--
          if (this.ssGauge <= 0) this.revert(w)
        }
      } else if (this.ssCooldown > 0) this.ssCooldown = Math.max(0, this.ssCooldown - (this.eff.ssCooldownRate >> 6))
    }
    // wraps: 8 new d-pad presses within 60 frames shake them off
    if (this.wraps > 0 && w.input.pressed & DPAD) {
      this.wrapPresses.push(w.frame)
      this.wrapPresses = this.wrapPresses.filter((f) => w.frame - f < 60)
      if (this.wrapPresses.length >= 8) {
        this.wraps = 0
        this.wrapPresses = []
        w.logEvent('info', 'mummy wraps shaken off')
      }
    }
  }

  private moveInput(w: World, allowDash: boolean) {
    const { held, pressed } = w.input
    if (!(held & DPAD)) {
      if (this.state !== 'idle') this.setState(w, 'idle')
      return
    }
    const nd = pressed & DPAD
    if (nd && allowDash) {
      if (w.frame - this.lastDirFrame < 20 && (nd & -nd) === nd && this.lastDirKey & nd) {
        this.lastDirKey = nd
        this.lastDirFrame = w.frame
        this.facing = nd & BTN.LEFT ? 2 : nd & BTN.RIGHT ? 3 : nd & BTN.UP ? 1 : 0
        audioEvent('dash')
        return this.setState(w, 'dash')
      }
      this.lastDirKey = nd
      this.lastDirFrame = w.frame
    }
    // idle -> walk: the frame that enters the walk state only sets the facing / animation; the walk state moves
    // from the next frame on (emulator trace: x unchanged on the first held frame, then 5*SPD+102 every frame)
    const starting = this.state !== 'walk'
    if (starting) this.setState(w, 'walk')
    const sp = starting ? 0 : this.walkSpeed()
    let combo = 0
    // walk state 0x08013230: horizontal step first (try-move flags 2), then vertical (flags 1)
    if (held & BTN.LEFT) {
      if (sp) this.walkTry(w, -sp, 0, 2)
      combo = 1
    } else if (held & BTN.RIGHT) {
      if (sp) this.walkTry(w, sp, 0, 2)
      combo = 2
    }
    if (held & BTN.UP) {
      if (sp) this.walkTry(w, 0, -sp, 1)
      combo += 3
    } else if (held & BTN.DOWN) {
      if (sp) this.walkTry(w, 0, sp, 1)
      combo += 6
    }
    this.facing = FACE_TBL[combo][this.facing] as Dir
  }

  private updDash(w: World) {
    const { held, pressed } = w.input
    if (!(this.lastDirKey & held)) return this.setState(w, 'idle')
    if (pressed & BTN.A) return this.melee(w)
    if (pressed & BTN.B) return this.startSkill(w)
    const [dx, dy] = DIR_VEC[this.facing]
    const sp = this.dashSpeed()
    if (!this.moveBy(w, dx * sp, dy * sp)) return this.setState(w, 'bump')
    const ws = this.walkSpeed()
    if (dx) this.moveBy(w, 0, held & BTN.UP ? -ws : held & BTN.DOWN ? ws : 0)
    else this.moveBy(w, held & BTN.LEFT ? -ws : held & BTN.RIGHT ? ws : 0, 0)
  }

  // ---- melee -------------------------------------------------------------------------------------------------
  /** 0x08012A24: one instant hit test (mult 0x400, attack 0x086B8098), then state 4 + rand(3); back to idle 16 frames later (emulator trace, work/compare/trace_dash.txt) */
  melee(w: World) {
    audioEvent('melee_swing')
    const rect = rectAt(PUNCH_RECT[this.facing], this.px, this.py)
    const msg: HitMsg = { team: 1, rect, dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this }
    const hits = this.dealtBefore
    broadcast(w, msg)
    if (this.dealtBefore > hits) this.ctx.spawnFx(w, 'fx_086fd9d0', (rect.x0 + rect.x1) >> 1, (rect.y0 + rect.y1) >> 1, [0, 1], 3)
    this.punchAnim = w.rng.rand(3, 'punch anim')
    this.setState(w, 'punch')
    this.setAnim(['attack1', 'attack2', 'attack3'][this.punchAnim], true, false)
  }
  /** count of damaging hits dealt (for the melee EP hook) */
  dealtBefore = 0
  /** called by enemies when this player's attack dealt damage */
  onDealt(_w: World, msg: HitMsg, dmg: number) {
    if (dmg <= 0) return
    this.dealtBefore++
    if (msg.source === this && msg.atk.kind === 0) {
      // script #2: +1 EP per damaging melee hit (Armor of Light +2, Talisman +1 more; Darkness / Vampire Cape: HP+1)
      if (this.eff.meleeHpInstead) this.hp = Math.min(this.maxHp, this.hp + 1)
      else this.ep = Math.min(this.maxEp, this.ep + this.eff.meleeEp)
    }
  }

  // ---- skills ------------------------------------------------------------------------------------------------
  skillLv(id: number) {
    const s = this.ctx.data.skills.get(id)
    return s && s.maxLevel === 2 ? Math.min(2, this.skillLevel) : 0
  }
  skillCost(id: number) {
    const s = this.ctx.data.skills.get(id)
    if (!s) return 0
    if (id === 7) return Math.ceil((this.maxEp * 0xccd) / 65536)
    if (id === 20 || id === 22 || id === 18 || id === 12) return id === 20 ? 10 : 0
    return s.costs[Math.min(this.skillLv(id), s.costs.length - 1)] ?? s.minEp
  }
  canUseSkill(id: number): boolean {
    if (this.ctx.candy > 0) return false
    switch (id) {
      case 12:
      case 19:
      case 21:
        return false
      case 18:
        return this.form.id === 10 || this.form.id === 14
      case 20:
        if (this.inSS) return true
        return this.form.up !== 255 && this.ssCooldown === 0 && this.ep >= this.scaled(10) && this.ctx.data.forms[this.form.up]?.ss
      case 22:
        return this.form.ss && !this.form.ss3 && this.form.up !== 255 && !!this.ctx.data.forms[this.form.up]?.ss3
      default:
        return this.canAfford(this.skillCost(id))
    }
  }

  startSkill(w: World) {
    const id = this.skill
    if (!this.canUseSkill(id)) return
    audioEvent('skill_start', { id })
    const lv = this.skillLv(id)
    const mult = (SKILL_MULT[id] ?? [1024])[Math.min(lv, (SKILL_MULT[id] ?? [1024]).length - 1)]
    // costs paid on press (moves.json: "paid on press" / activation)
    if ([4, 5, 7, 8, 13, 17, 23].includes(id)) this.spend(w, this.skillCost(id))
    if (id === 3) this.spend(w, 2)
    if (id === 20) {
      if (this.inSS) return this.revert(w)
      this.spend(w, 10)
      return this.startTransform(w, this.form.up)
    }
    if (id === 22) return this.startTransform(w, this.form.up)
    if (id === 18) {
      this.applyForm(this.form.id === 14 ? 10 : 14)
      this.ctx.puff(w, this.center()[0], this.center()[1])
      return
    }
    this.state = 'skill'
    this.t = 0
    this.sk = { id, t: 0, held: true, release: -1, acc: 0, proj: null }
    void mult
    const anim: Record<number, (string | number)[]> = {
      7: ['kiblast'], 4: ['kiblast'], 23: ['kiblast'], 2: ['special_charge', 'kiblast'], 17: ['special_charge', 'kiblast'], 9: ['special_charge', 'attack1'],
      8: ['attack2', 'attack1'], 5: ['special_charge', 'dash'], 13: ['special_charge', 'meditate'], 14: ['meditate', 'special_charge'],
    }
    this.setAnim(anim[id] ?? ['special_beam', 'kiblast'], true, false)
  }

  private muzzle(d = 16): [number, number] {
    const [cx, cy] = this.center()
    const [dx, dy] = DIR_VEC[this.facing]
    return [cx + dx * d, cy + dy * d + (dx ? 2 : 0)]
  }

  private endSkill(w: World) {
    const s = this.sk
    if (!s) return
    audioEvent('skill_end')
    const p = s.proj
    if (p && !p.removed) {
      if (p instanceof GrowBeam) p.stop()
      else if (p instanceof BigBangBall && !p.launched) p.removed = true
      else if (p instanceof MasenkoCursor && p.released < 0) p.removed = true
      else if (p instanceof AreaHit && p.kind === 'dragon') p.removed = true
    }
    for (const g of s.ghosts ?? []) if (!g.free) g.removed = true
    this.sk = null
    void w
  }

  private updSkill(w: World) {
    const s = this.sk
    if (!s) return this.setState(w, 'idle')
    s.t++
    const held = !!(w.input.held & BTN.B)
    if (s.held && !held) {
      s.held = false
      s.release = s.t
    }
    const id = s.id
    const lv = this.skillLv(id)
    const mult = (SKILL_MULT[id] ?? [1024])[Math.min(lv, (SKILL_MULT[id] ?? [1024]).length - 1)]
    const atk = ATK[SKILL_SUB[id] ?? 1]
    const end = () => this.setState(w, 'idle')
    switch (id) {
      case 7: {
        // Energy Blast: shot at state frame 16, idle at 48; B again once the timer reached 40 -> refire
        if (w.input.pressed & BTN.B && s.t > 1) s.refire = true
        if (s.t === 16) {
          const [mx, my] = this.muzzle(10)
          this.blast = kiBlast(this.ctx, this, this.facing, 1, mx << 8, my << 8)
          audioEvent('ki_blast')
          w.add(this.blast)
        }
        if (s.t >= 40 && s.refire && this.canAfford(this.skillCost(7))) {
          this.spend(w, this.skillCost(7))
          s.refire = false
          s.t = 12
          this.setAnim('kiblast', true, false)
        }
        if (s.t >= 48) end()
        return
      }
      case 15:
      case 16:
      case 6:
      case 1:
      case 11:
      case 3: {
        // beams: fire on frame 24 while B is held (24 for BBK's volley start ~26), drain per frame while held
        const fire = id === 3 ? 26 : id === 11 ? 30 : 24
        if (!s.proj) {
          if (s.t < fire) return
          if (!held) return s.t >= 30 ? end() : undefined
          const speed = id === 11 ? 2.14 : id === 3 ? 8.5 : 3.34
          const maxLen = id === 11 ? [64, 64, 96][lv] : 240
          audioEvent('beam_fire')
          s.proj = w.add(new GrowBeam(this.ctx, this, this.facing, 1, () => this.muzzle(16), { atk, mult, width: id === 1 ? 12 : lv === 0 ? 16 : 16, speed, maxLen, color: BEAM_COLOR[id] ?? ['#fff', '#3bf'] }))
          this.setAnim(['special_beam', 'kiblast'], false, true)
          return
        }
        if (!held) return end()
        const per = id === 1 ? 1024 : id === 3 ? 1000 : BEAM_DRAIN[lv]
        this.drain(w, per)
        if (this.energyLeft() <= 0) end()
        return
      }
      case 10: {
        // Final Flash: instant full beam, 8 hits from frame 31
        if (s.t === 30) {
          if (!held && s.release < 26) return end()
          const [mx, my] = this.muzzle(14)
          audioEvent('beam_fire')
          s.proj = w.add(new FullBeam(this.ctx, this, this.facing, 1, mx, my, { atk, mult, life: 8 }))
        }
        if (s.t > 30 && s.t <= 40) this.drain(w, BEAM_DRAIN[lv])
        if (s.t < 26 && !held) return end()
        if (s.t >= 45) end()
        return
      }
      case 2: {
        // Big Bang: charge while held (EP into the ball), release >= 60 frames to fire, else it fizzles
        if (!s.proj) {
          const [mx, my] = this.muzzle(22)
          s.proj = w.add(new BigBangBall(this.ctx, this, this.facing, mx << 8, my << 8))
        }
        const ball = s.proj as BigBangBall
        if (s.release < 0) {
          const rate = [43, 141, 427][lv] // 0.168 / 0.551 / 1.668 EP per frame in 1/256
          if (s.t > 60) ball.grow += [0, 0.36, 0.48][lv]
          if (this.energyLeft() > 0 && ball.charge256 < 1000 * 256) {
            ball.charge256 += rate
            this.drain(w, rate)
          }
          const [mx, my] = this.muzzle(22)
          ball.x = mx << 8
          ball.y = my << 8
          return
        }
        if (!ball.launched) {
          if (s.release < 60) {
            ball.removed = true
            return end()
          }
          ball.launched = true
          audioEvent('big_bang')
          this.setAnim(['special_release', 'kiblast'], true, false)
        }
        if (s.t - s.release >= 20) {
          this.sk!.proj = null
          end()
        }
        return
      }
      case 4: {
        // Burning Attack: one shot ~2.86 px/frame at frame 18, idle at 40
        if (s.t === 18) {
          const [mx, my] = this.muzzle(12)
          const [vx, vy] = VEL(this.facing, 732)
          w.add(new StraightShot(this.ctx, this, this.facing, 1, mx << 8, my << 8, { vx, vy, box: [7, 7, 14, 14], atk, mult, color: ['#fff6d0', '#e08020'], radius: 6, name: 'burning' }))
          audioEvent('wave')
        }
        if (s.t >= 40) end()
        return
      }
      case 23: {
        // Ultra Volleyball: shot at frame 13 (first hit f29 at 60 px), attack 16 (power 0)
        if (s.t === 13) {
          const [mx, my] = this.muzzle(14)
          const [vx, vy] = VEL(this.facing, 0x300)
          w.add(new StraightShot(this.ctx, this, this.facing, 1, mx << 8, my << 8, { vx, vy, box: [6, 6, 12, 12], atk: ATK[16], mult: 1024, color: ['#ffffff', '#f0a040'], radius: 5, name: 'volley' }))
          audioEvent('wave')
        }
        if (s.t >= 45) end()
        return
      }
      case 8: {
        // Energy Punch: 5 hits (frames 10..14); reach by level (only Lv3 connected at 40 px in the emulator; INFERRED)
        // released before the burst (frame 10) -> cancelled (measured: taps never hit, idle at frame 10)
        if (s.release >= 0 && s.release < 10) return s.t >= 10 ? end() : undefined
        if (s.t >= 10 && s.t <= 14) {
          const reach = [8, 16, 40][lv]
          const r = this.frontRect(reach, 24)
          broadcast(w, { team: 1, rect: r, dir: this.facing, mult, atk, attacker: this, source: this })
        }
        if (s.t >= 18) end()
        return
      }
      case 9: {
        // Energy Sword: charge while held (4 EP on 5 of 6 frames ~3.3/f), full charge at 120 f (INFERRED);
        // release after full charge -> one slash 3 frames later (x10.0); released earlier -> cancelled
        if (s.release < 0) {
          if (s.t % 6 !== 0) this.spend(w, 4)
          if (s.t >= 120 && !s.charged) {
            s.charged = true
            this.flash = 12
          }
          if (this.energyLeft() <= 0) return end()
          return
        }
        if (!s.charged) return end()
        if (s.t - s.release === 3) {
          this.setAnim(['attack3', 'attack1'], true, false)
          audioEvent('beam_fire')
          broadcast(w, { team: 1, rect: this.frontRect(40, 32), dir: this.facing, mult, atk, attacker: this, source: this })
        }
        if (s.t - s.release >= 16) end()
        return
      }
      case 5: {
        // Dragon Fist (fitted to the measured runs): released before the dragon appears -> cancelled (tap: idle at
        // frame 3); the head appears at frame 20, 24 px ahead, grows 4 px/frame to 56 px (no hit at 100 px) and is
        // steered with the d-pad; it lands 48 hits (x3.28) on frames 21..35 while it overlaps; 5 EP/frame for
        // 8 frames on top of the 300 paid on the press
        if (!s.held && s.t < 20) return end()
        if (s.t === 20) {
          s.charge = 24
          s.proj = w.add(new AreaHit(this.ctx, this, this.facing, 1, this.frontRect(8, 16), { life: 20, every: 1000, atk, mult, color: ['#fffbd0', '#ffb020'] }))
          s.proj.kind = 'dragon'
          audioEvent('beam_fire')
          s.hits = 0
        }
        if (s.proj && !s.proj.removed && s.t >= 20) {
          s.charge = Math.min(56, (s.charge ?? 24) + 4)
          const a = s.proj as AreaHit
          let off = 0
          if (w.input.held & (this.facing >= 2 ? BTN.UP : BTN.LEFT)) off = -1
          if (w.input.held & (this.facing >= 2 ? BTN.DOWN : BTN.RIGHT)) off = 1
          const [dx, dy] = DIR_VEC[this.facing]
          const [cx, cy] = this.center()
          const hx = cx + dx * s.charge + (dx ? 0 : off * (s.t - 20))
          const hy = cy + dy * s.charge + (dy ? 0 : off * (s.t - 20))
          a.box = { x0: hx - 10, y0: hy - 10, x1: hx + 10, y1: hy + 10 }
          if (s.t >= 21 && s.t <= 35) {
            const n = 3 + (s.t % 7 === 0 ? 1 : 0)
            for (let k = 0; k < n; k++) a.hit(w, a.box, atk, mult, false)
          }
          if (s.t >= 22 && s.t < 30) this.spend(w, 5)
        }
        if (s.t >= 40) end()
        return
      }
      case 13: {
        // Kamikaze Ghost: a ghost every 30 frames while held (max 5), released ghosts home in (INFERRED)
        s.ghosts ??= []
        if (s.release < 0) {
          if ((s.t - 1) % 30 === 0 && s.ghosts.length < 5) {
            const [cx, cy] = this.center()
            const g = new Ghost(this.ctx, this, this.facing, (cx + (s.ghosts.length - 2) * 10) << 8, (cy - 22) << 8)
            s.ghosts.push(w.add(g))
          }
          return
        }
        for (const g of s.ghosts) g.free = true
        s.ghosts = []
        if (s.t - s.release >= 10) end()
        return
      }
      case 14: {
        // Instant Transmission: hold -> pick an enemy with the d-pad; release -> appear beside it (INFERRED)
        const targets = this.ctx.liveEnemies()
        s.target ??= 0
        if (w.input.pressed & DPAD) s.target++
        if (s.release < 0) return
        if (!targets.length || !this.canAfford(40)) return end()
        const tgt = targets[s.target % targets.length]
        this.spend(w, 40)
        const [cx, cy] = this.center()
        this.ctx.puff(w, cx, cy)
        const side = tgt.facing === 3 ? -1 : 1
        const nx = tgt.px + (tgt.footW >> 1) - 8 + side * (tgt.footW / 2 + 14)
        const ny = tgt.py + (tgt.footH >> 1) - 4
        if (this.fits(w, nx, ny)) {
          this.x = nx << 8
          this.y = ny << 8
        }
        this.facing = side > 0 ? 2 : 3
        this.ctx.puff(w, this.center()[0], this.center()[1])
        return end()
      }
      case 17: {
        // Masenko (fitted): cursor 24 px ahead, +0.5 px/frame while held up to 48 px; the blast lands 51 frames after
        // the release (hold 60 -> f111, hold 150 -> f203 measured). Taps fizzle at Lv1/Lv2 (idle at f42), Lv3 fires.
        if (!s.proj) s.proj = w.add(new MasenkoCursor(this.ctx, this, this.facing, mult))
        const c = s.proj as MasenkoCursor
        if (s.release >= 0) {
          if (s.release < 10 && lv < 2) {
            c.removed = true
            s.proj = null
            if (s.t >= 42) end()
            return
          }
          c.release()
          if (s.t - s.release === 1) audioEvent('wave')
          if (s.t - s.release === 1) this.setAnim(['throw', 'special_release', 'kiblast'], true, false)
          if (s.t - s.release >= 51) {
            s.proj = null
            end()
          }
        }
        return
      }
      default:
        return end()
    }
  }

  frontRect(reach: number, thick: number): Rect {
    const b = this.bodyBox()
    const [cx, cy] = this.center()
    const h = thick >> 1
    switch (this.facing) {
      case 3:
        return { x0: b.x1, y0: cy - h, x1: b.x1 + reach, y1: cy + h }
      case 2:
        return { x0: b.x0 - reach, y0: cy - h, x1: b.x0, y1: cy + h }
      case 0:
        return { x0: cx - h, y0: b.y1, x1: cx + h, y1: b.y1 + reach }
      default:
        return { x0: cx - h, y0: b.y0 - reach, x1: cx + h, y1: b.y0 }
    }
  }

  // ---- forms ---------------------------------------------------------------------------------------------------
  private startTransform(w: World, to: number) {
    if (to === 255 || !this.ctx.data.forms[to]) return
    this.transformTo = to
    this.state = 'transform'
    audioEvent('transform')
    this.t = 0
    this.blocking = false
    this.setAnim(['transform', 'meditate', 'special_charge'], true, true)
    w.logEvent('info', `transforming -> form ${to}`)
  }
  /** ~211-frame transform; the gauge fills to maxEP during it (0x0801C4F4) */
  private updTransform(w: World) {
    const LEN = 211
    const to = this.ctx.data.forms[this.transformTo]
    if (to?.ss3) this.ss3Gauge = Math.floor((this.maxEp * 65536 * this.t) / LEN)
    else this.ssGauge = Math.floor((this.maxEp * this.t) / LEN)
    if (this.t >= LEN) {
      this.applyForm(this.transformTo)
      audioEvent('transform_done')
      if (this.form.ss3) this.ss3Gauge = this.maxEp * 65536
      else this.ssGauge = this.maxEp
      this.ctx.puff(w, this.center()[0], this.center()[1])
      this.setState(w, 'idle')
    }
  }
  /** revert to the base form; the SS cooldown (rec+4) starts (its start value is INFERRED: 600 frames) */
  revert(w: World) {
    let f = this.form
    while ((f.ss || f.ss3) && f.down !== 255 && this.ctx.data.forms[f.down]) f = this.ctx.data.forms[f.down]
    if (f.id === this.form.id) return
    this.applyForm(f.id)
    this.ssGauge = 0
    this.ss3Gauge = 0
    this.ssCooldown = 600 * 256
    this.ctx.puff(w, this.center()[0], this.center()[1])
    w.logEvent('info', `reverted to form ${f.id}`)
    audioEvent('transform_done')
    if (this.state === 'transform' || this.state === 'skill') this.setState(w, 'idle')
  }

  // ---- held ------------------------------------------------------------------------------------------------------
  private updHeld(w: World) {
    const k = this.holdKind
    const by = this.heldBy
    const pressed = w.input.pressed
    const mash = (mask: number, need: number, gap: number) => {
      if (pressed & mask) {
        if (w.frame - this.holdLast > gap) this.holdCount = 0
        this.holdCount++
        this.holdLast = w.frame
      }
      return this.holdCount > need
    }
    if (by && by.removed) return this.release(w)
    switch (k) {
      case 'vampire':
        // escape: frames with A/B/d-pad held count, > 10 frames without input resets, > 8 releases
        if (w.input.held & (BTN.A | BTN.B | DPAD)) {
          if (w.frame - this.holdLast > 10) this.holdCount = 0
          this.holdCount++
          this.holdLast = w.frame
        }
        if (this.holdCount > 8) return this.release(w)
        break
      case 'hug':
        if (mash(DPAD, 4, 90)) return this.release(w)
        break
      case 'latch':
        if (mash(BTN.A | BTN.B | DPAD, 8, 15)) return this.release(w)
        break
      case 'grapple':
        if (by) {
          const [bx, by2] = by.footCenter()
          const [px, py] = this.footCenter()
          const dx = Math.sign(bx - px)
          const dy = Math.sign(by2 - py)
          if (Math.abs(bx - px) + Math.abs(by2 - py) > 24) this.moveBy(w, dx * 512, dy * 512)
        }
        break
    }
    if (by && (k === 'vampire' || k === 'latch' || k === 'hug')) {
      // pinned next to the holder
      const [bx, byy] = by.footCenter()
      const nx = bx - 8 + (k === 'hug' ? 8 : 0)
      const ny = byy - 4 + (k === 'hug' ? -2 : 2)
      if (this.fits(w, nx, ny)) {
        this.x = nx << 8
        this.y = ny << 8
      }
    }
    if (this.holdLeft > 0 && --this.holdLeft === 0) this.release(w)
  }

  // ---- damage ------------------------------------------------------------------------------------------------
  onMessage(w: World, msg: HitMsg): boolean {
    if (msg.team === 1 || this.dead) return false
    if (this.state === 'held' && this.holdKind === 'eat') return false
    if (!hitsBody(msg, this)) return false
    // block: R state and (hit dir ^ 1) == facing; energy shield (R+B) negates energy hits (INFERRED)
    const front = (msg.dir ^ 1) === this.facing
    const blocking = this.blocking && front
    const res = computeDamage(msg.atk, msg.mult, msg.attacker.stats, this.stats, blocking, w.rng)
    let dmg = res.dmg
    if (this.state === 'shield' && msg.atk.kind === 1) dmg = 0
    msg.crit = res.crit
    msg.raw = res.raw
    msg.dmg = dmg
    const lost = this.hpAcc.take(dmg)
    audioEvent('hurt', { melee: msg.atk.kind === 0 })
    w.logEvent('damage', `${msg.attacker.kind === 'enemy' ? (msg.attacker as unknown as { label: string }).label : msg.attacker.kind} -> player: ${dmg}/1024${res.crit ? ' CRIT' : ''}${blocking ? ' blocked' : ''}${this.state === 'hurt' ? ' (i-frames)' : ''} = ${lost} HP`)
    if (dmg > 0) {
      w.stats.hitsTaken++
      if (lost > 0) {
        w.stats.damageTaken += Math.min(lost, this.hp)
        this.ctx.damagePopup(w, this, lost, res.crit, true)
      }
      this.hp = Math.max(0, this.hp - lost)
      this.flash = 12
      // thorns (record+0x5C): melee attackers take the thorns value (application INFERRED)
      if (this.eff.thorns && msg.atk.kind === 0 && (msg.attacker as unknown as { takeDirect?: (w: World, n: number) => void }).takeDirect) {
        ;(msg.attacker as unknown as { takeDirect: (w: World, n: number) => void }).takeDirect(w, this.eff.thorns)
      }
      this.afterHpChange(w)
      if (!this.dead && !blocking && this.state !== 'held' && this.state !== 'hurt' && this.state !== 'transform') {
        this.hurtDir = msg.dir
        this.setState(w, 'hurt')
      }
    }
    return true
  }
  private afterHpChange(w: World) {
    if (this.equip.some((e) => e?.id === 153)) this.refreshStats()
    if (this.hp <= 0 && !this.dead) {
      this.hp = 0
      this.heldBy = null
      this.holdKind = null
      this.alpha = 1
      this.setState(w, 'dying')
    }
  }

  draw(r: Renderer, w: World) {
    if (this.ctx.candy > 0 && !this.dead) {
      // candy: drawn as a wrapped sweet (the game swaps the player graphics; INFERRED look)
      const [cx, cy] = this.center()
      const g = r.ctx
      g.fillStyle = '#ff70c0'
      g.fillRect(r.sx(cx) - 6, r.sy(cy) - 4, 12, 8)
      g.fillStyle = '#ffe0f0'
      g.fillRect(r.sx(cx) - 9, r.sy(cy) - 3, 3, 6)
      g.fillRect(r.sx(cx) + 6, r.sy(cy) - 3, 3, 6)
      return
    }
    if (this.holdKind === 'stone') {
      super.draw(r, w)
      const b = this.bodyBox()
      r.ctx.globalAlpha = 0.6
      r.ctx.fillStyle = '#909090'
      r.ctx.fillRect(r.sx(b.x0), r.sy(b.y0), b.x1 - b.x0, b.y1 - b.y0)
      r.ctx.globalAlpha = 1
      return
    }
    super.draw(r, w)
    if (this.state === 'shield') {
      const [cx, cy] = this.center()
      r.ctx.globalAlpha = 0.35 + 0.15 * Math.sin(w.frame / 3)
      r.ctx.strokeStyle = '#80e0ff'
      r.ctx.lineWidth = 2
      r.ctx.beginPath()
      r.ctx.arc(r.sx(cx), r.sy(cy), 16, 0, Math.PI * 2)
      r.ctx.stroke()
      r.ctx.lineWidth = 1
      r.ctx.globalAlpha = 1
    }
    if (this.wraps > 0) drawText(r.ctx, `x${this.wraps}`, r.sx(this.px + 18), r.sy(this.py - 24), '#e0d0a0')
  }

  debugLabel() {
    return `${this.state} t${this.t}${this.sk ? ' skill ' + this.sk.id + ' t' + this.sk.t : ''}\nSTR${this.stats.STR} POW${this.stats.POW} END${this.stats.END}/${this.stats.END2}`
  }
}
