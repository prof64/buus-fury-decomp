import { useMemo, useState } from 'react'
import { useData } from '../../../data'
import { useSprites } from '../../../lib/sprites'
import { buuEnemies } from '../../../model/enemies'
import { LineChart } from '../../../components/Chart'
import { KV, Loading, NumField, Seg, Slider, Stat } from '../../../components/ui'
import * as BU from '../../../formulas/buu'
import { fmt, pct, range, type J } from '../../../lib/util'
import { Code, DistView, Section, SideNav } from './common'

const SECTIONS = [
  { id: 'damage', title: 'Damage formula' },
  { id: 'calc', title: 'Damage calculator' },
  { id: 'curves', title: 'Attack & defense curves' },
  { id: 'crit', title: 'Critical hits' },
  { id: 'xp', title: 'XP & levels' },
  { id: 'levelup', title: 'Level-up growth' },
  { id: 'fusion', title: 'Fusion' },
  { id: 'move', title: 'Movement speed' },
  { id: 'ss', title: 'Super Saiyan gauge' },
  { id: 'regen', title: 'Regen' },
  { id: 'rng', title: 'RNG' },
]

export function BuuFormulas() {
  const f = useData<J>('buu', 'formulas')
  const chars = useData<J>('buu', 'characters')
  const moves = useData<J>('buu', 'moves')
  const enemiesJ = useData<J>('buu', 'enemies')
  const levels = useData<J>('buu', 'levels')
  const sp = useSprites('buu')
  const enemies = useMemo(() => (enemiesJ ? buuEnemies(enemiesJ, sp).filter((e) => e.used) : []), [enemiesJ, sp])
  if (!f || !chars || !moves || !enemiesJ || !levels) return <Loading what="formulas" />
  return (
    <div className="with-side">
      <SideNav sections={SECTIONS} />
      <div className="stack">
        <Section id="damage" title="Damage formula — Character::ComputeDamage" addr="0x0800E9D8" evidence={f.damage.status}>
          <p>
            Buu’s Fury replaced LoG II’s formula. Damage is computed in 1/1024 HP: melee power is scaled by STR/20 and can crit for ×1.5;
            defense subtracts linearly from 221 (END ≥ 200 is compressed to at most 220); an attack factor A grows with the cube of the stat.
          </p>
          <Code>{f.damage.steps.join('\n')}</Code>
          <KV rows={Object.entries<string>(f.damage.inputs).map(([k, v]) => [k, v])} />
        </Section>
        <Calc chars={chars} moves={moves} enemies={enemies} />
        <Section id="curves" title="Attack & defense curves">
          <div className="grid2">
            <LineChart
              title="Attack factor A(S) = S + S²(S+100) >> 13"
              height={200}
              xLabel="STR / POW"
              series={[{ name: 'A(S)', points: range(256).map((s) => [s, BU.attackFactor(s)]) }]}
            />
            <LineChart
              title="Defense factor (221 − D′), D′ = clamped END"
              height={200}
              xLabel="END"
              series={[{ name: '221 − D′', points: range(301).map((d) => [d, 221 - BU.effectiveDef(d)]) }]}
            />
          </div>
          <p className="note">END above 200 is compressed: D′ = min(D/4 + 150, 220), so defense can never reduce damage to zero.</p>
        </Section>
        <Section id="crit" title="Critical hits" evidence={f.crit.status}>
          <Code>{`${f.crit.chance_per_melee_hit}
crit bonus = equipment crit % × 656     (Charge Gloves 5 %, Shock Boots 10 %, Soccer Cleats 20 %)
effect: ${f.crit.effect}; never for attack sub 3`}</Code>
          <LineChart
            height={180}
            xLabel="STR"
            yLabel="%"
            series={[0, 5, 10, 20].map((b) => ({ name: b ? `+${b}% equip` : 'no equipment', points: range(256).map((s) => [s, +(Math.min(1, (82 * s + b * 656) / 65536) * 100).toFixed(2)] as [number, number]) }))}
          />
        </Section>
        <XpSection levels={levels} f={f} />
        <Section id="levelup" title="Level-up growth" addr="0x08013678" evidence={f.level_up.status}>
          <Code>{f.level_up.per_level.join('\n')}</Code>
          <div className="plain-table">
            <table>
              <thead>
                <tr>
                  <th>character</th>
                  <th>new game</th>
                  <th>HP + per level</th>
                  <th>EP + per level</th>
                  <th>growth row (g0..g7)</th>
                </tr>
              </thead>
              <tbody>
                {chars.characters.map((c: J) => (
                  <tr key={c.index}>
                    <td>{c.name.split(' (')[0]}</td>
                    <td className="mono small">
                      Lv{c.new_game.level} HP {c.new_game.hp} EP {c.new_game.ep}
                    </td>
                    {c.growth.hp_add_range ? (
                      <>
                        <td className="mono small">
                          {fmt(c.growth.hp_add_range[0], 1)}–{fmt(c.growth.hp_add_range[1], 1)} + {pct(c.growth.hp_pct_range[0], 2)}–{pct(c.growth.hp_pct_range[1], 2)}
                        </td>
                        <td className="mono small">
                          {fmt(c.growth.ep_add_range[0], 1)}–{fmt(c.growth.ep_add_range[1], 1)} + {pct(c.growth.ep_pct_range[0], 2)}–{pct(c.growth.ep_pct_range[1], 2)}
                        </td>
                      </>
                    ) : (
                      <td colSpan={2} className="wrap small muted">
                        {c.growth.note ?? 'no own growth row'}
                      </td>
                    )}
                    <td className="mono small">{c.growth.raw_u32.join(', ')}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <LineChart
            xLabel="level"
            yLabel="max HP (typical rolls)"
            series={chars.characters.slice(0, 5).map((c: J) => ({
              name: c.name,
              points: BU.growthCurve(c.new_game.level, c.new_game.hp, c.new_game.ep, c.growth.raw_u32).map((r) => [r.level, r.hp[1]] as [number, number]),
            }))}
          />
        </Section>
        <FusionSection chars={chars} f={f} />
        <Section id="move" title="Movement speed" addr="0x0800E074" evidence={f.movement.status}>
          <Code>{`walk = 5 × SPD + 102     (1/256 px per frame)
dash = walk + 320          (double-tap within 20 frames)
Super Saiyan +128, SS3 +192`}</Code>
          <LineChart
            height={200}
            xLabel="SPD"
            yLabel="px / frame"
            series={[
              { name: 'walk', points: range(256).map((s) => [s, +(BU.walkSpeed(s) / 256).toFixed(3)]) },
              { name: 'dash', points: range(256).map((s) => [s, +(BU.dashSpeed(s) / 256).toFixed(3)]) },
            ]}
          />
        </Section>
        <SsSection f={f} />
        <Section id="regen" title="Regen" addr="0x0800E1D0" evidence={f.regen.status}>
          <KV rows={Object.entries<string>(f.regen).filter(([k]) => k !== 'status').map(([k, v]) => [k.replace(/_/g, ' '), v])} />
        </Section>
        <Section id="rng" title="RNG" addr="0x0803FAAC" evidence="VERIFIED">
          <Code>{`a = (s0 + 0x02CBBC71) ^ s1
b = (rol32(s1, 3) + 0x0632D80F) ^ a
s0, s1 = a, b;  return a − b
rand(n)   = ((next() & 0xFFFF) × n) >> 16        # 0x0803FACC
chance(p) = (next() & 0xFFFF) < p                # 0x0803FADE`}</Code>
          <p className="note">Same generator as LoG II (same constants). It reproduces damage rolls, level-ups and loot exactly.</p>
        </Section>
      </div>
    </div>
  )
}

function Calc({ chars, moves, enemies }: { chars: J; moves: J; enemies: ReturnType<typeof buuEnemies> }) {
  const [dir, setDir] = useState<'p2e' | 'e2p'>('p2e')
  const [ci, setCi] = useState(0)
  const [formSel, setFormSel] = useState<number | null>(null)
  const [pts, setPts] = useState(0)
  const [critEq, setCritEq] = useState(0)
  const [ei, setEi] = useState(Math.max(0, enemies.findIndex((e) => /majin fighter/i.test(e.name))))
  const [skill, setSkill] = useState('melee')
  const [lvl, setLvl] = useState(0)
  const [block, setBlock] = useState(false)
  const [ovr, setOvr] = useState<{ mult?: number; S?: number; D?: number }>({})
  const c = chars.characters[ci]
  const form = chars.forms[formSel !== null && c.forms.includes(formSel) ? formSel : c.new_game.form]
  const b = form.measured_bonus
  const e = enemies[ei]
  const types: J[] = moves.attack_types
  const sk = moves.skills.find((s: J) => 's' + s.id === skill)
  const lvData = sk?.measured?.by_level?.[String(lvl)] ?? sk?.measured?.by_level?.['0']
  const sub: number = skill === 'melee' ? 2 : (lvData?.attack_type_subs?.[0]?.[1] ?? 1)
  const at = types.find((t) => t.sub === sub) ?? types[1]
  const kind: 0 | 1 = at.kind === 0 ? 0 : 1
  const multDefault = skill === 'melee' ? 1024 : (lvData?.damage_multipliers?.[0] ?? 1024)
  const STR = c.new_game.STR + pts + b.STR
  const POW = c.new_game.POW + pts + b.POW
  const END = c.new_game.END + b.END
  const S = ovr.S ?? (dir === 'p2e' ? (kind === 0 ? STR : POW) : kind === 0 ? (e?.calc.STR ?? 1) : (e?.calc.POW ?? 1))
  const D = ovr.D ?? (dir === 'p2e' ? (e?.calc.END ?? 0) : END)
  const resist = dir === 'p2e' ? (kind === 0 ? (e?.calc.resistMelee ?? 128) : (e?.calc.resistEnergy ?? 128)) : 128
  const mult = ovr.mult ?? (dir === 'p2e' ? multDefault : 1024)
  const res = BU.damageDist({
    kind,
    sub: dir === 'p2e' ? sub : 2,
    power: at.power,
    mult,
    S,
    D,
    resist,
    critBonus: dir === 'p2e' ? critEq * 656 : 0,
    blockFactor: dir === 'e2p' && block ? 0x1000 : undefined,
  })
  const levelsAvail = sk?.measured?.by_level ? Object.keys(sk.measured.by_level) : ['0']
  return (
    <Section id="calc" title="Damage calculator">
      <div className="toolbar">
        <Seg value={dir} onChange={(v) => (setDir(v), setOvr({}))} options={[{ value: 'p2e', label: 'Player → enemy' }, { value: 'e2p', label: 'Enemy punch → player' }]} />
        <label className="field">
          <span>Character</span>
          <select value={ci} onChange={(ev) => (setCi(+ev.target.value), setFormSel(null), setOvr({}))}>
            {chars.characters.map((x: J, i: number) => (
              <option key={i} value={i}>
                {x.name.split(' (')[0]}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>Form</span>
          <select value={form.index} onChange={(ev) => (setFormSel(+ev.target.value), setOvr({}))}>
            {c.forms.map((fid: number) => (
              <option key={fid} value={fid}>
                {fid}: {chars.forms[fid].name}
                {chars.forms[fid].measured_bonus.STR ? ` +${chars.forms[fid].measured_bonus.STR}` : ''}
              </option>
            ))}
          </select>
        </label>
        <Slider label="+ stat points in STR & POW" min={0} max={300} value={pts} onChange={(v) => (setPts(v), setOvr({}))} />
        <label className="field">
          <span>Enemy</span>
          <select value={ei} onChange={(ev) => (setEi(+ev.target.value), setOvr({}))}>
            {enemies.map((x, i) => (
              <option key={x.key} value={i}>
                {x.name} (HP {fmt(x.hp)}, END {x.calc.END})
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="toolbar">
        {dir === 'p2e' && (
          <>
            <label className="field">
              <span>Attack</span>
              <select value={skill} onChange={(ev) => (setSkill(ev.target.value), setLvl(0), setOvr({}))}>
                <option value="melee">Punch (melee sub 2)</option>
                {moves.skills
                  .filter((s: J) => s.measured?.by_level?.['0']?.damage_multipliers?.length)
                  .map((s: J) => (
                    <option key={s.id} value={'s' + s.id}>
                      {s.name}
                    </option>
                  ))}
              </select>
            </label>
            {skill !== 'melee' && levelsAvail.length > 1 && (
              <Seg value={String(lvl)} onChange={(v) => (setLvl(+v), setOvr({}))} options={levelsAvail.map((l) => ({ value: l, label: `Lv${+l + 1}` }))} />
            )}
            <label className="field">
              <span>Crit equipment</span>
              <select value={critEq} onChange={(ev) => setCritEq(+ev.target.value)}>
                <option value={0}>none</option>
                <option value={5}>Charge Gloves 5%</option>
                <option value={10}>Shock Boots 10%</option>
                <option value={20}>Soccer Cleats 20%</option>
              </select>
            </label>
          </>
        )}
        {dir === 'e2p' && (
          <label className="field">
            <span>&nbsp;</span>
            <span>
              <input type="checkbox" checked={block} onChange={(ev) => setBlock(ev.target.checked)} /> blocking (front)
            </span>
          </label>
        )}
        <NumField label="hit mult (1024 = ×1)" value={mult} onChange={(v) => setOvr({ ...ovr, mult: v })} min={0} />
        <NumField label={kind === 0 ? 'attacker STR' : 'attacker POW'} value={S} onChange={(v) => setOvr({ ...ovr, S: v })} min={0} max={255} />
        <NumField label="defender END" value={D} onChange={(v) => setOvr({ ...ovr, D: v })} min={0} max={255} />
      </div>
      <div className="muted small" style={{ marginBottom: 8 }}>
        attack type sub {dir === 'p2e' ? sub : 2} ({at.kind_name}, power {at.power}) · resist {resist}/128
        {skill !== 'melee' && lvData?.damage_multipliers?.length > 1 && <> · this skill uses several multipliers: {lvData.damage_multipliers.join(', ')}</>}
      </div>
      <DistView
        dist={res.dist}
        hp={dir === 'p2e' ? e?.hp : undefined}
        digits={2}
        extra={kind === 0 ? <Stat label="Crit" value={pct(res.pCrit)} sub={`${fmt(res.crit[0], 2)}–${fmt(res.crit[1], 2)}`} /> : undefined}
      />
    </Section>
  )
}

function XpSection({ levels, f }: { levels: J; f: J }) {
  const [exp, setExp] = useState(1225)
  const [mult, setMult] = useState(256)
  return (
    <Section id="xp" title="XP & levels" addr={f.xp.function_enemy} evidence={f.xp.status}>
      <Code>{`${f.xp.enemy_kill}
${f.xp.level}`}</Code>
      <div className="toolbar">
        <NumField label="Enemy exp" value={exp} onChange={setExp} min={0} width={110} />
        <NumField label="XP multiplier /256" value={mult} onChange={setMult} min={0} />
        <Stat label="XP gained" value={fmt(BU.xpGain(exp, mult))} />
      </div>
      <LineChart
        logY
        xLabel="level"
        series={[
          { name: 'total XP for level', points: levels.levels.map((l: J) => [l.level, Math.max(1, l.total_xp)]) },
          { name: 'XP to next', points: levels.levels.filter((l: J) => l.to_next).map((l: J) => [l.level, l.to_next]), dashed: true },
        ]}
      />
    </Section>
  )
}

function FusionSection({ chars, f }: { chars: J; f: J }) {
  const [a, setA] = useState(2)
  const [b, setB] = useState(3)
  const rec = (i: number) => {
    const ng = chars.characters[i].new_game
    return { hp: ng.hp, ep: ng.ep, STR: ng.STR, POW: ng.POW, END: ng.END, SPD: ng.SPD, xp: ng.xp }
  }
  const r = BU.fusion(rec(a), rec(b))
  return (
    <Section id="fusion" title="Fusion" addr={f.fusion.function} evidence={f.fusion.status}>
      <Code>{`HP, maxHP, EP, maxEP = (A + B) × 154 >> 8      # ≈ ×1.2 of the sum
STR, POW, END = (A + B) / 2 + 10
SPD = (A + B) / 2 + 16;   XP = (A + B) / 2`}</Code>
      <div className="toolbar">
        {[
          [a, setA],
          [b, setB],
        ].map(([v, set], k) => (
          <label key={k} className="field">
            <span>Component {k ? 'B' : 'A'}</span>
            <select value={v as number} onChange={(e) => (set as (n: number) => void)(+e.target.value)}>
              {chars.characters.slice(0, 5).map((c: J, i: number) => (
                <option key={i} value={i}>
                  {c.name} (new game)
                </option>
              ))}
            </select>
          </label>
        ))}
      </div>
      <div className="stats">
        {Object.entries(r).map(([k, v]) => (
          <Stat key={k} label={k} value={fmt(v)} />
        ))}
      </div>
    </Section>
  )
}

function SsSection({ f }: { f: J }) {
  const [maxEp, setMaxEp] = useState(300)
  const ss3PerFrame = (22 * maxEp + 0x3333) / 65536
  return (
    <Section id="ss" title="Super Saiyan gauge" addr="0x0800E3E6" evidence={f.forms.status}>
      <KV rows={Object.entries<string>(f.forms).filter(([k, v]) => typeof v === 'string' && k !== 'status').map(([k, v]) => [k.replace(/_/g, ' '), v])} />
      <Code>{`SS gauge:  fills to maxEP during the transform; acc += 0xCCD per frame → −1 every 20 frames → lasts maxEP × 20 frames
SS3 gauge: drains (22·maxEP + 0x3333) / 65536 per frame
Energy moves spend the gauge instead of EP while transformed.`}</Code>
      <div className="toolbar">
        <NumField label="max EP" value={maxEp} onChange={setMaxEp} min={1} />
      </div>
      <div className="stats">
        <Stat label="SS duration" value={`${fmt(maxEp * 20)} f`} sub={`${fmt((maxEp * 20) / 60, 1)} s if no moves used`} />
        <Stat label="SS3 drain" value={`${ss3PerFrame.toFixed(3)} /f`} />
        <Stat label="SS3 duration" value={`${fmt(maxEp / ss3PerFrame, 0)} f`} sub={`${fmt(maxEp / ss3PerFrame / 60, 1)} s`} />
      </div>
    </Section>
  )
}
