// Fixed-timestep loop: the simulation always advances in whole game frames (GBA: 59.7275 Hz), rendering happens
// on requestAnimationFrame. Supports pause, speed multiplier and single-frame advance.

export const GBA_HZ = 16777216 / 280896 // 59.7275...

export class FixedLoop {
  hz: number
  speed = 1
  paused = false
  private acc = 0
  private last = 0
  private raf = 0
  private pendingSteps = 0
  /** frames actually simulated (for an fps readout) */
  simulated = 0

  private step: () => void
  private render: () => void
  constructor(step: () => void, render: () => void, hz = GBA_HZ) {
    this.step = step
    this.render = render
    this.hz = hz
  }

  start() {
    if (this.raf) return
    this.last = performance.now()
    const tick = (now: number) => {
      this.raf = requestAnimationFrame(tick)
      const dt = Math.min(250, now - this.last)
      this.last = now
      if (!this.paused) this.acc += (dt / 1000) * this.hz * this.speed
      let n = Math.floor(this.acc) + this.pendingSteps
      this.acc -= Math.floor(this.acc)
      this.pendingSteps = 0
      if (n > 8) n = 8 // don't spiral after a stall
      for (let i = 0; i < n; i++) {
        this.step()
        this.simulated++
      }
      this.render()
    }
    this.raf = requestAnimationFrame(tick)
  }

  stop() {
    cancelAnimationFrame(this.raf)
    this.raf = 0
  }

  /** advance exactly one frame (while paused) */
  frameAdvance(n = 1) {
    this.pendingSteps += n
  }
}
