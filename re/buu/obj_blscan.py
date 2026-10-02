"""obj_blscan.py addr... -- find Thumb BL call sites and literal-pool references to addresses"""
import sys
from common import *
R = rom()
targets = [int(a, 16) & ~1 for a in sys.argv[1:]]
for t in targets:
    print(f'== {t:08x}')
    for off in range(0, min(len(R), 0x800000) - 4, 2):
        h1 = R[off] | R[off + 1] << 8
        h2 = R[off + 2] | R[off + 3] << 8
        if (h1 & 0xF800) == 0xF000 and (h2 & 0xF800) == 0xF800:
            o = ((h1 & 0x7FF) << 12) | ((h2 & 0x7FF) << 1)
            if o & 0x400000:
                o -= 0x800000
            dst = 0x08000000 + off + 4 + o
            if dst == t:
                print(f'  bl from {0x08000000 + off:08x}')
    for off in range(0, len(R) - 4, 4):
        v = R[off] | R[off + 1] << 8 | R[off + 2] << 16 | R[off + 3] << 24
        if v in (t, t | 1):
            print(f'  word at {0x08000000 + off:08x}')
