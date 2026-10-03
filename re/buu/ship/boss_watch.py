"""boss_watch.py STATE.ss "GOALS" [every] [frames] -- run goals, print player + every enemy (x, y, hp, AI state stack)"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, alloc_keys, KEY, enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
d = ShipDriver(eval(sys.argv[2], {'alloc_keys': alloc_keys, 'KEY': KEY}))
every = int(sys.argv[3]) if len(sys.argv) > 3 else 10
cap = int(sys.argv[4]) if len(sys.argv) > 4 else 3000
for f in range(cap):
    if d.done():
        break
    k = d.keys(e)
    e.set_keys(k)
    e.run(1)
    if f % every == 0:
        s = status(e)
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        print(f, f'k{k:03x}', s['p'], 'hp', s['hp'], 'ep', e.r16(rec + 0x1A), 'sl', e.r8(rec + 0x15),
              [(o[1], o[2], o[4] >> 10, [hex(x) for x in ai_state(e, o[0])]) for o in enemies(e)])
