// "Agent programs a policy" mode: a small JS function (obs, mem) => buttons runs every frame inside node:vm.
// Isolation: a fresh context with code generation from strings disabled; the observation crosses the boundary as a
// JSON string parsed *inside* the context and the result comes back as a JSON string, so no host object (and no host
// Function constructor) is ever reachable from the sandbox. Each call has a wall-clock timeout. node:vm is not a
// hard security boundary - fine for a local prototype, not for untrusted multi-tenant use.
import vm from 'node:vm'
import { parseButtons, type Mask, type Session } from './session'
import { observe } from './observe'
import type { Policy } from './macros'

const PRELUDE = `
const BTN = { A: 1, B: 2, SELECT: 4, START: 8, RIGHT: 16, LEFT: 32, UP: 64, DOWN: 128, R: 256, L: 512 };
const __logs = [];
function log(...a) { if (__logs.length < 200) __logs.push(a.map((x) => typeof x === 'string' ? x : JSON.stringify(x)).join(' ')); }
function toward(from, to, dead) { dead = dead == null ? 2 : dead; const dx = to[0] - from[0], dy = to[1] - from[1]; const k = [];
  if (dx > dead) k.push('RIGHT'); else if (dx < -dead) k.push('LEFT'); if (dy > dead) k.push('DOWN'); else if (dy < -dead) k.push('UP'); return k; }
function dist(a, b) { return Math.hypot(a[0] - b[0], a[1] - b[1]); }
const mem = {};
`

export interface ControllerResult {
  frames: number
  stopped: string
  logs: string[]
  error?: string
  dealt: number
  hp_lost: number
  kills: number
}

export class Controller {
  private ctx: vm.Context
  private stepScript: vm.Script
  constructor(code: string, readonly timeoutMs = 50) {
    this.ctx = vm.createContext({}, { codeGeneration: { strings: false, wasm: false }, name: 'agent-controller' })
    new vm.Script(PRELUDE).runInContext(this.ctx, { timeout: 200 })
    const src = code.trim()
    // accept a function expression / arrow function, or a bare body that uses `obs` / `mem` and returns buttons
    const isFn = /^(async\s+)?(function\b|\(|[A-Za-z_$][\w$]*\s*=>)/.test(src)
    const wrapped = isFn ? `globalThis.__fn = (${src});` : `globalThis.__fn = function (obs, mem) {\n${src}\n};`
    new vm.Script(wrapped, { filename: 'controller.js' }).runInContext(this.ctx, { timeout: 200 })
    this.stepScript = new vm.Script('globalThis.__out = JSON.stringify(__fn(JSON.parse(globalThis.__obs), mem) ?? null)')
  }
  call(obs: unknown): unknown {
    this.ctx.__obs = JSON.stringify(obs)
    this.stepScript.runInContext(this.ctx, { timeout: this.timeoutMs })
    const out = this.ctx.__out as string
    return out === undefined ? null : JSON.parse(out)
  }
  logs(): string[] {
    return JSON.parse(new vm.Script('JSON.stringify(__logs.splice(0))').runInContext(this.ctx, { timeout: 50 }) as string)
  }
}

/** frame-by-frame policy driven by the controller; `null` / 'STOP' from the function ends the run */
export function* controllerPolicy(s: Session, c: Controller, every: number, state: { error?: string; stoppedBy?: string }): Policy {
  let mask: Mask = 0
  for (let f = 0; ; f++) {
    if (f % every === 0) {
      const obs = observe(s, { keepEvents: true })
      let out: unknown
      try {
        out = c.call(obs)
      } catch (e) {
        state.error = (e as Error).message
        return 'controller_error'
      }
      if (out === null || out === 'STOP' || out === false) {
        state.stoppedBy = 'controller'
        return 'controller_stop'
      }
      try {
        mask = parseButtons(out)
      } catch (e) {
        state.error = (e as Error).message
        return 'controller_error'
      }
    }
    yield mask
  }
}
