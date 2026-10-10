"""Why does Super Buu 0x77 get destroyed after the ball attack?  Watch writes to its vtable word and
log the AI stack / controller each frame; print hits with PCs and LR."""
from common import *
from bv_buu_lib import *
from bv_buu_batch import patch_waypoints, near

e, o = setup(0x77, dx=90)
patch_waypoints(e)
force(e, o, 0x29d)
near(e, o, 90, 0, settle=40)
print('ai words', [hex(e.r32(o + 0x238 + 4 * k)) for k in range(4)])
force(e, o, 0x29e)
w = e.watch(o, o + 4, 'wc')
b = e.breakpoint(REPLACE)
for f in range(260):
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    e.run(1)
    hs = e.hits()
    for h in hs:
        if h.get('addr') == REPLACE:
            if h['regs'][0] == o + 0x238:
                print(f, 'REPLACE state', hex(h['regs'][1]))
            continue
        print(f, 'HIT', {k: (hex(v) if isinstance(v, int) else v) for k, v in h.items() if k != 'regs'},
              'r0-3', [hex(x) for x in h['regs'][:4]], 'lr', hex(h['regs'][14]))
    if f % 20 == 0:
        c = e.r32(o + 0x14)
        print(f, 'ctrl', hex(c), 'vt', hex(e.r32(c)) if 0x02000000 <= c < 0x04000000 else '-', 'cf', e.r16(o + 0x12), 'pos', pos(e, o))
