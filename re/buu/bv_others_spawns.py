"""Spawn records for group 'others' and their spawn-condition bytecode (rec+4)."""
import json
from common import *
from spawns import scan
from scriptdis import dis_script

g = json.loads((WORK / 'boss_groups.json').read_text())['others']
types = {int(t, 16) for t in g['enemy_types']}
seen = set()
for s in scan():
    if s['type'] not in types:
        continue
    print(hex(s['type']), s['area'], s['world'], s['map'], 'rec', s['record'], 'xy', s['x'], s['y'], 'gfx', s['gfx'],
          'cond', s['cond'], 'extra', s['extra'])
    c = int(s['cond'], 16)
    if 0x08000000 <= c < 0x08800000 and c not in seen:
        seen.add(c)
        try:
            for a, t in dis_script(c, 40):
                print('     ', hex(a), t)
        except Exception as ex:  # noqa
            print('      (cond not a script)', [hex(u32(c + 4 * i)) for i in range(4)], ex)
