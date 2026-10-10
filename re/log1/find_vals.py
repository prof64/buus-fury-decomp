"""find_vals.py STATE v1 v2 ... : search EWRAM/IWRAM for 16-bit and 32-bit values."""
import sys, struct
from common import *
e = emu(sys.argv[1])
mem = {0x02000000: e.ewram(), 0x03000000: e.iwram()}
for v in sys.argv[2:]:
    v = int(v, 0)
    for base, m in mem.items():
        hits = []
        for i in range(0, len(m) - 1, 2):
            if struct.unpack_from('<H', m, i)[0] == v:
                hits.append(base + i)
        print(v, hex(base), len(hits), ' '.join(hex(h) for h in hits[:60]))
