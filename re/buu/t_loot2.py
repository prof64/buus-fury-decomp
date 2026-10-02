from common import *
from ents import enemies
e = emu('buu_fight_swforest.ss')
en = enemies(e)
o = en[0]
p = e.r32(0x03001c34)
for x in en[1:]:
    pass
e.w32(o + 0x28c, 1 << 10)
e.w8(p + 0x10, 3)
e.w32(o + 0x1a0, e.r32(p + 0x1a0) + (18 << 8)); e.w32(o + 0x1a4, e.r32(p + 0x1a4))
ids = [e.breakpoint(a) for a in (0x08004098, 0x0800253c, 0x08002556, 0x080025a4, 0x0800327e, 0x0800f8a0, 0x0800f866)]
for f in range(700):
    e.set_keys(KEY.A if f < 2 else 0)
    e.run(1)
for h in e.hits():
    print(h['frame'], hex(h['pc']), [hex(x) for x in h['regs'][:4]])
print('enemies', len(enemies(e)), len(en))
