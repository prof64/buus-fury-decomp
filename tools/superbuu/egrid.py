"""egrid.py SEGMENT.json FRAME TX0 TY0 TX1 TY1 [state]: replay to video frame FRAME and print the game's enemy grid
([0x03001A54], 256 x 256 tiles, a bit grid: bit tx & 7 of byte ty * 32 + tx >> 3, 1 = passable) over tiles [TX0, TX1) x [TY0, TY1), plus the walk grid ([0x03001A4C]...
see COLLISION.md: map object +0xA8 walk, +0xB0 enemy)"""
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from route import start_emu  # noqa: E402
from segment import load_doc  # noqa: E402

head, frames = load_doc(sys.argv[1])
n = int(sys.argv[2])
tx0, ty0, tx1, ty1 = (int(x) for x in sys.argv[3:7])
st = sys.argv[7] if len(sys.argv) > 7 else str(ROOT / 'work' / 'segments' / f"{head['name']}.ss")
e = start_emu(st)
for k in range(n + 1):
    e.set_keys(frames[k]['keys'])
    e.run(1)
for name, ptr in (('enemy', 0x03001A54), ('walk', 0x03001A4C)):
    g = e.r32(ptr)
    print(name, hex(g))
    print('    ' + ''.join(f'{tx % 100:3d}' for tx in range(tx0, tx1)))
    for ty in range(ty0, ty1):
        print(f'{ty:3d} ' + ''.join(f'{(e.r8(g + ty * 32 + (tx >> 3)) >> (tx & 7)) & 1:3d}' for tx in range(tx0, tx1)))
