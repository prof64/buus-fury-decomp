"""jan_hfil.py START.ss [--from MARK] [--to MARK] [--cap N]  -- the side-content agent's HFIL corridor route
(tools/side/hfil.py ROUTE, unchanged) played from the REAL route's state, into work/ship/jan/ (h_<mark>.ss +
h_<mark>.keys.json = the keys from this run's start, h_end.ss, h_keys.json)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'side'))
import hfil  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import jan_side  # noqa: E402,F401  (hfil.SideDriver = RealSide)

import os  # noqa: E402
OUT = Path(os.environ.get('JAN_OUT') or ROOT / 'work' / 'ship' / 'jan').resolve()
hfil.OUT = OUT
a = sys.argv
opt = lambda n: a[a.index(n) + 1] if n in a else None  # noqa: E731
goals = list(hfil.ROUTE)
st = Path(a[1]).resolve()
if opt('--from'):
    i = goals.index(('mark', opt('--from')))
    import maze
    for g in goals[:i]:
        if g[0] == 'openwall':
            maze.open_wall(g[1], g[2])
    goals = goals[i + 1:]
    st = OUT / f"h_{opt('--from')}.ss"
if opt('--to'):
    goals = goals[:goals.index(('mark', opt('--to'))) + 1]
hfil.run(st, goals, save=OUT / 'h_end.ss', cap=int(opt('--cap') or 10 ** 7), keys_out=str(OUT / 'h_keys.json'),
         quiet='-v' not in a)
