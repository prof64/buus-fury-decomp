"""vt_watch.py STATE.ss "GOALS" cap AREA -- run goals; in area AREA ('w-a') print the object world's vtable counts
whenever they change (finding pickup / effect object kinds)"""
import os
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY, alloc_keys  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
d = ShipDriver(eval(sys.argv[2], {'KEY': KEY, 'alloc_keys': alloc_keys}))
area = [int(x) for x in sys.argv[4].split('-')]
last = None
for f in range(int(sys.argv[3])):
    if d.done():
        break
    e.set_keys(d.keys(e))
    e.run(1)
    s = status(e)
    if s['area'][:2] != area:
        last = None
        continue
    n = e.r32(0x03001C4C + 4)
    c = Counter(hex(e.r32(e.r32(0x03001C4C + 8 + 4 * k))) for k in range(min(n, 255)))
    key = tuple(sorted(c.items()))
    if key != last:
        print(f, dict(c), 'equip n', e.r16(0x03001068 + 0xE4), flush=True)
        last = key
