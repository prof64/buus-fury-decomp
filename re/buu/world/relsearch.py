"""Relative search: find words under any additive byte (or u16) encoding."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
import numpy as np
r = np.frombuffer(rom(), dtype=np.uint8).astype(np.int16)
for w in sys.argv[1:]:
    d = [ord(w[i + 1]) - ord(w[i]) for i in range(len(w) - 1)]
    for stride in (1, 2):
        n = len(r) - stride * len(w)
        m = np.ones(n, dtype=bool)
        for i, dd in enumerate(d):
            m &= (r[stride * (i + 1):stride * (i + 1) + n] - r[stride * i:stride * i + n]) == dd
        hits = np.nonzero(m)[0]
        offs = {}
        for h in hits[:2000]:
            k = int(r[h]) - ord(w[0])
            offs.setdefault(k, []).append(h)
        for k, hs in sorted(offs.items(), key=lambda kv: -len(kv[1]))[:5]:
            print(w, 'stride', stride, 'shift', k, len(hs), [hex(0x08000000 + x) for x in hs[:5]])
