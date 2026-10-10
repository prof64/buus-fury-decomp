// Debug helper (skipped unless TRACE is set): prints game vs simulator rows around a divergence.
//   TRACE=ptero FROM=800 TO=830 npx vitest run tests/battle/log1_dbg.test.ts
import { it } from 'vitest'
import vec from '../../../data/battle/log1_vectors.json'
import { keysAt, simFromInit } from './log1_harness'
/* eslint-disable @typescript-eslint/no-explicit-any */
it.skipIf(!process.env.TRACE)('dbg', () => {
  const name = process.env.TRACE ?? 'ptero'
  const a = Number(process.env.FROM ?? 800)
  const b = Number(process.env.TO ?? 830)
  const k = Number(process.env.E ?? 0)
  const t = (vec as any).traces.find((x: any) => x.name === name)
  const sim = simFromInit(t.init)
  for (let f = 1; f <= b; f++) {
    sim.step(keysAt(t.keys, f))
    if (f >= a) {
      const c = sim.enemies[k].char
      console.log(f, 'game', JSON.stringify(t.rows[f - 1].slice(0, 6)), JSON.stringify(t.rows[f - 1].slice(6 + k * 8, 14 + k * 8)), 'sim', c.x, c.y, c.state, c.stun, c.act, 'path', c.pathIdx, c.pathLen, JSON.stringify(c.path.slice(0, 8)), 'lg', c.lastGoal, 'P', sim.player.sx, sim.player.sy, sim.player.st, 'cam', sim.w.camera.x0, sim.w.camera.y0, 'pt', sim.g.pathTimer, 'rng', sim.g.rng.getState())
    }
  }
})
