import { HashRouter, NavLink, Route, Routes } from 'react-router-dom'
import { GAMES } from './data'
import { Home } from './pages/Home'
import { GamePage } from './pages/GamePage'
import { BattlePage } from './pages/BattlePage'

export default function App() {
  return (
    <HashRouter>
      <header className="topbar">
        <NavLink to="/" className="brand">
          Legacy of Goku <span>X</span>
        </NavLink>
        <nav>
          {GAMES.map((g) => (
            <NavLink key={g.id} to={`/g/${g.id}`}>
              {g.short}
            </NavLink>
          ))}
          <NavLink to="/battle" className="battle-link">
            Battle
          </NavLink>
        </nav>
      </header>
      <main>
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/g/:game/*" element={<GamePage />} />
          <Route path="/battle" element={<BattlePage />} />
        </Routes>
      </main>
    </HashRouter>
  )
}
