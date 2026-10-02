/**
 * Custom move format exported by the Move lab (see SCHEMA.md next to this file).
 *
 * The JSON is self-contained so the battle simulator (site/src/battle) can import it later without the
 * game's data files: damage inputs, costs, frame data, per-facing hitbox and optional projectile are all
 * explicit. `rom` is informative only — the bytes the move would occupy in the game's own tables.
 *
 *   {
 *     "$schema": "legacy-of-goku-x/custom-move@1",
 *     "game": "log2",                       // "log1" | "log2" | "buu"
 *     "name": "Big Punch",
 *     "template": "p0",                     // Moves-page key of the move it was cloned from
 *     "user": "player",                     // "player" | "enemy"
 *     "attack": {
 *       "kind": "melee",                    // "melee" (STR, can crit) | "energy" (POW)
 *       "power": 1536,                      // attack-table power, 1024 = 1.0   (LoG2, Buu)
 *       "attackId": 4,                      // LoG2 attack id byte / Buu attack sub id
 *       "scale": 1024,                      // per-hit multiplier: LoG2 hit scale, Buu hit mult (1024 = 1.0)
 *       "canCrit": true,
 *       "log1": { "mode": "maxhp_pct", "base": 384, "range": 128 }   // LoG1 only, see Log1Damage
 *     },
 *     "cost": { "unit": "EP", "min": 4, "start": 4, "perFrame": 0 },
 *     "timing": { "startupFrames": 0, "activeFrames": 1, "recoveryFrames": 16, "hits": 1, "rehitInterval": 0 },
 *     "hitbox": { "down": [0,0,16,16], "up": [0,-24,16,0], "left": [-8,-24,0,8], "right": [16,-24,24,8] },
 *     "projectile": null | { "speed": 3, "w": 16, "h": 16, "lifetime": 40, "pierce": false,
 *                            "spawn": { "down": [8,0], "up": [8,-8], "left": [-2,-8], "right": [18,-8] } },
 *     "effects": { "stunFrames": 0, "knockback": 16 },
 *     "rom": { "table": "attack table", "address": "0x086DB34C", "bytes": "00 10 00 00 00 06 00 00" }
 *   }
 *
 * Rectangles are [x0, y0, x1, y1] in pixels relative to the attacker's entity position (the sprite anchor),
 * tested with strict overlap against the defender's body rectangle. Spawn points use the same origin.
 */
import { StructBuilder, type ByteField } from '../components/HexView'
import type { MoveVM } from '../model/moves'
import { findRects } from '../model/moves'
import type { J } from '../lib/util'

export type Rect4 = [number, number, number, number]
export type Facing = 'down' | 'up' | 'left' | 'right'
export const FACINGS: Facing[] = ['down', 'up', 'left', 'right']

export interface Log1Damage {
  /** maxhp_pct: (maxHP × (base + rand2(range))) >> 9 · maxhp_div: maxHP / divisor · ki_spent: Ki charged · fixed: value · cur_hp: hp/4 + rand(hp/2) */
  mode: 'maxhp_pct' | 'maxhp_div' | 'ki_spent' | 'fixed' | 'cur_hp'
  base?: number
  range?: number
  divisor?: number
  fixed?: number
}

export interface CustomMove {
  $schema: 'legacy-of-goku-x/custom-move@1'
  game: 'log1' | 'log2' | 'buu'
  name: string
  template: string
  description?: string
  user: 'player' | 'enemy'
  attack: { kind: 'melee' | 'energy'; power: number; attackId: number; scale: number; canCrit: boolean; log1?: Log1Damage }
  cost: { unit: 'EP' | 'Ki'; min: number; start: number; perFrame: number }
  timing: { startupFrames: number; activeFrames: number; recoveryFrames: number; hits: number; rehitInterval: number }
  hitbox: Record<Facing, Rect4>
  projectile: null | { speed: number; w: number; h: number; lifetime: number; pierce: boolean; spawn: Record<Facing, [number, number]> }
  effects: { stunFrames: number; knockback: number }
  rom?: { table: string; address: string; bytes: string }
}

