// Buu's Fury player: movement (walk 5*SPD+102, dash +320 on a double tap within 20 f), melee (one instant hit on
// the A press, rects 0x083C344C, 16-frame recovery), block (R: 1/16 from the front), energy shield (R+B), the 23
// skills with their EP rules (cost table 0x0871DA98 scaled by record+0x56/16, SS gauge while transformed), forms
// (SS / SS3 gauges, Saiyaman), regen (timer +110/frame -> 3% EP), 24-frame hurt state with zero resists, crits,
// equipment hooks (EP/HP per melee hit, HP regen, Crisis Ring) and the hold states enemies put the player in.
// Sources: re/buu/notes.md §3-§6, data/buu/{characters,moves,attacks_detail,formulas,items}.json.
// Skill state machines are not decoded instruction by instruction: their timings are fitted to the measured runs
// in moves.json / attacks_detail.json (first-hit frames, hit counts, EP spent) and marked INFERRED where guessed.

import { BTN, DPAD } from '../../core/input'
import { overlaps, rectAt, type Dir, type Rect } from '../../core/geom'
import type { Entity, World } from '../../core/world'
import type { Renderer } from '../../core/renderer'
import { Actor, broadcast, hitsBody, SIN256, type HitMsg } from './actor'
import type { BuuCtx, HoldKind, PlayerLike } from './context'
import { ATK, computeDamage, HpAcc } from './damage'
import type { BuuForm, EquipDef } from './data'
import { recalcStats, levelOf, levelUp, xpGain, type BuuRecord, type EffStats } from './progression'
import { audioEvent } from '../../audio/bus'
import { BEAM_POS, BeamCharge, beamGfx, drawRec, drawRecAddr, LoopFx, RecObj } from './fx'
import { BBK_FLARE_POS, BbkFlareFx, DragonFistFx, ENERGY_PUNCH_POS, ENERGY_PUNCH_TABLE, EnergySwordFx, FINAL_FLASH_POS, FINAL_FLASH_TABLES, FLAME_CHARGE, FlameKameFx, GHOST_SPAWN, ItMarkersFx, SS_AURA } from './skillfx'
import { ANIM_RAW, ANIM_STATE, tickBored, tickIdle, tickLoop, tickOnce, type AnimPos } from './panim'
import { BIGBANG_POS, BigBangBall, FullBeam, Ghost, GrowBeam, KameBeam, MasenkoCursor, VolleyWave, StraightShot, kiBlast, VEL, BURNING_HOLD, burningFx, burningEnd, type Shot } from './projectiles'

/** melee rects 0x083C344C (x0,y0,x1,y1 relative to the position; same for every form) */
/** Dabura's stone state (vtable 0x0804AB34), named like the emulator's lockstep rows */
const STONE = 'vt_0804AB34'

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
/** the raw animation each skill's activate sets (0x0800ECDA(player, raw - 1)), skill table 0x0871DAF8 +0x14 */
const SKILL_RAW: Record<number, number> = { 1: 19, 2: 19, 3: 19, 4: 4, 5: 20, 6: 19, 7: 4, 8: 20, 9: 20, 10: 20, 11: 19, 13: 20, 15: 19, 16: 19, 17: 13, 23: 4 }
/** canUse EP checks (EP >= cost * factor / 16): byte tables 0x0871DA98.. / u32 0x0871DAB0.. per level */
const SKILL_CHECK: Record<number, number[]> = {
  1: [200], 2: [10, 33, 100], 3: [12], 4: [35, 105, 420], 5: [300], 6: [15, 50, 150], 8: [25, 70, 290], 9: [350], 10: [15, 45, 140],
  11: [15, 44, 145], 13: [160], 14: [40], 15: [15, 45, 150], 16: [15, 45, 150], 17: [50, 200, 800], 23: [350],
}
/** Energy Punch Lv3 hit rects 0x0871DF30 (relative to the position; Lv1/2 use the melee rects) */
const ENERGY_PUNCH_RECT_L3 = [
  [0, 0, 24, 28],
  [-4, -36, 20, 0],
  [-20, -24, 0, 8],
  [16, -24, 36, 8],
]
/** Energy Sword strike boxes (defs 10..17: origin ox, oy and size; box = position - origin .. + size) */
const SWORD_BOX = [
  [39, 10, 32, 8],
  [29, 2, 32, 32],
  [-4, 0, 8, 32],
  [-13, 2, 32, 32],
  [-23, 10, 32, 8],
  [-12, 46, 32, 32],
  [-4, 46, 8, 32],
  [28, 46, 32, 32],
]
/** Ultra Volleyball ball position (player position + 0x0871E1E0[dir]) */
const VOLLEY_POS: [number, number][] = [[10, 8], [10, -20], [-8, -8], [24, -8]]
/** dash direction table 0x0807268C (times the dash speed) */
const DASH_VEC = [
  [0, 1],
  [0, -1],
  [-1, 0],
  [1, 0],
]
/** hurt / bump knockback 0x080725C0: position -= vector[facing] every frame (0.75 px) */
const KNOCK_VEC = [
  [0, 192],
  [0, -192],
  [-192, 0],
  [192, 0],
]
/** blocked hit pushback 0x080727A0: position += vector[facing] once (2 px backwards) */
const BLOCK_PUSH = [
  [0, -512],
  [0, 512],
  [512, 0],
  [-512, 0],
]
/** the energy shield graphics 0x087202E8 (record = power >> 3; 0..2 fade in, 3..6 loop) */
const SHIELD_FX = 'fx_087202e8'
/** stuck-bandage slots (0x08756BB4): y offsets above the player position */
const WRAP_SLOT_Y = [-2, -10, 2, 18, 6, 10, -6, 14]

export type PState = 'idle' | 'walk' | 'dash' | 'punch' | 'block' | 'shield' | 'hurt' | 'bump' | 'dying' | 'skill' | 'transform' | 'held'

