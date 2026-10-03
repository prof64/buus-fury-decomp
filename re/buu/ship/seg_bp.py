"""seg_bp.py SEGDIR NAME a b ADDR [ADDR...] -- replay a segment; for video frames a..b print every hit of the given
breakpoints with r0, r1 and LR (and the object at r1: vtable, x, y px)"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, a, b = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
addrs = [int(x, 0) for x in sys.argv[5:]]
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
for f in range(b + 1):
    if f == a:
        for x in addrs:
            e.breakpoint(x)
    e.set_keys(keys[f])
    e.run(1)
    if f >= a:
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            r = h['regs']
            o = r[1]
            info = ''
            if 0x02000000 <= o < 0x04000000:
                info = f'obj vt {e.r32(o):08x} at ({sv(e.r32(o + 0x1A0)) >> 8}, {sv(e.r32(o + 0x1A4)) >> 8}) def {e.r32(o + 0x58):08x}'
            print(f, hex(h['addr']), 'r0', hex(r[0]), 'r1', hex(r[1]), 'lr', hex(r[14]), info)
