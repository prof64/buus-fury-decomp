from common import *
from vtab import entries
known = {0x0800e1af: 'apply_normal', 0x0800e2f3: 'apply_SS(+5,+128)', 0x0800e345: 'apply_SS_b(+5,+128)',
         0x0800e471: 'apply_SS3(+15,+192)', 0x0800e551: 'apply_speed/4', 0x0800e5c1: 'apply_speed+64',
         0x0800e1d1: 'vitals_normal', 0x0800e4c5: 'vitals_SS?', 0x0800e639: 'vitals_?'}
e = emu('buu_snakeway_start.ss')
for slot in range(0x03001b60, 0x03001b90, 4):
    vt = e.r32(slot)
    ents = entries(vt, 12)
    tags = [known.get(x, hex(x)) for x in ents]
    print(hex(slot), hex(vt), tags)
