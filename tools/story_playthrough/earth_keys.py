"""Replay the Earth route's key log (work/earth/keys.json, per segment) from the chapter start state on the real game
(headless, muted); check every segment starts in its saved state's place (area, player, logic counter) and write the
whole log as one list (work/earth/keys_all.json) for tools/segments/segment.py record.

usage: python3 tools/story_playthrough/earth_keys.py"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from earth_route import SEGMENTS  # noqa: E402
from route import start_emu, status  # noqa: E402

OUT = ROOT / 'work' / 'earth'


def main():
    keys = json.loads((OUT / 'keys.json').read_text())
    e = start_emu(str(OUT / 'chapter_start.ss'))
    allk = []
    for name, _ in SEGMENTS:
        ref = start_emu(str(OUT / f'seg_{name}.ss'))
        a, b = status(e), status(ref)
        ok = a == b
        print(f'{name:14s} start {"ok" if ok else "MISMATCH"} {a}' + ('' if ok else f' vs saved {b}'))
        if not ok:
            break
        for k in keys[name]:
            e.set_keys(k)
            e.run(1)
        allk += keys[name]
    print('end', status(e), len(allk), 'frames')
    (OUT / 'keys_all.json').write_text(json.dumps(allk))


if __name__ == '__main__':
    main()
