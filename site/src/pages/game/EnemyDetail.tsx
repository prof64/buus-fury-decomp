import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { useData } from '../../data'
import { useSprites } from '../../lib/sprites'
import { enemiesOf, type EnemyVM } from '../../model/enemies'
import { SpritePlayer } from '../../components/SpritePlayer'
import { AiLegend, AiTree } from '../../components/AiTree'
import { BarList } from '../../components/Chart'
import { Addr, Card, Collapse, Crumbs, Empty, Evidence, JsonView, KV, Loading, Seg, Slider, SpriteThumb, Stat, Tag } from '../../components/ui'
import { Fields } from '../../components/ProseRecord'
import { fmt, pct, type J } from '../../lib/util'
import { stats as dstats, hitsToKill } from '../../formulas/dist'
import * as L1 from '../../formulas/log1'
import * as L2 from '../../formulas/log2'
import * as BU from '../../formulas/buu'

export function EnemyDetailPage({ game }: { game: string }) {
  const { id = '' } = useParams()
  const [params] = useSearchParams()
  const json = useData<J>(game, 'enemies')
  const sp = useSprites(game)
  const all = useMemo(() => (json ? enemiesOf(game, json, sp) : []), [json, sp, game])
  const e = all.find((x) => x.key === id)
  const [si, setSi] = useState(0)
  useEffect(() => setSi(0), [id])
  if (!json) return <Loading what="enemy" />
  if (!e) return <Empty>No enemy “{id}”.</Empty>
  const sprite = e.sprites[si] ?? e.sprites[0]
  const idx = all.indexOf(e)
  return (
    <div>
      <Crumbs items={[['Enemies & bosses', `/g/${game}/enemies`], [e.name]]} />
      <div className="row" style={{ justifyContent: 'space-between', marginBottom: 12 }}>
        <div>
          <h2 style={{ margin: 0 }}>
            {e.name} {e.boss && <Tag kind="bad">boss</Tag>} {!e.used && <Tag kind="muted">not placed in any map</Tag>}
          </h2>
          <div className="muted small">
            {e.idLabel} · <Addr a={e.rom} />
            {e.sub && <> · {e.sub}</>}
          </div>
        </div>
        <div className="row">
          {all[idx - 1] && <Link to={`/g/${game}/enemies/${all[idx - 1].key}`}>← {all[idx - 1].name}</Link>}
          {all[idx + 1] && <Link to={`/g/${game}/enemies/${all[idx + 1].key}`}>{all[idx + 1].name} →</Link>}
        </div>
      </div>
      <div className="split" style={{ gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1.25fr)' }}>
        <Card
          title="Sprite"
          actions={
            e.sprites.length > 1 && (
              <Seg value={si} onChange={setSi} options={e.sprites.map((s, i) => ({ value: i, label: s.name }))} />
            )
          }
        >
          {sprite ? (
            <>
              <SpritePlayer key={sprite.id} entity={sprite} compact zoom={sprite.frames.some((f) => f.h > 48) ? 2 : 3} />
              <p className="small">
                <Link to={`/g/${game}/sprites/${encodeURIComponent(sprite.id)}`}>Full sprite page →</Link>
              </p>
            </>
          ) : (
            <Empty>No graphics linked to this row.</Empty>
          )}
        </Card>
        <div className="stack">
          <Card title="Stats">
            <div className="stats">
              <Stat label="HP" value={fmt(e.hp)} />
              <Stat label="EXP" value={fmt(e.exp)} />
              {e.stats.map((s) => (
                <Stat key={s.label} label={s.label} value={fmt(s.value)} sub={s.title} />
              ))}
            </div>
            <StatsExtra game={game} e={e} />
          </Card>
          <Matchup game={game} e={e} />
        </div>
      </div>
      {game === 'log2' && <Log2Fights e={e} focus={params.get('fight')} all={all} />}
      {game === 'buu' && <BuuBoss e={e} />}
      <Behaviour game={game} e={e} json={json} />
      <Drops game={game} e={e} />
      <Spawns game={game} e={e} />
      <Collapse title="Raw record (JSON)">
        <JsonView value={e.raw} />
      </Collapse>
    </div>
  )
}

function StatsExtra({ game, e }: { game: string; e: EnemyVM }) {
  const r = e.raw
  if (game === 'log1')
    return (
      <KV
        rows={[
          ['Wake radius', `${fmt(r.wake_radius_px, 1)} px (aggro ${r.aggro_tiles} tiles; radius² = 96·tiles²)`],
          ['Speed', `${r.speed_raw_sixteenths}/16 px per frame (class ${r.speed_class})`],
          ['Ki regen', `${r.ki_regen_per_16_frames} per 16 frames`],
          ['Flying', r.flying ? 'yes (flying collision layer)' : 'no'],
          ['Group leader', r.group_flag >= 2 ? 'yes' : 'no'],
        ]}
      />
    )
  if (game === 'log2')
    return (
      <KV
        rows={[
          ['Damage taken', r.damage_taken_note],
          ['Death', r.death_behaviour],
          ['Initial AI state', r.ai_state],
          ['Body contact', r.body_contact_damage],
        ]}
      />
    )
  return (
    <KV
      rows={[
        ['Resist melee / energy', `${r.resist_melee} / ${r.resist_energy} (128 = ×1.0)`],
        ['Hit reaction', r.hit_reaction],
        ['Level byte', `${r.level_byte} (not read by combat code)`],
        ['Initial AI state', '0x' + Number(r.ai_state).toString(16)],
        ['Drop script', r.drop_script],
      ]}
    />
  )
}

/* ---------------- matchup: quick damage numbers against the player ---------------- */

function Matchup({ game, e }: { game: string; e: EnemyVM }) {
  if (game === 'log1') return <Log1Matchup e={e} />
  if (game === 'log2') return <Log2Matchup e={e} />
  return <BuuMatchup e={e} />
}

function Log1Matchup({ e }: { e: EnemyVM }) {
  const levels = useData<J>('log1', 'levels')
  const [lv, setLv] = useState(10)
  if (!levels) return null
  const row = levels.levels[lv - 1]
  const def = e.calc.def ?? 0
  const punch = L1.applyDefense(L1.punchDist(row.max_hp), def)
  const ps = dstats(punch)
  const blast = L1.applyDefense(new Map([[L1.kiBlastDamage(row.max_hp), 1]]), def)
  const raw = L1.enemyMeleeRaw(e.hp, row.max_hp)
  const em = L1.applyDefense(raw, L1.PLAYER_DEFENSE)
  const es = dstats(em)
  const neg = [...raw].filter(([v]) => v < 0).reduce((a, [, p]) => a + p, 0)
  const htk = e.hp > 0 && e.hp < 5e6 ? hitsToKill(punch, e.hp) : null
  return (
    <Card title="Matchup vs Goku" actions={<Slider label="Goku level" min={1} max={25} value={lv} onChange={setLv} />}>
      <div className="stats">
        <Stat label="Punch → enemy" value={`${fmt(ps.mean, 0)}`} sub={`avg after DEF ${def}; ${fmt(ps.min)}–${fmt(ps.max)}`} />
        <Stat label="Ki blast → enemy" value={fmt(dstats(blast).mean, 0)} sub={`maxHP/3 = ${L1.kiBlastDamage(row.max_hp)} raw`} />
        <Stat label="Punches to kill" value={htk ?? '—'} sub="50% chance" />
        <Stat label="Enemy melee → Goku" value={fmt(es.mean, 0)} sub={`avg after DEF 250; cap ${L1.damageCap(row.max_hp)}`} />
      </div>
      {neg > 0 && (
        <p className="note">
          rand() overflow bug: {pct(neg)} of this enemy’s melee rolls are negative — on a full-damage defense roll they <b>heal</b> Goku.
        </p>
      )}
    </Card>
  )
}

function useLog2Party() {
  const chars = useData<J>('log2', 'characters')
  return chars?.characters as J[] | undefined
}

function Log2Matchup({ e }: { e: EnemyVM }) {
  const chars = useLog2Party()
  const levels = useData<J>('log2', 'levels')
  const [ci, setCi] = useState(1)
  const [lv, setLv] = useState(20)
  if (!chars || !levels) return null
  const c = chars[ci]
  const g = c.growth_per_level
  const STR = Math.floor(L2.statAt(c.initial.STR_8p8, [g.STR.min_8p8, g.STR.max_8p8], lv).avg)
  const POW = Math.floor(L2.statAt(c.initial.POW_8p8, [g.POW.min_8p8, g.POW.max_8p8], lv).avg)
  const END = Math.floor(L2.statAt(c.initial.END_8p8, [g.END.min_8p8, g.END.max_8p8], lv).avg)
  const hp = L2.maxHpAt(c.initial.max_hp, lv).avg
  const punch = L2.damageDist({ type: 0, power: 1024, scale: 1024, S: STR, DEF: e.calc.END ?? 0, MULT: e.calc.meleeMult ?? 128 })
  const blast = L2.damageDist({ type: 1, power: 1024, scale: 1024, S: POW, DEF: e.calc.END ?? 0, MULT: e.calc.energyMult ?? 128 })
  const em = L2.damageDist({ type: 0, power: 1024, scale: 1024, S: e.calc.STR ?? 1, DEF: END, MULT: 128 })
  const ee = L2.damageDist({ type: 1, power: 1024, scale: 1024, S: e.calc.POW ?? 1, DEF: END, MULT: 128 })
  const expTable: number[] = levels.levels.map((l: J) => l.total_exp)
  const xp = L2.killExp(e.exp, lv, expTable)
  const ps = dstats(punch.dist)
  return (
    <Card
      title="Matchup vs party"
      actions={
        <div className="row">
          <select value={ci} onChange={(ev) => setCi(+ev.target.value)}>
            {chars.map((c: J, i: number) => (
              <option key={i} value={i}>
                {c.name}
              </option>
            ))}
          </select>
          <Slider label="Lv" min={1} max={50} value={lv} onChange={setLv} />
        </div>
      }
    >
      <div className="muted small" style={{ marginBottom: 8 }}>
        {c.name} Lv {lv} with average growth: STR {STR}, POW {POW}, END {END}, max HP ≈ {fmt(hp)}
      </div>
      <div className="stats">
        <Stat label="Punch → enemy" value={`${punch.normal[0]}–${punch.normal[1]}`} sub={`crit ${punch.crit[0]}–${punch.crit[1]} (${pct(punch.pCrit)})`} />
        <Stat label="Ki blast → enemy" value={`${blast.normal[0]}–${blast.normal[1]}`} />
        <Stat label="Punches to kill" value={ps.mean > 0 ? Math.ceil(e.hp / ps.mean) : '∞'} sub="≈ HP / avg" />
        <Stat label="Enemy melee → you" value={`${em.normal[0]}–${em.normal[1]}`} sub={`crit ${pct(em.pCrit)}`} />
        <Stat label="Enemy energy → you" value={`${ee.normal[0]}–${ee.normal[1]}`} />
        <Stat label="EXP for this kill" value={fmt(xp)} sub={e.exp === 0 ? 'class gives none (bosses reward via cutscene)' : e.exp > 5 ? 'clamped to [span/128, span/2]' : 'unclamped (≤ 5)'} />
      </div>
    </Card>
  )
}

function BuuMatchup({ e }: { e: EnemyVM }) {
  const chars = useData<J>('buu', 'characters')
  const [ci, setCi] = useState(0)
  const [extra, setExtra] = useState(0)
  if (!chars) return null
  const c = chars.characters[ci]
  const ng = c.new_game
  const STR = ng.STR + extra
  const POW = ng.POW + extra
  const punch = BU.damageDist({ kind: 0, sub: 2, power: 1024, mult: 1024, S: STR, D: e.calc.END ?? 0, resist: e.calc.resistMelee ?? 128 })
  const blast = BU.damageDist({ kind: 1, sub: 4, power: 1024, mult: 1024, S: POW, D: e.calc.END ?? 0, resist: e.calc.resistEnergy ?? 128 })
  const em = BU.damageDist({ kind: 0, sub: 2, power: 1024, mult: 1024, S: e.calc.STR ?? 1, D: ng.END, resist: 128 })
  const ps = dstats(punch.dist)
  const f = (v: number) => fmt(v, 1)
  return (
    <Card
      title="Matchup vs player"
      actions={
        <div className="row">
          <select value={ci} onChange={(ev) => setCi(+ev.target.value)}>
            {chars.characters.map((c: J, i: number) => (
              <option key={i} value={i}>
                {c.name.split(' (')[0]}
              </option>
            ))}
          </select>
          <Slider label="+STR/POW pts" min={0} max={150} value={extra} onChange={setExtra} />
        </div>
      }
    >
      <div className="muted small" style={{ marginBottom: 8 }}>
        {c.name.split(' (')[0]} new-game stats (Lv {ng.level}) STR {STR}, POW {POW}, END {ng.END}; base form, no equipment
      </div>
      <div className="stats">
        <Stat label="Punch → enemy" value={`${f(punch.normal[0])}–${f(punch.normal[1])}`} sub={`crit ×1.5 at ${pct(punch.pCrit)}`} />
        <Stat label="Energy blast → enemy" value={`${f(blast.normal[0])}–${f(blast.normal[1])}`} />
        <Stat label="Punches to kill" value={ps.mean > 0 ? Math.ceil(e.hp / ps.mean) : '∞'} sub="≈ HP / avg" />
        <Stat label="Enemy punch → you" value={`${f(em.normal[0])}–${f(em.normal[1])}`} sub={`blocked from the front: ÷16`} />
        <Stat label="XP for this kill" value={fmt(BU.xpGain(e.exp))} sub="× equipment XP multiplier" />
      </div>
    </Card>
  )
}

/* ---------------- behaviour ---------------- */

function Behaviour({ game, e, json }: { game: string; e: EnemyVM; json: J }) {
  if (game === 'log1') return <Log1Behaviour e={e} />
  if (game === 'log2') return <Log2Behaviour e={e} json={json} />
  return <BuuBehaviour e={e} />
}

function Log1Behaviour({ e }: { e: EnemyVM }) {
  const r = e.raw
  const moves = useData<J>('log1', 'moves')
  const ki = moves?.moves?.find((m: J) => m.id === 'enemy_ki_blast')
  const melee = L1.enemyMeleeRaw(e.hp)
  const ms = dstats(melee)
  return (
    <Card title="Behaviour (generic LoG1 AI)" className="mt">
      <p className="note">
        LoG1 has one AI routine for every enemy (tick 0x08008018, decision 0x0800755A); a template only picks three behaviours and a
        few radii.
      </p>
      <div className="grid3">
        <div>
          <h4>When idle — move type {r.move_type}</h4>
          <p>{r.move_type_desc}</p>
        </div>
        <div>
          <h4>Who it targets — target type {r.target_type}</h4>
          <p>{r.target_type_desc}</p>
        </div>
        <div>
          <h4>How it fights — fight style {r.fight_style}</h4>
          <p>{r.fight_style_desc}</p>
        </div>
      </div>
      <h4>Attacks</h4>
      <ul>
        <li>
          <b>Melee</b> when within 1 tile (Manhattan): <code>cur_hp/4 + rand(cur_hp/2)</code> → {fmt(ms.min)}–{fmt(ms.max)} at full HP,
          then capped at player maxHP/3 + 1 and reduced by the player’s defense roll. 16-frame animation, then a 45-frame cooldown.
        </li>
        <li>
          <b>Ki blast</b>:{' '}
          {r.ki_blast?.usable ? (
            <>
              needs Ki &gt; maxKi/16 and rough alignment, 2/101 chance per decision; damage = owner maxHP/3 = {fmt(Math.trunc(e.hp / 3))}{' '}
              (capped). {ki?.speed_px_per_frame} px/frame.
            </>
          ) : (
            'never (no Ki).'
          )}
        </li>
      </ul>
      <KV
        rows={[
          ['Wakes when', `player within ${fmt(r.wake_radius_px, 1)} px and within 144 px of the screen centre, or after being hit (aggro timer 900–1800 f)`],
          ['After being hit', 'stunned 20 frames, invulnerable 80 frames, no knockback'],
        ]}
      />
    </Card>
  )
}

function useStateFlash() {
  const [flash, setFlash] = useState<string>()
  const go = (s: string) => {
    setFlash(s)
    document.getElementById('ai-' + s)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    setTimeout(() => setFlash(undefined), 1500)
  }
  return [flash, go] as const
}

function Log2Behaviour({ e, json }: { e: EnemyVM; json: J }) {
  const ai = useData<J>('log2', 'ai')
  const [flash, go] = useStateFlash()
  const s = e.raw.ai_summary
  const attacks: J[] = (json.enemy_attacks ?? []).filter((a: J) => (e.raw.attack_keys ?? []).includes(a.key))
  const stateIds: number[] = useMemo(() => {
    const ids = new Set<number>([e.raw.ai_state, ...(s?.states ?? [])])
    return [...ids].sort((a, b) => a - b)
  }, [e, s])
  const [shown, setShown] = useState<Set<number>>(new Set())
  useEffect(() => setShown(new Set([e.raw.ai_state])), [e])
  if (!ai) return <Loading what="AI" />
  const st = (id: number) => ai.states.find((x: J) => x.id === id)
  const open = (id: number | string) => {
    const n = Number(id)
    setShown((prev) => new Set(prev).add(n))
    setTimeout(() => go(String(n)), 30)
  }
  return (
    <Card title="Behaviour (behaviour-tree AI)" className="mt">
      {s && (
        <KV
          rows={[
            ['Notices player', s.aggro_radius_px ? `${s.aggro_radius_px} px${s.aggro_requires_facing ? ' (only if facing the player)' : ''}` : '—'],
            ['Range checks', s.radii_px?.map((r: number) => r + ' px').join(', ')],
            [
              'Speeds',
              Object.entries(s.speeds ?? {})
                .map(([k, v]) => `${k} ${(v as number[]).join('/')} px/f`)
                .join(' · '),
            ],
            ['Attacks', (s.attacks ?? []).join('; ')],
            ['Waits', (s.waits ?? []).map((w: number) => w + ' f').join(', ')],
            ['Stunned by attack ids', (s.stun_on_attack_ids ?? []).join(', ') + ' (9 = Burning Attack, 15 = Spirit Bomb / Hercule)'],
            ['Absorbs energy', s.energy_absorb ? 'yes — energy hits heal 20% of raw damage instead' : 'no'],
            ['Patrols', s.uses_patrol ? 'yes (spawn waypoints)' : 'no'],
          ]}
        />
      )}
      {attacks.length > 0 && (
        <>
          <h4>Attack specs</h4>
          <div className="plain-table">
            <table>
              <thead>
                <tr>
                  <th>attack</th>
                  <th>node</th>
                  <th>attack id</th>
                  <th>stat</th>
                  <th>details</th>
                </tr>
              </thead>
              <tbody>
                {attacks.map((a) => (
                  <tr key={a.key}>
                    <td>{a.key.replace(/_/g, ' ')}</td>
                    <td className="mono small">{a.node ?? a.projectile?.node}</td>
                    <td className="num">
                      {a.attack_id} {a.attack_table_entry && <span className="muted">(power {a.attack_table_entry.power})</span>}
                    </td>
                    <td>{a.stat}</td>
                    <td className="wrap small">
                      {a.rule ?? a.hitbox ?? ''}
                      {a.projectile_params && (
                        <div>
                          {Object.entries(a.projectile_params)
                            .filter(([, v]) => typeof v !== 'object')
                            .map(([k, v]) => `${k.replace(/_/g, ' ')}: ${v}`)
                            .join(' · ')}
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      <h4 style={{ marginTop: 16 }}>AI states</h4>
      <p className="note">
        The enemy runs the tree of the state on top of its AI stack every frame; the on-hit tree runs when it is struck. Click a state
        link to open it.
      </p>
      <AiLegend />
      <div className="chips" style={{ marginBottom: 10 }}>
        {stateIds.map((sid) => (
          <button key={sid} className={'chip' + (shown.has(sid) ? ' on' : '')} onClick={() => open(sid)}>
            {sid}
            {sid === e.raw.ai_state ? ' (initial)' : ''}
          </button>
        ))}
        <button className="chip" onClick={() => setShown(new Set(stateIds))}>
          open all
        </button>
      </div>
      {[...shown]
        .sort((a, b) => a - b)
        .map((sid) => {
          const x = st(sid)
          if (!x) return null
          return (
            <div key={sid} id={'ai-' + sid} className={'ai-state' + (flash === String(sid) ? ' flash' : '')}>
              <h4>
                State {sid} {sid === e.raw.ai_state && <Tag kind="info">initial</Tag>} <Addr a={x.addr} />
                <button className="link" onClick={() => setShown((p) => (p.delete(sid), new Set(p)))}>
                  close
                </button>
              </h4>
              <div className="grid2">
                <div>
                  <div className="muted small">behaviour</div>
                  {x.behaviour ? <AiTree root={x.behaviour} onState={open} /> : <span className="muted">—</span>}
                </div>
                <div>
                  <div className="muted small">on hit</div>
                  {x.on_hit ? <AiTree root={x.on_hit} onState={open} /> : <span className="muted">—</span>}
                </div>
              </div>
            </div>
          )
        })}
    </Card>
  )
}

function BuuBehaviour({ e }: { e: EnemyVM }) {
  const ai = useData<J>('buu', 'enemy_ai')
  const [flash, go] = useStateFlash()
  const info = ai?.enemies?.find((x: J) => x.type === e.raw.type)
  const [shown, setShown] = useState<Set<string>>(new Set())
  useEffect(() => {
    if (info) setShown(new Set([info.initial_state]))
  }, [info])
  if (!ai) return <Loading what="AI" />
  if (!info) return null
  const s = info.summary ?? {}
  const norm = (v: number | string) => (typeof v === 'number' ? '0x' + v.toString(16) : v)
  const open = (id: number | string) => {
    const k = norm(id)
    setShown((p) => new Set(p).add(k))
    setTimeout(() => go(k), 30)
  }
  return (
    <Card title="Behaviour (behaviour-tree AI)" className="mt">
      <KV
        rows={[
          ['Notices player', (s.aggro_radius_px ?? []).map((r: number) => r + ' px').join(', ') || 'no aggro radius (acts immediately)'],
          ['Within checks', (s.within_px ?? []).map((r: number) => r + ' px').join(', ')],
          ['Farther checks', (s.farther_px ?? []).map((r: number) => r + ' px').join(', ')],
          [
            'Speeds',
            Object.entries(s.speeds ?? {})
              .map(([k, v]) => `${k} ${(v as number[]).join('/')} px/f`)
              .join(' · '),
          ],
          ['Attacks', (s.attacks ?? []).map((a: J) => `${a.node} (state ${a.state})`).join(', ')],
          ['Waits', (s.waits_frames ?? []).map((w: number) => w + ' f').join(', ')],
          ['Cooldowns', (s.cooldowns_frames ?? []).map((w: number) => w + ' f').join(', ')],
          ['HP thresholds', (s.hp_thresholds_percent ?? []).map((w: number) => w + '%').join(', ')],
          ['Blocks', s.blocks ? 'yes' : 'no'],
          ['Special nodes', (s.special_nodes ?? []).map((n: J) => (typeof n === 'string' ? n : n.name ?? JSON.stringify(n))).join(', ')],
        ]}
      />
      {s.chances && (
        <>
          <h4>Per-frame chances</h4>
          <BarList
            data={Object.entries(s.chances).map(([k, v]) => ({ label: k, value: Math.max(...(v as number[])) / 100, note: (v as number[]).join(', ') + ' %/frame' }))}
            format={(v) => (v * 100).toFixed(2) + '%/f'}
          />
        </>
      )}
      <h4 style={{ marginTop: 16 }}>AI states ({info.states.length})</h4>
      <AiLegend />
      <div className="chips" style={{ marginBottom: 10 }}>
        {info.states.map((sid: string) => (
          <button key={sid} className={'chip' + (shown.has(sid) ? ' on' : '')} onClick={() => open(sid)}>
            {sid}
            {sid === info.initial_state ? ' (initial)' : ''}
          </button>
        ))}
        <button className="chip" onClick={() => setShown(new Set(info.states))}>
          open all
        </button>
      </div>
      {[...shown].map((sid) => {
        const x = ai.states[sid]
        if (!x) return null
        return (
          <div key={sid} id={'ai-' + sid} className={'ai-state' + (flash === sid ? ' flash' : '')}>
            <h4>
              State {sid} ({parseInt(sid, 16)}) {sid === info.initial_state && <Tag kind="info">initial</Tag>}
              <button className="link" onClick={() => setShown((p) => (p.delete(sid), new Set(p)))}>
                close
              </button>
            </h4>
            {x.tree ? <AiTree root={x.tree} onState={open} /> : <pre className="json">{(x.text ?? []).join('\n')}</pre>}
          </div>
        )
      })}
    </Card>
  )
}

/* ---------------- drops & spawns ---------------- */

function Drops({ game, e }: { game: string; e: EnemyVM }) {
  const items = useData<J>(game, 'items')
  if (game === 'log1') return null
  if (game === 'log2') {
    const ds: J[] = e.raw.drop_scripts ?? []
    if (!ds.length) return null
    return (
      <Card title="Drops (per-spawn death scripts)" className="mt">
        <div className="grid2">
          {ds.map((d) => (
            <div key={d.script}>
              <div className="muted small">
                script <Addr a={d.script} /> · {d.spawn_count} spawn(s) · {d.drops?.roll}
              </div>
              {d.drops?.rules?.length ? (
                <BarList
                  data={d.drops.rules.map((r: J, i: number) => ({
                    label: `${r.item}${r.requires?.length ? ' — if ' + r.requires.join(', ') : ''}`,
                    value: (r.roll_below - (i ? d.drops.rules[i - 1].roll_below : 0)) / 100,
                    color: /Health|Fish/.test(r.item) ? 'var(--s8)' : 'var(--s1)',
                  }))}
                  format={(v) => pct(v, 0)}
                  max={0.4}
                />
              ) : (
                <div className="muted small">{d.drops?.note ?? 'no item drop'}</div>
              )}
            </div>
          ))}
        </div>
        <p className="note">First matching row wins; a failed condition falls through to the next row.</p>
      </Card>
    )
  }
  const dr = e.raw.drops
  if (!dr) return null
  const name = (k: string) => {
    const [kind, id] = k.split(':')
    if (kind === 'zenie') return `${id} Zenie coin`
    const list = kind === 'supply' ? items?.supplies : items?.equipment
    return (list?.find((x: J) => String(x.id) === id)?.name ?? k) + (kind === 'equip' ? ' (equipment)' : '')
  }
  return (
    <Card title="Loot (simulated from the drop bytecode)" className="mt">
      <div className="stats">
        <Stat label="Loot level" value={dr.loot_level} />
        <Stat label="Drops per kill" value={fmt(dr.simulated_per_kill?.avg_drops, 2)} />
        <Stat label="Zenie per kill" value={fmt(dr.simulated_per_kill?.avg_zenie, 1)} />
        {dr.forced_item !== null && dr.forced_item !== undefined && <Stat label="Forced item" value={name('equip:' + dr.forced_item).replace(' (equipment)', '')} sub="5% if not owned" />}
      </div>
      <h4>Expected count per kill</h4>
      <BarList
        data={Object.entries(dr.simulated_per_kill?.top ?? {}).map(([k, v]) => ({
          label: name(k),
          value: v as number,
          color: k.startsWith('zenie') ? 'var(--s4)' : k.startsWith('equip') ? 'var(--s7)' : 'var(--s3)',
        }))}
        format={(v) => fmt(v, 4)}
      />
    </Card>
  )
}

function Spawns({ game, e }: { game: string; e: EnemyVM }) {
  const r = e.raw
  let rows: ReactNode[] = []
  if (game === 'log1')
    rows = (r.appears ?? []).map((a: J, i: number) => (
      <tr key={i}>
        <td>
          <Link to={`/g/log1/maps/${a.map}`}>map {a.map}</Link>
        </td>
        <td className="num">{a.placed_enemies}</td>
        <td className="num">{a.scripted_npc_actors}</td>
      </tr>
    ))
  else if (game === 'log2')
    rows = (r.spawns ?? []).map((s: J, i: number) => (
      <tr key={i}>
        <td>
          <Link to={`/g/log2/maps/${s.map_index}`}>
            map {s.map_index} · {s.map_name}
          </Link>
        </td>
        <td className="mono">
          ({s.x}, {s.y})
        </td>
        <td>{s.spawn_condition === 0 ? 'at map load' : s.spawn_condition === 1 ? 'script only' : String(s.spawn_condition)}</td>
        <td className="num">{s.waypoints || ''}</td>
        <td>
          <Addr a={s.death_script} />
        </td>
      </tr>
    ))
  else
    rows = (r.spawns ?? []).map((s: J, i: number) => (
      <tr key={i}>
        <td>
          <Link to={`/g/buu/maps/${s.world}-${s.map}`}>
            {s.area} ({s.world}-{s.map})
          </Link>
        </td>
        <td className="num">{s.count}</td>
      </tr>
    ))
  if (!rows.length) return null
  const head =
    game === 'log1' ? ['map', 'placed', 'scripted'] : game === 'log2' ? ['map', 'position', 'spawn', 'waypoints', 'death script'] : ['area', 'count']
  return (
    <Card title={`Where it appears (${e.spawns})`} className="mt">
      <div className="plain-table" style={{ maxHeight: 320 }}>
        <table>
          <thead>
            <tr>
              {head.map((h) => (
                <th key={h}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>{rows}</tbody>
        </table>
      </div>
    </Card>
  )
}

/* ---------------- boss fights ---------------- */

function Log2Fights({ e, focus, all }: { e: EnemyVM; focus: string | null; all: EnemyVM[] }) {
  const bosses = useData<J>('log2', 'bosses')
  const fights: J[] = (bosses?.fights ?? []).filter((f: J) => f.classes.includes(Number(e.key)) || f.key === focus)
  if (!fights.length) return null
  return (
    <>
      {fights.map((f) => (
        <Card key={f.key} title={`Boss fight: ${f.name}`} className="mt">
          {f.notes && <p className="note">{f.notes}</p>}
          <div className="stack">
            {f.phases.map((p: J, i: number) => {
              const vm = all.find((x) => x.key === String(p.class_row))
              const meas = Object.entries(p.measured ?? {})
              return (
                <div key={i} className="ai-state">
                  <h4>
                    Phase {i + 1}: <Link to={`/g/log2/enemies/${p.class_row}`}>{vm?.name ?? 'class ' + p.class_row}</Link>
                    <span className="muted">
                      class {p.class_row} · map {p.map_index} {p.map_name} · {p.activation}
                    </span>
                  </h4>
                  <div className="row" style={{ alignItems: 'flex-start' }}>
                    <SpriteThumb entity={vm?.sprites[0]} size={56} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div className="stats">
                        <Stat label="HP" value={fmt(p.stats?.hp)} />
                        <Stat label="STR/POW/END" value={`${p.stats?.STR}/${p.stats?.POW}/${p.stats?.END}`} />
                        <Stat label="Detects at" value={p.detection_radius_px ? p.detection_radius_px + ' px' : '—'} />
                        <Stat label="AI" value={p.ai_state} sub={`${p.ai_states_used?.length ?? 0} states`} />
                      </div>
                      <div className="small" style={{ marginTop: 6 }}>
                        <b>Attacks:</b> {(p.attacks ?? []).join('; ')}
                        {p.special_attacks?.length > 0 && (
                          <>
                            {' '}
                            · <b>Specials:</b> {p.special_attacks.map((s: J) => (typeof s === 'string' ? s : s.name ?? s.node ?? JSON.stringify(s))).join('; ')}
                          </>
                        )}
                        {p.energy_absorb && <> · absorbs energy attacks</>}
                      </div>
                      {p.on_death?.effects?.length > 0 && (
                        <div className="small" style={{ marginTop: 4 }}>
                          <b>On death:</b>{' '}
                          {p.on_death.effects
                            .map((x: J) =>
                              x.op === 'give_exp'
                                ? `+${fmt(x.amount)} exp`
                                : x.op === 'change_map'
                                  ? `→ map ${x.map_index}`
                                  : x.op === 'spawn' || x.op === 'spawn_object'
                                    ? `spawn object ${x.index ?? x.object ?? ''}`
                                    : x.op + (x.char !== undefined ? ' ' + x.char : ''),
                            )
                            .join(' · ')}
                          {p.on_death.dialogue_excerpt?.[0] && <span className="muted"> — “{p.on_death.dialogue_excerpt[0].replace(/^[#@^]+/, '')}”</span>}
                        </div>
                      )}
                      {meas.length > 0 && (
                        <div className="small muted" style={{ marginTop: 4 }}>
                          Measured vs Lv40 Piccolo:{' '}
                          {meas
                            .map(([, m]) => {
                              const mm = m as J
                              return `start ${mm.start_dist}px → ${mm.hits} hits in ${mm.frames} f, dmg ${(mm.damage ?? []).slice(0, 3).join('/')}, every ${fmt(mm.mean_interval, 1)} f`
                            })
                            .join(' · ')}
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </Card>
      ))}
    </>
  )
}

function BuuBoss({ e }: { e: EnemyVM }) {
  const bosses = useData<J>('buu', 'bosses')
  const ai = useData<J>('buu', 'enemy_ai')
  const all: J = bosses?.bosses
  if (!all || typeof all !== 'object') return null
  const hex = '0x' + Number(e.raw.type).toString(16)
  const b: J = all[hex] ?? Object.values<J>(all).find((x) => x?.type === e.raw.type)
  if (!b) return null
  const sameAs = b.same_as && all[String(b.same_as).match(/0x[0-9a-f]+/i)?.[0]?.toLowerCase() ?? '']
  const projectiles: J = bosses.projectiles ?? {}
  const refs = (k: string, v: J) => {
    if (k !== 'projectile' || typeof v !== 'string' || !projectiles[v]) return null
    return (
      <Collapse title={`Projectile “${v}”: ${projectiles[v].what ?? ''}`}>
        <Fields obj={projectiles[v]} skip={['what', 'group']} />
      </Collapse>
    )
  }
  const usedProj = Object.keys(projectiles).filter((k) => JSON.stringify(b).includes(`"${k}"`))
  const skip = ['type', 'type_hex', 'group', 'name', 'stats_units', 'ai_states', 'tree', 'initial_state', 'status', 'verified']
  return (
    <Card
      title={`Boss analysis: ${b.name}`}
      className="mt"
      actions={
        <span className="row">
          <Tag kind="info">{b.group}</Tag>
          <Evidence text={b.status} />
        </span>
      }
    >
      {sameAs && (
        <p className="note">
          Same behaviour as <Link to={`/g/buu/enemies/${sameAs.type}`}>{sameAs.name}</Link> ({String(b.same_as)}).
        </p>
      )}
      <Fields obj={b} skip={skip} renderRef={refs} />
      {Array.isArray(b.verified) && b.verified.length > 0 && (
        <>
          <h4>Verified in the emulator</h4>
          <ul className="small">
            {b.verified.map((v: string, i: number) => (
              <li key={i}>
                <code>{v}</code>
              </li>
            ))}
          </ul>
        </>
      )}
      {usedProj.length > 0 && (
        <>
          <h4>Projectiles</h4>
          {usedProj.map((k) => (
            <Collapse key={k} title={`${k}: ${projectiles[k].what ?? ''}`}>
              <Fields obj={projectiles[k]} skip={['what', 'group']} />
            </Collapse>
          ))}
        </>
      )}
      {b.ai_states && !ai?.enemies?.find((x: J) => x.type === e.raw.type)?.states?.length && (
        <Collapse title={`AI states (${Object.keys(b.ai_states).length}, text)`}>
          <pre className="json">
            {Object.entries<string[]>(b.ai_states)
              .map(([k, v]) => `state ${k}\n${v.join('\n')}`)
              .join('\n\n')}
          </pre>
        </Collapse>
      )}
    </Card>
  )
}
