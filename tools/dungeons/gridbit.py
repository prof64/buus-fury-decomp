"""gridbit.py SEG.json STATE.ss TX TY [F0 F1]: replay a segment in the emulator and report when the enemy-layer grid
tile (TX, TY) (map object 0x030019A4 +0xB0: 32 bytes a row, bit set = free) changes, with the actors near it then"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

seg, st, tx, ty = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
f0 = int(sys.argv[5]) if len(sys.argv) > 5 else 0
f1 = int(sys.argv[6]) if len(sys.argv) > 6 else 10 ** 9
lines = Path(seg).read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(Path(st).resolve()))


def bit():
    g = e.r32(0x030019A4 + 0xB0)
    if not (0x02000000 <= g < 0x02040000):
        return None
    return (e.r8(g + ty * 32 + (tx >> 3)) >> (tx & 7)) & 1


prev = bit()
for f in range(min(len(keys), f1)):
    e.set_keys(keys[f])
    e.run(1)
    b = bit()
    if b != prev and f >= f0:
        c = e.r32(0x03001C4C + 4)
        near = []
        for k in range(min(c, 255)):
            o = e.r32(0x03001C4C + 8 + 4 * k)
            x, y = e.r32(o + 0x1A0) / 256, e.r32(o + 0x1A4) / 256
            if abs(x - tx * 8) < 48 and abs(y - ty * 8) < 48:
                near.append((hex(o), hex(e.r32(o)), x, y))
        print(f, 'bit', prev, '->', b, near)
    prev = b
