import { useMemo, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { CATEGORY_LABEL, CATEGORY_ORDER, useSprites, type SpriteEntity } from '../../lib/sprites'
import { Loading, Search, SpriteThumb } from '../../components/ui'

const PAGE = 120

export function SpritesPage({ game }: { game: string }) {
  const sp = useSprites(game)
  const [params, setParams] = useSearchParams()
  const cat = params.get('cat') ?? 'all'
  const q = params.get('q') ?? ''
  const set = (k: string, v: string) => {
    const n = new URLSearchParams(params)
    if (!v || v === 'all') n.delete(k)
    else n.set(k, v)
    setParams(n, { replace: true })
  }
  const [more, setMore] = useState<Record<string, number>>({})

  const groups = useMemo(() => {
    if (!sp) return []
    const ql = q.toLowerCase()
    const m = new Map<string, SpriteEntity[]>()
    for (const e of sp.all) {
      if (cat !== 'all' && e.category !== cat) continue
      if (ql && !(e.name.toLowerCase().includes(ql) || e.id.toLowerCase().includes(ql) || (e.rom_labels ?? []).join(' ').includes(ql))) continue
      if (!m.has(e.category)) m.set(e.category, [])
      m.get(e.category)!.push(e)
    }
    return CATEGORY_ORDER.filter((c) => m.has(c))
      .concat([...m.keys()].filter((c) => !CATEGORY_ORDER.includes(c)))
      .map((c) => [c, m.get(c)!] as const)
  }, [sp, cat, q])

  const counts = useMemo(() => {
    const c = new Map<string, number>()
    for (const e of sp?.all ?? []) c.set(e.category, (c.get(e.category) ?? 0) + 1)
    return c
  }, [sp])

  if (!sp) return <Loading what="sprites" />
  return (
    <div>
      <p className="lede">
        Every graphics entity decoded from the ROM: {sp.all.length} entries. Click one for the animation player, frame data, body
        rectangles and ROM addresses. Names marked <i>low confidence</i> are visual guesses.
      </p>
      <div className="toolbar">
        <Search value={q} onChange={(v) => set('q', v)} placeholder="Search by name, id or ROM label…" />
        <div className="chips">
          <button className={'chip' + (cat === 'all' ? ' on' : '')} onClick={() => set('cat', 'all')}>
            All<b>{sp.all.length}</b>
          </button>
          {CATEGORY_ORDER.filter((c) => counts.has(c)).map((c) => (
            <button key={c} className={'chip' + (cat === c ? ' on' : '')} onClick={() => set('cat', c)}>
              {CATEGORY_LABEL[c] ?? c}
              <b>{counts.get(c)}</b>
            </button>
          ))}
        </div>
        <Link to={`/g/${game}/maps`} className="chip">
          Maps →
        </Link>
      </div>
      {groups.length === 0 && <div className="empty-note">No sprite matches “{q}”.</div>}
      {groups.map(([c, list]) => {
        const lim = more[c] ?? PAGE
        return (
          <section key={c}>
            <div className="cat-head">
              <h2>{CATEGORY_LABEL[c] ?? c}</h2>
              <span className="muted">{list.length}</span>
            </div>
            <div className="gallery">
              {list.slice(0, lim).map((e) => (
                <Link key={e.id} to={`/g/${game}/sprites/${encodeURIComponent(e.id)}`} className="gallery-item">
                  <SpriteThumb entity={e} size={c === 'portrait' ? 64 : 72} />
                  <div className="gname">
                    {e.name}
                    {e.name_confidence === 'low' && <span className="muted"> ?</span>}
                  </div>
                  <div className="gid">
                    {e.sprite_index !== undefined ? `#${e.sprite_index} · ` : ''}
                    {Object.keys(e.animations ?? {}).length} anim · {e.frames.length} fr
                  </div>
                </Link>
              ))}
            </div>
            {list.length > lim && (
              <button style={{ marginTop: 10 }} onClick={() => setMore({ ...more, [c]: lim + PAGE * 3 })}>
                Show more ({list.length - lim} left)
              </button>
            )}
          </section>
        )
      })}
    </div>
  )
}
