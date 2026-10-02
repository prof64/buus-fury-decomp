"""Dabura: player punches (A every 12 frames, facing right toward Dabura) -- what increments obj+0x1E4
(the 'IfCounterEq 3 -> sphere' counter)?  Also a watchpoint on obj+0x1E4 writes."""
from bv_fighters_lib import *

cnt = lambda e, o: e.r32(o + 0x1e4)
hpw = lambda e, o: e.r32(o + 0x28c) >> 10


def setup(e, o):
    e.w8(e.r32(PLAYER) + 0x10, 3)    # face right


keys = lambda f: KEY.A if f % 12 < 2 else 0
e, o, L = fight(0x12, 900, dist=22, keys=keys, setup=setup, watch=(cnt, hpw))
print(summarize(L, o).split('\n')[0])
for w in L.watch:
    print('watch', w)
print('hits by player', sum(1 for h in L.hits if h['target'] == o))
