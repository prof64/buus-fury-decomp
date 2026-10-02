// LoG1 game state shared by every entity of a battle: the "char" structs (0x278 bytes, notes §1.4), the AI lists,
// global timers, the player controller fields, collision, and the generic enemy AI (AI tick 0x08008018, decision
// 0x0800755A, attack 0x0800F774, flee 0x0800F9AC, wander 0x0800F6A0, pathfinder 0x08006F1A + A* 0x08007A4E).
// Everything below is a line-by-line port of the disassembly (work/log1_dis.txt); quirks are kept on purpose
// and marked "QUIRK".

import type { Rect } from '../../core/geom'
import type { World } from '../../core/world'
import { applyDamage, damageCap, enemyMelee, type DamageResult } from './damage'
import type { CollisionMap, LevelRow, Log1Data, Template } from './data'
import { levelFromExp, SPEED_TABLE } from './data'
import type { Log1Rng, Log1Rng2 } from './rng'

export const TILE = 8
/** 0x5100 = 144^2: "near the screen centre" for waking and staying awake */
const NEAR_SCREEN2 = 0x5100

/** char state (+0x09) */
export const CS = { idle: 0, walk: 1, fly: 3, ki: 4, melee: 5, hurt: 6, dead: 7 } as const
/** char state -> entity animation/state number (table 0x086B6068) */
export const CS_ANIM = [0, 1, 6, 14, 5, 3, 0, 2]

/** Entity side of a char (player or enemy object) */
export interface CharEntity {
  /** set_state(entity, n) 0x08002466 (n = animation/state number) */
  setState(n: number): void
  /** current entity state number */
  entState(): number
  /** sprite has an animation for state n (set_char_state only switches when it exists) */
  hasState(n: number): boolean
  /** hit flash 0x0800BEB8 (8 frames) / death flash 0x0800BEEC */
  flash(frames: number): void
  /** death flash object -> Death state after 8 updates */
  startDeath(): void
  /** projectile currently charged by this entity (cancelled when the owner leaves its Ki state) */
  facing: number
}

export class Char {
  name: string
  team: 1 | 2
  tpl: Template | null
  /** +0x08 AI facing */
  face = 0
  /** +0x09 char state */
  state = 0
  /** +0x0A stun / cooldown */
  stun = 0
  /** +0x0B activity: 0 awake/deciding, 1 walking a path, 2 dormant, 3 roam-woken */
  act = 2
  /** +0x0C speed 1/16 px per frame */
  speed = 16
  /** +0x0E wake radius^2 (u16) */
  wakeR2 = 0
  /** +0x10 collision layer */
  layer = 0
  /** +0x12 roams */
  roams = 0
  /** +0x14 invulnerability */
  inv = 0
  /** +0x16 decision counter (A* expansion budget) */
  decCount = 0
  /** +0x18 clink sound mute until frame */
  clinkUntil = 0
  /** +0x1C flee/stuck counter */
  stuck = 0
  /** +0x1E aggro timer (u16) */
  aggro = 0
  /** +0x20/+0x22 position px; +0x24/+0x26 position 1/16 px */
  x = 0
  y = 0
  sx = 0
  sy = 0
  /** +0x28..+0x2B bbox offset/size */
  bbox = [0, 0, 16, 8]
  moveType = 0
  targetType = 0
  fightStyle = 0
  aggroTiles = 0
  hp = 1
  maxHp = 1
  ki = 0
  maxKi = 0
  kiRegen = 0
  defense = 0
  leashRadius = 0
  leashHome: [number, number] = [0, 0]
  guard: [number, number] = [0, 0]
  /** +0x78/+0x7C current tile */
  tx = 0
  ty = 0
  /** +0x80/+0x82 goal of the last path (template +0x2E/+0x30 = 0,0) */
  lastGoal: [number, number] = [0, 0]
  /** +0x84 exp (player) / exp reward (enemy) */
  exp = 0
  /** +0xA8 path index, +0xAC path length, +0xB0 waypoints (tiles) */
  pathIdx = 0
  pathLen = 0
  path: [number, number][] = []
  /** +0x240 patrol waypoints (init -1), +0x254 index */
  patrol: [number, number][] = [
    [-1, -1],
    [-1, -1],
    [-1, -1],
    [-1, -1],
    [-1, -1],
  ]
  patrolIdx = 0
  /** +0x258 target, +0x25C last attacker */
  target: Char | null = null
  lastAttacker: Char | null = null
  /** +0x260 assist list (filled by scripts; never in the simulator) */
  assist: (Char | null)[] = [null, null, null, null, null]
  alive = true
  ent: CharEntity | null = null
  constructor(name: string, team: 1 | 2, tpl: Template | null) {
    this.name = name
    this.team = team
    this.tpl = tpl
  }

