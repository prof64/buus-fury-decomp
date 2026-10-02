// Per-frame state dumps of the TypeScript Buu battle sim for the wf-core cross-check (engine/crates/wf-core/tests/
// cross_ts.rs). Runs headless in vitest from site/:
//   cd site && WF_DUMP_DIR=/tmp/dumps npx vitest run --dir .. tools/battle/buu_core_dump.test.ts
// Each scenario: a preset fight (enemy group, arena, seed) driven by pseudo-random but recorded inputs; the dump holds
// the inputs and, per frame, the RNG state, the player, every enemy, every projectile and the entity list.
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { fighter, makeBuuSim } from '../../site/tests/battle/buu_harness'
import { BTN } from '../../site/src/battle/core/input'
import type { Enemy } from '../../site/src/battle/rulesets/buu/enemy'
import type { Player } from '../../site/src/battle/rulesets/buu/player'

interface Scn {
  name: string
  enemy: string
  count?: number
  arena: string
  seed: number
  frames: number
  form?: number
  fighterId?: string
  /** input style: 'idle' (stand still: the AI acts alone), 'mix' (random walk / punch / skills) */
  style: 'idle' | 'mix' | 'melee'
  level?: number
}

const SCENARIOS: Scn[] = [
  { name: 'majin_fighters_idle', enemy: 't68', count: 4, arena: 'forest', seed: 7, frames: 900, style: 'idle' },
  { name: 'majin_fighters_mix', enemy: 't68', count: 4, arena: 'forest', seed: 11, frames: 1500, style: 'mix' },
  { name: 'majin_fighter_melee', enemy: 't68', count: 1, arena: 'htc', seed: 3, frames: 1200, style: 'melee' },
  { name: 'blister', enemy: 't6', arena: 'crater', seed: 5, frames: 1500, style: 'mix' },
  { name: 'dabura', enemy: 't18', arena: 'demon', seed: 9, frames: 1500, style: 'mix' },
  { name: 'kid_buu', enemy: 't60', arena: 'wasteland', seed: 13, frames: 1500, style: 'mix' },
  { name: 'broly', enemy: 't10', arena: 'volcano', seed: 17, frames: 1500, style: 'mix' },
  { name: 'worm', enemy: 't142', arena: 'crater', seed: 19, frames: 1200, style: 'idle' },
  { name: 'yakon', enemy: 't145', arena: 'yakon', seed: 23, frames: 1500, style: 'mix' },
  { name: 'majin_vegeta', enemy: 't70', arena: 'canyon', seed: 29, frames: 1500, style: 'mix' },
  { name: 'totenhotep', enemy: 't129', arena: 'crater', seed: 31, frames: 1200, style: 'mix' },
  { name: 'ninja_boss', enemy: 't80', arena: 'forest', seed: 37, frames: 1200, style: 'mix' },
  { name: 'guardian', enemy: 't63', arena: 'forest', seed: 41, frames: 1200, style: 'mix' },
  { name: 'warlord', enemy: 't0', arena: 'forest', seed: 43, frames: 1200, style: 'mix' },
  { name: 'janemba', enemy: 't51', arena: 'crater', seed: 47, frames: 1500, style: 'idle' },
  { name: 'super_buu', enemy: 't116', arena: 'crater2', seed: 53, frames: 1500, style: 'mix' },
  { name: 'majin_buu', enemy: 't26+~t29', arena: 'crater', seed: 59, frames: 1500, style: 'idle' },
  { name: 'tournament', enemy: 't111+t147', arena: 'forest', seed: 61, frames: 1200, style: 'idle' },
  { name: 'puipui', enemy: 't96', arena: 'demon', seed: 67, frames: 1200, style: 'mix' },
  { name: 'pikkon', enemy: 't91', arena: 'forest', seed: 71, frames: 1200, style: 'mix' },
  { name: 'vegeta_skills', enemy: 't68', count: 2, arena: 'forest', seed: 73, frames: 1500, style: 'mix', fighterId: 'vegeta', form: 26 },
  { name: 'gohan_skills', enemy: 't68', count: 2, arena: 'crater', seed: 79, frames: 1500, style: 'mix', fighterId: 'gohan', form: 10 },
  // enemy attacks / special holds (lockstep round 3: wraps, vampire, enzyme latch, fragments, grapple, waves, bombs)
  { name: 'mummy', enemy: 't77', count: 2, arena: 'crater', seed: 83, frames: 1500, style: 'melee' },
  { name: 'mummy_mix', enemy: 't77', count: 2, arena: 'crater', seed: 89, frames: 1500, style: 'mix' },
  { name: 'vampire', enemy: 't133', count: 2, arena: 'forest', seed: 97, frames: 1500, style: 'melee' },
  { name: 'enzyme', enemy: 't24', count: 3, arena: 'crater', seed: 101, frames: 1200, style: 'melee' },
  { name: 'fragment', enemy: 't14', count: 3, arena: 'crater', seed: 103, frames: 1200, style: 'mix' },
  { name: 'bio_mech', enemy: 't5', arena: 'forest', seed: 107, frames: 1500, style: 'melee' },
  { name: 'cyborg', enemy: 't17', count: 2, arena: 'forest', seed: 109, frames: 1200, style: 'mix' },
  { name: 'tank', enemy: 't127', arena: 'forest', seed: 113, frames: 1200, style: 'idle' },
  { name: 'turret', enemy: 't64', arena: 'forest', seed: 127, frames: 1200, style: 'mix' },
  { name: 'soldiers', enemy: 't69', count: 3, arena: 'forest', seed: 131, frames: 1200, style: 'mix' },
  { name: 'gunmen', enemy: 't44+t8', arena: 'forest', seed: 137, frames: 1200, style: 'idle' },
  { name: 'mechanoid', enemy: 't72', arena: 'forest', seed: 139, frames: 1200, style: 'mix' },
  { name: 'gotenks_ghosts', enemy: 't68', count: 2, arena: 'forest', seed: 149, frames: 1500, style: 'mix', fighterId: 'gotenks', form: 31 },
]

