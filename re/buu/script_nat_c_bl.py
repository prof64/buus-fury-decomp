"""script_nat_c_bl.py target... -- find Thumb BL call sites to each target in the ROM code area."""
import sys
from common import *

r = rom()
targets = [int(x, 16) & ~1 for x in sys.argv[1:]]
hits = {t: [] for t in targets}
for off in range(0, 0x60000, 2):
    hi = r[off] | (r[off + 1] << 8)
    if (hi & 0xF800) != 0xF000:
        continue
    lo = r[off + 2] | (r[off + 3] << 8)
    if (lo & 0xF800) != 0xF800:
        continue
    o = ((hi & 0x7FF) << 12) | ((lo & 0x7FF) << 1)
    if o & 0x400000:
        o -= 0x800000
    src = 0x08000000 + off
    t = src + 4 + o
    if t in hits:
        hits[t].append(hex(src))
for t, h in hits.items():
    print(hex(t), h)
