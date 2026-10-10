import { marked } from 'marked'
import { useLayoutEffect, useMemo, useRef, useState } from 'react'
import { slug } from '../lib/util'

export function Markdown({ text }: { text: string }) {
  const html = useMemo(() => marked.parse(text, { async: false, gfm: true }) as string, [text])
  return <div className="md" dangerouslySetInnerHTML={{ __html: html }} />
}

interface TocItem {
  id: string
  text: string
  depth: number
}

/** Markdown document with a sticky table of contents built from its headings. Links scroll instead of
 *  changing the URL hash (the app uses a HashRouter). */
export function MarkdownWithToc({ text, filter }: { text: string; filter?: string }) {
  const ref = useRef<HTMLDivElement>(null)
  const [toc, setToc] = useState<TocItem[]>([])
  const [active, setActive] = useState<string>()
  const html = useMemo(() => marked.parse(text, { async: false, gfm: true }) as string, [text])
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const seen = new Map<string, number>()
    const items: TocItem[] = []
    el.querySelectorAll('h1,h2,h3,h4').forEach((h) => {
      let id = slug(h.textContent ?? '') || 'section'
      const n = seen.get(id) ?? 0
      seen.set(id, n + 1)
      if (n) id += '-' + n
      h.id = id
      items.push({ id, text: h.textContent ?? '', depth: +h.tagName[1] })
    })
    // wrap wide tables so they scroll instead of overflowing on phones
    el.querySelectorAll('table').forEach((t) => {
      if (t.parentElement?.classList.contains('md-table')) return
      const w = document.createElement('div')
      w.className = 'md-table'
      t.parentElement?.insertBefore(w, t)
      w.appendChild(t)
    })
    setToc(items)
    const obs = new IntersectionObserver(
      (ents) => {
        const vis = ents.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)
        if (vis[0]) setActive(vis[0].target.id)
      },
      { rootMargin: '-80px 0px -70% 0px' },
    )
    el.querySelectorAll('h1,h2,h3').forEach((h) => obs.observe(h))
    return () => obs.disconnect()
  }, [html])
  const go = (id: string) => {
    const h = document.getElementById(id)
    if (h) window.scrollTo({ top: h.getBoundingClientRect().top + window.scrollY - 70, behavior: 'smooth' })
  }
  const q = (filter ?? '').toLowerCase()
  return (
    <div className="doc-layout">
      <aside className="toc">
        <div className="toc-inner">
          {toc
            .filter((t) => t.depth <= 3 && (!q || t.text.toLowerCase().includes(q)))
            .map((t) => (
              <button key={t.id} className={'toc-item d' + t.depth + (active === t.id ? ' on' : '')} onClick={() => go(t.id)}>
                {t.text}
              </button>
            ))}
        </div>
      </aside>
      <div ref={ref} className="md" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  )
}
