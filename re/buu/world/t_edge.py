"""How does the player leave a forest screen? Walk off each edge of 8-3 and catch EnterArea (0x08008D44)."""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
from emutools import catch


def area(e):
    w = e.r32(u32(0x080707d0))
    return e.r8(w), e.r8(w + 1)


def cb(e, pc):
    lr = e.reg(14)
    print(f'  0x08008D44 r1={e.reg(1)} r2={e.reg(2)} dest=({e.r16(e.reg(3))},{e.r16(e.reg(3)+2)}) lr={lr:08x}')


for name, key in (('left', KEY.LEFT), ('right', KEY.RIGHT), ('up', KEY.UP), ('down', KEY.DOWN)):
    e = emu('buu_swforest3.ss')
    p = e.r32(0x03001C34)
    print(name, 'start area', area(e), 'player', e.r32(p + 0x1a0) >> 8, e.r32(p + 0x1a4) >> 8)
    # god mode-ish: put far from enemies? just walk
    n = catch(e, [0x08008D44], 900, keyfn=lambda f: key, cb=cb)
    p = e.r32(0x03001C34)
    print('   end area', area(e), 'player', e.r32(p + 0x1a0) >> 8, e.r32(p + 0x1a4) >> 8, 'calls', n)
