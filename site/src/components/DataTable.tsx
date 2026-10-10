import { useMemo, useState } from 'react'

type Row = Record<string, unknown>

export function fmt(v: unknown): string {
  if (v === null || v === undefined) return ''
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, '')
  if (typeof v === 'string') return v
  if (typeof v === 'boolean') return v ? '✓' : '✗'
  return JSON.stringify(v)
}

function Cell({ v }: { v: unknown }) {
  const [open, setOpen] = useState(false)
  if (v && typeof v === 'object') {
    const n = Array.isArray(v) ? v.length : Object.keys(v).length
    const short = JSON.stringify(v)
    if (short.length <= 48) return <code className="cell-json">{short}</code>
    return (
      <span>
        <button className="link" onClick={() => setOpen(!open)}>
          {Array.isArray(v) ? `[${n}]` : `{${n}}`} {open ? '▾' : '▸'}
        </button>
        {open && <pre className="cell-pre">{JSON.stringify(v, null, 2)}</pre>}
      </span>
    )
  }
  if (typeof v === 'string' && /^0x[0-9a-f]+$/i.test(v)) return <code>{v}</code>
  return <>{fmt(v)}</>
}

export function DataTable({ rows, onRow, columns }: { rows: Row[]; onRow?: (r: Row) => void; columns?: string[] }) {
  const [q, setQ] = useState('')
  const [sort, setSort] = useState<{ k: string; dir: 1 | -1 } | null>(null)
  const cols = useMemo(() => {
    if (columns) return columns
    const seen = new Map<string, number>()
    for (const r of rows.slice(0, 400)) for (const k of Object.keys(r)) seen.set(k, (seen.get(k) ?? 0) + 1)
    return [...seen.keys()]
  }, [rows, columns])
  const shown = useMemo(() => {
    let out = rows
    if (q) {
      const ql = q.toLowerCase()
      out = out.filter((r) => JSON.stringify(r).toLowerCase().includes(ql))
    }
    if (sort) {
      out = [...out].sort((a, b) => {
        const x = a[sort.k]
        const y = b[sort.k]
        if (typeof x === 'number' && typeof y === 'number') return (x - y) * sort.dir
        return fmt(x).localeCompare(fmt(y), undefined, { numeric: true }) * sort.dir
      })
    }
    return out
  }, [rows, q, sort])
  return (
    <div className="table-wrap">
      <div className="table-tools">
        <input placeholder="Filter…" value={q} onChange={(e) => setQ(e.target.value)} />
        <span className="muted">
          {shown.length} / {rows.length} rows
        </span>
      </div>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              {cols.map((c) => (
                <th
                  key={c}
                  onClick={() => setSort(sort?.k === c ? { k: c, dir: sort.dir === 1 ? -1 : 1 } : { k: c, dir: 1 })}
                >
                  {c}
                  {sort?.k === c ? (sort.dir === 1 ? ' ▲' : ' ▼') : ''}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {shown.map((r, i) => (
              <tr key={i} onClick={onRow ? () => onRow(r) : undefined} className={onRow ? 'clickable' : ''}>
                {cols.map((c) => (
                  <td key={c}>
                    <Cell v={r[c]} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
