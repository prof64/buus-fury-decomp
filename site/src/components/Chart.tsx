// Lightweight inline-SVG charts (no chart library). Colours are the categorical slots defined as CSS
// variables --s1..--s8 in index.css (validated palette, separate light/dark steps).
import { useMemo, useRef, useState, type ReactNode } from 'react'
import { fmt } from '../lib/util'

export interface Series {
  name: string
  color?: string
  points: [number, number][]
  /** optional min/max band drawn behind the line: [x, lo, hi] */
  band?: [number, number, number][]
  dashed?: boolean
}

const SLOT = (i: number) => `var(--s${(i % 8) + 1})`

function niceTicks(lo: number, hi: number, n = 5): number[] {
  if (hi <= lo) return [lo]
  const span = hi - lo
  const step0 = span / n
  const mag = 10 ** Math.floor(Math.log10(step0))
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => span / s <= n) ?? step0
  const out: number[] = []
  for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-9; v += step) out.push(+v.toPrecision(12))
  return out
}

function short(v: number): string {
  const a = Math.abs(v)
  if (a >= 1e9) return (v / 1e9).toFixed(a >= 1e10 ? 0 : 1) + 'B'
  if (a >= 1e6) return (v / 1e6).toFixed(a >= 1e7 ? 0 : 1) + 'M'
  if (a >= 1e4) return (v / 1e3).toFixed(0) + 'k'
  return fmt(v, 2)
}

