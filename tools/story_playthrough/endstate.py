"""Replay the recorded playthrough's keys (data/story_playthrough/opening.json) and save the emulator state after its
last frame (or frame N), for exploring the route onward.

usage: python3 tools/story_playthrough/endstate.py OUT.ss [--frame N]"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from route import start_emu, intro_keys, status  # noqa: E402


def main():
    out = sys.argv[1]
    doc = json.loads((ROOT / 'data' / 'story_playthrough' / 'opening.json').read_text())
    rows = doc['frames']
    n = int(sys.argv[sys.argv.index('--frame') + 1]) + 1 if '--frame' in sys.argv else len(rows)
    e = start_emu('reset')
    for f in range(doc['skip']):
        e.set_keys(intro_keys(f))
        e.run(1)
    for k in range(n):
        e.set_keys(rows[k]['keys'])
        e.run(1)
    e.set_keys(0)
    e.save_file(out)
    print('saved', out, 'after frame', n - 1, status(e))


if __name__ == '__main__':
    main()
