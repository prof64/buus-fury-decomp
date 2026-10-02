import { NavLink, Navigate, Route, Routes, useLocation, useParams } from 'react-router-dom'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { GAMES } from '../data'
import { OverviewPage } from './game/Overview'
import { SpritesPage } from './game/Sprites'
import { SpriteDetailPage } from './game/SpriteDetail'
import { MapsPage } from './game/Maps'
import { CharactersPage } from './game/Characters'
import { EnemiesPage } from './game/Enemies'
import { EnemyDetailPage } from './game/EnemyDetail'
import { MovesPage } from './game/Moves'
import { FormulasPage } from './game/Formulas'
import { MoveLabPage } from '../movelab/MoveLab'
import { ItemsPage } from './game/Items'
import { NotesPage } from './game/Notes'
import { RawPage } from './game/Raw'

const TABS: [string, string][] = [
  ['', 'Overview'],
  ['sprites', 'Sprites'],
  ['maps', 'Maps'],
  ['characters', 'Characters'],
  ['enemies', 'Enemies & bosses'],
  ['moves', 'Moves'],
  ['formulas', 'Formulas'],
  ['lab', 'Move lab'],
  ['items', 'Items'],
  ['notes', 'Notes'],
  ['raw', 'Raw data'],
]

export function GamePage() {
  const { game = 'log1' } = useParams()
  const meta = GAMES.find((g) => g.id === game)
  const loc = useLocation()
  if (!meta) return <Navigate to="/" />
  return (
    <div className={'game-shell g-' + game}>
      <div className="game-head">
        <div className="game-title">
          <h1>{meta.title}</h1>
          <span className="muted">
            {meta.year} · {game === 'log1' ? 'older Webfoot engine' : 'Webfoot "WF" engine (shared by LoG II and Buu’s Fury)'}
          </span>
        </div>
        <nav className="tabs" aria-label="Sections">
          {TABS.map(([to, label]) => (
            <NavLink key={to} to={`/g/${game}${to ? '/' + to : ''}`} end={to === ''}>
              {label}
            </NavLink>
          ))}
        </nav>
      </div>
      <div className="page wide">
        <ErrorBoundary resetKey={loc.pathname}>
        <Routes>
          <Route index element={<OverviewPage game={game} />} />
          <Route path="sprites" element={<SpritesPage game={game} />} />
          <Route path="sprites/:id" element={<SpriteDetailPage game={game} />} />
          <Route path="maps" element={<MapsPage game={game} />} />
          <Route path="maps/:idx" element={<MapsPage game={game} />} />
          <Route path="characters" element={<CharactersPage game={game} />} />
          <Route path="characters/:id" element={<CharactersPage game={game} />} />
          <Route path="enemies" element={<EnemiesPage game={game} />} />
          <Route path="enemies/:id" element={<EnemyDetailPage game={game} />} />
          <Route path="moves" element={<MovesPage game={game} />} />
          <Route path="formulas" element={<FormulasPage game={game} />} />
          <Route path="lab" element={<MoveLabPage game={game} />} />
          <Route path="items" element={<ItemsPage game={game} />} />
          <Route path="notes" element={<NotesPage game={game} />} />
          <Route path="notes/:doc" element={<NotesPage game={game} />} />
          <Route path="raw" element={<RawPage game={game} />} />
          <Route path="raw/:name" element={<RawPage game={game} />} />
        </Routes>
        </ErrorBoundary>
      </div>
    </div>
  )
}
