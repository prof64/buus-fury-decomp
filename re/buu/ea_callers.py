"""ea_callers.py fn [fn ...] -- static BL callers of each function, named by the AI node registry (node vtable slots)
when the caller is a node function; also lists vtables that hold the function directly."""
import contextlib
import io
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'tools', 'battle'))
with contextlib.redirect_stdout(io.StringIO()):
    import gen_buu_enemy_anim as G
from common import rom

FN, _ = G.node_functions()
r = rom()
targets = [int(a, 16) & ~1 for a in sys.argv[1:]]
calls = {t: [] for t in targets}
for a in range(0x08000000, 0x08060000, 2):
    o = a - 0x08000000
    h1, h2 = struct.unpack_from('<HH', r, o)
    if (h1 & 0xF800) == 0xF000 and (h2 & 0xF800) == 0xF800:
        off = ((h1 & 0x7FF) << 12) | ((h2 & 0x7FF) << 1)
        if off & 0x400000:
            off -= 0x800000
        d = a + 4 + off
        if d in calls:
            calls[d].append(a)
for t in targets:
    out = []
    for c in calls[t]:
        s = G.fn_start(c)
        out.append('%08x(%s)' % (c, ','.join(FN.get(s, [])) or 'fn %08x' % (s or 0)))
    # self-relative vtable entries pointing at t (+1)
    vts = []
    for a in range(0x08049000, 0x08050000, 4):
        v = struct.unpack_from('<i', r, a - 0x08000000)[0]
        # entry base unknown: try every base within 0x40 before
        for k in range(0, 0x40, 4):
            if ((a - k + v) & ~1) == t:
                vts.append('%08x[%d]' % (a - k, k // 4))
                break
    print('%08x callers: %s' % (t, ' '.join(out[:12])))
    if vts:
        print('         in vtables: %s' % ' '.join(vts[:8]))
