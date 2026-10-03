"""run goals and print, every N frames, the player HP and the HP of enemies of given types.

usage: python tools/side/hpwatch.py STATE.ss "GOALS" "[types]" [--every N] [--cap F]"""
import ast
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
for p in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 'tools/script_lockstep', 'tools/side'):
    sys.path.insert(0, str(ROOT / p))
import sbdriver  # noqa: E402,F401
import sidedriver  # noqa: E402,F401
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402
from common_field import enemy_detail  # noqa: E402

a = sys.argv
st, goals, types = a[1], ast.literal_eval(a[2]), ast.literal_eval(a[3])
every = int(a[a.index('--every') + 1]) if '--every' in a else 300
cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 20000
e = start_emu(st)
d = Driver(goals)
f = 0
while not d.done() and f < cap:
    k = d.keys(e)
    e.set_keys(k)
    e.run(1)
    if f % every == 0:
        s = status(e)
        ens = [(hex(x['type']), x['hp1024'] >> 10, x['x'] >> 8, x['y'] >> 8) for x in enemy_detail(e) if x['type'] in types]
        print(f, 'goal', d.k, 'hp', s['hp'], 'p', s['p'], 'scene', s['scene'], ens, flush=True)
    f += 1
