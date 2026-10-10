"""recs.py CTOR [N] [LEN]: dump example object defs for a ctor (from area lists)."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
AREAS = 0x0808E2E0
ctor = int(sys.argv[1], 16)
N = int(sys.argv[2], 0) if len(sys.argv) > 2 else 6
L = int(sys.argv[3], 0) if len(sys.argv) > 3 else 0x30
n = 0
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    objs = []
    for k in range(u8(a + 3)):
        objs.append(u32(u32(a + 0x10) + 8 * k + 4))
    for k in range(u8(a + 4)):
        objs.append(u32(u32(a + 0x14) + 4 * k))
    for o in objs:
        if u32(o) != ctor:
            continue
        b = rom()[ra(o):ra(o) + L]
        ws = ' '.join(f'{struct.unpack_from("<I", b, k)[0]:08x}' for k in range(0, L, 4))
        print(f'area {u8(a)}-{u8(a+1)} {o:08x}: {ws}')
        n += 1
        if n >= N:
            sys.exit()
