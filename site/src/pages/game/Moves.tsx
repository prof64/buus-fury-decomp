import { useMemo, useState, type ReactNode } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { useData } from '../../data'
import { useSprites, type SpriteEntity, type SpriteIndex } from '../../lib/sprites'
import { buuMoves, findRects, legendFor, log1Moves, log2Moves, overlaysFor, type MoveVM } from '../../model/moves'
import { SpritePlayer, type Overlay } from '../../components/SpritePlayer'
import { Addr, Card, Collapse, Evidence, JsonView, KV, Loading, Seg, Tag } from '../../components/ui'
import { fmt, hexNum, type J } from '../../lib/util'

/* ---------- attacker sprites for drawing hitboxes ---------- */

export function useAttackerSprites(game: string): { list: { label: string; e: SpriteEntity }[]; sp?: SpriteIndex } {
  const sp = useSprites(game)
  const chars = useData<J>(game, 'characters')
  return useMemo(() => {
    const list: { label: string; e: SpriteEntity }[] = []
    if (!sp || !chars) return { list, sp }
    if (game === 'log1') {
      for (const [k, v] of Object.entries<J>(chars.characters[0].sprites)) {
        const e = sp.byDef.get(hexNum(v.def_addr) ?? -1)
        if (e) list.push({ label: k, e })
      }
    } else if (game === 'log2') {
      for (const c of chars.characters) {
        const e = sp.byGfx(chars.forms[c.initial.form].actor_id)
        if (e) list.push({ label: c.name, e })
      }
    } else {
      for (const c of chars.characters) {
        const f = chars.forms[c.new_game.form]
        const e = (f && sp.byGfx(f.gfx_id)) ?? c.forms.map((i: number) => sp.byGfx(chars.forms[i].gfx_id)).find(Boolean)
        if (e) list.push({ label: c.name.split(' (')[0], e })
      }
    }
    return { list, sp }
  }, [sp, chars, game])
}

/** LoG1 move geometry: hit point, projectile spawn/flight boxes, Kamehameha beam at full length. */
export function log1Overlays(m: J, dir: string): Overlay[] {
  const out: Overlay[] = []
  if (m.hit_point_offsets_from_player_pos) {
    const [x, y] = m.hit_point_offsets_from_player_pos[dir]
    out.push({ rect: { x0: x - 1, y0: y - 1, x1: x + 1, y1: y + 1 }, color: '#ff3b3b', label: 'hit point' })
  }
  const sp = m.spawn_offset?.[dir]
  if (sp && m.fly_offset_and_velocity) {
    const f = m.fly_offset_and_velocity[dir]
    for (let t = 0; t <= 3; t++) {
      const x = sp.dx + f.dx + f.vx * t * 6
      const y = sp.dy + f.dy + f.vy * t * 6
      out.push({ rect: { x0: x, y0: y, x1: x + 8, y1: y + 8 }, color: t ? '#ff9f1c' : '#ff3b3b', label: t ? `+${t * 6} f` : 'release', dashed: t > 0 })
    }
  }
  if (sp && m.start_offset && m.grow_per_frame) {
    const so = m.start_offset[dir]
    const g = m.grow_per_frame[dir]
    const x = sp.dx + so.dx
    const y = sp.dy + so.dy
    out.push({ rect: { x0: x, y0: y, x1: x + 8, y1: y + 8 }, color: '#ff9f1c', label: 'beam start 8×8', dashed: true })
    const n = 16
    out.push({
      rect: { x0: x + g.dx * n, y0: y + g.dy * n, x1: x + g.dx * n + 8 + g.dw * n, y1: y + g.dy * n + 8 + g.dh * n },
      color: '#b04bff',
      label: 'beam after 16 growth frames',
    })
  }
  return out
}

