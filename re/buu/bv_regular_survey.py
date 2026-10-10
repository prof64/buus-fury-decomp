"""List regular-group enemies: attack nodes, special nodes, AUTO nodes (with auto analysis summary)."""
import json
from common import *

d = json.loads((DATA / 'enemy_ai.json').read_text())
g = json.loads((WORK / 'boss_groups.json').read_text())['regular']
mine = set(g['auto_nodes'])
NT = {n['create_fn']: n for n in d['node_types']}
types = [int(t, 16) for t in g['enemy_types']]
ej = {e['type']: e for e in json.loads((DATA / 'enemies.json').read_text())['enemies']}
for t in types:
    e = d['enemies'][t]
    s = e['summary']
    used = set()
    for st in e['states']:
        txt = d['states'][st]['text'] if isinstance(d['states'], dict) else None
        for line in txt or []:
            for fn in NT:
                pass
    print(hex(t), e['name'], 'sp', len(ej[t].get('spawns') or []), 'atk', sorted({a['node'] for a in s['attacks']}),
          'special', s['special_nodes'])
print()
for fn in g['auto_nodes']:
    n = NT[fn]
    a = n.get('auto', {})
    print(fn, 'uses', n['uses'], [hex(x) for x in n['enemy_types']], a.get('kind'), 'anims', a.get('anims'),
          'spawn', a.get('spawns_objects'), 'rand', a.get('uses_rand'), 'data', a.get('data'),
          'ctrl', [(c.get('controller'), c.get('frame_thresholds'), c.get('spawns_objects'), c.get('data')) for c in a.get('controllers', [])],
          'cond', a.get('condition'), 'defreads', a.get('def_reads'))
