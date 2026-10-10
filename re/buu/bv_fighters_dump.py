"""Dump decoded trees of every state reachable by the fighters-group enemies -> work/buu/fighters_trees.txt"""
import json
import ai2_boot  # noqa
from common import *
from ai_decode import decode, pretty, state_root, initial_state, walk_states

g = json.loads((WORK / 'boss_groups.json').read_text())['fighters']
out = []
for th in g['enemy_types']:
    t = int(th, 16)
    s0 = initial_state(t)
    sts = walk_states(s0)
    out.append(f'===== {th} {g["names"][th]} init {hex(s0)} states {[hex(s) for s in sts]}')
    for s in sts:
        out.append(f'--- state {hex(s)}')
        out.extend(pretty(decode(state_root(s))))
out.append('AUTO nodes owned: ' + ' '.join(g['auto_nodes']))
(WORK / 'fighters_trees.txt').write_text('\n'.join(out))
print(len(out))
