import { marked } from 'marked'
import { useMemo } from 'react'

export function Markdown({ text }: { text: string }) {
  const html = useMemo(() => marked.parse(text, { async: false, gfm: true }) as string, [text])
  return <div className="md" dangerouslySetInnerHTML={{ __html: html }} />
}
