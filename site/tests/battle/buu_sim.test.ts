// Behaviour of the Buu's Fury simulator against emulator measurements (re/buu/notes.md §8 / §8b,
// enemy_ai.json `verified`, bosses.json): aggro radii, movement speeds, attack timings and boss mechanics.
// Player: new-game Goku Lv60 (STR/POW 27, END 26, 135 HP) unless noted.
import { describe, expect, it } from 'vitest'
import { fighter, makeBuuSim, type BuuSim } from './buu_harness'
import { BTN } from '../../src/battle/core/input'
import type { Enemy } from '../../src/battle/rulesets/buu/enemy'

/** place enemy 0 so that its body centre is `dx`,`dy` px from the player's body centre */
function place(s: BuuSim, dx: number, dy = 0, e: Enemy = s.enemies[0]) {
  const [pcx, pcy] = s.player.center()
  const [ecx, ecy] = e.center()
  e.x += (pcx + dx - ecx) << 8
  e.y += (pcy + dy - ecy) << 8
}
const god = (s: BuuSim) => {
  s.player.rec.hp = s.player.rec.maxHp = 99999
}
// default arena: the Time Chamber's open floor (no walls near the fighters)
function sim(enemy: string, arena = 'htc', seed = 1) {
  const s = makeBuuSim({ enemy, arena, seed, fighter: fighter() })
  // put the player in the middle of the arena
  const b = s.w.arena.bounds
  s.player.x = ((b.x0 + b.x1) >> 1) << 8
  s.player.y = ((b.y0 + b.y1) >> 1) << 8
  return s
}

describe('player', () => {
  it('new-game Goku Lv60: 135 HP / 90 EP, STR/POW 27, END 26, walk 352 dash 672', () => {
    const s = sim('t68')
    expect([s.player.maxHp, s.player.maxEp, s.player.stats.STR, s.player.stats.POW, s.player.stats.END]).toEqual([135, 90, 27, 27, 26])
    expect([s.player.walk, s.player.dashSpd]).toEqual([352, 672])
  })
  it('Super Saiyan form: +5 stats, +128 speed (measured_bonus)', () => {
    const s = makeBuuSim({ enemy: 't68', fighter: fighter({ form: 3 }) })
    expect([s.player.stats.STR, s.player.walk]).toEqual([32, 480])
  })
  it('Energy Blast costs 5% max EP; shots fly 3 px/frame', () => {
    const s = sim('t68')
    place(s, 200)
    s.run(1)
    const ep = s.player.ep
    s.run(20, (f) => (f < 2 ? BTN.B : 0))
    expect(ep - s.player.ep).toBe(Math.ceil((90 * 0xccd) / 65536))
    const blast = s.w.entities.find((e) => e.kind === 'kiblast') as unknown as { x: number }
    const x0 = blast.x
    s.run(5)
    expect((blast.x - x0) / 256).toBe(15)
  })
  it('hurt state: 24 frames with zero resists (i-frames)', () => {
    const s = sim('t68')
    god(s)
    place(s, 22)
    let hurtAt = -1
    for (let f = 0; f < 400 && hurtAt < 0; f++) {
      s.run(1)
      if (s.player.state === 'hurt') hurtAt = s.w.frame
    }
    expect(hurtAt).toBeGreaterThan(0)
    expect(s.player.stats.resM).toBe(0)
    // the hit frame counts (the hurt state ticks in it: emulator lockstep f02_hurt, hurt at frame 10, idle at 33)
    s.run(22)
    expect(s.player.state).toBe('hurt')
    s.run(1)
    expect(s.player.state).toBe('idle')
    expect(s.player.stats.resM).toBe(128)
  })
})

