"""OAM + party objects of a title / charsel scenario at given frames: c9.py SCENARIO FRAME..."""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
import capture
name = sys.argv[1]
want = {int(x) for x in sys.argv[2:]}
state, setup, pre, keys, *rest = capture.SCENARIOS[name]
extra = rest[0] if rest else {}
c = Cap(state)
if setup:
    setup(c.e)
for k in program(pre):
    c.step(k, keep=False)
if extra.get('trigger'):
    extra['trigger'](c.e)
SH = {0: [(8, 8), (16, 16), (32, 32), (64, 64)], 1: [(16, 8), (32, 8), (32, 16), (64, 32)], 2: [(8, 16), (8, 32), (16, 32), (32, 64)]}
for i, k in enumerate(program(keys)):
    c.step(k, keep=False)
    if i in want:
        e = c.e
        print('frame', i, 'lf', e.r32(0x03000F94))
        for j in range(128):
            a0, a1, a2 = e.r16(0x07000000 + 8 * j), e.r16(0x07000002 + 8 * j), e.r16(0x07000004 + 8 * j)
            if (a0 >> 8) & 3 == 2:
                continue
            y = a0 & 0xFF
            x = a1 & 0x1FF
            if x >= 256:
                x -= 512
            w, h = SH.get(a0 >> 14, [(0, 0)] * 4)[a1 >> 14]
            print(f'  oam {j:3d} x {x:4d} y {y:3d} {w}x{h} tile {a2 & 0x3FF} pal {a2 >> 12} prio {(a2 >> 10) & 3} hflip {(a1 >> 12) & 1} 8bpp {(a0 >> 13) & 1} mode {(a0 >> 10) & 3} aff {(a0 >> 8) & 1}')
        ow = 0x03001C4C
        n = e.r32(ow + 4)
        for j in range(min(n, 40)):
            o = e.r32(ow + 8 + 4 * j)
            print(f'  obj {j} {o:08x} vt {e.r32(o):08x} x {e.r32(o + 0x6c)} y {e.r32(o + 0x70)} def {e.r32(o + 0x58):08x} +10 {e.r8(o + 0x10)} +14 {e.r32(o + 0x14):08x} 30..3c {[hex(e.r32(o + q)) for q in (0x30, 0x34, 0x38, 0x3c)]} 40..50 {[hex(e.r32(o + q)) for q in range(0x40, 0x5c, 4)]}')
