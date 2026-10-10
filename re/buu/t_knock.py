"""Single punch / single ki blast on an enemy placed in front: track enemy position, resist, AI state
for 60 frames (knockback distance + stun duration)."""
import sys
from common import *
from ents import enemies
REC = 0x03001560
mode = sys.argv[1] if len(sys.argv) > 1 else 'A'
e = emu('buu_swforest3.ss')
p = e.r32(0x03001c34)
en = enemies(e)
o = en[0]
for x in en[1:]:
    e.w32(x + 0x1a0, 0x7000 << 8)
e.w8(p + 0x10, 3)
px, py = e.r32(p + 0x1a0), e.r32(p + 0x1a4)
e.w32(o + 0x1a0, px + (18 << 8)); e.w32(o + 0x1a4, py)
e.w16(REC + 0x1a, 90)
k = KEY.A if mode == 'A' else KEY.B
hp0 = e.r32(o + 0x28c)
prev = None
for f in range(70):
    e.set_keys(k if f < 2 else 0)
    e.run(1)
    st = (e.r32(o + 0x1a0) >> 8, e.r32(o + 0x1a4) >> 8, e.r8(o + 0x1b8), e.r32(o + 0x28c) >> 10)
    if st != prev:
        print(f, st)
        prev = st
