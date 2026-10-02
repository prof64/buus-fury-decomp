"""script_nat_b_actors.py [state] -- list script actor ids that resolve (0x080001C6) in a save state:
0 (player), 9, sprite ids 10..300, and negative placement ids -1..-40. Prints object, vtable, x, y."""
import sys
from common import *
from emutools import call

st = sys.argv[1] if len(sys.argv) > 1 else 'buu_snakeway_start.ss'
e = emu(st)
e.run(3)
ids = [0, 9] + list(range(10, 300)) + [-k for k in range(1, 41)]
for i in ids:
    o = call(e, 0x080001C6, (i & 0xffffffff, 0, 0, 0))
    if o and 0x02000000 <= o < 0x04000000:
        x, y = e.r32(o + 0x1a0), e.r32(o + 0x1a4)
        print(f'id {i:4d}: obj {o:08x} vt {e.r32(o):08x} x {x / 256:.1f} y {y / 256:.1f} dir {e.r8(o + 0x10)}')
