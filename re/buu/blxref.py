"""blxref.py target [...] -- find thumb BL instructions calling target"""
import sys
import numpy as np
from common import rom


def bl_targets():
    r = np.frombuffer(rom(), dtype=np.uint16).astype(np.int64)
    hi = r[:-1]
    lo = r[1:]
    m = ((hi & 0xF800) == 0xF000) & ((lo & 0xF800) == 0xF800)
    idx = np.nonzero(m)[0]
    off = ((hi[idx] & 0x7FF) << 12) | ((lo[idx] & 0x7FF) << 1)
    off = np.where(off & 0x400000, off - 0x800000, off)
    src = 0x08000000 + 2 * idx
    tgt = src + 4 + off
    return src, tgt


if __name__ == '__main__':
    src, tgt = bl_targets()
    for s in sys.argv[1:]:
        t = int(s, 16) & ~1
        hits = src[tgt == t]
        print(f'{t:08x}:', ' '.join(f'{x:08x}' for x in hits[:60]), f'({len(hits)})')
