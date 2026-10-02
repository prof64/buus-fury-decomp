"""script_callers.py target [target...] -- list every Thumb BL in the ROM code area that calls a target,
with the enclosing function start guess (nearest preceding push {..., lr})."""
import sys
from common import *

R = rom()
targets = {int(a, 16) & ~1 for a in sys.argv[1:]}
END = 0x00060000   # code area (thumb code lives below ~0x08056000)
for off in range(0, END, 2):
    hi = R[off] | (R[off + 1] << 8)
    if hi & 0xF800 != 0xF000:
        continue
    lo = R[off + 2] | (R[off + 3] << 8)
    if lo & 0xF800 != 0xF800:
        continue
    o = ((hi & 0x7FF) << 12) | ((lo & 0x7FF) << 1)
    if o & 0x400000:
        o -= 0x800000
    a = 0x08000000 + off
    t = a + 4 + o
    if t in targets:
        # function start guess
        f = off
        while f > 0 and f > off - 0x800:
            w = R[f] | (R[f + 1] << 8)
            if w & 0xFF00 == 0xB500:
                break
            f -= 2
        print(f'{a:08x} -> {t:08x}   fn~{0x08000000 + f:08x}')
