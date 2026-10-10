"""Batch-measure all skills with atk_measure -> work/buu/atk_measured.json"""
import json
import sys
from common import *
from atk_measure import measure
from strtab import S
SK = 0x0871DAF8
out = []
only = [int(x, 0) for x in sys.argv[1:]]
for sk in range(1, 0x18):
    if only and sk not in only:
        continue
    if sk in (12, 19, 21):
        continue
    mx = u32(SK + 24 * sk)
    for lv in ([0, 1, 2] if mx == 2 else [0]):
        for hold in (3, 60, 150):
            for dist in (40, 100):
                r = measure(sk, lv, hold, dist, frames=300)
                r['name'] = S(u16(SK + 24 * sk + 12))
                h = r['hits']
                print(sk, r['name'], lv, hold, dist, 'press', r['cost_on_press'], 'EP', r['ep_total'], 'idle', r['idle_frame'],
                      'hits', len(h), 'first', h[0][0] if h else None, 'last', h[-1][0] if h else None,
                      'mults', sorted(set(x[3] for x in h)), 'sub', sorted(set(x[2] for x in h)), 'form', r['form_after'], flush=True)
                out.append(r)
(WORK / ('atk_measured.json' if not only else 'atk_measured_%s.json' % '_'.join(map(str, only)))).write_text(json.dumps(out))
