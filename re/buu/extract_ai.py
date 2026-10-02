"""Generate data/buu/enemy_ai.json: decoded enemy behaviour trees.

  node_types : every node create-fn reachable from an enemy's initial state, with name, parameter
               layout + units, semantics and status (registry in ai_decode.py), or automatic analysis
               (runtime vtable, condition/action, anims, controller frame thresholds, spawns) for
               node types not hand-decoded.
  states     : decoded tree (JSON) + text rendering for every reachable AI state.
  enemies    : per enemy type: initial state, reachable states, summary (aggro/leash radii, speeds,
               attacks, waits/cooldowns, random chances).
"""
import ai2_boot  # noqa: F401
import json
from collections import Counter, defaultdict
from pathlib import Path
from common import *
from ai_decode import (REGISTRY, UNIT_DESC, N_ENEMIES, STATE_TABLE, ENEMY_TABLE, auto, decode, pretty,
                       state_root, initial_state, walk_states, convert)

OUT = Path(__file__).resolve().parents[2] / 'data' / 'buu' / 'enemy_ai.json'
ENEMIES_JSON = DATA / 'enemies.json'

ATTACK_NODES = {'MeleeAttack', 'KiBlast', 'GunShot', 'RifleShot', 'SoldierThrow'}
MOVE_NODES = {'ChasePlayer', 'StepTowardPlayer', 'StepAwayFromPlayer', 'AlignWithPlayer', 'Wander',
              'WalkToPlayer', 'FlyToPlayer'}


# emulator measurements (re/buu/ai2_verify.py; player pinned at dx px on the same row, enemy pinned for
# the aggro scan, AI state changes logged at 0x0800AA4A).  'transplant' = boss tree run on the SW-forest
# Majin Fighter body (bosses only spawn behind story flags), valid for movement/AI logic, not gfx.
VERIFIED = [
    dict(enemy='Majin Fighter 0x44', scene='buu_fight_swforest.ss (8,3)',
         checks=['aggro: leaves state 0x14B at dx<=100, not at 104 (IfPlayerWithin2 r2=10000 -> 100 px)',
                 'ChasePlayer 153/256: 0.5977 px/frame is the modal per-frame step',
                 'attack state 0x14E (MeleeAttack + Wait 18) lasts 41-47 frames; one hit object spawned on entry']),
    dict(enemy='Gunman 0x2C', scene='warp East District 439 (2,10)',
         checks=['aggro 88 px (yes at 88, no at 92)', 'ChasePlayer 256/256: 1.0 px/frame over 589 frames',
                 'shot state 0xF4 (Align, Wait 30, GunShot 16 f) lasts 50-51 f; bullet spawned 35 f after entry',
                 'melee state 0xEF lasts 29 f']),
    dict(enemy='Mercenary 0x4B', scene='warp East District 439 (2,14)',
         checks=['aggro 72 px (IfPlayerWithin r2=5184; yes at 72, no at 76)',
                 'state 0x178 (RifleShot 32 f + Wait 90) lasts exactly 123 f, bullet 8 f after entry',
                 'MISMATCH: while AlignWithPlayer(153) runs the enemy moves 0.5 px/frame, so that node speed unit is unconfirmed']),
    dict(enemy='Ghost 0x20', scene="warp Pilaf's Castle (10,8)",
         checks=['Wander speed 128/256: 0.5 px/frame for 848 frames', 'state 0xD8 -> 0xD9 unconditional (SpawnPuff)']),
    dict(enemy='Spopovich 0x6D (transplant)', scene='buu_fight_swforest.ss',
         checks=['aggro 100 px', 'WalkToPlayer 153/256: 0.5977 px/frame over 316 frames']),
    dict(enemy='Yakon 0x91 (transplant)', scene='buu_fight_swforest.ss',
         checks=['no aggro radius (MarkTime -> wander state immediately)', 'Wander 204/256: 0.7969 px/frame',
                 'combo state 0x322: 6 MeleeAttacks (6 hit objects ~20 f apart) over 161 f']),
    dict(enemy='Dabura 0x12 / Pui Pui 0x60 (transplant)', scene='buu_fight_swforest.ss',
         checks=['Dabura melee state 0x71 lasts 28-34 f; no aggro radius',
                 'Pui Pui runs the generic fighter tree (identical timings to Majin Fighter, aggro 100 px)']),
]


def names():
    try:
        return {e['type']: e['name'] for e in json.loads(ENEMIES_JSON.read_text())['enemies']}
    except (OSError, KeyError, ValueError):
        return {}


def iter_nodes(n):
    yield n
    for c in n.get('children', []):
        yield from iter_nodes(c)