  /** init_char 0x08006D90 enemy branch: template fields -> char */
  static fromTemplate(t: Template, x: number, y: number): Char {
    const c = new Char(t.name, 2, t)
    c.exp = t.exp
    c.moveType = t.moveType
    c.targetType = t.targetType
    c.fightStyle = t.fightStyle
    c.layer = t.flying ? 1 : 0
    c.aggroTiles = t.aggroTiles
    c.leashHome = [...t.leashHome]
    c.leashRadius = t.leashRadius
    c.guard = [...t.guard]
    c.speed = SPEED_TABLE[t.speedClass] ?? 16
    c.ki = c.maxKi = t.ki
    c.hp = c.maxHp = t.hp || 1
    c.defense = t.defense
    c.kiRegen = c.maxKi >> 6
    c.wakeR2 = (96 * t.aggroTiles * t.aggroTiles) & 0xffff
    c.roams = t.moveType >= 2 && t.moveType <= 5 ? 1 : 0
    c.setPos(x, y)
    return c
  }

  setPos(x: number, y: number) {
    this.x = x
    this.y = y
    this.sx = x << 4
    this.sy = y << 4
    this.tx = x >> 3
    this.ty = y >> 3
  }
  /** +0x2C bbox rect (enemy-enemy separation) */
  rect(): Rect {
    return { x0: this.x + this.bbox[0], y0: this.y + this.bbox[1], x1: this.x + this.bbox[0] + this.bbox[2], y1: this.y + this.bbox[1] + this.bbox[3] }
  }
  /** 0x0800F450: centre of the bbox */
  centre(): [number, number] {
    return [(this.x + (this.bbox[2] >>> 1)) << 16 >> 16, (this.y + (this.bbox[3] >>> 1)) << 16 >> 16]
  }
}

export interface Log1Options {
  /** keep the negative-damage heal (rand overflow) and the other decoded quirks */
  authenticBugs: boolean
  /** invincibility code UP,DOWN,LEFT,RIGHT,B,A (0x03000C18) */
  godmode: boolean
  /** level up from kill exp mid-battle (the game does) */
  exp: boolean
  popups: boolean
}

/** Per-battle game state ("globals" of the ROM). */
export class Log1Game {
  /** frame counter 0x03003B6C */
  fc = 0
  rng: Log1Rng
  rng2: Log1Rng2
  data: Log1Data
  opts: Log1Options
  /** awake list 0x030047F0 (player first) / dormant list 0x03003B54 */
  awake: Char[] = []
  dormant: Char[] = []
  player!: Char
  /** 0x03003B30 path/decision timer, 0x03003B34 Solar Flare freeze */
  pathTimer = 0
  solar = 0
  /** live player position (controller +0x14/+0x16 >> 4); the player char's x/y is the last seen position */
  px = 0
  py = 0
  /** player controller 0x030007AC */
  flying = false
  flight = 0
  maxFlight = 0
  takeoff: [number, number] = [0, 0]
  sfReady = 0
  selected = 0
  learned = 0b111
  /** 0x03000C1C per-hit cap for enemy attacks */
  cap = 1
  /** collision of the arena map + map bounds (0x03003B38 tiles, 0x03003B40 px) */
  col: CollisionMap | null
  mapTiles: [number, number]
  /** inventory (herbs, senzu) */
  herbs = 0
  senzu = 0
  levels: LevelRow[]
  world!: World
  /** event hooks for the battle (popups, stats) */
  onHit?: (att: Char, def: Char, res: DamageResult, raw: number, kind: 'melee' | 'ki') => void
  onKill?: (killer: Char, victim: Char) => void
  onLevelUp?: (level: number) => void
  log?: (kind: 'ai' | 'damage' | 'info', text: string) => void

  constructor(data: Log1Data, rng: Log1Rng, rng2: Log1Rng2, col: CollisionMap | null, mapTiles: [number, number], opts: Log1Options) {
    this.data = data
    this.rng = rng
    this.rng2 = rng2
    this.col = col
    this.mapTiles = mapTiles
    this.opts = opts
    this.levels = data.levels
  }

  // ---------------------------------------------------------------------------------------------- collision
  /** 0x08004FFA(map, rect, layer): every tile under the rect passable (rect x1/y1 rounded up) */
  passable(r: Rect, layer: number): boolean {
    const tx0 = r.x0 >> 3
    const tx1 = (r.x1 + 7) >> 3
    const ty0 = r.y0 >> 3
    const ty1 = (r.y1 + 7) >> 3
    const [w, h] = this.mapTiles
    if (tx0 < 0 || ty0 < 0 || w < tx1 || h < ty1) return false
    const col = this.col
    if (!col) return true
    const bits = col.layers[layer]
    if (!bits) return true
    for (let y = ty0; y < ty1; y++)
      for (let x = tx0; x < tx1; x++) {
        const i = y * col.w + x
        if (!((bits[i >> 3] >> (i & 7)) & 1)) return false
      }
    return true
  }
  /** 0x0800507A(map, rect, value, layer): mark the tiles under a rect passable (true) or blocked (false) */
  setPassable(r: Rect, layer: number, value: boolean) {
    const col = this.col
    if (!col) return
    const bits = col.layers[layer]
    for (let y = Math.max(0, r.y0 >> 3); y < Math.min(col.h, (r.y1 + 7) >> 3); y++)
      for (let x = Math.max(0, r.x0 >> 3); x < Math.min(col.w, (r.x1 + 7) >> 3); x++) {
        const i = y * col.w + x
        if (value) bits[i >> 3] |= 1 << (i & 7)
        else bits[i >> 3] &= ~(1 << (i & 7))
      }
  }
  /** char bbox placed at tile (tx,ty) (F404 <<3, F420), tested on `layer` */
  tileFree(c: Char, tx: number, ty: number, layer: number): boolean {
    const x = (tx << 3) << 16 >> 16
    const y = (ty << 3) << 16 >> 16
    return this.passable({ x0: x + c.bbox[0], y0: y + c.bbox[1], x1: x + c.bbox[0] + c.bbox[2], y1: y + c.bbox[1] + c.bbox[3] }, layer)
  }

