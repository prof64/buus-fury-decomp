import { Link } from 'react-router-dom'
import { assetUrl } from '../../data'
import { previewFrame } from '../../lib/sprites'
import { FACTS, SIGNATURE, useGameSummary } from '../../model/summary'
import { Card, Loading, SpriteThumb, Stat } from '../../components/ui'
import { fmt } from '../../lib/util'

const LINKS: [string, string, string][] = [
  ['sprites', 'Sprites', 'every entity, animated, with frame data'],
  ['maps', 'Maps', 'rendered maps with pan & zoom'],
  ['characters', 'Characters', 'stats, growth, forms, moves'],
  ['enemies', 'Enemies & bosses', 'stats, AI trees, drops, spawns'],
  ['moves', 'Moves', 'power, costs, frame data, hitboxes'],
  ['formulas', 'Formulas', 'exact math + calculators'],
  ['lab', 'Move lab', 'design a move, see its bytes'],
  ['items', 'Items', 'effects, equipment, drop tables'],
  ['notes', 'Notes', 'the full reverse-engineering write-up'],
  ['raw', 'Raw data', 'every extracted JSON file'],
]

export function OverviewPage({ game }: { game: string }) {
  const s = useGameSummary(game)
  const sig = SIGNATURE[game]
  if (!s) return <Loading what="overview" />
  return (
    <div className="stack">
      <div className="split" style={{ gridTemplateColumns: 'minmax(0, 1.3fr) minmax(0, 1fr)' }}>
        <Card title="At a glance">
          <div className="stats">
            <Stat label="Sprite entities" value={fmt(s.sprites)} sub={Object.entries(s.categories).map(([k, v]) => `${v} ${k}`).join(' · ')} />
            <Stat label="Maps" value={fmt(s.maps)} />
            <Stat label="Enemy types" value={s.enemiesUsed} sub={`${s.enemyRows} table rows · ${s.bosses} bosses`} />
            <Stat label="Playable" value={s.playable} sub={`levels ${s.levels}`} />
            <Stat label="Moves" value={s.moves} />
            <Stat label="Items" value={s.items} />
            {s.aiStates !== undefined && <Stat label="AI states" value={s.aiStates} sub="behaviour trees" />}
          </div>
          <div className="row" style={{ marginTop: 14, gap: 8 }}>
            {s.heroes.map((h) => {
              const f = previewFrame(h)
              return f ? <img key={h.id} className="portrait" src={assetUrl(f.png)} alt={h.name} title={h.name} width={56} height={56} /> : null
            })}
          </div>
        </Card>
        <Card title={`Signature formula: ${sig.title}`}>
          <div className="formula" style={{ whiteSpace: 'pre-wrap' }}>
            {sig.formula}
          </div>
          <p className="muted">{sig.blurb}</p>
          <Link to={`/g/${game}/formulas`}>Calculators →</Link>
        </Card>
      </div>
      <div className="grid2">
        <Card title="Things worth knowing">
          <ul>
            {FACTS[game].map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        </Card>
        <Card title="Bosses">
          <div className="row" style={{ gap: 6 }}>
            {s.bossSprites.map((b) => (
              <Link key={b.id} to={`/g/${game}/sprites/${encodeURIComponent(b.id)}`} title={b.name}>
                <SpriteThumb entity={b} size={64} />
              </Link>
            ))}
          </div>
          <p className="small">
            <Link to={`/g/${game}/enemies?f=boss`}>All bosses →</Link>
          </p>
        </Card>
      </div>
      <div className="grid-auto">
        {LINKS.map(([to, title, sub]) => (
          <Link key={to} to={`/g/${game}/${to}`} className="card2" style={{ display: 'block' }}>
            <h3 style={{ margin: 0 }}>{title} →</h3>
            <div className="muted small">{sub}</div>
          </Link>
        ))}
      </div>
    </div>
  )
}
