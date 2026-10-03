"""segpeek.py SEGDIR NAME F0 F1 STEP expr...: replay a segment on the emulator and print, every STEP frames in
[F0, F1], the values of the expressions; an expression is ADDR (hex u32 at an absolute address), p+OFF:SIZE (player
object field, SIZE 1/2/4), or oN+OFF:SIZE (world object N of the object list 0x03001C4C)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1, step = int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
exprs = sys.argv[6:]
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))


def rd(a, n):
    return {1: e.r8, 2: e.r16, 4: e.r32}[n](a)


def val(x):
    if x.startswith('p+'):
        off, n = x[2:].split(':')
        p = e.r32(0x03001C34)
        return rd(p + int(off, 16), int(n)) if p else None
    if x.startswith('o'):
        k, rest = x[1:].split('+')
        off, n = rest.split(':')
        cnt = e.r32(0x03001C4C + 4)
        if int(k) >= cnt:
            return None
        o = e.r32(0x03001C4C + 8 + 4 * int(k))
        return rd(o + int(off, 16), int(n))
    if x == 'world':
        cnt = e.r32(0x03001C4C + 4)
        return ['%08X' % e.r32(e.r32(0x03001C4C + 8 + 4 * k)) for k in range(min(cnt, 64))]
    return e.r32(int(x, 16))


for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f >= f0 and (f - f0) % step == 0:
        print(f, [(x, (hex(v) if isinstance(v, int) else v)) for x in exprs for v in [val(x)]])
