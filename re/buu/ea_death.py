"""ea_death.py [type ...] -- what the kill recordings show when the forced enemy's HP runs out: the frame it leaves the
enemy list, the objects spawned, the animation calls of every enemy and the sounds in the next 240 frames."""
import json
import sys
from collections import Counter
from common import *

types = [int(a, 0) for a in sys.argv[1:]] or list(range(150))
for t in types:
    p = WORK / 'eanim' / ('%02x_kill.json' % t)
    if not p.exists():
        continue
    d = json.load(open(p))
    tgt = None
    for k, o in d['objs'].items():
        if o['type'] == t and o['rows'] and (o['rows'][0][8] >> 8) != 0x7000:
            tgt = (k, o)
            break
    if not tgt:
        print('%02x no target' % t)
        continue
    k, o = tgt
    last = o['rows'][-1][0]
    zero = next((r[0] for r in o['rows'] if r[10] == 0), None)
    gone = last + 1 if last < len(d['lt']) - 1 else None
    f0 = zero if zero is not None else gone
    evs = [e for e in d['events'] if f0 is not None and f0 - 1 <= e[0] <= f0 + 240]
    spawns = Counter('%08x@%08x' % (e[3], e[4]) for e in evs if e[1] == 'spawn')
    sfx = [(e[0] - f0, e[3], '%08x' % e[4]) for e in evs if e[1] == 'sfx' and e[4] != 0x0800083E]
    anims = [(e[0] - f0, e[1], e[3] if e[1] != 'state' else '%08x' % e[3], '%08x' % e[4], 'T' if e[2] == int(k, 16) else 'o')
             for e in evs if e[1] in ('anim', 'raw', 'state', 'hold') and e[2] != d.get('pobj')]
    newobjs = [(kk, hex(oo['type']), oo['first'] - (f0 or 0)) for kk, oo in d['objs'].items() if f0 and oo['first'] >= f0 - 1]
    print('%02x hp0 %s gone %s | spawns %s | new enemies %s' % (t, zero, gone, dict(spawns), newobjs[:6]))
    print('    sfx', sfx[:12])
    print('    anim', [a for a in anims if a[1] != 'state'][:14])
