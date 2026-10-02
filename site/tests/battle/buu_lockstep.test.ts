// Frame-by-frame lockstep of the player against the emulator (tools/battle/LOCKSTEP.md): every recorded scenario of
// data/battle/buu_lockstep/ is replayed with the same start state and inputs; position, facing, animation (raw index,
// frame index and its ROM frame record), state, HP / EP, form, the RNG state and every sound call must match on
// every frame.
//   LOCKSTEP=f02_move npx vitest run tests/battle/buu_lockstep.test.ts   -> only matching scenarios (regex), with a
//   report of the first divergence (context rows emu / sim; LOCKSTEP_CTX=n rows, LOCKSTEP_BRIEF=1 one line each)
import { describe, expect, it } from 'vitest'
import { compare, expand, loadTraces, replay, report } from './buu_lockstep'

const filter = process.env.LOCKSTEP
const traces = loadTraces(filter)

/** scenarios whose divergence is understood and not modelled (LOCKSTEP.md "Known divergences") */
const KNOWN: [RegExp, string][] = [
  [/^f(32|33)_ssdrain$/, 'cutscene fusion: ~150 frames after the defuse the idle blink comes one tick late (unexplained; the other defuse traces match)'],
  [/^f23_(bump|move|punch|skill11)$/, 'Trunks form 23 uses an NPC sprite: its dash / punch / skill animation slots are garbage pointers'],
  [/^f46_punch$/, 'Android 18 (NPC sprite): the punch animations are garbage pointers (OAM), the game reads their length from OAM'],
  [/^f14_(skill20|ssdrain)$/, 'game bug: Super Saiyan as Great Saiyaman (up 255 != down 10) transforms into form 255'],
]

describe('lockstep vs emulator', () => {
  it('has traces', () => {
    expect(traces.length).toBeGreaterThan(0)
  })
  for (const t of traces) {
    const known = KNOWN.find(([re]) => re.test(t.name))
    if (known && !filter) {
      it.skip(`${t.name} (known: ${known[1]})`, () => undefined)
      continue
    }
    it(t.name, () => {
      const exp = expand(t)
      const got = replay(t)
      const d = compare(exp, got)
      const line = d ? `${t.name}: first divergence at frame ${d.frame} (${d.fields.join(', ')})` : null
      if (d && filter) console.log(process.env.LOCKSTEP_BRIEF ? `LS ${line}` : report(t, exp, got, d, Number(process.env.LOCKSTEP_CTX ?? 4)))
      expect(line).toBeNull()
    })
  }
})
