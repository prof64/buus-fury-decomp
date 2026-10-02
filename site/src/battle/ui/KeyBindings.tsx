import { useEffect, useState } from 'react'
import { BUTTON_ORDER, keyName, loadKeymap, RESERVED_CODES, saveKeymap, type ButtonName } from '../core/input'

const LABEL: Record<ButtonName, string> = {
  UP: 'Up',
  DOWN: 'Down',
  LEFT: 'Left',
  RIGHT: 'Right',
  A: 'A',
  B: 'B',
  L: 'L',
  R: 'R',
  START: 'Start',
  SELECT: 'Select',
}

/** Re-renders the caller whenever the bindings change (for control help text). */
export function useKeymapVersion() {
  const [v, setV] = useState(0)
  useEffect(() => {
    const bump = () => setV((x) => x + 1)
    window.addEventListener('bt-keymap', bump)
    return () => window.removeEventListener('bt-keymap', bump)
  }, [])
  return v
}

/** Emulator-style key binding table: click a button, press a key. */
export function KeyBindings() {
  const [map, setMap] = useState(loadKeymap)
  const [waiting, setWaiting] = useState<ButtonName | null>(null)

  useEffect(() => {
    const reload = () => setMap(loadKeymap())
    window.addEventListener('bt-keymap', reload)
    return () => window.removeEventListener('bt-keymap', reload)
  }, [])

  useEffect(() => {
    if (!waiting) return
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()
      if (e.code === 'Escape') return setWaiting(null)
      if (RESERVED_CODES.includes(e.code)) return
      // one key per button: drop this button's old keys and anything else on the new key
      const next = Object.fromEntries(Object.entries(map).filter(([k, b]) => b !== waiting && k !== e.code)) as Record<string, ButtonName>
      next[e.code] = waiting
      saveKeymap(next)
      setWaiting(null)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [waiting, map])

  return (
    <div className="bt-keys">
      <table>
        <tbody>
          {BUTTON_ORDER.map((b) => {
            const keys = Object.entries(map)
              .filter(([, x]) => x === b)
              .map(([k]) => keyName(k))
            return (
              <tr key={b}>
                <td>{LABEL[b]}</td>
                <td>
                  <button className={'bt-keycap' + (waiting === b ? ' waiting' : '')} onClick={() => setWaiting(waiting === b ? null : b)}>
                    {waiting === b ? 'press a key…' : keys.join(' / ') || '—'}
                  </button>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      <div className="bt-keys-foot">
        <button onClick={() => saveKeymap(null)}>Reset to mGBA defaults</button>
        <span className="muted small">Gamepads work too. Hotkeys: F1 debug · F2 hitboxes · F3 labels · . frame-step · Esc quit</span>
      </div>
    </div>
  )
}
