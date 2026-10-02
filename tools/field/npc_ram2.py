"""npc_ram2.py: vtables of AI node singletons (RunMotionController 0x03002FEC etc.) in 14-9."""
import sys
from npc_lib import *
from npc_re import vt

e = emu('buu_snakeway_start.ss')
fix = arm_warp(e, 14, 9, 0x40, 0x100)
a0 = e.r32(AREA_PTR)
e.set_keys(KEY.RIGHT)
for f in range(600):
    fix()
    e.run(1)
    if e.r32(AREA_PTR) != a0:
        break
e.set_keys(0)
e.run(10)
for s in [int(a, 16) for a in sys.argv[1:]] or [0x03002FEC, 0x03002FE8, 0x03002FF0]:
    v = e.r32(s)
    print(hex(s), 'vt', hex(v), [hex(x) for x in vt(v, 8)])
