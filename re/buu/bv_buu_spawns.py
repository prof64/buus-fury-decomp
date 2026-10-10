"""Spawn records of the Buu-group enemies with their spawn condition (+4: inline value or bytecode ptr)."""
from common import *
from spawns import scan
from scriptdis import dis_script

TYPES = [0x1a, 0x1b, 0x1c, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x3c, 0x3d, 0x04, 0x19, 0x1d]
seen = set()
for s in scan():
    if s['type'] not in TYPES:
        continue
    c = int(s['cond'], 16)
    key = (s['type'], s['world'], s['map'], c)
    if key in seen:
        continue
    seen.add(key)
    line = f"{s['type']:#04x} {s['area'] or '-':24s} w{s['world']:2d} m{s['map']:3d} xy=({s['x']},{s['y']}) gfx={s['gfx']} cond={c:#x}"
    if 0x08000000 <= c < 0x08800000:
        try:
            line += ' :: ' + ' ; '.join(t for _, t in dis_script(c, 40))
        except Exception as ex:
            line += f' (dis fail {ex})'
    print(line)
