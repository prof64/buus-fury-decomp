from common import *
import vtab
e = emu('buu_snakeway_start.ss')
e.run(1)
for a in (0x03001918, 0x03002f4c):
    vt = e.r32(a)
    print(hex(a), 'vt', hex(vt), [hex(e.r32(a + 4 * i)) for i in range(1, 4)])
    if 0x08000000 <= vt < 0x08800000:
        ents = vtab.entries(vt, 4)
        for i, f in enumerate(ents):
            print(' ', i, hex(f))
            if i == 2:
                print(dis(f & ~1, 30))
