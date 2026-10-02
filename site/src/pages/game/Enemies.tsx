import { useMemo, useState } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router-dom'
import { useData } from '../../data'
import { useSprites } from '../../lib/sprites'
import { enemiesOf, type EnemyVM } from '../../model/enemies'
import { Card, Loading, Search, Seg, SpriteThumb, Tag } from '../../components/ui'
import { fmt, type J } from '../../lib/util'

type SortKey = 'id' | 'name' | 'hp' | 'exp' | 'spawns' | string

export function EnemiesPage({ game }: { game: string }) {
  const json = useData<J>(game, 'enemies')
  const bosses = useData<J>(game, 'bosses')
  const sp = useSprites(game)
  const nav = useNavigate()
  const [params, setParams] = useSearchParams()
  const filter = params.get('f') ?? 'used'
  const [q, setQ] = useState('')
  const [sort, setSort] = useState<{ k: SortKey; dir: 1 | -1 }>({ k: 'id', dir: 1 })
  const all = useMemo(() => {
    const list = json ? enemiesOf(game, json, sp) : []
    // Buu: every type analysed in bosses.json counts as a boss / story fight
    if (game === 'buu' && bosses?.bosses) for (const e of list) if (bosses.bosses['0x' + Number(e.key).toString(16)]) e.boss = true
    return list
  }, [json, sp, game, bosses])
  const shown = useMemo(() => {
    const ql = q.toLowerCase()
    let r = all.filter((e) => {
      if (filter === 'used' && !e.used) return false
      if (filter === 'boss' && !e.boss) return false
      if (ql && !`${e.name} ${e.idLabel} ${e.sub ?? ''}`.toLowerCase().includes(ql)) return false
      return true
    })
    const val = (e: EnemyVM): number | string => {
      if (sort.k === 'id') return all.indexOf(e)
      if (sort.k === 'name') return e.name
      if (sort.k === 'hp') return e.hp
      if (sort.k === 'exp') return e.exp
      if (sort.k === 'spawns') return e.spawns
      return e.stats.find((s) => s.label === sort.k)?.value ?? 0
    }
    r = [...r].sort((a, b) => {
      const x = val(a),
        y = val(b)
      return (typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y))) * sort.dir
    })
    return r
  }, [all, filter, q, sort])
  if (!json) return <Loading what="enemies" />
  const statLabels = all[0]?.stats.map((s) => s.label) ?? []
  const th = (k: SortKey, label: string, num = true) => (
    <th className={num ? 'num' : ''} onClick={() => setSort(sort.k === k ? { k, dir: sort.dir === 1 ? -1 : 1 } : { k, dir: num ? -1 : 1 })}>
      {label}
      {sort.k === k ? (sort.dir === 1 ? ' ▲' : ' ▼') : ''}
    </th>
  )
  const fights: J[] = game === 'log2' ? (bosses?.fights ?? []) : []
  return (
    <div>
      <p className="lede">
        {game === 'log1' && 'Enemy templates at 0x086B6094 (0x40 bytes). All enemies share one generic AI driven by three numbers: move type, target type and fight style. Bosses are NPCs turned into enemies by scripts.'}
        {game === 'log2' && 'Enemy class table 0x086D56AC (109 × 24 bytes): HP, exp, STR/POW/END, damage-taken multipliers and the initial AI state. Behaviour is a stack of behaviour-tree states.'}
        {game === 'buu' && 'Enemy table 0x08711274 (150 types × 28 bytes). Same behaviour-tree AI engine as LoG II, much larger: 732 reachable states, 178 node types.'}
      </p>
      {fights.length > 0 && (
        <Card title="Boss fights" className="mb">
          <div className="grid-auto">
            {fights.map((f) => {
              const first = all.find((e) => e.key === String(f.classes[0]))
              return (
                <Link key={f.key} to={`/g/${game}/enemies/${f.classes[0]}?fight=${f.key}`} className="row" style={{ alignItems: 'center' }}>
                  <SpriteThumb entity={first?.sprites[0]} size={48} />
                  <div>
                    <b>{f.name}</b>
                    <div className="muted small">
                      {f.phases.length} phase{f.phases.length > 1 ? 's' : ''} · classes {f.classes.join(', ')}
                    </div>
                  </div>
                </Link>
              )
            })}
          </div>
        </Card>
      )}
      {game === 'buu' && bosses?.bosses && Object.keys(bosses.bosses).length > 0 && <BuuBossGroups bosses={bosses} all={all} game={game} />}
      <div className="toolbar" style={{ marginTop: 16 }}>
        <Search value={q} onChange={setQ} placeholder="Search enemies…" />
        <Seg
          value={filter}
          onChange={(v) => setParams(v === 'used' ? {} : { f: v }, { replace: true })}
          options={[
            { value: 'used', label: `In game (${all.filter((e) => e.used).length})` },
            { value: 'boss', label: `Bosses (${all.filter((e) => e.boss).length})` },
            { value: 'all', label: `All rows (${all.length})` },
          ]}
        />
      </div>
      <div className="table-wrap">
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th></th>
                {th('name', 'Name', false)}
                {th('id', 'id')}
                {th('hp', 'HP')}
                {th('exp', 'EXP')}
                {statLabels.map((l) => (
                  <th key={l} className="num" onClick={() => setSort(sort.k === l ? { k: l, dir: sort.dir === 1 ? -1 : 1 } : { k: l, dir: -1 })}>
                    {l}
                    {sort.k === l ? (sort.dir === 1 ? ' ▲' : ' ▼') : ''}
                  </th>
                ))}
                {th('spawns', 'Spawns')}
                <th>Notes</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((e) => (
                <tr key={e.key} className={'clickable' + (e.used ? '' : ' dim')} onClick={() => nav(`/g/${game}/enemies/${e.key}`)}>
                  <td>
                    <SpriteThumb entity={e.sprites[0]} size={40} />
                  </td>
                  <td className="wrap">
                    <Link to={`/g/${game}/enemies/${e.key}`} onClick={(ev) => ev.stopPropagation()}>
                      <b>{e.name}</b>
                    </Link>
                    {e.sub && <div className="muted small">{e.sub}</div>}
                  </td>
                  <td className="num mono small">{e.idLabel}</td>
                  <td className="num">{fmt(e.hp)}</td>
                  <td className="num">{fmt(e.exp)}</td>
                  {e.stats.map((s) => (
                    <td key={s.label} className="num">
                      {fmt(s.value)}
                    </td>
                  ))}
                  <td className="num">{e.spawns || ''}</td>
                  <td>
                    {e.boss && <Tag kind="bad">boss</Tag>} {e.npcRow && <Tag kind="muted">scouter/NPC row</Tag>}{' '}
                    {!e.used && !e.npcRow && <Tag kind="muted">unused</Tag>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  )
}

function BuuBossGroups({ bosses, all, game }: { bosses: J; all: EnemyVM[]; game: string }) {
  const groups = new Map<string, J[]>()
  for (const b of Object.values<J>(bosses.bosses)) {
    if (!groups.has(b.group)) groups.set(b.group, [])
    groups.get(b.group)!.push(b)
  }
  const title: Record<string, string> = { buu: 'Majin Buu forms', janemba: 'Janemba', fighters: 'Fighters & rivals', others: 'Other bosses', regular: 'Regular' }
  return (
    <Card title="Bosses & story fights (analysed behaviour)" className="mb">
      {[...groups].map(([g, list]) => (
        <div key={g} style={{ marginBottom: 10 }}>
          <h4 style={{ margin: '6px 0' }}>
            {title[g] ?? g} <span className="muted small">{list.length}</span>
          </h4>
          <div className="chips">
            {list.map((b) => {
              const vm = all.find((e) => e.key === String(b.type))
              return (
                <Link key={b.type} to={`/g/${game}/enemies/${b.type}`} className="chip" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                  <SpriteThumb entity={vm?.sprites[0]} size={24} />
                  {b.name}
                </Link>
              )
            })}
          </div>
        </div>
      ))}
    </Card>
  )
}
