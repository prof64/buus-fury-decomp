// Small presentational building blocks shared by every page.
import { useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { assetUrl } from '../data'
import { previewFrame, type SpriteEntity } from '../lib/sprites'

export function SpriteThumb({ entity, size = 64, title }: { entity?: SpriteEntity; size?: number; title?: string }) {
  const f = entity && previewFrame(entity)
  if (!entity || !f) return <div className="thumb empty" style={{ width: size, height: size }} />
  const scale = Math.max(1, Math.min(4, Math.floor(size / Math.max(f.w, f.h)))) || 1
  const s = Math.min(scale, size / Math.max(f.w, f.h))
  return (
    <div className="thumb" style={{ width: size, height: size }} title={title ?? entity.name}>
      <img src={assetUrl(f.png)} alt={entity.name} loading="lazy" style={{ width: f.w * s, height: f.h * s }} />
    </div>
  )
}

export function Card({ title, children, className, actions }: { title?: ReactNode; children: ReactNode; className?: string; actions?: ReactNode }) {
  return (
    <section className={'card2 ' + (className ?? '')}>
      {(title || actions) && (
        <header>
          {title && <h3>{title}</h3>}
          {actions}
        </header>
      )}
      {children}
    </section>
  )
}

export function Stat({ label, value, sub }: { label: ReactNode; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="stat">
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {sub !== undefined && <div className="stat-sub">{sub}</div>}
    </div>
  )
}

export function KV({ rows }: { rows: [ReactNode, ReactNode][] }) {
  return (
    <dl className="kv">
      {rows
        .filter(([, v]) => v !== undefined && v !== null && v !== '')
        .map(([k, v], i) => (
          <div key={i}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </div>
        ))}
    </dl>
  )
}

export function Addr({ a, title }: { a?: string | number | null; title?: string }) {
  if (a === undefined || a === null || a === '') return null
  const s = typeof a === 'number' ? '0x' + a.toString(16).toUpperCase().padStart(8, '0') : a
  return (
    <code className="addr" title={title ?? 'GBA bus address (ROM offset = address − 0x08000000)'}>
      {s}
    </code>
  )
}

export function Tag({ children, kind }: { children: ReactNode; kind?: 'ok' | 'warn' | 'info' | 'bad' | 'muted' }) {
  return <span className={'tag ' + (kind ?? '')}>{children}</span>
}

/** Coloured evidence tag for VERIFIED / DISASM / INFERRED / measured text. */
export function Evidence({ text }: { text?: string | null }) {
  if (!text) return null
  const t = String(text)
  const kind = /VERIF|measured|EXACT/i.test(t) ? 'ok' : /INFER|assum|guess|AUTO/i.test(t) ? 'warn' : 'info'
  const short = t.match(/^(VERIFIED(\([^)]*\))?|DISASM|INFERRED|STATIC|measured|verified|AUTO)/i)?.[0]
  return (
    <span className={'tag ' + kind} title={t}>
      {short ?? (t.length > 40 ? t.slice(0, 40) + '…' : t)}
    </span>
  )
}

export function Loading({ what }: { what?: string }) {
  return <div className="loading">Loading{what ? ` ${what}` : ''}…</div>
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty-note">{children}</div>
}

export function Collapse({ title, children, open: o = false }: { title: ReactNode; children: ReactNode; open?: boolean }) {
  const [open, setOpen] = useState(o)
  return (
    <div className={'collapse' + (open ? ' open' : '')}>
      <button className="collapse-head" onClick={() => setOpen(!open)}>
        <span className="chev">{open ? '▾' : '▸'}</span> {title}
      </button>
      {open && <div className="collapse-body">{children}</div>}
    </div>
  )
}

export function JsonView({ value, max = 400 }: { value: unknown; max?: number }) {
  const s = JSON.stringify(value, null, 2) ?? ''
  const lines = s.split('\n')
  const [all, setAll] = useState(false)
  const shown = all ? lines : lines.slice(0, max)
  return (
    <div>
      <pre className="json">{shown.join('\n')}</pre>
      {lines.length > max && !all && (
        <button className="link" onClick={() => setAll(true)}>
          Show all {lines.length} lines
        </button>
      )}
    </div>
  )
}

export function Search({ value, onChange, placeholder }: { value: string; onChange: (s: string) => void; placeholder?: string }) {
  return (
    <input
      className="search"
      type="search"
      value={value}
      placeholder={placeholder ?? 'Search…'}
      onChange={(e) => onChange(e.target.value)}
    />
  )
}

export function Seg<T extends string | number>({
  value,
  options,
  onChange,
}: {
  value: T
  options: { value: T; label: ReactNode }[]
  onChange: (v: T) => void
}) {
  return (
    <div className="seg">
      {options.map((o) => (
        <button key={String(o.value)} className={o.value === value ? 'on' : ''} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  )
}

export function NumField({
  label,
  value,
  onChange,
  min,
  max,
  step = 1,
  hint,
  width = 90,
}: {
  label: ReactNode
  value: number
  onChange: (v: number) => void
  min?: number
  max?: number
  step?: number
  hint?: ReactNode
  width?: number
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <input
        type="number"
        value={Number.isFinite(value) ? value : 0}
        min={min}
        max={max}
        step={step}
        style={{ width }}
        onChange={(e) => {
          const v = e.target.value === '' ? 0 : Number(e.target.value)
          if (Number.isFinite(v)) onChange(v)
        }}
      />
      {hint && <small>{hint}</small>}
    </label>
  )
}

export function Slider({
  label,
  value,
  onChange,
  min,
  max,
  step = 1,
  suffix,
}: {
  label: ReactNode
  value: number
  onChange: (v: number) => void
  min: number
  max: number
  step?: number
  suffix?: ReactNode
}) {
  return (
    <label className="slider">
      <span>{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(+e.target.value)} />
      <b className="mono">
        {value}
        {suffix}
      </b>
    </label>
  )
}

export function Crumbs({ items }: { items: [string, string?][] }) {
  return (
    <nav className="crumbs">
      {items.map(([label, to], i) => (
        <span key={i}>
          {i > 0 && <span className="sep">/</span>}
          {to ? <Link to={to}>{label}</Link> : <span>{label}</span>}
        </span>
      ))}
    </nav>
  )
}

export function Bar({ value, max, color }: { value: number; max: number; color?: string }) {
  const w = max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0
  return (
    <div className="hbar">
      <div style={{ width: `${w}%`, background: color }} />
    </div>
  )
}
