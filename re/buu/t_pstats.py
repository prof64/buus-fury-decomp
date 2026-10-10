from common import *
from emutools import catch
e = emu('buu_fight_swforest.ss')
p = e.r32(0x03001c34)
e.watch(p + 0x1b0, p + 0x1c4, 'w')
for f in range(60):
    e.run(1)
hs = e.hits()
seen = set()
for h in hs:
    k = (h['pc'], h['addr'] - p)
    if k in seen:
        continue
    seen.add(k)
    print(hex(h['pc']), hex(h['addr'] - p), h['new'], [hex(x) for x in h['regs'][:6]])
