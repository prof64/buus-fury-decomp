import { useMemo, useState, type ReactNode } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { assetUrl, useData } from '../../data'
import { previewFrame, useSprites, type SpriteEntity, type SpriteIndex } from '../../lib/sprites'
import { SpritePlayer } from '../../components/SpritePlayer'
import { LineChart } from '../../components/Chart'
import { Addr, Card, Collapse, Empty, KV, Loading, Seg, Slider, SpriteThumb, Stat, Tag } from '../../components/ui'
import { fmt, hexNum, pct, range, type J } from '../../lib/util'
import * as L1 from '../../formulas/log1'
import * as L2 from '../../formulas/log2'
import * as BU from '../../formulas/buu'

interface CharTab {
  id: string
  name: string
  portrait?: SpriteEntity
}

function CharTabs({ game, tabs, cur }: { game: string; tabs: CharTab[]; cur: string }) {
  const nav = useNavigate()
  return (
    <div className="char-tabs">
      {tabs.map((t) => {
        const f = t.portrait && previewFrame(t.portrait)
        return (
          <button key={t.id} className={'char-tab' + (t.id === cur ? ' active' : '')} onClick={() => nav(`/g/${game}/characters/${t.id}`)}>
            {f ? <img src={assetUrl(f.png)} alt="" /> : <span style={{ width: 32 }} />}
            {t.name}
          </button>
        )
      })}
    </div>
  )
}

function Portrait({ e, size = 128 }: { e?: SpriteEntity; size?: number }) {
  const f = e && previewFrame(e)
  if (!f) return null
  return <img className="portrait" src={assetUrl(f.png)} alt={e!.name} width={size} height={size} title={e!.name} />
}

export function CharactersPage({ game }: { game: string }) {
  if (game === 'log1') return <Log1Chars />
  if (game === 'log2') return <Log2Chars />
  return <BuuChars />
}

/* ======================= LoG1 ======================= */

