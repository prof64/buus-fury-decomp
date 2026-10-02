// Audio tab: every song and sound effect ripped from the game (tools/audio/rip_buu.py -> data/<game>/audio.json),
// with gapless loop playback through the same Web Audio engine the battle simulator uses.

import { useMemo, useState } from 'react'
import { loadData, useAsync } from '../../data'
import { Addr, Empty, Loading, Search, Seg, Tag } from '../../components/ui'
import { AudioControl, useAudioState } from '../../battle/audio/AudioControl'
import type { AudioCatalog, MusicTrack, SfxEntry } from '../../battle/audio/engine'

type Row = (MusicTrack | SfxEntry) & {
  kind: 'music' | 'sfx'
  confidence?: string
  context?: string
  trigger?: string
  areas?: string[]
  evidence?: string
  ends?: boolean
  tempo?: number
  channels?: number
  header?: string
  sample?: { rom: string; rate: number; length: number }
}

const CONF: Record<string, 'ok' | 'info' | 'warn' | 'muted'> = { high: 'ok', medium: 'info', low: 'warn', none: 'muted' }

function fmtTime(s: number) {
  const m = Math.floor(s / 60)
  return `${m}:${(s - m * 60).toFixed(2).padStart(5, '0')}`
}

export function AudioPage({ game }: { game: string }) {
  type Cat = AudioCatalog & { engine?: Record<string, unknown> }
  const cat = useAsync(() => loadData(game, 'audio').then((x) => (x ?? null) as Cat | null), [game])
  const a = useAudioState()
  const [tab, setTab] = useState<'music' | 'sfx'>('music')
  const [q, setQ] = useState('')
  const [playing, setPlaying] = useState<string | null>(null)
  const rows = useMemo(() => {
    if (!cat) return []
    const list = (tab === 'music' ? cat.music : cat.sfx) as Row[]
    const ql = q.toLowerCase()
    return list.filter((r) => !ql || JSON.stringify([r.id, r.name, r.context, r.trigger, r.areas]).toLowerCase().includes(ql))
  }, [cat, tab, q])
  if (cat === undefined) return <Loading what="audio catalog" />
  if (!cat) return <Empty>No audio has been ripped for this game (only Buu&apos;s Fury: tools/audio/rip_buu.py).</Empty>

  const play = (r: Row) => {
    a.unlock()
    const key = `${r.kind}:${r.id}`
    if (r.kind === 'music') {
      if (playing === key) {
        a.stopMusic()
        setPlaying(null)
        return
      }
      void a.playMusic(game, r.id)
      setPlaying(key)
    } else {
      void a.playSfx(game, r.id, (r as SfxEntry).game_volume ?? 64)
    }
  }
  const eng = cat.engine ?? {}
  return (
    <div>
      <p className="lede">
        {String(eng.name ?? '')}: songs are tracker patterns (song table <Addr a={String(eng.song_table ?? '')} />, PlaySong{' '}
        <Addr a={String(eng.play_song ?? '')} />), sound effects are 8-bit samples mixed on 16 voices (table{' '}
        <Addr a={String(eng.sfx_table ?? '')} />, PlaySfx <Addr a={String(eng.play_sfx ?? '')} />). Every file was rendered by the game&apos;s own
        driver in the emulator; music loops at the measured loop points (▶ plays it gaplessly, forever). Names come from where the game plays
        each ID — the confidence tag says how direct the evidence is.
      </p>
      <div className="toolbar">
        <Seg
          value={tab}
          onChange={setTab}
          options={[
            { value: 'music', label: `Music (${cat.music.length})` },
            { value: 'sfx', label: `Sound effects (${cat.sfx.length})` },
          ]}
        />
        <Search value={q} onChange={setQ} placeholder="Filter by name, area, trigger…" />
        <AudioControl />
        {playing && (
          <button
            onClick={() => {
              a.stopMusic()
              setPlaying(null)
            }}
          >
            ■ Stop music
          </button>
        )}
      </div>
      <div className="table-wrap">
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th />
                <th>ID</th>
                <th>Name</th>
                <th>Length</th>
                <th>Loop</th>
                <th>{tab === 'music' ? 'Where the game plays it' : 'Trigger'}</th>
                <th>Confidence</th>
                <th>Evidence</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const key = `${r.kind}:${r.id}`
                const on = playing === key && a.musicId === r.id
                return (
                  <tr key={key}>
                    <td>
                      <button onClick={() => play(r)} title={r.kind === 'music' ? (on ? 'Stop' : 'Play (loops)') : 'Play'} aria-label={`Play ${r.name}`}>
                        {on ? '■' : '▶'}
                      </button>
                    </td>
                    <td className="mono">{r.id}</td>
                    <td>
                      {r.name}
                      <div className="muted small">
                        <a href={import.meta.env.BASE_URL + r.file} download>
                          {r.file.split('/').pop()}
                        </a>
                      </div>
                    </td>
                    <td className="mono">{fmtTime(r.duration)}</td>
                    <td className="mono small">{r.loop ? `${fmtTime(r.loop.start)} → ${fmtTime(r.loop.end)}` : r.ends ? 'ends' : 'one-shot'}</td>
                    <td className="small" style={{ maxWidth: 420 }}>
                      {r.kind === 'music' ? r.context || <span className="muted">no play site found</span> : r.trigger}
                      {r.kind === 'music' && r.areas && r.areas.length > 0 && <div className="muted">{r.areas.length} areas</div>}
                    </td>
                    <td>
                      <Tag kind={CONF[r.confidence ?? 'none'] ?? 'muted'}>{r.confidence ?? 'none'}</Tag>
                    </td>
                    <td className="small muted" style={{ maxWidth: 360 }}>
                      {r.evidence}
                      {r.sample && (
                        <div className="mono">
                          sample {r.sample.rom} · {r.sample.rate} Hz · {r.sample.length} smp
                        </div>
                      )}
                      {r.header && (
                        <div className="mono">
                          header {r.header} · tempo {r.tempo} · {r.channels} ch
                        </div>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  )
}
