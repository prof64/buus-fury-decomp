import { useMemo, useState } from 'react'
import type { BattleConfig, EnemyChoice, FighterSetup, Ruleset, GameKey } from '../core/ruleset'
import { RULESET_INFO } from '../rulesets'
import { keysFor } from '../core/input'
import { KeyBindings, useKeymapVersion } from './KeyBindings'
import { Portrait, SpritePreview } from './SpritePreview'
import type { SpriteBank } from '../core/sprites'

export interface SetupState {
  game: GameKey
  fighter: FighterSetup
  mode: 'single' | 'preset'
  enemy: string
  count: number
  preset: string
  arena: string
  seed: number
}

export function defaultSetup(rs: Ruleset): SetupState {
  const f = rs.fighters()[4] ?? rs.fighters()[0]
  const enemies = rs.enemies()
  const defEnemy = enemies.find((e) => e.id === 'c53') ?? [...enemies].filter((e) => e.group === 'Boss').sort((a, b) => a.hp - b.hp)[0] ?? enemies[0]
  const opts: Record<string, string | number | boolean> = {}
  const bonus: Record<string, number> = {}
  for (const k of rs.knobs()) {
    if (k.bonus) bonus[k.key] = Number(k.default)
    else opts[k.key] = k.default
  }
  return {
    game: rs.info.id,
    fighter: { fighter: f.id, form: f.defaultForm, level: Math.max(rs.levelRange[0], rs.levelRange[1] >= 30 ? 30 : Math.ceil(rs.levelRange[1] / 2)), bonus, options: opts },
    mode: 'single',
    enemy: defEnemy?.id ?? '',
    count: defEnemy?.group === 'Boss' ? 1 : 3,
    preset: rs.presets()[0]?.id ?? '',
    arena: rs.arenas()[0]?.id ?? '',
    seed: 1234,
  }
}

export function toConfig(rs: Ruleset, s: SetupState): BattleConfig {
  const phases = s.mode === 'preset' ? (rs.presets().find((p) => p.id === s.preset)?.phases ?? []) : [{ enemy: s.enemy, count: s.count }]
  return { game: s.game, fighter: s.fighter, phases, arena: s.arena, seed: s.seed, options: {} }
}

const GROUP_ORDER = ['Boss', 'Enemy', 'NPC (scouter row)', 'Unused row']
const groupRank = (g: string) => (GROUP_ORDER.includes(g) ? GROUP_ORDER.indexOf(g) : GROUP_ORDER.length)

