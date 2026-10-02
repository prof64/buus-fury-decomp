// Buu's Fury HUD, rebuilt from the captured in-game HUD (site/public/assets/buu/ui/gameplay/hud.png, 80x16 at (8,8))
// and the skill icons of the skill table (+4 / +8 icon pointers, tools/battle/extract_buu_icons.py): the icon is the
// left 32x16 of the HUD (the "dim" icon when the skill cannot be used). Bars (measured on the capture):
// HP rows 3..5 starting at x 35/34/33, EP rows 7..9 starting at x 31/30/29, 41 px long, slanted; XP row 13.
// While transformed the EP bar shows the SS / SS3 gauge (energy moves spend it). The enemy bar at the bottom and
// the numbers under the HUD are simulator additions.

import { imageNow } from '../../core/assets'
import type { Renderer } from '../../core/renderer'
import { drawText, textWidth } from '../../core/pixelfont'
import type { BuuCtx } from './context'
import type { Enemy } from './enemy'
import type { Player } from './player'

export const HUD = 'assets/buu/ui/gameplay/hud.png'
const LEN = 41

export function drawHud(r: Renderer, ctx: BuuCtx, p: Player, enemies: Enemy[], phaseInfo: string) {
  const g = r.ctx
  const ox = 8
  const oy = 8
  const hud = imageNow(HUD)
  if (hud) g.drawImage(hud, ox, oy)
  else {
    g.fillStyle = '#222'
    g.fillRect(ox, oy, 80, 16)
  }
  const sk = ctx.data.skills.get(p.skill)
  if (sk) {
    const icon = imageNow(p.canUseSkill(p.skill) ? sk.icon : sk.iconDim)
    if (icon) g.drawImage(icon, 0, 0, 32, 16, ox, oy, 32, 16)
  }
  const bar = (rows: [number, number][], v: number, max: number, color?: string) => {
    const fill = max > 0 ? Math.ceil((LEN * Math.max(0, Math.min(v, max))) / max) : 0
    for (const [row, x0] of rows) {
      if (color) {
        g.fillStyle = color
        g.fillRect(ox + x0, oy + row, fill, 1)
      }
      g.fillStyle = '#101010'
      g.fillRect(ox + x0 + fill, oy + row, LEN - fill, 1)
    }
  }
  bar([[3, 35], [4, 34], [5, 33]], p.hp, p.maxHp)
  if (p.form.ss3) bar([[7, 31], [8, 30], [9, 29]], p.ss3Gauge / 65536, p.maxEp, '#ffe040')
  else if (p.form.ss) bar([[7, 31], [8, 30], [9, 29]], p.ssGauge, p.maxEp, '#ffe040')
  else bar([[7, 31], [8, 30], [9, 29]], p.ep, p.maxEp)
  // XP bar (row 13): progress inside the level
  const tbl = ctx.data.xpTable
  const L = p.rec.level
  const lo = tbl[L - 50] ?? 0
  const hi = tbl[L - 49] ?? lo + 1
  const frac = L >= 200 ? 1 : Math.max(0, Math.min(1, (p.rec.xp - lo) / Math.max(1, hi - lo)))
  g.fillStyle = '#4060c0'
  g.fillRect(ox + 4, oy + 13, Math.round(70 * frac), 1)

  drawText(g, `LV${L}`, ox + 1, oy + 19, '#fff')
  drawText(g, `${p.hp}/${p.maxHp}`, ox + 22, oy + 19, '#ff9494')
  const en = p.form.ss3 ? `SS3 ${p.ss3Gauge >> 16}` : p.form.ss ? `SS ${p.ssGauge}` : `${p.ep}/${p.maxEp}`
  drawText(g, en, ox + 22, oy + 26, p.inSS ? '#ffe040' : '#8cff9c')
  if (p.holyLeft > 0) drawText(g, 'HOLY', ox + 62, oy + 19, '#9ff')
  if (ctx.candy > 0) drawText(g, 'CANDY', ox + 62, oy + 26, '#ff9ad0')

  // enemy bar (simulator addition)
  const alive = enemies.filter((e) => !e.defeated && !e.removed && !e.part)
  const target = alive.find((e) => e.primary) ?? alive[0]
  if (target) {
    // top right, clear of the playfield centre
    const w = 84
    const x = r.viewW - w - 8
    const y = 16
    const same = alive.filter((e) => e.type.type === target.type.type || (e.primary && target.primary))
    // the game's own name (without our "(qualifier)"), as the scouter / database shows it
    const label = target.type.gameName ?? target.label
    const name = same.length > 1 ? `${label} x${same.length}` : label
    drawText(g, name.toUpperCase().slice(0, 21), x, y - 7, '#fff')
    if (phaseInfo) drawText(g, phaseInfo, x + w - textWidth(phaseInfo), y + 6, '#ffd23f')
    g.fillStyle = '#000'
    g.fillRect(x - 1, y - 1, w + 2, 5)
    const tot = same.reduce((a, e) => a + Math.max(0, e.hp1024), 0)
    const max = same.reduce((a, e) => a + e.maxHp1024, 0)
    g.fillStyle = '#5a0000'
    g.fillRect(x, y, w, 3)
    g.fillStyle = '#ff4a4a'
    g.fillRect(x, y, Math.min(w, Math.ceil((w * tot) / Math.max(1, max))), 3)
  }
}
