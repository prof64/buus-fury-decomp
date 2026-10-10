"""Super Buu reflect stance (state 0x255 -> pose controller 0x03001BA4): which code enters states 0x256/0x257;
ki blast vs melee during the pose; what the reflected blast does."""
from common import *
from bv_buu_lib import *

for label, dx, key, skill in (('kiblast', -60, KEY.B, 0), ('melee', -18, KEY.A, 0)):
    e, o = setup(0x74, dx=dx, dy=0)
    p = e.r32(PLAYER)
    e.w8(REC + 0x15, skill)
    e.w8(p + 0x10, 3)
    force(e, o, 0x255)
    b = e.breakpoint(REPLACE)
    log = []
    for f in range(120):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.set_keys(key if 5 <= f < 8 else 0)
        hp0 = e.r16(REC + 0x16)
        e.run(1)
        for h in e.hits():
            if h['regs'][0] == o + 0x238:
                log.append((f, hex(h['regs'][1]), 'lr', hex(h['regs'][14])))
        if e.r16(REC + 0x16) < hp0:
            log.append((f, 'player hp loss', hp0 - e.r16(REC + 0x16)))
    e.clear(b)
    print(label, 'boss hp', e.r32(o + 0x28c) >> 10, log[:12])
