// Toolbar control: mute toggle + master volume (remembered in localStorage), plus a hint while the browser still
// blocks audio (no user gesture yet).

import { useEffect, useState } from 'react'
import { audio } from './engine'

export function useAudioState() {
  const [, setV] = useState(0)
  useEffect(() => audio.subscribe(() => setV((v) => v + 1)), [])
  return audio
}

export function AudioControl({ compact = false }: { compact?: boolean }) {
  const a = useAudioState()
  const s = a.settings
  const blocked = !!a.ctx && a.ctx.state !== 'running'
  return (
    <span className="bt-audio" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
      <button
        onClick={() => {
          a.unlock()
          a.setSettings({ muted: !s.muted })
        }}
        title={s.muted ? 'Sound off (click to turn on)' : 'Sound on (click to mute)'}
        aria-pressed={!s.muted}
      >
        {s.muted ? '🔇 Sound' : '🔊 Sound'}
      </button>
      {!compact && (
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={s.volume}
          aria-label="Volume"
          title={`Volume ${Math.round(s.volume * 100)}%`}
          style={{ width: 80 }}
          onChange={(e) => {
            a.unlock()
            a.setSettings({ volume: +e.target.value, muted: false })
          }}
        />
      )}
      {blocked && !s.muted && (
        <span className="muted small" onClick={() => a.unlock()} style={{ cursor: 'pointer' }}>
          click to enable audio
        </span>
      )}
    </span>
  )
}
