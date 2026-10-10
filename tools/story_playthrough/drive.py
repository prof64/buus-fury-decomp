"""Try a goal list from a save state (route planning): python3 drive.py <state> "<python goal list>" [--save out.ss]"""
import ast
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402


def main():
    st = sys.argv[1]
    goals = ast.literal_eval(sys.argv[2])
    cap = int(sys.argv[sys.argv.index('--max') + 1]) if '--max' in sys.argv else 20000
    e = start_emu(st)
    d = Driver(goals)
    prev = None
    f = 0
    while not d.done() and f < cap:
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        s = status(e)
        key = (d.k, tuple(s['area']), s['scene'] > 0)
        if key != prev or f % 120 == 0:
            print(f, 'goal', d.k, d.goals[d.k] if not d.done() else '-', f'keys {k:03x}', s)
        prev = key
        f += 1
    print('end', f, status(e))
    e.set_keys(0)
    if '--save' in sys.argv:
        e.save_file(sys.argv[sys.argv.index('--save') + 1])
    if '--shot' in sys.argv:
        e.screenshot(sys.argv[sys.argv.index('--shot') + 1], scale=2)


if __name__ == '__main__':
    main()
