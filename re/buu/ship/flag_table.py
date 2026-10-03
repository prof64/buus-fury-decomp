"""Per flag (< 448): which scripts set / clear / read it (with their areas). usage: python3 flag_table.py [lo hi]"""
import sys
from collections import defaultdict
from flags_scan import S, ops, areas

lo, hi = (int(sys.argv[1]), int(sys.argv[2])) if len(sys.argv) > 2 else (0, 447)
T = defaultdict(lambda: defaultdict(list))
for sid in S:
    o = ops(sid)
    a = ','.join(sorted({x.split(':')[0] for x in areas(sid)})) or '?'
    for k in ('set', 'clear', 'read'):
        for f in o[k]:
            T[f][k].append(f'{sid}@{a}' if sid.startswith('s') else f'i{sid[9:]}@{a}')
for f in range(lo, hi + 1):
    if f in T:
        d = T[f]
        print(f, '| S', ' '.join(d['set'])[:160], '| C', ' '.join(d['clear'])[:100], '| R', len(d['read']), ' '.join(d['read'])[:120])
