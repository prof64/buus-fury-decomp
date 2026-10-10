"""fxA_lvl.py skill level hold frames face -- like fx_skill.py but sets the skill level (rec+0x2E/+0x2F), enemy parked."""
import sys
from common import *
from combat import setup, REC
from fxcap import FxIndex, capture

sk, lvl, hold, frames, face = (int(x, 0) for x in sys.argv[1:6])
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
e.w8(REC + 0x2e, lvl); e.w8(REC + 0x2f, lvl)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
for o in en:
    e.w32(o + 0x1a0, 0x7000 << 8); e.w32(o + 0x1a4, 0x7000 << 8)
e.w8(p + 0x10, face)
ix = FxIndex()
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
for f in range(frames):
    e.w16(REC + 0x1a, 999)
    e.set_keys(KEY.B if f < hold else 0)
    e.run(1)
    px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
    cx, cy = s(e.r32(0x030019BC)), s(e.r32(0x030019C0))
    out = [f"{o['fx'][0][0][3:]}#{o['fx'][0][1]}@tl{o['x'] + cx - px},{o['y'] + cy - py}" for o in capture(e, ix, unknown=False) if 'fx' in o]
    if out:
        print(f, e.r32(0x03000f94) % 1000, ' | '.join(out))
