"""Route exploration for the story playthrough (headless, muted): run a key program from a save state and print the
area / player / scene activity, optionally saving the end state.

usage: python3 tools/story_playthrough/explore.py <state.ss | reset> <program> [--save out.ss] [--every N]
  program: comma-separated segments KEYS:N[:a]  (KEYS = 0 or names joined by '+', e.g. R, U+R, A; ':a' adds the
           A mash 4 of every 30 frames)"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from route import program_keys, start_emu, status  # noqa: E402


def main():
    st, prog = sys.argv[1], sys.argv[2]
    save = sys.argv[sys.argv.index('--save') + 1] if '--save' in sys.argv else None
    every = int(sys.argv[sys.argv.index('--every') + 1]) if '--every' in sys.argv else 30
    e = start_emu(st)
    keys = program_keys(prog)
    prev = None
    for f, k in enumerate(keys):
        e.set_keys(k)
        e.run(1)
        s = status(e)
        key = (s['area'], s['scene'])
        if f % every == 0 or key != prev or f == len(keys) - 1:
            print(f, f"keys {k:03x}", s)
        prev = key
    e.set_keys(0)
    if save:
        e.save_file(save)
        print('saved', save)


if __name__ == '__main__':
    main()