function Log1Chars() {
  const chars = useData<J>('log1', 'characters')
  const levels = useData<J>('log1', 'levels')
  const moves = useData<J>('log1', 'moves')
  const sp = useSprites('log1')
  const [lv, setLv] = useState(10)
  const [log, setLog] = useState(true)
  const [variant, setVariant] = useState('goku')
  if (!chars || !levels || !sp) return <Loading what="Goku" />
  const g = chars.characters[0]
  const rows: L1.Log1Level[] = levels.levels
  const row = rows[lv - 1]
  const sprite = sp.byDef.get(hexNum(g.sprites[variant]?.def_addr) ?? -1)
  const portrait = sp.findByName('Goku', ['portrait'])
  return (
    <div>
      <CharTabs game="log1" tabs={[{ id: '0', name: 'Goku', portrait }]} cur="0" />
      <p className="lede">
        Goku is the only playable character. He has no attack stat: punches and ki blasts scale with his <b>max HP</b>, the Kamehameha
        with the Ki spent charging it. Defense is a constant 250 at every level. Stats come straight from the level table at{' '}
        <Addr a="0x080114A8" />.
      </p>
      <div className="split">
        <Card
          title="Sprite"
          actions={<Seg value={variant} onChange={setVariant} options={Object.keys(g.sprites).map((k) => ({ value: k, label: k }))} />}
        >
          <div className="row" style={{ alignItems: 'flex-start' }}>
            <Portrait e={portrait} size={96} />
          </div>
          {sprite && <SpritePlayer key={sprite.id} entity={sprite} compact zoom={4} />}
          <KV
            rows={[
              ['Walk', '1 px/frame, no diagonals (UP > DOWN > LEFT > RIGHT priority)'],
              ['Flight (R)', '2 px/frame, 1 energy per 8 frames; energy never regenerates'],
              ['Hurtbox', 'x 2..14, y −22..6 (all facings)'],
              ['Invincibility code', g.cheats?.invincibility?.input],
            ]}
          />
        </Card>
        <div className="stack">
          <Card title={`Level ${lv}`} actions={<Slider label="Level" min={1} max={25} value={lv} onChange={setLv} />}>
            <div className="stats">
              <Stat label="EXP needed" value={fmt(row.exp_required)} sub={row.exp_to_next ? `next +${fmt(row.exp_to_next)}` : 'max level'} />
              <Stat label="Max HP" value={fmt(row.max_hp)} />
              <Stat label="Max Ki" value={fmt(row.max_ki)} sub={`+${row.ki_regen_per_16_frames} per 16 f`} />
              <Stat label="Flight energy" value={row.max_flight_energy} sub={`${row.max_flight_energy * 8} frames of flight`} />
              <Stat label="Punch" value={`${fmt(row.melee_damage_min)}–${fmt(row.melee_damage_max)}`} sub="75–99.8% of max HP" />
              <Stat label="Ki blast" value={fmt(row.ki_blast_damage)} sub="max HP / 3" />
              <Stat label="Charge cost" value={L1.chargeCost(row.max_ki)} sub="Ki per 8 frames held" />
              <Stat label="Max hit taken" value={fmt(row.max_damage_taken_per_hit)} sub="max HP / 3 + 1" />
            </div>
          </Card>
          <Card title="Growth" actions={<Seg value={log ? 'log' : 'lin'} onChange={(v) => setLog(v === 'log')} options={[{ value: 'log', label: 'log' }, { value: 'lin', label: 'linear' }]} />}>
            <LineChart
              logY={log}
              marker={lv}
              xLabel="level"
              series={[
                { name: 'max HP', points: rows.map((r) => [r.level, r.max_hp]) },
                { name: 'max Ki', points: rows.map((r) => [r.level, r.max_ki]) },
                { name: 'EXP required', points: rows.map((r) => [r.level, Math.max(1, r.exp_required)]), dashed: true },
              ]}
            />
          </Card>
        </div>
      </div>
      <Card title="Moves" className="mt">
        <div className="plain-table">
          <table>
            <thead>
              <tr>
                <th>move</th>
                <th>input</th>
                <th>damage</th>
                <th>Ki</th>
                <th>learned</th>
              </tr>
            </thead>
            <tbody>
              {moves?.moves
                .filter((m: J) => m.user === 'player')
                .map((m: J) => (
                  <tr key={String(m.id)}>
                    <td>
                      <b>{m.name}</b>
                    </td>
                    <td className="wrap small">{m.input}</td>
                    <td className="wrap small">{String(m.damage ?? '—')}</td>
                    <td className="wrap small">{String(m.ki_cost ?? m.energy_cost ?? '')}</td>
                    <td className="wrap small">{m.learned ?? (m.id === 0 || m.id === 'melee' || m.id === 'fly' ? 'from the start' : '')}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        <p className="small">
          <Link to="/g/log1/moves">All move data →</Link> · <Link to="/g/log1/formulas">Formulas & calculators →</Link>
        </p>
      </Card>
      <Collapse title="Full level table">
        <div className="plain-table">
          <table>
            <thead>
              <tr>
                {['Lv', 'EXP', 'max HP', 'max Ki', 'flight', 'Ki regen/16f', 'ki blast', 'punch', 'max hit taken'].map((h) => (
                  <th key={h} className="num">
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.level} className={r.level === lv ? 'hl' : ''}>
                  <td className="num">{r.level}</td>
                  <td className="num">{fmt(r.exp_required)}</td>
                  <td className="num">{fmt(r.max_hp)}</td>
                  <td className="num">{fmt(r.max_ki)}</td>
                  <td className="num">{r.max_flight_energy}</td>
                  <td className="num">{r.ki_regen_per_16_frames}</td>
                  <td className="num">{fmt(r.ki_blast_damage)}</td>
                  <td className="num">
                    {fmt(r.melee_damage_min)}–{fmt(r.melee_damage_max)}
                  </td>
                  <td className="num">{fmt(r.max_damage_taken_per_hit)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Collapse>
    </div>
  )
}

/* ======================= LoG2 ======================= */

function log2Portrait(sp: SpriteIndex, form: J): SpriteEntity | undefined {
  const pid = form?.aux_gfx_ids?.[1]
  return pid !== undefined ? sp.portrait(pid - 8) : undefined
}

function Log2Chars() {
  const { id = '0' } = useParams()
  const chars = useData<J>('log2', 'characters')
  const levels = useData<J>('log2', 'levels')
  const sp = useSprites('log2')
  const [lv, setLv] = useState(20)
  const [formId, setFormId] = useState<number | null>(null)
  if (!chars || !levels || !sp) return <Loading what="characters" />
  const ci = Math.min(Number(id) || 0, chars.characters.length - 1)
  const c = chars.characters[ci]
  const forms: J[] = c.forms.map((f: number) => chars.forms[f])
  const startForm = chars.forms[c.initial.form]
  const curForm = formId !== null && c.forms.includes(formId) ? chars.forms[formId] : startForm
  const ctl = chars.form_controllers[curForm.controller.ram]
  const sprite = sp.byGfx(curForm.actor_id)
  const tabs = chars.characters.map((x: J) => ({ id: String(x.index), name: x.name, portrait: log2Portrait(sp, chars.forms[x.initial.form]) }))
  const g = c.growth_per_level
  const gr = (k: 'STR' | 'POW' | 'END'): [number, number] => [g[k].min_8p8, g[k].max_8p8]
  const st = (k: 'STR' | 'POW' | 'END', L: number) => L2.statAt(c.initial[`${k}_8p8`], gr(k), L)
  const expTable: number[] = levels.levels.map((l: J) => l.total_exp)
  const hp = L2.maxHpAt(c.initial.max_hp, lv)
  const ep = L2.maxEpAt(c.initial.max_ep, lv)
  const eff = (k: 'STR' | 'POW' | 'END', i: number) => (ctl.stat_override ? ctl.stat_override[i] : Math.floor(st(k, lv).avg) + (ctl.bonus?.[i] ?? 0))
  const levelsArr = range(50, 1)
  const unlocks: J[] = (chars.move_unlocks?.calls ?? []).filter((u: J) => u.char === c.name)
  const punch = L2.damageDist({ type: 0, power: 1024, scale: 1024, S: eff('STR', 0), DEF: 0, MULT: 128 })
  return (
    <div>
      <CharTabs game="log2" tabs={tabs} cur={String(ci)} />
      <div className="split">
        <Card title={c.name} actions={<span className="muted small">record <Addr a={c.init_record_rom} /></span>}>
          <div className="row" style={{ alignItems: 'flex-start', marginBottom: 10 }}>
            <Portrait e={log2Portrait(sp, curForm)} size={96} />
            <KV
              rows={[
                ['Start', `HP ${c.initial.max_hp} · EP ${c.initial.max_ep} · STR ${c.initial.STR} · POW ${c.initial.POW} · END ${c.initial.END}`],
                ['Start exp', c.initial.exp],
                ['Form', `${curForm.id}: ${curForm.label}`],
                ['Controller', `${ctl.name} — walk ${ctl.walk_speed} / dash ${ctl.run_speed} px/f`],
                ['Form tick', ctl.tick],
              ]}
            />
          </div>
          {sprite ? <SpritePlayer key={sprite.id} entity={sprite} compact zoom={4} /> : <Empty>No sprite for actor {curForm.actor_id}</Empty>}
        </Card>
        <div className="stack">
          <Card title={`Level ${lv} (average growth)`} actions={<Slider label="Level" min={1} max={50} value={lv} onChange={setLv} />}>
            <div className="stats">
              {(['STR', 'POW', 'END'] as const).map((k, i) => {
                const s = st(k, lv)
                return (
                  <Stat
                    key={k}
                    label={k}
                    value={fmt(s.avg, 1)}
                    sub={`${fmt(s.min, 1)}–${fmt(s.max, 1)}${ctl.stat_override ? ` · form sets ${ctl.stat_override[i]}` : ctl.bonus?.[i] ? ` · +${ctl.bonus[i]} in form` : ''}`}
                  />
                )
              })}
              <Stat label="Max HP" value={fmt(hp.avg)} sub={`${fmt(hp.min)}–${fmt(hp.max)}`} />
              <Stat label="Max EP" value={fmt(ep.avg)} sub={`${ep.min}–${ep.max}`} />
              <Stat label="EXP for level" value={fmt(expTable[lv - 1])} sub={lv < 50 ? `span ${fmt(expTable[lv] - expTable[lv - 1])}` : 'max'} />
              <Stat label="Kill exp range" value={lv < 50 ? `${fmt((expTable[lv] - expTable[lv - 1]) >> 7)}–${fmt((expTable[lv] - expTable[lv - 1]) >> 1)}` : '0'} sub="for enemies with exp > 5" />
              <Stat label="Punch vs END 0" value={`${punch.normal[0]}–${punch.normal[1]}`} sub={`crit ${pct(punch.pCrit)}, in this form`} />
            </div>
            <p className="note">
              Each level adds min..max/256 to every stat (one rand roll per stat), +5.5–7.5% max HP (compounding) and +2..4 max EP,
              then heals fully. Stats cap at 100. Effective stat = integer part {ctl.stat_override ? '(overridden by the form)' : '+ form bonus'}.
            </p>
          </Card>
          <Card title="Growth curves">
            <LineChart
              marker={lv}
              xLabel="level"
              series={(['STR', 'POW', 'END'] as const).map((k) => ({
                name: k,
                points: levelsArr.map((L) => [L, +st(k, L).avg.toFixed(2)] as [number, number]),
                band: levelsArr.map((L) => [L, st(k, L).min, st(k, L).max] as [number, number, number]),
              }))}
            />
            <LineChart
              marker={lv}
              xLabel="level"
              height={180}
              series={[
                { name: 'max HP', color: 'var(--s8)', points: levelsArr.map((L) => [L, L2.maxHpAt(c.initial.max_hp, L).avg]), band: levelsArr.map((L) => { const r = L2.maxHpAt(c.initial.max_hp, L); return [L, r.min, r.max] }) },
                { name: 'max EP', color: 'var(--s1)', points: levelsArr.map((L) => [L, L2.maxEpAt(c.initial.max_ep, L).avg]) },
              ]}
            />
          </Card>
        </div>
      </div>

      <Card title="Forms & transformations" className="mt">
        <div className="form-grid">
          {forms.map((f) => {
            const fc = chars.form_controllers[f.controller.ram]
            const s = sp.byGfx(f.actor_id)
            return (
              <button key={f.id} className="form-card" style={f.id === curForm.id ? { borderColor: 'var(--game)', boxShadow: '0 0 0 1px var(--game)' } : undefined} onClick={() => setFormId(f.id)}>
                <SpriteThumb entity={s} size={64} />
                <b>{f.label}</b>
                <span className="muted">form {f.id} · {fc.name.split(' (')[0]}</span>
                <span className="bonus">
                  {fc.stat_override ? `stats = ${fc.stat_override.join('/')}` : fc.bonus?.some((b: number) => b) ? `+${fc.bonus.join('/+')} STR/POW/END` : 'no bonus'}
                </span>
                <span className="muted">
                  {f.transform_to_form !== f.id ? `transforms → ${f.transform_to_form}` : ''} {f.revert_to_form !== f.id ? `reverts → ${f.revert_to_form}` : ''}
                </span>
                <span className="muted small">
                  special melee: {f.special_melee.hit_on_anim_frames.length} hits, lunge {f.special_melee.lunge_speed_px_per_frame} px/f
                </span>
              </button>
            )
          })}
        </div>
      </Card>

      <Card title="Moves" className="mt">
        <div className="plain-table">
          <table>
            <thead>
              <tr>
                <th>slot</th>
                <th>move</th>
                <th>description</th>
                <th>unlock</th>
              </tr>
            </thead>
            <tbody>
              {c.special_melee ? (
                <tr>
                  <td>A (hold)</td>
                  <td>
                    <b>{c.special_melee.name}</b>
                  </td>
                  <td className="wrap small">{c.special_melee.description}</td>
                  <td className="small">flag bit {c.special_melee.unlock_flag_bit}</td>
                </tr>
              ) : (
                <tr>
                  <td>A (hold)</td>
                  <td colSpan={3} className="muted small">
                    no special melee
                  </td>
                </tr>
              )}
              {c.move_slots.map((m: J) => (
                <tr key={m.slot}>
                  <td>B slot {m.slot}</td>
                  <td>
                    <b>{m.name ?? `type ${m.move_type}`}</b> <span className="muted small">type {m.move_type}</span>
                  </td>
                  <td className="wrap small">{m.description}</td>
                  <td className="small">{m.unlocked_at_start ? <Tag kind="ok">from start</Tag> : `flag bit ${m.unlock_flag_bit}`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {unlocks.length > 0 && (
          <>
            <h4>Where the story unlocks them</h4>
            <ul className="small">
              {unlocks.map((u, i) => (
                <li key={i}>
                  <b>{u.move ?? u.unlocks}</b> — script <Addr a={u.addr} /> {u.fn}({u.args?.join(', ')})
                  {u.text_before && <span className="muted"> · near “{u.text_before.replace(/^[@^#]+/, '')}”</span>}
                </li>
              ))}
            </ul>
          </>
        )}
        <p className="small">
          <Link to="/g/log2/moves">All move data, hitboxes and frame data →</Link>
        </p>
      </Card>
    </div>
  )
}

/* ======================= Buu ======================= */

function BuuChars() {
  const { id = '0' } = useParams()
  const chars = useData<J>('buu', 'characters')
  const sp = useSprites('buu')
  const [lvRaw, setLv] = useState<number | null>(null)
  const [formId, setFormId] = useState<number | null>(null)
  const [spd, setSpd] = useState<number | null>(null)
  const ci = Math.min(Number(id) || 0, (chars?.characters.length ?? 1) - 1)
  const c = chars?.characters[ci]
  const curve = useMemo(() => (c ? BU.growthCurve(c.new_game.level, c.new_game.hp, c.new_game.ep, c.growth.raw_u32) : []), [c])
  if (!chars || !sp || !c) return <Loading what="characters" />
  const ng = c.new_game
  const lv = Math.max(ng.level, lvRaw ?? ng.level + 20)
  const row = curve.find((r) => r.level === lv) ?? curve[0]
  const forms: J[] = c.forms.map((f: number) => chars.forms[f])
  const curForm = formId !== null && c.forms.includes(formId) ? chars.forms[formId] : chars.forms[ng.form]
  const short = (n: string) => n.split(' (')[0]
  // a few form graphics ids point at sprite definitions that did not decode; fall back to another form of the same character
  const sprite =
    sp.byGfx(curForm.gfx_id) ??
    forms.map((f) => sp.byGfx(f.gfx_id)).find(Boolean) ??
    sp.findByName(short(c.name), ['player'])
  const portraitOf = (name: string) => sp.findByName(short(name), ['portrait'])
  const tabs = chars.characters.map((x: J) => ({ id: String(x.index), name: short(x.name), portrait: portraitOf(x.name) }))
  const SPD = spd ?? ng.SPD
  const mb = curForm.measured_bonus
  const fusion = c.growth.raw_u32[0] === 0 && c.index >= 5
  return (
    <div>
      <CharTabs game="buu" tabs={tabs} cur={String(ci)} />
      <div className="split">
        <Card title={c.name} actions={<span className="muted small">record RAM {c.record_ram}</span>}>
          <div className="row" style={{ alignItems: 'flex-start', marginBottom: 10 }}>
            <Portrait e={portraitOf(c.name)} size={96} />
            <KV
              rows={[
                ['New game', `Lv ${ng.level} · HP ${ng.hp} · EP ${ng.ep}`],
                ['Base stats', `STR ${ng.STR} · POW ${ng.POW} · END ${ng.END} · SPD ${ng.SPD}`],
                ['Skills', ng.skill_names.join(', ')],
                ['Form', `${curForm.index}: ${curForm.name}${mb.STR ? ` (+${mb.STR} stats, +${mb.walk_speed} speed)` : ''}`],
              ]}
            />
          </div>
          {sprite ? <SpritePlayer key={sprite.id} entity={sprite} compact zoom={3} /> : <Empty>No sprite for gfx {curForm.gfx_id}</Empty>}
        </Card>
        <div className="stack">
          <Card title={`Level ${lv}`} actions={<Slider label="Level" min={ng.level} max={200} value={lv} onChange={setLv} />}>
            {fusion ? (
              <p className="note">Fusion characters have no growth row of their own: their stats are derived from the two components (see Formulas → Fusion).</p>
            ) : (
              <div className="stats">
                <Stat label="Max HP" value={fmt(row.hp[1])} sub={`${fmt(row.hp[0])}–${fmt(row.hp[2])}`} />
                <Stat label="Max EP" value={fmt(row.ep[1])} sub={`${fmt(row.ep[0])}–${fmt(row.ep[2])}`} />
                <Stat label="Stat points" value={3 * (lv - ng.level)} sub="3 per level, spent 1:1" />
                <Stat label="Walk / dash" value={`${fmt(BU.walkSpeed(SPD) / 256, 2)} / ${fmt(BU.dashSpeed(SPD) / 256, 2)}`} sub={`px/f at SPD ${SPD}`} />
              </div>
            )}
            <Slider label="SPD" min={0} max={255} value={SPD} onChange={setSpd} />
            <p className="note">
              Stats do not grow on their own — every level gives 3 points to spend on STR/POW/END/SPD. HP and EP grow by a fixed amount plus a
              percentage, each with one rand(256) roll per level; range shown is all-0 vs all-255 rolls.
            </p>
          </Card>
          {!fusion && (
            <Card title="HP / EP growth">
              <LineChart
                marker={lv}
                xLabel="level"
                series={[
                  { name: 'max HP', color: 'var(--s8)', points: curve.map((r) => [r.level, r.hp[1]]), band: curve.map((r) => [r.level, r.hp[0], r.hp[2]]) },
                  { name: 'max EP', color: 'var(--s1)', points: curve.map((r) => [r.level, r.ep[1]]), band: curve.map((r) => [r.level, r.ep[0], r.ep[2]]) },
                ]}
              />
              <KV
                rows={[
                  ['HP add / level', `${fmt(c.growth.hp_add_range[0], 2)}–${fmt(c.growth.hp_add_range[1], 2)} + ${pct(c.growth.hp_pct_range[0], 2)}–${pct(c.growth.hp_pct_range[1], 2)} of max`],
                  ['EP add / level', `${fmt(c.growth.ep_add_range[0], 2)}–${fmt(c.growth.ep_add_range[1], 2)} + ${pct(c.growth.ep_pct_range[0], 2)}–${pct(c.growth.ep_pct_range[1], 2)} of max`],
                  ['Growth row (u32)', <code key="g">{c.growth.raw_u32.join(', ')}</code>],
                ]}
              />
            </Card>
          )}
        </div>
      </div>
      <Card title={`Forms (${forms.length})`} className="mt">
        <div className="form-grid">
          {forms.map((f) => {
            const s = sp.byGfx(f.gfx_id)
            const b = f.measured_bonus
            const flags: ReactNode[] = []
            if (f.flags & 1) flags.push('Super Saiyan gauge')
            if (f.flags & 2) flags.push('SS3 gauge')
            if (f.flags & 4) flags.push('fusion')
            return (
              <button key={f.index} className="form-card" style={f.index === curForm.index ? { borderColor: 'var(--game)', boxShadow: '0 0 0 1px var(--game)' } : undefined} onClick={() => setFormId(f.index)}>
                <SpriteThumb entity={s} size={64} />
                <b>{s?.name ?? f.name}</b>
                <span className="muted">
                  form {f.index} · gfx {f.gfx_id}
                </span>
                <span className="bonus">
                  {b.STR || b.walk_speed ? `+${b.STR} stats, ${b.walk_speed >= 0 ? '+' : ''}${b.walk_speed} speed` : 'no bonus'}
                </span>
                <span className="muted small">{flags.join(', ')}</span>
                <span className="muted small">
                  {f.transform_up !== 255 ? `↑ form ${f.transform_up} ` : ''}
                  {f.transform_down !== 255 ? `↓ form ${f.transform_down}` : ''}
                </span>
              </button>
            )
          })}
        </div>
        <p className="note">
          Super Saiyan: +5 STR/POW/END and +128 speed; the gauge fills to max EP and drains 1 per 20 frames, and energy moves spend the gauge
          instead of EP. SS3: +15 and +192. Bonuses verified in the emulator for all 54 forms.
        </p>
      </Card>
      <p className="small">
        <Link to="/g/buu/moves">Skills, costs and hit data →</Link>
      </p>
    </div>
  )
}
