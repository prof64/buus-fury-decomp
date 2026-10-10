"""watch2.py STATE.ss "GOALS" START END STEP -- one driver from frame 0 (so its state is the real run's); prints
keys / player / hp / enemies (pos, hp, AI state, skipped?) every STEP frames between START and END"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status, KEY, enemies, ai_state  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
if sys.argv[2].startswith('@'):
    # @MARK: the fat Buu route's goals after that mark
    import ship_route
    gl = ship_route.goals_fatbuu(e)
    gl = gl[gl.index(('mark', sys.argv[2][1:])) + 1:]
    d = ShipDriver([g for g in gl if g[0] != 'mark'])
else:
    d = ShipDriver(eval(sys.argv[2], {'KEY': KEY}))
a, b, st = int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
for f in range(b):
    if d.done():
        break
    k = d.keys(e)
    e.set_keys(k or 0)
    e.run(1)
    s = status(e)
    if s['area'] != getattr(d, 'w_area', None) or (s['hp'] == 0) != getattr(d, 'w_dead', False):
        d.w_area, d.w_dead = s['area'], s['hp'] == 0
        print('--', f, s['area'], s['p'], 'hp', s['hp'], 'goal', d.k, d.goals[d.k] if not d.done() else None)
    if f >= a and (f - a) % st == 0:
        print(f, 'k%03x' % (k or 0), s['area'], s['p'], 'hp', s['hp'],
              [(o[1], o[2], o[4], o[5], [hex(x) for x in ai_state(e, o[0]) or []], 'skip' if o[0] in d.skip else '',
                d.tgt.get(o[0])) for o in enemies(e)])
