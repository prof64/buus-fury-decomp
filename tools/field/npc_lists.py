"""npc_lists.py W M X Y: object lists after a warp (vtables, positions)"""
import sys
from npc_lib import *
w, m, x, y = (int(a, 0) for a in sys.argv[1:5])
e = emu('buu_snakeway_start.ss')
fix = arm_warp(e, w, m, x, y)
area0 = e.r32(AREA_PTR)
e.set_keys(KEY.RIGHT)
for f in range(600):
    fix()
    e.run(1)
    if e.r32(AREA_PTR) != area0:
        break
e.set_keys(0)
e.run(60)
pl = e.r32(PLAYER)
for L in object_lists(e):
    n = e.r32(L)
    print('list', hex(L), n)
    for k in range(min(n, 256)):
        o = e.r32(L + 4 + 4 * k)
        print('  ', k, hex(o), hex(e.r32(o)), 'PLAYER' if o == pl else '', (s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8))
