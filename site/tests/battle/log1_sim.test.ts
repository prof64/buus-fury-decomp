// LoG1 battles played headlessly with a simple bot: every enemy/sprite variant, every arena and story preset must
// run without errors, and the mechanics visible from outside (ki regen, flight energy, Solar Flare freeze,
// the per-hit damage cap, level ups) must behave as decoded.
import { describe, expect, it } from 'vitest'
import { BTN } from '../../src/battle/core/input'
import { World } from '../../src/battle/core/world'
import type { BattleConfig } from '../../src/battle/core/ruleset'
import { Log1Ruleset } from '../../src/battle/rulesets/log1'
import { Log1Rng } from '../../src/battle/rulesets/log1/rng'
import type { Log1Game } from '../../src/battle/rulesets/log1/game'
import type { Player } from '../../src/battle/rulesets/log1/player'
import type { Enemy } from '../../src/battle/rulesets/log1/enemy'
import { log1Data } from './log1_harness'

const rs = new Log1Ruleset()
rs.d = log1Data()

function battle(enemy: string | { enemy: string; count: number }[], opts: Record<string, unknown> = {}, level = 25, arena = 'cliff', seed = 7) {
  const phases = typeof enemy === 'string' ? [{ enemy, count: 1 }] : enemy
  const cfg: BattleConfig = { game: 'log1', fighter: { fighter: 'goku', form: 0, level, bonus: {}, options: { pickups: false, ...opts } }, phases, arena, seed, options: {} }
  const w = new World(rs.createRng(seed) as Log1Rng, rs.arenaRuntime(arena))
  const b = rs.createBattle(cfg, w)
  const { g, player, enemies } = (w as unknown as { log1: { g: Log1Game; player: Player; enemies: Enemy[] } }).log1
  let prev = 0
  const step = (held: number) => {
    const inp = { held, pressed: held & ~prev, released: prev & ~held }
    prev = held
    b.preStep?.(w, inp)
    w.step(inp)
    b.postStep?.(w)
  }
  return { w, b, g, player, enemies, step }
}

/** chase the nearest enemy along one axis, punch when the punch point is in its hurtbox, tap ki blasts when aligned */
function bot(s: ReturnType<typeof battle>, f: number): number {
  const { player, enemies } = s
  const live = enemies.filter((e) => e.char.alive)
  if (!live.length || player.st === 2) return 0
  const t = live.reduce((a, e) => (Math.abs(e.x - player.x) + Math.abs(e.y - player.y) < Math.abs(a.x - player.x) + Math.abs(a.y - player.y) ? e : a))
  const dx = t.x - player.x
  const dy = t.y - player.y
  const face = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 3 : 2) : dy > 0 ? 0 : 1
  if (player.facing === face) {
    const [px, py] = player.punchPoint()
    const h = t.hurtRect()
    if (px >= h.x0 && px < h.x1 && py >= h.y0 && py < h.y1) return f % 18 === 0 ? BTN.A : 0
    if ((Math.abs(dx) < 6 || Math.abs(dy) < 10) && f % 24 === 0) return BTN.B
  }
  const dir = [BTN.DOWN, BTN.UP, BTN.LEFT, BTN.RIGHT][face]
  return f % 40 < 34 ? dir : 0
}

describe('LoG1 battles run for every enemy, arena and preset', () => {
  for (const e of rs.enemies()) {
    it(`${e.name} (${e.id})`, () => {
      const s = battle(e.id)
      for (let f = 0; f < 3000; f++) s.step(bot(s, f))
      // the simple bot is not expected to beat Frieza / the 70-80k HP soldiers; everything weaker must fall
      if (e.hp < 60000 && !e.tags.includes('no melee animation')) expect(s.b.outcome(s.w)).not.toBe('lost')
      expect(s.w.frame).toBe(3000)
    })
  }
  for (const a of rs.arenas()) {
    it(`arena ${a.id}: spawns on free ground, wolves fight`, () => {
      const s = battle([{ enemy: 't2:wolf', count: 3 }], {}, 25, a.id)
      expect(s.g.passable(s.player.footRect(), 0)).toBe(true)
      for (const e of s.enemies) expect(s.g.passable(e.char.rect(), 0)).toBe(true)
      for (let f = 0; f < 1500; f++) s.step(bot(s, f))
    })
  }
  for (const p of rs.presets()) {
    it(`preset ${p.id}`, () => {
      const s = battle(p.phases, {}, 25, p.arena)
      for (let f = 0; f < 6000 && s.b.outcome(s.w) === 'running'; f++) s.step(bot(s, f))
      expect(s.w.stats.hitsDealt).toBeGreaterThan(0)
    })
  }
})

