"""lzsizes.py SIZE... : 4-aligned LZ containers {type 1|2, size} with given decompressed sizes"""
import sys, struct
import numpy as np
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
r = np.frombuffer(rom()[:len(rom()) // 4 * 4], dtype='<u4')
for s in sys.argv[1:]:
    n = int(s, 16)
    t = r[:-1]
    hits = np.nonzero(((t == 1) | (t == 2)) & (r[1:] == n))[0]
    print(s, len(hits), [hex(0x08000000 + 4 * h) for h in hits[:60]])
