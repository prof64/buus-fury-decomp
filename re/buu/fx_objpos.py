"""fx_objpos.py skill vtable [hold] [frames] [level] -- per facing: spawn frame/tick and position (obj+0x20/0x24 >> 8)
relative to the player position of every object with that vtable, plus the first frame it moves."""
import sys
from common import *
from combat import setup, REC
from bv_fighters_lib import world_objs

sk = int(sys.argv[1], 0)
vt = int(sys.argv[2], 16)
hold = int(sys.argv[3]) if len(sys.argv) > 3 else 60
frames = int(sys.argv[4]) if len(sys.argv) > 4 else 80
lvl = int(sys.argv[5]) if len(sys.argv) > 5 else -1
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
for face in range(4):
    e = emu('buu_fight_swforest.ss')
    e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
    if lvl >= 0:
        e.w8(REC + 0x2e + sk, lvl)
    e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
    p, en = setup(e)
    e.w8(p + 0x10, face)
    t0 = e.r32(0x03000F94)
    seen = {}
    for f in range(frames):
        if en:
            e.w32(en[0] + 0x1a0, 0x7000 << 8)
        e.w16(REC + 0x1a, 999)
        e.set_keys(KEY.B if f < hold else 0)
        e.run(1)
        px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
        tick = e.r32(0x03000F94) - t0
        for o in world_objs(e):
            if e.r32(o) == vt:
                x, y = s(e.r32(o + 0x20)) >> 8, s(e.r32(o + 0x24)) >> 8
                if o not in seen:
                    seen[o] = {'f': f, 'tick': tick, 'rel': (x - px, y - py), 'pos': (x, y)}
                elif 'moved' not in seen[o] and (x, y) != seen[o]['pos']:
                    seen[o]['moved'] = (f, tick, x - px, y - py)
    print('face', face, list(seen.values())[:4])
