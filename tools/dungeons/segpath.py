"""segpath.py SEGDIR NAME F0 F1: per frame, every enemy's AI states and the path-follower runtimes found in its top
state's runtime tree (vtable 0x08049DA8 and kin: +4 speed, +8 budget, +0xC/+0x10 waypoint, +0x14 valid, +0x18
counter byte, +0x1C/+0x20 goal), plus the stagger byte 0x030032E5."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1 = int(sys.argv[3]), int(sys.argv[4])
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
PATHVT = {0x08049DA8}


def walk(r, depth=0, out=None):
    out = [] if out is None else out
    if depth > 6 or not (0x02000000 <= r < 0x02040000):
        return out
    vt = e.r32(r)
    if vt in PATHVT:
        s = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
        out.append((hex(r), [s(e.r32(r + o)) for o in (0xC, 0x10)], e.r8(r + 0x14), e.r8(r + 0x18), [s(e.r32(r + o)) for o in (0x1C, 0x20)]))
        return out
    for k in range(1, 8):
        p = e.r32(r + 4 * k)
        if 0x02000000 <= p < 0x02040000 and 0x08000000 <= e.r32(p) < 0x08100000:
            walk(p, depth + 1, out)
    return out


for f in range(f1 + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f < f0:
        continue
    row = []
    for o in enemies(e):
        st = o[0] + 0x238
        n = e.r32(st + 4)
        rt = e.r32(st + 8 + 8 * (n - 1)) if n else 0
        row.append(([hex(x) for x in ai_state(e, o[0])], walk(rt)))
    print(f, 'stagger', e.r8(0x030032E5), row)
