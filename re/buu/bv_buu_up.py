"""Walk node-def references upward from a def until a state root is found; pretty-print the root tree.
usage: bv_buu_up.py def_addr [def_addr...]"""
import sys
import numpy as np
from common import *
from ai_decode import decode, pretty, STATE_TABLE

r = rom()
a = np.frombuffer(r[:len(r) // 4 * 4], dtype=np.uint32)
roots = {u32(STATE_TABLE + 8 * s): s for s in range(0x400)}


def refs(d):
    return [0x08000000 + 4 * int(j) for j in np.nonzero(a == d)[0]]


def owner_def(ref):
    """find the def containing word address ref: scan back for a create fn (thumb, in 0x0801..0x0805)"""
    for back in range(0, 0x80, 4):
        d = ref - back
        w = u32(d)
        if 0x08008000 <= w < 0x08060000 and w & 1 and back >= 0x0c:
            return d
    return None


for s in sys.argv[1:]:
    d = int(s, 16)
    chain = [d]
    for _ in range(12):
        if d in roots:
            print('ROOT state', hex(roots[d]), 'chain', [hex(x) for x in chain])
            print('\n'.join(pretty(decode(d))))
            break
        rs = refs(d)
        if not rs:
            print('no refs to', hex(d), 'chain', [hex(x) for x in chain])
            break
        od = owner_def(rs[0])
        print('  ', hex(d), 'referenced at', [hex(x) for x in rs[:4]], '-> owner def', od and hex(od))
        if od is None:
            break
        d = od
        chain.append(d)
