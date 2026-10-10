(obs, mem) => {
  const p = obs.player
  const e = obs.enemies.filter((x) => !x.optional)[0] ?? obs.enemies[0]
  if (!e) return ''
  // dodge anything hostile that will reach us within 20 frames: step across its path
  const threat = (obs.projectiles || []).find((s) => s.team !== 'yours' && s.eta != null && s.eta < 20)
  if (threat && p.can_act) {
    const [vx, vy] = threat.vel
    if (Math.abs(vx) >= Math.abs(vy) && (vx || vy)) return p.pos[1] > threat.pos[1] ? 'DOWN' : 'UP'
    return p.pos[0] > threat.pos[0] ? 'RIGHT' : 'LEFT'
  }
  if (e.melee === 'in_range') {
    mem.a = !mem.a
    return mem.a ? 'A' : ''
  }
  if (e.melee) return e.melee.slice(5).toUpperCase()
  // walk to a spot beside it on its row
  const side = p.pos[0] < e.pos[0] ? -1 : 1
  return toward(p.pos, [e.pos[0] + side * 14, e.pos[1]], 2).join('+')
}
