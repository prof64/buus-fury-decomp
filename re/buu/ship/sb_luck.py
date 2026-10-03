"""sb_luck.py MARK TO N0 N1 [JOBS] [MODE]  -- luck search over a stretch of the Super Buu route (sb_real.py) on the
REAL state: trial n first waits n * 7 frames (MODE 'wait', default) or throws n punches (MODE 'punch') at
work/ship/sb/m_MARK.ss, then plays --from MARK --to TO in work/ship/sb_t<n>/. Prints each trial's last lines."""
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PY = str(ROOT / '.venv' / 'bin' / 'python')
S = ROOT / 'work' / 'ship' / 'sb'
mark, to, n0, n1 = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
jobs = int(sys.argv[5]) if len(sys.argv) > 5 else 6
mode = sys.argv[6] if len(sys.argv) > 6 else 'wait'


def trial(n):
    t = ROOT / 'work' / 'ship' / f'sb_t{n}'
    t.mkdir(parents=True, exist_ok=True)
    if n == 0:
        shutil.copyfile(S / f'm_{mark}.ss', t / f'm_{mark}.ss')
    else:
        goals = f"[('wait', {n * 7})]" if mode == 'wait' else f"[('keys', [(A, 2, 34)] * {n} + [(0, 1, 1)])]"
        subprocess.run([PY, str(ROOT / 're/buu/ship/fat_goals.py'), str(S / f'm_{mark}.ss'), str(t / f'm_{mark}.ss'),
                        goals], capture_output=True, text=True, cwd=ROOT)
    r = subprocess.run([PY, str(ROOT / 're/buu/ship/sb_real.py'), '--from', mark, '--to', to],
                       capture_output=True, text=True, cwd=ROOT, env=dict(os.environ, SB_OUT=str(t)))
    out = [x for x in (r.stdout + r.stderr).splitlines() if x.startswith('end ') or x.startswith('mark ')]
    return n, ' | '.join(x[:150] for x in out[-2:])


with ThreadPoolExecutor(jobs) as ex:
    for n, res in ex.map(trial, range(n0, n1)):
        print(n, res, flush=True)
