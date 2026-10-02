"""fxA_ghost.py hold frames dist -- Kamikaze Ghost: ghosts' sprite def / anim / pos per frame, effect spawns."""
import sys
from common import *
from combat import setup, pin, REC
from bv_fighters_lib import world_objs

hold, frames, dist = (int(x) for x in sys.argv[1:4])
DEFS = 0x086B6BD8
defs = {u32(DEFS + 4 * i): i for i in range(298)}
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, 13); e.w8(REC + 0x15, 0)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, 2)
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
bids = [e.breakpoint(a) for a in (0x0801952c, 0x0802a81c)]
seen = {}
for f in range(frames):
    if en and f < hold:
        pin(e, en[0], p, -dist, 0); e.w32(en[0] + 0x28c, 9999 << 10)
    e.w16(REC + 0x1a, 999)
    e.set_keys(KEY.B if f < hold else 0)
    e.run(1)
    px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
    out = []
    for o in world_objs(e):
        d = e.r32(o + 0x58)
        if d in defs and defs[d] in (148, 156, 35) or e.r32(o + 0x224) == 0x08711274 + 28 * 0x54:
            a = e.r32(o + 0x50)
            out.append(f"g{o & 0xffff:04x} def{defs.get(d)} anim{a:x} dir{e.r8(o + 0x10)} fr{e.r8(o + 0x11)} "
                       f"@{(s(e.r32(o + 0x1a0)) >> 8) - px},{(s(e.r32(o + 0x1a4)) >> 8) - py} st{e.r32(o + 0x238):x}")
    hs = [f"FX lr{h['lr']:x} r2={h['regs'][2]:x}" for h in e.hits()]
    line = ' | '.join(out + hs)
    if line != seen.get('l') or f % 25 == 0:
        print(f, e.r32(0x03000f94) % 1000, line)
        seen['l'] = line
