"""Dabura (0x12): force the 3-hit counter path (state 0x78 chain) and log states / global 0x030032E4 /
spawns / hits; second run with the player kept far (>100 px) to see the alternative outcome."""
import sys
from bv_fighters_lib import *

T = 0x12
far = len(sys.argv) > 1 and sys.argv[1] == 'far'
glob = lambda e, o: e.r8(0x030032e4)
pp = lambda e, o: tuple(round(v) for v in pos(e, e.r32(PLAYER)))
bp = lambda e, o: tuple(round(v) for v in pos(e, o))
cnt = lambda e, o: e.r32(o + 0x1e4)
p50 = lambda e, o: hex(e.r32(e.r32(PLAYER) + 0x50))


def keepfar(e, o, f):
    if far:
        bx, by = pos(e, o)
        setpos(e, e.r32(PLAYER), bx - 140, by)


e, o, L = fight(T, 1500, dist=40, state=0x78, watch=(glob, cnt, p50), every=keepfar)
print(summarize(L, o))
for w in L.watch:
    print('watch', w)
