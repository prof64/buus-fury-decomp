import { useMemo } from 'react'
import { Link, useParams } from 'react-router-dom'
import { assetUrl, useData } from '../../data'
import { CATEGORY_LABEL, useSprites } from '../../lib/sprites'
import { enemiesOf } from '../../model/enemies'
import { SpritePlayer } from '../../components/SpritePlayer'
import { Addr, Card, Collapse, Crumbs, Empty, KV, Loading, SpriteThumb, Tag } from '../../components/ui'
import type { J } from '../../lib/util'

export function SpriteDetailPage({ game }: { game: string }) {
  const { id = '' } = useParams()
  const sp = useSprites(game)
  const enemiesJson = useData<J>(game, 'enemies')
  const e = sp?.byId.get(decodeURIComponent(id))
  const users = useMemo(() => {
    if (!sp || !e || !enemiesJson) return []
    return enemiesOf(game, enemiesJson, sp).filter((v) => v.sprites.includes(e))
  }, [sp, e, enemiesJson, game])
  const siblings = useMemo(() => (sp && e ? sp.all.filter((x) => x.category === e.category) : []), [sp, e])
  if (!sp) return <Loading what="sprites" />
  if (!e) return <Empty>No sprite “{id}”.</Empty>
  const idx = siblings.indexOf(e)
  const prev = siblings[idx - 1]
  const next = siblings[idx + 1]
  const anims = Object.entries(e.animations ?? {})
  const sheetUrl = assetUrl(e.sheet)

  return (
    <div>
      <Crumbs items={[['Sprites', `/g/${game}/sprites`], [CATEGORY_LABEL[e.category] ?? e.category, `/g/${game}/sprites?cat=${e.category}`], [e.name]]} />
      <div className="row" style={{ justifyContent: 'space-between', marginBottom: 12 }}>
        <div>
          <h2 style={{ margin: 0 }}>{e.name}</h2>
          <div className="muted mono small">
            {e.id}
            {e.sprite_index !== undefined && ` · engine sprite #${e.sprite_index}`}
            {game === 'buu' && e.sprite_index !== undefined && ` (gfx id ${e.sprite_index + 9})`}
            {e.portrait_index !== undefined && ` · portrait #${e.portrait_index}`}
          </div>
        </div>
        <div className="row">
          <Tag kind={e.name_confidence === 'high' ? 'ok' : e.name_confidence === 'low' ? 'warn' : 'info'}>name: {e.name_confidence ?? '?'}</Tag>
          {prev && <Link to={`/g/${game}/sprites/${encodeURIComponent(prev.id)}`}>← {prev.name}</Link>}
          {next && <Link to={`/g/${game}/sprites/${encodeURIComponent(next.id)}`}>{next.name} →</Link>}
        </div>
      </div>
      <div className="split" style={{ gridTemplateColumns: 'minmax(0, 1.2fr) minmax(0, 1fr)' }}>
        <Card title="Animation player">
          <SpritePlayer key={e.id} entity={e} zoom={e.category === 'portrait' ? 3 : Math.max(2, Math.min(6, Math.floor(200 / Math.max(32, ...e.frames.map((f) => f.h)))))} />
          <p className="note">
            Red cross = entity position (where the frame’s anchor pixel sits). Dashed blue = per-direction body rectangle used
            for hit tests. Durations come from emulator measurement when available; frame lists themselves carry no timing.
          </p>
        </Card>
        <div className="stack">
          <Card title="Data">
            <KV
              rows={[
                ['Category', CATEGORY_LABEL[e.category] ?? e.category],
                ['Frames', e.frames.length],
                ['Animations', anims.length],
                ...Object.entries(e.rom ?? {}).map(([k, v]) => [k.replace(/_/g, ' '), typeof v === 'string' && v.startsWith('0x') ? <Addr key={k} a={v} /> : String(v)] as [string, React.ReactNode]),
                ...Object.entries(e.header ?? {}).map(([k, v]) => ['header.' + k, JSON.stringify(v)] as [string, React.ReactNode]),
                ['ROM labels', e.rom_labels?.join(', ')],
                ['Notes', e.notes],
              ]}
            />
          </Card>
          {users.length > 0 && (
            <Card title="Used by (mechanics data)">
              {users.map((u) => (
                <Link key={u.key} to={`/g/${game}/enemies/${u.key}`} className="row" style={{ padding: '4px 0' }}>
                  <SpriteThumb entity={u.sprites[0]} size={32} />
                  <span>{u.name}</span>
                  <span className="muted small">{u.idLabel} · HP {u.hp.toLocaleString()}</span>
                </Link>
              ))}
            </Card>
          )}
          {e.hitboxes?.body_per_direction && (
            <Card title="Body rectangles">
              <table className="plain-table">
                <thead>
                  <tr>
                    <th>dir</th>
                    <th className="num">x0</th>
                    <th className="num">y0</th>
                    <th className="num">x1</th>
                    <th className="num">y1</th>
                    <th className="num">w×h</th>
                  </tr>
                </thead>
                <tbody>
                  {e.hitboxes.body_per_direction.map((b) => (
                    <tr key={b.dir}>
                      <td>{b.dir}</td>
                      <td className="num">{b.x0}</td>
                      <td className="num">{b.y0}</td>
                      <td className="num">{b.x1}</td>
                      <td className="num">{b.y1}</td>
                      <td className="num">
                        {b.x1 - b.x0}×{b.y1 - b.y0}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {e.hitboxes.note && <p className="note">{e.hitboxes.note}</p>}
            </Card>
          )}
        </div>
      </div>

      {anims.length > 0 && (
        <Card title={`Animations (${anims.length})`} className="mt">
          <div className="table-scroll" style={{ maxHeight: 420 }}>
            <table>
              <thead>
                <tr>
                  <th>name</th>
                  <th className="num">raw</th>
                  <th>steps per direction</th>
                  <th>timing</th>
                  <th>observed orders</th>
                  <th>ROM</th>
                </tr>
              </thead>
              <tbody>
                {anims.map(([n, a]) => (
                  <tr key={n}>
                    <td>{n}</td>
                    <td className="num">{a.raw_index}</td>
                    <td className="mono small">
                      {Object.entries(a.directions)
                        .map(([d, s]) => `${d}:${s?.length}`)
                        .join(' ')}
                    </td>
                    <td className="small">{a.timing_source}</td>
                    <td className="mono small">{a.observed_frame_orders?.slice(0, 3).map((o) => o.join(',')).join(' | ')}</td>
                    <td>
                      <Addr a={a.rom} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      )}

      <Collapse title={`All ${e.frames.length} frames`} open={e.frames.length <= 60}>
        <div className="frames-grid">
          {e.frames.map((f, i) => (
            <div key={i} title={`${f.w}×${f.h} anchor (${f.anchor.x},${f.anchor.y}) ${f.rom ?? ''}`}>
              <img src={assetUrl(f.png)} alt="" loading="lazy" style={{ width: Math.min(f.w * 2, 64) }} />#{i} · {f.w}×{f.h}
            </div>
          ))}
        </div>
      </Collapse>
      <Collapse title="Sprite sheet (as exported)">
        <div className="sheet-wrap">
          <img src={sheetUrl} alt="sheet" className="pix" style={{ zoom: 2 } as React.CSSProperties} />
        </div>
        <p className="note">
          <a href={sheetUrl} target="_blank" rel="noreferrer">
            Open sheet.png
          </a>
        </p>
      </Collapse>
    </div>
  )
}
