(obs, mem) => {
  const p = obs.player
  const es = obs.enemies
  if (!es.length) return ''
  const dirOf = (e) => {
    const dx = e.pos[0] - p.pos[0], dy = e.pos[1] - p.pos[1]
    return Math.abs(dx) >= Math.abs(dy) ? (dx > 0 ? 'RIGHT' : 'LEFT') : dy > 0 ? 'DOWN' : 'UP'
  }
  // 1. punch whenever something is in range (the punch box hits every enemy inside it)
  const hit = es.find((e) => e.melee === 'in_range')
  if (hit && p.can_act) {
    mem.a = !mem.a
    return mem.a ? 'A' : ''
  }
  // 2. an adjacent enemy winding up a melee attack: face it and block (1/16 damage from the front)
  const attacker = es.find((e) => e.dist < 34 && /Melee|Strike|Attack/.test(e.ai) && !/IfPlayerAttacking/.test(e.ai))
  if (attacker && mem.block !== false) {
    const d = dirOf(attacker)
    if (p.state === 'block') return 'R'
    if (p.facing.toUpperCase() !== d) return d
    return 'R'
  }
  // 3. turn toward something reachable by turning
  const turn = es.find((e) => e.melee && e.melee.startsWith('turn_'))
  if (turn) return turn.melee.slice(5).toUpperCase()
  // 4. approach the nearest one from the side, on its row
  const e = es[0]
  const side = p.pos[0] < e.pos[0] ? -1 : 1
  return toward(p.pos, [e.pos[0] + side * 14, e.pos[1]], 2).join('+')
}
