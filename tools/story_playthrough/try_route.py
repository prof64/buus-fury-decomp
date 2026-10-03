"""Try route goals from a save state (headless, muted): runs driver.py's Driver on the goals and prints the area /
player / scene whenever they change, the goal index, and the end state; optionally saves the end state.

usage: python3 tools/story_playthrough/try_route.py STATE.ss "GOALS" [--save OUT.ss] [--cap FRAMES]
  GOALS: a Python list literal of driver goals, e.g. "[('walk', 256, 100), ('hold', 64, 30), ('area', 1, 16)]"
  (key masks: A 1, B 2, SELECT 4, START 8, RIGHT 16, LEFT 32, UP 64, DOWN 128, R 256, L 512)"""
import ast
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from driver import Driver  # noqa: E402
from route import start_emu, status  # noqa: E402


def main():
    st, goals = sys.argv[1], ast.literal_eval(sys.argv[2])
    save = sys.argv[sys.argv.index('--save') + 1] if '--save' in sys.argv else None
    cap = int(sys.argv[sys.argv.index('--cap') + 1]) if '--cap' in sys.argv else 20000
    e = start_emu(st)
    d = Driver(goals)
    prev = None
    f = 0
    while not d.done() and f < cap:
        k = d.keys(e)
        e.set_keys(k)
        e.run(1)
        s = status(e)
        key = (tuple(s['area']), s['scene'], d.k)
        if key != prev or f % 300 == 0:
            print(f, 'goal', d.k, d.goals[d.k] if not d.done() else None, f'keys {k:03x}', s)
        prev = key
        f += 1
    print('end', f, 'done' if d.done() else 'NOT DONE', status(e))
    e.set_keys(0)
    if save:
        e.save_file(save)
        print('saved', save)


if __name__ == '__main__':
    main()
