"""obj_bcast.py kind -- call sites of broadcast 0x08011C70 (and the first-acceptor variant) with r1 = kind set nearby"""
import sys
from common import *
R = rom()
kind = int(sys.argv[1], 0)
targets = {0x08011C70, 0x08011CB4, 0x08011CF8}
for off in range(0, 0x80000, 2):
    h1 = R[off] | R[off + 1] << 8
    h2 = R[off + 2] | R[off + 3] << 8
    if (h1 & 0xF800) == 0xF000 and (h2 & 0xF800) == 0xF800:
        o = ((h1 & 0x7FF) << 12) | ((h2 & 0x7FF) << 1)
        if o & 0x400000:
            o -= 0x800000
        dst = 0x08000000 + off + 4 + o
        if dst in targets:
            for back in range(2, 20, 2):
                b = R[off - back] | R[off - back + 1] << 8
                if b == (0x2100 | kind):
                    print(f'bl {dst:08x} at {0x08000000 + off:08x} (movs r1,#{kind} at -{back})')
                    break