  // ---------------------------------------------------------------------------------------------- lists
  private remove(list: Char[], c: Char) {
    const i = list.indexOf(c)
    if (i >= 0) list.splice(i, 1)
  }
  addEnemy(c: Char) {
    c.act = 2
    this.dormant.push(c)
  }

  // ---------------------------------------------------------------------------------------------- set_char_state
  /** 0x08007F4C set_char_state(char, s) */
  setCharState(c: Char, s: number) {
    if (c.state !== s) {
      if (c.state > 3) c.stun = 45
      c.state = s
    } else if (s !== 6) return
    if (s === CS.dead && c.team === 2) {
      this.remove(this.awake, c)
      this.remove(this.dormant, c)
    }
    const ent = c.ent
    if (s === CS.hurt) {
      ent?.flash(8)
      // set_state(ent, 0): the Standing enter (0x08005DE0 for enemies) resets the char state
      ent?.setState(0)
      if (c.state !== 0) {
        if (c.state > 3) c.stun = 45
        c.state = 0
      }
      if (c.team === 2) {
        c.stun = 20
        c.inv = 80
      }
      return
    }
    if (s === CS.dead) {
      ent?.startDeath()
      return
    }
    const anim = CS_ANIM[s]
    if (ent && ent.hasState(anim)) ent.setState(anim)
  }

  // ---------------------------------------------------------------------------------------------- hit()
  /** 0x08007416 hit(attacker, defender, flag (0 melee / 1 projectile), raw) */
  hit(att: Char, def: Char, flag: 0 | 1, raw: number) {
    if (def.inv !== 0) {
      this.clink(def)
      def.ent?.flash(8)
      return
    }
    if (!att.alive) {
      att.target = null
      if (def.lastAttacker === att) def.lastAttacker = null
      return
    }
    if (!def.alive) {
      def.target = null
      if (att.lastAttacker === def) att.lastAttacker = null
      return
    }
    if (att.team === 2) {
      if (flag === 0) this.setCharState(att, CS.melee)
      att.pathIdx = 0 // QUIRK: the attacker's path restarts from its first waypoint
      if (raw > this.cap) raw = this.cap
    } else {
      def.aggro = (900 + this.rng.randInc(900, 'aggro timer')) & 0xffff
    }
    att.target = def
    def.lastAttacker = att
    const res = applyDamage(this.rng, raw, def.defense, def.hp, { team: def.team, stun: def.stun, activity: def.act, godmode: this.opts.godmode })
    if (res.result === 2) {
      def.hp = res.hp
      this.onHit?.(att, def, res, raw, flag ? 'ki' : 'melee')
      this.setCharState(def, CS.hurt)
    } else if (res.result === 1) {
      def.hp = 0
      def.alive = false
      this.onHit?.(att, def, res, raw, flag ? 'ki' : 'melee')
      att.target = null
      if (att.lastAttacker === def) att.lastAttacker = null
      def.lastAttacker = null
      def.target = null
      this.setCharState(def, CS.dead)
      this.onKill?.(att, def)
      if (att.team === 1 && this.opts.exp) {
        att.exp += def.exp
        this.refreshPlayerStats()
      }
    } else {
      this.onHit?.(att, def, res, raw, flag ? 'ki' : 'melee')
      this.clink(def)
      def.ent?.flash(8)
    }
  }
  private clink(def: Char) {
    if (def.clinkUntil < this.fc) def.clinkUntil = this.fc + 20
  }

  /** 0x08005BFE: level stats from exp. Ki is not refilled; HP gains the max-HP delta; cap = maxHP/3+1. */
  refreshPlayerStats(): boolean {
    const p = this.player
    const idx = levelFromExp(this.levels, p.exp)
    const row = this.levels[idx]
    const oldMax = p.maxHp
    p.maxKi = row.maxKi
    p.maxHp = row.maxHp
    p.hp += p.maxHp - oldMax
    p.kiRegen = p.maxKi >> 6
    this.cap = damageCap(p.maxHp)
    this.maxFlight = row.maxFlight
    if (p.maxHp > oldMax) {
      this.onLevelUp?.(idx + 1)
      return true
    }
    return false
  }
  get level(): number {
    return levelFromExp(this.levels, this.player.exp) + 1
  }

  /** 0x080087A0 Solar Flare: global AI freeze; stuns longer than it are shortened, ground enemies go idle */
  freeze(d: number) {
    this.solar = d
    for (const c of [...this.awake.slice(1), ...this.dormant]) {
      if (c.stun !== 0 && c.stun > d) c.stun -= d
      if (c.state !== CS.fly) this.setCharState(c, CS.idle)
    }
  }

