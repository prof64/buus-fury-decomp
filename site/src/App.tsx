import { lazy, Suspense, useEffect, useState } from 'react'
import { HashRouter, NavLink, Route, Routes, useLocation } from 'react-router-dom'
import { GAMES } from './data'
import { Home } from './pages/Home'

// route-level code splitting: each section loads its code (and data) on first visit
const GamePage = lazy(() => import('./pages/GamePage').then((m) => ({ default: m.GamePage })))
const BattlePage = lazy(() => import('./pages/BattlePage').then((m) => ({ default: m.BattlePage })))
const ComparePage = lazy(() => import('./pages/ComparePage').then((m) => ({ default: m.ComparePage })))

function ThemeToggle() {
  const [theme, setTheme] = useState<string>(() => {
    try {
      return localStorage.getItem('theme') ?? 'auto'
    } catch {
      return 'auto'
    }
  })
  useEffect(() => {
    const root = document.documentElement
    if (theme === 'auto') root.removeAttribute('data-theme')
    else root.setAttribute('data-theme', theme)
    try {
      localStorage.setItem('theme', theme)
    } catch {
      /* ignore */
    }
  }, [theme])
  const next = theme === 'auto' ? 'dark' : theme === 'dark' ? 'light' : 'auto'
  return (
    <button className="theme-btn" onClick={() => setTheme(next)} title={`Theme: ${theme} (click for ${next})`}>
      {theme === 'auto' ? '◐' : theme === 'dark' ? '☾' : '☀'}
    </button>
  )
}

function ScrollTop() {
  const { pathname } = useLocation()
  useEffect(() => {
    window.scrollTo(0, 0)
  }, [pathname])
  return null
}

export default function App() {
  return (
    <HashRouter>
      <ScrollTop />
      <header className="topbar">
        <NavLink to="/" className="brand">
          Legacy of Goku <span>X</span>
        </NavLink>
        <nav>
          {GAMES.map((g) => (
            <NavLink key={g.id} to={`/g/${g.id}`} className={'game-link g-' + g.id}>
              {g.short}
            </NavLink>
          ))}
          <NavLink to="/compare">Compare</NavLink>
          <NavLink to="/battle" className="battle-link">
            Battle
          </NavLink>
        </nav>
        <ThemeToggle />
      </header>
      <main>
        <Suspense fallback={<div className="page loading">Loading…</div>}>
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/compare" element={<ComparePage />} />
          <Route path="/g/:game/*" element={<GamePage />} />
          <Route path="/battle" element={<BattlePage />} />
        </Routes>
        </Suspense>
      </main>
    </HashRouter>
  )
}
