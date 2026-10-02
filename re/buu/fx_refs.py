"""fx_refs.py [addr ...] -- code literal-pool references (in 0x08000000..0x08060000) to effect record tables.
Without args: every fx_ run of data/buu/sprites.json; a reference to any record inside the run counts."""
import sys
import json
import struct
import numpy as np
from common import *

r = rom()
a = np.frombuffer(r[:0x60000], dtype='<u4')


def refs(lo, hi):
    idx = np.nonzero((a >= lo) & (a < hi))[0]
    return [(0x08000000 + 4 * int(i), int(a[i])) for i in idx]


if len(sys.argv) > 1:
    for s in sys.argv[1:]:
        x = int(s, 16)
        print(hex(x), [(hex(p), hex(v)) for p, v in refs(x, x + 1)])
else:
    d = json.loads((DATA / 'sprites.json').read_text())
    for e in d['entities']:
        if not e['id'].startswith('fx_'):
            continue
        st = e['animations']['default']['directions']['any']
        lo = int(st[0]['rom'], 16)
        hi = int(st[-1]['rom'], 16) + 12
        rr = refs(lo, hi)
        print(e['id'], len(st), ' '.join(f'{p:08x}->{v - lo:+#x}' for p, v in rr))
