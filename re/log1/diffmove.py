"""Find RAM words that change monotonically when holding RIGHT."""
import sys, struct
from common import *
e = emu(sys.argv[1])
key = getattr(KEY, sys.argv[2]) if len(sys.argv) > 2 else KEY.RIGHT
snaps = []
for i in range(4):
    snaps.append(e.ewram() + e.iwram())
    e.hold(key, 8)
e.set_keys(0)
snaps.append(e.ewram() + e.iwram())
n = len(snaps[0])
for off in range(0, n, 4):
    vals = [struct.unpack_from('<i', s, off)[0] for s in snaps]
    d = [vals[i + 1] - vals[i] for i in range(len(vals) - 1)]
    if all(x > 0 for x in d[:3]) or all(x < 0 for x in d[:3]):
        addr = (0x02000000 + off) if off < 0x40000 else (0x03000000 + off - 0x40000)
        print(hex(addr), vals)
