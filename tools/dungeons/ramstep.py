"""ramstep.py SEGDIR NAME DELTA [FRAMES]: work-RAM words that change by DELTA (signed) on each of the first FRAMES
frames (default 3) of a cut - find a counter (a fade level, a timer)"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, delta = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3])
n = int(sys.argv[4]) if len(sys.argv) > 4 else 3
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
regions = [(0x03000000, 0x8000), (0x02000000, 0x40000)]


def snap(width):
    out = {}
    for b, sz in regions:
        for a in range(b, b + sz, width):
            out[a] = e.r32(a) if width == 4 else e.r16(a) if hasattr(e, 'r16') else (e.r32(a & ~3) >> (16 * ((a >> 1) & 1))) & 0xFFFF
    return out


for width in (4, 2):
    s = [snap(width)]
    for f in range(n):
        e.set_keys(keys[f])
        e.run(1)
        s.append(snap(width))
    e = start_emu(str(d / 'states' / f'{name}.ss'))
    mask = 0xFFFFFFFF if width == 4 else 0xFFFF
    for a in s[0]:
        if all(((s[i + 1][a] - s[i][a]) & mask) == (delta & mask) for i in range(n)):
            print(width, hex(a), [hex(x[a]) for x in s])