function HitboxPanel({ game, move, attackers }: { game: string; move: MoveVM; attackers: { label: string; e: SpriteEntity }[] }) {
  const pick = useMemo(() => {
    const t = (move.characters ?? '').toLowerCase()
    let best = 0,
      at = Infinity
    attackers.forEach((a, i) => {
      const p = t.indexOf(a.label.toLowerCase())
      if (p >= 0 && p < at) {
        at = p
        best = i
      }
    })
    return best
  }, [move, attackers])
  const [ai, setAi] = useState<number | null>(null)
  const a = attackers[ai ?? pick]
  const found = useMemo(() => (game === 'log1' ? [] : findRects(move.raw)), [move, game])
  const legend = game === 'log1' ? [] : legendFor(found)
  if (!a) return null
  const overlays = (dir: string) => (game === 'log1' ? log1Overlays(move.raw, dir) : overlaysFor(found, dir))
  const any = game === 'log1' ? ['down', 'up', 'left', 'right'].some((d) => log1Overlays(move.raw, d).length) : found.length > 0
  return (
    <div>
      <div className="row" style={{ marginBottom: 8 }}>
        <select value={ai ?? pick} onChange={(e) => setAi(+e.target.value)}>
          {attackers.map((x, i) => (
            <option key={i} value={i}>
              {x.label}
            </option>
          ))}
        </select>
        {!any && <span className="muted small">No hitbox geometry recorded for this move.</span>}
      </div>
      <SpritePlayer key={a.e.id + move.key} entity={a.e} compact zoom={3} initialDir="right" overlays={overlays} />
      {legend.length > 0 && (
        <div className="legend" style={{ marginTop: 6 }}>
          {legend.map((l) => (
            <span key={l.label}>
              <i style={{ background: l.color }} />
              {l.label}
            </span>
          ))}
        </div>
      )}
      <p className="note">
        Rectangles are relative to the entity position (red cross). Measured rectangles without a facing were recorded facing right.
      </p>
    </div>
  )
}

function scalarRows(obj: J, skip: string[] = []): [string, ReactNode][] {
  const rows: [string, ReactNode][] = []
  for (const [k, v] of Object.entries<J>(obj ?? {})) {
    if (skip.includes(k) || v === null || v === undefined) continue
    if (typeof v === 'object') {
      if (Array.isArray(v) && v.every((x) => typeof x !== 'object')) rows.push([k.replace(/_/g, ' '), v.join(', ')])
      else if (!Array.isArray(v) && Object.values(v).every((x) => typeof x !== 'object'))
        rows.push([
          k.replace(/_/g, ' '),
          Object.entries(v)
            .map(([a, b]) => `${a.replace(/_/g, ' ')}: ${b}`)
            .join(' · '),
        ])
      continue
    }
    const s = String(v)
    rows.push([k.replace(/_/g, ' '), /^0x[0-9a-f]+$/i.test(s) ? <Addr key={k} a={s} /> : s])
  }
  return rows
}

