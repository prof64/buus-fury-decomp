"""findseq.py v0 step count -- find 16-bit values v0, v0+step, ... at a constant stride in ROM"""
import sys
import numpy as np
from common import rom
v0, step, cnt = int(sys.argv[1], 0), int(sys.argv[2], 0), int(sys.argv[3])
a = np.frombuffer(rom(), dtype=np.uint16)
pos0 = np.nonzero(a == v0)[0]
for p in pos0:
    for stride in range(1, 64):
        ok = True
        for k in range(1, cnt):
            q = p + stride * k
            if q >= len(a) or a[q] != v0 + step * k:
                ok = False
                break
        if ok:
            print(f'{0x08000000 + 2 * p:08x} stride {2 * stride} bytes')
