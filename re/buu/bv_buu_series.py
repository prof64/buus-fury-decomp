"""Full per-frame series of changing fields of objects spawned in a forced state.
usage: bv_buu_series.py type dx dy frames state [vt_filter_hex] [nwords]"""
import sys
from common import *
from bv_buu_lib import *

t = int(sys.argv[1], 0)
dx, dy, frames = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
s = int(sys.argv[5], 16)
vtf = int(sys.argv[6], 16) if len(sys.argv) > 6 else None
nw = int(sys.argv[7], 0) if len(sys.argv) > 7 else 0x18
e, o = setup(t, dx=dx, dy=dy)
print('boss', pos(e, o), 'player', pos(e, e.r32(PLAYER)))
force(e, o, s)
r = trace(e, o, frames, nwords=nw)
print('runs', state_runs(r)[:10])
print('dmg', [(d['frame'], d['atk'], d['mult']) for d in r['damage']])
for ob, sn in r['objs'].items():
    if vtf and sn[0][1][0] != vtf:
        continue
    print('obj', hex(ob), 'vt', hex(sn[0][1][0]), 'life', r['life'].get(ob), 'first', sn[0][0])
    for k in range(nw):
        vals = [s32v(w[k]) for _, w in sn]
        if len(set(vals)) > 1 or k < 0:
            print(f'  +{4*k:#04x}', vals)
        else:
            print(f'  +{4*k:#04x} const', hex(vals[0] & 0xffffffff), vals[0])