def summarize(state_ids, trees):
    s = dict(aggro_radius_px=set(), within_px=set(), farther_px=set(), speeds=defaultdict(set), attacks=[],
             waits_frames=set(), cooldowns_frames=set(), time_in_state_frames=set(), hp_thresholds_percent=set(),
             chances=defaultdict(set), special_nodes=set(), blocks=False, counters=set())
    for sid in state_ids:
        for i, n in enumerate(iter_nodes(trees[sid])):
            nm, p = n['name'], n.get('params', {})
            if nm == 'IfPlayerWithin2' and i <= 2:
                s['aggro_radius_px'].add(p['r2']['px'])
            elif nm in ('IfPlayerWithin', 'IfPlayerWithin2'):
                s['within_px'].add(p['r2']['px'])
            elif nm == 'IfPlayerFarther':
                s['farther_px'].add(p['r2']['px'])
            elif nm in MOVE_NODES and 'speed' in p:
                s['speeds'][nm].add(p['speed']['px_per_frame'])
            elif nm == 'Wait':
                s['waits_frames'].add(p['frames']['frames'])
            elif nm == 'IfCooldownOver':
                s['cooldowns_frames'].add(p['frames']['frames'])
            elif nm == 'IfTimeInState':
                s['time_in_state_frames'].add(p['frames']['frames'])
            elif nm == 'IfHpBelow':
                s['hp_thresholds_percent'].add(p['frac']['percent'])
            elif nm == 'Chance':
                kid = n.get('children', [{}])[0]
                what = kid.get('name', '?')
                if what == 'GotoState':
                    what += f" {kid['params']['state']['raw']:#x}"
                s['chances'][what].add(p['p']['percent_per_frame'])
            elif nm == 'Block':
                s['blocks'] = True
            elif nm == 'IfCounterEq':
                s['counters'].add(p['n']['raw'])
            if nm in ATTACK_NODES:
                s['attacks'].append(dict(node=nm, state=hex(sid), addr=n['addr']))
            elif n['name'].startswith('node_'):
                s['special_nodes'].add(n['fn'])
    out = {}
    for k, v in s.items():
        if isinstance(v, set):
            out[k] = sorted(v)
        elif isinstance(v, defaultdict):
            out[k] = {kk: sorted(vv) for kk, vv in v.items()}
        else:
            out[k] = v
    # one entry per attack node type/state
    seen, atk = set(), []
    for a in out['attacks']:
        key = (a['node'], a['state'])
        if key not in seen:
            seen.add(key)
            atk.append(a)
    out['attacks'] = atk
    return out


def main():
    nm = names()
    trees, texts = {}, {}
    enemies = []
    users = defaultdict(set)
    counts = Counter()
    for t in range(N_ENEMIES):
        s0 = initial_state(t)
        sts = walk_states(s0)
        for sid in sts:
            if sid not in trees:
                trees[sid] = decode(state_root(sid))
                texts[sid] = pretty(trees[sid])
        for sid in sts:
            for n in iter_nodes(trees[sid]):
                users[int(n['fn'], 16)].add(t)
                counts[int(n['fn'], 16)] += 1
        enemies.append(dict(type=t, name=nm.get(t), initial_state=hex(s0), states=[hex(x) for x in sts],
                            summary=summarize(sts, trees)))
    node_types = []
    for fn, c in counts.most_common():
        r = REGISTRY.get(fn)
        e = dict(create_fn=hex(fn), uses=c, enemy_types=sorted(users[fn]))
        if r:
            e.update(name=r['name'], kind=r['kind'], status=r['status'], semantics=r['sem'],
                     params=[dict(offset=hex(o), name=n, unit=u, unit_desc=UNIT_DESC.get(u)) for o, n, u in r.get('params', [])],
                     children=('count @+0xC, defs from +0x10' if r.get('children') == 'list'
                               else [hex(o) for o in r.get('children', [])]))
        else:
            e.update(name=f'node_{fn:08x}', status='AUTO', auto=auto(fn),
                     enemy_names=sorted({nm.get(x) for x in users[fn]} - {None}))
        node_types.append(e)
    out = dict(
        meta=dict(generator='re/buu/extract_ai.py (library re/buu/ai_decode.py)',
                  state_table=hex(STATE_TABLE), enemy_table=hex(ENEMY_TABLE), enemy_initial_state_offset='+0x08',
                  node_def_layout='[create_fn, f1, f2, params from +0xC..., child def pointers]',
                  execution=('each enemy runs the tree of the state on top of its AI stack (obj+0x238) once per frame; '
                             'update returns 0 = running, non-zero = finished; GotoState replaces the state, '
                             'PushState pushes; SubState inlines another state tree'),
                  units=UNIT_DESC,
                  verified=VERIFIED,
                  status_legend=dict(VERIFIED='measured in the emulator', DISASM='read from the code',
                                     INFERRED='semantics guessed from partial reading', AUTO='automatic analysis only')),
        node_types=node_types,
        states={hex(k): dict(text=texts[k], tree=trees[k]) for k in sorted(trees)},
        enemies=enemies)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1))
    print('wrote', OUT, len(trees), 'states', len(node_types), 'node types')


if __name__ == '__main__':
    main()
