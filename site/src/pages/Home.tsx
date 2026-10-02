import { Link } from 'react-router-dom'
import { GAMES, assetUrl } from '../data'
import { previewFrame } from '../lib/sprites'
import { SIGNATURE, useGameSummary } from '../model/summary'
import { fmt } from '../lib/util'

function GameCard({ id, title, year }: { id: string; title: string; year: number }) {
  const s = useGameSummary(id)
  const sig = SIGNATURE[id]
  return (
    <div className={'game-card g-' + id}>
      <div>
        <h2>
          <Link to={`/g/${id}`}>{title}</Link>
        </h2>
        <div className="muted small">
          {year} · {id === 'log1' ? 'original engine' : 'WF engine'}
        </div>
      </div>
      <div className="portraits">
        {(s?.heroes ?? []).map((h) => {
          const f = previewFrame(h)
          return f ? <img key={h.id} className="portrait" src={assetUrl(f.png)} alt={h.name} title={h.name} width={44} height={44} /> : null
        })}
      </div>
      <div className="stats" style={{ gridTemplateColumns: 'repeat(3, minmax(0,1fr))' }}>
        <div className="stat">
          <div className="stat-label">sprites</div>
          <div className="stat-value">{s ? fmt(s.sprites) : '…'}</div>
        </div>
        <div className="stat">
          <div className="stat-label">enemies</div>
          <div className="stat-value">{s ? s.enemiesUsed : '…'}</div>
        </div>
        <div className="stat">
          <div className="stat-label">maps</div>
          <div className="stat-value">{s ? s.maps : '…'}</div>
        </div>
      </div>
      <div className="formula">{sig.formula}</div>
      <p className="muted small" style={{ margin: 0 }}>
        {sig.blurb}
      </p>
      <div className="quick-links">
        {[
          ['sprites', 'Sprites'],
          ['characters', 'Characters'],
          ['enemies', 'Enemies'],
          ['moves', 'Moves'],
          ['formulas', 'Formulas'],
          ['lab', 'Move lab'],
          ['notes', 'Notes'],
        ].map(([to, l]) => (
          <Link key={to} to={`/g/${id}/${to}`}>
            {l}
          </Link>
        ))}
      </div>
    </div>
  )
}

export function Home() {
  return (
    <div className="page wide">
      <div className="hero">
        <h1>Three games, taken apart</h1>
        <p className="lede">
          Everything recovered from Webfoot’s three Dragon Ball Z action RPGs on the Game Boy Advance: every sprite and animation, the stat
          tables, the damage and experience formulas (reproduced exactly, with calculators), every move and enemy, the enemy AI, and how the
          engine builds an attack — so you can design your own. Values come from the ROMs and were checked by running the games in an
          emulator; each claim carries its evidence tag and ROM address.
        </p>
      </div>
      <div className="game-cards">
        {GAMES.map((g) => (
          <GameCard key={g.id} id={g.id} title={g.title} year={g.year} />
        ))}
      </div>
      <div className="grid2" style={{ marginTop: 16 }}>
        <Link to="/compare" className="card2" style={{ display: 'block' }}>
          <h3 style={{ margin: 0 }}>How the mechanics evolved →</h3>
          <div className="muted small">LoG I → LoG II → Buu’s Fury: damage model, stats, regen, forms, AI engine, side by side.</div>
        </Link>
        <Link to="/battle" className="card2" style={{ display: 'block' }}>
          <h3 style={{ margin: 0, color: 'var(--accent)' }}>Battle simulator →</h3>
          <div className="muted small">Pick any playable fighter and face any enemy, using the real mechanics.</div>
        </Link>
      </div>
    </div>
  )
}
