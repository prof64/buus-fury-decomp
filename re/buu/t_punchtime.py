"""Measure melee attack state durations (frames from A press until back in idle state)."""
from collections import Counter
from common import *
PLAYER = 0x03001c34
IDLE = 0x03002b44
e = emu('buu_swforest3.ss')
p = e.r32(PLAYER)
res = Counter()
for trial in range(40):
    # wait for idle
    for _ in range(60):
        if e.r32(p + 0x14) == IDLE:
            break
        e.run(1)
    e.set_keys(KEY.A)
    e.run(1)
    e.set_keys(0)
    anim = e.r32(p + 0x50)
    n = 1
    st = e.r32(p + 0x14)
    while e.r32(p + 0x14) != IDLE and n < 200:
        e.run(1)
        n += 1
    res[(hex(st), hex(anim), n)] += 1
for k, v in sorted(res.items()):
    print(k, v)
