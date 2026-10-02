"""When does a boss spawn record's 'extra' handler (rec+0x14 -> {0x0800424F|0x0800421F, script}) run?
Kill Pilaf's Guardian (HP set to 1) with a punch and log calls of the two handlers + the drop runner."""
import sys
from common import *
from bv_buu_lib import setup, pos, REC
from forcespawn import setpos
from ents import PLAYER, enemies

t = int(sys.argv[1], 0) if len(sys.argv) > 1 else 0x3f
e, o = setup(t, dx=-14, dy=0)
p = e.r32(PLAYER)
ids = [e.breakpoint(a) for a in (0x0800424E, 0x0800421E, 0x08012588, 0x0800F866, 0x0800F8A0)]
e.hits()
e.w32(o + 0x28c, 1 << 10)
e.w8(p + 0x10, 3)
bx, by = pos(e, o)
for f in range(240):
    e.set_keys(KEY.A if f % 20 == 0 else 0)
    e.w8(p + 0x10, 3)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    e.run(1)
    for h in e.hits():
        print(f, hex(h['addr']), [hex(x) for x in h['regs'][:3]])
print('alive', o in enemies(e))
