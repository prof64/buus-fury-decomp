"""Hit / kill Babidi (0x19) with player punches and watch the flag [0x0300302C] and his states."""
from common import *
from bv_buu_lib import *

for hp in (4300, 1):
    e, o = setup(0x19, dx=-16, dy=0)
    p = e.r32(PLAYER)
    e.w8(p + 0x10, 3)
    e.w32(o + 0x28c, hp << 10)
    w = e.watch(0x0300302C, 0x03003030, 'wc')
    b = e.breakpoint(REPLACE)
    log = []
    for f in range(240):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.set_keys(KEY.A if f % 20 < 2 else 0)
        e.run(1)
        for h in e.hits():
            if h.get('kind') == 'watch':
                log.append((f, 'flag', hex(h['new']), 'pc', hex(h['pc']), 'lr', hex(h['lr'])))
            elif h['regs'][0] == o + 0x238:
                log.append((f, 'state', hex(h['regs'][1])))
    print('start hp', hp, 'end hp', e.r32(o + 0x28c) >> 10, 'vt', hex(e.r32(o)), log[:14], 'flag', hex(e.r32(0x0300302C)))
