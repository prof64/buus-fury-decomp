"""Enemy melee timing: frames from the switch of the motion controller (obj+0x14) to the melee hit
(ComputeDamage on the player), and the box that the hit routine 0x0802A0C4 sends (msg+0x14..0x20)."""
import sys
from collections import Counter
from common import *
from bv_regular_trace import trace
from emutools import cur_pc

for t in [int(a, 0) for a in sys.argv[1:]] or [0x44]:
    log = trace(t, 600, 20, 0)
    st = log['states']
    ch = []
    for (f0, c0, *_), (f1, c1, *_) in zip(st, st[1:]):
        if c1 != c0:
            ch.append((f1, hex(c1)))
    offs = Counter()
    for d in log['damage']:
        prev = [c for c in ch if c[0] <= d['f']]
        if prev:
            offs[(prev[-1][1], d['f'] - prev[-1][0])] += 1
    print(hex(t), 'hits', len(log['damage']), 'ok', sum(d['ok'] for d in log['damage']), 'ctrl->hit frame offsets', dict(offs))
    print('   ctrl changes', ch[:12])
