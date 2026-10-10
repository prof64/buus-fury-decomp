"""go.py IN.ss "GOALS" [OUT.ss] [--cap N] [--shot OUT.png]  -- run ShipDriver goals from a state (paths relative
to the cwd); prints area / scene / goal changes and the end status"""
import ast
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, alloc_keys, KEY  # noqa: E402
from route import start_emu  # noqa: E402

args = sys.argv[1:]
cap = 30000
shot = None
if '--cap' in args:
    i = args.index('--cap'); cap = int(args[i + 1]); del args[i:i + 2]
if '--shot' in args:
    i = args.index('--shot'); shot = args[i + 1]; del args[i:i + 2]
e = start_emu(os.path.abspath(args[0]))
d = ShipDriver(eval(args[1], {'alloc_keys': alloc_keys, 'KEY': KEY}))
prev = None
f = 0
err = None
try:
    while not d.done() and f < cap:
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        s = status(e)
        key = (tuple(s['area']), s['scene'], d.k)
        if key != prev:
            print(f, 'goal', d.k, d.goals[d.k] if not d.done() else None, f'keys {k:03x}', s)
        prev = key
        f += 1
except RuntimeError as x:
    err = x
print('end', f, 'done' if d.done() else 'NOT DONE', err or '', status(e))
e.set_keys(0)
if len(args) > 2:
    e.save_file(os.path.abspath(args[2]))
if shot:
    e.screenshot(os.path.abspath(shot), scale=2)
