"""fx_beampos.py skill [level] -- beam object position (obj+0x20/0x24) relative to the player, per facing, during
charge (state 0) and firing (state 1): finds the beam object (vtable 0x0804AEF8) in the world lists."""
import sys
from common import *
from combat import setup, pin, REC
from bv_fighters_lib import world_objs

sk = int(sys.argv[1], 0)
lvl = int(sys.argv[2]) if len(sys.argv) > 2 else -1
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
for face in range(4):
    e = emu('buu_fight_swforest.ss')
    e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
    if lvl >= 0:
        e.w8(REC + 0x2e + sk, lvl)
    e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
    p, en = setup(e)
    e.w8(p + 0x10, face)
    seen = {}
    for f in range(60):
        if en:
            e.w32(en[0] + 0x1a0, 0x7000 << 8)
        e.w16(REC + 0x1a, 999)
        e.set_keys(KEY.B)
        e.run(1)
        px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
        for o in world_objs(e):
            if e.r32(o) == 0x0804AEF8:
                st = e.r8(o + 0x10)
                if st not in seen:
                    seen[st] = (f, s(e.r32(o + 0x20)) - px, s(e.r32(o + 0x24)) - py, hex(e.r32(o + 0x4c)), e.r8(o + 0x12))
    print('face', face, seen)
