// Buu's Fury effect graphics: the game's own OBJ frame records (data/buu/sprites.json `fx_<addr>` runs, ROM order;
// `buu_beam_*` beam piece sets from tools/extract/buu_fx.py) drawn the way the game's draw routines place them.
//
// Record convention (base projectile draw 0x08019C44, generic effect 0x0802A81C): OBJ top-left = position - origin,
// i.e. the exported frame anchor sits on the object position -> drawRec(). Beam pieces (0x08020CF4) add the origin
// instead; their exported frames are already anchored on the piece position.

import { DIR_VEC, type Dir } from '../../core/geom'
import type { Renderer } from '../../core/renderer'
import { imageNow } from '../../core/assets'
import type { SpriteSet } from '../../core/sprites'
import type { Entity, World } from '../../core/world'

/** draw record `i` (index in ROM order inside the run) of an fx run with its origin on (x, y) (world px) */
export function drawRec(r: Renderer, set: SpriteSet | undefined, i: number, x: number, y: number, alpha = 1) {
  if (!set) return
  const st = set.anims.get('default')?.dirs[0][i]
  if (!st) return
  set.drawFrame(r.ctx, st.frame, r.sx(x), r.sy(y), st.flip, alpha === 1 ? {} : { alpha })
}

interface RecRef {
  set: SpriteSet
  frame: number
  flip: boolean
}
const recIndex = new WeakMap<object, Map<number, RecRef>>()

/** record at ROM address `addr` (any fx_<addr> run), found through the steps' `rom` fields */
export function recAt(bank: { byId: Map<string, SpriteSet> }, addr: number): RecRef | undefined {
  let m = recIndex.get(bank)
  if (!m) {
    m = new Map()
    for (const [id, set] of bank.byId) {
      if (!id.startsWith('fx_')) continue
      const steps = (set.json.animations.default?.directions.any ?? []) as { frame: number; flip: boolean; rom?: string }[]
      for (const st of steps) if (st.rom) m.set(parseInt(st.rom, 16), { set, frame: st.frame, flip: st.flip })
    }
    recIndex.set(bank, m)
  }
  return m.get(addr)
}

/** record geometry [ox, oy, w, h, hflip, vflip] (steps' `obj` field, tools/extract/misc_export.py) */
export type RecObjGeom = [number, number, number, number, boolean, boolean]
const recGeom = new WeakMap<object, Map<number, RecObjGeom>>()
export function recGeomAt(bank: { byId: Map<string, SpriteSet> }, addr: number): RecObjGeom | undefined {
  let m = recGeom.get(bank)
  if (!m) {
    m = new Map()
    for (const [id, set] of bank.byId) {
      if (!id.startsWith('fx_')) continue
      const steps = (set.json.animations.default?.directions.any ?? []) as { rom?: string; obj?: RecObjGeom }[]
      for (const st of steps) if (st.rom && st.obj) m.set(parseInt(st.rom, 16), st.obj)
    }
    recGeom.set(bank, m)
  }
  return m.get(addr)
}

const recCanvases = new Map<string, HTMLCanvasElement | null>()
/** the record as the game shows it (with its own flips) in a w x h canvas (null until the sheet is loaded) */
function recCanvas(bank: { byId: Map<string, SpriteSet> }, addr: number): [HTMLCanvasElement, RecObjGeom] | null {
  const g = recGeomAt(bank, addr)
  const ref = recAt(bank, addr)
  if (!g || !ref || typeof document === 'undefined') return null
  const key = `${ref.set.json.id}:${addr}`
  let c = recCanvases.get(key)
  if (!c) {
    if (!imageNowOf(ref.set)) return null
    c = document.createElement('canvas')
    c.width = g[2]
    c.height = g[3]
    ref.set.drawFrame(c.getContext('2d')!, ref.frame, g[0], g[1], ref.flip)
    recCanvases.set(key, c)
  }
  return [c, g]
}
function imageNowOf(set: SpriteSet): boolean {
  return !!imageNow(set.json.sheet)
}

