// LoG1 HUD rebuilt from runtime captures (tools/extract/ui_export.py -> assets/log1/ui/gameplay/hud.png at 8,8;
// tools/battle/log1_icons.py -> move icons, feather medal). Ki bar rows 4-5 and HP bar rows 8-10, columns 16..59.
// While flying the game swaps the HUD for the feather medal and an "EE/MM" flight-energy box.
// The enemy bar at the bottom is a simulator addition (LoG1 never shows enemy HP).

import { imageNow } from '../../core/assets'
import type { Renderer } from '../../core/renderer'
import { drawText, textWidth } from '../../core/pixelfont'
import type { Enemy } from './enemy'
import type { Log1Game } from './game'
import type { Player } from './player'

export const HUD_IMG = 'assets/log1/ui/gameplay/hud.png'
export const ICONS = ['assets/log1/battle/icon_0.png', 'assets/log1/battle/icon_1.png', 'assets/log1/battle/icon_2.png']
export const FEATHER = 'assets/log1/battle/feather.png'

// 5x7 digits in the style of the flight box (yellow with an orange lower half)
const DIG: Record<string, string[]> = {
  '0': ['01110', '11011', '11011', '11011', '11011', '11011', '01110'],
  '1': ['00110', '01110', '00110', '00110', '00110', '00110', '01111'],
  '2': ['01110', '11011', '00011', '00110', '01100', '11000', '11111'],
  '3': ['11110', '00011', '00011', '01110', '00011', '00011', '11110'],
  '4': ['00110', '01110', '11110', '10110', '11111', '00110', '00110'],
  '5': ['11111', '11000', '11110', '00011', '00011', '11011', '01110'],
  '6': ['01110', '11000', '11110', '11011', '11011', '11011', '01110'],
  '7': ['11111', '00011', '00110', '00110', '01100', '01100', '01100'],
  '8': ['01110', '11011', '11011', '01110', '11011', '11011', '01110'],
  '9': ['01110', '11011', '11011', '01111', '00011', '00011', '01110'],
  '/': ['00001', '00011', '00110', '01100', '11000', '10000', '00000'],
}
function bigText(g: CanvasRenderingContext2D, s: string, x: number, y: number) {
  for (const ch of s) {
    const rows = DIG[ch]
    if (rows)
      rows.forEach((row, ry) => {
        for (let rx = 0; rx < 5; rx++)
          if (row[rx] === '1') {
            g.fillStyle = '#211000'
            g.fillRect(x + rx + 1, y + ry + 1, 1, 1)
          }
      })
    if (rows)
      rows.forEach((row, ry) => {
        for (let rx = 0; rx < 5; rx++)
          if (row[rx] === '1') {
            g.fillStyle = ry < 3 ? '#ffe700' : ry < 5 ? '#f7a500' : '#de6b00'
            g.fillRect(x + rx, y + ry, 1, 1)
          }
      })
    x += 7
  }
}

export function drawHud(r: Renderer, g: Log1Game, player: Player, enemies: Enemy[], phaseInfo: string) {
  const ctx = r.ctx
  const p = player.char
  if (g.flying) {
    const fe = imageNow(FEATHER)
    if (fe) ctx.drawImage(fe, 6, 12)
    const bx = 24
    const by = 8
    const bw = 56
    const bh = 24
    ctx.fillStyle = '#001000'
    ctx.fillRect(bx, by, bw, bh)
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(bx + 1, by + 1, bw - 2, bh - 2)
    ctx.fillStyle = '#000000'
    ctx.fillRect(bx + 2, by + 2, bw - 4, bh - 4)
    ctx.fillStyle = '#0800ff'
    ctx.fillRect(bx + 3, by + 3, bw - 6, bh - 6)
    const s = `${String(Math.min(99, g.flight)).padStart(2, '0')}/${String(Math.min(99, g.maxFlight)).padStart(2, '0')}`
    bigText(ctx, s, bx + 6, by + 8)
  } else {
    const ox = 8
    const oy = 8
    const hud = imageNow(HUD_IMG)
    if (hud) ctx.drawImage(hud, ox, oy)
    const bar = (rows: number[], v: number, max: number) => {
      const fill = max > 0 ? Math.max(0, Math.min(44, Math.ceil((44 * Math.max(0, v)) / max))) : 0
      ctx.fillStyle = '#212121'
      for (const row of rows) ctx.fillRect(ox + 16 + fill, oy + row, 44 - fill, 1)
    }
    bar([4, 5], p.ki, p.maxKi)
    bar([8, 9, 10], p.hp, p.maxHp)
    const icon = imageNow(ICONS[g.selected] ?? ICONS[0])
    if (icon) ctx.drawImage(icon, ox + 63, oy + 3)
    if (g.selected === 2 && g.fc < g.sfReady) {
      ctx.fillStyle = 'rgba(0,0,0,0.55)'
      ctx.fillRect(ox + 64, oy + 4, 8, 8)
    }
  }
  // simulator additions: level / numbers / inventory, enemy bar
  const ty = g.flying ? 35 : 27
  const info = `LV${g.level}  HP ${p.hp}  KI ${p.ki}`
  drawText(ctx, info, 9, ty, '#fff')
  const inv = [g.herbs ? `HERB x${g.herbs}` : '', g.senzu ? `SENZU x${g.senzu}` : '', !g.flying ? `FLY ${g.flight}` : ''].filter(Boolean).join('  ')
  if (inv) drawText(ctx, inv, 9, ty + 7, '#cfe8ff')
  if (g.solar > 0) drawText(ctx, `FROZEN ${g.solar}`, r.viewW - 4 - textWidth(`FROZEN ${g.solar}`), 9, '#ffe700')

  const alive = enemies.filter((e) => !e.defeated)
  const target = alive[0]
  if (target) {
    const w = 120
    const x = (r.viewW - w) >> 1
    const y = r.viewH - 12
    const name = alive.length > 1 ? `${target.label} x${alive.length}` : target.label
    drawText(ctx, name.toUpperCase().slice(0, 28), x, y - 7, '#fff')
    if (phaseInfo) drawText(ctx, phaseInfo, x + w - textWidth(phaseInfo), y - 7, '#ffd23f')
    ctx.fillStyle = '#000'
    ctx.fillRect(x - 1, y - 1, w + 2, 5)
    const tot = alive.reduce((a, e) => a + Math.max(0, e.char.hp), 0)
    const max = alive.reduce((a, e) => a + e.char.maxHp, 0)
    ctx.fillStyle = '#5a0000'
    ctx.fillRect(x, y, w, 3)
    ctx.fillStyle = '#ff4a4a'
    ctx.fillRect(x, y, Math.ceil((w * Math.min(tot, max)) / Math.max(1, max)), 3)
  }
}
