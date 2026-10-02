"""Walk the enemy behaviour definitions reachable from the AI state table and count leaf node types."""
from collections import Counter, defaultdict
from common import *
T = 0x0870F514
N = 0x337
CONTAINERS = {0x08018f91: 'parallel?', 0x080191c7: 'cont_bda8', 0x080191e9: 'cont_b6d0', 0x0801920f: 'cont_a710'}
leaf = Counter()
leaf_ex = defaultdict(list)
seen = set()


def walk(d, depth=0):
    if d in seen or not (0x08000000 <= d < 0x08800000):
        return
    seen.add(d)
    f = u32(d)
    if f in CONTAINERS:
        n = u32(d + 0xc)
        for k in range(min(n, 32)):
            walk(u32(d + 0x10 + 4 * k), depth + 1)
    else:
        leaf[f] += 1
        leaf_ex[f].append(d)


for i in range(N):
    walk(u32(T + 8 * i))
print('defs', len(seen))
for f, n in leaf.most_common():
    ex = leaf_ex[f][0]
    print(f'{f:08x} n={n:3d} e.g. {ex:08x}: ' + ' '.join(f'{u32(ex + 4 * k):08x}' for k in range(8)))