/** draw a record with its OBJ top-left at (tlx, tly) (world px), with extra OAM flips (XOR with the record's) */
export function drawRecFlipped(r: Renderer, bank: { byId: Map<string, SpriteSet> }, addr: number, tlx: number, tly: number, hflip: boolean, vflip: boolean) {
  const rc = recCanvas(bank, addr)
  if (!rc) return
  const [c, g] = rc
  const ctx = r.ctx
  ctx.save()
  ctx.translate(r.sx(tlx) + (hflip ? g[2] : 0), r.sy(tly) + (vflip ? g[3] : 0))
  ctx.scale(hflip ? -1 : 1, vflip ? -1 : 1)
  ctx.drawImage(c, 0, 0)
  ctx.restore()
}

/** draw a record as an affine OBJ (no double size): OBJ box = position - origin, w x h; texture = the record's tiles
 *  without its flip bits; matrix diag(pa, pd) (8.8, screen -> texture around the box centre), clipped to the box */
export function drawRecAffine(r: Renderer, bank: { byId: Map<string, SpriteSet> }, addr: number, x: number, y: number, pa: number, pd: number) {
  const rc = recCanvas(bank, addr)
  if (!rc || !pa || !pd) return
  const [c, g] = rc
  const ctx = r.ctx
  const x0 = r.sx(x - g[0])
  const y0 = r.sy(y - g[1])
  ctx.save()
  ctx.beginPath()
  ctx.rect(x0, y0, g[2], g[3])
  ctx.clip()
  ctx.translate(x0 + g[2] / 2, y0 + g[3] / 2)
  ctx.scale(256 / pa, 256 / pd)
  // undo the record's own flips: the affine matrix ignores them
  ctx.scale(g[4] ? -1 : 1, g[5] ? -1 : 1)
  ctx.drawImage(c, -g[2] / 2, -g[3] / 2)
  ctx.restore()
}

/** draw the record at ROM address `addr` with its origin on (x, y) (world px) */
export function drawRecAddr(r: Renderer, bank: { byId: Map<string, SpriteSet> }, addr: number, x: number, y: number, alpha = 1) {
  const ref = recAt(bank, addr)
  if (ref) ref.set.drawFrame(r.ctx, ref.frame, r.sx(x), r.sy(y), ref.flip, alpha === 1 ? {} : { alpha })
}

/** number of records in an fx run */
export function recCount(set: SpriteSet | undefined): number {
  return set?.anims.get('default')?.dirs[0].length ?? 0
}

/** Generic animated effect object (ctor 0x0802A81C, vtable 0x0804CF3C): shows records start..start+n-1 of a run;
 *  every tick acc += rate, the record advances by acc >> 16 (acc &= 0xFFFF) and the object is removed once n records
 *  have elapsed. rate 0x2000 = 8 ticks per record. Never damages. */
export class FxAnim implements Entity {
  id = 0
  kind = 'fx'
  removed = false
  layer = 1
  set: SpriteSet | undefined
  x: number
  y: number
  rec: number
  left: number
  rate: number
  acc = 0
  /** draw-order offset (the game sorts OBJs by y; effects use the spawn point) */
  dy: number
  constructor(set: SpriteSet | undefined, x: number, y: number, start: number, n: number, rate: number, dy = 0) {
    this.set = set
    this.x = x
    this.y = y
    this.rec = start
    this.left = n
    this.rate = rate
    this.dy = dy
  }
  depth() {
    return this.y + this.dy
  }
  update() {
    this.acc += this.rate
    const adv = this.acc >> 16
    if (adv) {
      this.rec += adv
      this.left -= adv
      this.acc &= 0xffff
      if (this.left <= 0) this.removed = true
    }
  }
  draw(r: Renderer) {
    drawRec(r, this.set, this.rec, this.x, this.y)
  }
}

/** debris piece (class 0x0800C454, vtable 0x08049F38; bomb explosion 0x0801C750, breakable objects 0x0800C912):
 *  raw 8x8 OBJ 0x08072500 (buu_debris), top-left starts at the centre - 4, velocity by index (0x08072558: (96, 0),
 *  (-96, 0), (0, 64), (0, -64) in 1/256 px) gaining (+1, +2) per tick, 32 ticks */
const DEBRIS_V: [number, number][] = [[96, 0], [-96, 0], [0, 64], [0, -64]]
export class Debris implements Entity {
  id = 0
  kind = 'debris'
  removed = false
  layer = 1
  t = 0
  set: SpriteSet | undefined
  x: number
  y: number
  vx: number
  vy: number
  constructor(set: SpriteSet | undefined, cx: number, cy: number, i: number) {
    this.set = set
    this.x = (cx - 4) << 8
    this.y = (cy - 4) << 8
    ;[this.vx, this.vy] = DEBRIS_V[i & 3]
  }
  depth() {
    return (this.y >> 8) + 8
  }
  update() {
    this.vx += 1
    this.vy += 2
    this.x += this.vx
    this.y += this.vy
    if (++this.t >= 32) this.removed = true
  }
  draw(r: Renderer) {
    this.set?.drawFrame(r.ctx, 0, r.sx(this.x >> 8), r.sy(this.y >> 8), false)
  }
}

