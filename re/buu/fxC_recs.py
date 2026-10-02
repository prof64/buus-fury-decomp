"""fxC_recs.py addr n -- dump n 12-byte OBJ records (ox, oy, w, h, attr0, attr1, gfx) and which fx_ run (if any) holds them."""
import sys
import json
import struct
from common import *

d = json.loads((DATA / 'sprites.json').read_text())
where = {}
for e in d['entities']:
    if e['id'].startswith('fx_'):
        for i, s in enumerate(e['animations']['default']['directions']['any']):
            where[int(s['rom'], 16)] = (e['id'], i)
a = int(sys.argv[1], 16)
n = int(sys.argv[2]) if len(sys.argv) > 2 else 8
for i in range(n):
    r = a + 12 * i
    ox, oy, w, h, a0, a1, g = struct.unpack_from('<bbBBHHI', rom(), ra(r))
    print(f'{i:2d} {r:08x} o({ox},{oy}) {w}x{h} a0={a0:04x} a1={a1:04x} gfx={g:08x}', where.get(r, ''))
