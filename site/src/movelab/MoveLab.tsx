import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useData } from '../data'
import { useSprites } from '../lib/sprites'
import { buuMoves, log1Moves, log2Moves, type MoveVM } from '../model/moves'
import { enemiesOf } from '../model/enemies'
import { useAttackerSprites } from '../pages/game/Moves'
import { SpritePlayer, type Overlay } from '../components/SpritePlayer'
import { HexView } from '../components/HexView'
import { Addr, Card, Collapse, Loading, NumField, Slider, Stat, Tag } from '../components/ui'
import { DistView } from '../pages/game/formulas/common'
import { applyDefense, chargeCost, enemyMeleeRaw, kiSpent, PLAYER_DEFENSE } from '../formulas/log1'
import * as L2 from '../formulas/log2'
import * as BU from '../formulas/buu'
import { mapDist, stats, type Dist } from '../formulas/dist'
import { download, fmt, romOffset, type J } from '../lib/util'
import { encode, FACINGS, fromTemplate, hexOf, mirrorX, validate, type CustomMove, type Facing, type Rect4 } from './schema'
import { STRUCT_DOCS } from './docs'

export function MoveLabPage({ game }: { game: string }) {
  const g = game as CustomMove['game']
  const moves = useData<J>(game, 'moves')
  const detail = useData<J>(game, game === 'buu' ? 'attacks_detail' : 'moves')
  const [params, setParams] = useSearchParams()
  const templates: MoveVM[] = useMemo(() => {
    if (!moves) return []
    if (game === 'log1') return log1Moves(moves).filter((m) => m.key !== 'fly')
    if (game === 'log2') return log2Moves(moves).filter((m) => !/transform/i.test(m.name))
    return detail ? buuMoves(moves, detail).filter((m) => m.key === 'melee' || m.raw?.measured?.by_level?.['0']?.damage_multipliers?.length) : []
  }, [moves, detail, game])
  const tKey = params.get('template') ?? templates[0]?.key
  const tmpl = templates.find((t) => t.key === tKey) ?? templates[0]
  const [move, setMove] = useState<CustomMove>()
  useEffect(() => {
    if (tmpl) setMove(fromTemplate(g, tmpl, { attackTable: moves?.attack_table, buuTypes: moves?.attack_types }))
  }, [tmpl, g, moves])
  const [importText, setImportText] = useState('')
  const [importErr, setImportErr] = useState<string>()
  if (!moves || !tmpl || !move) return <Loading what="move lab" />
  const set = (fn: (m: CustomMove) => void) => {
    const n: CustomMove = JSON.parse(JSON.stringify(move))
    fn(n)
    setMove(n)
  }
  const encoded = encode(move, tmpl)
  const exportObj: CustomMove = {
    ...move,
    rom: encoded[0] ? { table: encoded[0].title, address: encoded[0].address ? '0x' + encoded[0].address.toString(16).toUpperCase().padStart(8, '0') : '', bytes: hexOf(encoded[0].fields) } : undefined,
  }
  const json = JSON.stringify(exportObj, null, 2)
  return (
    <div>
      <p className="lede">
        How to make your own move: pick an existing move as a template, change its numbers, and see what it would do in the real damage
        formula, where its hitbox lands on the sprite, and the bytes it would occupy in the ROM tables. Export the result as JSON
        (format: <code>site/src/movelab/SCHEMA.md</code>).
      </p>
      <Collapse title={`How a move is built in ${game === 'log1' ? 'The Legacy of Goku' : game === 'log2' ? 'LoG II' : 'Buu’s Fury'} (byte-by-byte)`} open={false}>
        {STRUCT_DOCS[game]}
      </Collapse>
      <div className="lab" style={{ marginTop: 16 }}>
        <div className="lab-form">
          <fieldset>
            <legend>Template</legend>
            <label className="field" style={{ flex: 1 }}>
              <span>Start from</span>
              <select value={tmpl.key} onChange={(e) => setParams({ template: e.target.value }, { replace: true })}>
                {templates.map((t) => (
                  <option key={t.key} value={t.key}>
                    {t.user === 'enemy' ? 'Enemy: ' : ''}
                    {t.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field" style={{ flex: 1 }}>
              <span>Name</span>
              <input value={move.name} onChange={(e) => set((m) => (m.name = e.target.value))} />
            </label>
          </fieldset>
          <fieldset>
            <legend>Damage</legend>
            {game === 'log1' ? (
              <Log1DamageFields move={move} set={set} />
            ) : (
              <>
                <label className="field">
                  <span>Kind</span>
                  <select
                    value={move.attack.kind}
                    onChange={(e) => set((m) => ((m.attack.kind = e.target.value as 'melee' | 'energy'), (m.attack.canCrit = e.target.value === 'melee' && m.attack.attackId !== 3)))}
                  >
                    <option value="melee">melee (STR, can crit)</option>
                    <option value="energy">energy (POW)</option>
                  </select>
                </label>
                <NumField label="power (1024 = 1×)" value={move.attack.power} onChange={(v) => set((m) => (m.attack.power = v))} min={0} />
                <NumField label={game === 'log2' ? 'attack id byte' : 'attack sub'} value={move.attack.attackId} onChange={(v) => set((m) => (m.attack.attackId = v))} min={0} max={255} />
                <NumField label={game === 'log2' ? 'hit scale (1024)' : 'hit mult (1024)'} value={move.attack.scale} onChange={(v) => set((m) => (m.attack.scale = v))} min={0} />
              </>
            )}
          </fieldset>
          <fieldset>
            <legend>Cost & frame data</legend>
            <NumField label={`min ${move.cost.unit}`} value={move.cost.min} onChange={(v) => set((m) => (m.cost.min = v))} min={0} width={70} />
            <NumField label={`${move.cost.unit} on start`} value={move.cost.start} onChange={(v) => set((m) => (m.cost.start = v))} min={0} width={70} />
            <NumField label={`${move.cost.unit}/frame held`} value={+move.cost.perFrame.toFixed(4)} onChange={(v) => set((m) => (m.cost.perFrame = v))} min={0} step={0.01} width={80} />
            <NumField label="startup f" value={move.timing.startupFrames} onChange={(v) => set((m) => (m.timing.startupFrames = v))} min={0} width={70} />
            <NumField label="active f" value={move.timing.activeFrames} onChange={(v) => set((m) => (m.timing.activeFrames = v))} min={1} width={70} />
            <NumField label="recovery f" value={move.timing.recoveryFrames} onChange={(v) => set((m) => (m.timing.recoveryFrames = v))} min={0} width={70} />
            <NumField label="hits" value={move.timing.hits} onChange={(v) => set((m) => (m.timing.hits = v))} min={1} width={70} />
            <NumField label="re-hit every f" value={move.timing.rehitInterval} onChange={(v) => set((m) => (m.timing.rehitInterval = v))} min={0} width={70} />
          </fieldset>
          <fieldset>
            <legend>Hitbox (relative to the attacker’s position)</legend>
            <div className="rect-edit">
              <span />
              {['x0', 'y0', 'x1', 'y1'].map((h) => (
                <span key={h} className="h">
                  {h}
                </span>
              ))}
              {FACINGS.map((f) => (
                <RectRow key={f} f={f} r={move.hitbox[f]} onChange={(r) => set((m) => (m.hitbox[f] = r))} />
              ))}
            </div>
            <button onClick={() => set((m) => (m.hitbox.left = mirrorX(m.hitbox.right)))}>Mirror right → left</button>
          </fieldset>
          <fieldset>
            <legend>
              <label>
                <input
                  type="checkbox"
                  checked={!!move.projectile}
                  onChange={(e) =>
                    set((m) => (m.projectile = e.target.checked ? { speed: 3, w: 16, h: 16, lifetime: 40, pierce: false, spawn: { down: [8, 0], up: [8, -8], left: [-2, -8], right: [18, -8] } } : null))
                  }
                />{' '}
                Projectile
              </label>
            </legend>
            {move.projectile ? (
              <>
                <NumField label="speed px/f" value={move.projectile.speed} onChange={(v) => set((m) => (m.projectile!.speed = v))} min={0} step={0.25} width={70} />
                <NumField label="w" value={move.projectile.w} onChange={(v) => set((m) => (m.projectile!.w = v))} min={1} width={60} />
                <NumField label="h" value={move.projectile.h} onChange={(v) => set((m) => (m.projectile!.h = v))} min={1} width={60} />
                <NumField label="lifetime f" value={move.projectile.lifetime} onChange={(v) => set((m) => (m.projectile!.lifetime = v))} min={0} width={70} />
                <label className="field">
                  <span>pierce</span>
                  <input type="checkbox" checked={move.projectile.pierce} onChange={(e) => set((m) => (m.projectile!.pierce = e.target.checked))} />
                </label>
                <div className="rect-edit" style={{ gridTemplateColumns: '54px repeat(2, minmax(0,1fr))' }}>
                  <span />
                  <span className="h">spawn dx</span>
                  <span className="h">spawn dy</span>
                  {FACINGS.map((f) => (
                    <SpawnRow key={f} f={f} p={move.projectile!.spawn[f]} onChange={(p) => set((m) => (m.projectile!.spawn[f] = p))} />
                  ))}
                </div>
              </>
            ) : (
              <span className="muted small">Instant hit: only the rectangle above is tested.</span>
            )}
          </fieldset>
          <fieldset>
            <legend>Effects</legend>
            <NumField label="stun frames" value={move.effects.stunFrames} onChange={(v) => set((m) => (m.effects.stunFrames = v))} min={0} width={80} />
            <NumField label="knockback param" value={move.effects.knockback} onChange={(v) => set((m) => (m.effects.knockback = v))} min={0} width={80} />
          </fieldset>
        </div>
        <div className="stack">
          <HitboxPreview game={game} move={move} />
          <DamagePreview game={game} move={move} />
          <Card title="ROM bytes">
            <p className="note">
              Little-endian, as the game stores them. Addresses are GBA bus addresses; file offset = address − 0x08000000. Table slots right
              after the existing entries are suggestions — check the space is free (or repoint the table) before patching.
            </p>
            {encoded.map((e) => (
              <div key={e.title} style={{ marginBottom: 16 }}>
                <h4 style={{ margin: '6px 0' }}>
                  {e.title}{' '}
                  {e.address !== undefined && (
                    <>
                      <Addr a={e.address} /> <span className="muted small">(file {romOffset(e.address)})</span>
                    </>
                  )}
                </h4>
                <div className="muted small" style={{ marginBottom: 6 }}>
                  {e.where}
                </div>
                <HexView fields={e.fields} base={e.address} />
              </div>
            ))}
          </Card>
          <Card
            title="Export"
            actions={
              <div className="row">
                <button className="primary" onClick={() => download(`${move.name.replace(/\W+/g, '_').toLowerCase() || 'move'}.${game}.move.json`, json)}>
                  Download JSON
                </button>
                <button onClick={() => navigator.clipboard?.writeText(json)}>Copy</button>
              </div>
            }
          >
            <pre className="json" style={{ maxHeight: 260 }}>
              {json}
            </pre>
            <h4>Import</h4>
            <textarea rows={4} placeholder="Paste a custom-move JSON here…" value={importText} onChange={(e) => setImportText(e.target.value)} />
            <div className="row" style={{ marginTop: 6 }}>
              <button
                onClick={() => {
                  try {
                    const j = JSON.parse(importText)
                    const err = validate(j)
                    if (err) return setImportErr(err)
                    if (j.game !== game) return setImportErr(`this move is for ${j.game}; switch game first`)
                    setImportErr(undefined)
                    setMove(j)
                  } catch (e) {
                    setImportErr(String(e))
                  }
                }}
              >
                Load
              </button>
              {importErr && <Tag kind="bad">{importErr}</Tag>}
            </div>
          </Card>
        </div>
      </div>
    </div>
  )
}

function RectRow({ f, r, onChange }: { f: Facing; r: Rect4; onChange: (r: Rect4) => void }) {
  return (
    <>
      <span>{f}</span>
      {r.map((v, i) => (
        <input
          key={i}
          type="number"
          value={v}
          onChange={(e) => {
            const n = [...r] as Rect4
            n[i] = Number(e.target.value) || 0
            onChange(n)
          }}
        />
      ))}
    </>
  )
}

function SpawnRow({ f, p, onChange }: { f: Facing; p: [number, number]; onChange: (p: [number, number]) => void }) {
  return (
    <>
      <span>{f}</span>
      {p.map((v, i) => (
        <input
          key={i}
          type="number"
          value={v}
          onChange={(e) => {
            const n = [...p] as [number, number]
            n[i] = Number(e.target.value) || 0
            onChange(n)
          }}
        />
      ))}
    </>
  )
}

function Log1DamageFields({ move, set }: { move: CustomMove; set: (fn: (m: CustomMove) => void) => void }) {
  const d = move.attack.log1 ?? { mode: 'maxhp_div', divisor: 3 }
  return (
    <>
      <label className="field">
        <span>Damage model</span>
        <select value={d.mode} onChange={(e) => set((m) => (m.attack.log1 = { ...d, mode: e.target.value as typeof d.mode }))}>
          <option value="maxhp_pct">(maxHP × (base + rand2(range))) &gt;&gt; 9 — like the punch</option>
          <option value="maxhp_div">maxHP / divisor — like the ki blast</option>
          <option value="ki_spent">Ki spent charging — like the Kamehameha</option>
          <option value="cur_hp">cur HP/4 + rand(cur HP/2) — enemy melee</option>
          <option value="fixed">fixed value</option>
        </select>
      </label>
      {d.mode === 'maxhp_pct' && (
        <>
          <NumField label="base" value={d.base ?? 384} onChange={(v) => set((m) => (m.attack.log1 = { ...d, base: v }))} min={0} />
          <NumField label="range" value={d.range ?? 128} onChange={(v) => set((m) => (m.attack.log1 = { ...d, range: v }))} min={1} />
        </>
      )}
      {d.mode === 'maxhp_div' && <NumField label="divisor" value={d.divisor ?? 3} onChange={(v) => set((m) => (m.attack.log1 = { ...d, divisor: Math.max(1, v) }))} min={1} />}
      {d.mode === 'fixed' && <NumField label="damage" value={d.fixed ?? 0} onChange={(v) => set((m) => (m.attack.log1 = { ...d, fixed: v }))} min={0} />}
      <p className="note" style={{ width: '100%' }}>
        LoG1 damage is code, not data: the chosen expression is what you would implement in the projectile’s update callback (the stock ones
        are at 0x08005676 punch, 0x0800B596 ki blast, 0x0800BD20 Kamehameha).
      </p>
    </>
  )
}

function HitboxPreview({ game, move }: { game: string; move: CustomMove }) {
  const { list } = useAttackerSprites(game)
  const [ai, setAi] = useState(0)
  const [dir, setDir] = useState<string>('right')
  const a = list[Math.min(ai, list.length - 1)]
  const overlays = (d: string): Overlay[] => {
    const f = d as Facing
    const r = move.hitbox[f]
    if (!r) return []
    const out: Overlay[] = [{ rect: { x0: r[0], y0: r[1], x1: r[2], y1: r[3] }, color: '#ff3b3b', label: 'hitbox' }]
    const p = move.projectile
    if (p && p.speed > 0) {
      const v: Record<Facing, [number, number]> = { down: [0, 1], up: [0, -1], left: [-1, 0], right: [1, 0] }
      const steps = 4
      const span = Math.min(p.lifetime || 30, 30)
      for (let i = 0; i <= steps; i++) {
        const t = (span * i) / steps
        const x = p.spawn[f][0] + v[f][0] * p.speed * t - p.w / 2
        const y = p.spawn[f][1] + v[f][1] * p.speed * t - p.h / 2
        out.push({ rect: { x0: x, y0: y, x1: x + p.w, y1: y + p.h }, color: i ? '#ff9f1c' : '#b04bff', label: i ? `+${Math.round(t)} f` : 'spawn', dashed: i > 0 })
      }
    }
    return out
  }
  if (!a) return null
  return (
    <Card
      title="Hitbox on the sprite"
      actions={
        <select value={ai} onChange={(e) => setAi(+e.target.value)}>
          {list.map((x, i) => (
            <option key={i} value={i}>
              {x.label}
            </option>
          ))}
        </select>
      }
    >
      <SpritePlayer key={a.e.id} entity={a.e} compact zoom={3} dir={dir} onDir={setDir} initialDir="right" overlays={overlays} />
      <p className="note">
        Red = the move’s hit rectangle for the shown facing; purple/orange = projectile at spawn and along its path (first 30 frames). Dashed blue
        = the attacker’s own body rectangle.
      </p>
    </Card>
  )
}

function useEnemyChoice(game: string) {
  const json = useData<J>(game, 'enemies')
  const sp = useSprites(game)
  return useMemo(() => (json ? enemiesOf(game, json, sp).filter((e) => e.used && e.hp > 1) : []), [json, sp, game])
}

function DamagePreview({ game, move }: { game: string; move: CustomMove }) {
  const enemies = useEnemyChoice(game)
  const chars = useData<J>(game, 'characters')
  const levels = useData<J>(game, 'levels')
  const [eiSel, setEi] = useState<number | null>(null)
  const ei = eiSel ?? Math.max(0, enemies.findIndex((x) => /wolf|majin fighter/i.test(x.name)))
  const [lv, setLv] = useState(game === 'buu' ? 0 : game === 'log1' ? 10 : 30)
  const [ci, setCi] = useState(game === 'log2' ? 4 : 0)
  const [held, setHeld] = useState(60)
  if (!chars || !levels || !enemies.length) return <Loading what="damage preview" />
  const e = enemies[Math.min(ei, enemies.length - 1)]
  let dist: Dist
  let info: ReactNode = null
  let unitHp = e.hp
  if (game === 'log1') {
    const row = levels.levels[Math.max(0, Math.min(24, lv - 1))]
    const d = move.attack.log1 ?? { mode: 'maxhp_div', divisor: 3 }
    let raw: Dist
    if (d.mode === 'maxhp_pct') {
      raw = new Map()
      const base = d.base ?? 384
      const rng = Math.max(1, d.range ?? 128)
      for (let r = 0; r < rng; r++) raw.set(Math.floor((row.max_hp * (base + r)) / 512), (raw.get(Math.floor((row.max_hp * (base + r)) / 512)) ?? 0) + 1 / rng)
    } else if (d.mode === 'maxhp_div') raw = new Map([[Math.trunc(row.max_hp / Math.max(1, d.divisor ?? 3)), 1]])
    else if (d.mode === 'ki_spent') raw = new Map([[kiSpent(row.max_ki, row.max_ki, held).spent, 1]])
    else if (d.mode === 'cur_hp') raw = enemyMeleeRaw(row.max_hp)
    else raw = new Map([[d.fixed ?? 0, 1]])
    dist = applyDefense(raw, move.user === 'enemy' ? PLAYER_DEFENSE : (e.calc.def ?? 0))
    info = (
      <>
        Goku Lv{lv}: max HP {fmt(row.max_hp)}, max Ki {fmt(row.max_ki)} (charge {chargeCost(row.max_ki)} Ki / 8 f) · vs {e.name} DEF {e.calc.def}
      </>
    )
  } else if (game === 'log2') {
    const c = chars.characters[ci]
    const k = move.attack.kind === 'melee' ? 'STR' : 'POW'
    const S = Math.floor(L2.statAt(c.initial[`${k}_8p8`], [c.growth_per_level[k].min_8p8, c.growth_per_level[k].max_8p8], lv).avg)
    const r = L2.damageDist({
      type: move.attack.kind === 'melee' ? 0 : 1,
      power: move.attack.power,
      scale: move.attack.scale,
      S,
      DEF: e.calc.END ?? 0,
      MULT: move.attack.kind === 'melee' ? (e.calc.meleeMult ?? 128) : (e.calc.energyMult ?? 128),
    })
    dist = r.dist
    if (!move.attack.canCrit && r.pCrit > 0) dist = L2.damageDist({ type: 1, power: move.attack.power, scale: move.attack.scale, S, DEF: e.calc.END ?? 0, MULT: e.calc.meleeMult ?? 128 }).dist
    info = (
      <>
        {c.name} Lv{lv} {k} {S} · vs {e.name} END {e.calc.END} · crit {move.attack.canCrit ? (L2.critChance(S) * 100).toFixed(1) + '%' : 'off'}
      </>
    )
  } else {
    const c = chars.characters[ci]
    const S = (move.attack.kind === 'melee' ? c.new_game.STR : c.new_game.POW) + lv
    const r = BU.damageDist({
      kind: move.attack.kind === 'melee' ? 0 : 1,
      sub: move.attack.canCrit ? move.attack.attackId : 3,
      power: move.attack.power,
      mult: move.attack.scale,
      S,
      D: e.calc.END ?? 0,
      resist: move.attack.kind === 'melee' ? (e.calc.resistMelee ?? 128) : (e.calc.resistEnergy ?? 128),
    })
    dist = r.dist
    unitHp = e.hp
    info = (
      <>
        {c.name.split(' (')[0]} {move.attack.kind === 'melee' ? 'STR' : 'POW'} {S} · vs {e.name} END {e.calc.END}
      </>
    )
  }
  const hits = Math.max(1, move.timing.hits)
  const perUse = mapDist(dist, (v) => v * hits)
  const ms = stats(perUse)
  const frames = move.timing.startupFrames + move.timing.activeFrames + move.timing.recoveryFrames
  return (
    <Card
      title="Damage vs an enemy"
      actions={
        <div className="row">
          {game !== 'log1' && (
            <select value={ci} onChange={(ev) => setCi(+ev.target.value)}>
              {chars.characters.map((x: J, i: number) => (
                <option key={i} value={i}>
                  {x.name.split(' (')[0]}
                </option>
              ))}
            </select>
          )}
          {game === 'buu' ? (
            <Slider label="+stat pts" min={0} max={200} value={lv} onChange={setLv} />
          ) : (
            <Slider label="Lv" min={1} max={game === 'log1' ? 25 : 50} value={lv} onChange={setLv} />
          )}
        </div>
      }
    >
      <div className="toolbar">
        <label className="field">
          <span>Target</span>
          <select value={ei} onChange={(ev) => setEi(+ev.target.value)}>
            {enemies.map((x, i) => (
              <option key={x.key} value={i}>
                {x.name} ({x.idLabel}, HP {fmt(x.hp)})
              </option>
            ))}
          </select>
        </label>
        {game === 'log1' && move.attack.log1?.mode === 'ki_spent' && <Slider label="frames held" min={0} max={600} value={held} onChange={setHeld} />}
      </div>
      <div className="muted small" style={{ marginBottom: 6 }}>
        {info}
      </div>
      <DistView dist={dist} hp={unitHp} digits={game === 'buu' ? 2 : 0} />
      <div className="stats" style={{ marginTop: 8 }}>
        <Stat label={`Per use (${hits} hit${hits > 1 ? 's' : ''})`} value={fmt(+ms.mean.toFixed(1))} />
        <Stat label="Frames per use" value={frames} sub="startup + active + recovery" />
        <Stat label="Damage / second" value={frames > 0 ? fmt(+((ms.mean * 60) / frames).toFixed(1)) : '—'} sub="if spammed" />
        <Stat label="Uses to kill" value={ms.mean > 0 ? Math.ceil(unitHp / ms.mean) : '∞'} />
      </div>
    </Card>
  )
}