type Bank = { byId: Map<string, SpriteSet> }

/** Record object (class 0x080507A0, vtable 0x0804CA80): shows the record at ROM address `rec()` at `pos()`; removed
 *  when `alive()` turns false. Sorted by the OBJ bottom (y0 + h; approximated by the position + `dy`). */
export class RecObj implements Entity {
  id = 0
  kind = 'recobj'
  removed = false
  layer = 1
  t = 0
  bank: Bank
  pos: () => [number, number]
  rec: (t: number) => number
  alive: (t: number) => boolean
  dy: number
  constructor(bank: Bank, pos: () => [number, number], rec: (t: number) => number, alive: (t: number) => boolean, dy = 0) {
    this.bank = bank
    this.pos = pos
    this.rec = rec
    this.alive = alive
    this.dy = dy
  }
  depth() {
    return this.pos()[1] + this.dy
  }
  update() {
    this.t++
    if (!this.alive(this.t)) this.removed = true
  }
  draw(r: Renderer) {
    const [x, y] = this.pos()
    drawRecAddr(r, this.bank, this.rec(this.t), x, y)
  }
}

/** Looping generic effect (0x0802A81C with a subclass that wraps to the first record, e.g. vtable 0x0804B9E0 for the
 *  Super Saiyan aura): records first..first+n-1 at `rate`, until `alive()` turns false */
export class LoopFx extends FxAnim {
  first: number
  n: number
  alive: () => boolean
  constructor(set: SpriteSet | undefined, x: number, y: number, first: number, n: number, rate: number, alive: () => boolean, dy = 0) {
    super(set, x, y, first, n, rate, dy)
    this.first = first
    this.n = n
    this.alive = alive
  }
  update() {
    if (!this.alive()) {
      this.removed = true
      return
    }
    this.acc += this.rate
    const adv = this.acc >> 16
    if (adv) {
      this.acc &= 0xffff
      this.rec = this.first + ((this.rec - this.first + adv) % this.n)
    }
  }
}

// ---------------------------------------------------------------------------------------------------------------
// Beams (object 0x080206B8; draw 0x08020DAC, update 0x0802084C)

export interface BeamGfxJson {
  mode: number
  flags: number
  step: [number, number]
  charge_fx: string | null
  charge_index: number | null
}

/** beam object position relative to the player position (obj+0x1A0) per facing (down, up, left, right), measured in
 *  the emulator (re/buu/fx_beampos.py, Goku): charge = state 0 (sparkle), fire = state 1 */
export const BEAM_POS: Record<string, { charge: [number, number][]; fire: [number, number][] }> = {
  kame: { charge: [[8, -2], [12, -2], [12, -2], [4, -2]], fire: [[8, 16], [8, -16], [-16, -6], [32, -6]] },
  bbk: { charge: [[8, -2], [12, -2], [12, -2], [4, -2]], fire: [[8, 48], [8, -64], [-48, -6], [64, -6]] },
  banshee: { charge: [[2, 6], [13, -25], [-8, -6], [24, -6]], fire: [[4, 16], [12, -16], [-16, -6], [32, -6]] },
}