  /** 0x08002134 camera follow (called by the player's movement while the d-pad is held): the camera centre moves
   *  toward the centre of Goku's sprite with a 16 px dead zone and at most 8 px per frame and axis. */
  followCamera(sprite: Rect) {
    const cam = this.world.camera
    const ccx = (cam.x0 + cam.x1) >> 1
    const ccy = (cam.y0 + cam.y1) >> 1
    const step = (d: number) => {
      if (d > 0) {
        d -= 16
        return d > 8 ? 8 : d < 0 ? 0 : d
      }
      if (d < 0) {
        d += 16
        return d < -8 ? -8 : d > 0 ? 0 : d
      }
      return 0
    }
    const dx = step(((sprite.x0 + sprite.x1) >> 1) - ccx)
    const dy = step(((sprite.y0 + sprite.y1) >> 1) - ccy)
    this.setCamera(ccx + dx - 120, ccy + dy - 80)
  }
  setCamera(x0: number, y0: number) {
    const mw = this.mapTiles[0] * 8
    const mh = this.mapTiles[1] * 8
    if (x0 < 0) x0 = 0
    if (y0 < 0) y0 = 0
    if (x0 > mw - 240) x0 = mw - 240
    if (y0 > mh - 160) y0 = mh - 160
    this.world.camera = { x0, y0, x1: x0 + 240, y1: y0 + 160 }
  }

  // ---------------------------------------------------------------------------------------------- AI tick
  /** 0x08008018: once per frame before the entities update */
  aiTick() {
    const p = this.player
    if (!this.flying && (this.fc & 15) === 0 && p.ki < p.maxKi) {
      p.ki += p.kiRegen
      if (p.ki > p.maxKi) p.ki = p.maxKi
    }
    if (this.solar !== 0) {
      this.solar--
      return
    }
    // player moved to another tile / path timer. The player char's +0x20 (p.x/p.y) is only refreshed here.
    let flag: boolean
    const ptx = this.px >> 3
    const pty = this.py >> 3
    if (p.x >> 3 !== ptx || p.y >> 3 !== pty) {
      p.x = this.px
      p.y = this.py
      p.tx = ptx
      p.ty = pty
      flag = true
    } else {
      flag = this.pathTimer === 0
      if (this.pathTimer === 0) this.pathTimer = this.rng.randInc(60, 'path timer') + 30
      else this.pathTimer--
    }
    const cam = this.world.camera
    const scx = (cam.x0 + cam.x1) >> 1
    const scy = (cam.y0 + cam.y1) >> 1
    const d2 = (ax: number, ay: number, bx: number, by: number) => {
      const dx = (ax - bx) << 16 >> 16
      const dy = (ay - by) << 16 >> 16
      return dx * dx + dy * dy
    }
    // dormant list: wake by proximity (or running aggro timer) near the screen, or roam-wake
    if (flag) {
      for (const c of this.dormant.slice()) {
        if (d2(this.px, this.py, c.x, c.y) < c.wakeR2 || c.aggro !== 0) {
          if (d2(scx, scy, c.x, c.y) <= NEAR_SCREEN2) this.wake(c, 1)
        } else if (c.roams && this.awake.length < 4 && d2(scx, scy, c.x, c.y) < NEAR_SCREEN2) {
          if (this.rng.randInc(0xffff, 'roam wake') < 200) this.wake(c, 3)
        }
      }
    }
    // awake list
    for (const c of this.awake.slice(1)) {
      if (!this.awake.includes(c)) continue
      if (c.aggro !== 0) {
        c.aggro = (c.aggro - 1) & 0xffff
        if (c.aggro !== 0 || c.targetType === 1) {
          if (this.rng.randInc(10000, 'forget target') > 9800) {
            c.target = null
            c.lastAttacker = null
          }
        } else {
          c.target = null
          c.lastAttacker = null
        }
      }
      if (c.inv !== 0) c.inv--
      if (c.stun !== 0) {
        c.stun--
        if (c.stun === 0) {
          const done = c.pathIdx >= c.pathLen
          const aligned = ((c.x | c.y) & 7) === 0
          if (!(done && aligned)) this.setCharState(c, c.layer === 0 ? CS.walk : CS.fly)
        }
        c.act = 0
        continue
      }
      if (c.state > 3) continue
      if (flag && c.act !== 3) {
        const near = d2(this.px, this.py, c.x, c.y) <= c.wakeR2 || c.aggro !== 0
        if (!near || d2(scx, scy, c.x, c.y) > NEAR_SCREEN2) {
          c.act = 2
          if (c.state < 3) this.setCharState(c, CS.idle)
          else c.face = 0
          this.remove(this.awake, c)
          this.dormant.push(c)
          continue
        }
      }
      if ((this.fc & 15) === 0 && c.ki < c.maxKi) {
        c.ki += c.kiRegen
        if (c.ki > c.maxKi) c.ki = c.maxKi
      }
      if (flag || c.pathIdx >= c.pathLen) {
        c.act = 0
        c.decCount = 0
        this.decide(c)
        if (!c.alive) continue
        if (c.pathIdx < c.pathLen) {
          if (c.state === 0) this.setCharState(c, c.layer === 0 ? CS.walk : CS.fly)
        } else {
          if (c.state <= 3) this.setCharState(c, CS.idle)
          continue
        }
      }
      if (c.state !== CS.walk && c.state !== CS.fly) continue
      this.followPath(c)
    }
  }

