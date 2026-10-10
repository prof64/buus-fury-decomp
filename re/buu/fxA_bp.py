"""fxA_bp.py skill hold frames face dist addr[,addr] -- run a skill and report every frame where any addr executed (lr)."""
import sys
from common import *
from combat import setup, pin, REC

sk, hold, frames, face, dist = (int(x, 0) for x in sys.argv[1:6])
addrs = [int(a, 16) for a in sys.argv[6].split(',')]
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, face)
dx, dy = {3: (dist, 0), 2: (-dist, 0), 0: (0, dist), 1: (0, -dist)}[face]
bids = [e.breakpoint(a) for a in addrs]
for f in range(frames):
    if en and f < 30:
        pin(e, en[0], p, dx, dy)
    e.w16(REC + 0x1a, 999)
    e.set_keys(KEY.B if f < hold else 0)
    e.run(1)
    for h in e.hits():
        print(f, e.r32(0x03000f94), hex(h['pc']), 'lr', hex(h['lr']), 'r0..r3', [hex(x) for x in h['regs'][:4]])
