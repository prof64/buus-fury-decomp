"""Find GotoState / SubState / PushState node defs targeting given states, and who references those defs.
usage: bv_buu_findgoto.py state [state...]"""
import sys
import numpy as np
from common import *

r = rom()
a = np.frombuffer(r[:len(r) // 4 * 4], dtype=np.uint32)
GOTO = {0x0801890B: 'GotoState', 0x08019285: 'SubState', 0x08018877: 'PushState'}
for s in [int(x, 16) for x in sys.argv[1:]]:
    idx = np.nonzero(a == s)[0]
    for i in idx:
        d = 0x08000000 + 4 * (i - 3)
        fn = int(a[i - 3]) if i >= 3 else 0
        if fn in GOTO:
            refs = np.nonzero(a == d)[0]
            print(f'state {s:#x}: {GOTO[fn]} def @{d:#x} referenced from', [hex(0x08000000 + 4 * j) for j in refs[:8]])
            for j in refs[:4]:
                # print the enclosing words around the reference
                base = 0x08000000 + 4 * (j - 4)
                print('    ctx', hex(base), [hex(int(a[k])) for k in range(j - 4, j + 2)])