export function MovesPage({ game }: { game: string }) {
  const moves = useData<J>(game, 'moves')
  const detail = useData<J>(game, game === 'buu' ? 'attacks_detail' : 'moves')
  const { list: attackers } = useAttackerSprites(game)
  const [params, setParams] = useSearchParams()
  const [who, setWho] = useState<'player' | 'enemy' | 'all'>('player')
  const all = useMemo(() => {
    if (!moves) return []
    if (game === 'log1') return log1Moves(moves)
    if (game === 'log2') return log2Moves(moves)
    return detail ? buuMoves(moves, detail) : []
  }, [moves, detail, game])
  const shown = all.filter((m) => who === 'all' || m.user === who)
  const sel = all.find((m) => m.key === params.get('m')) ?? shown[0]
  if (!moves) return <Loading what="moves" />
  return (
    <div>
      <p className="lede">
        {game === 'log1' && 'LoG1 moves are code plus small per-facing tables (spawn offsets, launch velocities, beam growth); there is no generic attack record. Damage scales with max HP or Ki spent.'}
        {game === 'log2' && 'A LoG2 attack = an 8-byte attack-table entry (type + power) sent inside a hit message with a rectangle and a scale. Moves differ in how they build that message: instant rect, projectile, beam, explosion.'}
        {game === 'buu' && 'Buu’s Fury skills point at an 8-byte attack type (kind, sub, power) and pass a per-hit multiplier. Most skills have three upgrade levels with separate costs and multipliers.'}{' '}
        <Link to={`/g/${game}/lab`}>Build your own in the Move lab →</Link>
      </p>
      {game !== 'buu' && (
        <div className="toolbar">
          <Seg
            value={who}
            onChange={setWho}
            options={[
              { value: 'player', label: `Player (${all.filter((m) => m.user === 'player').length})` },
              { value: 'enemy', label: `Enemy (${all.filter((m) => m.user === 'enemy').length})` },
              { value: 'all', label: 'All' },
            ]}
          />
        </div>
      )}
      <div className="table-wrap">
        <div className="table-scroll" style={{ maxHeight: 420 }}>
          <table>
            <thead>
              <tr>
                <th>move</th>
                <th>kind</th>
                <th className="num">power</th>
                <th>attack id</th>
                <th>{game === 'buu' ? 'mult Lv0 | Lv1 | Lv2' : 'scale'}</th>
                <th>{game === 'log1' ? 'Ki' : 'EP'}</th>
                <th>startup</th>
                <th>speed</th>
                <th>evidence</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((m) => (
                <tr key={m.key} className={'clickable' + (m === sel ? ' hl' : '')} onClick={() => setParams({ m: m.key }, { replace: true })}>
                  <td>
                    <b>{m.name}</b>
                    {m.characters && <div className="muted small" style={{ maxWidth: 260, whiteSpace: 'normal' }}>{m.characters}</div>}
                  </td>
                  <td>{m.kind}</td>
                  <td className="num">{m.power !== undefined ? fmt(m.power) : ''}</td>
                  <td className="mono small">{m.attackId ?? ''}</td>
                  <td className="mono small" style={{ whiteSpace: 'normal', maxWidth: 200 }}>{m.scale ?? ''}</td>
                  <td className="small" style={{ whiteSpace: 'normal', maxWidth: 220 }}>{m.cost ?? ''}</td>
                  <td className="small">{m.startup ?? ''}</td>
                  <td className="small">{m.speed ?? ''}</td>
                  <td>
                    <Evidence text={m.evidence} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      {sel && (
        <div className="split" style={{ marginTop: 16, gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1.2fr)' }}>
          <Card title={`Hitbox: ${sel.name}`}>
            <HitboxPanel game={game} move={sel} attackers={attackers} />
          </Card>
          <Card
            title={sel.name}
            actions={
              <Link to={`/g/${game}/lab?template=${encodeURIComponent(sel.key)}`}>
                <Tag kind="info">open in Move lab</Tag>
              </Link>
            }
          >
            {sel.description && <p>{sel.description}</p>}
            <KV rows={scalarRows(sel.raw, ['name', 'characters', 'evidence', 'verified'])} />
            {sel.extra && (
              <Collapse title={game === 'log2' ? 'Move-type table row' : 'Skill detail (attacks_detail.json)'}>
                <KV rows={scalarRows(sel.extra, ['ui_description'])} />
              </Collapse>
            )}
            {game === 'buu' && <BuuSkillLevels raw={sel.raw} />}
            <Collapse title="Full record (JSON)">
              <JsonView value={sel.extra ? { ...sel.raw, _detail: sel.extra } : sel.raw} />
            </Collapse>
          </Card>
        </div>
      )}
      {game === 'log2' && <Log2Tables moves={moves} />}
      {game === 'buu' && detail && <BuuTables moves={moves} detail={detail} />}
    </div>
  )
}

function BuuSkillLevels({ raw }: { raw: J }) {
  const lv = raw.measured?.by_level
  if (!lv) return null
  return (
    <div className="plain-table" style={{ marginTop: 10 }}>
      <table>
        <thead>
          <tr>
            <th>level</th>
            <th>EP cost</th>
            <th>multipliers</th>
            <th>first hit</th>
            <th>runs (pattern → hits / damage / EP)</th>
          </tr>
        </thead>
        <tbody>
          {Object.entries<J>(lv).map(([k, v]) => (
            <tr key={k}>
              <td>Lv{Number(k) + 1}</td>
              <td>{raw.ep?.ep_cost_by_level?.[Number(k)] ?? raw.ep?.canuse_min_ep ?? ''}</td>
              <td className="mono">{(v.damage_multipliers ?? []).join(', ')}</td>
              <td>{v.first_hit_frame !== null && v.first_hit_frame !== undefined ? `f${v.first_hit_frame}` : '—'}</td>
              <td className="wrap small">
                {(v.runs ?? [])
                  .map((r: J) => `${r.pattern}@${r.dist}px → ${r.hits} / ${fmt(r.total_damage, 1)} / ${r.ep_spent}`)
                  .join(' · ')}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function Log2Tables({ moves }: { moves: J }) {
  return (
    <>
      <h2>Attack table</h2>
      <p className="muted">
        <Addr a={moves.attack_table_rom} /> · {moves.attack_entry_format}. Effective power = power × scale / 1024.
      </p>
      <div className="plain-table">
        <table>
          <thead>
            <tr>
              <th className="num">id</th>
              <th>type</th>
              <th className="num">power</th>
              <th>used by</th>
              <th>ROM</th>
            </tr>
          </thead>
          <tbody>
            {moves.attack_table.map((a: J) => (
              <tr key={a.id}>
                <td className="num">{a.id}</td>
                <td>{a.type === 0 ? <Tag kind="bad">melee · STR</Tag> : <Tag kind="info">energy · POW</Tag>}</td>
                <td className="num mono">
                  {fmt(a.power)} <span className="muted">({a.power_hex})</span>
                </td>
                <td className="wrap small">{a.used_by}</td>
                <td>
                  <Addr a={a.rom} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <h2>Move-type table (B button)</h2>
      <p className="muted">
        <Addr a={moves.move_type_table_rom} /> · {moves.move_type_entry_format}
      </p>
      <div className="plain-table">
        <table>
          <thead>
            <tr>
              <th className="num">type</th>
              <th>name</th>
              <th className="num">min EP</th>
              <th className="num">EP on start</th>
              <th>state</th>
              <th>attack</th>
              <th>scale</th>
              <th>canUse / execute</th>
            </tr>
          </thead>
          <tbody>
            {moves.player_moves.map((m: J) => (
              <tr key={m.id}>
                <td className="num">{m.id}</td>
                <td>{m.name}</td>
                <td className="num">{m.min_ep_to_start}</td>
                <td className="num">{m.ep_cost_on_start}</td>
                <td className="mono small">{typeof m.player_state === 'number' ? '0x' + m.player_state.toString(16) : m.player_state}</td>
                <td>{m.attack_id ?? ''}</td>
                <td className="wrap small">{String(m.scale ?? '')}</td>
                <td className="mono small">
                  {m.can_use_fn} / {m.execute_fn}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <h2>Player states</h2>
      <div className="grid-auto" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', fontSize: 13 }}>
        {Object.entries<string>(moves.player_states).map(([k, v]) => (
          <div key={k}>
            <code>{k}</code> {v}
          </div>
        ))}
      </div>
    </>
  )
}

function BuuTables({ moves, detail }: { moves: J; detail: J }) {
  return (
    <>
      <h2>Attack types</h2>
      <p className="muted">
        <Addr a={moves.meta?.attack_type_table} /> · 8 bytes: u8 kind, u8 sub, u16 0, u32 power. Melee scales by STR/20 and can crit unless
        sub = 3.
      </p>
      <div className="plain-table">
        <table>
          <thead>
            <tr>
              <th className="num">sub</th>
              <th>kind</th>
              <th className="num">power</th>
              <th>name</th>
              <th>ROM</th>
            </tr>
          </thead>
          <tbody>
            {moves.attack_types.map((a: J) => (
              <tr key={a.sub}>
                <td className="num">{a.sub}</td>
                <td>{a.kind === 0 ? <Tag kind="bad">melee</Tag> : <Tag kind="info">energy</Tag>}</td>
                <td className="num">{a.power}</td>
                <td>{detail.attack_sub_names?.[String(a.sub)] ?? ''}</td>
                <td>
                  <Addr a={a.address} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <h2>Beam levels</h2>
      <p className="muted">
        Per-level beam definition at <Addr a="0x0871DA68" /> (16 bytes): u8 w, u8 h, u16 mult, u16, u16 attack sub, graphics, animation. Fires at
        frame {detail.beams?.fire_frame} while B is held; drain {detail.beams?.drain_formula}.
      </p>
      <div className="plain-table">
        <table>
          <thead>
            <tr>
              <th>level</th>
              <th>box</th>
              <th className="num">mult</th>
              <th className="num">sub</th>
              <th className="num">EP/frame</th>
              <th>gfx / anim</th>
            </tr>
          </thead>
          <tbody>
            {(detail.beams?.level_defs ?? []).map((l: J) => (
              <tr key={l.level}>
                <td>Lv{l.level + 1}</td>
                <td>
                  {l.w}×{l.h}
                </td>
                <td className="num">{l.mult}</td>
                <td className="num">{l.attack_sub}</td>
                <td className="num">{l.drain_ep_per_frame}</td>
                <td className="mono small">
                  {l.gfx} / {l.anim}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="grid2" style={{ marginTop: 16 }}>
        <Card title="Block (R)">
          <KV rows={scalarRows(detail.block)} />
          <h4>Energy shield (R+B)</h4>
          <KV rows={scalarRows(detail.energy_block)} />
        </Card>
        <Card title="Big Bang ball">
          <KV rows={scalarRows(detail.big_bang)} />
          <h4>Ki blast</h4>
          <KV rows={scalarRows(detail.ki_blast)} />
        </Card>
      </div>
    </>
  )
}
