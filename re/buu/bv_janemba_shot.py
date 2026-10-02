"""bv_janemba_shot.py [bx by px py] -- Super Janemba energy wave: place boss and player in the SW-forest
clearing, force AI state 0x2DA (AlignWithPlayer -> wave) via an injected GotoState call
(0x0800AA4A(obj+0x238, state)), then track the projectile (vtable 0x0804B9B4) per frame: spawn frame
after entry, velocity, box, lifetime, and the player's HP loss vs the energy damage model."""
import sys
from common import *
from emutools import call
from bv_janemba_spawn import jspawn, REC
from ents import PLAYER
from damage_model import *  # noqa

PROJ_VT = 0x0804B9B4
bx, by, px, py = [int(v) for v in sys.argv[1:5]] if len(sys.argv) > 4 else (330, 415, 440, 415)
e, o, ok = jspawn(0x7A)
p = e.r32(PLAYER)


def sv(v):
    return v - (1 << 32) if v & 0x80000000 else v


def setp(x, X, Y):
    e.w32(x + 0x1a0, (X << 8) & 0xffffffff); e.w32(x + 0x1a4, (Y << 8) & 0xffffffff)


setp(o, bx, by); setp(p, px, py)
e.w16(REC + 0x18, 9999); e.w16(REC + 0x16, 9999)
e.breakpoint(0x0801191A); e.breakpoint(0x0800AA4A)
call(e, 0x0800AA4A, (o + 0x238, 0x2DA, 0, 0))
setp(p, px, py)
q = None
t0 = e.frame
track = []
for f in range(200):
    e.w16(REC + 0x16, 9999)
    hp0 = e.r16(REC + 0x16)
    e.run(1)
    loss = 9999 - e.r16(REC + 0x16)
    for h in e.hits():
        if h['addr'] == 0x0801191A and e.r32(h['regs'][1]) == PROJ_VT:
            q = h['regs'][1]
            print(f'f{f}: wave spawned dir {e.r8(q + 0x10)} vel ({sv(e.r32(q + 0x40))},{sv(e.r32(q + 0x44))}) '
                  f'pos ({sv(e.r32(q + 0x20)) / 256},{sv(e.r32(q + 0x24)) / 256}) boss ({sv(e.r32(o + 0x1a0)) / 256},{sv(e.r32(o + 0x1a4)) / 256})')
        elif h['addr'] == 0x0800AA4A and h['regs'][0] == o + 0x238:
            print(f'f{f}: boss -> state {h["regs"][1]:#x}')
    if loss:
        print(f'f{f}: player HP -{loss}')
    if q:
        if e.r32(q) == PROJ_VT and q in [e.r32(0x03001c4c + 8 + 4 * i) for i in range(min(e.r32(0x03001c4c + 4), 200))]:
            track.append((f, sv(e.r32(q + 0x20)) / 256, sv(e.r32(q + 0x24)) / 256, [sv(e.r32(q + 0x28 + 4 * k)) for k in range(4)]))
        else:
            print(f'f{f}: wave gone; tracked {len(track)} frames: first {track[:2]} last {track[-1:]}')
            q = None
            break
st = e.r32(p + 0x1b6) if False else None
print('player END/END2 obj', e.r16(p + 0x1b4), e.r16(p + 0x1b6), 'boss POW', e.r16(o + 0x1b2), 'player resist energy', e.r8(p + 0x1b9))
