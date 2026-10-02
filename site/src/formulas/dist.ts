// Discrete probability distributions over integer outcomes, used to show exact min/avg/max and
// histograms of the games' randomised formulas.

export type Dist = Map<number, number> // value -> probability

export function distFromCounts(counts: Map<number, number>): Dist {
  let total = 0
  for (const c of counts.values()) total += c
  const d: Dist = new Map()
  for (const [v, c] of counts) d.set(v, c / total)
  return d
}

export function addTo(d: Dist, v: number, p: number) {
  d.set(v, (d.get(v) ?? 0) + p)
}

export function mapDist(d: Dist, f: (v: number) => number): Dist {
  const o: Dist = new Map()
  for (const [v, p] of d) addTo(o, f(v), p)
  return o
}

export function stats(d: Dist) {
  let min = Infinity,
    max = -Infinity,
    mean = 0
  for (const [v, p] of d) {
    if (p <= 0) continue
    min = Math.min(min, v)
    max = Math.max(max, v)
    mean += v * p
  }
  return { min, max, mean }
}

export function probWhere(d: Dist, pred: (v: number) => boolean): number {
  let s = 0
  for (const [v, p] of d) if (pred(v)) s += p
  return s
}

/** Bins a distribution into at most n histogram bars. */
export function histogram(d: Dist, n = 40): { x0: number; x1: number; n: number }[] {
  const { min, max } = stats(d)
  if (!isFinite(min)) return []
  const vals = [...d.keys()].filter((v) => (d.get(v) ?? 0) > 0).sort((a, b) => a - b)
  if (vals.length <= n) return vals.map((v) => ({ x0: v, x1: v, n: d.get(v)! }))
  const ints = vals.every(Number.isInteger)
  const w = (max - min) / n
  const bins = Array.from({ length: n }, (_, i) => {
    const a = min + i * w
    const b = min + (i + 1) * w
    return ints ? { x0: Math.ceil(a), x1: Math.max(Math.ceil(a), Math.ceil(b) - 1), n: 0 } : { x0: +a.toFixed(2), x1: +b.toFixed(2), n: 0 }
  })
  for (const [v, p] of d) {
    const i = Math.min(n - 1, Math.floor((v - min) / w))
    bins[i].n += p
  }
  return bins
}

/** Probability that `hits` independent draws sum to at least `hp` (by convolution, capped at hp). */
export function killChance(d: Dist, hp: number, hits: number): number {
  let cur: Dist = new Map([[0, 1]])
  for (let i = 0; i < hits; i++) {
    const next: Dist = new Map()
    for (const [a, pa] of cur) {
      if (a >= hp) {
        addTo(next, hp, pa)
        continue
      }
      for (const [b, pb] of d) addTo(next, Math.min(hp, Math.max(0, a + b)), pa * pb)
    }
    cur = next
  }
  return probWhere(cur, (v) => v >= hp)
}

/** Smallest number of hits whose kill chance reaches p (null if beyond limit). */
export function hitsToKill(d: Dist, hp: number, p = 0.5, limit = 400): number | null {
  const { mean, max } = stats(d)
  if (max <= 0) return null
  // fast path for large counts: estimate by the mean, then refine around it
  const est = Math.max(1, Math.floor(hp / Math.max(mean, 1e-9)) - 2)
  if (est > limit) return null
  if (d.size * hp > 4e6) return Math.ceil(hp / mean)
  for (let h = Math.max(1, Math.min(est, Math.ceil(hp / max))); h <= limit; h++) if (killChance(d, hp, h) >= p) return h
  return null
}
