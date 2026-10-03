"""seg_rng.py SEGDIR NAME frame [frame2] -- replay a segment and list the RNG calls (next() 0x0803FAAC: caller LR,
and the rand/chance wrapper's caller) during video frames frame..frame2"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401  (sets the paths)
from route import start_emu  # noqa: E402

NEXT, RAND, CHANCE = 0x0803FAAC, 0x0803FACC, 0x0803FADE
d, name, a = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3])
b = int(sys.argv[4]) if len(sys.argv) > 4 else a
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(b + 1):
    if f == a:
        bps = [e.breakpoint(x) for x in (NEXT, RAND, CHANCE)]
    e.set_keys(keys[f])
    e.run(1)
    if f >= a:
        out = []
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            r = h['regs']
            out.append((hex(h['addr'] & ~1), hex(r[14]), r[0]))
        print(f, len([o for o in out if o[0] == hex(NEXT)]), out)
