// Per-battle context shared by the Buu's Fury entities: data tables, the player, the enemy list, the few global
// words the AI reads (candy timer 0x03002FE4, Buu pacified 0x0300302C, Dabura sphere byte 0x030032E4, Super Buu
// waypoint 0x030032EC), spawning helpers and damage popups.

import type { SpriteSet } from '../../core/sprites'
import type { Popup, World } from '../../core/world'
import { Effect, type Actor } from './actor'
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
  /** 0x0300302C */
  buuPacified = 0
  /** 0x030032E4: 0 active, 1 destroyed, 2 fading, 3 expired */
  sphereState = 0
  /** 0x030032EC */
  waypoint = 0
  /** 0x03003228 TeleportChain4 counter */
  teleChain = 0
  /** spawn an enemy of `type` at integer px (set by index.ts) */
  spawnEnemy: (w: World, type: number, x: number, y: number, parent?: EnemyLike | null, startState?: number) => EnemyLike | null = () => null
  private lastPopup = new Map<Actor, { frame: number; p: Popup; value: number }>()

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
  puff(w: World, x: number, y: number) {
    this.spawnFx(w, 'fx_086fd9d0', x, y, [0, 1, 2, 3], 4)
  }
  explosion(w: World, x: number, y: number) {
    this.spawnFx(w, 'fx_08712fdc', x, y, [0, 1, 2, 3], 6)
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
