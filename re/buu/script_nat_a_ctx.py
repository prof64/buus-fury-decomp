"""Use counts + argument contexts of natives 0x00..0x57 -> work/buu/script/nat_a_ctx.txt"""
import json
from collections import Counter
from script_nat_a_lib import *

sc = all_scripts()
print('scripts', len(sc), 'bad', sum(1 for s in sc.values() if any(x[1] == 'bad' for x in s)))
ctx = native_contexts(sc)
uses = {n: len(v) for n, v in ctx.items()}
(WORK / 'script' / 'native_uses.json').write_text(json.dumps({f'{k:02x}': v for k, v in sorted(uses.items())}))
out = []
for n in range(0x58):
    v = ctx.get(n, [])
    out.append(f'=== native 0x{n:02x} uses={len(v)}')
    c = Counter(str(p[2]) for p in v)
    for k, m in c.most_common(12):
        ex = next(p for p in v if str(p[2]) == k)
        out.append(f'   {m:4d}x  pre={k}   e.g. #{ex[0]}@{ex[1]:x}')
(WORK / 'script' / 'nat_a_ctx.txt').write_text('\n'.join(out))