const PUNCH: Record<Facing, Rect4> = { down: [0, 0, 16, 16], up: [0, -24, 16, 0], left: [-8, -24, 0, 8], right: [16, -24, 24, 8] }

function num(v: J, d: number): number {
  const n = typeof v === 'number' ? v : parseFloat(String(v ?? ''))
  return Number.isFinite(n) ? n : d
}

/** Mirror a right-facing rect to the left around a body of width `bodyW` (16 px collision box). */
export function mirrorX(r: Rect4, bodyW = 16): Rect4 {
  return [bodyW - r[2], r[1], bodyW - r[0], r[3]]
}

function hitboxFrom(raw: J, fallback = PUNCH): Record<Facing, Rect4> {
  const hb = { ...fallback }
  const byFacing = raw?.hitbox_rect_by_facing ?? raw?.rects_by_facing ?? raw?.projectile?.rect_by_facing_at_spawn
  if (byFacing) {
    for (const f of FACINGS) if (Array.isArray(byFacing[f])) hb[f] = byFacing[f] as Rect4
    return hb
  }
  if (Array.isArray(raw?.rects_rel_player)) {
    for (const r of raw.rects_rel_player) hb[r.facing as Facing] = [r.x0, r.y0, r.x1, r.y1]
    return hb
  }
  const found = findRects(raw).filter((f) => f.label !== 'spawn point')
  const right = found.find((f) => (f.dir ?? 'right') === 'right')
  if (right) {
    const r: Rect4 = [right.rect.x0, right.rect.y0, right.rect.x1, right.rect.y1]
    hb.right = r
    hb.left = mirrorX(r)
    const down = found.find((f) => f.dir === 'down')
    if (down) hb.down = [down.rect.x0, down.rect.y0, down.rect.x1, down.rect.y1]
  }
  return hb
}

function spawnFrom(p: J): Record<Facing, [number, number]> {
  const s = p?.spawn_pos_by_facing ?? p?.spawn_offset_by_facing
  const out: Record<Facing, [number, number]> = { down: [8, 0], up: [8, -8], left: [-2, -8], right: [18, -8] }
  if (s) for (const f of FACINGS) if (Array.isArray(s[f])) out[f] = [s[f][0], s[f][1]]
  return out
}