describe('LoG1 mechanics in a live battle', () => {
  it('a Lv25 Goku beats Raditz and the bot collects the kill (no exp: story bosses give 0)', () => {
    const s = battle('t4:raditz')
    let f = 0
    for (; f < 4000 && s.b.outcome(s.w) === 'running'; f++) s.step(bot(s, f))
    expect(s.b.outcome(s.w)).toBe('won')
    expect(s.w.stats.kills).toBe(1)
  })
  it('Lv1 Goku levels up from wolf kills: HP gains only the max-HP delta, Ki is not refilled', () => {
    const s = battle([{ enemy: 't2:wolf', count: 6 }], {}, 1, 'paozu', 3)
    s.g.player.exp = 340
    s.g.player.hp = 80
    s.g.player.ki = 30
    const before = { hp: s.g.player.hp, ki: s.g.player.ki }
    // kill one wolf directly through hit() like a punch would
    const wolf = s.enemies[0].char
    wolf.hp = 1
    wolf.defense = 0
    s.g.hit(s.g.player, wolf, 0, 100)
    expect(wolf.alive).toBe(false)
    expect(s.g.player.exp).toBe(410)
    expect(s.g.level).toBe(2)
    expect(s.g.player.maxHp).toBe(200)
    expect(s.g.player.hp).toBe(before.hp + 95)
    expect(s.g.player.ki).toBe(before.ki)
    expect(s.g.cap).toBe(67)
  })
  it('enemy hits are capped at maxHP/3 + 1 and the negative-damage roll can heal', () => {
    const s = battle('t24:hench7', {}, 25)
    const g = s.g
    expect(g.cap).toBe(50001)
    // force many melee rolls from an 80000 HP soldier against Goku with defense 0
    g.player.defense = 0
    g.player.hp = 100000
    g.player.maxHp = 150000
    const c = s.enemies[0].char
    let healed = 0
    let capped = 0
    for (let i = 0; i < 400; i++) {
      const hp0 = g.player.hp
      c.state = 0
      g.attack(c, g.player, 0)
      if (g.player.hp > hp0) healed++
      if (hp0 - g.player.hp === 50001) capped++
      g.player.hp = 100000
    }
    expect(healed).toBeGreaterThan(0)
    expect(capped).toBeGreaterThan(0)
  })
  it('with authentic bugs off the soldier never heals Goku', () => {
    const s = battle('t24:hench7', { bugs: false }, 25)
    const g = s.g
    g.player.defense = 0
    g.player.hp = 100000
    const c = s.enemies[0].char
    for (let i = 0; i < 400; i++) {
      const hp0 = g.player.hp
      c.state = 0
      g.attack(c, g.player, 0)
      expect(g.player.hp).toBeLessThanOrEqual(hp0)
      g.player.hp = 100000
    }
  })
  it('Ki regenerates maxKi>>6 every 16 frames, not while flying; flight drains 1 energy per 8 moving frames', () => {
    const s = battle('t0:squirrel', {}, 10)
    const g = s.g
    g.player.ki = 0
    for (let f = 0; f < 64; f++) s.step(0)
    expect(g.player.ki).toBe(4 * (5000 >> 6))
    // take off and fly left for 80 frames
    s.step(BTN.R)
    s.step(0)
    expect(g.flying).toBe(true)
    const ki = g.player.ki
    const fe = g.flight
    for (let f = 0; f < 80; f++) s.step(BTN.LEFT)
    expect(g.player.ki).toBe(ki)
    expect(g.flight).toBe(fe - 10)
  })
  it('Solar Flare freezes the AI for 60 + charge*300/maxKi frames and blocks a re-use until it expires + 60', () => {
    const s = battle('t2:wolf', { move: '2' }, 10)
    const g = s.g
    g.selected = 2
    for (let f = 0; f < 41; f++) s.step(BTN.B)
    s.step(0)
    const spent = 5000 - g.player.ki
    expect(spent).toBeGreaterThan(0)
    expect(g.solar).toBeGreaterThan(60)
    expect(g.sfReady - g.fc).toBe(g.solar + 60)
  })
})
