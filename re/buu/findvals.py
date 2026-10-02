"""findvals.py state v1 v2 ... -- find 8/16/32-bit sequences of values within a 64-byte window"""
import sys
import numpy as np
from common import *
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
vals = [int(x, 0) for x in sys.argv[2:]]
mem = {0x02000000: e.ewram(), 0x03000000: e.iwram()}
for base, b in mem.items():
    for w in (1, 2, 4):
        arr = np.frombuffer(b[: len(b) // w * w], dtype={1: np.uint8, 2: np.uint16, 4: np.uint32}[w])
        pos = [set(np.nonzero(arr == v)[0] * w) for v in vals]
        for p in sorted(pos[0]):
            ok = all(any(abs(q - p) <= 64 for q in ps) for ps in pos[1:])
            if ok:
                print(f'w{w} {base + p:08x}', [hex(base + q) for ps in pos[1:] for q in sorted(ps) if abs(q - p) <= 64][:12])
