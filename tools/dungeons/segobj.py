"""segobj.py SEGDIR NAME VT F0 F1 STEP [NWORDS]: replay a segment on the emulator and dump, every STEP frames in
[F0, F1], the first NWORDS (default 16) u32 words of every world object (list 0x03001C4C) whose vtable is VT."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, vt = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3], 16)
f0, f1, step = int(sys.argv[4]), int(sys.argv[5]), int(sys.argv[6])
nw = int(sys.argv[7]) if len(sys.argv) > 7 else 16
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f >= f0 and (f - f0) % step == 0:
        cnt = e.r32(0x03001C4C + 4)
        for k in range(min(cnt, 255)):
            o = e.r32(0x03001C4C + 8 + 4 * k)
            if e.r32(o) == vt:
                print(f, '%08X' % o, ' '.join('%x' % e.r32(o + 4 * j) for j in range(nw)))
        p = e.r32(0x03001C34)
        if p:
            print(f, 'player', e.r32(p + 0x1A0) / 256, e.r32(p + 0x1A4) / 256, 'lf', e.r32(0x03000F94))
