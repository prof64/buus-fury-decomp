"""ea_freeze2.py -- find the world-update caller of the actor updates and the variable that skips a frame after a hit.
Replays 01 near to frame 342, then logs the callers of the enemy update (0x0800FAE6) and the player update."""
import sys
import ea_rec
from common import *
from forcespawn import spawn, setpos
from ents import PLAYER

t, scn, F = 1, 'near', 343
e, o, ok = spawn(t, settle=0)
p = e.r32(PLAYER)
s32v = ea_rec.s32v
bx, by = s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8
px, py = bx - 36, by
setpos(e, p, px, py)
e.w8(p + 0x10, 3)
ids = [e.breakpoint(a) for a in ea_rec.EV]
snap = None
for f in range(F + 2):
    e.w16(0x03001560 + 0x16, e.r16(0x03001560 + 0x18))
    setpos(e, p, px, py)
    e.set_keys(0)
    if f == F - 1:
        snap = e.save()
    e.run(1)
    e.hits()
# compare IWRAM/EWRAM globals between frame 343 (hit) and 342: find words that changed from 0 to small values
e.load(snap)
setpos(e, p, px, py)
e.run(1)
e.hits()
a = e.iwram()
bs = [e.breakpoint(x) for x in (0x0800FAE6,)]
e.run(1)
hs = e.hits()
print('frame344 enemy-update calls', [hex(h['lr']) for h in hs if h['addr'] & ~1 == 0x0800FAE6])
b = e.iwram()
e.run(1)
c = e.iwram()
import struct
for off in range(0, 0x8000, 4):
    x, y, z = (struct.unpack_from('<I', m, off)[0] for m in (a, b, c))
    if x != y and y != z and abs(x - y) < 64 and 0x03000000 + off < 0x03003800:
        print('%08x %d %d %d' % (0x03000000 + off, x, y, z))
