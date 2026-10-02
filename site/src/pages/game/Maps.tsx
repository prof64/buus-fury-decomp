import { useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { assetUrl, useData } from '../../data'
import { useSprites, type MapEntry } from '../../lib/sprites'
import { MapViewer } from '../../components/MapViewer'
import { Addr, Card, KV, Loading, Search, Seg } from '../../components/ui'
import type { J } from '../../lib/util'

function mapKey(m: MapEntry) {
  return String(m.map)
}

/** Human names for maps from the mechanics extraction (LoG1 descriptions, LoG2 location strings, Buu area names). */
function useMapNames(game: string) {
  const maps = useData<J>(game, game === 'buu' ? 'enemies' : 'maps')
  return useMemo(() => {
    const names = new Map<string, string>()
    if (!maps) return names
    if (game === 'log1') for (const m of maps.maps ?? []) names.set(String(m.index), m.description)
    if (game === 'log2') for (const m of maps.maps ?? []) names.set(String(m.index), `${m.name ?? ''} (zone ${m.zone}, area ${m.area})`)
    if (game === 'buu')
      for (const e of maps.enemies ?? []) for (const s of e.spawns ?? []) if (s.area) names.set(`${s.world}-${s.map}`, s.area)
    return names
  }, [maps, game])
}

export function MapsPage({ game }: { game: string }) {
  const sp = useSprites(game)
  const names = useMapNames(game)
  const { idx } = useParams()
  const nav = useNavigate()
  const [q, setQ] = useState('')
  const [layer, setLayer] = useState('composite')
  const maps = useMemo(() => {
    const raw = sp?.json.maps ?? []
    const byKey = new Map(raw.map((m) => [mapKey(m), m]))
    // some areas reuse another area's graphics ({map, same_as}); show the referenced render
    return raw
      .map((m: J) => (m.composite ? m : m.same_as && byKey.get(String(m.same_as)) ? { ...byKey.get(String(m.same_as))!, map: m.map, same_as: m.same_as } : null))
      .filter(Boolean) as (MapEntry & { same_as?: string })[]
  }, [sp])
  const cur = maps.find((m) => mapKey(m) === idx)
  const filtered = useMemo(
    () => maps.filter((m) => !q || `${m.map} ${names.get(mapKey(m)) ?? ''}`.toLowerCase().includes(q.toLowerCase())),
    [maps, q, names],
  )
  if (!sp) return <Loading what="maps" />
  const layerSrc = cur
    ? layer === 'composite'
      ? cur.composite
      : layer === 'screenshot'
        ? (cur.screenshot ?? cur.composite)
        : (cur.layers.find((l) => 'bg' + l.bg === layer)?.png ?? cur.composite)
    : ''
  return (
    <div>
      <p className="lede">
        {maps.length} maps rendered from the ROM’s tilemaps with the tiles and palette the game loaded (captured in the emulator).
        Drag to pan, scroll to zoom.
      </p>
      {cur && (
        <Card
          title={
            <>
              Map {String(cur.map)}
              {names.get(mapKey(cur)) && <span className="muted"> · {names.get(mapKey(cur))}</span>}
            </>
          }
          actions={<Link to={`/g/${game}/maps`}>Close ×</Link>}
        >
          <div className="toolbar">
            <Seg
              value={layer}
              onChange={setLayer}
              options={[
                { value: 'composite', label: 'Composite' },
                ...(cur.screenshot ? [{ value: 'screenshot', label: 'Screenshot' }] : []),
                ...cur.layers.map((l) => ({ value: 'bg' + l.bg, label: 'BG' + l.bg })),
              ]}
            />
          </div>
          <MapViewer key={layerSrc} src={assetUrl(layerSrc)} w={layer === 'composite' ? cur.size[0] : undefined} h={layer === 'composite' ? cur.size[1] : undefined} />
          <div className="grid2" style={{ marginTop: 12 }}>
            <KV
              rows={[
                ['Size', `${cur.size[0]} × ${cur.size[1]} px`],
                ['Tileset', cur.tileset],
                ['ROM record', cur.rom_record ? <Addr key="rom" a={cur.rom_record} /> : undefined],
                ['Graphics', cur.same_as ? `same render as map ${cur.same_as}` : undefined],
              ]}
            />
            <div className="plain-table">
              <table>
                <thead>
                  <tr>
                    <th>layer</th>
                    <th>priority</th>
                    <th>chunks</th>
                    <th>scroll</th>
                    <th>ROM</th>
                  </tr>
                </thead>
                <tbody>
                  {cur.layers.map((l: J) => (
                    <tr key={l.bg}>
                      <td>BG{l.bg}</td>
                      <td>{l.priority}</td>
                      <td className="mono">{l.chunks ? l.chunks.join('×') : ''}</td>
                      <td className="mono">{l.scroll_rate ? l.scroll_rate.map((v: number) => v.toFixed(2)).join(', ') : ''}</td>
                      <td>
                        <Addr a={l.rom_field_resource ?? l.rom_screen_list} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </Card>
      )}
      <div className="toolbar" style={{ marginTop: 16 }}>
        <Search value={q} onChange={setQ} placeholder="Filter maps by number or name…" />
        <span className="muted">{filtered.length} shown</span>
      </div>
      <div className="map-list">
        {filtered.map((m) => (
          <button
            key={mapKey(m)}
            className={'map-card' + (mapKey(m) === idx ? ' on' : '')}
            style={{ padding: 0, textAlign: 'left' }}
            onClick={() => {
              setLayer('composite')
              nav(`/g/${game}/maps/${encodeURIComponent(mapKey(m))}`)
            }}
          >
            <img src={assetUrl(m.screenshot ?? m.composite)} alt="" loading="lazy" />
            <div>
              <b>Map {String(m.map)}</b> <span className="muted">{m.size?.join('×')}</span>
              <div className="muted small">{names.get(mapKey(m)) ?? ' '}</div>
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}
