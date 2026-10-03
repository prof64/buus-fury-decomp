"""db4_try.py STATE "GOALS (python list)" MAXFRAMES STEP [--shot name] [--save out.ss]: run a goal list with
Db4Driver from a state, print the status every STEP frames (debugging)."""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import db_route  # noqa: E402
from db_route import nav, U, D, L, R, A, B, START, RS  # noqa: E402,F401
from db4_driver import Db4Driver, status, enemies  # noqa: E402
from ship_driver import ai_state  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
d = Db4Driver(eval(sys.argv[2]))
n, step = int(sys.argv[3]), int(sys.argv[4])
f = 0
try:
    while not d.done() and f < n:
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        f += 1
        if f % step == 0:
            print(f, d.k, k, status(e), [(o[1], o[2], o[3], o[4], o[5], [hex(x) for x in ai_state(e, o[0])][-1:])
                                         for o in enemies(e)][:6], flush=True)
except RuntimeError as ex:
    print('FAILED', ex)
print('end', f, d.k, status(e))
e.set_keys(0)
if '--shot' in sys.argv:
    e.screenshot('/tmp/poke_%s.png' % sys.argv[sys.argv.index('--shot') + 1], 2)
if '--save' in sys.argv:
    e.save_file(os.path.abspath(sys.argv[sys.argv.index('--save') + 1]))