/** Builds an editable custom move from one of the game's own moves. */
export function fromTemplate(game: CustomMove['game'], m: MoveVM, extra?: { attackTable?: J[]; buuTypes?: J[] }): CustomMove {
  const raw = m.raw
  const base: CustomMove = {
    $schema: 'legacy-of-goku-x/custom-move@1',
    game,
    name: 'My ' + m.name,
    template: m.key,
    description: m.description,
    user: m.user,
    attack: { kind: m.kind === 'melee' ? 'melee' : 'energy', power: m.power ?? 1024, attackId: 0, scale: 1024, canCrit: m.kind === 'melee' },
    cost: { unit: game === 'log1' ? 'Ki' : 'EP', min: 0, start: 0, perFrame: 0 },
    timing: { startupFrames: 0, activeFrames: 1, recoveryFrames: 16, hits: 1, rehitInterval: 0 },
    hitbox: { ...PUNCH },
    projectile: null,
    effects: { stunFrames: 0, knockback: 16 },
  }
  if (game === 'log1') {
    const id = raw.id
    base.attack.power = 0
    base.attack.canCrit = false
    if (id === 'melee') {
      base.attack.log1 = { mode: 'maxhp_pct', base: 384, range: 128 }
      const p = raw.hit_point_offsets_from_player_pos
      for (const f of FACINGS) base.hitbox[f] = [p[f][0] - 1, p[f][1] - 1, p[f][0] + 1, p[f][1] + 1]
      base.timing = { startupFrames: 0, activeFrames: 1, recoveryFrames: 16, hits: 1, rehitInterval: 0 }
    } else if (id === 'enemy_melee') {
      base.attack.log1 = { mode: 'cur_hp' }
      base.timing.recoveryFrames = 16 + 45
    } else if (id === 1) {
      base.attack.log1 = { mode: 'ki_spent' }
      base.cost = { unit: 'Ki', min: 0, start: 0, perFrame: 0 }
      base.timing = { startupFrames: 0, activeFrames: 16, recoveryFrames: 16, hits: 1, rehitInterval: 80 }
      const sp = raw.spawn_offset
      const so = raw.start_offset
      for (const f of FACINGS) {
        const x = sp[f].dx + so[f].dx
        const y = sp[f].dy + so[f].dy
        const g = raw.grow_per_frame[f]
        base.hitbox[f] = [x + g.dx * 16, y + g.dy * 16, x + g.dx * 16 + 8 + g.dw * 16, y + g.dy * 16 + 8 + g.dh * 16]
      }
    } else if (id === 2) {
      base.attack.log1 = { mode: 'fixed', fixed: 0 }
      base.effects.stunFrames = 360
    } else {
      base.attack.log1 = { mode: 'maxhp_div', divisor: 3 }
      const sp = raw.spawn_offset
      const fo = raw.fly_offset_and_velocity
      if (sp && fo) {
        const spawn = {} as Record<Facing, [number, number]>
        for (const f of FACINGS) {
          spawn[f] = [sp[f].dx + fo[f].dx + 8, sp[f].dy + fo[f].dy + 8]
          base.hitbox[f] = [spawn[f][0] - 8, spawn[f][1] - 8, spawn[f][0] + 8, spawn[f][1] + 8]
        }
        base.projectile = { speed: 3, w: 16, h: 16, lifetime: 72, pierce: false, spawn }
      } else base.projectile = { speed: 3, w: 16, h: 16, lifetime: 80, pierce: false, spawn: spawnFrom(null) }
    }
    return base
  }
  if (game === 'log2') {
    const tbl = extra?.attackTable ?? []
    const id = typeof m.attackId === 'number' ? m.attackId : raw.attack_table_id ?? raw.attack_table_ids?.[1] ?? raw.attack_id ?? 2
    const at = tbl.find((t) => t.id === id)
    base.attack = {
      kind: (at?.type ?? (m.kind === 'melee' ? 0 : 1)) === 0 ? 'melee' : 'energy',
      power: at?.power ?? m.power ?? 1024,
      attackId: at?.id_byte ?? id,
      scale: num(raw.scale, 1024),
      canCrit: (at?.type ?? 1) === 0,
    }
    if (raw.ep) base.cost = { unit: 'EP', min: num(raw.ep.min, 0), start: num(raw.ep.start, 0), perFrame: /86\/256/.test(String(raw.ep.drain)) ? 86 / 256 : /34\/256/.test(String(raw.ep.drain)) ? 34 / 256 : 0 }
    const t = raw.timing ?? {}
    base.timing = {
      startupFrames: num(t.startup_frames ?? t.charge_frames ?? t.wave_spawn_frame, 0),
      activeFrames: num(t.active_frames, 1),
      recoveryFrames: num(t.recovery_frames ?? t.recovery_frames_after_release ?? t.total_state_frames, 16),
      hits: num(raw.hits?.count, 1),
      rehitInterval: num(raw.hits?.rehit_interval_frames, 0),
    }
    base.hitbox = hitboxFrom(raw)
    const p = raw.projectile
    if (p && typeof p === 'object' && p.speed_px_per_frame !== undefined) {
      const size = Array.isArray(p.size) ? p.size : [16, 16]
      base.projectile = { speed: num(p.speed_px_per_frame, 3), w: size[0], h: size[1], lifetime: 40, pierce: !!raw.hits?.pierce, spawn: spawnFrom(p) }
    }
    if (m.user === 'enemy') {
      base.cost = { unit: 'EP', min: 0, start: 0, perFrame: 0 }
      const pp = raw.projectile_params ?? {}
      if (raw.projectile) base.projectile = { speed: num(pp.speed_px_per_frame ?? pp.speed, 3), w: 16, h: 16, lifetime: 60, pierce: false, spawn: spawnFrom(null) }
    }
    return base
  }
  // Buu
  const types = extra?.buuTypes ?? []
  if (m.key === 'melee') {
    base.attack = { kind: 'melee', power: 1024, attackId: 2, scale: 1024, canCrit: true }
    base.hitbox = hitboxFrom(raw)
    base.timing = { startupFrames: 0, activeFrames: 1, recoveryFrames: 17, hits: 1, rehitInterval: 0 }
    return base
  }
  const lv0 = raw.measured?.by_level?.['0']
  const sub = lv0?.attack_type_subs?.[0]?.[1] ?? 4
  const at = types.find((t) => t.sub === sub)
  base.attack = { kind: at?.kind === 0 ? 'melee' : 'energy', power: at?.power ?? 1024, attackId: sub, scale: lv0?.damage_multipliers?.[0] ?? 1024, canCrit: at?.kind === 0 && sub !== 3 }
  const costs = raw.ep?.ep_cost_by_level
  base.cost = { unit: 'EP', min: costs?.[0] ?? raw.ep?.canuse_min_ep ?? 0, start: costs?.[0] ?? 0, perFrame: 0 }
  base.timing = { startupFrames: lv0?.first_hit_frame ?? 0, activeFrames: 1, recoveryFrames: 24, hits: 1, rehitInterval: 0 }
  if (/beam/i.test(m.kind)) {
    base.projectile = { speed: 0, w: 32, h: 32, lifetime: 0, pierce: true, spawn: spawnFrom(null) }
    base.timing.rehitInterval = 1
  } else if (/projectile|blast/i.test(m.kind)) base.projectile = { speed: 3, w: 16, h: 16, lifetime: 80, pierce: false, spawn: spawnFrom(null) }
  return base
}

