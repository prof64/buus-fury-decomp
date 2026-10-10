// Ad-hoc driver (only with BUU_DEBUG=1): BUU_T enemy id, BUU_A arena, BUU_D start distance, BUU_N frames,
// BUU_S print step, BUU_K key mode (none | mash | blast | beam), BUU_F fighter, BUU_L level, BUU_GOD = 99999 HP
import { it } from 'vitest'
import { buuData, fighter, makeBuuSim } from './buu_harness'
import { BTN } from '../../src/battle/core/input'

it.skipIf(!process.env.BUU_DEBUG)('debug', () => {
  if (process.env.BUU_NOSPRITE) {
    console.log(buuData().enemies.filter((e) => !e.sprite).map((e) => `0x${e.type.toString(16)} ${e.name} hp ${e.hp}`).join('\n'))
    return
  }
  const t = process.env.BUU_T ?? 't68'
  const s = makeBuuSim({ enemy: t, arena: process.env.BUU_A ?? 'crater', fighter: fighter({ level: Number(process.env.BUU_L ?? 120), fighter: process.env.BUU_F ?? 'goku', form: Number(process.env.BUU_FORM ?? 2), options: { ...fighter().options, autoSpend: 'even' } }) })
  if (process.env.BUU_GOD) s.player.rec.hp = s.player.rec.maxHp = 99999
  if (process.env.BUU_D) {
    const e = s.enemies[0]
    s.player.x = e.x - Number(process.env.BUU_D) * 256
    s.player.y = e.y
  }
  const N = Number(process.env.BUU_N ?? 600)
  const S = Number(process.env.BUU_S ?? 30)
  const mode = process.env.BUU_K ?? 'none'
  const keys = (f: number) => {
    const p = s.player
    const e = s.enemies.find((x) => x.primary && !x.defeated)
    if (!e) return 0
    const [dx, dy] = [e.center()[0] - p.center()[0], e.center()[1] - p.center()[1]]
    let k = 0
    if (mode === 'mash') {
      if (Math.abs(dx) > 30) k |= dx > 0 ? BTN.RIGHT : BTN.LEFT
      else if (Math.abs(dy) > 6) k |= dy > 0 ? BTN.DOWN : BTN.UP
      if (Math.abs(dx) <= 40 && f % 6 < 2) k |= BTN.A
    } else if (mode === 'blast') {
      if (Math.abs(dy) > 4) k |= dy > 0 ? BTN.DOWN : BTN.UP
      else if (f % 10 < 2) k |= BTN.B
    } else if (mode === 'beam') {
      if (Math.abs(dy) > 4 && f % 200 < 20) k |= dy > 0 ? BTN.DOWN : BTN.UP
      else k |= BTN.B
    }
    return k
  }
  let f0 = 0
  for (let f = 0; f < N; f += S) {
    s.run(S, (i) => keys(f0 + i))
    f0 += S
    console.log(s.battle.debugLines(s.w).slice(1).join('\n'))
  }
  console.log(s.w.log.slice(-Number(process.env.BUU_LOG ?? 30)).map((e) => e.frame + ' ' + e.text).join('\n'))
  console.log('outcome', s.battle.outcome(s.w), 'dealt', s.w.stats.damageDealt, 'taken', s.w.stats.damageTaken)
})
