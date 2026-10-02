// Ties everything together for one fight: World + ruleset battle + fixed loop + renderer + input.

import { BTN, InputSource } from './input'
import { FixedLoop, GBA_HZ } from './loop'
import { Renderer, type ViewMode } from './renderer'
import type { BattleConfig, Outcome, Ruleset, RulesetBattle } from './ruleset'
import { World } from './world'
import { drawText, textWidth } from './pixelfont'

export interface SessionEvents {
  onEnd?: (outcome: Outcome, s: BattleSession) => void
}

export class BattleSession {
  readonly world: World
  readonly battle: RulesetBattle
  readonly renderer: Renderer
  readonly loop: FixedLoop
  readonly input: InputSource
  outcome: Outcome = 'running'
  /** frames shown after the outcome before onEnd fires */
  private endDelay = 0
  paused = false
  view: ViewMode = { widescreen: false, scale: 0 }
  private ended = false

  readonly ruleset: Ruleset
  readonly cfg: BattleConfig
  private events: SessionEvents
  constructor(ruleset: Ruleset, cfg: BattleConfig, canvas: HTMLCanvasElement, events: SessionEvents = {}) {
    this.ruleset = ruleset
    this.cfg = cfg
    this.events = events
    const rng = ruleset.createRng(cfg.seed)
    this.world = new World(rng, ruleset.arenaRuntime(cfg.arena))
    this.battle = ruleset.createBattle(cfg, this.world)
    this.renderer = new Renderer(canvas)
    this.input = new InputSource()
    this.loop = new FixedLoop(
      () => this.step(),
      () => this.render(),
      GBA_HZ,
    )
  }

  start() {
    this.input.attach()
    this.loop.start()
  }

  stop() {
    this.loop.stop()
    this.input.detach()
  }

  setPaused(p: boolean) {
    this.paused = p
    this.loop.paused = p
  }

  /** one simulation frame */
  step() {
    const inp = this.input.sample()
    if (inp.pressed & BTN.START && this.outcome === 'running') {
      this.setPaused(!this.paused)
      return
    }
    if (this.paused) return
    this.battle.preStep?.(this.world, inp)
    this.world.step(inp)
    this.battle.postStep?.(this.world)
    if (this.outcome === 'running') {
      this.outcome = this.battle.outcome(this.world)
      if (this.outcome !== 'running') this.endDelay = 150
    } else if (!this.ended) {
      if (--this.endDelay <= 0) {
        this.ended = true
        this.events.onEnd?.(this.outcome, this)
      }
    }
  }

  /** run n frames synchronously (tests / automation) */
  runFrames(n: number) {
    for (let i = 0; i < n; i++) this.step()
  }

  fit(w: number, h: number) {
    this.renderer.fit(w, h, this.view)
  }

  render() {
    const r = this.renderer
    this.world.draw(r)
    this.battle.drawHud(r, this.world)
    const c = r.ctx
    if (this.paused) {
      c.fillStyle = 'rgba(0,0,0,0.45)'
      c.fillRect(0, 0, r.viewW, r.viewH)
      const t = 'PAUSED'
      drawText(c, t, (r.viewW - textWidth(t, 2)) >> 1, (r.viewH >> 1) - 5, '#ffd23f', '#000', 2)
    }
    if (this.outcome !== 'running') {
      const t = this.outcome === 'won' ? 'YOU WIN!' : 'YOU HAVE DIED!'
      drawText(c, t, (r.viewW - textWidth(t, 2)) >> 1, (r.viewH >> 1) - 20, this.outcome === 'won' ? '#ffd23f' : '#ff5050', '#000', 2)
    }
    r.endFrame()
    this.world.drawOverlay(r)
  }
}
