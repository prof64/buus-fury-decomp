"""try_route.py with the sbdriver goals (boss, exitto, walke, areaw, node, prog, fly, ...).

usage: python tools/side/try.py STATE.ss "GOALS" [--save OUT.ss] [--cap FRAMES] [--quiet]"""
import ast
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
import sbdriver  # noqa: E402,F401
sys.path.insert(0, str(ROOT / 'tools' / 'side'))
import sidedriver  # noqa: E402,F401
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402


def main():
    st, goals = sys.argv[1], ast.literal_eval(sys.argv[2])
    a = sys.argv
    save = a[a.index('--save') + 1] if '--save' in a else None
    cap = int(a[a.index('--cap') + 1]) if '--cap' in a else 20000
    e = start_emu(st)
    d = Driver(goals)
    prev = None
    f = 0
    err = None
    try:
        while not d.done() and f < cap:
            k = d.keys(e)
            e.set_keys(k)
            e.run(1)
            s = status(e)
            key = (tuple(s['area']), s['scene'], d.k, s['p'] is None)
            if key != prev:
                print(f, 'goal', d.k, d.goals[d.k] if not d.done() else None, f'keys {k:03x}', s, flush=True)
            prev = key
            f += 1
    except Exception as x:  # report and still save
        err = x
    print('end', f, 'done' if d.done() else f'NOT DONE ({err})', status(e))
    e.set_keys(0)
    if save:
        e.save_file(save)
        print('saved', save)


if __name__ == '__main__':
    main()