export class BeamGfx {
  readonly set: SpriteSet | undefined
  readonly charge: SpriteSet | undefined
  readonly info: BeamGfxJson | undefined
  constructor(set: SpriteSet | undefined, charge: SpriteSet | undefined) {
    this.set = set
    this.charge = charge
    this.info = (set?.json as unknown as { beam?: BeamGfxJson } | undefined)?.beam
  }
  /** state 0 (charging) sparkle: record charge_index for t < 4, then charge_index + 1 + ((t >> 2) & 1) */
  drawCharge(r: Renderer, x: number, y: number, t: number) {
    const i = this.info?.charge_index
    if (i == null) return
    drawRec(r, this.charge, t < 4 ? i : i + 1 + ((t >> 2) & 1), x, y)
  }
  private piece(r: Renderer, name: string, dir: Dir, variant: number, x: number, y: number) {
    const a = this.set?.anims.get(name)
    if (!a) return
    const st = a.dirs[dir][variant] ?? a.dirs[dir][0]
    if (!st) return
    this.set!.drawFrame(r.ctx, st.frame, r.sx(x), r.sy(y), st.flip)
  }
  /** draw the beam (0x08020DAC): `firing` = state 1 (start piece) else state 2 (retracting, tail piece);
   *  `stopped` = obj+0x11 (head stopped by a hit / wall -> impact piece); `t` = obj+0x18 tick counter */
  draw(r: Renderer, dir: Dir, x: number, y: number, len: number, t: number, firing: boolean, stopped: boolean) {
    const b = this.info
    if (!b) return
    const phase = t >> 2
    let k: number
    if (b.mode === 2) {
      k = phase & 3
      if (k === 3) k = 1
    } else k = phase & 1
    const bodyV = b.mode === 1 ? phase & 3 : k
    const [dx, dy] = DIR_VEC[dir]
    const at = (o: number): [number, number] => [x + dx * o, y + dy * o]
    if (firing) {
      this.piece(r, 'start', dir, k, x, y)
      if (b.flags & 1) this.piece(r, 'start2', dir, k, x, y)
    } else {
      this.piece(r, 'tail', dir, k, x, y)
      if (b.flags & 2) this.piece(r, 'tail2', dir, k, x, y)
    }
    const [hx, hy] = at(len)
    if (stopped) {
      this.piece(r, 'impact', dir, k & 1, hx, hy)
      if (b.flags & 8) this.piece(r, 'impact2', dir, k & 1, hx, hy)
    } else {
      this.piece(r, 'head', dir, k, hx, hy)
      if (b.flags & 4) this.piece(r, 'head2', dir, k, hx, hy)
    }
    const step = b.step[dir >> 1] || 32
    let o = 0
    while (len + 7 - step > o) {
      this.piece(r, 'body32', dir, bodyV, ...at(o))
      o += step
    }
    if (len - 9 > o) {
      this.piece(r, 'body16', dir, bodyV, ...at(o))
      o += 16
    }
    if (len - 1 > o) this.piece(r, 'body8', dir, bodyV, ...at(o))
  }
}

/** beam level definitions per player skill (beam ctor callers 0x0801A6A4 Banshee, 0x0801A930 BBK, 0x0801ADF0
 *  Electric Kamehameha 0x08725018 + 16 * level, 0x0801BE1A Kamehameha 0x0871DA68 + 16 * level, 0x0801BF86
 *  Kamekameha 0x087315A8 + 16 * level) -> buu_beam_* entities (tools/extract/buu_fx.py) */
const BEAM_SETS: Record<number, (lv: number) => string> = {
  1: () => 'buu_beam_banshee_blast',
  3: () => 'buu_beam_big_bang_kamehameha',
  6: (lv) => `buu_beam_electric_kamehameha_${Math.min(lv, 2) + 1}`,
  15: (lv) => `buu_beam_kamehameha_${Math.min(lv, 2) + 1}`,
  16: (lv) => `buu_beam_kamekameha_${Math.min(lv, 2) + 1}`,
}

/** the game's beam graphics for a player skill / level (null: not a beam-object skill) */
export function beamGfx(ctx: { fx(id: string): SpriteSet | undefined }, skill: number, lv: number): BeamGfx | null {
  const id = BEAM_SETS[skill]?.(lv)
  const set = id ? ctx.fx(id) : undefined
  if (!set) return null
  const g = new BeamGfx(set, undefined)
  const cf = g.info?.charge_fx
  return cf ? new BeamGfx(set, ctx.fx(cf)) : g
}

/** visual-only beam charge sparkle (the beam object in state 0, before it fires) */
export class BeamCharge implements Entity {
  id = 0
  kind = 'beamcharge'
  removed = false
  layer = 1
  t = 0
  gfx: BeamGfx
  pos: () => [number, number]
  constructor(gfx: BeamGfx, pos: () => [number, number]) {
    this.gfx = gfx
    this.pos = pos
  }
  depth() {
    return this.pos()[1]
  }
  update(_w: World) {
    this.t++
  }
  draw(r: Renderer) {
    const [x, y] = this.pos()
    this.gfx.drawCharge(r, x, y, this.t)
  }
}
