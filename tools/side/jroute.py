"""The Janemba saga (HFIL) route, played by the real game from work/side/janemba_start.ss (mksave_side.py janemba).

Goals: tools/story_playthrough/driver.py + tools/superbuu/sbdriver.py. ('mark', name) saves work/side/j_<name>.ss.

usage: jroute.py [--from MARK] [--to MARK] [--cap N] [--keys out.json]"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = ROOT / 'work' / 'side'
OUT.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
import sbdriver  # noqa: E402,F401  (registers the extra goal kinds)
sys.path.insert(0, str(HERE))
import sidedriver  # noqa: E402,F401
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402

A, B, SEL, START, RIGHT, LEFT, UP, DOWN, R, L = (1 << i for i in range(10))

ROUTE = [
    ('mark', 'start'),
    # 1-5: s066 on arrival (King Yemma: Janemba; Goku), then the Instant Transmission pad (A, facing up) -> the
    # Other World map 0-51 (start label 0 = 1-5): LEFT to label 1 = HFIL, A -> 18-1
    ('expect', 100), ('wait', 30),
    ('walk', 344, 214), ('talk', 'U'), ('wait', 190), ('mark', 'ow_map'),
    ('hold', LEFT, 40), ('wait', 10), ('hold', A, 3), ('areaw', 18, 1), ('wait', 30), ('mark', 'hfil'),
]


def run(frm=None, to=None, cap=10 ** 7, keys_out=None):
    goals = ROUTE
    st = OUT / 'janemba_start.ss'
    if frm:
        i = goals.index(('mark', frm))
        goals = goals[i + 1:]
        st = OUT / f'j_{frm}.ss'
    if to:
        goals = goals[:goals.index(('mark', to)) + 1]
    e = start_emu(str(st))
    d = Driver(goals)
    log = []
    prev = None
    f = 0
    def on_mark(e, name):
        e.save_file(str(OUT / f'j_{name}.ss'))
        print(f, 'mark', name, status(e), flush=True)
    d.on_mark = on_mark
    while not d.done() and f < cap:
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        log.append(k)
        s = status(e)
        key = (tuple(s['area']), s['scene'], d.k)
        if key != prev:
            print(f, 'goal', d.k, d.goals[d.k] if not d.done() else None, s)
        prev = key
        f += 1
    print('end', f, 'done' if d.done() else 'NOT DONE', status(e))
    if keys_out:
        Path(keys_out).write_text(json.dumps(log))
    return e


if __name__ == '__main__':
    a = sys.argv
    opt = lambda n: a[a.index(n) + 1] if n in a else None
    run(opt('--from'), opt('--to'), int(opt('--cap') or 10 ** 7), opt('--keys'))
