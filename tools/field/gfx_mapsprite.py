"""gfx_mapsprite.py W A X Y: dump the map-sprite (level sign) objects (vt 0x0804D878) of an area: object words, def bytes,
the attached object (message 8 target), and their label pixels from VRAM (8bpp, 32x16)."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rec_objects as R  # noqa: E402
from warp import warp  # noqa: E402

w, a, x, y = (int(v) for v in sys.argv[1:5])
e = R.emu('buu_snakeway_start.ss')
assert warp(e, w, a, x, y)
R.place(e, x, y, 0, settle=30)
W = R.WORLD
for k in range(e.r32(W + 4)):
    o = e.r32(W + 8 + 4 * k)
    if e.r32(o) != 0x0804D878:
        continue
    d = e.r32(o + 0x14)
    t = e.r32(o + 0x10)
    print(hex(o), 'words', [hex(e.r32(o + 4 * i)) for i in range(6)], 'def', hex(d),
          [e.r8(d + i) for i in range(8)], 'target', hex(t), 'vt', hex(e.r32(t)) if t else None)
    tile = e.r16(o + 0xC)
    px = e.read(0x06010000 + 32 * tile, 32 * 16)
    # 8bpp 1D: 4 x 2 tiles of 8x8
    for row in range(16):
        line = ''
        for col in range(32):
            tx, ty = col // 8, row // 8
            v = px[(ty * 4 + tx) * 64 + (row % 8) * 8 + col % 8]
            line += '.' if v == 0 else format(v % 36, 'x')[-1]
        print('   ', line)
