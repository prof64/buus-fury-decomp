"""Inventory of AI node types that are not hand-decoded: which enemies use them, how often, and the
auto-analysis summary.  Also lists enemy types that have no drop script (bosses / story enemies)."""
import json
from collections import defaultdict
from common import *

d = json.loads((DATA / 'enemy_ai.json').read_text())
E = {e['type']: e for e in d['enemies']}
ej = {e['type']: e for e in json.loads((DATA / 'enemies.json').read_text())['enemies']}
undec = [n for n in d['node_types'] if n.get('status') not in ('VERIFIED', 'DISASM', 'INFERRED')]
print(len(undec), 'undecoded; statuses', {n.get('status') for n in d['node_types']})
by_enemy = defaultdict(list)
for n in sorted(undec, key=lambda n: -n['uses']):
    for t in n['enemy_types']:
        by_enemy[t].append(n['create_fn'])
    keys = {k: v for k, v in n.items() if k not in ('create_fn', 'uses', 'enemy_types', 'status')}
    print(n['create_fn'], 'uses', n['uses'], 'types', n['enemy_types'], json.dumps(keys)[:300])
print()
print('== enemies with undecoded nodes ==')
for t in sorted(by_enemy):
    e = ej[t]
    print(hex(t), E[t]['name'], 'hp', e['hp'], 'exp', e['exp'], 'drop', e['drop_script'], 'init', E[t]['initial_state'],
          'nstates', len(E[t]['states']), 'undec', len(by_enemy[t]))
print('== no-drop enemies without undecoded nodes ==')
for t, e in ej.items():
    if e['drop_script'] == '0x0' and t not in by_enemy:
        print(hex(t), e['name'], 'hp', e['hp'], 'init', E[t]['initial_state'], 'nstates', len(E[t]['states']))
