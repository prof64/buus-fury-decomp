import { useMemo, useState } from 'react'
import { useData } from '../../../data'
import { useSprites } from '../../../lib/sprites'
import { log1Enemies } from '../../../model/enemies'
import { BarList, Histogram, LineChart } from '../../../components/Chart'
import { KV, Loading, NumField, Seg, Slider, Stat } from '../../../components/ui'
import { histogram, stats, type Dist } from '../../../formulas/dist'
import * as L1 from '../../../formulas/log1'
import { fmt, pct, range, type J } from '../../../lib/util'
import { Code, DistView, Section, SideNav } from './common'

const SECTIONS = [
  { id: 'pipeline', title: 'Damage pipeline' },
  { id: 'calc', title: 'Damage calculator' },
  { id: 'defense', title: 'Defense roll' },
  { id: 'punch', title: 'Punch' },
  { id: 'ki', title: 'Ki charge & Kamehameha' },
  { id: 'solar', title: 'Solar Flare' },
  { id: 'enemy', title: 'Enemy melee & rand bug' },
  { id: 'exp', title: 'Experience & levels' },
  { id: 'regen', title: 'Regen, speed, wake radius' },
  { id: 'rng', title: 'RNG' },
]

export function Log1Formulas() {
  const f = useData<J>('log1', 'formulas')
  const levels = useData<J>('log1', 'levels')
  const enemiesJ = useData<J>('log1', 'enemies')
  const rng = useData<J>('log1', 'rng')
  const sp = useSprites('log1')
  const enemies = useMemo(() => (enemiesJ ? log1Enemies(enemiesJ, sp) : []), [enemiesJ, sp])
  if (!f || !levels || !enemiesJ) return <Loading what="formulas" />
  const rows: L1.Log1Level[] = levels.levels
  return (
    <div className="with-side">
      <SideNav sections={SECTIONS} />
      <div className="stack">
        <Section id="pipeline" title="Damage pipeline" evidence={f.damage_pipeline.verified}>
          <p>Every hit goes through the same steps. LoG1 has no attack or strength stat: the attacker’s raw damage is computed by the move, then the defender’s flat defense is applied by a random roll.</p>
          <ol>
            {f.damage_pipeline.order.map((s: string, i: number) => (
              <li key={i}>
                <code>{s}</code>
              </li>
            ))}
          </ol>
          <p className="note">
            Signed 32-bit math throughout. <code>rand(n)</code> is the main RNG and returns 0..n <b>inclusive</b>; <code>rand2(n)</code> is a
            second generator returning 0..n−1 (used only for the punch).
          </p>
        </Section>
        <Calc rows={rows} enemies={enemies} />
        <Section id="defense" title="Defense roll — apply_damage" addr="0x08007352" evidence="VERIFIED 113/113">
          <p>
            When the defender has defense, one <code>rand(10000)</code> picks how much of it applies. Almost half of all hits ignore defense
            entirely, and 2% of hits do nothing — so even a punch weaker than the target’s defense lands about half the time.
          </p>
          <Code>{f.apply_damage.defense_roll.replace(/; /g, '\n')}</Code>
          <BarList data={L1.DEFENSE_BUCKETS.map((b, i) => ({ label: b.label, value: b.p, color: `var(--s${i + 1})` }))} format={(v) => pct(v, 2)} max={0.5} />
          <DefenseCurve />
        </Section>
        <Section id="punch" title="Punch" addr={f.player_melee.code} evidence={f.player_melee.verified}>
          <p>The punch deals 75% to 99.8% of Goku’s <b>max HP</b>. The secondary RNG gives a uniform 0..127 roll.</p>
          <Code>{`dmg = (maxHP × (384 + rand2(128))) >> 9`}</Code>
          <LineChart
            logY
            xLabel="level"
            series={[
              { name: 'punch (avg)', points: rows.map((r) => [r.level, (r.melee_damage_min + r.melee_damage_max) / 2]), band: rows.map((r) => [r.level, r.melee_damage_min, r.melee_damage_max]) },
              { name: 'ki blast (maxHP/3)', points: rows.map((r) => [r.level, r.ki_blast_damage]) },
            ]}
          />
        </Section>
        <KiSection rows={rows} />
        <SolarSection />
        <EnemyMeleeSection rows={rows} enemies={enemies} />
        <Section id="exp" title="Experience & levels" addr={f.exp.code} evidence={f.exp.verified}>
          <p>Each kill adds the enemy’s exp reward. The level is the last table row whose exp you reach. Level-up copies max HP/Ki from the table, adds only the HP difference (no heal), and does not refill Ki.</p>
          <Code>{`exp += victim.exp_reward
level = largest i with table[i].exp <= exp
HP += newMaxHP − oldMaxHP;  kiRegen = maxKi >> 6;  cap = maxHP/3 + 1`}</Code>
          <LineChart logY xLabel="level" series={[{ name: 'total exp required', points: rows.map((r) => [r.level, Math.max(1, r.exp_required)]) }]} />
        </Section>
        <Section id="regen" title="Regen, movement, wake radius">
          <KV
            rows={[
              ['Ki regen', f.ki_regen.expr + ' — ' + f.ki_regen.when],
              ['HP regen', 'none (only herbs: +maxHP/4, Senzu: full)'],
              ['Walk', f.movement.player_walk + ' (1 px/f)'],
              ['Flight', `${f.flight.speed}, ${f.flight.cost}`],
              ['Enemy speed', `SPEED_TABLE[class]/16 px/f: ${L1.SPEED_TABLE.join(', ')}`],
              ['Enemy wakes', f.enemy_wake.expr + '; ' + f.enemy_wake.also],
              ['Enemy targets', f.enemy_target.expr],
              ['Timers', Object.entries(f.timers).map(([k, v]) => `${k.replace(/_/g, ' ')} ${v}`).join(' · ')],
            ]}
          />
          <LineChart
            height={180}
            xLabel="aggro tiles"
            yLabel="px"
            series={[{ name: 'wake radius', points: range(15, 1).map((t) => [t, +L1.wakeRadiusPx(t).toFixed(1)]) }]}
          />
          <p className="note">Wake radius² = (96·tiles²) & 0xFFFF — the 16-bit mask wraps for 27+ tiles.</p>
        </Section>
        <Section id="rng" title="RNG" addr={rng?.main?.code}>
          <Code>{`${rng?.main?.step?.join('\n')}\n${rng?.main?.output}\n\nsecondary (${rng?.secondary?.code}):\n${rng?.secondary?.rand2}`}</Code>
          <p className="note">{rng?.main?.seed}. The secondary RNG advances every frame, so punch damage depends on timing.</p>
        </Section>
      </div>
    </div>
  )
}

