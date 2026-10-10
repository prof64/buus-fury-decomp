"""fightprobe.py STATE "GOALS" [every]: run goals, print player HP / EP and each enemy's type + HP every N frames"""
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
from common_field import enemy_detail  # noqa

e = start_emu(str(OUT / sys.argv[1]))
d = Driver(ast.literal_eval(sys.argv[2]))
every = int(sys.argv[3]) if len(sys.argv) > 3 else 200
f = 0
while not d.done() and f < 60000:
    e.set_keys(d.keys(e))
    e.run(1)
    f += 1
    if f % every == 0:
        s = status(e)
        rec = 0x03001560 + 0x68 * e.r8(0x03001068)
        ens = [(x.get('type'), x.get('hp1024', 0) >> 10) for x in enemy_detail(e)]
        print(f, s['area'], 'hp', s['hp'], 'ep', e.r16(rec + 0x1A), 'scene', s['scene'], ens, flush=True)
print('end', f, status(e))
