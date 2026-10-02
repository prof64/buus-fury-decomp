// Ad-hoc bot fights (only with BUU_BOT=1): a simple melee / blast bot against bosses, prints outcomes.
import { it } from 'vitest'
import { fighter, makeBuuSim } from './buu_harness'
import { BTN } from '../../src/battle/core/input'

const LIST = (process.env.BUU_LIST ?? 't10:volcano t18:demon t116:crater2 t60:wasteland t70:canyon t122:hfil t51:hfil t28:crater t129:crater t0:forest t145:yakon t119:wasteland').split(' ')

it.skipIf(!process.env.BUU_BOT)('bot fights', () => {
  for (const item of LIST) {
    const [id, arena] = item.split(':')
    const s = makeBuuSim({ enemy: id, arena, seed: 3, fighter: fighter({ level: Number(process.env.BUU_L ?? 150), fighter: process.env.BUU_F ?? 'vegito', form: Number(process.env.BUU_FORM ?? 39), options: { ...fighter().options, autoSpend: 'even', skillLevel: '2' } }) })
    const mode = process.env.BUU_K ?? 'mash'
    let f = 0
    const N = Number(process.env.BUU_N ?? 7200)
    for (; f < N && s.battle.outcome(s.w) === 'running'; f++) {
      s.run(1, () => {
        const p = s.player
        const e = s.enemies.filter((x) => !x.defeated && !x.part).sort((a, b) => Number(b.primary) - Number(a.primary))[0]
        if (!e) return 0
        const [dx, dy] = [e.center()[0] - p.center()[0], e.center()[1] - p.center()[1]]
        let k = 0
        if (mode === 'mash') {
          if (Math.abs(dy) > 6) k |= dy > 0 ? BTN.DOWN : BTN.UP
          else if (Math.abs(dx) > 26) k |= dx > 0 ? BTN.RIGHT : BTN.LEFT
          if (Math.abs(dx) <= 40 && Math.abs(dy) <= 12 && s.w.frame % 6 < 2) k |= BTN.A
        } else {
          if (Math.abs(dy) > 4) k |= dy > 0 ? BTN.DOWN : BTN.UP
          else if (s.w.frame % 10 < 2) k |= BTN.B
          if (Math.abs(dy) <= 4 && Math.sign(dx) !== (p.facing === 3 ? 1 : p.facing === 2 ? -1 : 0)) k = dx > 0 ? BTN.RIGHT : BTN.LEFT
        }
        return k
      })
    }
    const prim = s.enemies.filter((x) => x.primary)
    console.log(`${id} ${arena}: ${s.battle.outcome(s.w)} after ${f} f; player HP ${s.player.hp}/${s.player.maxHp}; enemy ${prim.map((x) => `${x.label} ${x.hp}/${x.maxHp} ${x.ai.describe()}`).join(' | ')}; dealt ${s.w.stats.damageDealt} taken ${s.w.stats.damageTaken}`)
  }
})
