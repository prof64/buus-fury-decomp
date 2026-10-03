"""keys.py STATE "R:20,0:10,A:3" [--save OUT] [--shots N]: run a key program (route.program_keys syntax), screenshot
every N frames into work/superbuu/k_###.png and at the end k_end.png"""
import sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent / 'work' / 'superbuu'   # states, key logs (local)
OUT.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(HERE.parent.parent / 'tools' / 'story_playthrough'))
from route import start_emu, status, program_keys  # noqa: E402


def p(x):
    q = Path(x)
    return str(q if q.is_absolute() else OUT / q)


if __name__ == '__main__':
    arg = lambda k, d=None: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d
    e = start_emu(p(sys.argv[1]))
    ks = program_keys(sys.argv[2])
    n = int(arg('--shots', 0))
    prev = None
    for i, k in enumerate(ks):
        e.set_keys(k)
        e.run(1)
        s = status(e)
        key = (tuple(s['area']), s['scene'], tuple(s['p'] or ()))
        if key != prev and i % 10 == 0:
            print(i, s)
        prev = key
        if n and i % n == n - 1:
            e.screenshot(p(f'k_{i:04d}.png'), 1)
    print('end', status(e))
    e.screenshot(p('k_end.png'), 1)
    if arg('--save'):
        e.save_file(p(arg('--save')))
