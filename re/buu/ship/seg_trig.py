"""seg_trig.py SEGDIR NAME a b [vt] -- replay a segment and dump the trigger list's objects of vtable vt (default the
coin trigger 0x0804A0C0) for video frames a..b (first 0x30 bytes as s16 words)"""
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import status  # noqa: E402
from route import start_emu  # noqa: E402

TRIGS = 0x03002A64
d, name, a, b = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
vt = int(sys.argv[5], 0) if len(sys.argv) > 5 else 0x0804A0C0
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(min(b + 1, len(keys))):
    e.set_keys(keys[f])
    e.run(1)
    if f >= a:
        n = e.r32(TRIGS)
        out = []
        for k in range(min(n, 64)):
            o = e.r32(TRIGS + 4 + 4 * k)
            if e.r32(o) == vt:
                raw = e.read(o, 0x30)
                out.append((hex(o), struct.unpack('<24h', raw)))
        print(f, status(e)['p'], out)
