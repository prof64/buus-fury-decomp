import { useEffect, useState } from 'react'
import type { BattleConfig, Outcome, StatLine } from '../battle/core/ruleset'
import type { BattleSession } from '../battle/core/session'
import { getRuleset } from '../battle/rulesets'
import { defaultSetup, SetupScreen, toConfig, type SetupState } from '../battle/ui/SetupScreen'
import { BattleView } from '../battle/ui/BattleView'
import '../battle/ui/battle.css'

interface Result {
  outcome: Outcome
  lines: StatLine[]
}

const KEY = 'bt-setup-v1'

export function BattlePage() {
  const rs = getRuleset('log2')!
  const [ready, setReady] = useState(false)
  const [err, setErr] = useState<string>()
  const [setup, setSetup] = useState<SetupState>()
  const [cfg, setCfg] = useState<BattleConfig>()
  const [result, setResult] = useState<Result>()
  const [run, setRun] = useState(0)

  useEffect(() => {
    rs.load()
      .then(() => {
        const def = defaultSetup(rs)
        let s = def
        try {
          const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null')
          if (saved && saved.game === def.game) s = { ...def, ...saved, fighter: { ...def.fighter, ...saved.fighter, options: { ...def.fighter.options, ...saved.fighter?.options }, bonus: { ...def.fighter.bonus, ...saved.fighter?.bonus } } }
        } catch {
          /* ignore */
        }
        setSetup(s)
        setReady(true)
      })
      .catch((e) => setErr(String(e)))
  }, [rs])

  useEffect(() => {
    if (setup) localStorage.setItem(KEY, JSON.stringify(setup))
  }, [setup])

  if (err) return <div className="page">Failed to load: {err}</div>
  if (!ready || !setup) return <div className="page muted">Loading Legacy of Goku II data…</div>

  const start = () => {
    setResult(undefined)
    setCfg(toConfig(rs, setup))
    setRun((r) => r + 1)
  }
  const onEnd = (o: Outcome, s: BattleSession) => {
    const w = s.world
    const st = w.stats
    const secs = st.frames / 59.7275
    setResult({
      outcome: o,
      lines: [
        { label: 'Time', value: `${Math.floor(secs / 60)}:${String(Math.floor(secs % 60)).padStart(2, '0')} (${st.frames} frames)` },
        { label: 'Damage dealt', value: st.damageDealt },
        { label: 'Damage taken', value: st.damageTaken },
        { label: 'Hits dealt / taken', value: `${st.hitsDealt} / ${st.hitsTaken}` },
        { label: 'Critical hits', value: st.crits },
        { label: 'Biggest hit', value: st.maxHit },
        { label: 'Enemies defeated', value: `${st.kills} / ${st.enemiesTotal}` },
        { label: 'EP spent', value: st.epSpent },
        ...s.battle.summary(w),
      ],
    })
  }

  return (
    <div className="bt-page">
      {!cfg && <SetupScreen rs={rs} state={setup} onChange={setSetup} onStart={start} />}
      {cfg && (
        <BattleView
          key={run}
          rs={rs}
          cfg={cfg}
          onEnd={onEnd}
          onQuit={() => {
            setCfg(undefined)
            setResult(undefined)
          }}
        />
      )}
      {cfg && result && (
        <div className="bt-result">
          <div className="bt-result-card">
            <h2 className={result.outcome}>{result.outcome === 'won' ? 'Victory!' : 'Defeated…'}</h2>
            <table>
              <tbody>
                {result.lines.map((l) => (
                  <tr key={l.label}>
                    <td>{l.label}</td>
                    <td>{l.value}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="bt-row">
              <button className="bt-start" onClick={start}>
                Rematch
              </button>
              <button
                onClick={() => {
                  setCfg(undefined)
                  setResult(undefined)
                }}
              >
                Change setup
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
