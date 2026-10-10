"""findh.py v1 v2 ... -- find 16-bit (and 32-bit) occurrences of each value (aligned), report addresses"""
import sys
import numpy as np
from common import rom
r = rom()
a16 = np.frombuffer(r, dtype=np.uint16)
a32 = np.frombuffer(r, dtype=np.uint32)
for s in sys.argv[1:]:
    v = int(s, 0)
    i16 = np.nonzero(a16 == v)[0]
    i32 = np.nonzero(a32 == v)[0]
    print(f'{v:#x} h:', ' '.join(f'{0x08000000+2*i:08x}' for i in i16[:30]), f'({len(i16)})')
    print(f'{v:#x} w:', ' '.join(f'{0x08000000+4*i:08x}' for i in i32[:30]), f'({len(i32)})')