export function LineChart({
  series,
  height = 240,
  xLabel,
  yLabel,
  logY,
  yMin,
  marker,
  title,
  xFormat,
}: {
  series: Series[]
  height?: number
  xLabel?: string
  yLabel?: string
  logY?: boolean
  yMin?: number
  marker?: number
  title?: ReactNode
  xFormat?: (x: number) => string
}) {
  const W = 640
  const H = height
  const m = { l: 56, r: 16, t: 12, b: 34 }
  const ref = useRef<SVGSVGElement>(null)
  const [hx, setHx] = useState<number | null>(null)
  const { xs, x0, x1, y0, y1 } = useMemo(() => {
    const xs = new Set<number>()
    let x0 = Infinity,
      x1 = -Infinity,
      y0 = Infinity,
      y1 = -Infinity
    for (const s of series) {
      for (const [x, y] of s.points) {
        xs.add(x)
        x0 = Math.min(x0, x)
        x1 = Math.max(x1, x)
        y0 = Math.min(y0, y)
        y1 = Math.max(y1, y)
      }
      for (const [, lo, hi] of s.band ?? []) {
        y0 = Math.min(y0, lo)
        y1 = Math.max(y1, hi)
      }
    }
    if (yMin !== undefined) y0 = Math.min(y0, yMin)
    else if (!logY) y0 = Math.min(0, y0)
    if (logY) y0 = Math.max(y0, 1)
    return { xs: [...xs].sort((a, b) => a - b), x0, x1, y0, y1 }
  }, [series, logY, yMin])
  if (!isFinite(x0)) return null
  const ly = (v: number) => (logY ? Math.log10(Math.max(v, 1)) : v)
  const Y0 = ly(y0)
  const Y1 = ly(y1) === Y0 ? Y0 + 1 : ly(y1)
  const sx = (x: number) => m.l + ((x - x0) / (x1 - x0 || 1)) * (W - m.l - m.r)
  const sy = (y: number) => m.t + (1 - (ly(y) - Y0) / (Y1 - Y0)) * (H - m.t - m.b)
  const yt = logY
    ? niceTicks(Y0, Y1, 5)
        .filter((t) => Number.isInteger(t))
        .map((t) => 10 ** t)
    : niceTicks(y0, y1, 5)
  const xt = niceTicks(x0, x1, 8)
  const path = (pts: [number, number][]) => pts.map(([x, y], i) => `${i ? 'L' : 'M'}${sx(x).toFixed(1)},${sy(y).toFixed(1)}`).join('')
  const onMove = (e: React.PointerEvent) => {
    const svg = ref.current
    if (!svg) return
    const r = svg.getBoundingClientRect()
    const px = ((e.clientX - r.left) / r.width) * W
    const xv = x0 + ((px - m.l) / (W - m.l - m.r)) * (x1 - x0)
    let best = xs[0]
    for (const x of xs) if (Math.abs(x - xv) < Math.abs(best - xv)) best = x
    setHx(best)
  }
  const hoverVals = hx === null ? [] : series.map((s, i) => ({ s, i, p: s.points.find((p) => p[0] === hx), b: s.band?.find((b) => b[0] === hx) }))
  const xf = xFormat ?? ((x: number) => fmt(x))
  return (
    <figure className="chart">
      {title && <figcaption>{title}</figcaption>}
      {series.length > 1 && (
        <div className="legend">
          {series.map((s, i) => (
            <span key={s.name}>
              <i style={{ background: s.color ?? SLOT(i) }} className={s.dashed ? 'dashed' : ''} />
              {s.name}
            </span>
          ))}
        </div>
      )}
      <div className="chart-wrap">
        <svg ref={ref} viewBox={`0 0 ${W} ${H}`} onPointerMove={onMove} onPointerLeave={() => setHx(null)} role="img">
          {yt.map((t) => (
            <g key={t}>
              <line x1={m.l} x2={W - m.r} y1={sy(t)} y2={sy(t)} className="grid" />
              <text x={m.l - 6} y={sy(t) + 4} textAnchor="end" className="tick">
                {short(t)}
              </text>
            </g>
          ))}
          {xt.map((t) => (
            <text key={t} x={sx(t)} y={H - m.b + 16} textAnchor="middle" className="tick">
              {xf(t)}
            </text>
          ))}
          <line x1={m.l} x2={W - m.r} y1={H - m.b} y2={H - m.b} className="axis" />
          {xLabel && (
            <text x={(m.l + W - m.r) / 2} y={H - 4} textAnchor="middle" className="axis-label">
              {xLabel}
            </text>
          )}
          {yLabel && (
            <text x={12} y={(m.t + H - m.b) / 2} textAnchor="middle" className="axis-label" transform={`rotate(-90 12 ${(m.t + H - m.b) / 2})`}>
              {yLabel}
            </text>
          )}
          {series.map((s, i) =>
            s.band?.length ? (
              <path
                key={'b' + s.name}
                d={
                  s.band.map(([x, , hi], j) => `${j ? 'L' : 'M'}${sx(x)},${sy(hi)}`).join('') +
                  [...s.band].reverse().map(([x, lo]) => `L${sx(x)},${sy(lo)}`).join('') +
                  'Z'
                }
                fill={s.color ?? SLOT(i)}
                opacity={0.16}
              />
            ) : null,
          )}
          {series.map((s, i) => (
            <path
              key={s.name}
              d={path(s.points)}
              fill="none"
              stroke={s.color ?? SLOT(i)}
              strokeWidth={2}
              strokeDasharray={s.dashed ? '5 4' : undefined}
              strokeLinejoin="round"
            />
          ))}
          {marker !== undefined && marker >= x0 && marker <= x1 && (
            <line x1={sx(marker)} x2={sx(marker)} y1={m.t} y2={H - m.b} className="marker" />
          )}
          {hx !== null && (
            <g>
              <line x1={sx(hx)} x2={sx(hx)} y1={m.t} y2={H - m.b} className="crosshair" />
              {hoverVals.map(({ s, i, p }) =>
                p ? <circle key={s.name} cx={sx(p[0])} cy={sy(p[1])} r={4} fill={s.color ?? SLOT(i)} stroke="var(--panel)" strokeWidth={2} /> : null,
              )}
            </g>
          )}
        </svg>
        {hx !== null && (
          <div className="tooltip" style={{ left: `${(sx(hx) / W) * 100}%` }}>
            <b>
              {xLabel ? xLabel + ' ' : ''}
              {xf(hx)}
            </b>
            {hoverVals.map(({ s, i, p, b }) =>
              p ? (
                <div key={s.name}>
                  <i style={{ background: s.color ?? SLOT(i) }} />
                  {s.name}: <span className="mono">{fmt(p[1])}</span>
                  {b && (
                    <span className="muted mono">
                      {' '}
                      ({fmt(b[1])}–{fmt(b[2])})
                    </span>
                  )}
                </div>
              ) : null,
            )}
          </div>
        )}
      </div>
    </figure>
  )
}

