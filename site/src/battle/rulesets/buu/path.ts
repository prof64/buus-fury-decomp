// Buu's Fury enemy path following (tools/battle/COLLISION.md §4), shared by ChasePlayer (0x0802A09F), MoveToPoint
// (0x0802E105) and WanderToRandomPoint (0x0803BD65). The path object (0x24 bytes): +4 speed, +8 line budget (50),
// +0x0C/+0x10 waypoint (24.8), +0x14 valid, +0x18 re-plan counter, +0x1C/+0x20 goal (24.8).
//   enter 0x0803BB00: counter = global 0x030032E5 (then global = +1 & 7: enemies re-plan on different frames); plan
//   update 0x0803BB48: when the counter is 0 re-plan; counter = (counter + 7) & 7 (every 8th frame); step
//   plan 0x0803B6EA: walk a Bresenham line from the enemy toward the goal one pixel at a time for up to 49 px (budget
//     50); clear -> waypoint = goal; blocked -> wall-follow from the last free point
//   wall-follow 0x0803B4FC: two probes slide along the wall (one turning left, one right), each up to 256 one-pixel
//     steps, until the original direction is free again; waypoint = the probe that got free (both / neither: the one
//     nearer to the goal)
//   step 0x0803B934: face the waypoint on the dominant axis, velocity = d * speed / max(|dx|, |dy|) (pixel deltas of
//     the 24.8 positions), try the full move, else only x, else only y
// Every position test is the enemy's own grid test (vfunc +0x54, the enemy's marks lifted while planning / stepping).

import type { Enemy } from './enemy'

const tdiv = (a: number, b: number) => (b ? Math.trunc(a / b) : 0)

export class PathMover {
  speed: number
  budget = 50
  /** waypoint (24.8) */
  wx = 0
  wy = 0
  valid = 0
  counter = 0
  /** goal (24.8) */
  gx = 0
  gy = 0
  constructor(speed: number) {
    this.speed = speed
  }

  private ok(e: Enemy, x: number, y: number): boolean {
    return e.fits(e.w, x >> 8, y >> 8)
  }

  /** 0x0803BB00 */
  enter(e: Enemy) {
    const ctx = e.ctx
    this.counter = ctx.pathStagger
    ctx.pathStagger = (ctx.pathStagger + 1) & 7
    this.replan(e)
  }
  private replan(e: Enemy) {
    e.unmarkSelf()
    const r = this.plan(e)
    e.markSelf()
    if (!r) this.valid = 0
  }
  /** 0x0803BB48 */
  update(e: Enemy) {
    if (this.counter === 0) this.replan(e)
    this.counter = (this.counter + 7) & 7
    this.step(e)
  }

  /** 0x0803B6EA */
  plan(e: Enemy): boolean {
    const ex = e.x
    const ey = e.y
    const adx = Math.abs(this.gx - ex)
    const ady = Math.abs(this.gy - ey)
    let err: number
    let errStraight: number
    let errDiag: number
    let major: number
    let sx: number
    let sy: number
    let gxs = 256
    let gys = 256
    if (adx >= ady) {
      const mn = ady >> 8
      const mj = adx >> 8
      major = mj + 1
      err = 2 * mn - mj
      errStraight = 2 * mn
      errDiag = (mn - mj) * 2
      sx = 256
      sy = 0
    } else {
      const mn = adx >> 8
      const mj = ady >> 8
      major = mj + 1
      err = 2 * mn - mj
      errStraight = 2 * mn
      errDiag = (mn - mj) * 2
      sx = 0
      sy = 256
    }
    if (ex > this.gx) {
      gxs = -256
      sx = -sx
    }
    if (ey > this.gy) {
      gys = -256
      sy = -sy
    }
    let steps = this.budget
    if (major < steps) steps = major - 1
    let px = ex
    let py = ey
    for (let n = steps - 1; n > 0; n--) {
      if (err < 0) {
        err += errStraight
        px += sx
        py += sy
        if (!this.ok(e, px, py)) return this.wallFollow(e, px - sx, py - sy, sx, sy)
      } else {
        err += errDiag
        px += gxs
        py += gys
        if (this.ok(e, px, py)) continue
        px -= gxs
        py -= gys
        // the minor-axis part of the diagonal
        const ax = sx === gxs ? 0 : gxs
        const ay = sx === gxs ? gys : 0
        px += sx
        py += sy
        if (!this.ok(e, px, py)) return this.wallFollow(e, px - sx, py - sy, sx, sy)
        px += ax
        py += ay
        if (!this.ok(e, px, py)) return this.wallFollow(e, px - gxs, py - gys, ax, ay)
      }
    }
    this.wx = this.gx
    this.wy = this.gy
    this.valid = 1
    return true
  }

  /** 0x0803B4FC: probes from (x, y) blocked in direction (dx, dy) */
  wallFollow(e: Enemy, x: number, y: number, dx: number, dy: number): boolean {
    const probe = (side: [number, number]): { x: number; y: number; free: boolean } => {
      let ax = x
      let ay = y
      let s = side
      let f: [number, number] = [dx, dy]
      for (let k = 255; k >= 0; k--) {
        ax += s[0]
        ay += s[1]
        if (!this.ok(e, ax, ay)) {
          ax -= s[0]
          ay -= s[1]
          const old = s
          s = [-f[0], -f[1]]
          f = old
          continue
        }
        if (this.ok(e, ax + f[0], ay + f[1])) return { x: ax, y: ay, free: true }
      }
      return { x: ax, y: ay, free: false }
    }
    const a = probe([-dy, dx])
    const b = probe([dy, -dx])
    let w = a
    if (a.free && !b.free) w = a
    else if (b.free && !a.free) w = b
    else {
      const d2 = (p: { x: number; y: number }) => {
        const qx = (this.gx - p.x) >> 8
        const qy = (this.gy - p.y) >> 8
        return qx * qx + qy * qy
      }
      w = d2(a) < d2(b) ? a : b
    }
    this.wx = w.x
    this.wy = w.y
    this.valid = 1
    return a.free || b.free
  }

  /** 0x0803B934 */
  step(e: Enemy) {
    const dx = (this.wx >> 8) - (e.x >> 8)
    const dy = (this.wy >> 8) - (e.y >> 8)
    const adx = Math.abs(dx)
    const ady = Math.abs(dy)
    let m: number
    if (adx > ady) {
      m = adx
      e.facing = dx > 0 ? 3 : 2
    } else {
      m = ady
      e.facing = dy > 0 ? 0 : 1
    }
    if (!m) m = 1
    e.moving = true
    const vx = tdiv((dx * this.speed) << 8, m << 8)
    const vy = tdiv((dy * this.speed) << 8, m << 8)
    e.tryMove(vx, vy)
  }
}
