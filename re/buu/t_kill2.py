from common import *
from ents import enemies, info
REC = 0x03001560
e = emu('buu_fight_swforest.ss')
for o in enemies(e):
    e.w32(o + 0x28c, 1 << 10)
e.watch(0x087119e4, 0x087119fc, 'r')
for f in range(60):
    e.set_keys(KEY.A if f % 8 < 4 else 0)
    e.run(1)
for h in e.hits():
    print(h['frame'], hex(h['pc']), hex(h['lr']), hex(h['addr']), [hex(x) for x in h['regs'][:8]])
