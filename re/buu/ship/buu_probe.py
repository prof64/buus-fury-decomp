"""buu_probe.py STATE.ss -- play to the 8-71 fight, then for several x offsets keep the player beside Buu facing him
and mash A; print Buu's HP / state and the player's HP every 10 frames (which offsets land punches)"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY, enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
d = ShipDriver([('story', 8, 70, True)])
for f in range(20000):
    s = status(e)
    if s['scene'] == 0 and any(o[5] == 27 for o in enemies(e)):
        break
    e.set_keys(d.keys(e))
    e.run(1)
base = e.save()
TGT = int(sys.argv[3]) if len(sys.argv) > 3 else 27
DY = int(sys.argv[4]) if len(sys.argv) > 4 else 0
for off in [int(x) for x in sys.argv[2].split(',')]:
    e.load(base)
    print('offset', off)
    for f in range(600):
        s = status(e)
        b = next((o for o in enemies(e) if o[5] == TGT), None)
        bu = next((o for o in enemies(e) if o[5] == 27), None)
        if b is None or s['p'] is None:
            k = 0
        else:
            x, y = s['p'][0], s['p'][1]
            tx, ty = b[1] + off, b[2] + DY
            k = 0
            if abs(x - tx) > 2 or abs(y - ty) > 2:
                if abs(x - tx) > 2:
                    k |= KEY.RIGHT if tx > x else KEY.LEFT
                if abs(y - ty) > 2:
                    k |= KEY.DOWN if ty > y else KEY.UP
            else:
                face = 2 if off > 0 else 3
                if s['p'][2] != face:
                    k = KEY.LEFT if off > 0 else KEY.RIGHT
                else:
                    k = KEY.A if f % 6 < 2 else 0
        e.set_keys(k)
        e.run(1)
        if f % 20 == 0 and b is not None:
            print(" ", f, s["p"], "hp", s["hp"], "tgt", b[1], b[2], b[4] >> 10, [hex(v) for v in ai_state(e, b[0])], "buu", bu and bu[4] >> 10, bu and [hex(v) for v in ai_state(e, bu[0])], "pac", e.r32(0x0300302C))
