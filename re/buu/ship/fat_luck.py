"""fat_luck.py ROUTE DIR MARK TO N0 N1 [JOBS]  -- luck search for a stretch of a tools/db57 route on the REAL state:
trial n throws n punches in the air at DIR/MARK.ss first (each draws the RNG, so the enemies' rolls differ), then
plays ROUTE --from MARK --to TO in DIR_t<n>/ (fat_db57.py). Prints each trial's outcome; a passing trial's dir holds
MARK.ss (after the punches), MARK.pre.keys.json (the punches), keys_MARK.json and end.ss (the state at TO)."""
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PY = str(ROOT / '.venv' / 'bin' / 'python')
route, d, mark, to, n0, n1 = sys.argv[1], Path(sys.argv[2]).resolve(), sys.argv[3], sys.argv[4], int(sys.argv[5]), int(sys.argv[6])
jobs = int(sys.argv[7]) if len(sys.argv) > 7 else 6


def trial(n):
    t = d.parent / f'{d.name}_t{n}'
    t.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(d / 'start.ss', t / 'start.ss')
    if n == 0:
        shutil.copyfile(d / f'{mark}.ss', t / f'{mark}.ss')
    else:
        r = subprocess.run([PY, str(ROOT / 're/buu/ship/fat_goals.py'), str(d / f'{mark}.ss'), str(t / f'{mark}.ss'),
                            f"[('keys', [(A, 2, 34)] * {n} + [(0, 1, 1)])]"], capture_output=True, text=True, cwd=ROOT)
        if not (t / f'{mark}.ss').exists():
            return n, 'pre failed ' + r.stdout[-200:]
        shutil.move(str(t / f'{mark}.keys.json'), str(t / f'{mark}.pre.keys.json'))
    r = subprocess.run([PY, str(ROOT / 're/buu/ship/fat_db57.py'), route, str(d / 'start.ss'), str(t), '--from', mark,
                        '--to', to], capture_output=True, text=True, cwd=ROOT)
    out = [x for x in (r.stdout + r.stderr).splitlines() if x.strip() and not x.startswith('  ') and '~~' not in x]
    ok = any(x.startswith('total') for x in out)
    return n, ('OK ' if ok else 'FAIL ') + ' | '.join(x[:150] for x in out[-3:])


with ThreadPoolExecutor(jobs) as ex:
    for n, res in ex.map(trial, range(n0, n1)):
        print(n, res, flush=True)
