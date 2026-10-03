"""segwatch.py SEGDIR NAME F0 F1 ADDR [LEN]: replay a segment on the emulator and print every write that changes
RAM [ADDR, ADDR + LEN) (default 4) during video frames F0..F1: frame, address, old -> new, pc, lr."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name = Path(sys.argv[1]).resolve(), sys.argv[2]
f0, f1, addr = int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5], 16)
ln = int(sys.argv[6]) if len(sys.argv) > 6 else 4
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(f1 + 1):
    if f == f0:
        e.watch(addr, addr + ln, 'w')
    e.set_keys(keys[f])
    e.run(1)
    if f >= f0:
        for h in e.hits():
            if h['kind'] == 'watch' and (h.get('new') != h.get('old') or '--all' in sys.argv):
                r = h['regs']
                print(f, hex(h['addr']), hex(h['old']), '->', hex(h['new']), 'pc', hex(r[15]), 'lr', hex(r[14] & 0xffffffff),
                      'r0-3', [hex(x & 0xffffffff) for x in r[:4]])
