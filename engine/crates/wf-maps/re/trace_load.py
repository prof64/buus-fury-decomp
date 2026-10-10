"""Warp to a map and log every webfoot-LZ decompress (dst, src) and palette RAM writers during the load."""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/tools/extract')
from emu_states import buu_snakeway, BUU_TRIG, BUU_EXIT, BUU_PLAYER, BUU_AREA, KEY

world, area = int(sys.argv[1]), int(sys.argv[2])
e = buu_snakeway()
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
bps = [e.breakpoint(a) for a in (0x08049120, 0x03000040, 0x0800953e)]
wp = e.watch(0x05000000, 0x05000200, 'w')
pal_pcs = {}
for f in range(400):
    p = e.r32(BUU_PLAYER)
    if e.r32(BUU_AREA) == area0 and p:
        e.w16(BUU_EXIT + 8, e.r32(p + 0x1A0) >> 8)
        e.w16(BUU_EXIT + 0xA, e.r32(p + 0x1A4) >> 8)
    e.run(1)
    if e.r32(BUU_AREA) != area0:
        e.set_keys(0)
    for h in e.hits():
        if h['kind'] == 'break':
            r = h['regs']
            print(f'f{f} BREAK pc={h["pc"]:08x} r0={r[0]:08x} r1={r[1]:08x} r2={r[2]:08x} r3={r[3]:08x} lr={r[14]:08x}')
        else:
            k = (h['pc'], h['lr'])
            pal_pcs.setdefault(k, [f, 0, h['addr'], h['addr'], h['regs'][:4]])
            pal_pcs[k][1] += 1
            pal_pcs[k][3] = h['addr']
for (pc, lr), (f, n, a0, a1, regs) in pal_pcs.items():
    print(f'PAL f{f} pc={pc:08x} lr={lr:08x} n={n} {a0:08x}..{a1:08x} regs={[hex(x) for x in regs]}')
print('area', hex(e.r32(BUU_AREA)))
