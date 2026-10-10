from common import *
from vtab import entries
e = emu('buu_fight_swforest.ss')
T = 0x0807273c
for i in range(25):
    o = u32(T + 4 * i)
    vt = e.r32(o)
    ents = entries(vt, 6) if 0x08000000 <= vt < 0x08800000 else []
    print(i, hex(o), 'vt', hex(vt), [hex(x) for x in ents])