/* ---------------- ROM encodings ---------------- */

export interface Encoded {
  title: string
  address?: number
  where: string
  fields: ByteField[]
}

const rectStruct = (b: StructBuilder, hb: Record<Facing, Rect4>, size: 2 | 4) => {
  for (const f of FACINGS) {
    const [x0, y0, x1, y1] = hb[f]
    b.add(`${f}.x0`, size, x0, undefined, String(x0))
    b.add(`${f}.y0`, size, y0, undefined, String(y0))
    b.add(`${f}.x1`, size, x1, undefined, String(x1))
    b.add(`${f}.y1`, size, y1, undefined, String(y1))
  }
}

export function encode(m: CustomMove, template?: J): Encoded[] {
  const out: Encoded[] = []
  if (m.game === 'log1') {
    const sd = new StructBuilder()
    sd.add('w', 2, m.projectile?.w ?? 16, 'hitbox width (projectiles) / foot box (fighters)')
    sd.add('h', 2, m.projectile?.h ?? 16, 'hitbox height')
    rectStruct(sd, m.hitbox, 2)
    sd.ptr('palette', 0x086b3fdc, '16-colour palette (reuse the ki blast palette)')
    sd.ptr('anim table', 0x08018680, 'animation table: 4 strip pointers per state (reuse ki blast)')
    out.push({ title: 'Sprite definition (0x2C bytes)', where: 'free ROM space; pointer goes in fire_move sprite table 0x08011E38[new id]', fields: sd.fields })
    if (m.projectile) {
      const lt = new StructBuilder()
      const v: Record<Facing, [number, number]> = { down: [0, 1], up: [0, -1], left: [-1, 0], right: [1, 0] }
      for (const f of FACINGS) {
        lt.add(`${f}.dx`, 2, m.projectile.spawn[f][0] - Math.round(m.projectile.w / 2), "top-left = spawn centre − size/2")
        lt.add(`${f}.dy`, 2, m.projectile.spawn[f][1] - Math.round(m.projectile.h / 2))
        lt.add(`${f}.vx`, 2, v[f][0] * Math.round(m.projectile.speed), 'px/frame')
        lt.add(`${f}.vy`, 2, v[f][1] * Math.round(m.projectile.speed), 'px/frame')
      }
      out.push({ title: 'Launch table {dx, dy, vx, vy} × 4 facings', address: 0x08011e68, where: 'like the ki blast launch table 0x08011E68. Here dx,dy are relative to the owner; the game splits them into the spawn table 0x08011E50 + this launch offset', fields: lt.fields })
    }
    return out
  }
  if (m.game === 'log2') {
    const at = new StructBuilder()
    at.add('type', 1, m.attack.kind === 'melee' ? 0 : 1, '0 melee (STR, crit, melee mult) / 1 energy (POW)')
    at.add('id', 1, m.attack.attackId, 'id byte: receivers react to 9 (stun) and 15 (zero-damage stun)')
    at.add('pad', 2, 0)
    at.add('power', 4, m.attack.power, '1024 = basic punch')
    out.push({ title: 'Attack-table entry (8 bytes)', address: 0x086db2cc + 16 * 8, where: 'appended after the 16 entries of 0x086DB2CC (index 16) — check for free space or repoint the table', fields: at.fields })
    const rt = new StructBuilder()
    rectStruct(rt, m.hitbox, 4)
    out.push({ title: 'Hit rectangle table (s32 × 4 per facing)', where: 'like the punch table 0x083B5144 / special-melee table 0x083B5208', fields: rt.fields })
    const mt = new StructBuilder()
    const icon = template?.extra?.icon_gfx ?? ['0x086D8C7C', '0x086D8E7C']
    mt.ptr('icon (normal)', parseInt(icon[0], 16))
    mt.ptr('icon (alt)', parseInt(icon[1], 16))
    mt.ptr('canUse(rec, player)', parseInt(template?.extra?.can_use_fn ?? '0x0801315D', 16), `checks EP ≥ ${m.cost.min}`)
    mt.ptr('execute(rec, player)', parseInt(template?.extra?.execute_fn ?? '0x08013179', 16), `EP −= ${m.cost.start}; setState(...)`)
    out.push({ title: 'Move-type row (16 bytes)', address: 0x081d9900 + 14 * 16, where: 'after the 14 rows of 0x081D9900; then put the new type id in a character slot (record +0x18)', fields: mt.fields })
    if (m.projectile) {
      const pv = new StructBuilder()
      const v: Record<Facing, [number, number]> = { down: [0, 1], up: [0, -1], left: [-1, 0], right: [1, 0] }
      for (const f of FACINGS) {
        pv.add(`${f}.vx`, 4, Math.round(v[f][0] * m.projectile.speed * 256), '24.8 fixed px/f', String(v[f][0] * m.projectile.speed))
        pv.add(`${f}.vy`, 4, Math.round(v[f][1] * m.projectile.speed * 256), '24.8 fixed px/f', String(v[f][1] * m.projectile.speed))
      }
      out.push({ title: 'Projectile velocity table (24.8 per facing)', where: 'like 0x086DE904 (Big Bang) / 0x086E15CC (Burning Attack)', fields: pv.fields })
      const box = new StructBuilder()
      box.add('dx', 1, -Math.round(m.projectile.w / 2), 's8, rect = pos − (dx,dy)')
      box.add('dy', 1, -Math.round(m.projectile.h / 2))
      box.add('w', 1, m.projectile.w)
      box.add('h', 1, m.projectile.h)
      out.push({ title: 'Projectile frame box (projectile +0x18)', where: 'per-frame box {s8 dx, s8 dy, u8 w, u8 h} read by 0x08013BF4', fields: box.fields })
    }
    return out
  }
  // Buu
  const at = new StructBuilder()
  at.add('kind', 1, m.attack.kind === 'melee' ? 0 : 1, '0 melee (STR/20, crit) / 1 energy (POW, END2)')
  at.add('sub', 1, m.attack.attackId, 'sub id (3 = melee that cannot crit)')
  at.add('pad', 2, 0)
  at.add('power', 4, m.attack.power, '1024 = 1.0')
  out.push({ title: 'Attack type (8 bytes)', address: 0x086b8088 + 18 * 8, where: 'appended after the 18 entries of 0x086B8088', fields: at.fields })
  const sk = new StructBuilder()
  const raw: string | undefined = template?.extra?.raw
  const rb = raw ? raw.match(/../g)!.map((h: string) => parseInt(h, 16)) : []
  const u32 = (o: number) => (rb.length >= o + 4 ? rb[o] | (rb[o + 1] << 8) | (rb[o + 2] << 16) | (rb[o + 3] * 2 ** 24) : 0)
  const u16 = (o: number) => (rb.length >= o + 2 ? rb[o] | (rb[o + 1] << 8) : 0)
  sk.add('max level', 4, template?.raw?.max_level === 2 ? 2 : (u32(0) || 0xff), '2 = three levels, 0xFF = fixed', template?.raw?.max_level === 2 ? '2' : '0xFF')
  sk.ptr('icon', u32(4))
  sk.ptr('icon 2', u32(8))
  sk.add('name string', 2, u16(12), 'pick a free string id')
  sk.add('description string', 2, u16(14))
  sk.ptr('canUse(record)', u32(16), `EP ≥ ${m.cost.min} via 0x080134B8 (cost × rec+0x56/16)`)
  sk.ptr('activate(record, player)', u32(20), 'pushes the move’s player state')
  out.push({ title: 'Skill entry (24 bytes)', address: 0x0871daf8 + 24 * 24, where: 'after the skill table 0x0871DAF8; add the id to a character record (+0x0D.., bump +0x0C)', fields: sk.fields })
  const rt = new StructBuilder()
  rectStruct(rt, m.hitbox, 4)
  out.push({ title: 'Hit rectangle table (s32 × 4 per facing)', where: 'like the melee table 0x083C344C', fields: rt.fields })
  if (m.projectile && m.projectile.speed === 0) {
    const bd = new StructBuilder()
    bd.add('w', 1, m.projectile.w)
    bd.add('h', 1, m.projectile.h)
    bd.add('mult', 2, m.attack.scale, 'per-hit multiplier')
    bd.add('unk', 2, 0)
    bd.add('attack sub', 2, m.attack.attackId)
    bd.ptr('graphics', 0x08721188, 'reuse Kamehameha graphics')
    bd.ptr('animation', 0x0871d168)
    out.push({ title: 'Beam level definition (16 bytes)', address: 0x0871da68, where: 'one per level, like 0x0871DA68 + 16·level; fired by the beam object 0x080206B8', fields: bd.fields })
  }
  return out
}

export function hexOf(fields: ByteField[]): string {
  return fields
    .flatMap((f) => f.bytes)
    .map((b) => b.toString(16).padStart(2, '0').toUpperCase())
    .join(' ')
}

export function validate(j: J): string | null {
  if (!j || typeof j !== 'object') return 'not an object'
  if (j.$schema !== 'legacy-of-goku-x/custom-move@1') return 'missing "$schema": "legacy-of-goku-x/custom-move@1"'
  if (!['log1', 'log2', 'buu'].includes(j.game)) return 'game must be log1, log2 or buu'
  for (const k of ['attack', 'cost', 'timing', 'hitbox']) if (!j[k]) return `missing "${k}"`
  for (const f of FACINGS) if (!Array.isArray(j.hitbox[f]) || j.hitbox[f].length !== 4) return `hitbox.${f} must be [x0,y0,x1,y1]`
  return null
}