  private wake(c: Char, act: number) {
    c.act = act
    this.remove(this.dormant, c)
    this.awake.push(c)
  }

  /** 0x080083C4..0x08008588: walk toward the current waypoint, x first, then separate overlapping enemies */
  private followPath(c: Char) {
    let wp = c.path[c.pathIdx]
    let wx = wp[0] << 3
    let wy = wp[1] << 3
    if (c.x === wx && c.y === wy) {
      if (c.act !== 2) {
        c.tx = wx >> 3
        c.ty = wy >> 3
        c.pathIdx++
      }
      if (c.pathIdx >= c.pathLen) {
        c.act = 0
        return
      }
      wp = c.path[c.pathIdx]
      wx = wp[0] << 3
      wy = wp[1] << 3
      c.act = 1
    }
    if (c.x === wx) {
      if (c.y < wy) {
        c.sy = (c.sy + c.speed) << 16 >> 16
        if (c.sy >> 4 > wy) c.sy = (wy << 4) << 16 >> 16
        c.face = 0
      } else {
        c.sy = (c.sy - c.speed) << 16 >> 16
        if (c.sy >> 4 < wy) c.sy = (wy << 4) << 16 >> 16
        c.face = 1
      }
      c.x = c.sx >> 4
      c.y = c.sy >> 4
      c.ty = c.y >> 3
    } else {
      if (c.x < wx) {
        c.sx = (c.sx + c.speed) << 16 >> 16
        if (c.sx >> 4 > wx) c.sx = (wx << 4) << 16 >> 16
        c.face = 3
      } else {
        c.sx = (c.sx - c.speed) << 16 >> 16
        if (c.sx >> 4 < wx) c.sx = (wx << 4) << 16 >> 16
        c.face = 2
      }
      c.x = c.sx >> 4
      c.y = c.sy >> 4
      c.tx = c.x >> 3
    }
    // separation (0x08008512): QUIRK - the ROM compares the FIRST awake enemy with this enemy and every enemy after
    // it in the list (the loop advances the wrong register); the slower of an overlapping pair stops for 8 frames.
    if (c.state < 3) {
      const first = this.awake[1]
      if (!first) return
      for (let i = this.awake.indexOf(c); i >= 0 && i < this.awake.length; i++) {
        const o = this.awake[i]
        if (o === first) continue
        const r = first.rect()
        const q = o.rect()
        if (r.x0 < q.x1 && q.x0 < r.x1 && r.y0 < q.y1 && q.y0 < r.y1 && first.state !== 0) {
          if (first.speed > o.speed) {
            this.setCharState(o, CS.idle)
            o.stun = 8
            return
          }
          this.setCharState(first, CS.idle)
          first.stun = 8
        }
      }
    }
  }

  // ---------------------------------------------------------------------------------------------- decision
  /** Manhattan tile distance 0x0800F570 */
  static manh(ax: number, ay: number, bx: number, by: number) {
    return Math.abs(ax - bx) + Math.abs(ay - by)
  }

  /** 0x0800755A */
  decide(c: Char) {
    const ex = c.tx
    const ey = c.ty
    const t = c.lastAttacker ?? c.target
    if (t) {
      if (c.targetType === 4) return this.flee(c, t)
      const d = Log1Game.manh(ex, ey, t.tx, t.ty)
      switch (c.fightStyle) {
        case 3:
          if (c.maxHp >> 1 > c.hp && this.rng.randInc(100, 'flee roll') <= 5) return this.flee(c, t)
          return this.attack(c, t, d)
        case 1:
          if ((t.hp >> 3) + t.hp >= t.maxHp) return this.flee(c, t)
          return this.attack(c, t, d)
        case 2:
          if (c.hp > t.hp) return this.attack(c, t, d)
          return this.flee(c, t)
        default:
          return this.attack(c, t, d)
      }
    }
    const p = this.player
    const tt = c.targetType
    const target = c.aggroTiles >= Log1Game.manh(ex, ey, p.tx, p.ty) ? p : null
    if (target) {
      const d = Log1Game.manh(ex, ey, target.tx, target.ty)
      if (tt === 1) return this.attack(c, target, d)
      if (tt === 2) {
        // assist list (+0x260, filled by scripts): never contains the player here
        for (let i = 0; i < 5; i++) if (c.assist[i] === target) return this.attack(c, target, d)
        return this.flee(c, target)
      }
      if (tt === 3) {
        if (target.lastAttacker) return this.attack(c, target, d)
        return this.flee(c, target)
      }
      return this.flee(c, target)
    }
    if (c.pathIdx < c.pathLen - 1) return
    switch (c.moveType) {
      case 2:
        // walk back to the guard tile (templates use 0,0 -> lastGoal (0,0) makes the pathfinder a no-op)
        if (c.guard[0] !== ex || c.guard[1] !== ey) this.findPath(c, c.guard[0], c.guard[1], 0)
        return
      case 3: {
        // patrol waypoints (+0x240) are only set by scripts; with the initial (-1, ..) entries this is a wander
        const w = c.patrol[c.patrolIdx]
        if (w[0] >= 0 && w[1] >= 0 && this.findPath(c, w[0], w[1], 0)) return
        return this.wander(c, ex, ey)
      }
      case 4:
        return this.wander(c, ex, ey)
      case 5:
        if (Log1Game.manh(ex, ey, c.leashHome[0], c.leashHome[1]) >= c.leashRadius && this.findPath(c, c.leashHome[0], c.leashHome[1], 0)) return
        return this.wander(c, ex, ey)
      default:
        return
    }
  }

