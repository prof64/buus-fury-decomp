import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { listDocs, loadDoc, useAsync } from '../../data'
import { MarkdownWithToc } from '../../components/Markdown'
import { Loading, Search, Seg } from '../../components/ui'

export function NotesPage({ game }: { game: string }) {
  const { doc } = useParams()
  const nav = useNavigate()
  const docs = [...listDocs(game), 'GRAPHICS']
  const cur = doc && docs.includes(doc) ? doc : docs[0]
  const text = useAsync(() => (cur === 'GRAPHICS' ? loadDoc('graphics', 'GRAPHICS') : loadDoc(game, cur)), [game, cur])
  const [q, setQ] = useState('')
  return (
    <div>
      <div className="toolbar">
        <Seg
          value={cur}
          onChange={(d) => nav(`/g/${game}/notes/${d}`)}
          options={docs.map((d) => ({ value: d, label: d === 'notes' ? 'Mechanics notes' : d === 'GRAPHICS' ? 'Graphics formats (all games)' : d }))}
        />
        <Search value={q} onChange={setQ} placeholder="Filter sections…" />
      </div>
      {text === undefined ? <Loading what="notes" /> : <MarkdownWithToc text={text} filter={q} />}
    </div>
  )
}
