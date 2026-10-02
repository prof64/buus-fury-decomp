// LoG2 HUD, rebuilt from the captured in-game HUD (site/public/assets/log2/ui/gameplay/hud.png, drawn at (8,8))
// and the per-move-type icon tiles (move-type table 0x081D9900 icon_gfx, extracted by tools/battle/extract_icons.py;
// each icon is the right 32x16 part of the HUD, "alt" = greyed when the move can't be used).
// Bars: HP rows 2..4 and EP rows 7..9, columns 8..55 of the HUD; row 12 is the experience bar.
// The enemy bar at the bottom is a simulator addition (the game shows enemy HP only through the scouter).

import { imageNow } from '../../core/assets'
import type { Renderer } from '../../core/renderer'
import { drawText, textWidth } from '../../core/pixelfont'
import type { Log2Ctx } from './context'
import type { Enemy } from './enemy'
import type { Player } from './player'

const HUD = 'assets/log2/ui/gameplay/hud.png'

export function drawHud(r: Renderer, ctx: Log2Ctx, player: Player, enemies: Enemy[], phaseInfo: string) {
  const g = r.ctx
  const ox = 8
  const oy = 8
  const hud = imageNow(HUD)
  if (hud) g.drawImage(hud, ox, oy)
  const type = player.char.slots[player.slot]
  const mv = ctx.data.moves[type]
  const usable = canUse(player, type)
  const icon = imageNow(usable ? mv.icon : mv.iconDim)
  if (icon) g.drawImage(icon, ox + 48, oy)
  // bars: blank the empty part (bar pixels span x 8..55)
  const bar = (rows: number[], v: number, max: number) => {
    const fill = max > 0 ? Math.ceil((48 * Math.max(0, v)) / max) : 0
    g.fillStyle = '#000'
    for (const row of rows) g.fillRect(ox + 8 + fill, oy + row, 48 - fill, 1)
  }
  bar([2, 3, 4], player.hp, player.maxHp)
  bar([7, 8, 9], player.ep, player.maxEp)
  // experience bar (row 12): progress inside the current level
  const tbl = ctx.data.expTable
  const L = player.level
  const lo = tbl[L - 1] ?? 0
  const hi = tbl[L] ?? lo + 1
  const frac = L >= 50 ? 1 : Math.max(0, Math.min(1, (player.rec.exp - lo) / (hi - lo)))
  g.fillStyle = '#212142'
  g.fillRect(ox + 4, oy + 12, 56, 1)
  g.fillStyle = '#8cd8ff'
  g.fillRect(ox + 4, oy + 12, Math.round(56 * frac), 1)
  drawText(g, `LV${L}`, ox + 1, oy + 17, '#fff')
  drawText(g, `${player.hp}/${player.maxHp}`, ox + 22, oy + 17, '#ff9494')
  drawText(g, `${player.ep}`, ox + 62, oy + 17, '#8cff9c')

  // enemy bar (simulator addition)
  const alive = enemies.filter((e) => !e.defeated)
  const target = alive[0]
  if (target) {
    const w = 120
    const x = (r.viewW - w) >> 1
    const y = r.viewH - 12
    const name = alive.length > 1 ? `${target.label} x${alive.length}` : target.label
    drawText(g, name.toUpperCase().slice(0, 28), x, y - 7, '#fff')
    if (phaseInfo) drawText(g, phaseInfo, x + w - textWidth(phaseInfo), y - 7, '#ffd23f')
    g.fillStyle = '#000'
    g.fillRect(x - 1, y - 1, w + 2, 5)
    const tot = alive.reduce((a, e) => a + e.hp, 0)
    const max = alive.reduce((a, e) => a + e.maxHp, 0)
    g.fillStyle = '#5a0000'
    g.fillRect(x, y, w, 3)
    g.fillStyle = '#ff4a4a'
    g.fillRect(x, y, Math.ceil((w * tot) / Math.max(1, max)), 3)
  }
}

export function canUse(p: Player, type: number): boolean {
  const mv = p.ctx.data.moves[type]
  if (type === 11 || type === 12) return !p.form.transformLocked && (p.form.transformed || p.ep >= 10)
  if (type === 5) return false
  return p.ep >= mv.minEp
}
