"""Where do enemy drops get executed? Kill one enemy, then trace every call into the script runner and
drop natives, plus Zenie, for 900 frames; take periodic screenshots."""
from common import *
from ents import enemies
e = emu('buu_fight_swforest.ss')
en = enemies(e)
o = en[0]
p = e.r32(0x03001c34)
e.w32(o + 0x28c, 1 << 10)
e.w8(p + 0x10, 3)
e.w32(o + 0x1a0, e.r32(p + 0x1a0) + (18 << 8)); e.w32(o + 0x1a4, e.r32(p + 0x1a4))
ids = [e.breakpoint(a) for a in (0x08004098, 0x08004038, 0x0800253c, 0x08002556, 0x080025a4, 0x0800327e, 0x0800324a)]
S = Shots(e, 'loot3')
z0 = e.r32(0x0300155c)
for f in range(900):
    e.set_keys(KEY.A if f < 2 else 0)
    e.run(1)
    if f % 100 == 10:
        S.snap()
S.write()
seen = 0
for h in e.hits():
    print(h['frame'], hex(h['pc']), [hex(x) for x in h['regs'][:4]])
    seen += 1
    if seen > 40:
        break
print('zenie', z0, e.r32(0x0300155c), 'enemies', len(enemies(e)))