describe('enemy AI vs emulator', () => {
  for (const [name, id, from, r] of [
    ['Majin Fighter', 't68', 0x14b, 100],
    ['Gunman', 't44', -1, 88],
    ['Mercenary', 't75', -1, 72],
  ] as [string, string, number, number][]) {
    it(`${name} aggro radius ${r} px (yes at ${r}, no at ${r + 4})`, () => {
      const far = sim(id)
      place(far, r + 4)
      const st0 = far.enemies[0].ai.top().state
      far.run(5)
      expect(far.enemies[0].ai.top().state).toBe(st0)
      const near = sim(id)
      place(near, r)
      near.run(5)
      expect(near.enemies[0].ai.top().state).not.toBe(st0)
      if (from >= 0) expect(st0).toBe(from)
    })
  }
  it('Majin Fighter chases at 153/256 = 0.5977 px/frame', () => {
    const s = sim('t68')
    god(s)
    place(s, 98)
    const e = s.enemies[0]
    const steps: number[] = []
    let prev = e.x
    for (let f = 0; f < 120; f++) {
      s.run(1)
      if (e.x !== prev) steps.push(Math.abs(e.x - prev))
      prev = e.x
    }
    const mode = [...steps].sort((a, b) => steps.filter((v) => v === b).length - steps.filter((v) => v === a).length)[0]
    expect(Math.abs(mode - 153)).toBeLessThanOrEqual(1)
  })
  it('Ghost wanders at 0.5 px/frame', () => {
    const s = sim('t32', 'crater')
    place(s, 120, 0)
    const e = s.enemies[0]
    let moved = 0
    let frames = 0
    let prev = [e.x, e.y]
    for (let f = 0; f < 300; f++) {
      s.run(1)
      const d = Math.hypot(e.x - prev[0], e.y - prev[1])
      if (d) {
        moved += d
        frames++
      }
      prev = [e.x, e.y]
    }
    expect(moved / frames / 256).toBeCloseTo(0.5, 2)
  })
  it('Majin Fighter melee on Goku Lv60 = 17..19 HP per hit (14/14 exact in the emulator)', () => {
    const s = sim('t68')
    god(s)
    place(s, 20)
    s.run(900)
    const hits = s.w.log.filter((l) => /Majin Fighter -> player: [1-9]/.test(l.text)).map((l) => Number(/= (\d+) HP/.exec(l.text)![1]))
    expect(hits.length).toBeGreaterThan(3)
    for (const h of hits) expect(h).toBeGreaterThanOrEqual(17)
    for (const h of hits) expect(h).toBeLessThanOrEqual(29) // crit x1.5
  })
  it("Pilaf's Guardian HeavyStrike reaches 60 px (centre to centre) but not 65; the strike lands 58 frames into its state", () => {
    for (const [d, expectHit] of [
      [60, true],
      [65, false],
    ] as [number, boolean][]) {
      const s = sim('t63')
      god(s)
      place(s, d)
      s.enemies[0].heavyStrike()
      const hit = s.w.log.some((l) => /Guardian -> player: [1-9]/.test(l.text))
      expect(hit, `at ${d} px`).toBe(expectHit)
    }
    // full timing: let it walk in, the hit comes 58 frames after it enters the strike state 0x138
    const s = sim('t63')
    god(s)
    place(s, 40)
    let entered = -1
    let hitAt = -1
    for (let f = 0; f < 3000 && hitAt < 0; f++) {
      s.run(1)
      if (entered < 0 && s.enemies[0].ai.top().state === 0x138) entered = s.w.frame
      if (entered >= 0 && s.w.log.some((l) => /Guardian -> player/.test(l.text))) hitAt = s.w.frame
    }
    expect(hitAt - entered).toBe(58)
  })
  it('Vampire grab: -POW HP every 16 frames, the vampire heals the same', () => {
    const s = sim('t133')
    god(s)
    place(s, 16)
    let grabbed = -1
    for (let f = 0; f < 1500 && grabbed < 0; f++) {
      s.run(1)
      if (s.player.holdKind === 'vampire') grabbed = f
    }
    expect(grabbed).toBeGreaterThanOrEqual(0)
    const hp0 = s.player.hp
    s.run(64)
    expect(hp0 - s.player.hp).toBe(4 * 10)
  })
  it('Blister spawns an Enzyme every 120 frames, 8 at most', () => {
    const s = sim('t6', 'crater')
    god(s)
    place(s, 150)
    s.run(119 * 1 + 1)
    const n1 = s.enemies.filter((e) => e.type.type === 0x18).length
    s.run(120 * 12)
    const n2 = s.enemies.filter((e) => e.type.type === 0x18).length
    expect(n1).toBe(1)
    expect(n2).toBe(8)
  })
})

