"""Probe inputs from a save state (headless, muted): run a key program and print, every N frames, the player row
(position, facing, state), the record's skill slot / skill list, scene depth and the world's object kinds.

usage: python3 tools/story_playthrough/probe.py STATE.ss PROGRAM [--every N] [--save OUT.ss]
  PROGRAM: comma-separated KEYS:N segments (see explore.py: names joined by '+': A B L R U D S E LS RS, or 0)"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / 'tools' / 'script_lockstep'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import program_keys, start_emu, status  # noqa: E402
from common_field import player_row, record_of  # noqa: E402
from rec_objects import objects  # noqa: E402


def main():
    st, prog = sys.argv[1], sys.argv[2]
    every = int(sys.argv[sys.argv.index('--every') + 1]) if '--every' in sys.argv else 5
    save = sys.argv[sys.argv.index('--save') + 1] if '--save' in sys.argv else None
    e = start_emu(st)
    for f, k in enumerate(program_keys(prog)):
        e.set_keys(k)
        e.run(1)
        if f % every == 0:
            r = record_of(e)
            s = status(e)
            print(f, f'keys {k:03x}', 'p', player_row(e), 'slot', r['slot'], r['skills'], 'ep', r['ep'], 'area', s['area'], 'scene', s['scene'],
                  [o[0] for o in objects(e) if o[0] not in ('hud',)])
    e.set_keys(0)
    if save:
        e.save_file(save)
        print('saved', save)


if __name__ == '__main__':
    main()
