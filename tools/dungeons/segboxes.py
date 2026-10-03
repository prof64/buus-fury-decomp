"""segboxes.py SEGDIR NAME FRAME: the body-box table (obj+0x58 -> +0x10 + 16 * facing: x0, y0, x1, y1 relative to the
position) and the footprint of every enemy and of the player at a frame of a segment (emulator)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import enemies  # noqa: E402
from route import start_emu  # noqa: E402

d, name, fr = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3])
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(fr + 1):
    e.set_keys(keys[f])
    e.run(1)


def s32(v):
    return v - (1 << 32) if v & 0x80000000 else v


def show(tag, o):
    t = e.r32(o + 0x58)
    boxes = [[s32(e.r32(t + 0x10 + 16 * f + 4 * k)) for k in range(4)] for f in range(4)]
    print(tag, 'pos', s32(e.r32(o + 0x1A0)) / 256, s32(e.r32(o + 0x1A4)) / 256, 'facing', e.r8(o + 0x10), 'table %08X' % t,
          'head words', [hex(e.r32(t + 4 * k)) for k in range(4)], 'boxes D/U/L/R', boxes)


for o in enemies(e):
    show(f'enemy type {o[5]}', o[0])
p = e.r32(0x03001C34)
if p:
    show('player', p)
