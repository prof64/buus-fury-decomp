"""Catalogue actor behaviour lists (count + ptr to pointer list of {fn, params} records)."""
import sys
sys.path.insert(0, '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu')
from collections import Counter, defaultdict
from common import *
AREAS = 0x0808E2E0
CT = {0x0800fd55: (0x18, 0x1c), 0x080111a9: (0x10, 0x14), 0x08017da3: (0x14, 0x18)}
fns = Counter()
ex = defaultdict(list)
counts = Counter()
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    for k in range(u8(a + 4)):
        o = u32(u32(a + 0x14) + 4 * k)
        c = u32(o)
        if c not in CT:
            continue
        no, po = CT[c]
        n, p = u32(o + no), u32(o + po)
        counts[(hex(c), n)] += 1
        for j in range(min(n, 20)):
            r = u32(o + no + 4 + 4 * j)
            f = u32(r)
            fns[(hex(c), hex(f))] += 1
            if len(ex[f]) < 3:
                ex[f].append(f'{u8(a)}-{u8(a+1)} rec {r:08x}: ' + ' '.join(f'{u32(r + 4 * q):08x}' for q in range(4)))
print(counts)
for (c, f), n in fns.most_common():
    print(c, f, n)
for f, l in ex.items():
    print(hex(f))
    for s in l:
        print('   ', s)
