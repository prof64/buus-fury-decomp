// In-process smoke test of the agent API (no MCP transport): node dist/scripts/selftest.mjs
// Plays the two validation fights with the macros and the bundled controllers, checks the replay hash and that
// screenshots do not perturb the simulation.
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import * as G from '../src/game'

const here = resolve(dirname(fileURLToPath(import.meta.url)), '../../scripts')
const code = (f: string) => readFileSync(resolve(here, f), 'utf8')
let fail = 0
const check = (name: string, ok: boolean, info: unknown) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name} ${JSON.stringify(info)}`)
  if (!ok) fail++
}

// dash covers ~2.6 px/frame at Lv60
{
  const s = G.newBattle({ fighter: 'goku', level: 60, enemy: 't68', arena: 'forest' }).session
  G.moveTo(s, 200, 470, 600, [])
  const o = G.doObserve(s, {}) as { player: { pos: number[] }; walls: Record<string, number> }
  // dash where there is room (dashing into a wall bumps you back)
  const dir = Object.entries(o.walls).sort((a, b) => b[1] - a[1])[0][0]
  G.dash(s, dir, 20)
  const p1 = (G.doObserve(s, {}).player as { pos: number[] }).pos
  const moved = Math.hypot(p1[0] - o.player.pos[0], p1[1] - o.player.pos[1])
  check(`dash ${dir} moves > 40 px in 20 frames`, moved > 40, { moved })
}

// regular group: 4 Majin Fighters, Lv60 Goku, macros only
{
  const s = G.newBattle({ fighter: 'goku', level: 60, preset: 'majin_fighters' }).session
  let r: { result: { stopped: string } } | null = null
  for (let i = 0; i < 8 && !r?.result.stopped.startsWith('battle_'); i++) r = G.attack(s, undefined, undefined, 1500)
  check('majin fighters (attack macro)', r!.result.stopped === 'battle_won', r!.result)
}

// same fight with the group controller, then replay determinism
{
  const s = G.newBattle({ fighter: 'goku', level: 60, preset: 'majin_fighters' }).session
  const r = G.runController(s, { code: code('ctl_group.js'), frames: 5000 })
  check('majin fighters (controller)', r.result.stopped === 'battle_won', r.result)
  const rp = G.playReplay(G.getReplay(s), false)
  check('replay deterministic', rp.deterministic, rp)
}

// boss: Janemba -> Super Janemba at Lv110: macros for phase 1 (with a rewind / save branch), controller for phase 2
{
  const s = G.newBattle({ fighter: 'goku', level: 110, preset: 'janemba' }).session
  G.saveState(s, 'start')
  G.attack(s, undefined, undefined, 400)
  G.rewind(s, 100)
  G.doScreenshot(s, 'wide', 1, true)
  G.attack(s, undefined, undefined, 4000, ['phase'])
  G.wait(s, 300, ['phase'])
  const r = G.runController(s, { code: code('ctl_melee.js'), frames: 6000 })
  check('janemba preset (macros + controller)', r.result.stopped === 'battle_won', r.result)
  const rp = G.playReplay(G.getReplay(s), false)
  check('replay deterministic after rewind + screenshot', rp.deterministic, rp)
}
process.exit(fail ? 1 : 0)
