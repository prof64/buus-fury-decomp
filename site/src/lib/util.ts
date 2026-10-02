// Small shared helpers: number formatting, hex, exact integer shifts.

/** Raw JSON is heterogeneous across the three games; this alias marks deliberate untyped access. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type J = any

/** Arithmetic right shift that stays exact beyond 32 bits (JS `>>` truncates to int32). */
export function shr(x: number, k: number): number {
  return Math.floor(x / 2 ** k)
}

/** Unsigned 32-bit wrap of a non-negative integer product (exact while x < 2^53). */
export function u32(x: number): number {
  const m = x % 4294967296
  return m < 0 ? m + 4294967296 : m
}

/** int32 reinterpretation (for the LoG1 signed rand bug). */
export function i32(x: number): number {
  const v = u32(x)
  return v >= 2147483648 ? v - 4294967296 : v
}

export function hexNum(v: unknown): number | undefined {
  if (typeof v === 'number') return v
  if (typeof v === 'string' && /^0x[0-9a-f]+$/i.test(v.trim())) return parseInt(v, 16)
  return undefined
}

/** Normalises "0x86d5844" / "0x086D5844" to "0x086D5844". */
export function hx(v: unknown, digits = 8): string {
  const n = hexNum(v)
  if (n === undefined) return String(v ?? '')
  return '0x' + n.toString(16).toUpperCase().padStart(digits, '0')
}

export function romOffset(addr: number): string {
  return '0x' + (addr - 0x08000000).toString(16).toUpperCase().padStart(6, '0')
}

export function fmt(n: number | undefined | null, digits = 2): string {
  if (n === undefined || n === null || Number.isNaN(n)) return '–'
  if (Number.isInteger(n)) return n.toLocaleString('en-US')
  return n.toLocaleString('en-US', { maximumFractionDigits: digits })
}

export function pct(p: number, digits = 1): string {
  return (p * 100).toFixed(digits) + '%'
}

export function slug(s: string): string {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
}

export function titleCase(s: string): string {
  return s.replace(/[_]+/g, ' ').replace(/\b[a-z]/g, (c) => c.toUpperCase())
}

export function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v))
}

export function range(n: number, start = 0): number[] {
  return Array.from({ length: n }, (_, i) => i + start)
}

export const DIRS = ['down', 'up', 'left', 'right'] as const
export type Dir = (typeof DIRS)[number]

/** Downloads text as a file. */
export function download(name: string, text: string, type = 'application/json') {
  const a = document.createElement('a')
  a.href = URL.createObjectURL(new Blob([text], { type }))
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(a.href), 1000)
}
