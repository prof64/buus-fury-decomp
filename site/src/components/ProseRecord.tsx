// Readable rendering of hand-written analysis records (e.g. data/buu/bosses.json): strings become
// paragraphs, string lists bullets, object lists small cards. Evidence tags inside [brackets] are coloured.
import type { ReactNode } from 'react'
import { Evidence } from './ui'
import type { J } from '../lib/util'

const label = (k: string) => k.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase())

/** Splits "text [VERIFIED]" into text + tag chips. */
export function TaggedText({ text }: { text: string }) {
  const parts = String(text).split(/(\[[A-Z][A-Za-z ;:,.()0-9_-]*\])/g)
  return (
    <>
      {parts.map((p, i) =>
        /^\[[A-Z]/.test(p) ? (
          <span key={i}>
            {' '}
            <Evidence text={p.slice(1, -1)} />
          </span>
        ) : (
          <span key={i}>{p}</span>
        ),
      )}
    </>
  )
}

function Value({ v, renderRef }: { v: J; renderRef?: (k: string, v: J) => ReactNode | null }): ReactNode {
  if (v === null || v === undefined) return null
  if (typeof v !== 'object') return <TaggedText text={String(v)} />
  if (Array.isArray(v)) {
    if (v.every((x) => typeof x !== 'object'))
      return (
        <ul className="prose-list">
          {v.map((x, i) => (
            <li key={i}>
              <TaggedText text={String(x)} />
            </li>
          ))}
        </ul>
      )
    return (
      <div className="prose-cards">
        {v.map((x, i) => (
          <div key={i} className="prose-card">
            {typeof x === 'object' && x ? <Fields obj={x} renderRef={renderRef} /> : <TaggedText text={String(x)} />}
          </div>
        ))}
      </div>
    )
  }
  return <Fields obj={v} renderRef={renderRef} />
}

export function Fields({ obj, skip = [], renderRef }: { obj: J; skip?: string[]; renderRef?: (k: string, v: J) => ReactNode | null }) {
  return (
    <dl className="kv">
      {Object.entries<J>(obj)
        .filter(([k, v]) => !skip.includes(k) && v !== null && v !== undefined && v !== '')
        .map(([k, v]) => (
          <div key={k}>
            <dt>{label(k)}</dt>
            <dd>
              <Value v={v} renderRef={renderRef} />
              {renderRef?.(k, v)}
            </dd>
          </div>
        ))}
    </dl>
  )
}
