"""fxC_sphere.py -- run Dabura's dark sphere (state 0x79) 200 frames and dump BG0 (screen 0x0600E000, 8bpp tiles at
char base 0): distinct tile ids used, the colour indices inside them and their BG palette colours; plus the sphere
object fields (radius, centre)."""
import struct
from bv_fighters_lib import *
from bv_fighters_lib import spawn2, set_state, pos, setpos

e, o, ok = spawn2(0x12)
p = e.r32(PLAYER)
bx, by = pos(e, o)
setpos(e, p, bx - 60, by)
set_state(e, o, 0x79)
e.run(200)
vram = e.vram()
pal = e.palram()
scr = vram[0xE000:0xE800]
ids = {}
for i in range(1024):
    v = struct.unpack_from('<H', scr, 2 * i)[0]
    ids[v] = ids.get(v, 0) + 1
print('screen entries', sorted(ids.items(), key=lambda x: -x[1])[:12])
for v, n in sorted(ids.items(), key=lambda x: -x[1])[:6]:
    t = v & 0x3FF
    px = vram[t * 64:(t + 1) * 64]
    cols = sorted(set(px))
    print(f'tile {t} flags {v >> 10:x} n={n} colours', cols,
          [f'{struct.unpack_from("<H", pal, 2 * c)[0]:04x}' for c in cols])
    for r in range(8):
        print('   ', ' '.join(f'{b:02x}' for b in px[r * 8:r * 8 + 8]))
