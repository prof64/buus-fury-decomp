"""Dump group 'others': owned AUTO nodes (auto analysis) and every state tree text for each enemy type."""
import json
from common import *

g = json.loads((WORK / 'boss_groups.json').read_text())['others']
d = json.loads((DATA / 'enemy_ai.json').read_text())
nt = {n['create_fn']: n for n in d['node_types']}
st = dict((k, v) for k, v in d['states'])  if isinstance(d['states'], list) else d['states']
E = {e['type']: e for e in d['enemies']}
ej = {e['type']: e for e in json.loads((DATA / 'enemies.json').read_text())['enemies']}
print('OWNED AUTO NODES')
for fn in g['auto_nodes']:
    n = nt[fn]
    print(fn, 'uses', n['uses'], 'types', [hex(t) for t in n['enemy_types']], json.dumps(n.get('auto'))[:700])
print()
for th in g['enemy_types']:
    t = int(th, 16)
    e = E[t]; x = ej[t]
    print('=' * 100)
    print(th, e['name'], 'hp', x['hp'], 'exp', x['exp'], 'STR', x['STR'], 'POW', x['POW'], 'END', x['END'],
          'res', x['resist_melee'], x['resist_energy'], 'react', x['hit_reaction'], 'init', e['initial_state'],
          'spawns', json.dumps(x.get('spawns'))[:300])
    print('summary', json.dumps(e['summary'])[:600])
    for s in e['states']:
        print('-- state', s)
        print('\n'.join(st[s]['text']))
