"""trace_fight.py STATE.ss "GOALS" [every] -- print player / HP / enemies every N frames while running goals"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, alloc_keys, KEY, objs  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
d = ShipDriver(eval(sys.argv[2], {'alloc_keys': alloc_keys, 'KEY': KEY}))
every = int(sys.argv[3]) if len(sys.argv) > 3 else 60
f = 0
while not d.done() and f < 20000:
    k = d.keys(e)
    e.set_keys(k)
    e.run(1)
    s = status(e)
    if f % every == 0 or s['hp'] == 0:
        ens = [(o[2], o[3]) for o in objs(e) if o[0] == 'enemy']
        print(f, f'k{k:03x}', s['area'], s['p'], 'hp', s['hp'], 'goal', d.k, 'en', ens)
        if s['hp'] == 0:
            break
    f += 1
