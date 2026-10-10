"""worldlist.py SEGDIR NAME F0 F1: per frame, the world list (0x03001C4C: count, objects) as vtables (changes only)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1 = int(sys.argv[3]), int(sys.argv[4])
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
prev = None
for f in range(-1, f1 + 1):
    if f >= 0:
        e.set_keys(keys[f])
        e.run(1)
    if f < f0 - 1:
        continue
    n = e.r32(0x03001C4C + 4)
    rows = []
    for k in range(min(n, 255)):
        o = e.r32(0x03001C4C + 8 + 4 * k)
        rows.append('%x@%x' % (e.r32(o), o))
    if rows != prev:
        print(f, n, ' '.join(rows))
    prev = rows