describe('bosses vs emulator', () => {
  it('Majin Buu 0x1A/0x1B take no damage (no ApplyDamage in their on-hit trees; 0 HP lost in the emulator), 0x1C does', () => {
    for (const [id, dmg] of [
      ['t26', false],
      ['t27', false],
      ['t28', true],
    ] as [string, boolean][]) {
      const s = sim(id, 'crater')
      god(s)
      const e = s.enemies[0]
      place(s, 30)
      s.player.facing = 3
      const hp = e.hp1024
      s.run(200, (f) => {
        place(s, 30)
        return f % 10 < 2 ? BTN.A : 0
      })
      expect(e.hp1024 < hp, id).toBe(dmg)
    }
  })
  it('Janemba: idle 240 frames, ten hand strikes in 573 frames, then rises off-screen (no damage while airborne)', () => {
    const s = sim('t51', 'hfil')
    god(s)
    const e = s.enemies[0]
    const strikes: number[] = []
    for (let f = 0; f < 1100; f++) {
      s.run(1)
      if (s.w.entities.some((x) => x.kind === 'hand' && (x as unknown as { t: number }).t === 1)) strikes.push(f)
      if (e.z < -50) break
    }
    expect(strikes.length).toBe(10)
    expect(strikes[0]).toBeGreaterThanOrEqual(240)
    expect(strikes[0]).toBeLessThanOrEqual(260)
    expect(strikes[9] - strikes[0]).toBeGreaterThanOrEqual(573 - 57 - 20)
    expect(strikes[9] - strikes[0]).toBeLessThanOrEqual(573)
    const hp = e.hp1024
    place(s, 0)
    s.run(30, (f) => (f % 10 < 2 ? BTN.A : 0))
    expect(e.hp1024).toBe(hp)
  })
  it('Super Janemba wave: 157 HP measured on Goku Lv60 (model 151-166)', () => {
    const s = sim('t122', 'hfil')
    god(s)
    place(s, 70)
    s.run(2500)
    const waves = s.w.log.filter((l) => /Super Janemba -> player/.test(l.text) && / 1\/1024|x1024|\b/.test(l.text)).map((l) => Number(/= (\d+) HP/.exec(l.text)?.[1] ?? 0))
    const big = waves.filter((v) => v >= 140)
    expect(big.length).toBeGreaterThan(0)
    for (const v of big.filter((x) => x < 200)) expect(v).toBeGreaterThanOrEqual(151)
  })
  it('Kid Buu splits: HP halved into a type 0x3D copy', () => {
    const s = sim('t60', 'wasteland')
    god(s)
    place(s, 100)
    const e = s.enemies[0]
    let split = false
    for (let f = 0; f < 3000 && !split; f++) {
      s.run(1, () => 0)
      place(s, 100)
      split = s.enemies.some((x) => x.type.type === 0x3d)
    }
    expect(split).toBe(true)
    const copy = s.enemies.find((x) => x.type.type === 0x3d)!
    expect(e.hp1024).toBe((125000 * 1024 + 1024) >> 1)
    expect(copy.hp1024).toBe(e.hp1024)
  })
  it('Spopovich + Yamu: bear hug, then Yamu drains a quarter of max EP at 1 EP / 8 frames', () => {
    const s = makeBuuSim({ enemy: 't111+t147', arena: 'stadium', fighter: fighter() })
    god(s)
    s.player.rec.ep = s.player.rec.maxEp = 400
    s.player.regenTimer = -1e9 // isolate the drain from the 3% EP regen ticks
    const spo = s.enemies.find((e) => e.type.type === 0x6f)!
    place(s, 12, 0, spo)
    let hugged = false
    for (let f = 0; f < 3000; f++) {
      s.run(1)
      place(s, 12, 0, spo)
      if (s.player.holdKind === 'hug') hugged = true
      if (hugged && s.player.holdKind !== 'hug') break
    }
    expect(hugged).toBe(true)
    expect(400 - s.player.ep).toBeGreaterThanOrEqual(95)
    expect(400 - s.player.ep).toBeLessThanOrEqual(101)
  })
})
