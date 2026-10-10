"""objwatch.py STATE "GOALS" a b [step]: run goals, from frame a to b print the player and every world object
(kind, vtable, x, y) every `step` frames"""
import ast
import sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = ROOT / 'work' / 'superbuu'
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
import sbdriver  # noqa
from driver import Driver  # noqa
from route import start_emu, status  # noqa
from common_field import objs  # noqa

e = start_emu(str(OUT / sys.argv[1]))
d = Driver(ast.literal_eval(sys.argv[2]))
a, b = int(sys.argv[3]), int(sys.argv[4])
step = int(sys.argv[5]) if len(sys.argv) > 5 else 1
f = 0
while f < b:
    k = d.keys(e) if not d.done() else 0
    e.set_keys(k)
    e.run(1)
    f += 1
    if f >= a and f % step == 0:
        s = status(e)
        print(f, 'k', k, 'p', s['p'], 'hp', s['hp'], [o for o in objs(e) if o[0] not in ('hud',)])
