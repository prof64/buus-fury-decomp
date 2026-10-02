from common import *
from warp import warp
REC = 0x03001560
e = emu('buu_snakeway_start.ss')
e.watch(REC + 0x54, REC + 0x56, 'r')
warp(e, 8, 3, 0x80, 0x80, frames=200)
seen = set()
for h in e.hits():
    if h['pc'] in seen:
        continue
    seen.add(h['pc'])
    print(hex(h['pc']), hex(h['lr']), [hex(x) for x in h['regs'][:8]])
