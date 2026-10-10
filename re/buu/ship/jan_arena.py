"""jan_arena.py START.ss [--from MARK] [--to MARK] [--cap N] [--wait N]  -- the side-content agent's Janemba arena
route (tools/side/jroute.py ARENA, unchanged) from the REAL route's state, into work/ship/jan/ (or $JAN_OUT):
j_<mark>.ss, a_keys.json (the keys from this run's start)."""
import os
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'side'))
import jroute  # noqa: E402

OUT = Path(os.environ.get('JAN_OUT') or ROOT / 'work' / 'ship' / 'jan').resolve()
jroute.OUT = OUT
# (the real state has s448 behind it: no event on the load)
jroute.ROUTES['arena'] = ([g for i, g in enumerate(jroute.ARENA) if i not in (1, 2)], 'arena_start.ss')
a = sys.argv
opt = lambda n: a[a.index(n) + 1] if n in a else None  # noqa: E731
if not opt('--from'):
    shutil.copyfile(a[1], OUT / 'arena_start.ss')
jroute.run(opt('--from'), opt('--to'), int(opt('--cap') or 10 ** 7), str(OUT / 'a_keys.json'), 'arena',
           int(opt('--wait') or 0))
