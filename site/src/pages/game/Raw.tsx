import { useMemo, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { listDataFiles, useData } from '../../data'
import { DataTable } from '../../components/DataTable'
import { JsonView, Loading, Seg } from '../../components/ui'
import type { J } from '../../lib/util'

/** Every array of objects inside a JSON document, with its path. */
function tablesIn(v: J, path = '', out: { path: string; rows: J[] }[] = [], depth = 0) {
  if (depth > 4 || !v || typeof v !== 'object') return out
  if (Array.isArray(v)) {
    if (v.length && v.every((x) => x && typeof x === 'object' && !Array.isArray(x))) out.push({ path: path || '(root)', rows: v })
    return out
  }
  const vals = Object.values(v)
  if (vals.length > 8 && vals.every((x) => x && typeof x === 'object' && !Array.isArray(x)))
    out.push({ path: (path || '(root)') + ' {map}', rows: Object.entries(v).map(([k, x]) => ({ key: k, ...(x as object) })) })
  for (const [k, x] of Object.entries(v)) tablesIn(x, path ? `${path}.${k}` : k, out, depth + 1)
  return out
}

export function RawPage({ game }: { game: string }) {
  const { name } = useParams()
  const nav = useNavigate()
  const files = listDataFiles(game)
  const cur = name && files.includes(name) ? name : files[0]
  const json = useData<J>(game, cur)
  const tables = useMemo(() => (json ? tablesIn(json).sort((a, b) => b.rows.length - a.rows.length) : []), [json])
  const [sel, setSel] = useState<string>('')
  const [mode, setMode] = useState<'table' | 'json'>('table')
  const t = tables.find((x) => x.path === sel) ?? tables[0]
  return (
    <div>
      <p className="lede">
        The extracted JSON exactly as it is in <code>data/{game}/</code>. Pick a file, then any array inside it to browse as a
        sortable, filterable table.
      </p>
      <div className="toolbar">
        <div className="chips">
          {files.map((f) => (
            <button key={f} className={'chip' + (f === cur ? ' on' : '')} onClick={() => (setSel(''), nav(`/g/${game}/raw/${f}`))}>
              {f}.json
            </button>
          ))}
        </div>
      </div>
      {json === undefined ? (
        <Loading what={cur + '.json'} />
      ) : (
        <>
          <div className="toolbar">
            <Seg value={mode} onChange={setMode} options={[{ value: 'table', label: 'Table' }, { value: 'json', label: 'JSON' }]} />
            {mode === 'table' && tables.length > 0 && (
              <label className="field">
                <span>Array</span>
                <select value={t?.path} onChange={(e) => setSel(e.target.value)}>
                  {tables.map((x) => (
                    <option key={x.path} value={x.path}>
                      {x.path} ({x.rows.length})
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          {mode === 'json' || !t ? <JsonView value={json} max={600} /> : <DataTable key={cur + t.path} rows={t.rows} />}
        </>
      )}
    </div>
  )
}
