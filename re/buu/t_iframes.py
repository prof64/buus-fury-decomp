"""Measure the player's post-hit invulnerability window (resist bytes obj+0x1B8/0x1B9 == 0) and the
state sequence after taking a hit; also enemy post-hit behaviour (knockback distance, stun)."""
from common import *
REC = 0x03001560
e = emu('buu_fight_swforest.ss')
p = e.r32(0x03001c34)
hp = e.r16(REC + 0x16)
log = []
for f in range(400):
    e.run(1)
    st = e.r32(p + 0x14)
    log.append((f, e.r16(REC + 0x16), e.r8(p + 0x1b8), hex(st), e.r32(p + 0x1a0) >> 8, e.r32(p + 0x1a4) >> 8))
    e.w16(REC + 0x16, max(e.r16(REC + 0x16), 50))
prev = None
for l in log:
    key = (l[2], l[3])
    if key != prev:
        print(l)
        prev = key
