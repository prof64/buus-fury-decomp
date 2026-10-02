"""npc_ram.py: RAM facts in Yemma's office: state singleton vtables, grid pointers, combat flag."""
from npc_lib import *
from npc_re import vt

e = emu('buu_snakeway_start.ss')
fix = arm_warp(e, 1, 3, 240, 416)
area0 = e.r32(AREA_PTR)
e.set_keys(KEY.RIGHT)
for f in range(600):
    fix()
    e.run(1)
    if e.r32(AREA_PTR) != area0:
        break
e.set_keys(0)
e.run(60)
for s in (0x03001B0C, 0x03001B58, 0x03001AF0, 0x03001AF4):
    v = e.r32(s)
    print('singleton', hex(s), 'vt', hex(v), [hex(x) for x in vt(v, 4)])
for a in (0x080716E8, 0x080716F0, 0x086FC238):
    p = e.r32(u32(a))
    base = u32(a)
    print('ptr', hex(a), '->', hex(base), 'grids +0xA8', hex(e.r32(base + 0xA8)), '+0xB0', hex(e.r32(base + 0xB0)))
print('combat flag [0x083c33f8+0xc]+0xc', hex(u32(0x083C33F8 + 0xC)), e.r32(u32(0x083C33F8 + 0xC) + 0xC))
