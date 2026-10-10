"""findobj.py state vtable [vtable...] -- find EWRAM/IWRAM objects whose first word is a vtable"""
import sys
import numpy as np
from common import *
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
for base, b in ((0x02000000, e.ewram()), (0x03000000, e.iwram())):
    a = np.frombuffer(b, dtype=np.uint32)
    for s in sys.argv[2:]:
        v = int(s, 16)
        for i in np.nonzero(a == v)[0]:
            o = base + 4 * i
            print(f'{s} obj {o:08x}:', ' '.join(f'{x:08x}' for x in a[i:i + 8]))
