"""xref.py addr [addr...] -- find 32-bit words equal to addr (or addr|1) in ROM"""
import sys
import numpy as np
from common import rom
r = rom()
a = np.frombuffer(r, dtype=np.uint32)
for s in sys.argv[1:]:
    v = int(s, 16)
    for vv in {v, v | 1}:
        idx = np.nonzero(a == vv)[0]
        print(f'{vv:08x}:', ' '.join(f'{0x08000000 + 4 * i:08x}' for i in idx[:40]), f'({len(idx)})')