function DefenseCurve() {
  const [raw, setRaw] = useState(1000)
  const pts = range(41).map((i) => {
    const def = Math.round((raw * 2 * i) / 40)
    return [def, stats(L1.applyDefense(new Map([[raw, 1]]), def)).mean] as [number, number]
  })
  return (
    <>
      <div className="toolbar" style={{ marginTop: 12 }}>
        <NumField label="Raw damage" value={raw} onChange={setRaw} min={1} />
      </div>
      <LineChart
        height={200}
        xLabel="defender defense"
        yLabel="avg damage"
        series={[
          { name: 'average damage', points: pts },
          { name: '0.98·d − 0.2125·def (approx.)', points: pts.map(([d]) => [d, Math.max(0, 0.98 * raw - 0.2125 * d)]), dashed: true },
        ]}
      />
    </>
  )
}

function Calc({ rows, enemies }: { rows: L1.Log1Level[]; enemies: ReturnType<typeof log1Enemies> }) {
  const [dir, setDir] = useState<'p2e' | 'e2p'>('p2e')
  const [lv, setLv] = useState(10)
  const [move, setMove] = useState<'punch' | 'blast' | 'kame'>('punch')
  const [frames, setFrames] = useState(60)
  const [ei, setEi] = useState(enemies.findIndex((e) => e.key === '2'))
  const [eatk, setEatk] = useState<'melee' | 'ki'>('melee')
  const r = rows[lv - 1]
  const e = enemies[Math.max(0, ei)]
  let raw: Dist
  let def: number
  let note = ''
  if (dir === 'p2e') {
    def = e.calc.def ?? 0
    if (move === 'punch') raw = L1.punchDist(r.max_hp)
    else if (move === 'blast') raw = new Map([[L1.kiBlastDamage(r.max_hp), 1]])
    else {
      const k = L1.kiSpent(r.max_ki, r.max_ki, frames)
      raw = new Map([[k.spent, 1]])
      note = `${k.ticks} charge ticks × ${L1.chargeCost(r.max_ki)} Ki = ${k.spent} Ki spent = damage`
    }
  } else {
    def = L1.PLAYER_DEFENSE
    if (eatk === 'melee') raw = L1.enemyMeleeRaw(e.hp, r.max_hp)
    else raw = new Map([[Math.min(Math.trunc(e.hp / 3), L1.damageCap(r.max_hp)), 1]])
  }
  const final = L1.applyDefense(raw, def)
  return (
    <Section id="calc" title="Damage calculator">
      <div className="toolbar">
        <Seg value={dir} onChange={setDir} options={[{ value: 'p2e', label: 'Goku → enemy' }, { value: 'e2p', label: 'Enemy → Goku' }]} />
        <Slider label="Goku level" min={1} max={25} value={lv} onChange={setLv} />
        <label className="field">
          <span>Enemy</span>
          <select value={ei} onChange={(ev) => setEi(+ev.target.value)}>
            {enemies.map((x, i) => (
              <option key={x.key} value={i}>
                {x.name} (HP {fmt(x.hp)}, DEF {x.calc.def})
              </option>
            ))}
          </select>
        </label>
        {dir === 'p2e' ? (
          <Seg
            value={move}
            onChange={setMove}
            options={[
              { value: 'punch', label: 'Punch' },
              { value: 'blast', label: 'Ki blast' },
              { value: 'kame', label: 'Kamehameha' },
            ]}
          />
        ) : (
          <Seg value={eatk} onChange={setEatk} options={[{ value: 'melee', label: 'Melee' }, { value: 'ki', label: 'Ki blast' }]} />
        )}
        {dir === 'p2e' && move === 'kame' && <Slider label="Frames held" min={0} max={600} value={frames} onChange={setFrames} />}
      </div>
      <div className="muted small" style={{ marginBottom: 8 }}>
        Goku Lv{lv}: max HP {fmt(r.max_hp)}, max Ki {fmt(r.max_ki)}, defense 250 · {e.name}: HP {fmt(e.hp)}, DEF {e.calc.def}
        {note && <> · {note}</>}
        {dir === 'e2p' && <> · damage cap per hit = Goku maxHP/3 + 1 = {L1.damageCap(r.max_hp)}</>}
      </div>
      <DistView dist={final} hp={dir === 'p2e' ? e.hp : r.max_hp} />
    </Section>
  )
}