  /** 0x0800F774: melee when adjacent (Manhattan <= 1), else maybe a ki blast, else path toward the target */
  attack(c: Char, t: Char, d: number) {
    if (d <= 1) {
      if (this.flying) return
      c.face = this.faceToward(c, t)
      const raw = enemyMelee(this.rng, c.hp, this.opts.authenticBugs)
      this.hit(c, t, 0, raw)
      return
    }
    const [cx, cy] = c.centre()
    if (c.ki > c.maxKi >> 4) {
      const w = t.bbox[2]
      const h = t.bbox[3]
      const inX = t.x - (w >> 1) < cx && t.x + w + (w >> 1) > cx
      const inY = t.y - (h >> 1) < cy && t.y + h + (h >> 1) > cy
      if (inX || inY) {
        if (this.rng.randInc(100, 'ki blast roll') <= 1 && !this.flying) {
          if (c.state === CS.ki) return
          if (c.state > 3) c.stun = 45
          c.state = CS.ki
          c.face = this.faceToward(c, t)
          const power = (this.rng.randInc(c.ki >> 1, 'ki blast power', !this.opts.authenticBugs) + (c.ki >> 2)) | 0
          this.fireEnemyKi?.(c, power)
          return
        }
      }
    }
    // path toward the target
    if (c.stuck >= 15) {
      this.wander(c, c.tx, c.ty)
      c.stuck = 0
      return
    }
    if (!this.findPath(c, t.x >> 3, t.y >> 3, 0)) c.stuck = (c.stuck + 1) & 0xffff
    else c.stuck = 0
  }
  fireEnemyKi?: (c: Char, power: number) => void

  /** facing toward the target by centres (dominant axis, ties horizontal) */
  private faceToward(c: Char, t: Char): number {
    const [ax, ay] = c.centre()
    const [bx, by] = t.centre()
    const dx = ax - bx
    const dy = ay - by
    if (dx * dx < dy * dy) return ay >= by ? 1 : 0
    return ax < bx ? 3 : 2
  }

  /** 0x0800F6A0: random tile within min(aggro,14) around (ex,ey) */
  wander(c: Char, ex: number, ey: number) {
    if (c.pathIdx < c.pathLen - 1) return
    let r = c.aggroTiles
    if (r > 14) r = 14
    if (r <= 0) return
    const a = this.rng.randInc(r, 'wander x')
    const b = this.rng.randInc(r, 'wander y')
    let gx = (r >> 1) - a + ex
    let gy = (r >> 1) - b + ey
    const [w, h] = this.mapTiles
    if (gx < 0) gx = 0
    if (gy < 0) gy = 0
    if (gx > w) gx = w
    if (gy > h) gy = h
    if (this.tileFree(c, gx, gy, 0)) this.findPath(c, gx, gy, 0)
  }

  /** 0x0800F9AC */
  flee(c: Char, threat: Char) {
    const ex = c.tx
    const ey = c.ty
    const dx = threat.tx - ex
    const dy = threat.ty - ey
    const dx2 = dx * dx
    const dy2 = dy * dy
    const sx = threat.tx > ex ? -1 : 1
    const sy = threat.ty > ey ? -1 : 1
    const a = c.aggroTiles
    const half = a >> 1
    const direct = (x: number, y: number) => {
      c.lastGoal = [x, y]
      c.path = [[x, y]]
      c.pathLen = 1
      c.pathIdx = 0
    }
    const tryPath = (x: number, y: number) => this.tileFree(c, x, y, 0) && this.findPath(c, x, y, 1)
    if (dx2 + dy2 >= a * a && tryPath(ex - dx, ey - dy)) return
    if (tryPath(ex + sx * half, ey)) return
    if (this.tileFree(c, ex, ey + sy * half, 0)) return direct(ex, ey + sy * half)
    if (this.tileFree(c, ex + sx, ey, 0)) return direct(ex + sx, ey)
    const h1 = half + 1
    if (dx2 < dy2) {
      if (tryPath(ex - h1 * sx, ey)) return
      if (tryPath(ex, ey - h1 * sy)) return
      if (this.tileFree(c, ex - sx, ey, 0)) return direct(ex - sx, ey)
      if (this.tileFree(c, ex, ey - sy, 0)) return direct(ex, ey - sy)
    } else {
      if (tryPath(ex, ey - h1 * sy)) return
      if (tryPath(ex - h1 * sx, ey)) return
      if (this.tileFree(c, ex, ey - sy, 0)) return direct(ex, ey - sy)
      if (this.tileFree(c, ex - sx, ey, 0)) return direct(ex - sx, ey)
    }
  }

