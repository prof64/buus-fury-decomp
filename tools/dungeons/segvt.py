"""segvt.py SEGDIR NAME VT F0 F1 [NWORDS]: per frame, the words of every world object with vtable VT (changes only)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, vt = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3], 16)
f0, f1 = int(sys.argv[4]), int(sys.argv[5])
nw = int(sys.argv[6]) if len(sys.argv) > 6 else 14
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
        if e.r32(o) == vt:
            rows.append('%x: ' % o + ' '.join('%x' % e.r32(o + 4 * i) for i in range(1, nw)))
    if rows != prev:
        print(f, 'keys', keys[f] if f >= 0 else '-', ' | '.join(rows))
    prev = rows
