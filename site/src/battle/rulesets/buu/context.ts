// Per-battle context shared by the Buu's Fury entities: data tables, the player, the enemy list, the few global
// words the AI reads (candy timer 0x03002FE4, Buu pacified 0x0300302C, Dabura sphere byte 0x030032E4, Super Buu
// waypoint 0x030032EC), spawning helpers and damage popups.

import type { SpriteSet } from '../../core/sprites'
import type { Popup, World } from '../../core/world'
import { Effect, type Actor } from './actor'
import { FxAnim } from './fx'
import type { BuuCollision } from './collision'
import type { BuuData, EnemyType } from './data'

export interface PlayerLike extends Actor {
  hp: number
  maxHp: number
  ep: number
  maxEp: number
  dead: boolean
  /** player is in a hurt/hold state that the AI must not grab again */
  heldBy: Actor | null
  /** current state is an attack / charging state (IfPlayerAttackingMe, IfInPlayerBeamLine) */
  attacking(): boolean
  charging(): boolean
  /** the player's live Energy Blast projectile (IfPlayerBlastIncoming, 0x03001B94 current projectile) */
  blast: { removed: boolean; rect(): { x0: number; y0: number; x1: number; y1: number }; dir: number } | null
  /** forced states driven by enemies */
  hold(w: World, by: Actor, kind: HoldKind, frames: number): void
  release(w: World): void
  /** HP loss bypassing the formula (vampire / enzyme / eat), never below `floor` */
  loseHp(w: World, n: number, floor?: number): number
  loseEp(n: number): void
  isStone(): boolean
  isCandy(): boolean
  wraps: number
  gainXp(w: World, exp: number): void
  /** current animation is "slot 0x13" (bat-the-ball swing) - INFERRED: a melee swing */
  swinging(): boolean
}

export type HoldKind = 'vampire' | 'hug' | 'latch' | 'eat' | 'freeze' | 'stone' | 'grapple'

export interface BuuOptions {
  popups: boolean
  xp: boolean
}

export interface EnemyLike extends Actor {
  type: EnemyType
  hp1024: number
  defeated: boolean
  parentE: EnemyLike | null
  children: EnemyLike[]
  primary: boolean
  part: boolean
  die(explode: boolean): void
}

export class BuuCtx {
  player: PlayerLike | null = null
  enemies: EnemyLike[] = []
  /** global 0x03002FE4: candy frames left (> 0 = the player is candy) */
  candy = 0
  /** frame of sprite 45 (three sweets) the candy-ed player shows: rand(3) on the candy-beam hit */
  candyFrame = 0
  /** 0x0300302C */
  buuPacified = 0
  /** 0x030032E4: 0 active, 1 destroyed, 2 fading, 3 expired */
  sphereState = 0
  /** 0x030032EC */
  waypoint = 0
  /** 0x03003228 TeleportChain4 counter */
  teleChain = 0
  /** 0x030032E5: re-plan phase handed to the next path follower (path.ts), +1 & 7 each time */
  pathStagger = 0
  /** spawn an enemy of `type` at integer px (set by index.ts) */
  spawnEnemy: (w: World, type: number, x: number, y: number, parent?: EnemyLike | null, startState?: number) => EnemyLike | null = () => null
  private lastPopup = new Map<Actor, { frame: number; p: Popup; value: number }>()

