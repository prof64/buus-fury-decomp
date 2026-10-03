"""play.py STATE "GOALS" [--save OUT] [--cap N] [--shot PNG] [--every N]
Runs driver goals from a state (paths relative to work/superbuu/), prints changes, optional end screenshot."""
import ast
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent / 'work' / 'superbuu'   # states, key logs (local)
OUT.mkdir(parents=True, exist_ok=True)
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(HERE))
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402
import sbdriver  # noqa: E402,F401  (extra goals)


def p(x):
    q = Path(x)
    return str(q if q.is_absolute() else OUT / q)


def main():
    st, goals = sys.argv[1], ast.literal_eval(sys.argv[2])
    arg = lambda k, d=None: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d
    save, cap, shot, every = arg('--save'), int(arg('--cap', 30000)), arg('--shot'), int(arg('--every', 600))
    e = start_emu(p(st))
    d = Driver(goals)
    prev = None
    f = 0
    shots = []
    while not d.done() and f < cap:
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        s = status(e)
        key = (tuple(s['area']), s['scene'], d.k)
        if key != prev or f % every == 0:
            print(f, 'goal', d.k, d.goals[d.k] if not d.done() else None, f'keys {k:03x}', s, flush=True)
        prev = key
        f += 1
    print('end', f, 'done' if d.done() else 'NOT DONE', status(e))
    e.set_keys(0)
    if save:
        e.save_file(p(save))
    if shot:
        e.screenshot(p(shot), 1)


if __name__ == '__main__':
    main()