export function SetupScreen({ rs, state, onChange, onStart, onGame }: { rs: Ruleset; state: SetupState; onChange: (s: SetupState) => void; onStart: () => void; onGame?: (g: GameKey) => void }) {
  useKeymapVersion()
  const fighters = useMemo(() => rs.fighters(), [rs])
  const enemies = useMemo(() => rs.enemies(), [rs])
  const arenas = useMemo(() => rs.arenas(), [rs])
  const presets = useMemo(() => rs.presets(), [rs])
  const knobs = useMemo(() => rs.knobs(), [rs])
  const [q, setQ] = useState('')
  const [group, setGroup] = useState<string>('all')
  const fighter = fighters.find((f) => f.id === state.fighter.fighter) ?? fighters[0]
  const form = fighter.forms.find((f) => f.id === state.fighter.form) ?? fighter.forms[0]
  const bank = rs.sprites()
  const setF = (p: Partial<FighterSetup>) => onChange({ ...state, fighter: { ...state.fighter, ...p } })
  const preview = rs.preview(state.fighter)
  const enemy = enemies.find((e) => e.id === state.enemy)
  const preset = presets.find((p) => p.id === state.preset)
  const shown = enemies
    .filter((e) => group === 'all' || e.group === group)
    .filter((e) => !q || (e.name + ' ' + e.id + ' ' + e.group + ' ' + e.tags.join(' ')).toLowerCase().includes(q.toLowerCase()))
    .sort((a, b) => groupRank(a.group) - groupRank(b.group) || a.hp - b.hp)
  const groups = [...new Set(enemies.map((e) => e.group))].sort((a, b) => groupRank(a) - groupRank(b))

  return (
    <div className="bt-setup">
      <div className="bt-games">
        {RULESET_INFO.map((g) => (
          <button key={g.id} className={g.id === state.game ? 'on' : ''} disabled={!g.enabled} title={g.note} onClick={() => g.enabled && onGame?.(g.id)}>
            {g.title}
            {!g.enabled && <small> — {g.note}</small>}
          </button>
        ))}
      </div>

      <div className="bt-grid">
        <section className="bt-card">
          <h3>Fighter</h3>
          <div className="bt-fighters">
            {fighters.map((f) => (
              <button key={f.id} className={'bt-fighter' + (f.id === fighter.id ? ' on' : '')} onClick={() => setF({ fighter: f.id, form: f.defaultForm })}>
                <Portrait src={f.portrait} size={40} />
                <span>{f.name}</span>
              </button>
            ))}
          </div>
          <div className="bt-fighter-detail">
            <div className="bt-stage">
              <SpritePreview sprite={form.visual.sprite ? bank.get(form.visual.sprite) : undefined} size={56} scale={3} />
            </div>
            <div className="bt-forms">
              <label>Form</label>
              {fighter.forms.map((f) => (
                <button key={f.id} className={'bt-chip' + (f.id === form.id ? ' on' : '')} onClick={() => setF({ form: f.id })} title={f.note}>
                  <Portrait src={f.portrait} size={24} />
                  {f.name}
                </button>
              ))}
              <p className="muted small">{form.note}</p>
            </div>
          </div>
          <div className="bt-row">
            <label>
              Level <b>{state.fighter.level}</b>
            </label>
            <input type="range" min={rs.levelRange[0]} max={rs.levelRange[1]} value={state.fighter.level} onChange={(e) => setF({ level: +e.target.value })} />
          </div>
          <div className="bt-knobs">
            {knobs.map((k) => {
              const val = k.bonus ? state.fighter.bonus[k.key] : state.fighter.options[k.key]
              const set = (v: number | string | boolean) => (k.bonus ? setF({ bonus: { ...state.fighter.bonus, [k.key]: Number(v) } }) : setF({ options: { ...state.fighter.options, [k.key]: v } }))
              return (
                <label key={k.key} title={k.hint} className="bt-knob">
                  <span>{k.label}</span>
                  {k.kind === 'select' ? (
                    <select value={String(val)} onChange={(e) => set(e.target.value)}>
                      {k.options!.map((o) => (
                        <option key={o.value} value={o.value}>
                          {o.label}
                        </option>
                      ))}
                    </select>
                  ) : k.kind === 'boolean' ? (
                    <input type="checkbox" checked={!!val} onChange={(e) => set(e.target.checked)} />
                  ) : (
                    <input type="number" min={k.min} max={k.max} value={Number(val ?? 0)} onChange={(e) => set(Math.max(k.min ?? 0, Math.min(k.max ?? 1e9, +e.target.value || 0)))} />
                  )}
                </label>
              )
            })}
          </div>
          <table className="bt-stats">
            <tbody>
              {preview.map((s) => (
                <tr key={s.label} title={s.hint}>
                  <td>{s.label}</td>
                  <td>{s.value}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <ul className="bt-moves">
            {fighter.moves.map((m, i) => (
              <li key={i}>
                <code>{m.button}</code> {m.name} {m.note && <span className="muted">· {m.note}</span>}
              </li>
            ))}
          </ul>
        </section>

        <section className="bt-card">
          <h3>Opponent</h3>
          <div className="bt-tabs">
            <button className={state.mode === 'single' ? 'on' : ''} onClick={() => onChange({ ...state, mode: 'single' })}>
              Any enemy / boss
            </button>
            <button className={state.mode === 'preset' ? 'on' : ''} onClick={() => onChange({ ...state, mode: 'preset' })}>
              Story boss fights
            </button>
          </div>
          {state.mode === 'single' ? (
            <>
              <div className="bt-row">
                <input className="bt-search" placeholder={`Search ${enemies.length} enemies…`} value={q} onChange={(e) => setQ(e.target.value)} />
                <select value={group} onChange={(e) => setGroup(e.target.value)}>
                  <option value="all">All</option>
                  {groups.map((g) => (
                    <option key={g}>{g}</option>
                  ))}
                </select>
              </div>
              <div className="bt-enemy-list">
                {shown.map((e) => (
                  <EnemyRow key={e.id} e={e} on={e.id === state.enemy} bank={bank} onClick={() => onChange({ ...state, enemy: e.id })} />
                ))}
              </div>
              {enemy && (
                <div className="bt-enemy-detail">
                  <div className="bt-stage">
                    <SpritePreview sprite={enemy.visual.sprite ? bank.get(enemy.visual.sprite) : undefined} size={64} scale={2} />
                  </div>
                  <div>
                    <b>{enemy.name}</b> <span className="muted">({enemy.group})</span>
                    <div className="small">
                      {Object.entries(enemy.stats)
                        .map(([k, v]) => `${k} ${v}`)
                        .join(' · ')}
                    </div>
                    {enemy.note && <div className="small muted">{enemy.note}</div>}
                    <div className="small">{enemy.faithfulAi ? 'AI: decoded ROM state machine' : 'AI: none (Nop row)'}</div>
                    <div className="bt-row">
                      <label>Count</label>
                      <input type="number" min={1} max={12} value={state.count} onChange={(e) => onChange({ ...state, count: Math.max(1, Math.min(12, +e.target.value || 1)) })} />
                    </div>
                  </div>
                </div>
              )}
            </>
          ) : (
            <div className="bt-presets">
              {presets.map((p) => (
                <button
                  key={p.id}
                  className={'bt-preset' + (p.id === state.preset ? ' on' : '')}
                  onClick={() =>
                    onChange({
                      ...state,
                      preset: p.id,
                      arena: p.arena ?? state.arena,
                    })
                  }
                >
                  <b>{p.name}</b>
                  <span className="small">
                    {p.phases.map((ph) => enemies.find((e) => e.id === ph.enemy)?.name ?? ph.enemy).join(' → ')}
                  </span>
                </button>
              ))}
              {preset?.note && <p className="small muted">{preset.note}</p>}
            </div>
          )}
        </section>

        <section className="bt-card">
          <h3>Arena</h3>
          <div className="bt-arenas">
            {arenas.map((a) => (
              <button key={a.id} className={'bt-arena' + (a.id === state.arena ? ' on' : '')} onClick={() => onChange({ ...state, arena: a.id })}>
                <div
                  className="bt-arena-img"
                  style={{
                    backgroundImage: `url(${import.meta.env.BASE_URL + a.image})`,
                    backgroundPosition: `-${a.bounds.x0 + (a.bounds.x1 - a.bounds.x0) / 2 - 80}px -${a.bounds.y0 + (a.bounds.y1 - a.bounds.y0) / 2 - 50}px`,
                  }}
                />
                <span>{a.name}</span>
              </button>
            ))}
          </div>
          <div className="bt-row">
            <label title="RNG seed = frames spent on the title screen (the game's only entropy)">Seed</label>
            <input type="number" value={state.seed} onChange={(e) => onChange({ ...state, seed: +e.target.value || 0 })} />
            <button className="bt-small" onClick={() => onChange({ ...state, seed: Math.floor(Math.random() * 100000) })}>
              random
            </button>
          </div>
          <h3>Controls</h3>
          <table className="bt-controls">
            <tbody>
              {rs.controls().map((c) => (
                <tr key={c.button}>
                  <td>
                    <b>{c.button === 'DPAD' ? 'D-pad' : c.button}</b>
                    <div className="small muted">
                      {c.button === 'DPAD' ? ['UP', 'DOWN', 'LEFT', 'RIGHT'].map((d) => keysFor(d as 'UP')[0] ?? '?').join(' ') : keysFor(c.button).join(' / ')}
                    </div>
                  </td>
                  <td className="small">{c.action}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <details className="bt-keys-details">
            <summary>Change key bindings</summary>
            <KeyBindings />
          </details>
          <button className="bt-start" onClick={onStart}>
            FIGHT!
          </button>
        </section>
      </div>
    </div>
  )
}

function EnemyRow({ e, on, bank, onClick }: { e: EnemyChoice; on: boolean; bank: SpriteBank; onClick: () => void }) {
  return (
    <button className={'bt-enemy' + (on ? ' on' : '')} onClick={onClick}>
      <SpritePreview sprite={e.visual.sprite ? bank.get(e.visual.sprite) : undefined} size={40} scale={1} animate={on} />
      <span className="name">
        {e.name}
        <small>
          {e.group} · class {String(e.stats.class)} · HP {e.hp}
          {e.tags.length ? ' · ' + e.tags.join(', ') : ''}
        </small>
      </span>
    </button>
  )
}
