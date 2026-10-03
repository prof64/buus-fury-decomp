"""story_watch.py STATE.ss "GOALS" cap [OUT.ss] -- run ShipDriver goals, print a line whenever the area / scene /
player presence / enemy set / active character changes (story exploration); saves OUT.ss at the end"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY, enemies, alloc_keys  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
d = ShipDriver(eval(sys.argv[2], {'KEY': KEY, 'alloc_keys': alloc_keys}))
cap = int(sys.argv[3])
out = sys.argv[4] if len(sys.argv) > 4 else None
last = None
for f in range(cap):
    if d.done():
        print('done', f)
        break
    k = d.keys(e)
    e.set_keys(k)
    e.run(1)
    s = status(e)
    ens = [(o[5], o[4] >> 10) for o in enemies(e)]
    key = (tuple(s['area']), s['scene'] > 0, s['p'] is None, tuple(t for t, _ in ens), e.r8(0x03001068))
    if key != last:
        print(f, s['area'], 'scene', s['scene'], 'p', s['p'], 'hp', s['hp'], 'char', e.r8(0x03001068), 'ens', ens,
              flush=True)
        last = key
    if s['area'][:2] == [0, 2]:
        print('dead')
        break
if out:
    e.save_file(os.path.abspath(out))
print(status(e))
