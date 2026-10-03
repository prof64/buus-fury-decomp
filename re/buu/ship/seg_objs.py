"""seg_objs.py SEGDIR NAME a b VT [nbytes] -- replay a segment; for video frames a..b dump the world objects of vtable VT
(first nbytes as hex words)"""
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

OBJW = 0x03001C4C
d, name, a, b, vt = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5], 0)
nb = int(sys.argv[6], 0) if len(sys.argv) > 6 else 0x40
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(b + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f >= a:
        n = e.r32(OBJW + 4)
        for k in range(min(n, 255)):
            o = e.r32(OBJW + 8 + 4 * k)
            if e.r32(o) == vt:
                w = struct.unpack(f'<{nb // 4}i', e.read(o, nb))
                print(f, hex(o), ' '.join(f'{x:x}' for x in w))
