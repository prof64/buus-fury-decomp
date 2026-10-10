"""oam.py STATEFILE : list visible OAM entries"""
import sys, struct
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
SZ = {(0, 0): (8, 8), (0, 1): (16, 16), (0, 2): (32, 32), (0, 3): (64, 64), (1, 0): (16, 8), (1, 1): (32, 8),
      (1, 2): (32, 16), (1, 3): (64, 32), (2, 0): (8, 16), (2, 1): (8, 32), (2, 2): (16, 32), (2, 3): (32, 64)}
e = emu(sys.argv[1])
e.run(int(sys.argv[2]) if len(sys.argv) > 2 else 0)
o = e.oam()
for i in range(128):
    a0, a1, a2 = struct.unpack_from('<HHH', o, i * 8)
    if (a0 >> 8) & 3 == 2:
        continue
    w, h = SZ.get((a0 >> 14, a1 >> 14), (0, 0))
    y = a0 & 0xff; x = a1 & 0x1ff
    print(f'{i:3d} x={x:3d} y={y:3d} {w}x{h} tile={a2 & 0x3ff:4d} pal={a2 >> 12} prio={(a2 >> 10) & 3} 8bpp={(a0 >> 13) & 1} mode={(a0 >> 10) & 3} hf={(a1 >> 12) & 1} affine={(a0 >> 8) & 1}')
