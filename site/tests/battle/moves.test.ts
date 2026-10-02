// Every player move replayed against the emulator measurement runs (re/log2/pl_measurements.json):
// same character, slot, facing, input script and the pinned wolves at the same offsets. Compares, hit by hit,
// the frame, attack table id, scale and the attack rectangle relative to the player.
import { describe, expect, it } from 'vitest'
import { replay, RUNS } from './replay'

const key = (h: { frame: number; attack_id: number; scale: number; rect_rel: number[]; enemy: number }) => `f${h.frame} e${h.enemy} id${h.attack_id} s${h.scale} [${h.rect_rel.join(',')}]`

// runs whose outcome depends on enemy AI movement / attacks (the free enemy walks into the beam / bomb; in
// spmelee_piccolo the wolf's strike interrupted the charge), so they can't be replayed hit-for-hit
const AI_DRIVEN = new Set(['kame_free', 'spirit_free', 'spmelee_piccolo'])

describe('player moves vs emulator runs', () => {
  for (const name of Object.keys(RUNS)) {
    if (AI_DRIVEN.has(name)) continue
    it(name, () => {
      const { sim, real } = replay(name)
      const n = real.length
      expect(sim.slice(0, n).map(key)).toEqual(real.slice(0, n).map(key))
      expect(sim.length).toBe(real.length)
    })
  }
})
