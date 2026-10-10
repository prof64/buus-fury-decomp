"""db3_grid.py STATE: print the live walk grid of the state's area ('.' walkable, '#' blocked, 'P' player tile,
'E' enemy tile), one char per 8 px tile; plus the static grid's differences ('+' = static walkable, live blocked;
'-' = live walkable, static blocked)."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status, enemies, MAPOBJ  # noqa: E402
from route import start_emu  # noqa: E402
from grid import Grid, map_of  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
e.run(2)
s = status(e)
g = Grid(map_of(*s['area']))
raw = e.read(e.r32(MAPOBJ + 0xA8), 32 * g.h)
print(s, 'grid', g.w, g.h)
en = {(o[1] // 8, o[2] // 8) for o in enemies(e)}
print('enemies', [(o[1], o[2], o[3], o[4], o[5]) for o in enemies(e)])
p = (s['p'][0] // 8, s['p'][1] // 8) if s['p'] else None
if len(sys.argv) > 3:
    g.w0, g.h0 = int(sys.argv[2]), int(sys.argv[3])
else:
    g.w0, g.h0 = 40, 30
W = g.w
g_w, g.h = g.w0, g.h0


class _V:
    pass
print('    ' + ''.join(str(x // 10 % 10) for x in range(g_w)))
print('    ' + ''.join(str(x % 10) for x in range(g_w)))
for ty in range(g.h):
    row = ''
    for tx in range(g_w):
        live = (raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1
        st = g.walk[ty * W + tx]
        c = '.' if live else '#'
        if live != st:
            c = '-' if live else '+'
        if (tx, ty) in en:
            c = 'E'
        if (tx, ty) == p:
            c = 'P'
        row += c
    print('%3d ' % ty + row)
