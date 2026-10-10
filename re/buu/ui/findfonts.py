"""Scan ROM for font structs {u16* T, u32 wide_w, u16* wide_data|0} as used by 0x0803F444."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
r = rom()
N = len(r)
def ok(a):
    return 0x08000000 <= a < 0x08000000 + N
for o in range(0, N - 12, 4):
    t, w, d = struct.unpack_from('<III', r, o)
    if not ok(t) or t & 1 or not (0 <= w <= 32) or not (d == 0 or ok(d)):
        continue
    try:
        ga = u16(t + 2 * 0x41)
        if ga == 0 or ga > 0x2000:
            continue
        gw = u16(t + 2 * ga)
        if not 1 <= gw <= 16:
            continue
        # most of A-Z valid
        cnt = 0
        for c in range(0x41, 0x5b):
            g = u16(t + 2 * c)
            if g and 1 <= u16(t + 2 * g) <= 16 and g > 0x7f:
                cnt += 1
        if cnt < 5:
            continue
    except Exception:
        continue
    print(f'font @{0x08000000 + o:08x}: T={t:08x} wide_w={w} wide={d:08x} A: g={ga} w={gw}')
