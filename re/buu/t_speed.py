from common import *
from warp import warp
e = emu('buu_snakeway_start.ss')
e.watch(0x02000000, 0x02040000, 'wc')  # too broad; instead find player after warp
e.clear(0)
REC = 0x03001560
print('spd', e.r8(REC + 0x2b), 'eff spd rec+0x54', e.r16(REC + 0x54))
p = e.r32(0x03001c34)
print('player speed field', hex(e.r32(p + 0x238)), hex(e.r32(p + 0x23c)), hex(e.r32(p + 0x240)))
# change SPD and recompute via warp (recreate player)
for spd in (10, 50, 100, 150, 255):
    e2 = emu('buu_snakeway_start.ss')
    e2.w8(REC + 0x2b, spd)
    e2.w16(REC + 0x54, spd)
    e2.watch(e2.r32(0x03001c34) + 0x238, e2.r32(0x03001c34) + 0x23c, 'w')
    warp(e2, 8, 3, 0x80, 0x80, frames=200)
    p2 = e2.r32(0x03001c34)
    hs = e2.hits()
    print(spd, 'speed', e2.r32(p2 + 0x238), e2.r32(p2 + 0x23c), hex(e2.r32(p2 + 0x240)))
