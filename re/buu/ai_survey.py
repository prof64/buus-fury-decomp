"""Survey the enemy AI state table (0x0870F514, 8 bytes per state id: {state_def*, transitions*})."""
from collections import Counter, defaultdict
from common import *
T = 0x0870F514
N = 0x337
fns = Counter()
ex = defaultdict(list)
for i in range(N):
    d, tr = u32(T + 8 * i), u32(T + 8 * i + 4)
    if not (0x08000000 <= d < 0x08800000):
        continue
    f0 = u32(d)
    fns[f0] += 1
    ex[f0].append(i)
for f, n in fns.most_common():
    print(f'{f:08x} n={n} ids={[hex(x) for x in ex[f][:12]]}')
