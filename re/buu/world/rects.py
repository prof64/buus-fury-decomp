"""Map header +0x24/+0x28 records for areas given as world-area keys."""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from common import *
AREAS = 0x0808E2E0
want = set(sys.argv[1:])
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    key = f'{u8(a)}-{u8(a+1)}'
    if key not in want:
        continue
    hdr = u32(u32(a + 0x2c))
    n, p = u32(hdr + 0x24), u32(hdr + 0x28)
    print(key, 'hdr', hex(hdr), 'n', n, hex(p))
    for k in range(n):
        r = p + 0x18 * k
        print('  ', ' '.join(f'{u32(r + 4 * j):08x}' for j in range(6)))