  /** map collision of the arena (null = open arena, bounds only) */
  col: BuuCollision | null = null
  /** HUD +0x10: frame of the last low-HP beep (0x0800BA4A) */
  lastBeep = -1000
  /** set the camera's 22.10 position (index.ts camera; lockstep harness) */
  setCamera: (x: number, y: number) => void = () => undefined
  /** screen shakes 0x0800797C(duration, intensity) (task 0x080079BC) and the camera offset they set (px) */
  shakes: { left: number; intensity: number; period: number; cnt: number }[] = []
  shakeOff: [number, number] = [0, 0]
  /** 0x0800797C: dash into a wall (8, 100), critical melee hit (15, 75) */
  shake(duration: number, intensity: number) {
    this.shakes.push({ left: duration, intensity, period: Math.trunc((100 - intensity) / 16) + 1, cnt: 1 })
  }
  /** the shake tasks run after the objects each frame (first in their creation frame): every `period` frames two
   *  draws of the game RNG pick the offset ((next - 0x8000) * intensity) >> 19 per axis; at the end offset 0 */
  tickShakes(w: World) {
    for (const s of this.shakes) {
      if (--s.left === 0) {
        this.shakeOff = [0, 0]
        continue
      }
      if (--s.cnt === 0) {
        s.cnt = s.period
        const dx = Math.imul(((w.rng.next('shake x') - 0x8000) | 0), s.intensity) >> 19
        const dy = Math.imul(((w.rng.next('shake y') - 0x8000) | 0), s.intensity) >> 19
        this.shakeOff = [dx, dy]
      }
    }
    this.shakes = this.shakes.filter((s) => s.left > 0)
  }
  readonly data: BuuData
  readonly opts: BuuOptions
  constructor(data: BuuData, opts: BuuOptions) {
    this.data = data
    this.opts = opts
  }

  fx(id: string): SpriteSet | undefined {
    return this.data.sprites.get(id)
  }
  spawnFx(w: World, id: string, x: number, y: number, frames = [0, 1, 2, 3], dur = 4) {
    w.add(new Effect(this.fx(id), x, y, frames, dur))
  }
  /** generic animated effect object 0x0802A81C: records start..start+n-1 of fx run `id`, 0x10000 / rate ticks each */
  fxAnim(w: World, id: string, x: number, y: number, start: number, n: number, rate: number): FxAnim {
    return w.add(new FxAnim(this.fx(id), x, y, start, n, rate))
  }
  /** smoke puff 0x086FD9D0 (4 records; the game's spawn sites mostly use rate 0x1000 + rand(0x1000), prio 0x37) */
  puff(w: World, x: number, y: number, rate = 0x1800) {
    this.fxAnim(w, 'fx_086fd9d0', x, y, 0, 4, rate)
  }
  /** puff with the game's random rate 0x1000 + rand(0x1000) (one RNG call; sites verified in the emulator: ninja
   *  teleport, shield break, energy-wave hit, Dabura's sphere destroyed, stone break-free) */
  puffRng(w: World, x: number, y: number) {
    this.puff(w, x, y, 0x1000 + w.rng.rand(0x1000, 'puff rate'))
  }
  /** explosion 0x08712FDC (4 records; projectile explosions use rate 0x2000 = 8 ticks per record, prio 0x38) */
  explosion(w: World, x: number, y: number, rate = 0x2000) {
    this.fxAnim(w, 'fx_08712fdc', x, y, 0, 4, rate)
  }
  liveEnemies() {
    return this.enemies.filter((e) => !e.removed && !e.defeated)
  }

  damagePopup(w: World, target: Actor, dmg: number, crit: boolean, toPlayer: boolean) {
    if (!this.opts.popups || dmg <= 0) return
    const b = target.bodyBox()
    const x = (b.x0 + b.x1) >> 1
    const y = b.y0 - 6
    const prev = this.lastPopup.get(target)
    const color = crit ? '#ffd23f' : toPlayer ? '#ff6060' : '#ffffff'
    if (prev && w.frame - prev.frame < 8 && prev.p.age < prev.p.life && w.popups.includes(prev.p)) {
      prev.value += dmg
      prev.p.text = String(prev.value)
      prev.p.age = 0
      if (crit) prev.p.color = color
      prev.frame = w.frame
      return
    }
    w.popup(String(dmg), x, y, color, 40)
    this.lastPopup.set(target, { frame: w.frame, p: w.popups[w.popups.length - 1], value: dmg })
  }
}
