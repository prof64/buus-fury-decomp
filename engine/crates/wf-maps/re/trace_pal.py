"""Warp to a map; find the RGB888 palette buffer used by the fade routine and who writes it during the load."""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/tools/extract')
from emu_states import buu_snakeway, BUU_TRIG, BUU_EXIT, BUU_PLAYER, BUU_AREA, KEY

world, area = int(sys.argv[1]), int(sys.argv[2])
e = buu_snakeway()
b = e.breakpoint(0x080070a0)
e.run(2)
hs = e.hits()
e.clear(b)
buf = hs[0]['regs'][4] if hs else None
print('fade buffer', hex(buf) if buf else None, len(hs))
for i in range(0, 0x20, 4):
    e.w32(BUU_EXIT + i, 0)
e.w8(BUU_EXIT + 0x10, 2)
e.w8(BUU_EXIT + 0x11, world)
e.w8(BUU_EXIT + 0x12, area)
e.w16(BUU_EXIT + 0x14, 0x100)
e.w16(BUU_EXIT + 0x16, 0x100)
e.w32(BUU_TRIG, 0x08049BC4)
e.w8(BUU_TRIG + 0x10, 2)
e.w8(BUU_TRIG + 0x11, 0xFF)
e.w32(BUU_TRIG + 0x14, BUU_EXIT)
p = e.r32(BUU_PLAYER)
e.w32(p + 0x1A0, 0x160 << 8)
e.w32(p + 0x1A4, 0xA0 << 8)
area0 = e.r32(BUU_AREA)
e.set_keys(KEY.RIGHT)
# buffer: 512 colours x 3 bytes? watch 0x600 bytes from the first fade input
w = e.watch(buf - 0x600, buf + 0x600, 'w')
seen = {}
for f in range(120):
    p = e.r32(BUU_PLAYER)
    if e.r32(BUU_AREA) == area0 and p:
        e.w16(BUU_EXIT + 8, e.r32(p + 0x1A0) >> 8)
        e.w16(BUU_EXIT + 0xA, e.r32(p + 0x1A4) >> 8)
    e.run(1)
    if e.r32(BUU_AREA) != area0:
        e.set_keys(0)
    for h in e.hits():
        k = (h['pc'], h['lr'])
        if k not in seen:
            seen[k] = [f, 0, h['addr'], h['addr'], [hex(x) for x in h['regs'][:8]]]
        seen[k][1] += 1
        seen[k][3] = h['addr']
for (pc, lr), v in seen.items():
    print(f'pc={pc:08x} lr={lr:08x} first f{v[0]} n={v[1]} {v[2]:08x}..{v[3]:08x} {v[4]}')
