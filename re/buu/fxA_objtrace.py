"""fxA_objtrace.py skill hold frames face vtable[,vtable..] [fields] -- per frame: world objects with the given vtables:
pos (+0x20/+0x24 >>8), record index rel. to fx table (+0x18), and extra u32 fields (hex offsets, comma list)."""
import sys
from common import *
from combat import setup, pin, REC
from bv_fighters_lib import world_objs

sk = int(sys.argv[1], 0)
hold = int(sys.argv[2])
frames = int(sys.argv[3])
face = int(sys.argv[4])
vts = [int(v, 16) for v in sys.argv[5].split(',')]
fields = [int(v, 16) for v in sys.argv[6].split(',')] if len(sys.argv) > 6 and sys.argv[6] else []
release_at = int(sys.argv[7]) if len(sys.argv) > 7 else 10 ** 9
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, face)
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
dist = 200
dx, dy = {3: (dist, 0), 2: (-dist, 0), 0: (0, dist), 1: (0, -dist)}[face]
for f in range(frames):
    if en:
        pin(e, en[0], p, dx, dy)
        e.w32(en[0] + 0x28c, 9999 << 10)
    e.w16(REC + 0x1a, 999)
    e.set_keys(KEY.B if f < hold else 0)
    e.run(1)
    px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
    out = []
    for o in world_objs(e):
        if e.r32(o) in vts:
            x, y = s(e.r32(o + 0x20)) >> 8, s(e.r32(o + 0x24)) >> 8
            out.append(f"{o & 0xffff:04x}:vt{e.r32(o) & 0xffff:04x} @{x - px},{y - py} rec={e.r32(o + 0x18):08x} " +
                       ' '.join(f"+{k:x}={e.r32(o + k):x}" for k in fields))
    print(f, e.r32(0x03000f94) % 1000, 'pst', e.r8(p + 0x11), '|', ' | '.join(out))
