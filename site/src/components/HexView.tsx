// Byte-level view of an encoded ROM structure, coloured per field.
import { useState } from 'react'

export interface ByteField {
  name: string
  offset: number
  size: number
  bytes: number[]
  desc?: string
  value?: string
}

const COLORS = ['var(--s1)', 'var(--s2)', 'var(--s3)', 'var(--s4)', 'var(--s5)', 'var(--s6)', 'var(--s7)', 'var(--s8)']

export function HexView({ fields, base }: { fields: ByteField[]; base?: number }) {
  const [hover, setHover] = useState<number | null>(null)
  const bytes: { b: number; f: number }[] = []
  fields.forEach((f, fi) => f.bytes.forEach((b) => bytes.push({ b, f: fi })))
  const rows: { b: number; f: number }[][] = []
  for (let i = 0; i < bytes.length; i += 16) rows.push(bytes.slice(i, i + 16))
  const hexStr = bytes.map((x) => x.b.toString(16).padStart(2, '0').toUpperCase()).join(' ')
  return (
    <div className="hexview">
      <div className="hex-grid mono">
        {rows.map((r, ri) => (
          <div key={ri} className="hex-row">
            <span className="hex-off">{base !== undefined ? (base + ri * 16).toString(16).toUpperCase().padStart(8, '0') : (ri * 16).toString(16).padStart(4, '0')}</span>
            {r.map((x, i) => (
              <span
                key={i}
                className={'hex-b' + (hover === x.f ? ' hl' : '')}
                style={{ borderBottomColor: COLORS[x.f % 8] }}
                onPointerEnter={() => setHover(x.f)}
                onPointerLeave={() => setHover(null)}
              >
                {x.b.toString(16).padStart(2, '0').toUpperCase()}
              </span>
            ))}
          </div>
        ))}
      </div>
      <table className="hex-fields">
        <thead>
          <tr>
            <th>off</th>
            <th>size</th>
            <th>field</th>
            <th>value</th>
            <th>bytes</th>
          </tr>
        </thead>
        <tbody>
          {fields.map((f, i) => (
            <tr key={i} className={hover === i ? 'hl' : ''} onPointerEnter={() => setHover(i)} onPointerLeave={() => setHover(null)}>
              <td className="mono">+0x{f.offset.toString(16).toUpperCase().padStart(2, '0')}</td>
              <td className="mono">{f.size}</td>
              <td>
                <i className="swatch" style={{ background: COLORS[i % 8] }} />
                {f.name}
                {f.desc && <div className="muted small">{f.desc}</div>}
              </td>
              <td className="mono">{f.value}</td>
              <td className="mono small">{f.bytes.map((b) => b.toString(16).padStart(2, '0').toUpperCase()).join(' ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="hex-copy">
        <code className="mono">{hexStr}</code>
        <button onClick={() => navigator.clipboard?.writeText(hexStr)}>Copy hex</button>
      </div>
    </div>
  )
}

/** Little-endian encoders. Values are wrapped to the field width (two's complement for signed). */
export function le(value: number, size: number): number[] {
  let v = Math.trunc(value)
  if (v < 0) v = 2 ** (size * 8) + v
  const out: number[] = []
  for (let i = 0; i < size; i++) {
    out.push(Math.floor(v / 2 ** (8 * i)) & 0xff)
  }
  return out
}

export class StructBuilder {
  fields: ByteField[] = []
  off = 0
  add(name: string, size: number, value: number, desc?: string, display?: string) {
    this.fields.push({ name, offset: this.off, size, bytes: le(value, size), desc, value: display ?? String(value) })
    this.off += size
    return this
  }
  ptr(name: string, value: number, desc?: string) {
    return this.add(name, 4, value, desc, '0x' + (value >>> 0).toString(16).toUpperCase().padStart(8, '0'))
  }
}
