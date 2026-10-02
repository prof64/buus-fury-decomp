from common import *
import sys
T=0x83b5c00
ops=[int(x,16) for x in sys.argv[1:]]
for op in ops:
    h=u32(T+4*op)
    print(f'=== op {op:#x} handler {h:#x}')
    if h: print(dis(h&~1, int(14)))
