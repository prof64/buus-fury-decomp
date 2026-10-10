import { useEffect, useRef, useState } from 'react'
import type { BattleConfig, Outcome, Ruleset } from '../core/ruleset'
import { BattleSession } from '../core/session'
import { keysFor } from '../core/input'
import { KeyBindings, useKeymapVersion } from './KeyBindings'
import type { ViewMode } from '../core/renderer'
import { attachBattleAudio } from '../audio/director'
import { AudioControl } from '../audio/AudioControl'

declare global {
  interface Window {
    __battle?: BattleSession
  }
}

/** view choices: PC view (window-sized, auto or fixed integer zoom), the classic 240x160 screen, legacy widescreen */
const VIEWS: { id: string; label: string; mode: ViewMode }[] = [
  { id: 'auto', label: 'PC · auto zoom', mode: { kind: 'pc', zoom: 0, widescreen: false, scale: 0 } },
  { id: '1', label: 'PC · 1×', mode: { kind: 'pc', zoom: 1, widescreen: false, scale: 0 } },
  { id: '2', label: 'PC · 2×', mode: { kind: 'pc', zoom: 2, widescreen: false, scale: 0 } },
  { id: '3', label: 'PC · 3×', mode: { kind: 'pc', zoom: 3, widescreen: false, scale: 0 } },
  { id: '4', label: 'PC · 4×', mode: { kind: 'pc', zoom: 4, widescreen: false, scale: 0 } },
  { id: 'gba', label: 'GBA 240×160', mode: { kind: 'gba', widescreen: false, scale: 0 } },
  { id: 'wide', label: 'GBA widescreen', mode: { kind: 'wide', widescreen: true, scale: 0 } },
]
const viewKey = (game: string) => 'bt-view-' + game
function loadView(game: string): string {
  try {
    const v = localStorage.getItem(viewKey(game))
    if (v && VIEWS.some((x) => x.id === v)) return v
    // Buu's Fury defaults to the PC view; the other games keep their previous GBA / widescreen choice
    if (game === 'buu') return 'auto'
    return localStorage.getItem('bt-wide') === '1' ? 'wide' : 'gba'
  } catch {
    return game === 'buu' ? 'auto' : 'gba'
  }
}
const modeOf = (id: string): ViewMode => ({ ...(VIEWS.find((v) => v.id === id) ?? VIEWS[0]).mode })

