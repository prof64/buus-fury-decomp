"""db3_trace.py RUNDIR [STEP]: replay RUNDIR/keys.json and print a line whenever the area, the Hamburger count or
the level changes, and every STEP frames (default 300): frame, area, pos, hp/max, burgers, level, enemies (type:hp)."""
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status, enemies, level_of, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

GD, REC = 0x03001068, 0x03001560
run = Path(os.path.abspath(sys.argv[1]))
step = int(sys.argv[2]) if len(sys.argv) > 2 else 300
f0 = int(sys.argv[3]) if len(sys.argv) > 3 else 0
f1 = int(sys.argv[4]) if len(sys.argv) > 4 else 10 ** 9
log = json.loads((run / 'keys.json').read_text())
e = start_emu(str(run / (log['from'] + '.ss' if log.get('from') else 'start.ss')))
prev = None
for i, k in enumerate(log['keys'][:f1]):
    e.set_keys(k)
    e.run(1)
    if i < f0:
        continue
    s = status(e)
    c = e.r8(GD)
    r = REC + 0x68 * c
    key = (tuple(s['area']), e.r8(GD + 2 + 3), level_of(e, c))
    if key != prev or (i - f0) % step == 0:
        print(i, k, s['area'], s['p'], 'scene', s['scene'], 'hp', e.r16(r + 0x16), '/', e.r16(r + 0x18), 'burgers', key[1],
              'lv', key[2], [(o[5], o[1], o[2], o[3], o[4], [hex(x) for x in ai_state(e, o[0])][-1:]) for o in enemies(e)],
              flush=True)
    prev = key
