"""watch.py RUNDIR F0 F1 STEP [--shot]: replay RUNDIR/keys.json from its start and print the status + enemies every
STEP frames in [F0, F1) (debugging a route); --shot saves /tmp/dbw_<frame>.png."""
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status, enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

run = Path(os.path.abspath(sys.argv[1]))
f0, f1, step = map(int, sys.argv[2:5])
log = json.loads((run / 'keys.json').read_text())
e = start_emu(str(run / (log['from'] + '.ss' if log.get('from') else 'start.ss')))
for i, k in enumerate(log['keys'][:f1]):
    e.set_keys(k)
    e.run(1)
    if i >= f0 and (i - f0) % step == 0:
        print(i, k, status(e), [(o[1], o[2], o[3], o[4], o[5], [hex(x) for x in ai_state(e, o[0])]) for o in enemies(e)])
        if '--shot' in sys.argv:
            e.screenshot(f'/tmp/dbw_{i}.png', 1)
if '--save' in sys.argv:
    e.save_file(sys.argv[sys.argv.index('--save') + 1])
