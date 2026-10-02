// Renders decoded enemy behaviour trees (LoG2 ai.json / Buu enemy_ai.json) as readable nested lists.
// Both games use the same engine idea: composites, conditions, transitions, actions; params differ in shape.
import { useState, type ReactNode } from 'react'
import { fmt, type J } from '../lib/util'

const KIND_OF: Record<string, string> = {
  Sequence: 'composite',
  RaceFirst: 'composite',
  RaceAll: 'composite',
  Selector: 'composite',
  ParallelAll: 'composite',
  ParallelAny: 'composite',
  GotoState: 'transition',
  PushState: 'transition',
  PopState: 'transition',
  SubState: 'inline',
  ClassDefaultState: 'inline',
}

const HUMAN: Record<string, string> = {
  Sequence: 'do in order',
  RaceFirst: 'run together, stop when the first finishes',
  RaceAll: 'run together, stop when any finishes',
  Selector: 'first branch that applies (priority list)',
  ParallelAll: 'run together until all finish',
  ParallelAny: 'run together until any finishes',
  IfPlayerWithin: 'if the player is within',
  IfPlayerSeen: 'if the player is seen within',
  Chance: 'with probability per frame',
  GotoState: 'switch to state',
  PushState: 'push state',
  SubState: 'run state inline',
  Wait: 'wait',
  ChasePlayer: 'chase the player',
  MeleeStrike: 'melee strike',
  MeleeAttack: 'melee attack',
  ShootKiBlast: 'shoot a ki blast',
  KiBlast: 'shoot a ki blast',
  Patrol: 'patrol spawn waypoints',
  FleePlayer: 'run away',
  StepTowardPlayer: 'step toward the player',
  StepAwayFromPlayer: 'step away',
  FacePlayer: 'face the player',
  ApplyDamage: 'take the damage',
  Knockback: 'knockback',
}

function kindOf(n: J): string {
  if (n.kind) return n.kind
  const t = n.type ?? n.name ?? ''
  if (KIND_OF[t]) return KIND_OF[t]
  if (/^If|Chance|While/.test(t)) return 'cond'
  return 'action'
}

/** Extracts readable params from a node of either game. Returns [label, value, stateLink?]. */
function paramsOf(n: J): { k: string; v: string; state?: number | string }[] {
  const out: { k: string; v: string; state?: number | string }[] = []
  if (n.params && typeof n.params === 'object' && !Array.isArray(n.params)) {
    for (const [k, p] of Object.entries<J>(n.params)) {
      if (p && typeof p === 'object') {
        if (p.unit === 'state') out.push({ k, v: '0x' + Number(p.value ?? p.raw).toString(16), state: Number(p.value ?? p.raw) })
        else if (p.percent_per_frame !== undefined) out.push({ k, v: `${fmt(p.percent_per_frame, 2)}%/f` })
        else if (p.px_per_frame !== undefined) out.push({ k, v: `${fmt(p.px_per_frame, 3)} px/f` })
        else if (p.radius_px !== undefined) out.push({ k, v: `${fmt(p.radius_px, 1)} px` })
        else if (p.unit === 'frames') out.push({ k, v: `${p.raw} f` })
        else if (p.unit === 'dist2') out.push({ k: 'radius', v: `${fmt(Math.sqrt(p.raw), 1)} px` })
        else if (p.unit === 'speed256') out.push({ k, v: `${fmt(p.raw / 256, 3)} px/f` })
        else if (p.unit === 'prob16') out.push({ k, v: `${fmt((p.raw / 65536) * 100, 2)}%/f` })
        else if (p.unit === 'frac16') out.push({ k, v: `${fmt((p.raw / 65536) * 100, 1)}%` })
        else if (p.percent !== undefined) out.push({ k, v: `${fmt(p.percent, 2)}%` })
        else out.push({ k, v: String(p.value ?? p.raw ?? JSON.stringify(p)) })
      } else out.push({ k, v: String(p) })
    }
    return out
  }
  for (const [k, v] of Object.entries<J>(n)) {
    if (['addr', 'type', 'name', 'children', 'fn', 'kind', 'text', 'raw', 'words'].includes(k)) continue
    if (v === null || v === undefined || typeof v === 'object') continue
    if (k === 'state') out.push({ k, v: String(v), state: Number(v) })
    else if (k === 'radius_sq') continue
    else out.push({ k: k.replace(/_/g, ' '), v: typeof v === 'number' ? fmt(v, 4) : String(v) })
  }
  return out
}

export function AiNode({ node, onState, depth = 0 }: { node: J; onState?: (s: number | string) => void; depth?: number }) {
  const [open, setOpen] = useState(depth < 6)
  if (!node) return null
  const t = node.type ?? node.name ?? '?'
  const kind = kindOf(node)
  const kids: J[] = Array.isArray(node.children) ? node.children.filter((c: J) => c && typeof c === 'object') : []
  const params = paramsOf(node)
  return (
    <li className={'ai-node k-' + kind}>
      <div className="ai-line">
        {kids.length > 0 ? (
          <button className="ai-tog" onClick={() => setOpen(!open)}>
            {open ? '▾' : '▸'}
          </button>
        ) : (
          <span className="ai-tog" />
        )}
        <span className="ai-name">{t}</span>
        {HUMAN[t] && <span className="ai-human">{HUMAN[t]}</span>}
        {params.map((p) =>
          p.state !== undefined && onState ? (
            <button key={p.k} className="ai-param link-btn" onClick={() => onState(p.state!)}>
              → state {p.v}
            </button>
          ) : (
            <span key={p.k} className="ai-param">
              <i>{p.k}</i> {p.v}
            </span>
          ),
        )}
        {node.addr && <code className="ai-addr">{node.addr}</code>}
      </div>
      {open && kids.length > 0 && (
        <ul>
          {kids.map((c, i) => (
            <AiNode key={i} node={c} onState={onState} depth={depth + 1} />
          ))}
        </ul>
      )}
    </li>
  )
}

export function AiTree({ root, onState }: { root: J; onState?: (s: number | string) => void }) {
  return (
    <ul className="ai-tree">
      <AiNode node={root} onState={onState} />
    </ul>
  )
}

export function AiLegend(): ReactNode {
  return (
    <div className="ai-legend">
      <span className="k-composite">composite</span>
      <span className="k-cond">condition</span>
      <span className="k-transition">state change</span>
      <span className="k-inline">inline sub-state</span>
      <span className="k-action">action</span>
    </div>
  )
}
