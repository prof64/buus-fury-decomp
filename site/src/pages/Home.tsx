import { Link } from 'react-router-dom'
import { GAMES, listDataFiles, listDocs } from '../data'

export function Home() {
  return (
    <div className="page">
      <h1>Three games, taken apart</h1>
      <p className="lede">
        Everything recovered from the three Webfoot DBZ action RPGs on GBA: sprites and animations, stat tables,
        damage and experience formulas, every move and enemy, and how the engine builds an attack. Each value
        comes from the ROM itself or was checked by running the game in an emulator.
      </p>
      <div className="cards">
        {GAMES.map((g) => (
          <Link key={g.id} to={`/g/${g.id}`} className="card">
            <h2>{g.title}</h2>
            <div className="muted">{g.year}</div>
            <div className="chips">
              {listDocs(g.id).map((d) => (
                <span key={d} className="chip">
                  {d}
                </span>
              ))}
              {listDataFiles(g.id).map((d) => (
                <span key={d} className="chip data">
                  {d}
                </span>
              ))}
            </div>
          </Link>
        ))}
        <Link to="/battle" className="card battle">
          <h2>Battle simulator</h2>
          <div className="muted">Pick any playable fighter, face any enemy, using the real mechanics.</div>
        </Link>
      </div>
    </div>
  )
}
