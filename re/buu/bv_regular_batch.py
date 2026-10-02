"""Trace every regular-group enemy (force-spawned) with the player held at 3 offsets; collect spawned
object classes, hit messages and damage to the player.  Writes work/buu/ptrace/batch.json."""
import json
import sys
from collections import Counter, defaultdict
from common import *
from bv_regular_trace import trace

g = json.loads((WORK / 'boss_groups.json').read_text())['regular']
types = [int(t, 16) for t in g['enemy_types']]
if len(sys.argv) > 1:
    types = [int(a, 0) for a in sys.argv[1:]]
ej = {e['type']: e for e in json.loads((DATA / 'enemies.json').read_text())['enemies']}
out = {}
for t in types:
    res = dict(name=ej[t]['name'], runs=[])
    for dx, dy in ((60, 0), (0, 60), (120, 0)):
        try:
            log = trace(t, 700, dx, dy)
        except Exception as ex:  # noqa
            res['runs'].append(dict(dx=dx, dy=dy, error=repr(ex)))
            continue
        objs = defaultdict(list)
        for k, L in log['objs'].items():
            if len(objs[L['vt']]) < 3:
                objs[L['vt']].append(dict(site=L['site'], first=L['first'], lived=L['lived'], frames=L['frames'][:80]))
        res['runs'].append(dict(
            dx=dx, dy=dy, spawned=bool(log['states']),
            spawns=[list(k) + [v] for k, v in Counter((s['vt'], s['site']) for s in log['spawns']).items()],
            sends=[list(k) + [v] for k, v in Counter((s['lr'], s['sender_vt'], s['atk'], s['mult'], s['world'], s['kind'])
                                                     for s in log['sends']).items()],
            damage=[{k: d[k] for k in ('f', 'to_player', 'attacker', 'atk', 'kind', 'sub', 'mult', 'S', 'POW', 'E', 'E2',
                                       'actual', 'predicted', 'ok', 'lr', 'stack')} for d in log['damage']],
            objs=objs))
    out[hex(t)] = res
    r0 = res['runs']
    print(hex(t), res['name'], [(r.get('spawned'), len(r.get('damage', [])), sorted({tuple(s[:2]) for s in r.get('spawns', [])})) for r in r0], flush=True)
name = 'batch.json' if len(sys.argv) <= 1 else 'batch_' + '_'.join(sys.argv[1:]) + '.json'
(WORK / 'ptrace').mkdir(exist_ok=True)
(WORK / 'ptrace' / name).write_text(json.dumps(out))