  // ---------------------------------------------------------------------------------------------- pathfinder
  /** 0x08006F1A path(char, goal, mode): returns true when the char has a path (or keeps its old one) */
  findPath(c: Char, gx: number, gy: number, mode: number): boolean {
    gx = gx << 16 >> 16
    gy = gy << 16 >> 16
    const cx = c.x >> 3
    const cy = c.y >> 3
    if ((cx === gx && cy === gy) || (c.lastGoal[0] === gx && c.lastGoal[1] === gy)) return true // QUIRK: same goal -> keep the old path
    const [w, h] = this.mapTiles
    if (gx < 0 || gx >= w || gy < 0 || gy >= h) return false
    const d = Log1Game.manh(cx, cy, gx, gy)
    const setSingle = (x: number, y: number) => {
      c.lastGoal = [x, y]
      c.path = [[x, y]]
      c.pathLen = 1
      c.pathIdx = 0
    }
    if (d <= 1) {
      if (this.tileFree(c, gx, gy, 0)) {
        setSingle(gx, gy)
        return true
      }
      return false
    }
    const ip = c.aggroTiles
    if (ip >= d) {
      if (this.tileFree(c, gx, gy, 0) && this.astar(cx, cy, gx, gy, c, 2)) return true
      if (mode === 1) return false
      const ddx = gx - cx
      const ddy = gy - cy
      let mx = (cx + (ddx >> 1)) << 16 >> 16
      let my = (cy + (ddy >> 1)) << 16 >> 16
      if (mx === cx && my === cy) return false
      if (this.tileFree(c, mx, my, 0) && this.astar(cx, cy, mx, my, c, 2)) return true
      mx = (cx + (ddx >> 2)) << 16 >> 16
      my = (cy + (ddy >> 2)) << 16 >> 16
      if (mx === cx && my === cy) return false
      let tx: number
      let ty: number
      if ((mx - cx) * (mx - cx) > (my - cy) * (my - cy)) {
        tx = mx
        ty = gy
      } else {
        tx = gy // QUIRK: the ROM passes goal.y as the x coordinate here (0x0800711E)
        ty = my
      }
      if (this.tileFree(c, tx, ty, 0) && this.astar(cx, cy, tx, ty, c, 2)) return true
      return false
    }
    // far target: aim aggro/2 tiles past the goal along the dominant axis, else step one tile
    const sx = cx <= gx ? 1 : -1
    const sy = cy <= gy ? 1 : -1
    const half = ip >> 1
    if ((gx - cx) * (gx - cx) > (gy - cy) * (gy - cy)) {
      const tx = gx + half * sx
      if (this.tileFree(c, tx, gy, 0) && this.astar(cx, cy, tx, gy, c, 2)) return true
      if (this.tileFree(c, cx + sx, cy, 0)) {
        setSingle(cx + sx, cy)
        return true
      }
      return false
    }
    const ty = gy + half * sy
    if (this.tileFree(c, gx, ty, 0) && this.astar(cx, cy, gx, ty, c, 2)) return true
    if (this.tileFree(c, cx, cy + sy, 0)) {
      setSingle(cx, cy + sy)
      return true
    }
    return false
  }

  /** 0x0800786A: passable neighbours (px coords) in the order left, right, up, down; counts toward the budget */
  private neighbours(c: Char, px: number, py: number): [number, number][] {
    c.decCount = (c.decCount + 1) & 0xffff
    const out: [number, number][] = []
    const maxX = this.mapTiles[0] * 8
    const maxY = this.mapTiles[1] * 8
    const test = (x: number, y: number) => {
      if (this.passable({ x0: x + c.bbox[0], y0: y + c.bbox[1], x1: x + c.bbox[0] + c.bbox[2], y1: y + c.bbox[1] + c.bbox[3] }, c.layer)) out.push([x >> 3, y >> 3])
    }
    if (px > 0) test(px - 8, py)
    if (px < maxX) test(px + 8, py)
    if (py > 0) test(px, py - 8)
    if (py < maxY) test(px, py + 8)
    return out
  }

