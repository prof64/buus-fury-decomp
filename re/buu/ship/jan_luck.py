"""jan_luck.py MARK TO N0 N1 [JOBS]  -- luck search over a stretch of the HFIL route (jan_hfil.py) on the REAL state:
trial n throws n punches in the air at work/ship/jan/h_MARK.ss first (each draws the RNG), then plays --from MARK
--to TO in work/ship/jan_t<n>/. Prints each trial's last line."""
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PY = str(ROOT / '.venv' / 'bin' / 'python')
J = ROOT / 'work' / 'ship' / 'jan'
mark, to, n0, n1 = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
jobs = int(sys.argv[5]) if len(sys.argv) > 5 else 6


def trial(n):
    t = ROOT / 'work' / 'ship' / f'jan_t{n}'
    t.mkdir(parents=True, exist_ok=True)
    if n == 0:
        shutil.copyfile(J / f'h_{mark}.ss', t / f'h_{mark}.ss')
    else:
        subprocess.run([PY, str(ROOT / 're/buu/ship/fat_goals.py'), str(J / f'h_{mark}.ss'), str(t / f'h_{mark}.ss'),
                        f"[('keys', [(A, 2, 34)] * {n} + [(0, 1, 1)])]"], capture_output=True, text=True, cwd=ROOT)
    r = subprocess.run([PY, str(ROOT / 're/buu/ship/jan_hfil.py'), 'x', '--from', mark, '--to', to],
                       capture_output=True, text=True, cwd=ROOT, env=dict(os.environ, JAN_OUT=str(t)))
    out = [x for x in (r.stdout + r.stderr).splitlines() if x.startswith('end ') or x.startswith('mark ')]
    return n, ' | '.join(x[:140] for x in out[-2:])


with ThreadPoolExecutor(jobs) as ex:
    for n, res in ex.map(trial, range(n0, n1)):
        print(n, res, flush=True)
