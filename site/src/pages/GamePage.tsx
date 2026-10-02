import { NavLink, Route, Routes, useParams } from 'react-router-dom'
import { GAMES, listDataFiles, listDocs, loadDoc, recordsOf, useAsync, useData } from '../data'
import { DataTable } from '../components/DataTable'
import { Markdown } from '../components/Markdown'

function DocView({ game, name }: { game: string; name: string }) {
  const text = useAsync(() => loadDoc(game, name), [game, name])
  if (text === undefined) return <div className="muted">Loading…</div>
  return <Markdown text={text} />
}

function DataView({ game, name }: { game: string; name: string }) {
  const json = useData(game, name)
  if (json === undefined) return <div className="muted">Loading…</div>
  const rows = recordsOf(json)
  if (!rows.length) return <pre className="cell-pre">{JSON.stringify(json, null, 2)}</pre>
  return <DataTable rows={rows} />
}

export function GamePage() {
  const { game = 'log1' } = useParams()
  const meta = GAMES.find((g) => g.id === game)
  const docs = listDocs(game)
  const files = listDataFiles(game)
  return (
    <div className="page wide">
      <h1>{meta?.title ?? game}</h1>
      <nav className="tabs">
        {docs.map((d) => (
          <NavLink key={d} to={`/g/${game}/doc/${d}`}>
            {d === 'notes' ? 'Notes' : d}
          </NavLink>
        ))}
        {files.map((f) => (
          <NavLink key={f} to={`/g/${game}/data/${f}`}>
            {f}
          </NavLink>
        ))}
      </nav>
      <Routes>
        <Route index element={docs.length ? <DocView game={game} name={docs[0]} /> : <Empty />} />
        <Route path="doc/:name" element={<Param kind="doc" game={game} />} />
        <Route path="data/:name" element={<Param kind="data" game={game} />} />
      </Routes>
    </div>
  )
}

function Param({ kind, game }: { kind: 'doc' | 'data'; game: string }) {
  const { name = '' } = useParams()
  return kind === 'doc' ? <DocView game={game} name={name} /> : <DataView game={game} name={name} />
}

function Empty() {
  return <div className="muted">Nothing extracted for this game yet.</div>
}
