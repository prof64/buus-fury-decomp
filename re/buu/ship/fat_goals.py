"""fat_goals.py START.ss OUT.ss "GOALS" [CAP]  -- play a ShipDriver goal list (a transit between two dungeon routes of
the real fat Buu route) from START.ss; writes OUT.ss and OUT.keys.json ({'keys': per-frame keys from START})."""
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY  # noqa: E402
from route import start_emu  # noqa: E402
import ship_route  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
out = os.path.abspath(sys.argv[2])
env = {'KEY': KEY, 'nav': ship_route.nav, 'charsel': ship_route.charsel, 'rest_at': ship_route.rest_at,
       'alloc_keys': ship_route.alloc_keys, 'A': KEY.A, 'B': KEY.B, 'U': KEY.UP, 'D': KEY.DOWN, 'L': KEY.LEFT,
       'R': KEY.RIGHT}
d = ShipDriver(eval(sys.argv[3], env))
cap = int(sys.argv[4]) if len(sys.argv) > 4 else 60000
keys = []
prev = None
while not d.done():
    k = d.keys(e) or 0
    e.set_keys(k)
    e.run(1)
    keys.append(k)
    s = status(e)
    if (s['area'], d.k) != prev:
        prev = (s['area'], d.k)
        print(len(keys), s, 'goal', d.k, d.goals[d.k] if not d.done() else None, flush=True)
    if len(keys) > cap or s['area'][:2] == [0, 2]:
        e.save_file(out + '.fail.ss')
        raise SystemExit(f'FAILED at goal {d.k} {d.goals[d.k]} {s}')
e.set_keys(0)
e.save_file(out)
Path(out[:-3] + '.keys.json').write_text(json.dumps({'from': None, 'keys': keys}))
print('total', len(keys), status(e))
