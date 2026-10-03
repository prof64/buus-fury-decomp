"""db3_grid2.py STATE [W H]: dump the bit grids behind the pointers at MAPOBJ+0x98 .. +0xC0 (looking for the second
collision layer: open_area_alt)."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status, MAPOBJ  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
e.run(2)
print(status(e))
W, H = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else (40, 30)
for off in range(0x98, 0xC4, 4):
    p = e.r32(MAPOBJ + off)
    print(hex(off), hex(p))
    if not (0x02000000 <= p < 0x02040000 or 0x03000000 <= p < 0x03008000 or 0x08000000 <= p < 0x0A000000):
        continue
    raw = e.read(p, 32 * H)
    for ty in range(H):
        print('%3d ' % ty + ''.join('.' if (raw[ty * 32 + (tx >> 3)] >> (tx & 7)) & 1 else '#' for tx in range(W)))
