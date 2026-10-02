"""Map header +0x40 count / +0x44 list."""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
AREAS = 0x0808E2E0
want = set(sys.argv[1:])
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    key = f'{u8(a)}-{u8(a+1)}'
    if want and key not in want:
        continue
    hdr = u32(u32(a + 0x2c))
    n, p = u32(hdr + 0x40), u32(hdr + 0x44)
    print(key, 'n', n, hex(p), '+30', hex(u32(hdr + 0x30)))
    for k in range(min(n, 8)):
        q = u32(p + 4 * k)
        print('    ', hex(q), ' '.join(f'{u32(q + 4 * j):08x}' for j in range(6)) if 0x08000000 <= q < 0x08800000 else '')
