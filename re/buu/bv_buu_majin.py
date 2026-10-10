"""Majin Buu family: force attack states and measure timing, projectiles, damage, player/boss effects.
usage: bv_buu_majin.py type dx dy frames state [state ...]"""
import sys
import json
from common import *
from bv_buu_lib import *

t = int(sys.argv[1], 0)
dx, dy, frames = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
states = [int(a, 16) for a in sys.argv[5:]]
e, o = setup(t, dx=dx, dy=dy)
print('type', hex(t), 'obj', hex(o) if o else None, 'boss pos', pos(e, o), 'player', pos(e, e.r32(PLAYER)),
      'boss hp', e.r32(o + 0x28c) >> 10)
base = e.save()
for s in states:
    e.load(base)
    e.w32(o + 0x28c, (e.r32(o + 0x28c) >> 1))     # half HP so heals are visible
    force(e, o, s)
    r = trace(e, o, frames)
    print(f'=== forced state {s:#x}')
    print(' runs', state_runs(r)[:14])
    print(' spawns', r['spawns'][:12])
    for ob, sn in list(r['objs'].items())[:4]:
        life = r['life'].get(ob)
        print('  obj', hex(ob), 'vt', hex(sn[0][1][0]), 'first', sn[0][0], 'life', life)
        mf = moving_fields(sn)
        print('   moving', json.dumps(mf)[:700])
    for d in r['damage'][:10]:
        print(' dmg', d)
    pl = [x for x in r['player'] if x[1] or x[2]]
    print(' player hp-loss/global/ctrl (non-zero frames):', [(f, l, g, c) for f, l, g, c, v in pl][:40])
    hp = [(f, h) for f, x, y, a, c, cf, h in r['boss']]
    ch = [hp[0]] + [b for a, b in zip(hp, hp[1:]) if a[1] != b[1]]
    print(' boss hp changes', ch[:20])
    mv = [(f, round(x, 2), round(y, 2)) for f, x, y, *_ in r['boss'][::10]]
    print(' boss pos /10f', mv[:26])
