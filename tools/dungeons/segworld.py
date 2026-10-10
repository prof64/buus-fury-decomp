"""segworld.py SEGDIR NAME F0 F1: every frame in [F0, F1] the world objects (list 0x03001C4C): vtable, position
(+0x1A0/+0x1A4, 24.8 -> px), height (+0x1A8) and the zenie count."""
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
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f >= f0:
        cnt = e.r32(0x03001C4C + 4)
        out = []
        for k in range(min(cnt, 255)):
            o = e.r32(0x03001C4C + 8 + 4 * k)
            out.append('%07X(%d,%d,z%d)' % (e.r32(o) & 0xFFFFFFF, sv(e.r32(o + 0x1A0)) >> 8, sv(e.r32(o + 0x1A4)) >> 8, sv(e.r32(o + 0x1A8))))
        print(f, ' '.join(out))
