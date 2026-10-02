import { useEffect, useRef, useState } from 'react'
import type { BattleConfig, Outcome, Ruleset } from '../core/ruleset'
import { BattleSession } from '../core/session'
import { keysFor } from '../core/input'

declare global {
  interface Window {
    __battle?: BattleSession
  }
}

export function BattleView({ rs, cfg, onEnd, onQuit }: { rs: Ruleset; cfg: BattleConfig; onEnd: (o: Outcome, s: BattleSession) => void; onQuit: () => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const wrapRef = useRef<HTMLDivElement>(null)
  const sessRef = useRef<BattleSession | null>(null)
  const [wide, setWide] = useState(() => localStorage.getItem('bt-wide') === '1')
  const [debug, setDebug] = useState(() => localStorage.getItem('bt-debug') === '1')
  const [boxes, setBoxes] = useState(false)
  const [labels, setLabels] = useState(false)
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
      s.view.widescreen = localStorage.getItem('bt-wide') === '1'
      s.fit(el.clientWidth, el.clientHeight)
    }
    fit()
    const ro = new ResizeObserver(fit)
    ro.observe(wrapRef.current!)
    s.start()
    const iv = setInterval(() => {
      setTick((t) => t + 1)
      setPaused(s.paused)
    }, 150)
    return () => {
      ro.disconnect()
      clearInterval(iv)
      s.stop()
      if (window.__battle === s) window.__battle = undefined
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const s = sessRef.current
    if (!s) return
    localStorage.setItem('bt-wide', wide ? '1' : '0')
    s.view.widescreen = wide
    const el = wrapRef.current!
    s.fit(el.clientWidth, el.clientHeight)
  }, [wide])
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
      } else if (e.code === 'KeyH') setBoxes((b) => !b)
      else if (e.code === 'KeyL' && !e.metaKey) setLabels((b) => !b)
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
        <label>
          <input type="checkbox" checked={wide} onChange={(e) => setWide(e.target.checked)} /> Widescreen
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
          <input type="checkbox" checked={debug} onChange={(e) => setDebug(e.target.checked)} /> Debug panel <kbd>`</kbd>
        </label>
        <label>
          <input type="checkbox" checked={boxes} onChange={(e) => setBoxes(e.target.checked)} /> Hitboxes <kbd>H</kbd>
        </label>
        <label>
          <input type="checkbox" checked={labels} onChange={(e) => setLabels(e.target.checked)} /> States/AI <kbd>L</kbd>
        </label>
        <span className="muted small">
          Move {keysFor('UP').concat(keysFor('LEFT')).join('')}… · A {keysFor('A').join('/')} · B {keysFor('B').join('/')} · L {keysFor('L').join('/')} · Start {keysFor('START').join('/')} · frame-step <kbd>.</kbd> when paused
        </span>
      </div>
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
