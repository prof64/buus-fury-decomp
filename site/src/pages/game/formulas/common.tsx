import type { ReactNode } from 'react'
import { Histogram } from '../../../components/Chart'
import { Addr, Evidence, Stat } from '../../../components/ui'
import { histogram, probWhere, stats, type Dist } from '../../../formulas/dist'
import { fmt, pct } from '../../../lib/util'

export interface SectionDef {
  id: string
  title: string
}

export function SideNav({ sections }: { sections: SectionDef[] }) {
  return (
    <nav className="side-nav">
      {sections.map((s) => (
        <button
          key={s.id}
          onClick={() => {
            const el = document.getElementById('f-' + s.id)
            if (el) window.scrollTo({ top: el.getBoundingClientRect().top + window.scrollY - 150, behavior: 'smooth' })
          }}
        >
          {s.title}
        </button>
      ))}
    </nav>
  )
}

export function Section({
  id,
  title,
  addr,
  evidence,
  children,
}: {
  id: string
  title: string
  addr?: string
  evidence?: string
  children: ReactNode
}) {
  return (
    <section id={'f-' + id} className="card2 formula-section">
      <header>
        <h3>{title}</h3>
        <div className="row">
          {addr && <Addr a={addr} />}
          <Evidence text={evidence} />
        </div>
      </header>
      {children}
    </section>
  )
}

export function Code({ children }: { children: string }) {
  return <div className="formula">{children.replace(/^\n/, '')}</div>
}

/** Summary tiles + histogram for a damage distribution. */
export function DistView({ dist, hp, unit = '', digits = 0, extra }: { dist: Dist; hp?: number; unit?: string; digits?: number; extra?: ReactNode }) {
  const s = stats(dist)
  const zero = probWhere(dist, (v) => v <= 0)
  return (
    <>
      <div className="stats">
        <Stat label="Min" value={fmt(+s.min.toFixed(digits)) + unit} />
        <Stat label="Average" value={fmt(+s.mean.toFixed(Math.max(digits, 1))) + unit} />
        <Stat label="Max" value={fmt(+s.max.toFixed(digits)) + unit} />
        <Stat label="P(no damage)" value={pct(zero)} />
        {hp !== undefined && hp > 0 && <Stat label="Hits to kill (avg)" value={s.mean > 0 ? Math.ceil(hp / s.mean) : '∞'} sub={`HP ${fmt(hp)}`} />}
        {extra}
      </div>
      <Histogram bins={histogram(dist, 48)} xLabel="damage per hit" />
    </>
  )
}
