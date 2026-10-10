"""snakeway: walk right into the s039 touch_once text trigger; watch the trigger list control words"""
from common import *
import obj_list
e = emu('buu_snakeway_start.ss')
L = 0x03002A64
P = e.r32(0x03001c34)
print('ctl', [hex(e.r32(L + 0xC0 + 4 * i)) for i in range(5)])
e.watch(L + 0xC0, L + 0xD4, 'w')
b1 = e.breakpoint(0x0801279c)
seen = 0
for f in range(400):
    e.set_keys(KEY.RIGHT if f < 200 else (KEY.A if f % 20 == 0 else 0))
    e.run(1)
    for h in e.hits():
        if h['kind'] == 'watch' and h['addr'] in (L + 0xC4,) and seen > 3:
            continue
        if h['kind'] == 'watch' and h['addr'] == L + 0xC4:
            seen += 1
        print(f, h['kind'], hex(h['addr']), hex(h['old']), '->', hex(h['new']), 'pc', hex(h['pc']), 'r1', hex(h['regs'][1]), 'player', e.r32(P + 0x1a0) >> 8, e.r32(P + 0x1a4) >> 8)
print('ctl', [hex(e.r32(L + 0xC0 + 4 * i)) for i in range(5)])
obj_list.dump(e)
