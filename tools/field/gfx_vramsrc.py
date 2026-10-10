"""gfx_vramsrc.py W A TILE: warp to area W-A and log who writes OBJ VRAM tile TILE (8bpp tile numbering in 32-byte
units: address 0x06010000 + 32*TILE) during the load: pc, lr, registers (DMA writes show the DMA source regs)."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rec_objects as R  # noqa: E402
from rec_objects import emu  # noqa: E402
from warp import warp  # noqa: E402

w, a, tile = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])
e = emu('buu_snakeway_start.ss')
addr = 0x06010000 + 32 * tile
e.watch(addr, addr + 4, 'w')
warp(e, w, a, 280, 290) if len(sys.argv) < 6 else warp(e, w, a, int(sys.argv[4]), int(sys.argv[5]))
seen = set()
for h in e.hits():
    k = (h['pc'], h['lr'])
    if k in seen:
        continue
    seen.add(k)
    print(hex(h['addr']), 'pc', hex(h['pc']), 'lr', hex(h['lr']), 'regs', [hex(r) for r in h['regs'][:8]], 'frame', h['frame'])
# DMA3 registers last values
print('DMA3 SAD', hex(e.r32(0x040000D4)), 'DAD', hex(e.r32(0x040000D8)))
