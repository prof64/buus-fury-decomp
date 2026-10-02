// GBA button input: keyboard + Gamepad API mapped to the 10 GBA keys, sampled once per simulation frame.
// Bit order is the hardware KEYINPUT order (same as tools/emu KEY), so rulesets can port input code 1:1.

export const BTN = {
  A: 1 << 0,
  B: 1 << 1,
  SELECT: 1 << 2,
  START: 1 << 3,
  RIGHT: 1 << 4,
  LEFT: 1 << 5,
  UP: 1 << 6,
  DOWN: 1 << 7,
  R: 1 << 8,
  L: 1 << 9,
} as const
export type ButtonName = keyof typeof BTN
export const DPAD = BTN.RIGHT | BTN.LEFT | BTN.UP | BTN.DOWN

/** Per-frame view of the buttons (what the game reads from its input object). */
export interface InputFrame {
  held: number
  pressed: number
  released: number
}

/** mGBA's default keyboard layout, so it feels like an emulator. Users can rebind (saved in localStorage). */
export const DEFAULT_KEYMAP: Record<string, ButtonName> = {
  ArrowUp: 'UP',
  ArrowDown: 'DOWN',
  ArrowLeft: 'LEFT',
  ArrowRight: 'RIGHT',
  KeyX: 'A',
  KeyZ: 'B',
  KeyA: 'L',
  KeyS: 'R',
  Enter: 'START',
  Backspace: 'SELECT',
}

export const BUTTON_ORDER: ButtonName[] = ['UP', 'DOWN', 'LEFT', 'RIGHT', 'A', 'B', 'L', 'R', 'START', 'SELECT']

/** Codes the battle view keeps for its own hotkeys; never bindable. */
export const RESERVED_CODES = ['Escape', 'F1', 'F2', 'F3', 'Period', 'Backquote']

const KEYMAP_STORAGE = 'bt-keymap-v2'

/** The active keymap: the user's saved bindings, else the mGBA defaults. */
export function loadKeymap(): Record<string, ButtonName> {
  try {
    const saved = JSON.parse(localStorage.getItem(KEYMAP_STORAGE) ?? 'null')
    if (saved && typeof saved === 'object') return saved
  } catch {
    /* storage unavailable */
  }
  return { ...DEFAULT_KEYMAP }
}

/** Saves bindings (null restores defaults) and tells live InputSources to reload them. */
export function saveKeymap(map: Record<string, ButtonName> | null) {
  try {
    if (map) localStorage.setItem(KEYMAP_STORAGE, JSON.stringify(map))
    else localStorage.removeItem(KEYMAP_STORAGE)
  } catch {
    /* storage unavailable */
  }
  window.dispatchEvent(new Event('bt-keymap'))
}

export function keyName(code: string): string {
  const names: Record<string, string> = {
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Backspace: 'Backspace',
    Enter: 'Enter',
    Space: 'Space',
    ShiftLeft: 'L-Shift',
    ShiftRight: 'R-Shift',
  }
  return names[code] ?? code.replace(/^(Key|Digit)/, '')
}

/** Human-readable control help (generic part; rulesets add what each button does). */
export function keysFor(btn: ButtonName, map: Record<string, ButtonName> = loadKeymap()): string[] {
  return Object.entries(map)
    .filter(([, b]) => b === btn)
    .map(([k]) => keyName(k))
}

/**
 * Collects keyboard/gamepad state between frames. `sample()` is called once per fixed simulation step and
 * returns held/pressed/released edges. Taps shorter than a frame are latched so they are never lost.
 */
export class InputSource {
  private live = 0
  private latched = 0
  private prev = 0
  private injected = 0
  private injectedOnce = 0
  keymap: Record<string, ButtonName>
  enabled = true
  gamepadIndex: number | null = null
  private onKeyDown = (e: KeyboardEvent) => this.key(e, true)
  private onKeyUp = (e: KeyboardEvent) => this.key(e, false)
  private onBlur = () => {
    this.live = 0
  }

  private onKeymap = () => {
    this.keymap = loadKeymap()
    this.live = 0
  }

  constructor(keymap = loadKeymap()) {
    this.keymap = keymap
  }

  attach(target: Window = window) {
    target.addEventListener('keydown', this.onKeyDown)
    target.addEventListener('keyup', this.onKeyUp)
    target.addEventListener('blur', this.onBlur)
    target.addEventListener('bt-keymap', this.onKeymap)
  }
  detach(target: Window = window) {
    target.removeEventListener('keydown', this.onKeyDown)
    target.removeEventListener('keyup', this.onKeyUp)
    target.removeEventListener('blur', this.onBlur)
    target.removeEventListener('bt-keymap', this.onKeymap)
  }

  private key(e: KeyboardEvent, down: boolean) {
    if (!this.enabled) return
    const t = e.target as HTMLElement | null
    if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return
    const b = this.keymap[e.code]
    if (!b) return
    e.preventDefault()
    const bit = BTN[b]
    if (down) {
      this.live |= bit
      this.latched |= bit
    } else this.live &= ~bit
  }

  /** Programmatic input (tests, automation): hold buttons until changed. */
  setInjected(mask: number) {
    this.injected = mask
  }
  /** Programmatic tap: pressed for exactly one sampled frame. */
  tap(mask: number) {
    this.injectedOnce |= mask
  }

  private gamepad(): number {
    if (typeof navigator === 'undefined' || !navigator.getGamepads) return 0
    let m = 0
    for (const gp of navigator.getGamepads()) {
      if (!gp || !gp.connected) continue
      if (this.gamepadIndex == null) this.gamepadIndex = gp.index
      const b = (i: number) => !!gp.buttons[i]?.pressed
      if (b(1)) m |= BTN.A // east
      if (b(0)) m |= BTN.B // south
      if (b(2)) m |= BTN.B
      if (b(3)) m |= BTN.A
      if (b(4) || b(6)) m |= BTN.L
      if (b(5) || b(7)) m |= BTN.R
      if (b(8)) m |= BTN.SELECT
      if (b(9)) m |= BTN.START
      if (b(12)) m |= BTN.UP
      if (b(13)) m |= BTN.DOWN
      if (b(14)) m |= BTN.LEFT
      if (b(15)) m |= BTN.RIGHT
      const [ax, ay] = [gp.axes[0] ?? 0, gp.axes[1] ?? 0]
      if (ax < -0.5) m |= BTN.LEFT
      if (ax > 0.5) m |= BTN.RIGHT
      if (ay < -0.5) m |= BTN.UP
      if (ay > 0.5) m |= BTN.DOWN
    }
    return m
  }

  sample(): InputFrame {
    const held = (this.live | this.latched | this.injected | this.injectedOnce | this.gamepad()) & 0x3ff
    this.latched = 0
    this.injectedOnce = 0
    const f = { held, pressed: held & ~this.prev, released: this.prev & ~held }
    this.prev = held
    return f
  }

  reset() {
    this.live = this.latched = this.prev = this.injected = this.injectedOnce = 0
  }
}
