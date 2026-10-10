"""Print the janemba group's assignment and every reachable state tree (text) for its enemies."""
import json
from common import *

g = json.loads((WORK / 'boss_groups.json').read_text())['janemba']
print(json.dumps(g, indent=1))
d = json.loads((DATA / 'enemy_ai.json').read_text())
E = {e['type']: e for e in d['enemies']}
S = dict((k, v) for k, v in d['states'].items()) if isinstance(d['states'], dict) else dict(d['states'])
NT = {n['create_fn']: n for n in d['node_types']}
seen = set()
for th in g['enemy_types']:
    t = int(th, 16)
    e = E[t]
    print('=' * 20, th, e['name'], 'init', e['initial_state'], 'states', e['states'])
    for s in e['states']:
        if s in seen:
            print('  state', s, '(see above)')
            continue
        seen.add(s)
        print('  state', s)
        for line in S[s]['text']:
            print('    ' + line)
print('=' * 20, 'AUTO nodes')
for fn in g['auto_nodes']:
    n = NT[fn]
    print(fn, 'uses', n['uses'], 'types', [hex(x) for x in n['enemy_types']], json.dumps(n['auto']))
