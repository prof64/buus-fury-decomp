"""Yakon (0x91): free fight; while in state 0x326 (downward beam) keep the player 60 px below the beam origin
(x+32) to record the beam hits; afterwards count the summoned Mini-Yakons (type 0x92)."""
from bv_fighters_lib import *
from forcespawn import etype
from ents import enemies

cur = {'s': None}


def every(e, o, f):
    if L_ref and L_ref[0].states:
        cur['s'] = L_ref[0].states[-1][1]
    if cur['s'] == 0x326:
        bx, by = pos(e, o)
        setpos(e, e.r32(PLAYER), bx + 32, by + 60)


L_ref = []
import bv_fighters_lib as B
orig = B.Log


class Log2(orig):
    def __init__(self):
        super().__init__()
        L_ref.append(self)


B.Log = Log2
e, o, L = fight(0x91, 1300, dist=60, every=every)
txt = summarize(L, o)
lines = txt.split('\n')
print(lines[0])
beam = [h for h in L.hits if h['sub'] == 11]
print('beam hits', len(beam), 'first', beam[0]['frame'] if beam else None, 'last', beam[-1]['frame'] if beam else None,
      'mult', {h['mult'] for h in beam})
print('other hits', [(h['frame'], h['mult'], h['sub']) for h in L.hits if h['sub'] != 11])
mini = [x for x in enemies(e) if etype(e, x) == 0x92]
print('mini-yakons alive', len(mini), [tuple(round(v) for v in pos(e, x)) for x in mini])
print('yakon pos', pos(e, o))