function KiSection({ rows }: { rows: L1.Log1Level[] }) {
  const [lv, setLv] = useState(10)
  const [frames, setFrames] = useState(64)
  const r = rows[lv - 1]
  const k = L1.kiSpent(r.max_ki, r.max_ki, frames)
  return (
    <Section id="ki" title="Ki charge & Kamehameha" addr="0x0800B436" evidence="VERIFIED">
      <p>
        Holding B takes <code>(maxKi + 15) {'>>'} 4</code> Ki every 8th frame (only while you have that much). A tap costs nothing. The
        Kamehameha’s damage is exactly the Ki spent; the ki blast always does maxHP/3 regardless of charge, but its size and range grow.
      </p>
      <div className="toolbar">
        <Slider label="Level" min={1} max={25} value={lv} onChange={setLv} />
        <Slider label="Frames held" min={0} max={600} value={frames} onChange={setFrames} />
      </div>
      <div className="stats">
        <Stat label="Cost per tick" value={L1.chargeCost(r.max_ki)} sub="every 8 frames" />
        <Stat label="Ki spent" value={fmt(k.spent)} sub={`${k.ticks} ticks`} />
        <Stat label="Kamehameha dmg" value={fmt(k.spent)} sub="before defense" />
        <Stat label="Ki blast size" value={L1.kiBlastSize(k.spent)} />
        <Stat label="Ki blast range" value={`${3 * (10 + k.spent)} px`} sub="3 px/f × (10 + Ki spent) f" />
      </div>
      <LineChart
        height={200}
        xLabel="frames held"
        series={[{ name: 'Kamehameha damage (full Ki)', points: range(76).map((i) => [i * 8, L1.kiSpent(r.max_ki, r.max_ki, i * 8).spent]) }]}
        marker={frames}
      />
    </Section>
  )
}

