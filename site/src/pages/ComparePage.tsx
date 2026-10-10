import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { LineChart } from '../components/Chart'
import { Card } from '../components/ui'
import * as L2 from '../formulas/log2'
import * as BU from '../formulas/buu'
import { range } from '../lib/util'

const ROWS: [string, ReactNode, ReactNode, ReactNode][] = [
  [
    'Engine',
    'Older Webfoot engine, C with 0x40-byte entities and 0x278-byte characters; ~64 KB of Thumb code.',
    'Webfoot “WF” C++ engine: self-relative vtables, message broadcast for hits, state singletons.',
    'Same WF engine, extended: bytecode VM for items/loot/story, larger AI node set.',
  ],
  [
    'Damage model',
    <>
      Move-specific raw damage (punch 75–99.8% of <b>max HP</b>, ki blast maxHP/3, Kamehameha = Ki spent), then a random flat-defense roll.
    </>,
    <>
      One formula: power × (128−END)/128 × f(S) cubic in STR/POW × scale × taken-multiplier, +0–9.8% spread.
    </>,
    <>
      New formula in 1/1024 HP: melee power × STR/20, (221 − END) linear defense, A(S) cubic factor, per-hit mult, resist.
    </>,
  ],
  ['Stats', 'HP, Ki, flight energy; defense constant 250. No attack stat.', 'HP, EP, STR, POW, END (8.8 fixed, cap 100).', 'HP, EP, STR, POW, END, END2, SPD (u8) + equipment modifiers.'],
  [
    'Growth',
    'Fixed table: 25 levels of absolute HP/Ki/flight values.',
    'Per level: +min..max/256 per stat (random), max HP +5.5–7.5%, max EP +2..4; levels 1–50.',
    'Per level: HP/EP add + percent (random); 3 stat points to spend by hand; levels 50–200.',
  ],
  ['Crits', 'None.', 'Melee only: STR × 0.25%, ×2 power.', 'Melee only: (82·STR + equipment)/65536, ×1.5 power; sub 3 never crits.'],
  ['Defense', 'Roll: 48% full, 25% −def/4, 15% −def/2, 10% −def, 2% miss.', '×(128 − END)/128 and a per-class taken multiplier.', '(221 − END′)/221, END ≥ 200 compressed; frontal block = 1/16.'],
  ['Per-hit cap / i-frames', 'Enemy hits capped at player maxHP/3 + 1; no player i-frames; enemies 80 f invulnerable.', 'Player hurt 32 f invulnerable; no general cap.', 'Player hurt 24 f invulnerable (resist bytes 0).'],
  ['Experience', 'exp += reward; table lookup.', 'Reward clamped to [span/128, span/2] of the current level; active character only.', '(exp × multiplier + 255) >> 8; equipment raises the multiplier; fusions feed both.'],
  ['Regen', 'Ki +maxKi/64 every 16 f (not while flying). No HP regen.', 'Active EP +3% / bench HP & EP +5% every 595.8 f.', 'Same timer and rates as LoG II; “Z” Armor adds HP regen.'],
  ['Forms', 'Sprite variants only (Super Saiyan, mallet) — no stat effect found.', '22 forms; controllers add +6/+10/+15 or set 100, change speed, drain EP.', '54 forms; SS +5 stats/+128 speed, SS3 +15/+192 with gauges; fusion forms.'],
  ['Moves', 'Punch + 3 ki moves (Ki Blast, Kamehameha, Solar Flare) + flight.', '14 move types, special melee per form, attack table of 16 entries.', '23 skills with 3 upgrade levels, 18 attack types, block and energy shield.'],
  ['Enemy AI', 'One generic routine: move type × target type × fight style, flow-field pathing.', 'Behaviour trees: 152 states, ~50 node types, AI stack with on-hit trees.', 'Same tree engine: 732 reachable states, 178 node types; bosses add special nodes.'],
  ['Drops', 'None — items are fixed pickups.', 'Per-spawn bytecode: d100 roll against HP/EP orb tables.', 'Script #497: 1–8 rolls of coins, equipment, supplies (with a global-variable quirk).'],
  ['RNG', 'Two xor/rotate generators, rand(n) inclusive with a signed-overflow bug.', 'Same recurrence, rand(n) = ((x&0xFFFF)·n)>>16.', 'Identical to LoG II.'],
  ['Graphics', '4bpp tiles, 16-colour palette per sprite; strip-based animations.', '8bpp, one 256-colour OBJ palette, Webfoot-LZ compressed frames.', 'Same, multi-OBJ frames (bosses up to 64×64 pieces), frame-major animations.'],
]

export function ComparePage() {
  // energy hit at power 1.0, scale/mult 1.0, no defense, average random roll — in HP
  const l2 = range(101).map((s) => [s, +avg(L2.damageDist({ type: 1, power: 1024, scale: 1024, S: s, DEF: 0, MULT: 128 }).dist).toFixed(1)] as [number, number])
  const bu = range(101).map((s) => [s, +avg(BU.damageDist({ kind: 1, sub: 4, power: 1024, mult: 1024, S: s, D: 0, resist: 128 }).dist).toFixed(1)] as [number, number])
  return (
    <div className="page wide">
      <h1>How the mechanics evolved</h1>
      <p className="lede">
        The Legacy of Goku (2002) runs on an older engine; LoG II (2003) and Buu’s Fury (2004) share Webfoot’s C++ “WF” engine but Buu’s Fury
        rewrote the damage model and progression. Click a game name for its full breakdown.
      </p>
      <div className="table-wrap">
        <div className="table-scroll" style={{ maxHeight: 'none' }}>
          <table className="compare-table">
            <thead>
              <tr>
                <th></th>
                <th className="g-log1">
                  <Link to="/g/log1">The Legacy of Goku</Link>
                </th>
                <th className="g-log2">
                  <Link to="/g/log2">Legacy of Goku II</Link>
                </th>
                <th className="g-buu">
                  <Link to="/g/buu">Buu’s Fury</Link>
                </th>
              </tr>
            </thead>
            <tbody>
              {ROWS.map(([k, a, b, c]) => (
                <tr key={k}>
                  <td>{k}</td>
                  <td>{a}</td>
                  <td>{b}</td>
                  <td>{c}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <div className="grid2" style={{ marginTop: 16 }}>
        <Card title="Damage vs attack stat (energy hit, power 1.0, no defense)">
          <LineChart
            xLabel="POW"
            yLabel="HP per hit"
            series={[
              { name: 'LoG II', color: 'var(--g-log2)', points: l2 },
              { name: 'Buu’s Fury', color: 'var(--g-buu)', points: bu },
            ]}
          />
          <p className="note">Average of the 26 random rolls. LoG I has no attack stat (damage follows max HP), so it is not shown.</p>
        </Card>
        <Card title="Share of damage left after defense">
          <LineChart
            xLabel="END"
            yLabel="%"
            series={[
              { name: 'LoG II (128 − END)/128', color: 'var(--g-log2)', points: range(128).map((d) => [d, +(((128 - d) / 128) * 100).toFixed(1)]) },
              { name: 'Buu’s Fury (221 − END′)/221', color: 'var(--g-buu)', points: range(256).map((d) => [d, +(((221 - BU.effectiveDef(d)) / 221) * 100).toFixed(1)]) },
            ]}
          />
          <p className="note">LoG II defense can reach 0% at END 128 (players cap at 100); Buu’s Fury never drops below 1/221 thanks to the END ≥ 200 compression.</p>
        </Card>
      </div>
    </div>
  )
}

function avg(d: Map<number, number>): number {
  let s = 0
  for (const [v, p] of d) s += v * p
  return s
}
