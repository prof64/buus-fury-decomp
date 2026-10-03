"""seggrid.py SEGDIR NAME FRAME TX0 TY0 TX1 TY1: the game's live enemy-layer grid (map object 0x030019A4 +0xB0;
bitmap, 32 bytes a row, bit tx & 7 of byte ty * 32 + (tx >> 3)) in a tile window at a frame of a segment, in the
format of the harness's SEGMENT_GRID ('.' free, '#' blocked); 'walk' as 8th argument = the walk grid (+0xA8)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, fr = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3])
x0, y0, x1, y1 = map(int, sys.argv[4:8])
off = 0xA8 if len(sys.argv) > 8 and sys.argv[8] == 'walk' else 0xB0
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(fr + 1):
    e.set_keys(keys[f])
    e.run(1)
g = e.r32(0x030019A4 + off)
for ty in range(y0, y1):
    row = e.read(g + ty * 32, 32)
    print('      grid %d %s' % (ty, ''.join('.' if row[tx >> 3] >> (tx & 7) & 1 else '#' for tx in range(x0, x1))))
