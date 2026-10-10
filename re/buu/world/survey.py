"""Survey area object lists: group records by ctor."""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from collections import defaultdict, Counter
from common import *
AREAS = 0x0808E2E0
by = defaultdict(list)
listkind = Counter()
for i in range(0x1c4):
    a = AREAS + 0x38 * i
    c3, c4, c5, c6, c7 = (u8(a + k) for k in range(3, 8))
    p10, p14, p18, p1c, p20 = (u32(a + k) for k in (0x10, 0x14, 0x18, 0x1c, 0x20))
    # +0x10: cond objects 8-byte {cond, obj}
    for k in range(c3):
        cond = u32(p10 + 8 * k); obj = u32(p10 + 8 * k + 4)
        by[u32(obj)].append((i, 'cond', obj, cond))
    for name, c, p in (('l14', c4, p14), ('l18', c5, p18), ('l1c', c6, p1c), ('l20', c7, p20)):
        for k in range(c):
            obj = u32(p + 4 * k)
            if not 0x08000000 <= obj < 0x09000000:
                listkind[(name, 'bad')] += 1; continue
            by[u32(obj)].append((i, name, obj, None))
for ctor, recs in sorted(by.items(), key=lambda kv: -len(kv[1])):
    lists = Counter(r[1] for r in recs)
    print(f'{ctor:08x} n={len(recs):5d} uniq={len(set(r[2] for r in recs)):5d} {dict(lists)} ex={recs[0][2]:08x}')
print(listkind)
print('c6/c7 nonzero', sum(1 for i in range(0x1c4) if u8(AREAS+0x38*i+6) or u8(AREAS+0x38*i+7)))
