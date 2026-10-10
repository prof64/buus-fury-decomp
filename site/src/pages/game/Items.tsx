import { useMemo, useState } from 'react'
import { useData } from '../../data'
import { useSprites, type SpriteEntity, type SpriteIndex } from '../../lib/sprites'
import { Addr, Card, Loading, Search, Seg, SpriteThumb } from '../../components/ui'
import { BarList } from '../../components/Chart'
import { coinDist, lootRolls } from '../../formulas/buu'
import { fmt, pct, type J } from '../../lib/util'

function effectText(eff: J): string {
  if (!eff) return ''
  if (typeof eff === 'string') return eff
  if (Array.isArray(eff)) return eff.map(effectText).join('; ')
  if (eff.text) return eff.text
  return Object.entries(eff)
    .filter(([k]) => k !== 'note')
    .map(([k, v]) => `${k.replace(/_/g, ' ')}: ${typeof v === 'object' ? JSON.stringify(v) : v}`)
    .join(', ')
}

function iconFor(sp: SpriteIndex | undefined, prefix: string, id: number): SpriteEntity | undefined {
  if (!sp) return undefined
  return sp.all.find((e) => e.category === 'item' && e.item_id === id && e.id.startsWith(prefix))
}

export function ItemsPage({ game }: { game: string }) {
  const json = useData<J>(game, 'items')
  const sp = useSprites(game)
  const [q, setQ] = useState('')
  const [tab, setTab] = useState<'items' | 'equipment' | 'drops'>('items')
  const ql = q.toLowerCase()
  const match = (s: string) => !ql || s.toLowerCase().includes(ql)
  const rows = useMemo(() => {
    if (!json) return []
    if (game === 'buu') return tab === 'equipment' ? json.equipment : json.supplies
    return json.items
  }, [json, game, tab])
  if (!json) return <Loading what="items" />
  return (
    <div>
      <p className="lede">
        {game === 'log1' && 'LoG1 items are fixed map pickups and script rewards; enemies never drop anything. The ROM stores no item names, only pickup messages.'}
        {game === 'log2' && 'Item table 0x086ADE24 (50 × 56 bytes). Health/Energy bonus orbs are used on touch; Caps raise stats permanently. Enemy drops are per-spawn bytecode scripts.'}
        {game === 'buu' && 'Supplies (0x083BD2F8) are used from the menu through bytecode scripts; equipment (0x086F82EC) adds stats and runs an effect script during stat recalculation.'}
      </p>
      <div className="toolbar">
        <Seg
          value={tab}
          onChange={setTab}
          options={
            game === 'buu'
              ? [
                  { value: 'items', label: `Supplies (${json.supplies.length})` },
                  { value: 'equipment', label: `Equipment (${json.equipment.length})` },
                  { value: 'drops', label: 'Loot rules' },
                ]
              : game === 'log2'
                ? [
                    { value: 'items', label: `Items (${json.items.length})` },
                    { value: 'drops', label: `Drop tables (${json.drop_tables.length})` },
                  ]
                : [{ value: 'items', label: `Items (${json.items.length})` }]
          }
        />
        {tab !== 'drops' && <Search value={q} onChange={setQ} placeholder="Filter items…" />}
      </div>

      {tab !== 'drops' && (
        <div className="table-wrap">
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th></th>
                  <th className="num">id</th>
                  <th>name</th>
                  {game === 'buu' && tab === 'equipment' ? (
                    <>
                      <th>slot</th>
                      <th className="num">lvl</th>
                      <th className="num">STR</th>
                      <th className="num">POW</th>
                      <th className="num">END</th>
                      <th className="num">END2</th>
                      <th className="num">SPD</th>
                      <th className="num">XP+</th>
                    </>
                  ) : null}
                  {game === 'buu' && <th className="num">price</th>}
                  {game === 'log2' && <th className="num">max</th>}
                  <th>effect</th>
                  <th>ROM</th>
                </tr>
              </thead>
              <tbody>
                {rows
                  .filter((it: J) => match(`${it.name ?? it.kind} ${it.description ?? ''} ${it.pickup_message ?? ''}`))
                  .map((it: J) => {
                    const icon =
                      game === 'buu' ? iconFor(sp, tab === 'equipment' ? 'equipment_' : 'supply_', it.id) : iconFor(sp, 'item_', it.id)
                    return (
                      <tr key={it.id}>
                        <td>
                          <SpriteThumb entity={icon} size={32} />
                        </td>
                        <td className="num">{it.id}</td>
                        <td className="wrap">
                          <b>{it.name ?? it.kind}</b>
                          <div className="muted small">{it.description ?? it.pickup_message}</div>
                        </td>
                        {game === 'buu' && tab === 'equipment' && (
                          <>
                            <td>{it.slot}</td>
                            <td className="num">{it.required_level}</td>
                            {['STR', 'POW', 'END', 'END_energy', 'SPD'].map((k) => (
                              <td key={k} className="num">
                                {it[k] ? (it[k] > 0 ? '+' : '') + it[k] : ''}
                              </td>
                            ))}
                            <td className="num">{it.exp_bonus_256ths ? '+' + pct(it.exp_bonus_256ths / 256, 0) : ''}</td>
                          </>
                        )}
                        {game === 'buu' && <td className="num">{it.price >= 0 ? fmt(it.price) : ''}</td>}
                        {game === 'log2' && <td className="num">{it.max_count}</td>}
                        <td className="wrap small">
                          {game === 'log1'
                            ? it.on_use_desc
                            : game === 'log2'
                              ? effectText(it.effect)
                              : [effectText(it.effect), ...(it.script_effects ?? []).map(effectText), effectText(it.other_effects)].filter(Boolean).join('; ')}
                        </td>
                        <td>
                          <Addr a={it.rom_addr ?? it.address} />
                        </td>
                      </tr>
                    )
                  })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {tab === 'drops' && game === 'log2' && (
        <div className="grid-auto" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(340px, 1fr))' }}>
          {json.drop_tables.map((t: J) => (
            <Card key={t.id} title={`Table ${t.id}`} actions={<span className="muted small">{t.uses} spawns · {t.example_script}</span>}>
              {t.rows?.length ? (
                <>
                  <div className="note">{t.roll}; first matching row wins.</div>
                  <BarList
                    data={t.rows.map((r: J, i: number) => ({
                      label: `${r.item_name}${r.condition ? ' (' + r.condition + ')' : ''}`,
                      value: (r.roll_lt - (i ? t.rows[i - 1].roll_lt : 0)) / 100,
                      color: /Health|Fish/.test(r.item_name) ? 'var(--s8)' : 'var(--s1)',
                    }))}
                    format={(v) => pct(v, 0)}
                    max={0.4}
                  />
                </>
              ) : (
                <div className="muted small">{t.note ?? 'no drop rows (event script)'}</div>
              )}
            </Card>
          ))}
        </div>
      )}

      {tab === 'drops' && game === 'buu' && <BuuLoot />}
    </div>
  )
}

function BuuLoot() {
  const [luck, setLuck] = useState(0)
  const [level, setLevel] = useState(100)
  const rolls = lootRolls(luck)
  const coins = coinDist(level)
  return (
    <div className="grid2">
      <Card title="Number of loot rolls per kill (script #497)">
        <div className="toolbar">
          <label className="field">
            <span>Luck modifier (equipment)</span>
            <select value={luck} onChange={(e) => setLuck(+e.target.value)}>
              <option value={-3}>−3 Hare’s Foot</option>
              <option value={-2}>−2 Doom Amulet</option>
              <option value={-1}>−1 Skull Ring</option>
              <option value={0}>none</option>
              <option value={2}>+2 Lucky Charm / Rabbit’s Foot</option>
              <option value={3}>+3 Four-Leaf Clover</option>
              <option value={5}>+5 Clover + Charm</option>
            </select>
          </label>
        </div>
        <BarList data={rolls.map((r) => ({ label: r.rolls === 0 ? 'nothing (1%)' : `${r.rolls} roll${r.rolls > 1 ? 's' : ''}`, value: r.p }))} format={(v) => pct(v)} />
        <p className="note">
          Each roll: 85% coin, else 25% equipment (once per kill), else a supply. Because the sub-scripts share globals G1/G2,
          after the first coin practically every later roll is also a coin — items come almost only from the first roll.
        </p>
      </Card>
      <Card title="Coin size (script #500)">
        <label className="slider">
          <span>Loot level</span>
          <input type="range" min={50} max={200} value={level} onChange={(e) => setLevel(+e.target.value)} />
          <b className="mono">{level}</b>
        </label>
        <BarList data={coins.map((c) => ({ label: `${c.coin} Zenie`, value: c.p, color: 'var(--s4)' }))} format={(v) => pct(v)} />
        <div className="formula">{`G = (level − 48) × 100;  r = rand(400)
100Z if r ≤ G/100, 50Z if r ≤ G/83, 25Z if r ≤ G/66, else 5Z`}</div>
      </Card>
    </div>
  )
}
