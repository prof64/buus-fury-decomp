"""findseq.py : search the ROM for the map-screen frame tilemap row (any base offset, u16 or u8)"""
import sys, struct
import numpy as np
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
row = [0x0373, 0x0374, 0x037b, 0x037c, 0x037d, 0x037d, 0x037d, 0x037d, 0x037d, 0x037e, 0x037f]
d = [row[i + 1] - row[i] for i in range(len(row) - 1)]
r = np.frombuffer(rom()[:len(rom()) // 2 * 2], dtype='<u2').astype(np.int32)
for stride, arr in (('u16', r), ('u8', np.frombuffer(rom(), dtype=np.uint8).astype(np.int32))):
    n = len(arr) - len(row)
    m = np.ones(n, dtype=bool)
    for i, dd in enumerate(d):
        m &= (arr[i + 1:i + 1 + n] - arr[i:i + n]) == dd
    hits = np.nonzero(m)[0][:20]
    print(stride, [(hex(0x08000000 + (h * 2 if stride == 'u16' else h)), int(arr[h])) for h in hits])
