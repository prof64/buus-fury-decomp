"""segbp.py SEGDIR NAME F0 F1 ADDR [ADDR ...]: replay a segment on the emulator with breakpoints at the ROM
addresses during video frames F0..F1; prints per frame the hits (address, r0..r3, lr)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1 = int(sys.argv[3]), int(sys.argv[4])
addrs = [int(x, 16) for x in sys.argv[5:]]
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(f1 + 1):
    if f == f0:
        for a in addrs:
            e.breakpoint(a)
    e.set_keys(keys[f])
    e.run(1)
    if f >= f0:
        out = []
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            r = h['regs']
            out.append('%x(%x,%x,%x,%x lr %x)' % (h['addr'] & ~1, r[0], r[1], r[2], r[3], r[14]))
        print(f, len(out), ' '.join(out[:40]))
