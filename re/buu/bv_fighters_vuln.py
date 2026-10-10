"""bv_fighters_vuln.py type [frames] [donor] -- player punches (A every 10 f, re-faced toward the boss each
frame, kept 20 px left of it); log boss HP changes, ComputeDamage events on the boss, the boss's obj+0x1CC
flag and states: which states let damage through?"""
import sys
from bv_fighters_lib import *

t = int(sys.argv[1], 0)
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 900
donor = int(sys.argv[3], 0) if len(sys.argv) > 3 else None
flag = lambda e, o: e.r32(o + 0x1cc)
hpw = lambda e, o: e.r32(o + 0x28c) >> 10


def stick(e, o, f):
    bx, by = pos(e, o)
    p = e.r32(PLAYER)
    setpos(e, p, bx - 18, by)
    e.w8(p + 0x10, 3)


keys = lambda f: KEY.A if f % 10 < 2 else 0
e, o, L = fight(t, frames, dist=18, keys=keys, every=stick, watch=(flag, hpw), donor=donor)
print(summarize(L, o).split('\n')[0][:1500])
for w in L.watch[:80]:
    print('watch', w)
ph = [h for h in L.hits if h['target'] == o]
print('player->boss ComputeDamage calls', len(ph), [h['frame'] for h in ph][:40])