  /**
   * 0x08007A4E A* with a binary heap (open, 0x03000C28) and an insertion-"sorted" closed list (0x030023AC).
   * The "path" is the sequence of popped nodes, cleaned so consecutive nodes are adjacent; with partial = mode the
   * search may stop early (budget: char+0x16 > 7 expansions, 99 nodes, cost bound max(2d,6)) and keeps the prefix
   * up to the first node within `mode` tiles of the goal.
   */
  astar(sx: number, sy: number, gx: number, gy: number, c: Char, mode: number): boolean {
    c.pathLen = 0
    c.pathIdx = 0
    if (sx === gx && sy === gy) return true
    const d = Log1Game.manh(sx, sy, gx, gy)
    if (d === 1) {
      c.pathLen = 1
      c.lastGoal = [gx, gy]
      c.path = [[gx, gy]]
      return true
    }
    type Node = { x: number; y: number; g: number; f: number; key: number; prio: number }
    const key = (x: number, y: number) => ((x << 10) + y) | 0
    // closed list: 1-based array, QUIRK insertion that never compares the newest element (0x0800F628)
    const closed: Node[] = [null as unknown as Node]
    const closedInsert = (n: Node) => {
      closed.push({ ...n })
      const count = closed.length - 1
      let i = count - 1
      if (count < 2 || i <= 1) return
      while (i > 1) {
        if (closed[i].key >= closed[i - 1].key) break
        const t = closed[i]
        closed[i] = closed[i - 1]
        closed[i - 1] = t
        i--
      }
    }
    // 0x080079E4 lookup: slot 1, last slot, then a binary search
    const closedHas = (k: number) => {
      const count = closed.length - 1
      if (count === 0) return false
      if (k === closed[1].key) return true
      if (count === 1) return false
      if (k === closed[count].key) return true
      let lo = 1
      let hi = count
      for (;;) {
        const mid = (lo + hi) >> 1
        if (mid === lo) return false
        const mk = closed[mid].key
        if (k === mk) return true
        if (k < mk) hi = mid
        else lo = mid
      }
    }
    // open heap: 1-based, slot 0 sentinel prio -100000 (0x0800F588 push, 0x08007BC0 pop)
    const heap: Node[] = [{ x: 0, y: 0, g: 0, f: 0, key: 0, prio: -100000 }]
    const push = (n: Node) => {
      heap.push(n)
      let i = heap.length - 1
      if (i <= 1) return
      while (heap[i >> 1].prio > n.prio) {
        heap[i] = heap[i >> 1]
        i >>= 1
      }
      heap[i] = n
    }
    const pop = (): Node => {
      const top = heap[1]
      const last = heap[heap.length - 1]
      heap.pop()
      const count = heap.length - 1
      if (count === 0) return top
      let i = 1
      const half = count >> 1
      while (half >= i) {
        let ch = 2 * i
        if (ch < count && heap[ch].prio >= heap[ch + 1].prio) ch++
        if (last.prio < heap[ch].prio) break
        heap[i] = heap[ch]
        i = ch
      }
      heap[i] = last
      return top
    }
    const mk = (x: number, y: number, g: number): Node => {
      const f = Log1Game.manh(x, y, gx, gy) + g
      return { x, y, g, f, key: key(x, y), prio: (f << 7) + (0x7f - g) }
    }
    closedInsert({ x: sx, y: sy, g: 0, f: d, key: key(sx, sy), prio: (d << 7) + 0x7f })
    const maxCost = Math.max(d * 2, 6)
    const path: [number, number][] = []
    let partial = false
    let done = false
    for (const [nx, ny] of this.neighbours(c, sx << 3, sy << 3)) push(mk(nx, ny, 1))
    while (!done) {
      if (heap.length - 1 <= 0) {
        if (mode === 0) return false
        partial = true
        break
      }
      const n = pop()
      if (closedHas(n.key)) continue
      path.push([n.x, n.y])
      const g1 = n.g + 1
      if (g1 + n.f - 1 > maxCost || path.length >= 0x63 || c.decCount > 7) {
        if (mode === 0) return false
        partial = true
        break
      }
      closedInsert(n)
      for (const [nx, ny] of this.neighbours(c, n.x << 3, n.y << 3)) {
        if (nx === gx && ny === gy) {
          path.push([gx, gy])
          done = true
          break
        }
        const nn = mk(nx, ny, g1)
        if (!closedHas(nn.key)) push(nn)
      }
    }
    // 0x08007DE0: path cleanup
    let n = path.length
    if (n <= 1) return false
    const removed = new Array(n).fill(false)
    let any = false
    let last = n - 1
    for (let i = n - 2; i > 0; i--) {
      if (Log1Game.manh(path[last][0], path[last][1], path[i][0], path[i][1]) !== 1) {
        removed[i] = true
        any = true
      } else last = i
    }
    if (Log1Game.manh(path[last][0], path[last][1], path[0][0], path[0][1]) !== 1) {
      removed[0] = true
      any = true
    }
    let clean = any ? path.filter((_, i) => !removed[i]) : path
    n = clean.length
    if (n <= 0) return false
    if (partial) {
      let cut = -1
      for (let i = 0; i < n; i++)
        if (Log1Game.manh(clean[i][0], clean[i][1], gx, gy) <= mode) {
          cut = i
          break
        }
      if (cut < 0) return false
      clean = clean.slice(0, cut)
      n = cut
    } else if (clean[n - 1][0] !== gx || clean[n - 1][1] !== gy) return false
    c.path = clean
    c.pathLen = n
    c.lastGoal = [gx, gy]
    return true
  }
}

export function levelRow(levels: LevelRow[], level: number): LevelRow {
  return levels[Math.max(0, Math.min(levels.length - 1, level - 1))]
}

export { damageCap }
export type { DamageResult }
