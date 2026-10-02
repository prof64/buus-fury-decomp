// Lockstep debugging aid (skipped unless DBG is set): replays one trace exactly as the lockstep test does and prints,
// per frame in FROM..TO, the player / enemy state, the enemy's AI path, the other objects and every RNG call.
//   DBG='^f02_vs_enemy_gun$' FROM=40 TO=50 npx vitest run tests/battle/buu_lockstep_debug.test.ts
import { it } from 'vitest'
import { loadTraces, replay } from './buu_lockstep'
import type { BuuRng } from '../../src/battle/rulesets/buu/rng'

it.skipIf(!process.env.DBG)('lockstep debug', () => {
  const t = loadTraces(process.env.DBG)[0]
  const from = Number(process.env.FROM ?? 1)
  const to = Number(process.env.TO ?? 10)
  let rng: BuuRng | null = null
  replay(t, (f, { w, player, enemy: e, lag }) => {
    if (!rng) {
      rng = w.rng as BuuRng
      rng.historyLimit = 1000
    }
    if (f >= from && f <= to) {
      const objs = w.entities.filter((x) => x.kind !== 'player' && x.kind !== 'enemy' && x.kind !== 'lockstep' && 'x' in x).map((x) => `${x.kind}@${((x as unknown as { x: number }).x / 256).toFixed(2)},${((x as unknown as { y: number }).y / 256).toFixed(2)}${'len' in x ? ' len ' + (x as unknown as { len: number }).len : ''}`)
      process.stdout.write(`\n${f}${lag ? ' LAG' : ''} p ${player.gs} ${player.x / 256},${player.y / 256} hp ${player.hp}` + (e ? ` | e ${e.x / 256},${e.y / 256} d${e.facing} ai ${e.ai.describe()} anim ${e.anim?.name}/${e.ea.pos.frame}` : '') + ` rng ${(rng.getState()[0] >>> 0).toString(16)} ${objs.join(' ')}`)
      for (const h of rng.history) process.stdout.write(`\n    ${h.fn}(${h.arg}) = ${h.result} ${h.label}`)
    }
    rng.history = []
  })
})
