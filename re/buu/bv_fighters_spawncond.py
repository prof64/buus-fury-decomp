"""Spawn records + spawn-condition bytecode (record +4) for the fighters-group enemies
-> work/buu/fighters_spawncond.txt"""
import json
from common import *
from spawns import scan
from scriptdis import dis_script

g = json.loads((WORK / 'boss_groups.json').read_text())['fighters']
types = {int(x, 16) for x in g['enemy_types']}
out = []
for s in scan():
    if s['type'] not in types:
        continue
    out.append(f"type {s['type']:#x} area {s['area']!r} w{s['world']} m{s['map']} rec {s['record']} pos ({s['x']},{s['y']}) "
               f"gfx {s['gfx']} cond {s['cond']} extra {s['extra']}")
    c = int(s['cond'], 16)
    if 0x08000000 <= c < 0x08800000:
        try:
            for a, t in dis_script(c, 40):
                out.append(f'    {a:#x} {t}')
        except Exception as ex:  # noqa
            out.append(f'    (dis failed {ex!r})')
(WORK / 'fighters_spawncond.txt').write_text('\n'.join(out))
print('\n'.join(out))