export function BattleView({ rs, cfg, onEnd, onQuit }: { rs: Ruleset; cfg: BattleConfig; onEnd: (o: Outcome, s: BattleSession) => void; onQuit: () => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const wrapRef = useRef<HTMLDivElement>(null)
  const sessRef = useRef<BattleSession | null>(null)
  useKeymapVersion()
  const [view, setView] = useState(() => loadView(cfg.game))
  const [debug, setDebug] = useState(() => localStorage.getItem('bt-debug') === '1')
  const [boxes, setBoxes] = useState(false)
  const [labels, setLabels] = useState(false)
  const [showKeys, setShowKeys] = useState(false)
  const [speed, setSpeed] = useState(1)
  const [paused, setPaused] = useState(false)
  const [, setTick] = useState(0)

  useEffect(() => {
    const s = new BattleSession(rs, cfg, canvasRef.current!, { onEnd })
    sessRef.current = s
    window.__battle = s
    const fit = () => {
      const el = wrapRef.current
      if (!el) return
      s.view = modeOf(loadView(cfg.game))
      s.fit(el.clientWidth, el.clientHeight)
    }
    fit()
    const ro = new ResizeObserver(fit)
    ro.observe(wrapRef.current!)
    s.start()
    const detachAudio = attachBattleAudio(s)
    const iv = setInterval(() => {
      setTick((t) => t + 1)
      setPaused(s.paused)
    }, 150)
    return () => {
      ro.disconnect()
      clearInterval(iv)
      s.stop()
      detachAudio()
      if (window.__battle === s) window.__battle = undefined
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const s = sessRef.current
    if (!s) return
    try {
      localStorage.setItem(viewKey(cfg.game), view)
    } catch {
      /* storage unavailable: keep the choice for this battle only */
    }
    s.view = modeOf(view)
    const el = wrapRef.current!
    s.fit(el.clientWidth, el.clientHeight)
  }, [view, cfg.game])
  useEffect(() => {
    const s = sessRef.current
    if (!s) return
    localStorage.setItem('bt-debug', debug ? '1' : '0')
    s.world.debug.hitboxes = boxes
    s.world.debug.labels = labels
  }, [debug, boxes, labels])
  useEffect(() => {
    if (sessRef.current) sessRef.current.loop.speed = speed
  }, [speed])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = sessRef.current
      if (!s) return
      if (e.code === 'Backquote' || e.code === 'F1') {
        e.preventDefault()
        setDebug((d) => !d)
      } else if (e.code === 'F2') {
        e.preventDefault()
        setBoxes((b) => !b)
      } else if (e.code === 'F3') {
        e.preventDefault()
        setLabels((b) => !b)
      }
      else if (e.code === 'Period' && s.paused) {
        s.setPaused(false)
        s.step()
        s.setPaused(true)
      } else if (e.code === 'Escape') onQuit()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onQuit])

  const s = sessRef.current
  return (
    <div className={'bt-battle' + (debug ? ' with-debug' : '')}>
      <div className="bt-toolbar">
        <button onClick={onQuit}>◀ Setup</button>
        <button onClick={() => s && (s.setPaused(!s.paused), setPaused(!s.paused))}>{paused ? '▶ Resume' : '❚❚ Pause'}</button>
        <label title="PC view: the arena at the game's pixel size, as much of it as fits the window (auto picks the zoom so the arena fills the view). GBA: the original 240×160 screen.">
          View{' '}
          <select value={view} onChange={(e) => setView(e.target.value)}>
            {VIEWS.map((v) => (
              <option key={v.id} value={v.id}>
                {v.label}
              </option>
            ))}
          </select>
          {s && s.renderer.zoom > 0 && <span className="muted small"> {s.renderer.viewW}×{s.renderer.viewH} @{s.renderer.scale}×</span>}
        </label>
        <label>
          Speed{' '}
          <select value={speed} onChange={(e) => setSpeed(+e.target.value)}>
            <option value={0.25}>¼×</option>
            <option value={0.5}>½×</option>
            <option value={1}>1×</option>
            <option value={2}>2×</option>
          </select>
        </label>
        <label>
          <input type="checkbox" checked={debug} onChange={(e) => setDebug(e.target.checked)} /> Debug panel <kbd>F1</kbd>
        </label>
        <label>
          <input type="checkbox" checked={boxes} onChange={(e) => setBoxes(e.target.checked)} /> Hitboxes <kbd>F2</kbd>
        </label>
        <label>
          <input type="checkbox" checked={labels} onChange={(e) => setLabels(e.target.checked)} /> States/AI <kbd>F3</kbd>
        </label>
        <button onClick={() => setShowKeys((k) => !k)}>⌨ Keys</button>
        <AudioControl />
        <span className="muted small">
          A {keysFor('A').join('/')} · B {keysFor('B').join('/')} · L {keysFor('L').join('/')} · R {keysFor('R').join('/')} · Start {keysFor('START').join('/')} · Select {keysFor('SELECT').join('/')}
        </span>
      </div>
      {showKeys && (
        <div className="bt-keys-pop">
          <KeyBindings />
        </div>
      )}
      <div className="bt-main">
        <div className="bt-canvas-wrap" ref={wrapRef}>
          <canvas ref={canvasRef} className="bt-canvas" />
        </div>
        {debug && s && <DebugPanel s={s} />}
      </div>
    </div>
  )
}

function DebugPanel({ s }: { s: BattleSession }) {
  const w = s.world
  const lines = s.battle.debugLines(w)
  const rng = w.rng.history.slice(-16).reverse()
  const log = w.log.slice(-40).reverse()
  return (
    <aside className="bt-debug">
      <h4>State</h4>
      <pre>{lines.join('\n')}</pre>
      <h4>RNG calls (newest first)</h4>
      <pre>
        {rng.map((c) => `${c.frame.toString().padStart(6)} ${c.fn}(${c.arg}) = ${c.result}  ${c.label}`).join('\n')}
      </pre>
      <h4>Damage / events</h4>
      <pre>{log.map((e) => `${e.frame.toString().padStart(6)} ${e.text}`).join('\n')}</pre>
    </aside>
  )
}