const SKILL_MULT: Record<number, number[]> = {
  1: [1640], // Banshee
  3: [164], // Big Bang Kamehameha
  4: [410, 820, 1649], // Burning Attack
  5: [328], // Dragon Fist
  6: [41, 164, 328], // Electric Kamehameha
  8: [62, 144, 226], // Energy Punch (0x0871DF90)
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
  sk: { id: number; t: number; held: boolean; release: number; proj?: Shot | null; fx?: Entity | null; acc: number; refire?: boolean; ghosts?: Ghost[]; target?: number; charged?: boolean; charge?: number; hits?: number } | null = null
  hpAcc = new HpAcc()
  epAcc = 0
  regenTimer = 0
  /** story Trunks forms 24 / 25: the form object's countdown (+0xC; form 24 transforms, form 25 reverts on expiry) */
  storyCnt = 0
  /** cutscene fusions (form object 0x03001B64): its countdown (+0x10, 300 at the construction) and the defuse script */
  fusCnt = 300
  defuse: { t: number; x0: number } | null = null
  /** the partner's record that takes over at the defuse (the lockstep copies the game's; else a default) */
  partner: { char: number; form: number; hp: number; maxHp: number; ep: number; maxEp: number; base: number[]; skills: number[]; slot: number; levels: number[]; ssGauge: number; ss3Gauge: number; cooldown: number } | null = null
  hpRegenAcc = 0
  dead = false
  heldBy: Actor | null = null
  holdKind: HoldKind | null = null
  holdLeft = 0
  holdCount = 0
  holdLast = 0
  blast: StraightShot | null = null
  wraps = 0
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
  /** the game's state object at obj+0x14 (lockstep name, tools/battle/lockstep/emu_lockstep.py STATE_NAMES) */
  gs = 'idle'
  /** obj+0x50 animation (raw index) / +0x11 frame / +0x12 tick (panim.ts) */
  ap: AnimPos = { raw: 1, frame: 0, tick: 0 }
  /** state object scratch: idle +4 counter, block +4 shield EP accumulator */
  sCnt = 0
  /** energy shield object 0x0801DFE0 of the block state: +0x40 power (0..55), +0x44 active */
  shield: { pow: number; on: boolean } | null = null
  /** obj+0x1B8 / +0x1B9 (0 during hurt / a full shield) */
  resist = 128
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
    this.syncAnim()
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
    this.syncAnim()
  }
  /** RecalcStats 0x08013C66 + apply to the player object 0x0800E074 + form bonuses */
  refreshStats() {
    this.eff = recalcStats(this.rec, this.equip, this.holyLeft > 0)
    const b = this.form.bonus
    const e = this.eff
    this.stats = {
      STR: e.STR + b.STR,
      POW: e.POW + b.POW,
      END: e.END + b.END,
      END2: e.END2 + b.END2,
      resM: this.resist,
      resE: this.resist,
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
    // the state's vfunc +0x1C (IfInPlayerBeamLine 0x08029B30). Ki Blast (state vtable 0x0804DC14): 0x0804FA30 =
    // state byte +4 (the refire request: B pressed again, control 0x0801AFE2) or anim tick < 16, i.e. until the
    // blast leaves the hand (lockstep f02_vs_hold_stone: Dabura's guard 0x75 / 0x77). Other skills: INFERRED any
    // skill state.
    if (this.gs === 'skill7') return !!this.sk?.refire || this.ap.tick < 16
    return this.state === 'skill'
  }
  isStone() {
    // IfPlayerPetrified 0x0803AE65: the player's current animation is the stone pose (raw 22)
    return this.gs === STONE || this.holdKind === 'stone'
  }

  // ---- Dabura's stone (special hold; lockstep f02_vs_hold_stone) ----------------------------------------------------
  // The spit's accepted hit (0x0802E60A, after the normal hit: damage and reaction) does SetAnimRaw(0x15) (raw 22)
  // + SetState(new state, vtable 0x0804AB34). Enter 0x0802E354: state+8 = 150; anim tick 0; a counter object
  // (vtable 0x08049D74, +4 = 150) replaces the player's controller obj+0x224 (0x08010BB8). Its damage reaction
  // (+0xC, 0x0802E348) is counter -= damage >> 10 instead of the hurt state (HP is still lost). Tick (+4): none.
  // Control (+8, 0x0802E384): a newly pressed A / B / d-pad key (mask 0xF3) -> counter -= 5; frame = counter < 75
  // (cracked); counter <= 0 -> a puff at the body centre (rate 0x1000 + rand(0x1000), pool room >= 0x400) and
  // SetAnim(0). No timer. Exit 0x0802E42E: the default controller 0x03001C3C back. A new spit while petrified
  // builds a new state (counter 150 again).
  stoneHp = 0
  petrify(w: World) {
    if (this.dead) return
    this.ap.raw = 22
    this.enterState(w, STONE)
    this.stoneHp = 150
  }
  private ctlStone(w: World) {
    if (w.input.pressed & 0xf3) this.stoneHp -= 5
    this.ap.frame = 75 > this.stoneHp ? 1 : 0
    if (this.stoneHp > 0) return
    this.ctx.puffRng(w, ...this.center())
    this.setAnimId(w, 0)
  }
  // ---- end stone ---------------------------------------------------------------------------------------------------
  isCandy() {
    return this.ctx.candy > 0
  }
  swinging() {
    return this.state === 'punch'
  }
  /** `raw`: the animation the holder shows (frame 0); default the hurt animation's first frame (raw 10) */
  hold(w: World, by: import('./actor').Actor, kind: HoldKind, frames: number, raw = 10) {
    if (this.dead) return
    this.endSkill(w)
    this.heldBy = by
    this.holdKind = kind
    this.holdLeft = frames
    this.holdCount = 0
    this.holdLast = w.frame
    this.enterState(w, 'hold')
    this.state = 'held'
    this.t = 0
    this.blocking = false
    // INFERRED: the enemies' hold controllers are not decoded; the hurt animation's first frame is shown
    if (kind === 'eat') this.alpha = 0
    else this.ap.raw = raw
    this.ap.frame = 0
  }
  release(w: World) {
    if (this.state !== 'held') return
    // breaking out of the stone: a puff at the body centre (rate 0x1000 + rand(0x1000))
    if (this.holdKind === 'stone') this.ctx.puffRng(w, ...this.center())
    this.heldBy = null
    this.holdKind = null
    this.alpha = 1
    this.setAnimId(w, 0)
  }
  // [H] generic infrastructure for the enemies' holds that drive the player's state directly
  /** 0x0800ED18(player, 0x03001B00): the hold state (frame kept, no tick, no control) */
  enterHold(w: World) {
    if (this.dead) return
    this.endSkill(w)
    this.enterState(w, 'hold')
  }
  /** SetAnim(0) (vfunc +0x28 0x0800ECF0 with id 0) */
  toIdle(w: World) {
    if (this.dead) return
    this.setAnimId(w, 0)
  }
  /** SetAnimRaw(raw - 1) (0x0800ECDA), frame = `frame`, then SetState(0x03001B00) (BearHug enter 0x0802C4C9) */
  holdPose(w: World, raw: number, frame: number) {
    if (this.dead) return
    this.endSkill(w)
    this.ap.raw = raw
    this.ap.frame = frame
    this.enterState(w, 'hold')
  }
  get poseFrame() {
    return this.ap.frame
  }
  set poseFrame(f: number) {
    this.ap.frame = f
  }

  // [H] ---- Majin Buu's candy (beam 0x080287AD, candy states 0x0804BE60 / 0x0804B7AC / 0x0804C2CC, eat 0x0802AF11) -----
  /** the shown sprite def is 0x36 (the sweets, sprite index 45) instead of the form's (SetDef 0x0800EDF8) */
  candyDef = false
  /** candy hop: the move code kept in the walk state (+4): 1 / 2 left / right + 3 / 6 up / down */
  candyCode = 0
  /** the player object is out of the world (Majin Buu's eat, tick 8 .. 119): no update, no hits, not drawn */
  gone = false
  /** the beam's accepted hit 0x08028862: SetDef(0x36), a new candy idle state (SetAnimRaw(0) + SetState), frame
   *  rand(3), candy timer 0x03002FE4 = 300 */
  candyHit(w: World) {
    if (this.dead) return
    this.endSkill(w)
    this.candyDef = true
    this.ap.raw = 1
    this.enterState(w, 'candy')
    this.ap.frame = w.rng.rand(3, 'candy sweet')
    this.ctx.candy = 300
    w.logEvent('info', 'player turned into candy (300 f)')
  }
  /** eat controller t 0: SetDef(0x36), the candy-eat state (enter 0x0804EAD5: tick 0, position = (x, y), KO flag 0) */
  candyEat(w: World, x: number, y: number) {
    this.candyDef = true
    this.ap.raw = 1
    this.enterState(w, 'candy_eat')
    this.z = 0
    this.x = x
    this.y = y
  }
  /** eat t 8: the player object leaves the world (0x08011944); the candy-eat state's destructor (0x0804F69D) clears
   *  the candy timer */
  candyGone(_w: World) {
    this.gone = true
    this.ctx.candy = 0
  }
  /** eat t 120: the new player object (0x080109AA(0, position, the form's def, 1)): facing down, the form's def, a new
   *  idle state (SetAnimRaw(0) + SetState, the idle enter's rand(255)); the KO flag -> KO (INFERRED: the message
   *  vfunc +0x5C with multiplier 0x400 is taken as the KO) */
  candyRespawn(w: World, x: number, y: number, ko: boolean) {
    this.gone = false
    this.candyDef = false
    this.x = x
    this.y = y
    this.z = 0
    this.facing = 0
    this.flash = 0
    // the constructor: an idle start tick (0x0800DFBE) and SetAnim(0) into the idle singleton 0x03002B44 (its enter
    // draws again), then the eat's own new idle state 0x0804C790 (SetAnimRaw(0) + SetState: a third rand(255))
    w.rng.rand(255, 'new player idle tick')
    w.rng.rand(255, 'new player idle 0x03002B44')
    this.ap.raw = 1
    this.enterState(w, 'idle')
    if (ko) {
      this.hp = 0
      this.afterHpChange(w)
    }
  }
  /** candy idle control 0x08028390: timer 0 -> the form's def, SetAnim(0); a held d-pad key -> facing (left 2,
   *  right 3, up 1, down 0; the later one wins), a new candy walk state (SetAnimRaw(0)); then timer > 0 -> - 1 */
  private ctlCandy(w: World) {
    const held = w.input.held
    if (this.ctx.candy === 0) {
      this.candyDef = false
      return this.setAnimId(w, 0)
    }
    if (held & DPAD) {
      if (held & BTN.LEFT) this.facing = 2
      if (held & BTN.RIGHT) this.facing = 3
      if (held & BTN.UP) this.facing = 1
      if (held & BTN.DOWN) this.facing = 0
      this.ap.raw = 1
      this.enterState(w, 'candy_walk')
    }
    // [H] the candy timer 0x03002FE4 counts down in the candy states' controls only (ctlCandy / ctlCandyWalk)
  }
  /** candy walk control 0x080284C0 (t = obj+0x12): at t 0 the timer 0 -> the form's def, SetAnim(0) (nothing more);
   *  no d-pad -> a new candy idle state; else a walk-speed step (left / right first, try-move flags 2, then up / down,
   *  flags 1) and its move code kept; t 8..120 repeat the kept code's steps; then t = (t + 8) & 0x7F, timer > 0 -> - 1.
   *  The facing never changes here. */
  private ctlCandyWalk(w: World) {
    const held = w.input.held
    const a = this.ap
    const sp = this.walkSpeed()
    if (a.tick === 0) {
      if (this.ctx.candy === 0) {
        this.candyDef = false
        this.z = 0
        return this.setAnimId(w, 0)
      }
      if (!(held & DPAD)) {
        this.ap.raw = 1
        this.enterState(w, 'candy')
      } else {
        let code = 0
        if (held & BTN.LEFT) {
          this.walkTry(w, -sp, 0, 2)
          code = 1
        } else if (held & BTN.RIGHT) {
          this.walkTry(w, sp, 0, 2)
          code = 2
        }
        if (held & BTN.UP) {
          this.walkTry(w, 0, -sp, 1)
          code += 3
        } else if (held & BTN.DOWN) {
          this.walkTry(w, 0, sp, 1)
          code += 6
        }
        this.candyCode = code
      }
    } else {
      const c = this.candyCode
      if (c === 1 || c === 4 || c === 7) this.walkTry(w, -sp, 0, 2)
      if (c === 2 || c === 5 || c === 8) this.walkTry(w, sp, 0, 2)
      if (c === 3 || c === 4 || c === 5) this.walkTry(w, 0, -sp, 1)
      if (c === 6 || c === 7 || c === 8) this.walkTry(w, 0, sp, 1)
    }
    a.tick = (a.tick + 8) & 0x7f
    if (this.ctx.candy > 0) this.ctx.candy--
  }
  /** Actor.setAnim for the skill code: the named animation (raw index n+1 / name) becomes obj+0x50 */
  setAnim(ref: number | string | (number | string)[], restart = true, _loop?: boolean) {
    if (!this.sprite) return
    const refs = (Array.isArray(ref) ? ref : [ref]).map((r) => (typeof r === 'number' ? r + 1 : r))
    const a = this.sprite.pick(...refs)
    if (!a) return
    if (a.raw !== this.ap.raw || restart) this.ap.tick = this.ap.frame = 0
    this.ap.raw = a.raw
    this.syncAnim()
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
      this.sfx(36) // level up: bytecode script #3 (push 36, native 0x1F)
      w.logEvent('info', `level ${old} -> ${nl} (+${(nl - old) * 3} stat points)`)
    }
  }

  // ---- state machine (the game's player states; panim.ts, tools/battle/LOCKSTEP.md) --------------------------------
  /** coarse state for the AI / skill code: 'idle' = SetAnim(0) (idle enter, random tick), others only relabel */
  setState(w: World, s: PState) {
    if (s === 'idle') return this.setAnimId(w, 0)
    if (this.state === 'skill' && s !== 'skill') this.endSkill(w)
    this.state = s
    this.t = 0
  }

  /** nframes of the shown animation (anim struct +0); a sprite without the animation has a null entry: 0 frames
   *  (one-shot states end on their first tick, emulator: Gohan form 9 / Krillin / Tien) */
  animN(): number {
    const a = this.sprite?.anim(this.ap.raw)
    return a ? Math.max(1, ...a.dirs.map((d) => d.length)) : 0
  }
  hasAnim(raw: number) {
    return !!this.sprite?.anim(raw)
  }
  /** the sprite shown = obj+0x50 / +0x11 */
  syncAnim() {
    const a = this.sprite?.anim(this.ap.raw) ?? this.sprite?.anim(1)
    this.anim = a
    this.frameIdx = this.sprite?.anim(this.ap.raw) ? this.ap.frame : 0
  }
  /** SetAnim(id) (vfunc +0x28 0x0800ECF0): the raw animation of the id + its state (0x0807273C; id 0 = idle) */
  setAnimId(w: World, id: number) {
    this.ap.raw = ANIM_RAW[id]
    this.enterState(w, id === 0 ? this.idleState() : ANIM_STATE[id])
  }
  /** the form controller's idle state: 0x03002B44, or 0x03001B0C (no control) for the cutscene variants */
  idleState(): string {
    return this.form.controller === 0x3001b64 ? 'idle2' : 'idle'
  }
  /** SetState (0x0800ED18): exit the old state object, enter the new one */
  enterState(w: World, gs: string) {
    const old = this.gs
    if (old === 'block') {
      // 0x08012BA2: blocking off, the shield object removed
      this.blocking = false
      this.shield = null
    } else if (old === 'hurt') this.setResist(128) // 0x08012D78
    else if (old === 'skill20' || old === 'ss_up') {
      // 0x0801C552: an unfinished transformation empties the gauge it was filling; the sound 54 stops
      if (!this.transformDone) {
        if (this.form.ss) this.ss3Gauge = 0
        else this.ssGauge = 0
      }
      this.sfxStop(54)
    }
    if (this.state === 'skill' && !gs.startsWith('skill')) this.endSkill(w)
    this.gs = gs
    this.t = 0
    this.shakeX = this.shakeY = 0
    const a = this.ap
    switch (gs) {
      case 'idle':
        // 0x08012FE6: tick = rand(255) (0x0800DFBE), state counter 0, hittable, resists 128
        a.tick = w.rng.rand(255, 'idle tick')
        a.frame = 0
        this.sCnt = 0
        this.hittable = true
        this.setResist(128)
        break
      case 'idle2':
        a.tick = w.rng.rand(255, 'idle tick')
        a.frame = 0
        break
      case 'hold':
        break
      // [H] candy states: enter t = 0 (0x0804EA21 / 0x0804E921 / 0x0804EAD5), the frame kept
      case 'candy':
      case 'candy_eat':
        a.tick = 0
        break
      case 'candy_walk':
        a.tick = 0
        this.candyCode = 0
        break
      case 'punch':
        // 0x0800DEE8: swing sound 50 + rand(2)
        a.tick = a.frame = 0
        this.sfx(50 + w.rng.rand(2, 'swing sfx'))
        break
      case 'block':
        // 0x08012B90
        a.tick = a.frame = 0
        this.blocking = true
        this.sCnt = 0
        this.shield = null
        break
      case 'hurt':
        // 0x08012D64: i-frames (both resists 0)
        a.tick = a.frame = 0
        this.setResist(0)
        break
      case 'skill20':
      case 'ss_up':
        // 0x0801C3EC: the transformation sound 54 (+ the aura effect object)
        a.tick = a.frame = 0
        this.transformDone = false
        this.sfx(54)
        break
      case 'ss_revert':
        // 0x0801C1F6: not hittable while reverting
        a.tick = a.frame = 0
        this.hittable = false
        break
      default:
        a.tick = a.frame = 0
    }
    this.state = this.coarse(gs)
    if (this.state === 'dying') {
      this.dead = true
      this.hittable = false
    }
  }
  private coarse(gs: string): PState {
    switch (gs) {
      case 'walk':
        return 'walk'
      case 'dash':
        return 'dash'
      case 'punch':
        return 'punch'
      case 'block':
        return 'block'
      case 'hurt':
        return 'hurt'
      case 'ko':
        return 'dying'
      case 'hold':
        return this.dead ? 'dying' : this.state === 'skill' ? 'skill' : 'idle'
      case 'ss_up':
      case 'ss_revert':
      case 'skill20':
      case 'skill18':
      case 'skill18r':
        return 'transform'
      default:
        return gs.startsWith('skill') ? 'skill' : 'idle'
    }
  }
  /** ---- mummy bandages (projectiles.ts MummyWrap): every live bandage's update runs 0x0802D8E6 - the form controller's
   *  stat reset (0x0800E074: resists 128, so the hurt state's i-frames end) and walk / dash -48 per stuck bandage;
   *  the shake-off is the bandages' own (0x08040284(input, d-pad, 8, 60)) */
  wrapStats(stuck: number) {
    this.resist = 128
    this.refreshStats()
    this.wraps = stuck
  }
  setResist(v: number) {
    if (this.resist === v) return
    this.resist = v
    this.refreshStats()
  }
  /** a PlaySfx call of the game (0x0803E378): id, volume 0..64 */
  sfx(id: number, vol = 64) {
    audioEvent('sfx', { id, vol })
  }
  sfxStop(id: number) {
    audioEvent('sfx_stop', { id })
  }

  update(w: World) {
    if (this.itSel) {
      // Instant Transmission target pick: the game's modal loop (only the cursor runs)
      this.itSelect(w)
      this.syncAnim()
      return
    }
    // [H] eaten (out of the world): nothing of the player runs
    if (this.gone) return
    if (this.flash > 0) this.flash--
    this.t++
    this.shieldTick()
    // objects updated before the player this frame: a gone beam's destructor sounds, the ghosts' own RNG draws
    for (const b of this.beamEnd) if (b.shot.removed) for (const [k, id] of b.sfx) k === 'stop' ? this.sfxStop(id) : this.sfx(id)
    this.beamEnd = this.beamEnd.filter((b) => !b.shot.removed)
    for (const g of this.ghostRng) if (g.at === w.frame) w.rng.rand(g.n, 'ghost')
    this.ghostRng = this.ghostRng.filter((g) => g.at > w.frame)
    // player update 0x08010BD2: form controller (regen, gauges), then the actor update 0x0800EF08: the state's
    // tick, then the control of the state that is current after the tick. The death script (KO + 199) blocks the
    // player update for a frame, then only the state tick runs. The story forms' controllers keep running at HP 0.
    const sinceKo = this.koAt >= 0 ? w.frame - this.koAt : -1
    if (sinceKo === 199) {
      // the death script's loop runs from here on: no other object is updated any more (no enemy AI / RNG draws
      // from KO + 199 to the end of the recording; lockstep f02_vs_enemy_t23 / t28 / t5a)
      w.modal = [this]
      // ... and every actor gets message 0xF (0x0800F44A: the current state's vfunc +0x10; the walk state's
      // 0x0800E050 is SetAnim(0), idle's does nothing; lockstep f02_vs_enemy_t13: rand(255) of a walking enemy)
      for (const e of this.ctx.enemies) if (!e.removed) (e as unknown as { stopMsg?: () => void }).stopMsg?.()
      return this.syncAnim()
    }
    if (this.defuse) return this.defuseStep(w)
    if (sinceKo < 199 && (!this.dead || this.storyForm())) this.passive(w)
    if (this.defuse) return this.syncAnim()
    this.animTick(w)
    if (!this.dead) this.control(w)
    this.syncAnim()
    // KO: the death script (0x08799D0C) starts the game-over song 8 200 frames later
    if (this.koAt >= 0 && w.frame - this.koAt === 200) audioEvent('song', { id: 8 })
    // the task list (screen shakes) runs after the objects; the player is the last object updated
    this.ctx.tickShakes(w)
    w.viewShake = this.ctx.shakeOff
  }

  /** the state's tick (vtable +4) */
  private animTick(w: World) {
    const a = this.ap
    const n = this.animN()
    switch (this.gs) {
      case 'idle':
      case 'idle2':
        return tickIdle(a, n)
      case 'walk':
      case 'loop_b':
      case 'loop_c':
        return tickLoop(a, n, 3)
      case 'dash':
        // 0x08012FC4: loop, the dash sound 25 (volume 32) every 16 ticks
        tickLoop(a, n, 3)
        if ((a.tick & 15) === 0) this.sfx(25, 32)
        return
      case 'bored':
        return tickBored(a, n)
      case 'punch':
        if (tickOnce(a, n, 2)) this.setAnimId(w, 0)
        return
      case 'once3':
      case 'hurt':
        if (tickOnce(a, n, 3)) this.setAnimId(w, 0)
        return
      case 'once4':
        if (tickOnce(a, n, 4)) this.setAnimId(w, 0)
        return
      case 'ko':
      case 'once_hold':
        // 0x0800DED0: then the hold state (last frame kept)
        if (tickOnce(a, n, 5)) this.enterState(w, 'hold')
        return
      case 'hold':
      case 'hold0':
      case 'block':
      case STONE:
        return
      // [H] candy idle / eat ticks do nothing; the hop tick 0x080284AB: height obj+0x176 = -sin(t) >> 13
      case 'candy':
      case 'candy_eat':
        return
      case 'candy_walk':
        this.z = -SIN256[a.tick & 0xff] >> 13
        return
      case 'skill20':
      case 'ss_up':
        if (tickOnce(a, n, 4)) this.finishTransform(w)
        return
      case 'ss_revert':
        if (tickOnce(a, n, 4)) this.finishRevert(w)
        return
      case 'skill18':
      case 'skill18r':
        if (tickOnce(a, n, 2)) this.finishSaiyaman(w)
        return
      case 'skill14':
        return
      default:
        return this.skillTick(w)
    }
  }

  /** tick of the skill states (vtable +4 of each skill state) */
  private skillTick(w: World) {
    switch (this.gs) {
      case 'skill7':
        return this.tickBlast(w)
      case 'skill2': {
        // Big Bang tick 0x0801A80E: frame (t >> 3) & 1 while the ball is held; after the release idle past tick 16
        const a = this.ap
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (!this.sk?.proj) {
          if (t > 16) this.setAnimId(w, 0)
        } else a.frame = (t >> 3) & 1
        return
      }
    }
  }
  /** Energy Blast tick 0x0801AF3C: frame 1 + the shot at tick 16; at tick 40 a queued B press with enough EP pays
   *  again, toggles frame 1 <-> 2, rewinds to tick 16 and fires; else SetAnim(0) */
  private tickBlast(w: World) {
    const a = this.ap
    a.tick = (a.tick + 1) & 0xffff
    if (a.tick < 40) {
      if (a.tick === 16) {
        a.frame = 1
        this.fireBlast(w)
      }
      return
    }
    const cost = this.skillCost(7)
    if (this.sk?.refire && this.canAfford(cost)) {
      this.sk.refire = false
      this.spend(w, cost)
      a.frame = 3 - a.frame
      a.tick = 16
      this.fireBlast(w)
    } else this.setAnimId(w, 0)
  }
  /** 0x0800FF44: the shot origin on the body box (0x0800FE78: def+0x10 + 16 * dir, at the integer position), centre c:
   *  down (cx, position y), up (cx, cy), left (x0, cy), right (x1, cy) - per form (each sprite has its own box) */
  handPoint(): [number, number] {
    const b = this.bodyBox()
    const cx = (b.x0 + b.x1) >> 1
    const cy = (b.y0 + b.y1) >> 1
    return this.facing === 0 ? [cx, this.py] : this.facing === 1 ? [cx, cy] : this.facing === 2 ? [b.x0, cy] : [b.x1, cy]
  }
  /** 0x08019858 -> ki blast 0x080197B4 at the muzzle (its constructor picks the sound 20 + rand(2)) */
  private fireBlast(w: World) {
    const [mx, my] = this.handPoint()
    this.blast = kiBlast(this.ctx, this, this.facing, 1, mx << 8, my << 8, undefined, undefined, w)
    // the player's blast (vtable 0x0804B454) tests a 16 x 16 box centred on its position (obj+0x28..+0x34, emulator)
    this.blast.o.box = [8, 8, 16, 16]
    w.add(this.blast)
  }

  /** the state's control (vtable +8) */
  private control(w: World) {
    if (this.gs === STONE) return this.ctlStone(w)
    if (this.state === 'held') return this.updHeld(w)
    switch (this.gs) {
      case 'idle':
        return this.ctlIdle(w)
      case 'walk':
        return this.ctlWalk(w)
      case 'dash':
        return this.ctlDash(w)
      case 'bored':
        return this.ctlBored(w)
      case 'block':
        return this.ctlBlock(w)
      case 'hurt':
        return this.ctlKnock(w)
      case 'skill14':
        return this.ctlIT(w)
      case 'skill18':
      case 'skill18r':
        return
      // [H] candy
      case 'candy':
        return this.ctlCandy(w)
      case 'candy_walk':
        return this.ctlCandyWalk(w)
      case 'candy_eat':
        return
    }
    if (this.state === 'skill') return this.updSkill(w)
    if (this.state === 'transform') return this.updTransform(w)
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

  /** story Trunks: form 24 (0x03001B88, update 0x0800E592) / 25 (0x03001B8C, 0x0800E5F4) */
  storyForm(): boolean {
    return this.form.controller === 0x3001b88 || this.form.controller === 0x3001b8c
  }
  /** the story form object: no regen, gauges or cooldown; countdown - 1, once negative SetAnim(23) (form 24: into
   *  form 25) / SetAnim(24) (form 25: back to 24) every frame the player is not already in that state (0x08010D26) -
   *  it interrupts any state, KO included */
  private storyTick(w: World) {
    if (--this.storyCnt >= 0) return
    if (this.form.controller === 0x3001b88) {
      if (this.gs !== 'ss_up') {
        this.transformTo = this.form.up
        this.setAnimId(w, 23)
      }
    } else if (this.gs !== 'ss_revert') this.setAnimId(w, 24)
  }
  /** the new form object at the end of a transformation / revert (0x0801064C, after SetAnim(0)): form 24's countdown
   *  120 + rand(180) (ctor 0x0804F034), form 25's 240 + rand(120) (0x0804F004); the fusion's 300 */
  private newFormObject(w: World) {
    if (this.form.controller === 0x3001b88) this.storyCnt = 120 + w.rng.rand(180, 'form 24 countdown')
    else if (this.form.controller === 0x3001b8c) this.storyCnt = 240 + w.rng.rand(120, 'form 25 countdown')
    else if (this.form.controller === 0x3001b64) this.fusCnt = 300
    if (this.dead && this.hp === 0) {
      // a story form's transformation ending after a KO: the player is idle again (HP 0, hittable, controllable)
      this.dead = false
    }
  }
  /** the cutscene fusions' defuse (script s014 0x0877D028, run synchronously by the form object 0x0800E638 when its
   *  countdown goes negative; t = frames since): only the state tick runs (not at t 90..94), t 85 the partner's
   *  record takes over (form exit, SetAnim(0) without a tick, the NPC's rand(255)), t 95..174 x -= 50 per frame,
   *  t 175 x = start - 16 px, t 295 the NPC's walk ends (rand(255)), t 297 the control runs again (the new form
   *  object from the next frame) */
  private defuseStep(w: World) {
    const d = this.defuse!
    d.t++
    if (d.t === 85) {
      this.swapToPartner(w)
      w.rng.rand(255, 'defuse npc')
    } else if (d.t < 90 || d.t > 94) this.animTick(w)
    if (d.t >= 95 && d.t <= 174) this.x -= 50
    if (d.t === 175) this.x = (d.x0 - 16) << 8
    if (d.t === 295) w.rng.rand(255, 'defuse npc walk')
    if (d.t >= 297) {
      this.defuse = null
      if (!this.dead) this.control(w)
    }
    this.syncAnim()
  }
  private swapToPartner(w: World) {
    this.formExit()
    const ch = this.form.char
    const pr = this.partner ?? { char: ch === 7 ? 0 : 3, form: ch === 7 ? 2 : 21, hp: this.hp, maxHp: this.maxHp, ep: this.ep, maxEp: this.maxEp, base: [this.rec.STR, this.rec.POW, this.rec.END, this.rec.SPD], skills: this.skills, slot: 0, levels: [], ssGauge: 0, ss3Gauge: 0, cooldown: 0 }
    const r = this.rec
    r.hp = pr.hp
    r.maxHp = pr.maxHp
    r.ep = pr.ep
    r.maxEp = pr.maxEp
    ;[r.STR, r.POW, r.END, r.SPD] = pr.base
    this.skills = pr.skills.length ? pr.skills : [7]
    this.slot = pr.slot
    this.skillLevel = pr.levels[0] ?? 0
    this.ssGauge = pr.ssGauge
    this.ss3Gauge = pr.ss3Gauge * 65536
    this.ssCooldown = pr.cooldown
    this.regenTimer = 0
    this.applyForm(pr.form)
    this.formEnter()
    this.ap.raw = ANIM_RAW[0]
    this.enterState(w, this.idleState())
    w.logEvent('info', `defused -> form ${pr.form}`)
  }

  private passive(w: World) {
    if (this.storyForm()) return this.storyTick(w)
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
    // gauges (form controllers 0x0800E3E6 SS / 0x0800E4C4 SS3; not while reverting): SS acc += 0xCCD, SS3 acc +=
    // 22*maxEP + 0x3333; each overflow takes acc >> 16 from the gauge; an empty gauge at an overflow -> revert
    // (SetAnim(24)). The SS cooldown (rec+4) falls only in the other forms (0x08013E06).
    // the story variants with the form objects 0x03001B7C / 0x03001B84 (Gohan 17, Vegeta 30, Gotenks 37 / 38, Goku 7
    // / 8) have no drain (emulator: the gauge never runs out)
    const drains = [0x3001b68, 0x3001b6c, 0x3001b78, 0x3001b80].includes(this.form.controller)
    if (this.gs !== 'ss_revert' && drains) {
      if (this.form.ss3) {
        this.ssAcc += 22 * this.maxEp + 0x3333
        const n = this.ssAcc >> 16
        if (n) {
          const g = this.ss3Gauge >> 16
          if (g > n) this.ss3Gauge = (g - n) * 65536
          else if (this.hp !== 0) {
            this.ss3Gauge = 0
            this.revert(w)
          }
          this.ssAcc &= 0xffff
        }
      } else if (this.form.ss) {
        this.ssAcc += 0xccd
        if (this.ssAcc >> 16) {
          if (this.ssGauge !== 0) this.ssGauge--
          else if (this.hp !== 0) this.revert(w)
          this.ssAcc &= 0xffff
        }
      }
    }
    if (!this.form.ss && !this.form.ss3 && this.ssCooldown > 0) this.ssCooldown = Math.max(0, this.ssCooldown - (this.eff.ssCooldownRate >> 6))
    // the cutscene fusion's form object (0x0800E638): after the regen its countdown; negative -> the defuse script
    if (this.form.controller === 0x3001b64 && --this.fusCnt < 0) {
      this.x = this.px << 8
      this.y = this.py << 8
      this.facing = 0
      this.defuse = { t: 0, x0: this.px }
    }
  }

  /** A / B at the end of the idle, walk, dash and bored controls */
  private attackInput(w: World) {
    const p = w.input.pressed
    if (p & BTN.A) return this.melee(w)
    if (p & BTN.B) return this.startSkill(w)
  }
  /** R held and the sprite has the guard animation (anim id 7 = raw 8): block state (0x0804C744) */
  private tryBlock(w: World): boolean {
    if (!(w.input.held & BTN.R) || !this.hasAnim(8)) return false
    this.ap.raw = 8
    this.enterState(w, 'block')
    return true
  }

  /** idle control 0x08013022 */
  private ctlIdle(w: World) {
    const { held, pressed } = w.input
    if (this.ctx.candy > 0) return
    if (pressed & BTN.L) this.cycleSkill()
    if (this.tryBlock(w)) return
    // after 1024 idle frames: the "bored" animation (anim id 8) if the sprite has it
    if (++this.sCnt > 1024) {
      if (this.hasAnim(9)) this.setAnimId(w, 8)
      else this.sCnt = 0
    }
    const nd = pressed & DPAD
    if (nd) {
      // double tap: the same single d-pad key again within 20 frames -> dash (the dash keeps the facing)
      if (w.frame - this.lastDirFrame < 20 && (nd & (nd - 1)) === 0 && this.lastDirKey & nd) return this.setAnimId(w, 2)
      this.lastDirKey = nd
      this.lastDirFrame = w.frame
    }
    if (held & DPAD) {
      if (pressed & BTN.LEFT) this.facing = 2
      if (pressed & BTN.RIGHT) this.facing = 3
      if (pressed & BTN.UP) this.facing = 1
      if (pressed & BTN.DOWN) this.facing = 0
      this.setAnimId(w, 1)
    }
    this.attackInput(w)
  }

  /** walk control 0x080131BA: horizontal step first (try-move flags 2), then vertical (flags 1) */
  private ctlWalk(w: World) {
    const { held, pressed } = w.input
    if (pressed & BTN.L) this.cycleSkill()
    if (this.tryBlock(w)) return
    if (held & DPAD) {
      const sp = this.walkSpeed()
      let combo = 0
      if (held & BTN.LEFT) {
        this.walkTry(w, -sp, 0, 2)
        combo = 1
      } else if (held & BTN.RIGHT) {
        this.walkTry(w, sp, 0, 2)
        combo = 2
      }
      if (held & BTN.UP) {
        this.walkTry(w, 0, -sp, 1)
        combo += 3
      } else if (held & BTN.DOWN) {
        this.walkTry(w, 0, sp, 1)
        combo += 6
      }
      this.facing = FACE_TBL[combo][this.facing] as Dir
    } else this.setAnimId(w, 0)
    if (this.ctx.candy > 0) return
    this.attackInput(w)
  }

  /** dash control 0x08012DEE: dash speed along the facing (whole-footprint test 0x080097EA); blocked -> the hurt
   *  state with the hurt animation (bump) and sound 59; else the other d-pad keys add the walk speed */
  private ctlDash(w: World) {
    const { held } = w.input
    const key = this.lastDirKey
    if ((held & key) !== key) this.setAnimId(w, 0)
    else {
      const [vx, vy] = DASH_VEC[this.facing]
      const sp = this.dashSpeed()
      const nx = this.x + vx * sp
      const ny = this.y + vy * sp
      if (!this.fits(w, nx >> 8, ny >> 8)) {
        this.ctx.shake(8, 100)
        this.ap.raw = 10
        this.enterState(w, 'hurt')
        this.sfx(59)
      } else {
        this.x = nx
        this.y = ny
        const ws = this.walkSpeed()
        let tx = nx
        let ty = ny
        if (held & BTN.UP && this.facing !== 1) ty -= ws
        if (held & BTN.DOWN && this.facing !== 0) ty += ws
        if (held & BTN.LEFT && this.facing !== 2) tx -= ws
        if (held & BTN.RIGHT && this.facing !== 3) tx += ws
        if (this.fits(w, tx >> 8, ty >> 8)) {
          this.x = tx
          this.y = ty
        }
      }
    }
    this.attackInput(w)
  }

  /** bored control 0x08012D8A */
  private ctlBored(w: World) {
    if (w.input.held & DPAD) this.setAnimId(w, 0)
    this.attackInput(w)
  }

  /** hurt / bump control 0x0800DF1C: knocked back 0.75 px per frame against the facing */
  private ctlKnock(w: World) {
    const [kx, ky] = KNOCK_VEC[this.facing]
    if (this.fits(w, (this.x - kx) >> 8, (this.y - ky) >> 8)) {
      this.x -= kx
      this.y -= ky
    }
  }

  /** block control 0x08012BC6 + the energy shield object 0x0801DFE0 (update 0x0801E080) */
  private ctlBlock(w: World) {
    const { held, pressed } = w.input
    const sh = this.shield
    if (!sh) {
      if (!(held & BTN.R)) return this.setAnimId(w, 0)
      if (pressed & BTN.B && this.energyLeft() !== 0) this.shield = { pow: 0, on: true }
      return
    }
    // EP: acc += rec+0x57 * 0x3277 >> 4, spend acc >> 16 every frame while the shield exists
    this.sCnt += (this.eff.epBlockFactor * 0x3277) >> 4
    this.spend(w, this.sCnt >> 16, true)
    this.sCnt &= 0xffff
    sh.on = this.energyLeft() !== 0 && !!(held & BTN.R) && !!(held & BTN.B)
    if (sh.pow === 0 && !sh.on) {
      if (held & BTN.R) this.shield = null
      else this.setAnimId(w, 0)
      return
    }
    // a full shield (power >= 24) makes both resists 0
    this.setResist(sh.pow >= 24 ? 0 : 128)
  }
  /** shield object update (the object runs after the player: it is added to the world later) */
  private shieldTick() {
    const sh = this.shield
    if (!sh) return
    if (sh.on) {
      if (++sh.pow >= 0x38) sh.pow = 0x18
    } else if (sh.pow >= 0x18) sh.pow = 0x17
    else if (sh.pow > 0) sh.pow--
  }

  // ---- melee -------------------------------------------------------------------------------------------------
  /** 0x08012A24: one instant hit test (mult 0x400, attack 0x086B8098) to every object; a hit plays 48 + rand(2);
   *  then (HP != 0) SetAnim(4 + rand(3)), whose punch state (shift 2, 16 frames) plays the swing sound */
  melee(w: World) {
    const rect = rectAt(PUNCH_RECT[this.facing], this.px, this.py)
    const msg: HitMsg = { team: 1, rect, dir: this.facing, mult: 0x400, atk: ATK[2], attacker: this, source: this }
    const any = broadcast(w, msg)
    if (any) {
      this.sfx(48 + w.rng.rand(2, 'hit sfx'))
      if (msg.crit) this.ctx.shake(15, 75)
    }
    if (this.hp === 0) return
    this.punchAnim = w.rng.rand(3, 'punch anim')
    this.setAnimId(w, 4 + this.punchAnim)
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
        // 0x0801A460: always (the skill is only in Gohan's list)
        return true
      case 14:
        // 0x0801A216: the skills flag, the global 0x03002C04 (not modelled) and 40 EP
        return this.canAfford(40)
      case 20:
        // 0x0801A4C4: a form with up == down cannot; SS3 cannot; SS can (reverts); else cooldown 0 and EP >= 10
        if (this.form.up === this.form.down) return false
        if (this.form.ss3) return false
        if (this.form.ss) return true
        return this.ssCooldown === 0 && this.canAfford(10)
      case 22:
        // 0x0801A55E: SS3 can (reverts); SS with a form above and 10 EP (of the SS gauge)
        if (this.form.ss3) return true
        return this.form.ss && this.form.up !== 255 && this.canAfford(10)
      case 2:
        // 0x08019D78: only one Big Bang ball at a time (byte 0x03003284 counts the live balls)
        if (this.bigBang && !this.bigBang.removed) return false
        return this.canAfford(SKILL_CHECK[2][Math.min(this.skillLv(2), 2)])
      default:
        if (SKILL_CHECK[id]) return this.canAfford(SKILL_CHECK[id][Math.min(this.skillLv(id), SKILL_CHECK[id].length - 1)])
        return this.canAfford(this.skillCost(id))
    }
  }
  bigBang: BigBangBall | null = null

  startSkill(w: World) {
    const id = this.skill
    if (!this.canUseSkill(id)) return
    audioEvent('skill_start', { id })
    const lv = this.skillLv(id)
    const mult = (SKILL_MULT[id] ?? [1024])[Math.min(lv, (SKILL_MULT[id] ?? [1024]).length - 1)]
    // costs paid on press (moves.json: "paid on press" / activation)
    if ([4, 5, 7, 8, 13, 17, 23].includes(id)) this.spend(w, this.skillCost(id))
    if (id === 3) this.spend(w, 2)
    if (id === 20) return this.form.ss ? this.revert(w) : this.startTransform(w, this.form.up)
    if (id === 22) return this.form.ss3 ? this.revert(w) : this.startTransform(w, this.form.up)
    if (id === 18) {
      // Saiyaman activate 0x0801A464: a form with a form above -> the change state (raw 20) into form 14, else the
      // way back (raw 12) into form 10 (both hardcoded); shift 2 one-shots, the end applies the form
      this.ap.raw = this.form.up !== 255 ? 20 : 12
      this.enterState(w, this.form.up !== 255 ? 'skill18' : 'skill18r')
      return
    }
    if (id === 14) return this.startIT(w, lv)
    this.enterState(w, 'skill' + id)
    this.state = 'skill'
    this.t = 0
    this.sk = { id, t: 0, held: true, release: -1, acc: 0, proj: null }
    if (SKILL_RAW[id]) {
      // the activate function's SetAnimRaw (0x0800ECDA) before the state's enter
      this.ap.raw = SKILL_RAW[id]
      this.syncAnim()
      this.epAcc = 0
      this.skillEnter(w, id, lv, mult)
      return
    }
    void mult
    const anim: Record<number, (string | number)[]> = {
      7: ['kiblast'], 4: ['kiblast'], 23: ['kiblast'], 2: ['special_charge', 'kiblast'], 17: ['special_charge', 'kiblast'], 9: ['special_charge', 'attack1'],
      8: ['attack2', 'attack1'], 5: ['special_charge', 'dash'], 13: ['special_charge', 'meditate'], 14: ['meditate', 'special_charge'],
    }
    this.setAnim(anim[id] ?? ['special_beam', 'kiblast'], true, false)
  }

  /** what each skill state's enter creates (the objects' constructors play the charge sound 3) */
  private skillEnter(w: World, id: number, lv: number, mult: number) {
    const s = this.sk!
    const atk = ATK[SKILL_SUB[id] ?? 1]
    switch (id) {
      case 15:
      case 16:
      case 6:
      case 1:
      case 3:
      case 10:
      case 11:
        // the beam object 0x080206B8 / Final Flash enter / Flame charge object: sound 3
        this.sfx(3)
        return
      case 2: {
        // Big Bang ball 0x08037480 (sound 3) at the hands (player + 0x0871DD9C[dir])
        const [bx, by] = BIGBANG_POS[this.facing]
        s.proj = this.bigBang = w.add(new BigBangBall(this.ctx, this, this.facing, (this.px + bx) << 8, (this.py + by) << 8, lv))
        this.sfx(3)
        return
      }
      case 17:
        // Masenko beam (0x08021280, sound 3) + aim objects
        s.proj = w.add(new MasenkoCursor(this.ctx, this, this.facing, mult, lv))
        this.sfx(3)
        return
      case 5:
        // Dragon Fist enter: sound 24
        this.sfx(24)
        return
      case 8:
        // Energy Punch enter: sound 47
        this.sfx(47)
        return
      case 9: {
        // Energy Sword enter: the sword object 0x08037C1C (charging) + sound 3
        const sk = s
        s.fx = w.add(new EnergySwordFx(this.ctx, this, () => this.sk === sk))
        this.sfx(3)
        return
      }
      case 4: {
        // Burning Attack ball (class 0x08029384) in the hands: no movement, no hit test until launched
        const [hx, hy] = BURNING_HOLD[this.facing]
        const [vx, vy] = VEL(this.facing, 768)
        s.proj = w.add(new StraightShot(this.ctx, this, this.facing, 1, (this.px + hx) << 8, (this.py + hy) << 8, { vx, vy, box: [7, 7, 14, 14], atk, mult, color: ['#fff6d0', '#e08020'], radius: 6, name: 'burning', delay: 1 << 30, fx: burningFx, fxEnd: burningEnd, rngHit: [2], sfxHit: 48 }))
        return
      }
    }
  }
  /** fired beams still on screen: when one is gone, the sounds its destructor plays (stop 32 / sound 33) */
  beamEnd: { shot: Entity; sfx: [string, number][] }[] = []
  /** RNG draws the Kamikaze ghosts make later on their own (ghost state change 24 frames after the spawn) */
  ghostRng: { at: number; n: number }[] = []

  private endSkill(w: World) {
    const s = this.sk
    if (!s) return
    // the state's exit: objects still charging are removed (their destructors stop the charge sound 3)
    switch (s.id) {
      case 15:
      case 16:
      case 6:
      case 1:
      case 3:
        // 0x0802082A: a beam still charging is removed (stop 3); a fired one retracts (beamEnd: stop 32 + 33)
        if (!s.proj) this.sfxStop(3)
        break
      case 2:
        // the ball still in the hands is removed (stop 3)
        if (s.proj && !(s.proj as BigBangBall).launched) this.sfxStop(3)
        break
      case 17:
        if (s.proj && (s.proj as MasenkoCursor).released < 0) this.sfxStop(3)
        break
      case 5:
        // 0x0801AB96: a dragon still out is cancelled: stop 32, then every segment explodes at once (head first)
        if (s.fx && !s.fx.removed && s.fx instanceof DragonFistFx) {
          this.sfxStop(32)
          s.fx.explode(w, true)
          s.fx = null
        }
        break
      case 9:
        // 0x0801B04A: the sword is removed: stop 3 while charging, 32 while swinging
        this.sfxStop(s.charged ? 32 : 3)
        break
      case 10:
        // 0x0801B4A4: the release path cleared the beam (it ends on its own: stop 32 8 frames later) -> stop 3;
        // interrupted while firing -> the beam is removed now
        if (s.proj && !s.proj.removed) {
          s.proj.removed = true
          this.beamEnd = this.beamEnd.filter((b) => b.shot !== s.proj)
          this.sfxStop(32)
          s.proj = null
        } else this.sfxStop(3)
        break
      case 11:
        // 0x0801B81A: the charge effect (stop 3) / the flame (stop 32) are removed at once
        if (s.fx) this.sfxStop(3)
        if (s.proj && !s.proj.removed) {
          s.proj.removed = true
          this.sfxStop(32)
          s.proj = null
        }
        break
      case 13: {
        // 0x0801B880: committed ghosts attack; the others vanish in a puff (rand(4096))
        const committed = s.charge ?? 0
        ;(s.ghosts ?? []).forEach((g, i) => {
          if (i < committed) {
            g.free = true
            g.wait = 0
          } else {
            g.removed = true
            w.rng.rand(4096, 'ghost puff')
          }
        })
        s.ghosts = []
        break
      }
    }
    audioEvent('skill_end')
    const p = s.proj
    if (p && !p.removed) {
      if (p instanceof GrowBeam || p instanceof KameBeam) p.stop()
      else if (p instanceof BigBangBall && !p.launched) p.removed = true
      else if (p instanceof MasenkoCursor && p.released < 0) p.removed = true
      else if (p instanceof StraightShot && p.o.delay) p.removed = true
    }
    for (const g of s.ghosts ?? []) if (!g.free) g.removed = true
    if (s.fx && !(s.fx instanceof DragonFistFx)) s.fx.removed = true
    this.sk = null
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
      case 7:
        // Energy Blast control 0x0801AFE2: a B press queues the next shot (fired by the tick, skillTick)
        if (w.input.pressed & BTN.B) s.refire = true
        return
      case 15:
      case 16:
      case 6:
      case 1:
      case 3: {
        // beam family control (Kamehameha 0x0801BE40, Kamekameha, Electric Kamehameha, Banshee 0x0801A6CC, Big Bang
        // Kamehameha): the beam object exists from the press (charge, sound 3); B is not read before tick 24; at 24
        // released -> idle (the charging beam is removed: stop 3), held -> the beam fires (stop 3, loop 32); from 24 on
        // acc += drain * factor >> 4, spend acc >> 8 every frame; EP 0 or B released -> idle (the beam retracts: stop
        // 32 + sound 33 when it is gone); frame ((tick >> 3) & 1) + 1 (Banshee without the + 1)
        const a = this.ap
        const bg = beamGfx(this.ctx, id, lv)
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t === 1 && bg && !s.fx) {
          // the beam object's charge state: the sparkle at the hands (BEAM_POS charge)
          const cp = BEAM_POS[id === 1 ? 'banshee' : 'kame'].charge
          s.fx = w.add(new BeamCharge(bg, () => [this.px + cp[this.facing][0], this.py + cp[this.facing][1]]))
        }
        if (t < 24) return
        if (t === 24) {
          if (!held) return end()
          if (s.fx) {
            s.fx.removed = true
            s.fx = null
          }
          this.sfxStop(3)
          this.sfx(32)
          // the beam object (projectiles.ts KameBeam) fires at the fire position; it was built at the press (tick 1)
          const [fx, fy] = BEAM_POS[id === 1 ? 'banshee' : id === 3 ? 'bbk' : 'kame'].fire[this.facing]
          const beam = w.add(new KameBeam(this.ctx, this, this.facing, this.px + fx, this.py + fy, w.frame - 23, { atk, mult, color: BEAM_COLOR[id] ?? ['#fff', '#3bf'], gfx: bg }))
          s.proj = beam
          this.beamEnd.push({ shot: beam, sfx: [['stop', 32], ['play', 33]] })
          if (id === 3) {
            const [fx, fy] = BBK_FLARE_POS[this.facing]
            w.add(new BbkFlareFx(this.ctx, this.px + fx, this.py + fy, this.facing, () => !beam.removed && !beam.done))
          }
        }
        const D = id === 1 || id === 3 ? 1174 : id === 6 ? [128, 427, 1707][lv] : [107, 384, 1280][lv]
        this.drain(w, D)
        if (this.energyLeft() === 0) return end()
        a.frame = ((t >> 3) & 1) + (id === 1 ? 0 : 1)
        if (!held) end()
        return
      }
      case 11: {
        // Flame Kamehameha control 0x0801B6B0: charge effect (sound 3) until tick 24; at 24 released -> idle, held ->
        // stop 3, loop 32 and the flame object (no drain that frame); then drain, EP 0 / released -> idle, and the
        // tick advances twice per frame: frame ((tick >> 3) & 1) + 1
        const a = this.ap
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t === 1 && !s.fx) {
          // Flame Kamehameha charge: record object fx_08721164 12 + f (f = t >> 3, > 3 -> 2 + (f & 1))
          const [cx, cy] = FLAME_CHARGE[this.facing]
          const sk = s
          s.fx = w.add(new RecObj(this.ctx.data.sprites, () => [this.px + cx, this.py + cy], (tt) => {
            const f = tt >> 3
            return 0x08721164 + 12 * (12 + (f > 3 ? 2 + (f & 1) : f))
          }, () => this.sk === sk && this.ap.tick < 24))
        }
        if (t < 24) return
        if (t === 24) {
          if (!held) return end()
          if (s.fx) {
            s.fx.removed = true
            s.fx = null
          }
          this.sfxStop(3)
          this.sfx(32)
          const sk = s
          s.proj = w.add(new FlameKameFx(this.ctx, this, this.facing, lv, () => this.sk === sk, { atk, mult })) as unknown as Shot
          return
        }
        this.drain(w, [107, 376, 1238][lv])
        if (this.energyLeft() === 0 || !held) return end()
        a.tick = (a.tick + 1) & 0xffff
        a.frame = ((a.tick >> 3) & 1) + 1
        return
      }
      case 10: {
        // Final Flash control 0x0801B4EC: charge (sound 3 from the enter) with frames (t >> 2) & 1 (2 from t 20);
        // released before 24 -> idle; at 24 the beam object (loop 32) + stop 3; then drain, EP 0 / released -> the
        // beam ends (stop 32 8 frames later) + idle (exit: stop 3); frame ((t >> 3) & 1) + 3
        const a = this.ap
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t < 24) {
          if (!held) return end()
          a.frame = t < 20 ? (t >> 2) & 1 : 2
          return
        }
        if (t === 24) {
          if (!held) return end()
          // the beam object (0x08036FA2) at player + 0x0871DF9C[dir] (FINAL_FLASH_POS), built now
          const [fx, fy] = FINAL_FLASH_POS[this.facing]
          this.sfx(32)
          s.proj = w.add(new FullBeam(this.ctx, this, this.facing, 1, this.px + fx, this.py + fy, { atk, mult, life: 1 << 30, table: FINAL_FLASH_TABLES[Math.min(2, lv)], visPos: FINAL_FLASH_POS, game: { built: w.frame } }))
          this.beamEnd.push({ shot: s.proj, sfx: [['stop', 32]] })
          this.sfxStop(3)
        }
        this.drain(w, [107, 363, 1174][lv])
        if (this.energyLeft() === 0 || !held) {
          const fb = s.proj as FullBeam | null
          if (fb) fb.life = fb.t
          s.proj = null
          return end()
        }
        a.frame = ((t >> 3) & 1) + 3
        return
      }
      case 2: {
        // Big Bang control 0x0801A83E (the tick 0x0801A80E sets the frame): while held, acc += rate[lv], the ball
        // takes acc >> 8 EP (unscaled) up to 1000; release -> launched (fires if the tick reached 16, else fizzles:
        // stop 3), tick 0; the tick fn goes idle 17 frames later
        const ball = s.proj as BigBangBall | null
        if (!ball) return
        if (!held) {
          if (this.ap.tick >= 16) {
            ball.launched = true
            this.sfxStop(3)
            this.sfx(32)
          } else {
            ball.removed = true
            this.sfxStop(3)
          }
          s.proj = null
          this.ap.tick = 0
          return
        }
        if (ball.charge256 >> 8 >= 1000) return
        s.acc += [43, 141, 427][lv]
        const d = s.acc >> 8
        ball.charge256 += d << 8
        this.spend(w, d, true)
        s.acc &= 0xff
        return
      }
      case 4: {
        // Burning Attack control 0x0801AAEC: the ball (class 0x08029384) waits in the hands from the press; at tick 16
        // it is launched from the ki-blast muzzle (sound 34), frame 1; idle at tick 32
        const a = this.ap
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t === 16) {
          const [mx, my] = this.handPoint()
          ;(s.proj as StraightShot | null)?.launch(mx << 8, my << 8)
          s.proj = null
          this.sfx(34)
          a.frame = 1
        } else if (t === 32) end()
        return
      }
      case 23: {
        // Ultra Volleyball control 0x0801C63E: frame t >> 4; the ball at tick 16 (its constructor plays 34); idle
        // after tick 32
        const a = this.ap
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t > 32) return end()
        a.frame = t >> 4
        if (t === 16) {
          const [ox, oy] = VOLLEY_POS[this.facing]
          w.add(new VolleyWave(this.ctx, this, this.facing, this.px + ox, this.py + oy))
          this.sfx(34)
        }
        return
      }
      case 8: {
        // Energy Punch control 0x0801B296: frames tick >> 2 until tick 8 (B not read); then every frame up to tick 59
        // while B is held: frame ((tick >> 2) & 1) + 2, one message to every object (x62 / x144 / x226 0x0871DF90,
        // attack sub 9) in the melee rect 0x0871DEF0 (Lv3: the larger rect 0x0871DF30) from the position before the
        // step, and from Lv2 a step of 0.375 px along the facing; tick 60 or released -> idle. The target accepts every
        // overlapping message (no per-target limit; lockstep f19_vs_skill8*)
        const a = this.ap
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t < 8) {
          a.frame = t >> 2
          return
        }
        if (t >= 60 || !held) return end()
        a.frame = ((t >> 2) & 1) + 2
        if (t === 8 && lv > 1) {
          // Lv3 only: record object 0x08762270 + 12 * (3 * dir + (tick >= 56 ? 1 : 0)) beside the player
          const sk = s
          s.fx = w.add(new RecObj(this.ctx.data.sprites, () => [this.px + ENERGY_PUNCH_POS[this.facing][0], this.py + ENERGY_PUNCH_POS[this.facing][1]], () => ENERGY_PUNCH_TABLE + 12 * (3 * this.facing + (this.ap.tick >= 56 ? 1 : 0)), () => this.sk === sk))
        }
        // one message to every object (0x08011C0C) per frame, no sound / RNG: the rect from the position before the step
        const r = (lv > 1 ? ENERGY_PUNCH_RECT_L3 : PUNCH_RECT)[this.facing]
        broadcast(w, { team: 1, rect: rectAt(r, this.px, this.py), dir: this.facing, mult, atk, attacker: this, source: this })
        if (lv > 0) {
          const [vx, vy] = DASH_VEC[this.facing]
          if (this.fits(w, (this.x + vx * 0x60) >> 8, (this.y + vy * 0x60) >> 8)) {
            this.x += vx * 0x60
            this.y += vy * 0x60
          }
        }
        return
      }
      case 9: {
        // Energy Sword control 0x0801B088. Phase 0: while B is held pay 4 (scaled) per frame (no EP check), tick++,
        // frame (tick >> 2) & 1; released with the sword charged (80 updates) -> phase 1 (stop 3, loop 32), else idle.
        // Phase 1: counter += 2 per frame, 90 frames; the d-pad steers at half the walk speed; frame
        // ((c / 3) & 7) + 2 picks the sword's strike box (0x08037C72, x10.0, first receiver only; a hit: 48 + rand(2))
        const a = this.ap
        if (!s.fx) {
          const sk = s
          s.fx = w.add(new EnergySwordFx(this.ctx, this, () => this.sk === sk))
        }
        if (!s.charged) {
          if (held) {
            this.spend(w, 4)
            a.tick = (a.tick + 1) & 0xffff
            a.frame = (a.tick >> 2) & 1
          } else if (s.t >= 80) {
            s.charged = true
            s.hits = 0
            this.sfxStop(3)
            this.sfx(32)
            ;(s.fx as EnergySwordFx).startSlash()
          } else end()
          return
        }
        const c = (s.hits = (s.hits ?? 0) + 1)
        if (c >= 180) return end()
        const { held: hk } = w.input
        if (hk & DPAD) {
          const hw = this.walkSpeed() >> 1
          if (hk & BTN.LEFT) this.walkTry(w, -hw, 0, 2)
          else if (hk & BTN.RIGHT) this.walkTry(w, hw, 0, 2)
          if (hk & BTN.UP) this.walkTry(w, 0, -hw, 1)
          else if (hk & BTN.DOWN) this.walkTry(w, 0, hw, 1)
        }
        s.hits = c + 1
        a.frame = (Math.trunc(c / 3) & 7) + 2
        const def = (this.facing === 3 ? [14, 13, 12, 11, 10, 17, 16, 15] : [10, 11, 12, 13, 14, 15, 16, 17])[a.frame - 2]
        const [ox, oy, bw, bh] = SWORD_BOX[def - 10]
        const rect = { x0: this.px - ox, y0: this.py - oy, x1: this.px - ox + bw, y1: this.py - oy + bh }
        // every swing frame: one message to the first receiver (0x08011C70); every accepted frame plays 48 + rand(2)
        if (broadcast(w, { team: 1, rect, dir: [2, 0, 0, 0, 3, 1, 1, 1][def - 10] as Dir, mult: 0x2800, atk, attacker: this, source: this }, true)) {
          this.sfx(48 + w.rng.rand(2, 'sword hit sfx'))
        }
        return
      }
      case 5: {
        // Dragon Fist control 0x0801ABBE (phase byte s.charged): before the dragon, tick++, released -> idle, tick 16 ->
        // the 6-segment dragon (loop 32), frame tick >> 3. Dragon out (its segments update and hit before this control,
        // skillfx.ts DragonFistFx): below tick 90 the player steps 0.375 px along the facing if the footprint fits and
        // the chain follows (0x08038E8A, v = 0x60 or 0 when blocked), 5 EP per frame (acc += factor * 80), and once B is
        // released / tick 300 / EP 0 the dragon is launched (0x08038E42: one x2.5 message with the head's box; stop 32,
        // sound 23); then 24 more ticks; frame ((tick >> 3) & 1) + 2
        const a = this.ap
        if (s.charged) {
          if (a.tick >= 24) return end()
          a.tick = (a.tick + 1) & 0xffff
          a.frame = ((a.tick >> 3) & 1) + 2
          return
        }
        if (!s.fx) {
          a.tick = (a.tick + 1) & 0xffff
          if (!held) return end()
          if (a.tick === 16) {
            s.fx = w.add(new DragonFistFx(this.ctx, this, this.facing))
            this.sfx(32)
          }
          a.frame = a.tick >> 3
          return
        }
        const df = s.fx as DragonFistFx
        if (a.tick < 90) {
          const [vx, vy] = DASH_VEC[this.facing]
          let v = 0
          if (this.fits(w, (this.x + vx * 0x60) >> 8, (this.y + vy * 0x60) >> 8)) {
            this.x += vx * 0x60
            this.y += vy * 0x60
            v = 0x60
          }
          df.shift(v)
        }
        s.acc += this.eff.epCostFactor * 80
        this.spend(w, s.acc >> 8, true)
        s.acc &= 0xff
        if (!(held && a.tick < 300 && this.energyLeft() !== 0)) {
          df.launch(w)
          s.fx = null
          s.charged = true
          a.tick = 0
          this.sfxStop(32)
          this.sfx(23)
        }
        a.tick = (a.tick + 1) & 0xffff
        a.frame = ((a.tick >> 3) & 1) + 2
        return
      }
      case 13: {
        // Super Kamikaze Ghost control 0x0801B916: frames t >> 3; a ghost at tick 16 (max 3: its constructor draws
        // rand(255)); from tick 40 the spawned ghosts are committed; B held -> frame 0 and, with < 3 ghosts and
        // 160 EP (paid), tick 0 for the next one; released -> idle (exit: the committed ghosts attack)
        const a = this.ap
        s.ghosts ??= []
        const t = (a.tick = (a.tick + 1) & 0xffff)
        if (t >= 40) {
          s.charge = Math.max(s.charge ?? 0, s.ghosts.length)
          if (held) {
            a.frame = 0
            if (s.ghosts.length < 3 && this.canAfford(160)) {
              this.spend(w, 160)
              a.tick = 0
            }
          } else end()
          return
        }
        a.frame = t >> 3
        if (t === 16) {
          const [gx, gy] = GHOST_SPAWN[this.facing]
          const g = new Ghost(this.ctx, this, this.facing, (this.px + gx) << 8, (this.py + gy) << 8, s.ghosts.length)
          g.init(w)
          s.ghosts.push(w.add(g))
        }
        return
      }
      case 17: {
        // Masenko control 0x0801C16A: the beam + aim objects wait from the press (sound 3); the B release (edge)
        // fires (tick 1, frame 1: stop 3, sound 34); idle 31 frames later. No hold limit, no drain.
        const a = this.ap
        const c = s.proj as MasenkoCursor | null
        if (a.tick === 0) {
          if (w.input.released & BTN.B) {
            a.tick = 1
            a.frame = 1
            this.sfxStop(3)
            this.sfx(34)
            c?.release()
          }
          return
        }
        a.tick = (a.tick + 1) & 0xffff
        if (a.tick >= 32) {
          s.proj = null
          end()
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
  /** Super Saiyan / SS3 activate (0x0801A512 / 0x0801A5BC): the transformation state (heap 0x0804E2B0, shift 4) with
   *  the transformation animation (raw 11); no EP is paid (canUse only checks EP >= 10) */
  private startTransform(w: World, to: number) {
    if (to === 255 || !this.ctx.data.forms[to]) return
    this.transformTo = to
    this.ap.raw = 11
    this.enterState(w, 'skill20')
    this.blocking = false
    w.logEvent('info', `transforming -> form ${to}`)
    const [ax, ay] = this.center()
    w.add(new LoopFx(this.ctx.fx(SS_AURA.id), ax, ay + 1, 0, SS_AURA.n, SS_AURA.rate, () => this.state === 'transform', 1))
  }
  /** transformation control 0x0801C4F4: the gauge fills to maxEP * tick / (16 * nframes) (the SS3 gauge when the
   *  current form is SS) */
  private updTransform(_w: World) {
    if (this.gs !== 'skill20' && this.gs !== 'ss_up') return
    const v = Math.floor((this.maxEp * this.ap.tick) / (this.animN() * 16 || 1)) & 0xffff
    if (this.form.ss) this.ss3Gauge = v * 65536
    else this.ssGauge = v
  }
  /** transformation end 0x0801C312: done; rec+4 (SS cooldown) = 0x37FFF; the form's exit (0x0801712C), form = up,
   *  the new form's enter (0x0801711C: the aura sound 54 for the SS controllers), its graphics, SetAnim(0) (whose
   *  exit of the transformation stops 54 again), then the new gauge full */
  private finishTransform(w: World) {
    this.transformDone = true
    this.ssCooldown = 0x37fff
    const to = this.form.up !== 255 ? this.form.up : this.transformTo
    this.formExit()
    this.applyForm(to)
    this.formEnter()
    this.setAnimId(w, 0)
    this.newFormObject(w)
    if (this.form.ss3) this.ss3Gauge = this.maxEp * 65536
    else if (this.form.ss) this.ssGauge = this.maxEp
    w.logEvent('info', `transformed -> form ${to}`)
  }
  transformDone = false
  /** the form objects (form +8) whose enter / exit play / stop the aura sound 54 (0x0800E3B2 / 0x0800E3CE) */
  private auraForm() {
    return [0x3001b68, 0x3001b6c, 0x3001b78, 0x3001b7c, 0x3001b80, 0x3001b84, 0x3001b8c].includes(this.form.controller)
  }
  private formEnter() {
    if (this.auraForm()) this.sfx(54)
  }
  private formExit() {
    if (this.auraForm()) this.sfxStop(54)
  }
  /** revert: SetAnim(24) (state 0x03002BB4, raw 12, shift 4, not hittable) whose end 0x0801C206 drops to the form
   *  below (gauges 0) - from Super Saiyan (skill 20 again / empty gauge) or SS3 */
  revert(w: World) {
    if (this.form.down === 255 || this.dead) return
    this.setAnimId(w, 24)
  }
  /** Saiyaman end: form 14 (from a form with a form above) or 10, hardcoded; no sound, gauge or cooldown change */
  private finishSaiyaman(w: World) {
    const to = this.gs === 'skill18' ? 14 : 10
    this.formExit()
    this.applyForm(to)
    this.formEnter()
    this.setAnimId(w, 0)
  }

  // ---- Instant Transmission ----------------------------------------------------------------------------------
  /** target pick (activate 0x0801A23A): the hold state with raw 20 frame 0, n = min(level + 1, 3 / 2 / 1 by EP 600 /
   *  120 / 40) picks among the enemies whose hurt box is on screen (max 16); then a modal loop (0x0803D8A0) freezes
   *  the world while B is held: a cursor from the body centre (v = v * 3/4, +-256 per d-pad direction, clamped to the
   *  camera) marks the candidate box it touches. Released: no pick -> idle, else the teleport state (0x0804E334). */
  itSel: { n: number; cand: { e: Actor; picked: boolean }[]; picks: Actor[]; x: number; y: number; vx: number; vy: number; cursor: boolean } | null = null
  it: { dir: Dir; idx: number; targets: Actor[]; origin: [number, number]; t: number } | null = null
  private startIT(w: World, lv: number) {
    this.ap.raw = 20
    this.ap.frame = 0
    this.enterState(w, 'hold')
    this.state = 'skill'
    const tier = this.canAfford(600) ? 3 : this.canAfford(120) ? 2 : this.canAfford(40) ? 1 : 0
    const cam = w.camera
    const cand = this.ctx
      .liveEnemies()
      .filter((e) => overlaps(e.bodyBox(), cam))
      .slice(0, 16)
      .map((e) => ({ e: e as Actor, picked: false }))
    const [cx, cy] = this.center()
    const sel = (this.itSel = { n: Math.min(lv + 1, tier), cand, picks: [] as Actor[], x: cx << 8, y: cy << 8, vx: 0, vy: 0, cursor: true })
    const fx = w.add(new ItMarkersFx(this.ctx, () => (sel.cursor ? [sel.x >> 8, sel.y >> 8] : null), () => sel.picks.map((e) => e.center()), () => this.itSel === sel || (!!this.it && this.it.targets === sel.picks)))
    w.modal = [this, fx]
  }
  private itSelect(w: World) {
    const s = this.itSel!
    if (!(w.input.held & BTN.B)) {
      w.modal = null
      this.itSel = null
      if (!s.picks.length) return this.setAnimId(w, 0)
      this.it = { dir: this.facing, idx: 0, targets: s.picks, origin: [this.px, this.py], t: -24 }
      this.hittable = false
      this.ap.raw = 20
      this.enterState(w, 'skill14')
      this.hittable = false
      return
    }
    if (s.n > 0 && s.cursor) {
      const px = s.x >> 8
      const py = s.y >> 8
      const c = s.cand.find((k) => {
        const b = k.e.bodyBox()
        return !k.picked && px >= b.x0 && px < b.x1 && py >= b.y0 && py < b.y1
      })
      if (c) {
        c.picked = true
        s.picks.push(c.e)
        if (--s.n === 0) s.cursor = false
      }
    }
    if (s.cursor) {
      const h = w.input.held
      s.vx = Math.trunc((s.vx * 3) / 4) + (h & BTN.RIGHT ? 256 : 0) - (h & BTN.LEFT ? 256 : 0)
      s.vy = Math.trunc((s.vy * 3) / 4) + (h & BTN.DOWN ? 256 : 0) - (h & BTN.UP ? 256 : 0)
      s.x += s.vx
      s.y += s.vy
      const cam = w.camera
      if (s.x < cam.x0 << 8 || s.x > cam.x1 << 8) {
        s.x = Math.max(cam.x0 << 8, Math.min(cam.x1 << 8, s.x))
        s.vx = 0
      }
      if (s.y < cam.y0 << 8 || s.y > cam.y1 << 8) {
        s.y = Math.max(cam.y0 << 8, Math.min(cam.y1 << 8, s.y))
        s.vy = 0
      }
    }
  }
  /** teleport sequence control (state 0x0804E334): per target 60 frames (raw 20: frames counting down, the jump
   *  under the target facing up, the punch x1.5 at t 24, the punch frames); EP 40 / 80 / 480 + sound 31 per jump */
  private ctlIT(w: World) {
    const it = this.it
    if (!it) return this.setAnimId(w, 0)
    this.hittable = false
    const a = this.ap
    const t = ++it.t
    if (t < 0) {
      if (t === -23 && it.targets.length > it.idx) {
        this.spend(w, [40, 80, 480][Math.min(2, it.idx)])
        this.sfx(31)
      }
      a.raw = 20
      a.frame = -t >> 3
      return
    }
    if (t === 0) {
      while (it.idx < it.targets.length && it.targets[it.idx].removed) it.idx++
      if (it.idx >= it.targets.length) {
        this.x = it.origin[0] << 8
        this.y = it.origin[1] << 8
        this.facing = it.dir
        return
      }
      const e = it.targets[it.idx]
      this.x = (((e.px * 2 + e.footW) >> 1) - 8) << 8
      this.y = (e.py + e.footH + 8) << 8
      this.facing = 1
      return
    }
    if (t < 24) {
      a.frame = (24 - t) >> 3
      return
    }
    if (it.idx >= it.targets.length) {
      this.it = null
      return this.setAnimId(w, 0)
    }
    if (t === 24) {
      const rect = rectAt(PUNCH_RECT[this.facing], this.px, this.py)
      if (broadcast(w, { team: 1, rect, dir: this.facing, mult: 0x600, atk: ATK[2], attacker: this, source: this })) this.sfx(48 + w.rng.rand(2, 'it hit sfx'))
    }
    a.frame = (t - 24) >> 2
    if (a.frame >= this.animN()) {
      a.frame = 0
      a.raw = 20
      it.idx++
      it.t = -24
    }
  }

  private finishRevert(w: World) {
    const down = this.form.down
    if (down === 255 || !this.ctx.data.forms[down]) return this.setAnimId(w, 0)
    if (this.form.ss || this.form.ss3) {
      this.ssGauge = 0
      this.ss3Gauge = 0
    }
    this.formExit()
    this.applyForm(down)
    this.formEnter()
    this.setAnimId(w, 0)
    this.newFormObject(w)
    w.logEvent('info', `reverted to form ${down}`)
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
        // ---- vampire grab: the held state 0x03001B00 does nothing itself; the escape count, the drain and the
        // vampire's position are the VampireGrab node's (ai.ts, update 0x08033A2A); the player stays in place
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
    // (the vampire is not pinned: VampireGrab's enter moves the vampire next to the player, 0x0803399E)
    if (by && (k === 'latch' || k === 'hug')) {
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
    if (msg.team === 1) return false
    if (this.state === 'held' && this.holdKind === 'eat') return false
    // [H] the eaten player is not in the world
    if (this.gone) return false
    if (!hitsBody(msg, this)) return false
    // KO: 0x08010F74 skips ComputeDamage (+0x1CC clear) but still returns 1 for a box overlapping the body - an enemy
    // energy wave hitting the KO'd player puffs and fades (lockstep f02_vs_enemy_t30)
    if (this.dead) return true
    // not hittable (+0x1CC: reverting, Instant Transmission, the cutscene fusions): the message is not accepted
    if (!this.hittable) return false
    // message 0 (0x08010F40) -> ComputeDamage (block: blocking flag and (hit dir ^ 1) == facing; a full energy shield
    // and the hurt state make both resists 0) -> the reaction 0x080105FA
    const front = (msg.dir ^ 1) === this.facing
    const blocking = this.blocking && front
    const res = computeDamage(msg.atk, msg.mult, msg.attacker.stats, this.stats, blocking, w.rng)
    const dmg = res.dmg
    msg.crit = res.crit
    msg.raw = res.raw
    msg.dmg = dmg
    // 0x08010BF2: hit flash (12 frames) + damage number; HP in 1/1024 with the remainder carried; HP 0 -> KO
    const before = this.hp
    const lost = dmg > 0 && this.hp > 0 ? this.hpAcc.take(dmg) : 0
    w.logEvent('damage', `${msg.attacker.kind === 'enemy' ? (msg.attacker as unknown as { label: string }).label : msg.attacker.kind} -> player: ${dmg}/1024${res.crit ? ' CRIT' : ''}${blocking ? ' blocked' : ''}${this.state === 'hurt' ? ' (i-frames)' : ''} = ${lost} HP`)
    // 0x0800E8E6 runs for every accepted hit, 0 damage included: the 12-frame flash restarts, a ceil(damage / 1024)
    // popup (a red 0 in the i-frames)
    this.flash = 12
    this.ctx.damagePopup(w, this, Math.ceil(dmg / 1024), res.crit, true, true)
    if (dmg === 0 || before === 0) return true
    w.stats.hitsTaken++
    if (lost > 0) {
      w.stats.damageTaken += Math.min(lost, this.hp)
      this.hp = Math.max(0, this.hp - lost)
      // thorns (player+0x1C4, record+0x5C): a melee attacker takes (damage * thorns) >> 8 (1/1024 HP), no reaction
      if (this.eff.thorns && msg.atk.kind === 0 && (msg.attacker as unknown as { takeDirect?: (w: World, n: number) => void }).takeDirect) {
        ;(msg.attacker as unknown as { takeDirect: (w: World, n: number) => void }).takeDirect(w, ((dmg * this.eff.thorns) >> 8) / 1024)
      }
      this.afterHpChange(w)
      if (this.dead) return true
    }
    // stone: the counter's reaction 0x0802E348 replaces the hurt state
    if (this.gs === STONE) {
      this.stoneHp -= dmg >> 10
      return true
    }
    if (this.state === 'held') return true
    if (msg.noReact) {
      // the flames skip the reaction; in the transformation singleton (SetAnim(23)) they call SetAnim(0)
      if (this.gs === 'ss_up') this.setAnimId(w, 0)
      return true
    }
    // 0x080107FC: blocked from the front -> pushed back 2 px; else turn to the attacker and the hurt state (raw 10)
    if (blocking) {
      const [bx, by] = BLOCK_PUSH[this.facing]
      if (this.fits(w, (this.x + bx) >> 8, (this.y + by) >> 8)) {
        this.x += bx
        this.y += by
      }
      return true
    }
    this.facing = (msg.dir ^ 1) as Dir
    this.hurtDir = msg.dir
    this.ap.raw = 10
    this.enterState(w, 'hurt')
    return true
  }
  /** KO (0x0801088A): state 'ko' (raw 18, shift 5, then hold); the death script plays song 8 200 frames later */
  private afterHpChange(w: World) {
    if (this.equip.some((e) => e?.id === 153)) this.refreshStats()
    if (this.hp <= 0 && !this.dead) {
      this.hp = 0
      this.heldBy = null
      this.holdKind = null
      this.alpha = 1
      // KO enter 0x08012D24: forms with flag 0x40 (the transformed ones) fall back to their base form (byte +0x17)
      // through 0x0801712C, which stops the transformation sound 54; the sprite stays
      if (this.form.flags & 0x40 && this.ctx.data.forms[this.form.down]) {
        this.sfxStop(54)
        this.form = this.ctx.data.forms[this.form.down]
      }
      this.ap.raw = 18
      this.enterState(w, 'ko')
      // the death script (started here) first runs on the next frame: the script runner comes before the object
      // updates, whichever object dealt the KO (lockstep f02_vs_enemy_t23 / t83 melee, t13 flames, t5e: song 8 at
      // KO + 201 ticks; the emulator's injected hits land before the script runner - buu_lockstep.ts injectHit)
      this.koAt = w.frame + 1
    }
  }
  koAt = -1

  draw(r: Renderer, w: World) {
    if (this.gone) return
    if (this.candyDef) {
      // [H] candy (beam hit 0x08028862): the player's def is 0x36 (sprite 45, three sweets), raw 1, the frame obj+0x11
      const sweets = this.ctx.data.sprites.index(45)
      this.drawAs(r, w, sweets, sweets?.anim(1), this.ap.frame)
      return
    }
    if (this.gs === STONE) {
      this.drawAs(r, w, this.sprite, this.sprite?.anim(22), this.ap.frame)
      return
    }
    if (this.holdKind === 'stone') {
      // petrified: player animation 0x15 (raw 22, the stone pose), frame 0 (1 = cracked once the d-pad counter is
      // below half; the sim does not count the presses)
      this.drawAs(r, w, this.sprite, this.sprite?.anim(22), 0)
      return
    }
    super.draw(r, w)
    if (this.shield) {
      // shield object 0x0801DFE0: record power >> 3 of 0x087202E8 at the body box centre (base draw 0x08019C44)
      const [cx, cy] = this.center()
      drawRec(r, this.ctx.fx(SHIELD_FX), this.shield.pow >> 3, cx, cy)
    }
    // bandages stuck on the player (0x0802D79C): record 0x08757218 at the player + (4, -slotY) (slots 0x08756BB4)
    for (let i = 0; i < Math.min(8, this.wraps); i++) drawRecAddr(r, this.ctx.data.sprites, 0x08757218, this.px + 4, this.py - WRAP_SLOT_Y[i])
  }

  /** draw with another sprite / animation / frame (candy, stone pose), keeping shadow and position */
  private drawAs(r: Renderer, w: World, sprite: Actor['sprite'], anim: Actor['anim'], frame: number) {
    const save = [this.sprite, this.anim, this.frameIdx] as const
    this.sprite = sprite
    this.anim = anim
    this.frameIdx = frame
    super.draw(r, w)
    ;[this.sprite, this.anim, this.frameIdx] = save
  }

  // ---- lockstep (tools/battle/LOCKSTEP.md) -------------------------------------------------------------------------
  /** copy the emulator's animation fields at frame 0 */
  lockstepInit(raw: number, frame: number, tick: number, gs = 'idle') {
    this.gs = gs === 'ss_up' || gs === 'ss_revert' ? gs : this.idleState()
    this.state = this.coarse(this.gs)
    if (this.gs === 'ss_up') {
      this.transformTo = this.form.up
      this.transformDone = false
    }
    this.ap = { raw, frame, tick }
    this.syncAnim()
  }
  /** what the lockstep compares: state name, raw animation (-1 when the sprite lacks it), frame, ROM frame record */
  lockstepView(): { state: string; raw: number; frame: number; rom: number; x?: number; y?: number; dir?: number } {
    // [H] eaten: the recorder finds no player object (state 'gone', everything 0)
    if (this.gone) return { state: 'gone', raw: 0, frame: 0, rom: 0, x: 0, y: 0, dir: 0 }
    // [H] candy: the def is 0x36 (sprite 45)
    const spr = this.candyDef ? this.ctx.data.sprites.index(45) : this.sprite
    const a = spr?.anim(this.ap.raw)
    const state = this.gs === 'idle2' ? 'idle' : this.gs
    if (!a) return { state, raw: -1, frame: this.ap.frame, rom: 0 }
    type J ={ raw_index: number; directions: Record<string, { rom?: string }[]> }
    const anims = spr!.json.animations as unknown as Record<string, J>
    const j = anims[a.name]
    const d = j.directions[['down', 'up', 'left', 'right'][this.facing]] ?? j.directions.any
    const s = d?.[this.ap.frame]
    // the emulator names an animation by the first def slot holding the same struct: slots sharing one struct
    // (e.g. Gohan's three punches) report the lowest raw index
    const key = (x: J) => JSON.stringify(Object.values(x.directions).map((v) => v.map((q) => q.rom)))
    const k = key(j)
    let raw = this.ap.raw
    for (const o of Object.values(anims)) if (o.raw_index < raw && key(o) === k) raw = o.raw_index
    return { state, raw, frame: this.ap.frame, rom: s?.rom ? parseInt(s.rom, 16) : 0 }
  }

  debugLabel() {
    return `${this.state} t${this.t}${this.sk ? ' skill ' + this.sk.id + ' t' + this.sk.t : ''}\nSTR${this.stats.STR} POW${this.stats.POW} END${this.stats.END}/${this.stats.END2}`
  }
}