export interface BarDatum {
  label: string
  value: number
  color?: string
  note?: string
}

/** Horizontal bar list: good for probability breakdowns with labels. */
export function BarList({ data, format, max }: { data: BarDatum[]; format?: (v: number) => string; max?: number }) {
  const mx = max ?? Math.max(...data.map((d) => d.value), 1e-9)
  const f = format ?? ((v: number) => fmt(v))
  return (
    <div className="barlist">
      {data.map((d, i) => (
        <div key={d.label + i} className="barlist-row" title={d.note}>
          <span className="barlist-label">{d.label}</span>
          <span className="barlist-track">
            <span style={{ width: `${Math.max(0, (d.value / mx) * 100)}%`, background: d.color ?? SLOT(0) }} />
          </span>
          <span className="barlist-val mono">{f(d.value)}</span>
        </div>
      ))}
    </div>
  )
}

/** Vertical histogram (e.g. damage distribution). */
export function Histogram({ bins, height = 160, xLabel, color }: { bins: { x0: number; x1: number; n: number }[]; height?: number; xLabel?: string; color?: string }) {
  const [hover, setHover] = useState<number | null>(null)
  if (!bins.length) return null
  const W = 640
  const H = height
  const m = { l: 10, r: 10, t: 10, b: 30 }
  const mx = Math.max(...bins.map((b) => b.n), 1e-12)
  const bw = Math.min(48, (W - m.l - m.r) / bins.length)
  m.l = Math.max(m.l, (W - bw * bins.length) / 2)
  m.r = m.l
  const total = bins.reduce((a, b) => a + b.n, 0)
  return (
    <figure className="chart">
      <div className="chart-wrap">
        <svg viewBox={`0 0 ${W} ${H}`} onPointerLeave={() => setHover(null)}>
          {bins.map((b, i) => {
            const h = (b.n / mx) * (H - m.t - m.b)
            return (
              <g key={i} onPointerEnter={() => setHover(i)}>
                <rect x={m.l + i * bw} y={m.t} width={bw} height={H - m.t - m.b} fill="transparent" />
                <rect
                  x={m.l + i * bw + 1}
                  y={H - m.b - h}
                  width={Math.max(1, bw - 2)}
                  height={h}
                  rx={Math.min(3, bw / 3)}
                  fill={color ?? 'var(--s1)'}
                  opacity={hover === null || hover === i ? 1 : 0.55}
                />
              </g>
            )
          })}
          <line x1={m.l} x2={W - m.r} y1={H - m.b} y2={H - m.b} className="axis" />
          <text x={m.l} y={H - 10} className="tick">
            {short(bins[0].x0)}
          </text>
          <text x={W - m.r} y={H - 10} className="tick" textAnchor="end">
            {short(bins[bins.length - 1].x1)}
          </text>
          {xLabel && (
            <text x={W / 2} y={H - 10} className="axis-label" textAnchor="middle">
              {xLabel}
            </text>
          )}
        </svg>
        {hover !== null && (
          <div className="tooltip" style={{ left: `${((m.l + (hover + 0.5) * bw) / W) * 100}%` }}>
            <b>
              {fmt(bins[hover].x0)}
              {bins[hover].x1 !== bins[hover].x0 ? ` – ${fmt(bins[hover].x1)}` : ''}
            </b>
            <div>{((bins[hover].n / total) * 100).toFixed(2)}%</div>
          </div>
        )}
      </div>
    </figure>
  )
}
