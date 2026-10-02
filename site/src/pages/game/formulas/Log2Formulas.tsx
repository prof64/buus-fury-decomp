import { useMemo, useState } from 'react'
import { useData } from '../../../data'
import { useSprites } from '../../../lib/sprites'
import { log2Enemies } from '../../../model/enemies'
import { LineChart } from '../../../components/Chart'
import { KV, Loading, NumField, Seg, Slider, Stat } from '../../../components/ui'
import * as L2 from '../../../formulas/log2'
import { fmt, pct, range, type J } from '../../../lib/util'
import { Code, DistView, Section, SideNav } from './common'

const SECTIONS = [
  { id: 'damage', title: 'Damage formula' },
  { id: 'calc', title: 'Damage calculator' },
  { id: 'fs', title: 'Attack curve f(S)' },
  { id: 'crit', title: 'Critical hits' },
  { id: 'exp', title: 'Experience award' },
  { id: 'levelup', title: 'Level-up growth' },
  { id: 'regen', title: 'Regen & forms' },
  { id: 'knock', title: 'Knockback & hitstun' },
  { id: 'rng', title: 'RNG' },
]

export function Log2Formulas() {
  const f = useData<J>('log2', 'formulas')
  const chars = useData<J>('log2', 'characters')
  const moves = useData<J>('log2', 'moves')
  const enemiesJ = useData<J>('log2', 'enemies')
  const levels = useData<J>('log2', 'levels')
  const sp = useSprites('log2')
  const enemies = useMemo(() => (enemiesJ ? log2Enemies(enemiesJ, sp).filter((e) => e.used) : []), [enemiesJ, sp])
  if (!f || !chars || !moves || !enemiesJ || !levels) return <Loading what="formulas" />
  const expTable: number[] = levels.levels.map((l: J) => l.total_exp)
  return (
    <div className="with-side">
      <SideNav sections={SECTIONS} />
      <div className="stack">
        <Section id="damage" title="Damage formula — EntityBase::computeDamage" addr="0x0800F910" evidence={f.damage.status}>
          <p>
            One routine computes every hit in the game, player or enemy. Melee uses the attacker’s <b>STR</b> and can crit; energy uses{' '}
            <b>POW</b>. The defender’s <b>END</b> removes END/128 of the power, a cubic curve of the attack stat scales it, a +0–9.8% random
            factor spreads it, and the hit’s <i>scale</i> (charge level) and the defender’s damage-taken multiplier finish it.
          </p>
          <Code>{`crit   = melee ? chance(0xA4 × STR) : 0        # P = STR × 0.25 %
b      = (power × (1 + crit) × (128 − END)) >> 7
f(S)   = (((26S − 701) × S + 115618) × S) >> 8
r      = rand(26) + 256                        # 256..281
t      = (((r × b) >> 8) × f(S)) >> 18          # unsigned 32-bit products
t      = (scale × t) >> 10                     # scale 1024 = ×1.0
damage = (t × MULT) >> 7                       # MULT = defender melee/energy taken (128 = ×1.0)`}</Code>
          <KV rows={Object.entries<string>(f.damage.inputs).map(([k, v]) => [k, v])} />
        </Section>
        <Calc chars={chars} moves={moves} enemies={enemies} />
        <Section id="fs" title="Attack curve f(S)">
          <p>Damage grows roughly with the cube of the attack stat: at power 0x400, no defense and scale 1.0 a hit does f(S)/256.</p>
          <LineChart
            xLabel="attack stat S (STR or POW)"
            yLabel="damage"
            series={[{ name: 'damage at power 1024, END 0', points: range(101).map((s) => [s, +(L2.fS(s) / 256).toFixed(2)]) }]}
          />
          <p className="note">S = 10 → 17 · S = 30 → 54 · S = 60 → 153 · S = 100 → 466 (before the random factor).</p>
        </Section>
        <Section id="crit" title="Critical hits" evidence={f.crit.status}>
          <p>Only melee crits. The chance is 164·STR out of 65536 (0.25% per STR point, 25% at STR 100); a crit doubles power before defense.</p>
          <LineChart height={180} xLabel="STR" yLabel="%" series={[{ name: 'crit chance %', points: range(101).map((s) => [s, +(L2.critChance(s) * 100).toFixed(2)]) }]} />
        </Section>
        <ExpSection expTable={expTable} f={f} />
        <Section id="levelup" title="Level-up growth" addr={f.level_up.function} evidence={f.level_up.status}>
          <Code>{`for STR, POW, END (8.8 fixed):  stat += min·n + rand((max − min + 1)·n),  cap 0x6400 (100.0)
per level gained:  maxHP += maxHP × (rand(0x51F) + 0xE14) >> 16    # +5.50 % .. +7.50 %
                   maxEP += rand(3) + 2                             # +2 .. +4
                   HP = maxHP; EP = maxEP`}</Code>
          <div className="plain-table">
            <table>
              <thead>
                <tr>
                  <th>character</th>
                  <th>STR / level</th>
                  <th>POW / level</th>
                  <th>END / level</th>
                  <th>start HP/EP/STR/POW/END</th>
                </tr>
              </thead>
              <tbody>
                {chars.characters.map((c: J) => (
                  <tr key={c.index}>
                    <td>{c.name}</td>
                    {(['STR', 'POW', 'END'] as const).map((k) => (
                      <td key={k} className="mono">
                        {c.growth_per_level[k].min.toFixed(2)}–{c.growth_per_level[k].max.toFixed(2)}
                      </td>
                    ))}
                    <td className="mono">
                      {c.initial.max_hp}/{c.initial.max_ep}/{c.initial.STR}/{c.initial.POW}/{c.initial.END}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <LineChart
            xLabel="level"
            yLabel="avg STR"
            series={chars.characters.map((c: J) => ({
              name: c.name,
              points: range(50, 1).map((L) => [L, +L2.statAt(c.initial.STR_8p8, [c.growth_per_level.STR.min_8p8, c.growth_per_level.STR.max_8p8], L).avg.toFixed(1)] as [number, number]),
            }))}
          />
        </Section>
        <Section id="regen" title="Regen & form controllers" addr="0x08017028" evidence="VERIFIED (normal mode)">
          <Code>{`acc += 110 every frame;  on 16-bit overflow (every 595.8 f ≈ 9.9 s):
   active.EP += maxEP × 0x7AE >> 16      # 3 %
   every benched character: HP, EP += max × 0xCCD >> 16   # 5 %
No HP regen for the active character in normal form.`}</Code>
          <div className="plain-table">
            <table>
              <thead>
                <tr>
                  <th>controller</th>
                  <th>stats</th>
                  <th>walk / dash px/f</th>
                  <th>per-frame tick</th>
                </tr>
              </thead>
              <tbody>
                {Object.values<J>(chars.form_controllers).map((c) => (
                  <tr key={c.vtable}>
                    <td>{c.name}</td>
                    <td className="mono">{c.stat_override ? `= ${c.stat_override.join('/')}` : `+${(c.bonus ?? [0, 0, 0]).join('/+')}`}</td>
                    <td className="mono">
                      {c.walk_speed} / {c.run_speed}
                    </td>
                    <td className="wrap small">{c.tick}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Section>
        <Section id="knock" title="Knockback & hitstun" addr="0x0801A947">
          <Code>{`enemy knockback:  k = (rand(64) + 96) × p >> 7      # p = node param (16 normal, 32 fighters, 24 Cell)
                  velocity = 8·k (1/256 px/f) along the hit direction, + perpendicular jitter
player hurt:      state 0x20 for 32 frames, pushed 0.75 px/f opposite to own facing, damage multipliers 0`}</Code>
          <KV
            rows={[
              ['Wall behind enemy', 'KnockbackInvulnerable(40/48): unhittable until the hurt animation ends'],
              ['Attack id 9 (Burning Attack)', 'stun 128 f + knockback 16'],
              ['Attack id 15 (Spirit Bomb ≥128, Hercule)', 'stun 256 f (324 f measured on snakes)'],
              ['Android 19/20', 'energy hits heal 20% of raw damage instead of damaging'],
            ]}
          />
        </Section>
        <Section id="rng" title="RNG" addr="0x080213FC" evidence={f.rng.status}>
          <Code>{`${f.rng.next}
${f.rng['rand(n)']}
${f.rng['chance(p)']}`}</Code>
          <p className="note">The state starts at 0,0; only the title screen advances it every frame, so the seed depends on how long you wait there.</p>
        </Section>
      </div>
    </div>
  )
}

function ExpSection({ expTable, f }: { expTable: number[]; f: J }) {
  const [lv, setLv] = useState(20)
  const [exp, setExp] = useState(500)
  const span = lv < 50 ? expTable[lv] - expTable[lv - 1] : 0
  return (
    <Section id="exp" title="Experience award" addr="0x0800E4D2" evidence={f.exp_award.status}>
      <p>
        Kill exp is clamped to between 1/128 and 1/2 of the exp needed for your current level, so weak enemies stay worth something and
        strong ones can’t give more than half a level. Only the active character gains exp.
      </p>
      <Code>{`E = class.exp
if E > 5 and level < 50:
    span = EXP[L] − EXP[L−1]
    E = clamp(E, span >> 7, span >> 1)
level 50 → no exp`}</Code>
      <div className="toolbar">
        <Slider label="Your level" min={1} max={50} value={lv} onChange={setLv} />
        <NumField label="Enemy exp" value={exp} onChange={setExp} min={0} width={110} />
      </div>
      <div className="stats">
        <Stat label="Level span" value={fmt(span)} />
        <Stat label="Floor (span/128)" value={fmt(span >> 7)} />
        <Stat label="Ceiling (span/2)" value={fmt(span >> 1)} />
        <Stat label="You get" value={fmt(L2.killExp(exp, lv, expTable))} />
      </div>
      <LineChart
        logY
        xLabel="level"
        series={[
          { name: 'total exp for level', points: expTable.map((v, i) => [i + 1, Math.max(1, v)]) },
          { name: 'max per kill (span/2)', points: expTable.slice(0, 49).map((v, i) => [i + 1, Math.max(1, (expTable[i + 1] - v) >> 1)]), dashed: true },
          { name: 'min per kill (span/128)', points: expTable.slice(0, 49).map((v, i) => [i + 1, Math.max(1, (expTable[i + 1] - v) >> 7)]), dashed: true },
        ]}
        marker={lv}
      />
    </Section>
  )
}

function Calc({ chars, moves, enemies }: { chars: J; moves: J; enemies: ReturnType<typeof log2Enemies> }) {
  const [dir, setDir] = useState<'p2e' | 'e2p'>('p2e')
  const [ci, setCi] = useState(4)
  const [lv, setLv] = useState(30)
  const [formSel, setFormSel] = useState<number | null>(null)
  const [ei, setEi] = useState(Math.max(0, enemies.findIndex((e) => e.key === '17')))
  const [atk, setAtk] = useState(2)
  const [scale, setScale] = useState(1024)
  const [ovr, setOvr] = useState<{ S?: number; DEF?: number; MULT?: number }>({})
  const c = chars.characters[ci]
  const form = chars.forms[formSel !== null && c.forms.includes(formSel) ? formSel : c.initial.form]
  const ctl = chars.form_controllers[form.controller.ram]
  const stat = (k: 'STR' | 'POW' | 'END', i: number) => {
    const base = Math.floor(L2.statAt(c.initial[`${k}_8p8`], [c.growth_per_level[k].min_8p8, c.growth_per_level[k].max_8p8], lv).avg)
    return ctl.stat_override ? ctl.stat_override[i] : base + (ctl.bonus?.[i] ?? 0)
  }
  const e = enemies[ei]
  const entry = moves.attack_table.find((a: J) => a.id === atk)
  const type: 0 | 1 = entry?.type === 0 ? 0 : 1
  const computed =
    dir === 'p2e'
      ? { S: type === 0 ? stat('STR', 0) : stat('POW', 1), DEF: e?.calc.END ?? 0, MULT: type === 0 ? (e?.calc.meleeMult ?? 128) : (e?.calc.energyMult ?? 128) }
      : { S: type === 0 ? (e?.calc.STR ?? 1) : (e?.calc.POW ?? 1), DEF: stat('END', 2), MULT: 128 }
  const S = ovr.S ?? computed.S
  const DEF = ovr.DEF ?? computed.DEF
  const MULT = ovr.MULT ?? computed.MULT
  const res = L2.damageDist({ type, power: entry?.power ?? 1024, scale, S, DEF, MULT })
  const hpPlayer = L2.maxHpAt(c.initial.max_hp, lv).avg
  return (
    <Section id="calc" title="Damage calculator">
      <div className="toolbar">
        <Seg value={dir} onChange={(v) => (setDir(v), setOvr({}))} options={[{ value: 'p2e', label: 'Party → enemy' }, { value: 'e2p', label: 'Enemy → party' }]} />
        <label className="field">
          <span>Character</span>
          <select value={ci} onChange={(ev) => (setCi(+ev.target.value), setFormSel(null), setOvr({}))}>
            {chars.characters.map((x: J, i: number) => (
              <option key={i} value={i}>
                {x.name}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>Form</span>
          <select value={form.id} onChange={(ev) => (setFormSel(+ev.target.value), setOvr({}))}>
            {c.forms.map((fid: number) => (
              <option key={fid} value={fid}>
                {chars.forms[fid].label}
              </option>
            ))}
          </select>
        </label>
        <Slider label="Level" min={1} max={50} value={lv} onChange={(v) => (setLv(v), setOvr({}))} />
        <label className="field">
          <span>Enemy class</span>
          <select value={ei} onChange={(ev) => (setEi(+ev.target.value), setOvr({}))}>
            {enemies.map((x, i) => (
              <option key={x.key} value={i}>
                {x.name} · {x.idLabel} (HP {x.hp})
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="toolbar">
        <label className="field">
          <span>Attack-table entry</span>
          <select value={atk} onChange={(ev) => (setAtk(+ev.target.value), setOvr({}))}>
            {moves.attack_table.map((a: J) => (
              <option key={a.id} value={a.id}>
                #{a.id} {a.type === 0 ? 'melee' : 'energy'} {a.power} — {a.used_by.split(/[;(]/)[0].slice(0, 48)}
              </option>
            ))}
          </select>
        </label>
        <NumField label="scale (1024 = ×1)" value={scale} onChange={setScale} min={0} />
        <NumField label={type === 0 ? 'attacker STR' : 'attacker POW'} value={S} onChange={(v) => setOvr({ ...ovr, S: v })} min={0} max={255} />
        <NumField label="defender END" value={DEF} onChange={(v) => setOvr({ ...ovr, DEF: v })} min={0} max={127} />
        <NumField label="defender mult /128" value={MULT} onChange={(v) => setOvr({ ...ovr, MULT: v })} min={0} max={255} />
      </div>
      <div className="muted small" style={{ marginBottom: 8 }}>
        Party stats use average growth to Lv{lv} plus the form controller ({ctl.name}). Edit any number to override.
      </div>
      <DistView
        dist={res.dist}
        hp={dir === 'p2e' ? e?.hp : hpPlayer}
        extra={
          type === 0 ? (
            <Stat label="Crit" value={pct(res.pCrit)} sub={`${res.crit[0]}–${res.crit[1]}`} />
          ) : (
            <Stat label="Range" value={`${res.normal[0]}–${res.normal[1]}`} sub="energy never crits" />
          )
        }
      />
    </Section>
  )
}
