// Tiny 3x5 bitmap font so in-game text (damage numbers, banners) stays crisp at the GBA's native resolution.

const G: Record<string, string> = {
  '0': '111101101101111',
  '1': '010110010010111',
  '2': '111001111100111',
  '3': '111001111001111',
  '4': '101101111001001',
  '5': '111100111001111',
  '6': '111100111101111',
  '7': '111001010010010',
  '8': '111101111101111',
  '9': '111101111001111',
  A: '010101111101101',
  B: '110101110101110',
  C: '011100100100011',
  D: '110101101101110',
  E: '111100110100111',
  F: '111100110100100',
  G: '011100101101011',
  H: '101101111101101',
  I: '111010010010111',
  J: '001001001101010',
  K: '101101110101101',
  L: '100100100100111',
  M: '101111111101101',
  N: '110101101101101',
  O: '010101101101010',
  P: '110101110100100',
  Q: '010101101110011',
  R: '110101110101101',
  S: '011100010001110',
  T: '111010010010010',
  U: '101101101101111',
  V: '101101101101010',
  W: '101101111111101',
  X: '101101010101101',
  Y: '101101010010010',
  Z: '111001010100111',
  '!': '010010010000010',
  '.': '000000000000010',
  ':': '000010000010000',
  '-': '000000111000000',
  '+': '000010111010000',
  '/': '001001010100100',
  '%': '101001010100101',
  ' ': '000000000000000',
  '?': '111001010000010',
  "'": '010010000000000',
  ',': '000000000010100',
}

export function textWidth(s: string, scale = 1): number {
  return s.length ? (s.length * 4 - 1) * scale : 0
}

/** Draw text with an optional 1px dark outline. (x, y) = top-left. */
export function drawText(ctx: CanvasRenderingContext2D, s: string, x: number, y: number, color = '#fff', outline: string | null = '#000', scale = 1) {
  const up = s.toUpperCase()
  const pass = (c: string, ox: number, oy: number) => {
    ctx.fillStyle = c
    let cx = x
    for (const ch of up) {
      const g = G[ch] ?? G['?']
      for (let i = 0; i < 15; i++) {
        if (g[i] === '1') ctx.fillRect(cx + (i % 3) * scale + ox, y + Math.floor(i / 3) * scale + oy, scale, scale)
      }
      cx += 4 * scale
    }
  }
  if (outline) {
    for (const [ox, oy] of [
      [-1, 0],
      [1, 0],
      [0, -1],
      [0, 1],
      [-1, -1],
      [1, 1],
      [-1, 1],
      [1, -1],
    ])
      pass(outline, ox * scale, oy * scale)
  }
  pass(color, 0, 0)
}
