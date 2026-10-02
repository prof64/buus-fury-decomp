"""Reach scan: force <state> with the player at dx (same row) and count damage events.
usage: bv_buu_reach.py type state frames dy dx [dx...]"""
import sys
from collections import Counter
from common import *
from bv_buu_lib import *

t, s, frames, dy = int(sys.argv[1], 0), int(sys.argv[2], 16), int(sys.argv[3]), int(sys.argv[4])
dxs = [int(a) for a in sys.argv[5:]]
e, o = setup(t, dx=200, dy=0)
base = e.save()
bx, by = pos(e, o)
p = e.r32(PLAYER)
for dx in dxs:
    e.load(base)
    setpos(e, p, bx + dx, by + dy)
    force(e, o, s)
    r = trace(e, o, frames)
    fr = [d['frame'] for d in r['damage'] if d['target'] == hex(p)]
    c = Counter((d['atk'], d['mult']) for d in r['damage'] if d['target'] == hex(p))
    hl = sum(l for f, l, g, cc, v in r['player'])
    print(f'dx {dx:4d} dy {dy:4d}: hits {len(fr):3d} first {fr[:1]} last {fr[-1:]} types {dict(c)} hp_lost {hl}')
