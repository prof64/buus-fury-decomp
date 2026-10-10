"""grind_safe.py START.ss OUT.ss LEVEL W1 A1 W2 A2 [CAP]  -- ('grind', LEVEL, [(W1, A1), (W2, A2)]) with a retry on
death: a snapshot (emulator + driver) each time the player stands in W1-A1 (the safe room); a game over goes back to
the last snapshot and throws one more punch in the air first (each draws the RNG), up to 16. The key log is the one
of the surviving line only (OUT.keys.json), so the result is still one continuous real input sequence."""
import copy
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY, level_of  # noqa: E402
from route import start_emu  # noqa: E402

a = sys.argv
e = start_emu(os.path.abspath(a[1]))
out = os.path.abspath(a[2])
lvl, w1, a1, w2, a2 = (int(x) for x in a[3:8])
cap = int(a[8]) if len(a) > 8 else 400000
d = ShipDriver([('grind', lvl, [(w1, a1), (w2, a2)]), ('nav', w1, a1, True), ('wait', 30)])
keys = []
snap = None
tries = 0
last_area = None
while not d.done() and len(keys) < cap:
    s = status(e)
    if s['area'][:2] == [w1, a1] and s['p'] is not None and s['scene'] == 0 and last_area != (w1, a1) and not d.prog:
        snap = (e.save(), copy.deepcopy(d), len(keys))
        tries = 0
        print(len(keys), 'level', level_of(e), s, flush=True)
    if s['p'] is not None:
        last_area = tuple(s['area'][:2])
    if s['area'][:2] == [0, 2] or snap is not None and len(keys) - snap[2] > 8000:   # dead, or stuck in a room
        if snap is None or tries >= 16:
            raise SystemExit(f'FAILED: dead, no way on {s}')
        tries += 1
        e.load(snap[0])
        d = copy.deepcopy(snap[1])
        del keys[snap[2]:]
        d.prog = ([KEY.A] * 2 + [0] * 34) * tries
        last_area = (w1, a1)
        print('  retry', tries, flush=True)
        continue
    k = d.keys(e) or 0
    e.set_keys(k)
    e.run(1)
    keys.append(k)
e.set_keys(0)
e.save_file(out)
Path(out[:-3] + '.keys.json').write_text(json.dumps({'from': None, 'keys': keys}))
print('total', len(keys), status(e), 'done' if d.done() else 'CAP')