function SolarSection() {
  const [maxKi, setMaxKi] = useState(400)
  const [charge, setCharge] = useState(125)
  return (
    <Section id="solar" title="Solar Flare" addr="0x0800B692" evidence="VERIFIED (charge 125/400 → 153 frames)">
      <p>Freezes all enemy AI. The freeze lasts longer the more of your max Ki you charged into it; the next one is allowed 60 frames after it ends.</p>
      <Code>{`duration = 60 + min(charge, maxKi) × 300 / maxKi      (60..360 frames)
cooldown  = release + duration + 60`}</Code>
      <div className="toolbar">
        <NumField label="max Ki" value={maxKi} onChange={setMaxKi} min={1} />
        <NumField label="Ki charged" value={charge} onChange={setCharge} min={0} />
      </div>
      <div className="stats">
        <Stat label="Freeze" value={`${L1.solarFlareFrames(charge, maxKi)} f`} sub={`${(L1.solarFlareFrames(charge, maxKi) / 60).toFixed(2)} s`} />
        <Stat label="Next allowed after" value={`${L1.solarFlareFrames(charge, maxKi) + 60} f`} />
      </div>
    </Section>
  )
}

function EnemyMeleeSection({ rows, enemies }: { rows: L1.Log1Level[]; enemies: ReturnType<typeof log1Enemies> }) {
  const [hp, setHp] = useState(80000)
  const [lv, setLv] = useState(25)
  const raw = useMemo(() => L1.enemyMeleeRaw(hp), [hp])
  const capped = useMemo(() => L1.enemyMeleeRaw(hp, rows[lv - 1].max_hp), [hp, lv, rows])
  const s = stats(raw)
  const neg = [...raw].filter(([v]) => v < 0).reduce((a, [, p]) => a + p, 0)
  const final = L1.applyDefense(capped, L1.PLAYER_DEFENSE)
  const heal = [...final].filter(([v]) => v < 0).reduce((a, [, p]) => a + p, 0)
  return (
    <Section id="enemy" title="Enemy melee & the rand() overflow bug" addr="0x0800F7EC" evidence="VERIFIED (henchman_5 dealt −13557)">
      <p>
        Enemies hit for a quarter of their <b>current</b> HP plus a random amount up to half of it. <code>rand(n)</code> multiplies a 16-bit
        value by n+1 in a signed 32-bit register: once n ≥ 32768 the product can pass 2³¹ and the result turns negative. Enemies with 65536+ HP
        (late henchmen, the Ginyu Force, Frieza) can roll negative damage, which <b>heals</b> Goku when the defense roll lands on “full damage”.
      </p>
      <Code>{`raw = cur_hp/4 + rand(cur_hp/2)
rand(n) = asr32(int32((x & 0xFFFF) × (n+1)), 16)
raw = min(raw, player_maxHP/3 + 1)   then apply_damage with Goku's defense 250`}</Code>
      <div className="toolbar">
        <label className="field">
          <span>Enemy</span>
          <select value={hp} onChange={(e) => setHp(+e.target.value)}>
            {[...new Map(enemies.filter((e) => e.hp > 1 && e.hp < 1e9).map((e) => [e.hp, e])).values()].map((e) => (
              <option key={e.key} value={e.hp}>
                {e.name} (HP {fmt(e.hp)})
              </option>
            ))}
          </select>
        </label>
        <NumField label="Current HP" value={hp} onChange={setHp} min={1} width={110} />
        <Slider label="Goku level (cap)" min={1} max={25} value={lv} onChange={setLv} />
      </div>
      <div className="stats">
        <Stat label="Raw min" value={fmt(s.min)} />
        <Stat label="Raw max" value={fmt(s.max)} />
        <Stat label="Raw negative" value={pct(neg)} />
        <Stat label="Heals Goku" value={pct(heal)} sub="after cap + defense" />
        <Stat label="Avg vs Goku" value={fmt(stats(final).mean, 0)} sub={`cap ${L1.damageCap(rows[lv - 1].max_hp)}`} />
      </div>
      <Histogram bins={histogram(raw, 48)} xLabel="raw melee damage (before cap)" />
    </Section>
  )
}
