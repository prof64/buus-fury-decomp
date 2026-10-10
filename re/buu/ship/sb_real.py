"""sb_real.py [--start STATE.ss] [--from MARK] [--to MARK] [--cap N] [-v]  -- the Super Buu agent's route
(tools/superbuu/sbroute.py ROUTE, unchanged) from the REAL route's states, into work/ship/sb/ (or $SB_OUT):
m_<mark>.ss at each mark, sbk_<from>.json = the keys of this run (from the start state / the --from mark), a mark
log line with the frame count; fail.ss on a game over / an error."""
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
import sbroute  # noqa: E402
from sbroute import Driver, start_emu, status  # noqa: E402

OUT = Path(os.environ.get('SB_OUT') or ROOT / 'work' / 'ship' / 'sb').resolve()
OUT.mkdir(parents=True, exist_ok=True)
a = sys.argv
opt = lambda n, d=None: a[a.index(n) + 1] if n in a else d  # noqa: E731
goals = list(sbroute.ROUTE)
if os.environ.get('SB_MELEE'):
    # the real Gohan is a melee build (STR 134, POW 53): punches instead of the route's Electric Kamehameha
    goals = [('boss', g[1]) if g[0] == 'beam' else g for g in goals]
frm, to, cap = opt('--from'), opt('--to'), int(opt('--cap', 10 ** 7))
st = opt('--start') or str(OUT / 'sb_start.ss')
if frm:
    goals = goals[goals.index(('mark', frm)) + 1:]
    if not opt('--start'):
        st = str(OUT / f'm_{frm}.ss')
if to:
    goals = goals[:goals.index(('mark', to)) + 1]
goals = goals[int(opt('--skip', 0)):]      # (the first goals after --from already played by hand)
e = start_emu(st)
d = Driver(goals)
f = 0
log = []
prev = None


def on_mark(ee, name):
    ee.save_file(str(OUT / f'm_{name}.ss'))
    print('mark', name, f, status(ee), flush=True)


d.on_mark = on_mark
err = None
try:
    while not d.done() and f < cap:
        k = d.keys(e)
        log.append(k)
        e.set_keys(k)
        e.run(1)
        f += 1
        s = status(e)
        key = (tuple(s['area']), s['scene'] > 0, d.k)
        if '-v' in a and key != prev:
            print(f, d.k, d.goals[d.k] if not d.done() else None, s, flush=True)
        prev = key
        if s['area'][:2] == [0, 2] and s['hp'] == 0:      # (0-2 with HP left is the Spirit Bomb's scene)
            raise RuntimeError('game over')
except Exception as x:  # noqa: BLE001
    err = x
    e.save_file(str(OUT / 'fail.ss'))
print('end', f, 'done' if d.done() else f'NOT DONE {err!r} at goal {d.k} {d.goals[d.k] if not d.done() else None}',
      status(e))
e.set_keys(0)
e.save_file(str(OUT / 'end.ss'))
(OUT / f"sbk_{frm or 'start'}{'_skip' if opt('--skip') else ''}.json").write_text(json.dumps({'from': frm, 'keys': log, 'failed': err is not None}))
