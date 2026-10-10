"""Broly 0x0b jump smash (state 0x4c): watch controller (obj+0x14), anim ptr (obj+0x50), height (obj+0x176) and
positions, to see why state 0x4e stalls after landing in some runs.  arg: player distance."""
import sys
from bv_fighters_lib import *

d = float(sys.argv[1]) if len(sys.argv) > 1 else 90
ctrl = lambda e, o: hex(e.r32(o + 0x14))
anim = lambda e, o: hex(e.r32(o + 0x50))
z = lambda e, o: e.r16(o + 0x176)
fr = lambda e, o: e.r16(o + 0x12)
e, o, L = fight(0x0b, 700, dist=d, state=0x4c, watch=(ctrl, anim, z))
print(summarize(L, o).split('\n')[0])
for w in [w for w in L.watch if w[0] > 230][:40]:
    print('watch', w)
S = Shots(e, 'broly2_stall')
S.snap()
S.write()
