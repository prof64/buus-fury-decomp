import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
from cap import Cap, program
import capture
name = sys.argv[1]
lo, hi = int(sys.argv[2]), int(sys.argv[3])
state, setup, pre, keys = capture.SCENARIOS[name]
c = Cap(state)
if setup:
    setup(c.e)
for k in program(pre):
    c.step(k, keep=False)
for i, k in enumerate(program(keys)):
    c.step(k, keep=False)
    if lo <= i <= hi:
        e = c.e
        m = e.r32(0x03000F90)
        n = e.r32(m + 4)
        kids = [e.r32(m + 8 + 4 * j) for j in range(min(n, 30))]
        foc = e.r32(m + 0x108)
        fi = kids.index(foc) if foc in kids else hex(foc)
        print(i, k, 'lf', e.r32(0x03000F94), 'n', n, 'focus', fi, 'sel', e.r32(m + 0x10C), 'drawn', e.r32(m + 0x110),
              'alloc', e.r8(m + 0x130), 'held', hex(e.r16(0x03003302)), 'vt', [hex(e.r32(o))[-4:] for o in kids])