const KEYS = [0, BTN.LEFT, BTN.RIGHT, BTN.UP, BTN.DOWN, BTN.A, BTN.A, BTN.B, BTN.B, BTN.R, BTN.LEFT | BTN.A, BTN.RIGHT | BTN.B, BTN.L, BTN.UP | BTN.A, BTN.R | BTN.B, BTN.DOWN | BTN.A]

/** recorded pseudo-random inputs (an LCG; the dump stores the result, so nothing else depends on it) */
function inputs(s: Scn): number[] {
  let x = s.seed * 2654435761 >>> 0
  const out: number[] = []
  let cur = 0
  let left = 0
  for (let f = 0; f < s.frames; f++) {
    if (left <= 0) {
      x = (Math.imul(x, 1103515245) + 12345) >>> 0
      left = 6 + ((x >>> 16) % 30)
      const k = KEYS[(x >>> 8) % KEYS.length]
      cur = s.style === 'idle' ? 0 : s.style === 'melee' ? (f % 24 < 2 ? BTN.A : 0) | (k & (BTN.LEFT | BTN.RIGHT | BTN.UP | BTN.DOWN)) : k
    }
    left--
    out.push(s.style === 'melee' ? (f % 24 < 2 ? BTN.A : 0) | (cur & ~BTN.A) : cur)
  }
  return out
}

describe('wf-core cross-check dumps', () => {
  const dir = process.env.WF_DUMP_DIR
  it('has an output dir', () => expect(true).toBe(true))
  if (!dir) return
  mkdirSync(dir, { recursive: true })
  for (const s of SCENARIOS) {
    it(s.name, () => {
      const f = fighter({ fighter: s.fighterId ?? 'goku', form: s.form ?? 2, level: s.level ?? 60 })
      const sim = makeBuuSim({ enemy: s.enemy, count: s.count ?? 1, arena: s.arena, seed: s.seed, fighter: f })
      const keys = inputs(s)
      const frames: unknown[] = []
      let prev = 0
      for (let i = 0; i < s.frames; i++) {
        const held = keys[i]
        const inp = { held, pressed: held & ~prev, released: prev & ~held }
        sim.battle.preStep?.(sim.w, inp)
        sim.w.step(inp)
        sim.battle.postStep?.(sim.w)
        prev = held
        const w = sim.w
        const p = sim.player as Player
        const ents = w.entities.filter((e) => e.kind !== 'collision')
        frames.push({
          rng: w.rng.getState().map((v) => v >>> 0),
          cam: [w.camera.x0, w.camera.y0],
          p: [p.x, p.y, p.facing, p.hp, p.ep, p.gs, p.ap.raw, p.ap.frame, p.form.id, p.dead ? 1 : 0, p.wraps, p.heldBy ? p.heldBy.id : 0],
          e: ents.filter((e) => e.kind === 'enemy').map((e) => {
            const n = e as Enemy
            return [n.id, n.type.type, n.x, n.y, n.z, n.facing, n.hp1024, n.defeated ? 1 : 0, n.hittable ? 1 : 0, n.ai.stack.map((t) => t.state).join('>')]
          }),
          s: ents.filter((e) => 'owner' in e && 'dir' in e).map((e) => {
            const q = e as unknown as { id: number; kind: string; x: number; y: number; t: number }
            return [q.id, q.kind, q.x, q.y]
          }),
          k: ents.map((e) => `${e.id}:${e.kind}`).join(' '),
        })
      }
      writeFileSync(join(dir, `${s.name}.json`), JSON.stringify({ scenario: s, inputs: keys, frames }))
    })
  }
})
