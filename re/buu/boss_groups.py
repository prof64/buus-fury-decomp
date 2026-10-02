"""Split the boss / undecoded-AI work into groups: enemy types per group and the AUTO (not hand-decoded)
node types each group owns (first group, in GROUP order, whose enemies use the node).
Writes work/buu/boss_groups.json."""
import json
from common import *

GROUPS = dict(
    buu=[0x1a, 0x1b, 0x1c, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x3c, 0x3d, 0x04, 0x19, 0x1d],
    janemba=[0x33, 0x7a, 0x38, 0x39, 0x36, 0x37, 0x34, 0x35, 0x8d],
    fighters=[0x0a, 0x0b, 0x12, 0x46, 0x88, 0x89, 0x60, 0x91, 0x92, 0x6d, 0x6e, 0x6f, 0x70, 0x93, 0x94, 0x0d, 0x0e,
              0x22, 0x72, 0x7e],
    others=[0x00, 0x06, 0x18, 0x3f, 0x50, 0x51, 0x81, 0x8e, 0x8f, 0x90, 0x64, 0x5b, 0x23, 0x25, 0x26, 0x27, 0x28,
            0x2d, 0x31, 0x3b, 0x55, 0x5a, 0x82, 0x84, 0x87, 0x54, 0x07],
)
ORDER = ['buu', 'janemba', 'fighters', 'others', 'regular']

if __name__ == '__main__':
    d = json.loads((DATA / 'enemy_ai.json').read_text())
    names = {e['type']: e['name'] for e in d['enemies']}
    allg = {t for v in GROUPS.values() for t in v}
    groups = dict(GROUPS, regular=[t for t in range(150) if t not in allg])
    own = {g: [] for g in ORDER}
    for n in d['node_types']:
        if n['status'] != 'AUTO':
            continue
        ts = set(n['enemy_types'])
        g = next((g for g in ORDER if ts & set(groups[g])), 'regular')
        own[g].append(n['create_fn'])
    out = {g: dict(enemy_types=[hex(t) for t in groups[g]], names={hex(t): names.get(t) for t in groups[g]},
                   auto_nodes=own[g]) for g in ORDER}
    (WORK / 'boss_groups.json').write_text(json.dumps(out, indent=1))
    for g in ORDER:
        print(g, len(groups[g]), 'enemies', len(own[g]), 'auto nodes')
