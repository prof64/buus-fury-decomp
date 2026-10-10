import numpy as np
from common import *
e = emu('buu_snakeway_start.ss')
e.run(5)
s0 = e.save()
m = []
for k in (0, KEY.RIGHT, KEY.A, KEY.RIGHT | KEY.A):
    e.load(s0)
    e.hold(k, 1)
    m.append(np.frombuffer(e.iwram() + e.ewram(), dtype=np.uint16).copy())
for i in np.nonzero((m[0] != m[1]) & (m[0] != m[2]))[0]:
    a = 0x03000000 + 2 * i if 2 * i < 0x8000 else 0x02000000 + 2 * i - 0x8000
    print(hex(a), [hex(x[i]) for x in m])
