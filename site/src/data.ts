// Loads reverse-engineering outputs from ../data/<game>/*.json and ../re/<game>/*.md.
// Everything is lazy so the dashboard only pulls what a page shows.
import { useEffect, useState } from 'react'

export const GAMES = [
  { id: 'log1', title: 'The Legacy of Goku', year: 2002, short: 'LoG I' },
  { id: 'log2', title: 'The Legacy of Goku II', year: 2003, short: 'LoG II' },
  { id: 'buu', title: "Buu's Fury", year: 2004, short: "Buu's Fury" },
] as const

export type GameId = (typeof GAMES)[number]['id']

const jsonFiles = import.meta.glob('../../data/*/*.json', { import: 'default' })
const mdFiles = import.meta.glob(['../../re/*/*.md', '../../tools/extract/*.md'], {
  query: '?raw',
  import: 'default',
})

function key(path: string) {
  const m = path.match(/data\/([^/]+)\/([^/]+)\.json$/)
  return m ? { game: m[1], name: m[2] } : null
}

export function listDataFiles(game: string): string[] {
  return Object.keys(jsonFiles)
    .map(key)
    .filter((k): k is { game: string; name: string } => !!k && k.game === game)
    .map((k) => k.name)
    .sort()
}

export function listDocs(game: string): string[] {
  return Object.keys(mdFiles)
    .filter((p) => p.includes(`/re/${game}/`))
    .map((p) => p.split('/').pop()!.replace(/\.md$/, ''))
    .sort((a, b) => (a === 'notes' ? -1 : b === 'notes' ? 1 : a.localeCompare(b)))
}

const cache = new Map<string, Promise<unknown>>()

export function loadData(game: string, name: string): Promise<unknown> {
  const path = `../../data/${game}/${name}.json`
  const loader = jsonFiles[path]
  if (!loader) return Promise.resolve(undefined)
  if (!cache.has(path)) cache.set(path, loader())
  return cache.get(path)!
}

export function loadDoc(game: string, name: string): Promise<string | undefined> {
  const path = game === 'graphics' ? `../../tools/extract/${name}.md` : `../../re/${game}/${name}.md`
  const loader = mdFiles[path]
  return loader ? (loader() as Promise<string>) : Promise.resolve(undefined)
}

export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): T | undefined {
  const [v, setV] = useState<T>()
  useEffect(() => {
    let alive = true
    setV(undefined)
    fn().then((x) => alive && setV(x))
    return () => {
      alive = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)
  return v
}

export function useData<T = unknown>(game: string, name: string) {
  return useAsync(() => loadData(game, name) as Promise<T>, [game, name])
}

/** Finds the main array of records inside a JSON file regardless of exact shape. */
export function recordsOf(json: unknown): Record<string, unknown>[] {
  if (Array.isArray(json)) return json as Record<string, unknown>[]
  if (json && typeof json === 'object') {
    const obj = json as Record<string, unknown>
    let best: unknown[] | undefined
    for (const v of Object.values(obj)) {
      if (Array.isArray(v) && v.length && typeof v[0] === 'object' && (!best || v.length > best.length)) best = v
    }
    if (best) return best as Record<string, unknown>[]
    // map of id -> record
    const vals = Object.values(obj)
    if (vals.length > 1 && vals.every((v) => v && typeof v === 'object' && !Array.isArray(v)))
      return Object.entries(obj).map(([k, v]) => ({ key: k, ...(v as object) }))
  }
  return []
}
