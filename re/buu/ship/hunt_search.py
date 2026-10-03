"""hunt_search.py STATE.ss OUT.json [W0 W1 STEP] -- luck search for the Bandana (equipment 126): from STATE (West City,
mark d03_city) punch the air W times, take the capsule, then ('hunt', 126); a try fails on a game over, on Trunks passing 20,500 XP (level 71
at 22,025: the low-level bandit group needs <= 70) or after 60,000 frames. Prints every try; the first success writes
OUT.json = {"wait": W, "keys": [...]} (the per-frame keys from STATE) and OUT.ss."""
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY  # noqa: E402
from route import start_emu  # noqa: E402

GD = 0x03001068
st, out = os.path.abspath(sys.argv[1]), sys.argv[2]
w0, w1, step = (int(x) for x in sys.argv[3:6]) if len(sys.argv) > 5 else (0, 40, 1)
e = start_emu(st)
base = e.save()


def have(e):
    n = e.r16(GD + 0xE4)
    return any(e.r32(GD + 0xE8 + 4 * i) & 0xFFFF == 126 for i in range(min(n, 250)))


for w in range(w0, w1, step):
    e.load(base)
    # w punches in the air first: each draws the RNG (the encounters' rolls follow from it; waiting does not)
    d = ShipDriver([('wait', 90), ('keys', [(KEY.A, 2, 34)] * w + [(0, 1, 1)]), ('unstick',), ('walk', 416, 360), ('face', 'U'), ('press', KEY.A),
                    ('wait', 60), ('hunt', 126)])
    keys = []
    why = 'cap'
    for f in range(60000):
        if d.done():
            why = 'ok'
            break
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        keys.append(k)
        if f % 30 == 0:
            if status(e)['area'][:2] == [0, 2]:
                why = 'dead'
                break
            if e.r32(0x03001560 + 0x68 * 3 + 0x24) > 20500:
                why = 'xp'
                break
    xp = e.r32(0x03001560 + 0x68 * 3 + 0x24)
    print('wait', w, why, 'frames', len(keys), 'xp', xp, 'hp', status(e)['hp'], 'bandana', have(e), flush=True)
    if why == 'ok' and have(e):
        Path(out).write_text(json.dumps({'wait': w, 'keys': keys}))
        e.set_keys(0)
        e.save_file(os.path.abspath(out.replace('.json', '.ss')))
        break
