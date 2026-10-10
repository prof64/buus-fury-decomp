"""yakon_hpwatch.py [skill] [dist] [frames] -- force-spawned Yakon, player fires skill (default 7 Energy Blast)
every 48 f from `dist` px; print every write to his HP word (obj+0x28C) with pc / lr."""
import sys
from common import *
from forcespawn import setpos
from ents import PLAYER
from bv_fighters_lib import spawn2, pos, REC

sk = int(sys.argv[1]) if len(sys.argv) > 1 else 7
dist = int(sys.argv[2]) if len(sys.argv) > 2 else 50
frames = int(sys.argv[3]) if len(sys.argv) > 3 else 240
e, o, ok = spawn2(0x91)
p = e.r32(PLAYER)
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
wid = e.watch(o + 0x28c, o + 0x290, 'w')
for f in range(frames):
    e.w16(REC + 0x16, 9999); e.w16(REC + 0x1a, 999)
    bx, by = pos(e, o)
    setpos(e, p, bx - dist, by)
    e.w8(p + 0x10, 3)
    e.set_keys(KEY.B if f % 48 < 2 else 0)
    e.run(1)
    for h in e.hits():
        if h['kind'] == 'watch' and h['new'] != h['old']:
            r = h['regs']
            print(f, hex(h['addr']), h['old'] / 1024, '->', h['new'] / 1024, 'pc', hex(r[15]), 'lr', hex(r[14] & 0xffffffff))
