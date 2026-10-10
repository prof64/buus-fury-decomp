"""lists.py: dump the +0x18 / +0x1C / +0x20 area lists."""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
AREAS = 0x0808E2E0
which = sys.argv[1]
n = 0
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    if which == '18':
        for k in range(u8(a + 5)):
            r = u32(u32(a + 0x18) + 4 * k)
            print(f'{u8(a)}-{u8(a+1)} {r:08x}:', ' '.join(f'{u32(r + 4 * j):08x}' for j in range(10)))
    elif which == '1c':
        for k in range(u8(a + 6)):
            r = u32(u32(a + 0x1c) + 4 * k)
            print(f'{u8(a)}-{u8(a+1)} {r:08x}:', ' '.join(f'{u32(r + 4 * j):08x}' for j in range(10)))
    elif which == '20':
        for k in range(u8(a + 7)):
            r = u32(a + 0x20) + 8 * k
            print(f'{u8(a)}-{u8(a+1)} {r:08x}:', ' '.join(f'{u32(r + 4 * j):08x}' for j in range(4)))
    elif which == 'misc':
        print(f'{u8(a)}-{u8(a+1)} b2={u8(a+2)} +8={u32(a+8):x} +0x24={u32(a+0x24):x} +0x28={u32(a+0x28):x} +0x34={u32(a+0x34):x} c6={u8(a+6)} c7={u8(a+7)}')
