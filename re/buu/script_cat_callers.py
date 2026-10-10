"""script_cat_callers.py target... -- list Thumb BL call sites to the given targets, plus literal-pool words equal to them."""
import sys
import struct
from common import *

R = rom()
targets = [int(x, 16) for x in sys.argv[1:]]


def bl_sites(tgts):
    tg = set(t & ~1 for t in tgts)
    out = []
    n = len(R) - 4
    for off in range(0, min(n, 0x800000), 2):
        h = R[off] | (R[off + 1] << 8)
        if (h & 0xF800) != 0xF000:
            continue
        l = R[off + 2] | (R[off + 3] << 8)
        if (l & 0xF800) not in (0xF800, 0xE800):
            continue
        hi = h & 0x7FF
        if hi & 0x400:
            hi -= 0x800
        t = 0x08000000 + off + 4 + (hi << 12) + ((l & 0x7FF) << 1)
        if t in tg:
            out.append((0x08000000 + off, t))
    return out


def lit_sites(tgts):
    out = []
    for t in tgts:
        for v in (t, t | 1):
            pat = struct.pack('<I', v)
            i = R.find(pat)
            while i >= 0:
                if i % 4 == 0:
                    out.append((0x08000000 + i, v))
                i = R.find(pat, i + 1)
    return out


if __name__ == '__main__':
    for a, t in bl_sites(targets):
        print(f'BL  {a:08x} -> {t:08x}')
    for a, v in lit_sites(targets):
        print(f'LIT {a:08x} = {v:08x}')
