// Small geometry helpers shared by every ruleset. Coordinates are integer pixels unless a name says otherwise
// (entity positions are kept in 24.8 fixed point like the Webfoot engine: px * 256).

/** Facing / direction index used by all three games: 0 down, 1 up, 2 left, 3 right. */
export type Dir = 0 | 1 | 2 | 3
export const DIR_NAMES = ['down', 'up', 'left', 'right'] as const
export type DirName = (typeof DIR_NAMES)[number]

export const DIR_VEC: readonly (readonly [number, number])[] = [
  [0, 1],
  [0, -1],
  [-1, 0],
  [1, 0],
]

export interface Rect {
  x0: number
  y0: number
  x1: number
  y1: number
}

export function rect(x0: number, y0: number, x1: number, y1: number): Rect {
  return { x0, y0, x1, y1 }
}

/** Offset a rect given as [x0,y0,x1,y1] (as stored in the data files) by an integer position. */
export function rectAt(r: readonly number[] | Rect, x: number, y: number): Rect {
  if (Array.isArray(r)) return { x0: r[0] + x, y0: r[1] + y, x1: r[2] + x, y1: r[3] + y }
  const q = r as Rect
  return { x0: q.x0 + x, y0: q.y0 + y, x1: q.x1 + x, y1: q.y1 + y }
}

/** Strict overlap test used by the Webfoot hit code: x0 < hx1 && hx0 < x1 && y0 < hy1 && hy0 < y1. */
export function overlaps(a: Rect, b: Rect): boolean {
  return a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

export function rectCenter(r: Rect): [number, number] {
  return [(r.x0 + r.x1) >> 1, (r.y0 + r.y1) >> 1]
}

export function inflate(r: Rect, dx: number, dy: number = dx): Rect {
  return { x0: r.x0 - dx, y0: r.y0 - dy, x1: r.x1 + dx, y1: r.y1 + dy }
}

/** Arithmetic shift right that also works for negative numbers (like ARM `asrs`). */
export const asr = (v: number, n: number) => Math.floor(v / 2 ** n)


/** Dominant-axis facing from a delta (ties go vertical, like most WF code paths). */
export function facingToward(dx: number, dy: number): Dir {
  if (Math.abs(dx) > Math.abs(dy)) return dx > 0 ? 3 : 2
  return dy > 0 ? 0 : 1
}
